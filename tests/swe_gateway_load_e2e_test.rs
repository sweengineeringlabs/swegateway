//! Load tests for swe-gateway.
//!
//! These tests exercise the gateway under high-volume workloads to verify
//! correctness at scale (not just speed — that belongs in the perf tests).

use std::sync::Arc;

use swe_gateway::prelude::*;

// ── Helpers ────────────────────────────────────────────────────────────────

/// Build a record with an explicit id and a payload field.
fn make_record(id: usize, payload: &str) -> database::Record {
    let mut r = serde_json::Map::new();
    r.insert("id".to_string(), serde_json::json!(id.to_string()));
    r.insert("payload".to_string(), serde_json::json!(payload));
    r.insert("index".to_string(), serde_json::json!(id));
    r
}

// ── 1. Insert 10,000 records and verify count ──────────────────────────────

#[tokio::test]
async fn test_load_insert_10k_records_and_verify_count() {
    let db = swe_gateway::saf::memory_database();
    let total = 10_000usize;

    for i in 0..total {
        db.insert("items", make_record(i, "load-test"))
            .await
            .expect("insert should succeed");
    }

    let count = db
        .count("items", database::QueryParams::new())
        .await
        .expect("count should succeed");

    assert_eq!(
        count, total as u64,
        "database should contain exactly {total} records after inserting {total}"
    );

    // Spot-check a few records to confirm data integrity.
    for probe in [0, 4999, 9999] {
        let rec = db
            .get_by_id("items", &probe.to_string())
            .await
            .expect("get_by_id should succeed")
            .unwrap_or_else(|| panic!("record {probe} must exist"));
        assert_eq!(
            rec.get("payload").and_then(|v| v.as_str()),
            Some("load-test"),
            "record {probe} payload mismatch"
        );
    }
}

// ── 2. Query 10,000 records with pagination (100 per page) ────────────────

#[tokio::test]
async fn test_load_query_10k_records_paginated_100_per_page() {
    let db = swe_gateway::saf::memory_database();
    let total = 10_000usize;
    let page_size = 100usize;

    // Seed data.
    let records: Vec<database::Record> = (0..total).map(|i| make_record(i, "paginated")).collect();
    db.batch_insert("items", records)
        .await
        .expect("batch_insert should succeed");

    // Walk all pages, collecting unique IDs.
    let mut all_ids = std::collections::HashSet::new();
    let expected_pages = total / page_size;

    for page in 0..expected_pages {
        let offset = page * page_size;
        let params = database::QueryParams::new().paginate(offset, page_size);
        let page_results = db
            .query("items", params)
            .await
            .expect("query should succeed");

        assert_eq!(
            page_results.len(),
            page_size,
            "page {page} should contain exactly {page_size} records, got {}",
            page_results.len()
        );

        for rec in &page_results {
            let id = rec
                .get("id")
                .and_then(|v| v.as_str())
                .expect("record must have string id");
            all_ids.insert(id.to_string());
        }
    }

    assert_eq!(
        all_ids.len(),
        total,
        "paginating through all pages should yield exactly {total} unique IDs"
    );
}

// ── 3. Batch insert 1,000 records in one call ─────────────────────────────

#[tokio::test]
async fn test_load_batch_insert_1000_records() {
    let db = swe_gateway::saf::memory_database();
    let batch_size = 1_000usize;

    let records: Vec<database::Record> =
        (0..batch_size).map(|i| make_record(i, "batch")).collect();

    let result = db
        .batch_insert("items", records)
        .await
        .expect("batch_insert should succeed");

    assert_eq!(
        result.rows_affected, batch_size as u64,
        "batch_insert should report {batch_size} rows affected"
    );

    let count = db
        .count("items", database::QueryParams::new())
        .await
        .expect("count should succeed");

    assert_eq!(
        count, batch_size as u64,
        "count should match batch size"
    );

    // Verify first and last record exist with correct data.
    let first = db.get_by_id("items", "0").await.unwrap().expect("first record must exist");
    assert_eq!(first.get("payload").and_then(|v| v.as_str()), Some("batch"));

    let last_id = (batch_size - 1).to_string();
    let last = db.get_by_id("items", &last_id).await.unwrap().expect("last record must exist");
    assert_eq!(last.get("payload").and_then(|v| v.as_str()), Some("batch"));
}

// ── 4. Write and read 100 files sequentially ──────────────────────────────

#[tokio::test]
async fn test_load_write_and_read_100_files_sequentially() {
    let temp_dir = tempfile::TempDir::new().expect("failed to create temp dir");
    let gw = swe_gateway::saf::local_file_gateway(temp_dir.path());
    let file_count = 100usize;

    // Write phase.
    for i in 0..file_count {
        let path = format!("file_{i:04}.txt");
        let content = format!("content-for-file-{i}");
        gw.write(&path, content.as_bytes().to_vec(), file::UploadOptions::overwrite())
            .await
            .unwrap_or_else(|e| panic!("write {path} failed: {e}"));
    }

    // Read phase — verify each file's content matches what was written.
    for i in 0..file_count {
        let path = format!("file_{i:04}.txt");
        let expected = format!("content-for-file-{i}");
        let actual = gw
            .read(&path)
            .await
            .unwrap_or_else(|e| panic!("read {path} failed: {e}"));
        assert_eq!(
            String::from_utf8(actual).expect("file should be valid UTF-8"),
            expected,
            "content mismatch for {path}"
        );
    }
}

// ── 5. List directory with 500+ files ─────────────────────────────────────

#[tokio::test]
async fn test_load_list_directory_with_500_plus_files() {
    let temp_dir = tempfile::TempDir::new().expect("failed to create temp dir");
    let gw = swe_gateway::saf::local_file_gateway(temp_dir.path());
    let file_count = 550usize;

    // Create the files.
    for i in 0..file_count {
        let path = format!("entry_{i:04}.dat");
        gw.write(&path, vec![0u8; 16], file::UploadOptions::overwrite())
            .await
            .unwrap_or_else(|e| panic!("write {path} failed: {e}"));
    }

    // List without any max_results cap — should return all files.
    let result = gw
        .list(file::ListOptions::default())
        .await
        .expect("list should succeed");

    assert!(
        result.files.len() >= file_count,
        "expected at least {file_count} files, got {}",
        result.files.len()
    );

    // Verify none of the entries are directories.
    let dirs = result.files.iter().filter(|f| f.is_directory).count();
    assert_eq!(dirs, 0, "all entries should be files, not directories");
}

// ── 6. 1,000 sequential pipeline executions ───────────────────────────────

#[tokio::test]
async fn test_load_1000_sequential_pipeline_executions() {
    // A simple pipeline: passthrough middleware + echo router.
    struct Passthrough;

    #[async_trait]
    impl RequestMiddleware for Passthrough {
        async fn process_request(
            &self,
            request: serde_json::Value,
        ) -> GatewayResult<serde_json::Value> {
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

    let pipeline = Pipeline::new(
        vec![Arc::new(Passthrough) as Arc<dyn RequestMiddleware>],
        Arc::new(EchoRouter) as Arc<dyn Router>,
        vec![],
    );

    let iterations = 1_000usize;

    for i in 0..iterations {
        let input = serde_json::json!({"seq": i});
        let output = pipeline
            .execute(input.clone())
            .await
            .unwrap_or_else(|e| panic!("pipeline execution {i} failed: {e}"));
        assert_eq!(
            input, output,
            "pipeline should echo the request on iteration {i}"
        );
    }
}
