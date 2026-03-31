//! Performance benchmark tests for swe-gateway.
//!
//! Each test measures wall-clock time using `std::time::Instant` and asserts
//! that the operation completes within a generous-but-meaningful bound.
//! These are CI-safe sanity checks, not micro-benchmarks — the thresholds
//! are deliberately loose to avoid flakiness on slow CI runners.

use std::sync::Arc;
use std::time::{Duration, Instant};

use swe_gateway::prelude::*;

// ── Helpers ────────────────────────────────────────────────────────────────

/// Build a record with an explicit id and indexed fields for filtering.
fn make_record(id: usize, status: &str) -> database::Record {
    let mut r = serde_json::Map::new();
    r.insert("id".to_string(), serde_json::json!(id.to_string()));
    r.insert("status".to_string(), serde_json::json!(status));
    r.insert("value".to_string(), serde_json::json!(id));
    r
}

/// Extremely low refill rate so token buckets don't silently refill during tests.
const NEAR_ZERO_REFILL: f64 = 0.001;

// ── 1. Insert 1,000 records completes in < 1 second ───────────────────────

#[tokio::test]
async fn test_perf_insert_1000_records_under_1s() {
    let db = swe_gateway::saf::memory_database();
    let total = 1_000usize;
    let max_duration = Duration::from_secs(1);

    let start = Instant::now();
    for i in 0..total {
        db.insert("items", make_record(i, "active"))
            .await
            .expect("insert should succeed");
    }
    let elapsed = start.elapsed();

    // Verify correctness — the timing assertion is meaningless if the data is wrong.
    let count = db
        .count("items", database::QueryParams::new())
        .await
        .expect("count should succeed");
    assert_eq!(count, total as u64, "all records must be present");

    assert!(
        elapsed < max_duration,
        "inserting {total} records took {elapsed:?}, which exceeds the {max_duration:?} budget"
    );
}

// ── 2. Query with filter on 10,000 records completes in < 500ms ──────────

#[tokio::test]
async fn test_perf_query_with_filter_10k_records_under_500ms() {
    let db = swe_gateway::saf::memory_database();
    let total = 10_000usize;
    let max_duration = Duration::from_millis(500);

    // Seed data: alternate between "active" and "inactive".
    let records: Vec<database::Record> = (0..total)
        .map(|i| {
            let status = if i % 2 == 0 { "active" } else { "inactive" };
            make_record(i, status)
        })
        .collect();
    db.batch_insert("items", records)
        .await
        .expect("batch_insert should succeed");

    // Time the filtered query.
    let start = Instant::now();
    let params = database::QueryParams::new().filter("status", "active");
    let results = db
        .query("items", params)
        .await
        .expect("query should succeed");
    let elapsed = start.elapsed();

    // Correctness: half the records are "active".
    assert_eq!(
        results.len(),
        total / 2,
        "expected {} active records, got {}",
        total / 2,
        results.len()
    );

    assert!(
        elapsed < max_duration,
        "filtering {total} records took {elapsed:?}, which exceeds the {max_duration:?} budget"
    );
}

// ── 3. File read/write cycle completes in < 100ms ─────────────────────────

#[tokio::test]
async fn test_perf_file_read_write_cycle_under_100ms() {
    let temp_dir = tempfile::TempDir::new().expect("failed to create temp dir");
    let gw = swe_gateway::saf::local_file_gateway(temp_dir.path());
    let max_duration = Duration::from_millis(100);

    let content = b"performance-test-payload-with-some-body-data".to_vec();
    let path = "perf_test.txt";

    let start = Instant::now();

    gw.write(path, content.clone(), file::UploadOptions::overwrite())
        .await
        .expect("write should succeed");

    let read_back = gw.read(path).await.expect("read should succeed");

    let elapsed = start.elapsed();

    // Correctness: data round-trips.
    assert_eq!(
        read_back, content,
        "read-back content must match what was written"
    );

    assert!(
        elapsed < max_duration,
        "file write+read cycle took {elapsed:?}, which exceeds the {max_duration:?} budget"
    );
}

// ── 4. Rate limiter allows exactly capacity requests (timing test) ────────

#[tokio::test]
async fn test_perf_rate_limiter_allows_exactly_capacity_requests() {
    let capacity = 500u64;
    let limiter = swe_gateway::saf::rate_limiter(capacity, NEAR_ZERO_REFILL);
    let max_duration = Duration::from_millis(500);

    let start = Instant::now();

    let mut success_count = 0u64;
    let mut reject_count = 0u64;

    // Attempt capacity + 100 requests — exactly `capacity` should succeed.
    for _ in 0..(capacity + 100) {
        match limiter.try_acquire() {
            Ok(()) => success_count += 1,
            Err(_) => reject_count += 1,
        }
    }

    let elapsed = start.elapsed();

    assert_eq!(
        success_count, capacity,
        "rate limiter should allow exactly {capacity} requests, but allowed {success_count}"
    );
    assert_eq!(
        reject_count, 100,
        "rate limiter should reject exactly 100 excess requests, but rejected {reject_count}"
    );

    assert!(
        elapsed < max_duration,
        "processing {} acquire calls took {elapsed:?}, which exceeds the {max_duration:?} budget",
        capacity + 100
    );
}

