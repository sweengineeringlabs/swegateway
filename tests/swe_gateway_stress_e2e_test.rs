//! Concurrent access stress tests for swe-gateway.
//!
//! Exercises thread-safety of MemoryDatabase (RwLock), LocalFileGateway,
//! RateLimiter (parking_lot::Mutex), and Pipeline under heavy contention.

use std::sync::Arc;

use swe_gateway::saf::{self, DatabaseInbound, DatabaseOutbound, FileInbound, FileOutbound};
use swe_gateway::saf::database::{QueryParams, Record};
use swe_gateway::saf::file::UploadOptions;
use swe_gateway::saf::{
    ClosureRouter, GatewayError, Pipeline, RateLimiter, Router,
};

use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a Record with an "id" and a "value" field.
fn make_record(id: &str, value: &str) -> Record {
    let mut r = serde_json::Map::new();
    r.insert("id".to_string(), serde_json::json!(id));
    r.insert("value".to_string(), serde_json::json!(value));
    r
}

/// Build a Record with "id", "category", and "score" fields.
fn make_scored_record(id: &str, category: &str, score: i64) -> Record {
    let mut r = serde_json::Map::new();
    r.insert("id".to_string(), serde_json::json!(id));
    r.insert("category".to_string(), serde_json::json!(category));
    r.insert("score".to_string(), serde_json::json!(score));
    r
}

// ===========================================================================
// 1. 100 concurrent inserts — all succeed, count is 100
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_inserts_all_succeed_and_count_is_100() {
    let db = Arc::new(saf::memory_database());
    let mut handles = Vec::with_capacity(100);

    for i in 0..100 {
        let db = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            let record = make_record(&format!("rec-{i}"), &format!("val-{i}"));
            db.insert("items", record).await
        }));
    }

    for h in handles {
        let result = h.await.expect("task should not panic");
        assert!(result.is_ok(), "insert should succeed: {:?}", result.err());
    }

    let count = db.count("items", QueryParams::new()).await.unwrap();
    assert_eq!(count, 100, "table should contain exactly 100 records");
}

// ===========================================================================
// 2. Concurrent read + write — readers see consistent state
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_read_write_consistent_state() {
    let db = Arc::new(saf::memory_database());

    // Seed 50 records.
    for i in 0..50 {
        db.insert("data", make_record(&format!("seed-{i}"), "initial"))
            .await
            .unwrap();
    }

    let mut handles = Vec::new();

    // 50 writers inserting new records.
    for i in 50..100 {
        let db = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            db.insert("data", make_record(&format!("seed-{i}"), "new"))
                .await
                .unwrap();
        }));
    }

    // 50 readers querying all records. Each must see a valid snapshot.
    for _ in 0..50 {
        let db = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            let records = db.query("data", QueryParams::new()).await.unwrap();
            // Every returned record must have both "id" and "value" fields —
            // proving no partial/torn records.
            for rec in &records {
                assert!(
                    rec.get("id").is_some(),
                    "record must have 'id' field (no partial record)"
                );
                assert!(
                    rec.get("value").is_some(),
                    "record must have 'value' field (no partial record)"
                );
            }
            // Count must be between seeded (50) and final (100).
            assert!(
                records.len() >= 50 && records.len() <= 100,
                "reader should see between 50 and 100 records, got {}",
                records.len()
            );
        }));
    }

    for h in handles {
        h.await.expect("no task should panic");
    }
}

// ===========================================================================
// 3. Concurrent updates to the same record — last write wins, no panic
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_updates_same_record_no_panic() {
    let db = Arc::new(saf::memory_database());
    db.insert("items", make_record("contested", "original"))
        .await
        .unwrap();

    let mut handles = Vec::new();
    for i in 0..50 {
        let db = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            let mut updates = serde_json::Map::new();
            updates.insert("value".to_string(), serde_json::json!(format!("v{i}")));
            db.update("items", "contested", updates).await
        }));
    }

    for h in handles {
        let result = h.await.expect("task should not panic");
        assert!(result.is_ok(), "update should succeed: {:?}", result.err());
    }

    // The record must exist and hold one of the written values.
    let rec = db
        .get_by_id("items", "contested")
        .await
        .unwrap()
        .expect("record must still exist after concurrent updates");
    let val = rec.get("value").unwrap().as_str().unwrap();
    assert!(
        val.starts_with('v'),
        "value should be one of the written values, got: {val}"
    );
}

