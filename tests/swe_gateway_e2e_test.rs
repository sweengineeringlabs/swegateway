//! End-to-end SAF surface API verification tests.
//!
//! Validates that all builder functions, trait re-exports, prelude imports,
//! error handling, and health checks work through the public API.

use swe_gateway::prelude::*;
use swe_gateway::saf;

// ── Test 1: All builder functions return usable instances ─────────────────

#[tokio::test]
async fn test_memory_database_builder_returns_functional_gateway() {
    let db = saf::memory_database();
    let health = DatabaseInbound::health_check(&db).await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
}

#[tokio::test]
async fn test_memory_database_with_tables_builder_returns_functional_gateway() {
    let db = saf::memory_database_with_tables(vec!["users", "orders"]);
    // Insert into a predefined table.
    let mut record = saf::database::Record::new();
    record.insert("id".to_string(), serde_json::json!("t-1"));
    let result = DatabaseOutbound::insert(&db, "users", record).await.unwrap();
    assert_eq!(result.rows_affected, 1);
}

#[tokio::test]
async fn test_local_file_gateway_builder_returns_functional_gateway() {
    let tmp = tempfile::tempdir().unwrap();
    let gw = saf::local_file_gateway(tmp.path());
    let health = FileInbound::health_check(&gw).await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
}

#[tokio::test]
async fn test_rest_client_builder_returns_constructable_gateway() {
    let config = saf::http_config_with_base_url("http://localhost:9999");
    let client = saf::rest_client(config);
    // We can't make real HTTP calls, but we can verify construction and health.
    let health = HttpInbound::health_check(&client).await.unwrap();
    assert_eq!(
        health.status, HealthStatus::Healthy,
        "rest client health check must report healthy (it checks internal state, not connectivity)"
    );
}

#[tokio::test]
async fn test_rest_client_with_base_url_builder_returns_constructable_gateway() {
    let _client = saf::rest_client_with_base_url("http://example.com");
    // Construction success is the verification.
}

#[tokio::test]
async fn test_console_notifier_builder_returns_functional_gateway() {
    let notifier = saf::silent_notifier();
    let health = NotificationInbound::health_check(&notifier).await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
}

#[tokio::test]
async fn test_mock_payment_gateway_builder_returns_functional_gateway() {
    let payments = saf::mock_payment_gateway();
    let health = PaymentInbound::health_check(&payments).await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
}

#[tokio::test]
async fn test_rate_limiter_builder_returns_functional_middleware() {
    let limiter = saf::rate_limiter(10, 5.0);
    let result = limiter.try_acquire();
    assert!(result.is_ok(), "rate limiter must allow first request");
}

#[tokio::test]
async fn test_rate_limiter_builder_step_by_step() {
    let limiter = saf::rate_limiter_builder()
        .capacity(3)
        .refill_rate(1.0)
        .build();
    // Exhaust capacity.
    for _ in 0..3 {
        limiter.try_acquire().unwrap();
    }
    let err = limiter.try_acquire().unwrap_err();
    assert!(
        matches!(err, GatewayError::RateLimitExceeded(_)),
        "must reject when bucket is empty"
    );
}

// ── Test 2: All trait methods accessible through SAF re-exports ──────────

