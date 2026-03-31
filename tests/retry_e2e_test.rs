//! End-to-end tests for the retry middleware.
//!
//! These tests exercise `RetryMiddleware` through the public `saf` API surface,
//! verifying retry behavior, backoff timing, and predicate overrides.

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use swe_gateway::saf::{
    async_trait, GatewayError, GatewayResult, RequestMiddleware,
    RetryMiddleware,
};

// ── Test helpers ────────────────────────────────────────────────────────────

/// Middleware that fails a configurable number of times before succeeding.
struct TransientFailure {
    remaining: Mutex<u32>,
    error_factory: Box<dyn Fn() -> GatewayError + Send + Sync>,
    success_value: serde_json::Value,
}

impl TransientFailure {
    fn new(
        failures: u32,
        error_factory: impl Fn() -> GatewayError + Send + Sync + 'static,
        success_value: serde_json::Value,
    ) -> Self {
        Self {
            remaining: Mutex::new(failures),
            error_factory: Box::new(error_factory),
            success_value,
        }
    }
}

#[async_trait]
impl RequestMiddleware for TransientFailure {
    async fn process_request(
        &self,
        _request: serde_json::Value,
    ) -> GatewayResult<serde_json::Value> {
        let mut remaining = self.remaining.lock();
        if *remaining > 0 {
            *remaining -= 1;
            Err((self.error_factory)())
        } else {
            Ok(self.success_value.clone())
        }
    }
}

/// Middleware that always fails with the given error.
struct PermanentFailure {
    error_factory: Box<dyn Fn() -> GatewayError + Send + Sync>,
}

impl PermanentFailure {
    fn new(error_factory: impl Fn() -> GatewayError + Send + Sync + 'static) -> Self {
        Self {
            error_factory: Box::new(error_factory),
        }
    }
}

#[async_trait]
impl RequestMiddleware for PermanentFailure {
    async fn process_request(
        &self,
        _request: serde_json::Value,
    ) -> GatewayResult<serde_json::Value> {
        Err((self.error_factory)())
    }
}

/// Middleware that tracks how many times it was called.
struct CountingMiddleware {
    call_count: Mutex<u32>,
    inner: Arc<dyn RequestMiddleware>,
}

impl CountingMiddleware {
    fn new(inner: Arc<dyn RequestMiddleware>) -> Self {
        Self {
            call_count: Mutex::new(0),
            inner,
        }
    }

    fn call_count(&self) -> u32 {
        *self.call_count.lock()
    }
}

#[async_trait]
impl RequestMiddleware for CountingMiddleware {
    async fn process_request(
        &self,
        request: serde_json::Value,
    ) -> GatewayResult<serde_json::Value> {
        {
            let mut count = self.call_count.lock();
            *count += 1;
        }
        self.inner.process_request(request).await
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

/// Retry on transient errors succeeds after N attempts.
#[tokio::test]
async fn test_retry_on_transient_error_succeeds_after_n_attempts() {
    let inner = Arc::new(TransientFailure::new(
        2,
        || GatewayError::Unavailable("service temporarily down".into()),
        serde_json::json!({"status": "recovered"}),
    ));
    let counter = Arc::new(CountingMiddleware::new(inner));

    let mw = RetryMiddleware::builder()
        .max_attempts(3)
        .fixed_backoff(Duration::from_millis(1)) // minimal delay for test speed
        .build_with(counter.clone() as Arc<dyn RequestMiddleware>);

    let result = mw.process_request(serde_json::json!({})).await;

    assert!(result.is_ok(), "should succeed on 3rd attempt");
    assert_eq!(result.unwrap(), serde_json::json!({"status": "recovered"}));
    assert_eq!(counter.call_count(), 3, "inner should be called exactly 3 times");
}

/// Gives up after max attempts are exhausted.
#[tokio::test]
async fn test_retry_gives_up_after_max_attempts() {
    let inner = Arc::new(PermanentFailure::new(|| {
        GatewayError::Timeout("request timed out".into())
    }));
    let counter = Arc::new(CountingMiddleware::new(inner));

    let mw = RetryMiddleware::builder()
        .max_attempts(4)
        .fixed_backoff(Duration::from_millis(1))
        .build_with(counter.clone() as Arc<dyn RequestMiddleware>);

    let result = mw.process_request(serde_json::json!({})).await;

    assert!(result.is_err(), "should fail after exhausting retries");
    let err = result.unwrap_err();
    assert!(
        matches!(err, GatewayError::Timeout(_)),
        "should return the last error variant, got: {err:?}"
    );
    assert_eq!(
        counter.call_count(),
        4,
        "should have attempted exactly max_attempts times"
    );
}

/// Non-retryable errors are not retried.
#[tokio::test]
async fn test_retry_does_not_retry_non_retryable_errors() {
    let inner = Arc::new(PermanentFailure::new(|| {
        GatewayError::NotFound("resource does not exist".into())
    }));
    let counter = Arc::new(CountingMiddleware::new(inner));

    let mw = RetryMiddleware::builder()
        .max_attempts(5)
        .fixed_backoff(Duration::from_millis(1))
        .build_with(counter.clone() as Arc<dyn RequestMiddleware>);

    let result = mw.process_request(serde_json::json!({})).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), GatewayError::NotFound(_)));
    assert_eq!(
        counter.call_count(),
        1,
        "should NOT retry non-retryable errors — only 1 attempt expected"
    );
}

