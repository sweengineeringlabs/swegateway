//! Integration tests for swe-gateway.
//!
//! Cross-gateway workflows that exercise multiple gateway types together,
//! middleware composition, and configuration-driven gateway construction.

use std::sync::Arc;

use swe_gateway::prelude::*;
use swe_gateway::saf;

// ── Test 1: Database insert -> query -> stream -> verify same data ────────

#[tokio::test]
async fn test_database_insert_query_stream_roundtrip() {
    let db = saf::memory_database();

    // Insert a record.
    let mut record = saf::database::Record::new();
    record.insert("id".to_string(), serde_json::json!("user-1"));
    record.insert("name".to_string(), serde_json::json!("Alice"));
    record.insert("age".to_string(), serde_json::json!(30));

    let write_result = DatabaseOutbound::insert(&db, "users", record.clone()).await.unwrap();
    assert_eq!(
        write_result.rows_affected, 1,
        "insert must affect exactly 1 row"
    );

    // Query the record back.
    let params = saf::database::QueryParams::new().filter("id", "user-1");
    let rows = DatabaseInbound::query(&db, "users", params).await.unwrap();
    assert_eq!(rows.len(), 1, "query must return the inserted record");
    assert_eq!(
        rows[0].get("name").and_then(|v| v.as_str()),
        Some("Alice"),
        "queried name must match inserted name"
    );

    // Stream all records from the table.
    let stream_params = saf::database::QueryParams::new();
    let mut stream = DatabaseInbound::query_stream(&db, "users", stream_params)
        .await
        .unwrap();

    let mut streamed_records = Vec::new();
    while let Some(item) = StreamExt::next(&mut stream).await {
        streamed_records.push(item.unwrap());
    }
    assert!(
        !streamed_records.is_empty(),
        "stream must yield at least the inserted record"
    );
    assert_eq!(
        streamed_records[0].get("name").and_then(|v| v.as_str()),
        Some("Alice"),
        "streamed record must match inserted data"
    );
}

// ── Test 2: File write -> read -> metadata -> verify consistency ─────────

#[tokio::test]
async fn test_file_write_read_metadata_consistency() {
    let tmp = tempfile::tempdir().unwrap();
    let file_gw = saf::local_file_gateway(tmp.path());

    let content = b"hello, gateway world!";
    let path = "test-file.txt";

    // Write.
    let write_info = FileOutbound::write(
        &file_gw,
        path,
        content.to_vec(),
        saf::file::UploadOptions::overwrite(),
    )
    .await
    .unwrap();

    assert_eq!(write_info.path, path, "written file path must match");
    assert_eq!(
        write_info.size,
        content.len() as u64,
        "written file size must match content length"
    );

    // Read back.
    let read_bytes = FileInbound::read(&file_gw, path).await.unwrap();
    assert_eq!(
        read_bytes, content,
        "read bytes must exactly match written bytes"
    );

    // Metadata.
    let meta = FileInbound::metadata(&file_gw, path).await.unwrap();
    assert_eq!(meta.path, path, "metadata path must match");
    assert_eq!(
        meta.size,
        content.len() as u64,
        "metadata size must match content length"
    );
    assert!(!meta.is_directory, "regular file must not be flagged as directory");

    // Exists check.
    let exists = FileInbound::exists(&file_gw, path).await.unwrap();
    assert!(exists, "file must exist after write");

    let missing = FileInbound::exists(&file_gw, "nonexistent.txt").await.unwrap();
    assert!(!missing, "nonexistent file must not exist");
}

// ── Test 3: Pipeline with retry middleware wrapping a flaky operation ─────

#[tokio::test]
async fn test_pipeline_retry_middleware_recovers_from_transient_failure() {
    use parking_lot::Mutex;
    use std::time::Duration;

    let call_count = Arc::new(Mutex::new(0u32));

    // Inner middleware that fails twice then succeeds.
    struct FlakyMiddleware {
        call_count: Arc<Mutex<u32>>,
    }

    #[async_trait]
    impl RequestMiddleware for FlakyMiddleware {
        async fn process_request(
            &self,
            request: serde_json::Value,
        ) -> Result<serde_json::Value, GatewayError> {
            let mut count = self.call_count.lock();
            *count += 1;
            if *count <= 2 {
                Err(GatewayError::Unavailable(format!("attempt {}", *count)))
            } else {
                Ok(request)
            }
        }
    }

    let flaky = Arc::new(FlakyMiddleware {
        call_count: call_count.clone(),
    });

    let retry_spec = saf::retry_middleware()
        .max_attempts(5)
        .fixed_backoff(Duration::from_millis(1))
        .build();

    let retry_mw: Arc<dyn RequestMiddleware> = Arc::new(retry_spec.wrap(flaky));

    let router = Arc::new(ClosureRouter::new(|req: &serde_json::Value| {
        Ok(req.clone())
    }));

    let pipeline = Pipeline::new(vec![retry_mw], router as Arc<dyn Router>, vec![]);

    let result = pipeline
        .execute(serde_json::json!({"op": "insert"}))
        .await;

    assert!(result.is_ok(), "pipeline should succeed after retries");
    assert_eq!(
        *call_count.lock(),
        3,
        "inner middleware must be called 3 times (2 failures + 1 success)"
    );
}