#[tokio::test]
async fn test_database_trait_methods_accessible_via_saf() {
    let db = saf::memory_database();

    // Outbound: insert, update, delete, batch_insert, update_where, delete_where
    let mut rec = saf::database::Record::new();
    rec.insert("id".to_string(), serde_json::json!("r1"));
    rec.insert("val".to_string(), serde_json::json!(1));
    DatabaseOutbound::insert(&db, "t", rec).await.unwrap();

    let mut rec2 = saf::database::Record::new();
    rec2.insert("val".to_string(), serde_json::json!(2));
    DatabaseOutbound::update(&db, "t", "r1", rec2).await.unwrap();

    // Inbound: query, get_by_id, exists, count, health_check
    let found = DatabaseInbound::get_by_id(&db, "t", "r1").await.unwrap();
    assert!(found.is_some(), "get_by_id must find the updated record");

    let exists = DatabaseInbound::exists(&db, "t", "r1").await.unwrap();
    assert!(exists, "exists must return true for inserted record");

    let count = DatabaseInbound::count(&db, "t", saf::database::QueryParams::new())
        .await
        .unwrap();
    assert!(count >= 1, "count must be at least 1");

    DatabaseOutbound::delete(&db, "t", "r1").await.unwrap();
    let gone = DatabaseInbound::exists(&db, "t", "r1").await.unwrap();
    assert!(!gone, "record must be gone after delete");
}

#[tokio::test]
async fn test_file_trait_methods_accessible_via_saf() {
    let tmp = tempfile::tempdir().unwrap();
    let gw = saf::local_file_gateway(tmp.path());

    // write, read, exists, metadata, list, delete
    FileOutbound::write(
        &gw,
        "a.txt",
        b"aaa".to_vec(),
        saf::file::UploadOptions::overwrite(),
    )
    .await
    .unwrap();

    let exists = FileInbound::exists(&gw, "a.txt").await.unwrap();
    assert!(exists);

    let list_result = FileInbound::list(&gw, saf::file::ListOptions::default())
        .await
        .unwrap();
    assert!(
        list_result.files.iter().any(|f| f.path.contains("a.txt")),
        "list must include the written file"
    );

    FileOutbound::delete(&gw, "a.txt").await.unwrap();
    let gone = FileInbound::exists(&gw, "a.txt").await.unwrap();
    assert!(!gone, "file must be gone after delete");
}

#[tokio::test]
async fn test_notification_trait_methods_accessible_via_saf() {
    let notifier = saf::silent_notifier();

    // send
    let notif = saf::notification::Notification::console("test body");
    let receipt = NotificationOutbound::send(&notifier, notif).await.unwrap();
    assert_eq!(receipt.status, saf::notification::NotificationStatus::Delivered);

    // send_batch
    let batch = vec![
        saf::notification::Notification::console("msg1"),
        saf::notification::Notification::console("msg2"),
    ];
    let receipts = NotificationOutbound::send_batch(&notifier, batch).await.unwrap();
    assert_eq!(receipts.len(), 2, "batch send must return a receipt per notification");

    // get_status (using the id from the first send)
    let status = NotificationInbound::get_status(&notifier, &receipt.notification_id)
        .await
        .unwrap();
    assert_eq!(
        status.notification_id, receipt.notification_id,
        "status lookup must return the correct notification"
    );
}

#[tokio::test]
async fn test_payment_trait_methods_accessible_via_saf() {
    let payments = saf::mock_payment_gateway();

    // create_payment
    let payment = saf::payment::Payment::new(saf::payment::Money::usd(1000))
        .with_customer("cust-1")
        .with_description("Test payment")
        .with_payment_method("card");
    let result = PaymentOutbound::create_payment(&payments, payment).await.unwrap();
    assert!(!result.payment_id.is_empty(), "payment must have an id");

    // get_payment
    let fetched = PaymentInbound::get_payment(&payments, &result.payment_id)
        .await
        .unwrap();
    assert_eq!(fetched.payment_id, result.payment_id);
}

// ── Test 3: Prelude import gives access to all needed types ──────────────