// ===========================================================================
// 4. Concurrent delete + read — no panic, read returns None or valid record
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_delete_and_read_no_panic() {
    let db = Arc::new(saf::memory_database());
    db.insert("items", make_record("ephemeral", "exists"))
        .await
        .unwrap();

    let mut handles = Vec::new();

    // One deleter.
    {
        let db = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            db.delete("items", "ephemeral").await.unwrap();
        }));
    }

    // Many readers racing against the delete.
    for _ in 0..50 {
        let db = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            let result = db.get_by_id("items", "ephemeral").await;
            // Must not panic. Result is Ok(Some(..)) or Ok(None).
            assert!(result.is_ok(), "get_by_id must not error: {:?}", result.err());
            if let Some(rec) = result.unwrap() {
                assert_eq!(
                    rec.get("id").unwrap().as_str().unwrap(),
                    "ephemeral",
                    "if returned, record should be the correct one"
                );
            }
        }));
    }

    for h in handles {
        h.await.expect("no task should panic");
    }
}

// ===========================================================================
// 5. Concurrent batch_insert from multiple tasks
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_batch_inserts() {
    let db = Arc::new(saf::memory_database());
    let mut handles = Vec::new();

    // 10 tasks each batch-inserting 10 unique records.
    for batch in 0..10 {
        let db = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            let records: Vec<Record> = (0..10)
                .map(|i| make_record(&format!("b{batch}-r{i}"), &format!("batch-{batch}")))
                .collect();
            db.batch_insert("bulk", records).await
        }));
    }

    for h in handles {
        let result = h.await.expect("task should not panic");
        assert!(result.is_ok(), "batch_insert should succeed: {:?}", result.err());
    }

    let count = db.count("bulk", QueryParams::new()).await.unwrap();
    assert_eq!(count, 100, "10 batches x 10 records = 100 total");
}

// ===========================================================================
// 6. 50 concurrent queries with different filters — all return correct results
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_filtered_queries_return_correct_results() {
    let db = Arc::new(saf::memory_database());
    let categories = ["alpha", "beta", "gamma", "delta", "epsilon"];

    // Insert 100 records spread across 5 categories with scores.
    for i in 0..100 {
        let cat = categories[i % categories.len()];
        let record = make_scored_record(&format!("q-{i}"), cat, i as i64);
        db.insert("scored", record).await.unwrap();
    }

    let mut handles = Vec::new();

    for task_id in 0..50 {
        let db = Arc::clone(&db);
        let cat = categories[task_id % categories.len()];
        let cat_owned = cat.to_string();
        handles.push(tokio::spawn(async move {
            let params = QueryParams::new().filter("category", cat_owned.as_str());
            let results = db.query("scored", params).await.unwrap();
            // Each category has exactly 20 records (100 / 5).
            assert_eq!(
                results.len(),
                20,
                "category '{cat_owned}' should have 20 records, got {}",
                results.len()
            );
            // Every returned record must belong to the requested category.
            for rec in &results {
                let actual_cat = rec.get("category").unwrap().as_str().unwrap();
                assert_eq!(
                    actual_cat, cat_owned,
                    "record category mismatch: expected '{cat_owned}', got '{actual_cat}'"
                );
            }
        }));
    }

    for h in handles {
        h.await.expect("no task should panic");
    }
}

// ===========================================================================
// 7. File gateway: concurrent writes to different files — all succeed
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_file_gateway_concurrent_writes_different_files() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path().to_path_buf();
    let gw = Arc::new(saf::local_file_gateway(base_path));
    let mut handles = Vec::new();

    for i in 0..50 {
        let gw = Arc::clone(&gw);
        handles.push(tokio::spawn(async move {
            let path = format!("file-{i}.txt");
            let content = format!("content-{i}").into_bytes();
            gw.write(&path, content, UploadOptions::overwrite()).await
        }));
    }

    for h in handles {
        let result = h.await.expect("task should not panic");
        assert!(result.is_ok(), "file write should succeed: {:?}", result.err());
    }

    // Verify every file is readable with correct content.
    for i in 0..50 {
        let path = format!("file-{i}.txt");
        let data = gw.read(&path).await.unwrap();
        let text = String::from_utf8(data).unwrap();
        assert_eq!(
            text,
            format!("content-{i}"),
            "file '{path}' should contain its expected content"
        );
    }
}