// ── 5. Pipeline with 10 middleware layers processes in < 50ms ─────────────

#[tokio::test]
async fn test_perf_pipeline_10_middleware_layers_under_50ms() {
    /// Middleware that appends a tag to the JSON request.
    struct TagMiddleware {
        tag: String,
    }

    #[async_trait]
    impl RequestMiddleware for TagMiddleware {
        async fn process_request(
            &self,
            mut request: serde_json::Value,
        ) -> GatewayResult<serde_json::Value> {
            if let Some(obj) = request.as_object_mut() {
                let key = format!("mw_{}", self.tag);
                obj.insert(key, serde_json::json!(true));
            }
            Ok(request)
        }
    }

    struct EchoRouter;

    #[async_trait]
    impl Router for EchoRouter {
        async fn dispatch(
            &self,
            request: &serde_json::Value,
        ) -> GatewayResult<serde_json::Value> {
            Ok(request.clone())
        }
    }

    let middleware_count = 10;
    let pre: Vec<Arc<dyn RequestMiddleware>> = (0..middleware_count)
        .map(|i| {
            Arc::new(TagMiddleware {
                tag: i.to_string(),
            }) as Arc<dyn RequestMiddleware>
        })
        .collect();

    let pipeline = Pipeline::new(
        pre,
        Arc::new(EchoRouter) as Arc<dyn Router>,
        vec![],
    );

    let max_duration = Duration::from_millis(50);
    let input = serde_json::json!({"model": "gpt-4"});

    let start = Instant::now();
    let output = pipeline
        .execute(input)
        .await
        .expect("pipeline should succeed");
    let elapsed = start.elapsed();

    // Correctness: all 10 middleware tags should be present.
    let obj = output.as_object().expect("output should be a JSON object");
    for i in 0..middleware_count {
        let key = format!("mw_{i}");
        assert_eq!(
            obj.get(&key),
            Some(&serde_json::json!(true)),
            "middleware tag '{key}' should be present in output"
        );
    }
    // Original field preserved.
    assert_eq!(obj.get("model"), Some(&serde_json::json!("gpt-4")));

    assert!(
        elapsed < max_duration,
        "pipeline with {middleware_count} middleware layers took {elapsed:?}, exceeds {max_duration:?}"
    );
}

// ── 6. Retry middleware with exponential backoff respects timing bounds ────

#[tokio::test]
async fn test_perf_retry_exponential_backoff_respects_timing_bounds() {
    /// A middleware that always fails with a retryable error.
    struct AlwaysFailRetryable;

    #[async_trait]
    impl RequestMiddleware for AlwaysFailRetryable {
        async fn process_request(
            &self,
            _request: serde_json::Value,
        ) -> GatewayResult<serde_json::Value> {
            Err(GatewayError::unavailable("simulated transient failure"))
        }
    }

    // Configure: 3 attempts, fixed backoff at 50ms (no jitter for determinism).
    // Expected: attempt 1 fails -> sleep 50ms -> attempt 2 fails -> sleep 50ms -> attempt 3 fails -> return error.
    // Total sleep = 100ms. With overhead, should be between ~80ms and ~500ms.
    let retry = RetryMiddleware::builder()
        .max_attempts(3)
        .fixed_backoff(Duration::from_millis(50))
        .build()
        .wrap(Arc::new(AlwaysFailRetryable) as Arc<dyn RequestMiddleware>);

    let min_expected = Duration::from_millis(80);
    let max_expected = Duration::from_millis(500);

    let start = Instant::now();
    let result = retry
        .process_request(serde_json::json!({"test": true}))
        .await;
    let elapsed = start.elapsed();

    // Must fail after exhausting retries.
    assert!(
        result.is_err(),
        "should fail after exhausting all retry attempts"
    );
    let err = result.unwrap_err();
    assert!(
        matches!(err, GatewayError::Unavailable(_)),
        "expected Unavailable error, got: {err:?}"
    );

    // Timing: at least 2 sleep intervals of 50ms, but not excessively long.
    assert!(
        elapsed >= min_expected,
        "retry sequence completed in {elapsed:?}, expected at least {min_expected:?} \
         (2 backoff sleeps of 50ms each)"
    );
    assert!(
        elapsed < max_expected,
        "retry sequence took {elapsed:?}, which exceeds the generous {max_expected:?} upper bound"
    );
}