#[test]
fn test_prelude_provides_core_types() {
    // This test verifies at compile time that the prelude exports the expected types.
    // If any of these types were missing from the prelude, the test would fail to compile.

    let _health = HealthCheck::healthy();
    let _status: HealthStatus = HealthStatus::Healthy;
    let _err = GatewayError::internal("test");
    let _code = GatewayErrorCode::NotFound;
    let _page = Pagination::first(10);
    let _resp: PaginatedResponse<i32> = PaginatedResponse::new(vec![1], 1, 0, 10);

    // Middleware types.
    fn _assert_middleware_types_exist() {
        fn _req(_: &dyn RequestMiddleware) {}
        fn _resp(_: &dyn ResponseMiddleware) {}
    }

    // Provider traits (compile-time check).
    fn _assert_provider_traits<T: StatelessProvider>(_: &T) {}
    fn _assert_stateful_traits<T: StatefulProvider>(_: &T) {}

    // CachedService and ConfiguredCache.
    let _cache: CachedService<String> = CachedService::new();
    let _ccache: ConfiguredCache<String, String> = ConfiguredCache::new();
}

#[test]
fn test_prelude_provides_domain_modules() {
    // Database domain types.
    let _qp = saf::database::QueryParams::new();
    let _wr = saf::database::WriteResult::new(0);
    let _dc = saf::database::DatabaseConfig::memory();

    // File domain types.
    let _fi = saf::file::FileInfo::new("test", 0);
    let _lo = saf::file::ListOptions::default();
    let _uo = saf::file::UploadOptions::overwrite();
    let _fc = saf::file::FileStorageConfig::local("/tmp");

    // Notification domain types.
    let _n = saf::notification::Notification::console("test");
    let _nr = saf::notification::NotificationReceipt::success("n-1");

    // Payment domain types.
    let _pc = saf::payment::PaymentConfig::default();
}

// ── Test 4: GatewayError conversion and classification works end-to-end ──

#[test]
fn test_gateway_error_code_roundtrip_for_all_variants() {
    let cases: Vec<(GatewayError, GatewayErrorCode)> = vec![
        (GatewayError::internal("x"), GatewayErrorCode::Internal),
        (GatewayError::not_found("x"), GatewayErrorCode::NotFound),
        (GatewayError::invalid_input("x"), GatewayErrorCode::InvalidInput),
        (GatewayError::already_exists("x"), GatewayErrorCode::AlreadyExists),
        (GatewayError::permission_denied("x"), GatewayErrorCode::PermissionDenied),
        (GatewayError::timeout("x"), GatewayErrorCode::Timeout),
        (GatewayError::unavailable("x"), GatewayErrorCode::Unavailable),
        (GatewayError::configuration("x"), GatewayErrorCode::Configuration),
    ];

    for (err, expected_code) in cases {
        assert_eq!(
            err.code(),
            expected_code,
            "GatewayError::{:?} must map to {:?}",
            err,
            expected_code,
        );
    }
}

#[test]
fn test_gateway_error_retryable_classification() {
    // Retryable errors.
    assert!(GatewayError::ConnectionFailed("x".into()).is_retryable());
    assert!(GatewayError::RateLimitExceeded("x".into()).is_retryable());
    assert!(GatewayError::Timeout("x".into()).is_retryable());
    assert!(GatewayError::Unavailable("x".into()).is_retryable());

    // Non-retryable errors.
    assert!(!GatewayError::NotFound("x".into()).is_retryable());
    assert!(!GatewayError::ValidationError("x".into()).is_retryable());
    assert!(!GatewayError::AuthenticationFailed("x".into()).is_retryable());
    assert!(!GatewayError::PermissionDenied("x".into()).is_retryable());
    assert!(!GatewayError::AlreadyExists("x".into()).is_retryable());
    assert!(!GatewayError::Configuration("x".into()).is_retryable());
}

#[test]
fn test_gateway_error_with_details_preserves_variant_and_appends() {
    let err = GatewayError::not_found("user")
        .with_details("id=abc-123");
    let msg = err.to_string();

    assert!(msg.contains("user"), "original message must be preserved");
    assert!(msg.contains("[id=abc-123]"), "details must be appended in brackets");
    assert_eq!(
        err.code(),
        GatewayErrorCode::NotFound,
        "error code must survive with_details"
    );
}