/// Exponential backoff timing increases between retries.
#[tokio::test]
async fn test_retry_exponential_backoff_timing() {
    let inner = Arc::new(PermanentFailure::new(|| {
        GatewayError::ConnectionFailed("connection refused".into())
    }));

    let mw = RetryMiddleware::builder()
        .max_attempts(4)
        .exponential_backoff(Duration::from_millis(50), false)
        .build_with(inner as Arc<dyn RequestMiddleware>);

    let start = Instant::now();
    let _ = mw.process_request(serde_json::json!({})).await;
    let elapsed = start.elapsed();

    // Expected cumulative sleep: 50 + 100 + 200 = 350ms (3 sleeps for 4 attempts).
    // Allow generous tolerance for CI environments.
    assert!(
        elapsed >= Duration::from_millis(300),
        "elapsed {elapsed:?} should be at least ~350ms from exponential backoff"
    );
    assert!(
        elapsed < Duration::from_millis(2000),
        "elapsed {elapsed:?} should complete within a reasonable bound"
    );
}

/// Custom predicate override allows retrying normally non-retryable errors.
#[tokio::test]
async fn test_retry_custom_predicate_override() {
    // ValidationError is NOT retryable by default.
    let inner = Arc::new(TransientFailure::new(
        2,
        || GatewayError::ValidationError("temporary schema mismatch".into()),
        serde_json::json!({"validated": true}),
    ));
    let counter = Arc::new(CountingMiddleware::new(inner));

    let mw = RetryMiddleware::builder()
        .max_attempts(5)
        .fixed_backoff(Duration::from_millis(1))
        .retry_predicate(|err| matches!(err, GatewayError::ValidationError(_)))
        .build_with(counter.clone() as Arc<dyn RequestMiddleware>);

    let result = mw.process_request(serde_json::json!({})).await;

    assert!(result.is_ok(), "custom predicate should allow retrying ValidationError");
    assert_eq!(result.unwrap(), serde_json::json!({"validated": true}));
    assert_eq!(counter.call_count(), 3, "should have retried twice then succeeded");
}

/// The builder factory function `swe_gateway::saf::retry_middleware()` works.
#[tokio::test]
async fn test_retry_builder_factory_function() {
    let inner = Arc::new(TransientFailure::new(
        1,
        || GatewayError::Unavailable("flicker".into()),
        serde_json::json!({"ok": 1}),
    ));

    // Use the SAF builder function (not RetryMiddleware::builder directly)
    let mw = swe_gateway::saf::retry_middleware()
        .max_attempts(3)
        .fixed_backoff(Duration::from_millis(1))
        .build_with(inner as Arc<dyn RequestMiddleware>);

    let result = mw.process_request(serde_json::json!({})).await;
    assert!(result.is_ok());
}

/// RetryMiddlewareSpec::wrap deferred-build pattern works end-to-end.
#[tokio::test]
async fn test_retry_spec_wrap_deferred_build() {
    let spec = RetryMiddleware::builder()
        .max_attempts(2)
        .fixed_backoff(Duration::from_millis(1))
        .build();

    let inner = Arc::new(TransientFailure::new(
        1,
        || GatewayError::Timeout("slow".into()),
        serde_json::json!({"fast": true}),
    ));

    let mw = spec.wrap(inner as Arc<dyn RequestMiddleware>);
    let result = mw.process_request(serde_json::json!({})).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), serde_json::json!({"fast": true}));
}

/// Verifies that different retryable error variants are all retried by default.
#[tokio::test]
async fn test_retry_all_default_retryable_variants() {
    let retryable_factories: Vec<Box<dyn Fn() -> GatewayError + Send + Sync>> = vec![
        Box::new(|| GatewayError::ConnectionFailed("conn".into())),
        Box::new(|| GatewayError::RateLimitExceeded("rate".into())),
        Box::new(|| GatewayError::Timeout("timeout".into())),
        Box::new(|| GatewayError::Unavailable("unavail".into())),
    ];

    for factory in retryable_factories {
        let err = factory();
        assert!(
            err.is_retryable(),
            "{err:?} should be retryable by default"
        );
    }

    // Also verify non-retryable variants are NOT retried:
    let non_retryable: Vec<GatewayError> = vec![
        GatewayError::NotFound("nf".into()),
        GatewayError::ValidationError("ve".into()),
        GatewayError::AuthenticationFailed("auth".into()),
        GatewayError::PermissionDenied("pd".into()),
        GatewayError::Conflict("c".into()),
        GatewayError::AlreadyExists("ae".into()),
    ];

    for err in &non_retryable {
        assert!(
            !err.is_retryable(),
            "{err:?} should NOT be retryable by default"
        );
    }
}