// ===========================================================================
// 8. File gateway: concurrent read + write to same file — no crash
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_file_gateway_concurrent_read_write_same_file_no_crash() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path().to_path_buf();
    let gw = Arc::new(saf::local_file_gateway(base_path));

    // Seed the file.
    gw.write(
        "shared.txt",
        b"initial".to_vec(),
        UploadOptions::overwrite(),
    )
    .await
    .unwrap();

    let mut handles = Vec::new();

    // 25 writers overwriting the same file.
    for i in 0..25 {
        let gw = Arc::clone(&gw);
        handles.push(tokio::spawn(async move {
            let content = format!("writer-{i}").into_bytes();
            let _ = gw
                .write("shared.txt", content, UploadOptions::overwrite())
                .await;
        }));
    }

    // 25 readers racing against writers.
    for _ in 0..25 {
        let gw = Arc::clone(&gw);
        handles.push(tokio::spawn(async move {
            let result = gw.read("shared.txt").await;
            // Must not panic. May succeed or transiently fail; either is fine.
            // If it succeeds, the content must be valid UTF-8 starting with
            // either "initial" or "writer-".
            if let Ok(data) = result {
                let text = String::from_utf8(data);
                assert!(
                    text.is_ok(),
                    "file contents should be valid UTF-8"
                );
                let text = text.unwrap();
                // During a race, a reader may see:
                // - the original "initial" content
                // - one of the "writer-N" values
                // - an empty string (file truncated by a concurrent writer)
                // - a partial write (prefix of a valid value)
                // All are acceptable; the key invariant is: no crash.
                assert!(
                    text.is_empty()
                        || text.starts_with("initial")
                        || text.starts_with("writer-"),
                    "unexpected file content: '{text}'"
                );
            }
        }));
    }

    for h in handles {
        h.await.expect("no task should panic");
    }
}

// ===========================================================================
// 9. Rate limiter: 100 concurrent requests with capacity 50 — ~50 allowed
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_rate_limiter_concurrent_requests_respects_capacity() {
    // Capacity 50, near-zero refill so no tokens regenerate during the test.
    let limiter = Arc::new(RateLimiter::new(50, 0.001));
    let allowed = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let rejected = Arc::new(std::sync::atomic::AtomicU64::new(0));

    let mut handles = Vec::new();

    for _ in 0..100 {
        let limiter = Arc::clone(&limiter);
        let allowed = Arc::clone(&allowed);
        let rejected = Arc::clone(&rejected);
        handles.push(tokio::spawn(async move {
            match limiter.try_acquire() {
                Ok(()) => {
                    allowed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                Err(GatewayError::RateLimitExceeded(_)) => {
                    rejected.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                Err(e) => panic!("unexpected error variant: {e:?}"),
            }
        }));
    }

    for h in handles {
        h.await.expect("task should not panic");
    }

    let total_allowed = allowed.load(std::sync::atomic::Ordering::Relaxed);
    let total_rejected = rejected.load(std::sync::atomic::Ordering::Relaxed);

    assert_eq!(
        total_allowed + total_rejected,
        100,
        "all 100 requests must be accounted for"
    );
    assert_eq!(
        total_allowed, 50,
        "exactly 50 requests should be allowed (capacity=50, refill~0)"
    );
    assert_eq!(
        total_rejected, 50,
        "exactly 50 requests should be rejected"
    );
}

// ===========================================================================
// 10. Pipeline: concurrent executions — no state leakage
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_pipeline_concurrent_executions_no_state_leakage() {
    // Router that echoes the request with an added "routed: true" field.
    let router: Arc<dyn Router> = Arc::new(ClosureRouter::new(
        |req: &serde_json::Value| -> Result<serde_json::Value, GatewayError> {
            let mut out = req.clone();
            out.as_object_mut()
                .unwrap()
                .insert("routed".to_string(), serde_json::json!(true));
            Ok(out)
        },
    ));

    let pipeline = Arc::new(Pipeline::new(vec![], router, vec![]));
    let mut handles = Vec::new();

    for i in 0..100 {
        let pipeline = Arc::clone(&pipeline);
        handles.push(tokio::spawn(async move {
            let input = serde_json::json!({"task_id": i});
            let output = pipeline.execute(input).await.unwrap();

            // The output must contain exactly the task_id we sent — no leakage
            // from another concurrent execution.
            let returned_id = output
                .get("task_id")
                .expect("output must have 'task_id'")
                .as_i64()
                .expect("task_id must be a number");
            assert_eq!(
                returned_id, i,
                "pipeline must not leak state: expected task_id={i}, got {returned_id}"
            );
            assert_eq!(
                output.get("routed").unwrap(),
                &serde_json::json!(true),
                "router must have processed the request"
            );
        }));
    }

    for h in handles {
        h.await.expect("no task should panic");
    }
}