#[test]
fn test_gateway_error_display_is_actionable() {
    let err = GatewayError::configuration("missing api_key for Stripe");
    let display = format!("{err}");
    assert!(
        display.contains("missing api_key"),
        "error display must include actionable detail: got '{display}'"
    );
}

#[test]
fn test_result_gateway_ext_maps_std_error() {
    let io_err: Result<(), std::io::Error> = Err(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "access denied",
    ));
    let gw_result = io_err.gateway_err("reading config file");
    let err = gw_result.unwrap_err();
    assert_eq!(err.code(), GatewayErrorCode::Internal);
    let msg = err.to_string();
    assert!(
        msg.contains("reading config file"),
        "context must appear in error message"
    );
    assert!(
        msg.contains("access denied"),
        "original error must appear in details"
    );
}

// ── Test 5: HealthCheck across all gateway types ─────────────────────────

#[tokio::test]
async fn test_health_check_database_gateway() {
    let db = saf::memory_database();
    let health = DatabaseInbound::health_check(&db).await.unwrap();
    assert_eq!(
        health.status,
        HealthStatus::Healthy,
        "memory database must be healthy"
    );
}

#[tokio::test]
async fn test_health_check_file_gateway() {
    let tmp = tempfile::tempdir().unwrap();
    let gw = saf::local_file_gateway(tmp.path());
    let health = FileInbound::health_check(&gw).await.unwrap();
    assert_eq!(
        health.status,
        HealthStatus::Healthy,
        "local file gateway with valid path must be healthy"
    );
}

#[tokio::test]
async fn test_health_check_http_gateway() {
    let client = saf::rest_client_with_base_url("http://localhost:1");
    let health = HttpInbound::health_check(&client).await.unwrap();
    // RestClient health check validates internal state, not connectivity.
    assert_eq!(health.status, HealthStatus::Healthy);
}

#[tokio::test]
async fn test_health_check_notification_gateway() {
    let notifier = saf::silent_notifier();
    let health = NotificationInbound::health_check(&notifier).await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
}

#[tokio::test]
async fn test_health_check_payment_gateway() {
    let payments = saf::mock_payment_gateway();
    let health = PaymentInbound::health_check(&payments).await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
}

#[tokio::test]
async fn test_health_check_metadata_and_fields() {
    let check = HealthCheck::healthy_with_latency(5)
        .with_metadata("version", serde_json::json!("1.0.0"));

    assert_eq!(check.status, HealthStatus::Healthy);
    assert_eq!(check.latency_ms, Some(5));
    assert_eq!(
        check.metadata.get("version"),
        Some(&serde_json::json!("1.0.0"))
    );
    assert!(check.message.is_none());

    let degraded = HealthCheck::degraded("high latency");
    assert_eq!(degraded.status, HealthStatus::Degraded);
    assert_eq!(degraded.message.as_deref(), Some("high latency"));

    let unhealthy = HealthCheck::unhealthy("connection refused");
    assert_eq!(unhealthy.status, HealthStatus::Unhealthy);
    assert_eq!(unhealthy.message.as_deref(), Some("connection refused"));
}

// ── Test: MockFailureMode with payment gateway ───────────────────────────

#[tokio::test]
async fn test_mock_payment_gateway_with_failure_mode_rejects_payments() {
    let payments = saf::mock_payment_gateway_with_failure(
        MockFailureMode::FailAllPayments("all payments disabled".into()),
    );

    let payment = saf::payment::Payment::new(saf::payment::Money::usd(500))
        .with_customer("cust-1");

    let result = PaymentOutbound::create_payment(&payments, payment).await.unwrap();
    assert_eq!(
        result.status,
        saf::payment::PaymentStatus::Failed,
        "FailAllPayments mode must mark every payment as Failed"
    );
    assert!(
        result.error_message.is_some(),
        "failed payment must include an error message"
    );
    assert!(
        result.error_message.unwrap().contains("all payments disabled"),
        "error message must reflect the configured failure reason"
    );
}