// ── Test 4: Pipeline with rate limiter + real gateway operation ───────────

#[tokio::test]
async fn test_pipeline_rate_limiter_allows_within_capacity() {
    // Rate limiter with capacity 5, very low refill to prevent refill during test.
    let limiter: Arc<dyn RequestMiddleware> = Arc::new(saf::rate_limiter(5, 0.001));

    let router = Arc::new(ClosureRouter::new(|req: &serde_json::Value| {
        Ok(req.clone())
    }));

    let pipeline = Pipeline::new(vec![limiter.clone()], router as Arc<dyn Router>, vec![]);

    // First 5 requests should succeed.
    for i in 0..5 {
        let result = pipeline
            .execute(serde_json::json!({"seq": i}))
            .await;
        assert!(
            result.is_ok(),
            "request {i} within capacity must succeed"
        );
    }

    // 6th request should be rate-limited.
    let result = pipeline
        .execute(serde_json::json!({"seq": 5}))
        .await;
    assert!(result.is_err(), "request exceeding capacity must fail");
    let err = result.unwrap_err();
    assert!(
        matches!(err, GatewayError::RateLimitExceeded(_)),
        "error must be RateLimitExceeded, got: {err:?}"
    );
}

// ── Test 5: Configuration load -> build gateways -> perform operations ───

#[tokio::test]
async fn test_config_driven_gateway_construction_and_operation() {
    let toml_str = r#"
[database]
database_type = "memory"

[file]
storage_type = "local"
base_path = "."

[http]
timeout_secs = 10

[notification]
default_channel = "console"

[payment]
provider = "mock"
sandbox = true
"#;

    let config = saf::load_config_from_str(toml_str).unwrap();

    // Build gateways from config.
    let db = config.database_gateway();
    let notifier = config.notification_gateway();

    // Verify database gateway works: insert + query roundtrip.
    let mut record = saf::database::Record::new();
    record.insert("id".to_string(), serde_json::json!("cfg-1"));
    record.insert("status".to_string(), serde_json::json!("active"));

    DatabaseOutbound::insert(&db, "items", record).await.unwrap();

    let params = saf::database::QueryParams::new().filter("id", "cfg-1");
    let results = DatabaseInbound::query(&db, "items", params).await.unwrap();
    assert_eq!(results.len(), 1, "config-driven db must support insert+query");

    // Verify notification gateway works: send a console notification.
    let notif = saf::notification::Notification::console("config test message");
    let receipt = NotificationOutbound::send(&notifier, notif).await.unwrap();
    assert_eq!(
        receipt.status,
        saf::notification::NotificationStatus::Delivered,
        "console notification must report Sent status"
    );
}

// ── Test 6: Multiple gateways composed in a single workflow ──────────────

#[tokio::test]
async fn test_multi_gateway_workflow_db_file_notification() {
    let tmp = tempfile::tempdir().unwrap();

    // Create all gateways.
    let db = saf::memory_database();
    let file_gw = saf::local_file_gateway(tmp.path());
    let notifier = saf::silent_notifier();

    // Step 1: Insert a record into the database.
    let mut record = saf::database::Record::new();
    record.insert("id".to_string(), serde_json::json!("order-42"));
    record.insert("product".to_string(), serde_json::json!("Widget"));
    record.insert("quantity".to_string(), serde_json::json!(10));

    DatabaseOutbound::insert(&db, "orders", record).await.unwrap();

    // Step 2: Query the record back.
    let params = saf::database::QueryParams::new().filter("id", "order-42");
    let orders = DatabaseInbound::query(&db, "orders", params).await.unwrap();
    assert_eq!(orders.len(), 1);

    // Step 3: Write the order as a JSON file.
    let order_json = serde_json::to_vec_pretty(&orders[0]).unwrap();
    FileOutbound::write(
        &file_gw,
        "order-42.json",
        order_json.clone(),
        saf::file::UploadOptions::overwrite(),
    )
    .await
    .unwrap();

    // Step 4: Read the file back and verify consistency.
    let read_back = FileInbound::read(&file_gw, "order-42.json").await.unwrap();
    assert_eq!(
        read_back, order_json,
        "file content must match the serialized order"
    );

    // Step 5: Send a notification about the completed workflow.
    let notif = saf::notification::Notification::console("Order order-42 exported successfully");
    let receipt = NotificationOutbound::send(&notifier, notif).await.unwrap();
    assert_eq!(
        receipt.status,
        saf::notification::NotificationStatus::Delivered,
        "workflow completion notification must succeed"
    );

    // Step 6: Verify the database still has the record (no side effects from file ops).
    let exists = DatabaseInbound::exists(&db, "orders", "order-42").await.unwrap();
    assert!(exists, "database record must persist across file operations");
}
