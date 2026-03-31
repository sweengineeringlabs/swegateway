//! Built-in retry middleware for transient failure recovery.
//!
//! Wraps a request operation with configurable retry logic including
//! backoff strategies and retryable-error predicates.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::api::middleware::RequestMiddleware;
use crate::api::types::GatewayError;

// ── Backoff strategies ──────────────────────────────────────────────────────

/// Strategy for computing the delay between retry attempts.
#[derive(Debug, Clone)]
pub enum BackoffStrategy {
    /// Constant delay between each attempt.
    Fixed {
        /// Delay between retries.
        delay: Duration,
    },
    /// Exponentially increasing delay: `base * 2^attempt`.
    ///
    /// With jitter enabled, a random fraction of the computed delay is added
    /// to spread out concurrent retries (decorrelated jitter).
    Exponential {
        /// Base delay (attempt 0 waits `base`, attempt 1 waits `base * 2`, etc.).
        base: Duration,
        /// If `true`, add uniform random jitter in `[0, computed_delay)`.
        jitter: bool,
    },
}

impl BackoffStrategy {
    /// Compute the sleep duration for the given zero-based `attempt` index.
    fn compute_delay(&self, attempt: u32) -> Duration {
        match self {
            BackoffStrategy::Fixed { delay } => *delay,
            BackoffStrategy::Exponential { base, jitter } => {
                let multiplier = 2u64.saturating_pow(attempt);
                let base_ms = base.as_millis() as u64;
                let delay_ms = base_ms.saturating_mul(multiplier);

                if *jitter {
                    // Simple pseudo-random jitter using current time nanos.
                    // Not cryptographically random — fine for retry jitter.
                    let nanos = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .subsec_nanos() as u64;
                    let jitter_ms = if delay_ms > 0 { nanos % delay_ms } else { 0 };
                    Duration::from_millis(delay_ms.saturating_add(jitter_ms))
                } else {
                    Duration::from_millis(delay_ms)
                }
            }
        }
    }
}

// ── Retryable predicate ─────────────────────────────────────────────────────

/// Predicate that decides whether a `GatewayError` is worth retrying.
///
/// The default predicate delegates to [`GatewayError::is_retryable`].
pub type RetryPredicate = Arc<dyn Fn(&GatewayError) -> bool + Send + Sync>;

/// Returns the default retry predicate backed by `GatewayError::is_retryable`.
fn default_retry_predicate() -> RetryPredicate {
    Arc::new(|err: &GatewayError| err.is_retryable())
}

// ── Sleeper (abstracted for testing) ────────────────────────────────────────

/// Abstraction over `tokio::time::sleep` so unit tests can substitute a
/// zero-cost or observable implementation.
#[async_trait]
pub(crate) trait Sleeper: Send + Sync {
    async fn sleep(&self, duration: Duration);
}

/// Production sleeper that delegates to `tokio::time::sleep`.
struct TokioSleeper;

#[async_trait]
impl Sleeper for TokioSleeper {
    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

// ── RetryMiddleware ─────────────────────────────────────────────────────────

/// A request middleware that retries transient failures with configurable
/// backoff and retry-predicate logic.
///
/// # Builder
///
/// Use [`RetryMiddlewareBuilder`] (via [`RetryMiddleware::builder`]) for
/// ergonomic construction:
///
/// ```rust
/// use swe_gateway::saf::RetryMiddleware;
/// use std::time::Duration;
///
/// let mw = RetryMiddleware::builder()
///     .max_attempts(5)
///     .exponential_backoff(Duration::from_millis(100), true)
///     .build();
/// ```
pub struct RetryMiddleware {
    /// Maximum number of total attempts (1 = no retry).
    max_attempts: u32,
    /// Backoff strategy between attempts.
    backoff: BackoffStrategy,
    /// Predicate deciding whether an error is retryable.
    predicate: RetryPredicate,
    /// Inner request middleware to wrap with retry logic.
    inner: Arc<dyn RequestMiddleware>,
    /// Sleeper implementation (swappable for tests).
    sleeper: Arc<dyn Sleeper>,
}

impl RetryMiddleware {
    /// Start building a `RetryMiddleware` with sensible defaults.
    pub fn builder() -> RetryMiddlewareBuilder {
        RetryMiddlewareBuilder::new()
    }

    /// Create directly with all fields (prefer the builder for public use).
    fn new(
        max_attempts: u32,
        backoff: BackoffStrategy,
        predicate: RetryPredicate,
        inner: Arc<dyn RequestMiddleware>,
        sleeper: Arc<dyn Sleeper>,
    ) -> Self {
        Self {
            max_attempts,
            backoff,
            predicate,
            inner,
            sleeper,
        }
    }
}

#[async_trait]
impl RequestMiddleware for RetryMiddleware {
    async fn process_request(
        &self,
        request: serde_json::Value,
    ) -> Result<serde_json::Value, GatewayError> {
        let mut last_error: Option<GatewayError> = None;

        for attempt in 0..self.max_attempts {
            // Clone the request for each attempt since the inner middleware
            // consumes it by value.
            let req_clone = request.clone();

            match self.inner.process_request(req_clone).await {
                Ok(response) => return Ok(response),
                Err(err) => {
                    let is_last_attempt = attempt + 1 >= self.max_attempts;
                    let is_retryable = (self.predicate)(&err);

                    if is_last_attempt || !is_retryable {
                        return Err(err);
                    }

                    tracing::warn!(
                        attempt = attempt + 1,
                        max_attempts = self.max_attempts,
                        error = %err,
                        "retrying transient gateway error"
                    );

                    let delay = self.backoff.compute_delay(attempt);
                    self.sleeper.sleep(delay).await;

                    last_error = Some(err);
                }
            }
        }

        // Should be unreachable if max_attempts >= 1, but guard defensively.
        Err(last_error.unwrap_or_else(|| {
            GatewayError::internal("retry loop exited without result or error")
        }))
    }
}

// ── Builder ─────────────────────────────────────────────────────────────────

/// Builder for [`RetryMiddleware`].
///
/// Defaults:
/// - `max_attempts`: 3
/// - `backoff`: `Exponential { base: 200ms, jitter: true }`
/// - `predicate`: `GatewayError::is_retryable`
pub struct RetryMiddlewareBuilder {
    max_attempts: u32,
    backoff: BackoffStrategy,
    predicate: RetryPredicate,
    sleeper: Option<Arc<dyn Sleeper>>,
}

impl RetryMiddlewareBuilder {
    /// Create a builder with production defaults.
    pub fn new() -> Self {
        Self {
            max_attempts: 3,
            backoff: BackoffStrategy::Exponential {
                base: Duration::from_millis(200),
                jitter: true,
            },
            predicate: default_retry_predicate(),
            sleeper: None,
        }
    }

    /// Set the maximum number of total attempts (including the first).
    ///
    /// # Panics
    ///
    /// Panics if `max_attempts` is 0.
    pub fn max_attempts(mut self, max_attempts: u32) -> Self {
        assert!(max_attempts > 0, "max_attempts must be at least 1");
        self.max_attempts = max_attempts;
        self
    }

    /// Use a fixed delay between retries.
    pub fn fixed_backoff(mut self, delay: Duration) -> Self {
        self.backoff = BackoffStrategy::Fixed { delay };
        self
    }

    /// Use exponential backoff with optional jitter.
    pub fn exponential_backoff(mut self, base: Duration, jitter: bool) -> Self {
        self.backoff = BackoffStrategy::Exponential { base, jitter };
        self
    }

    /// Override the retry predicate.
    ///
    /// The default predicate uses [`GatewayError::is_retryable`].
    pub fn retry_predicate(
        mut self,
        predicate: impl Fn(&GatewayError) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.predicate = Arc::new(predicate);
        self
    }

    /// Inject a custom sleeper (test-only, not public).
    #[cfg(test)]
    pub(crate) fn sleeper(mut self, sleeper: Arc<dyn Sleeper>) -> Self {
        self.sleeper = Some(sleeper);
        self
    }

    /// Build the middleware wrapping the given inner `RequestMiddleware`.
    pub fn build_with(self, inner: Arc<dyn RequestMiddleware>) -> RetryMiddleware {
        let sleeper = self.sleeper.unwrap_or_else(|| Arc::new(TokioSleeper));
        RetryMiddleware::new(self.max_attempts, self.backoff, self.predicate, inner, sleeper)
    }

    /// Build the middleware wrapping the given inner `RequestMiddleware`.
    ///
    /// Alias that takes ownership of a concrete type.
    pub fn build(self) -> RetryMiddlewareSpec {
        RetryMiddlewareSpec {
            max_attempts: self.max_attempts,
            backoff: self.backoff,
            predicate: self.predicate,
            sleeper: self.sleeper,
        }
    }
}

impl Default for RetryMiddlewareBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A partially-built retry specification that can be finalized with an inner
/// middleware via [`RetryMiddlewareSpec::wrap`].
///
/// This avoids requiring the inner middleware at builder time, which is useful
/// when the builder is used as a factory function return value.
pub struct RetryMiddlewareSpec {
    max_attempts: u32,
    backoff: BackoffStrategy,
    predicate: RetryPredicate,
    sleeper: Option<Arc<dyn Sleeper>>,
}

impl RetryMiddlewareSpec {
    /// Finalize the spec by wrapping the given inner middleware.
    pub fn wrap(self, inner: Arc<dyn RequestMiddleware>) -> RetryMiddleware {
        let sleeper = self.sleeper.unwrap_or_else(|| Arc::new(TokioSleeper));
        RetryMiddleware::new(self.max_attempts, self.backoff, self.predicate, inner, sleeper)
    }
}

// ── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    /// Sleeper that records durations instead of actually sleeping.
    struct RecordingSleeper {
        recorded: Mutex<Vec<Duration>>,
    }

    impl RecordingSleeper {
        fn new() -> Self {
            Self {
                recorded: Mutex::new(Vec::new()),
            }
        }

        fn recorded_delays(&self) -> Vec<Duration> {
            self.recorded.lock().clone()
        }
    }

    #[async_trait]
    impl Sleeper for RecordingSleeper {
        async fn sleep(&self, duration: Duration) {
            self.recorded.lock().push(duration);
        }
    }

    /// Middleware that fails N times with a given error, then succeeds.
    struct FailNThenSucceed {
        remaining_failures: Mutex<u32>,
        error_factory: Box<dyn Fn() -> GatewayError + Send + Sync>,
        success_value: serde_json::Value,
    }

    impl FailNThenSucceed {
        fn new(
            failures: u32,
            error_factory: impl Fn() -> GatewayError + Send + Sync + 'static,
            success_value: serde_json::Value,
        ) -> Self {
            Self {
                remaining_failures: Mutex::new(failures),
                error_factory: Box::new(error_factory),
                success_value,
            }
        }
    }

    #[async_trait]
    impl RequestMiddleware for FailNThenSucceed {
        async fn process_request(
            &self,
            _request: serde_json::Value,
        ) -> Result<serde_json::Value, GatewayError> {
            let mut remaining = self.remaining_failures.lock();
            if *remaining > 0 {
                *remaining -= 1;
                Err((self.error_factory)())
            } else {
                Ok(self.success_value.clone())
            }
        }
    }

    /// Middleware that always fails.
    struct AlwaysFail {
        error_factory: Box<dyn Fn() -> GatewayError + Send + Sync>,
    }

    impl AlwaysFail {
        fn new(error_factory: impl Fn() -> GatewayError + Send + Sync + 'static) -> Self {
            Self {
                error_factory: Box::new(error_factory),
            }
        }
    }

    #[async_trait]
    impl RequestMiddleware for AlwaysFail {
        async fn process_request(
            &self,
            _request: serde_json::Value,
        ) -> Result<serde_json::Value, GatewayError> {
            Err((self.error_factory)())
        }
    }

    // ── Smoke: backoff computation ──

    #[test]
    fn test_compute_delay_fixed_returns_constant() {
        let strategy = BackoffStrategy::Fixed {
            delay: Duration::from_millis(100),
        };
        assert_eq!(strategy.compute_delay(0), Duration::from_millis(100));
        assert_eq!(strategy.compute_delay(1), Duration::from_millis(100));
        assert_eq!(strategy.compute_delay(5), Duration::from_millis(100));
    }

    #[test]
    fn test_compute_delay_exponential_without_jitter_doubles() {
        let strategy = BackoffStrategy::Exponential {
            base: Duration::from_millis(100),
            jitter: false,
        };
        assert_eq!(strategy.compute_delay(0), Duration::from_millis(100));
        assert_eq!(strategy.compute_delay(1), Duration::from_millis(200));
        assert_eq!(strategy.compute_delay(2), Duration::from_millis(400));
        assert_eq!(strategy.compute_delay(3), Duration::from_millis(800));
    }

    #[test]
    fn test_compute_delay_exponential_with_jitter_at_least_base() {
        let strategy = BackoffStrategy::Exponential {
            base: Duration::from_millis(100),
            jitter: true,
        };
        // With jitter the delay is base * 2^attempt + random(0..base*2^attempt)
        // so it should be >= base * 2^attempt.
        for attempt in 0..5 {
            let delay = strategy.compute_delay(attempt);
            let min = Duration::from_millis(100 * 2u64.pow(attempt));
            assert!(
                delay >= min,
                "attempt {attempt}: delay {delay:?} < min {min:?}"
            );
        }
    }

    // ── Retry succeeds after transient failures ──

    #[tokio::test]
    async fn test_retry_succeeds_after_transient_failures() {
        let sleeper = Arc::new(RecordingSleeper::new());
        let inner = Arc::new(FailNThenSucceed::new(
            2,
            || GatewayError::unavailable("service down"),
            serde_json::json!({"ok": true}),
        ));

        let mw = RetryMiddleware::builder()
            .max_attempts(3)
            .fixed_backoff(Duration::from_millis(50))
            .sleeper(sleeper.clone())
            .build_with(inner);

        let result = mw
            .process_request(serde_json::json!({}))
            .await;

        assert!(result.is_ok(), "should succeed on 3rd attempt");
        assert_eq!(result.unwrap(), serde_json::json!({"ok": true}));

        let delays = sleeper.recorded_delays();
        assert_eq!(delays.len(), 2, "should have slept twice before succeeding");
        assert_eq!(delays[0], Duration::from_millis(50));
        assert_eq!(delays[1], Duration::from_millis(50));
    }

    // ── Gives up after max attempts ──

    #[tokio::test]
    async fn test_retry_gives_up_after_max_attempts() {
        let sleeper = Arc::new(RecordingSleeper::new());
        let inner = Arc::new(AlwaysFail::new(|| {
            GatewayError::timeout("request timed out")
        }));

        let mw = RetryMiddleware::builder()
            .max_attempts(3)
            .fixed_backoff(Duration::from_millis(10))
            .sleeper(sleeper.clone())
            .build_with(inner);

        let result = mw.process_request(serde_json::json!({})).await;

        assert!(result.is_err(), "should fail after exhausting attempts");
        let err = result.unwrap_err();
        assert!(
            matches!(err, GatewayError::Timeout(_)),
            "should return the last error: got {err:?}"
        );

        // 3 attempts = 2 sleeps (sleep happens between attempts, not after the last)
        let delays = sleeper.recorded_delays();
        assert_eq!(delays.len(), 2);
    }

    // ── Does not retry non-retryable errors ──

    #[tokio::test]
    async fn test_retry_does_not_retry_non_retryable_error() {
        let sleeper = Arc::new(RecordingSleeper::new());
        let inner = Arc::new(AlwaysFail::new(|| {
            GatewayError::not_found("resource missing")
        }));

        let mw = RetryMiddleware::builder()
            .max_attempts(5)
            .fixed_backoff(Duration::from_millis(10))
            .sleeper(sleeper.clone())
            .build_with(inner);

        let result = mw.process_request(serde_json::json!({})).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GatewayError::NotFound(_)));

        // No sleeps — gave up on the first attempt.
        assert!(
            sleeper.recorded_delays().is_empty(),
            "should not sleep for non-retryable errors"
        );
    }

    // ── Exponential backoff timing ──

    #[tokio::test]
    async fn test_retry_exponential_backoff_delays_increase() {
        let sleeper = Arc::new(RecordingSleeper::new());
        let inner = Arc::new(AlwaysFail::new(|| {
            GatewayError::ConnectionFailed("connection refused".into())
        }));

        let mw = RetryMiddleware::builder()
            .max_attempts(4)
            .exponential_backoff(Duration::from_millis(100), false)
            .sleeper(sleeper.clone())
            .build_with(inner);

        let _ = mw.process_request(serde_json::json!({})).await;

        let delays = sleeper.recorded_delays();
        assert_eq!(delays.len(), 3, "4 attempts = 3 sleeps");
        assert_eq!(delays[0], Duration::from_millis(100)); // 100 * 2^0
        assert_eq!(delays[1], Duration::from_millis(200)); // 100 * 2^1
        assert_eq!(delays[2], Duration::from_millis(400)); // 100 * 2^2
    }

    // ── Custom predicate override ──

    #[tokio::test]
    async fn test_retry_custom_predicate_overrides_default() {
        let sleeper = Arc::new(RecordingSleeper::new());
        // NotFound is NOT retryable by default, but we override:
        let inner = Arc::new(FailNThenSucceed::new(
            1,
            || GatewayError::not_found("temporary 404"),
            serde_json::json!({"found": true}),
        ));

        let mw = RetryMiddleware::builder()
            .max_attempts(3)
            .fixed_backoff(Duration::from_millis(10))
            .retry_predicate(|err| matches!(err, GatewayError::NotFound(_)))
            .sleeper(sleeper.clone())
            .build_with(inner);

        let result = mw.process_request(serde_json::json!({})).await;

        assert!(result.is_ok(), "should retry NotFound with custom predicate");
        assert_eq!(result.unwrap(), serde_json::json!({"found": true}));
        assert_eq!(sleeper.recorded_delays().len(), 1);
    }

    // ── Single attempt (no retry) ──

    #[tokio::test]
    async fn test_retry_max_attempts_one_does_not_retry() {
        let sleeper = Arc::new(RecordingSleeper::new());
        let inner = Arc::new(AlwaysFail::new(|| {
            GatewayError::unavailable("down")
        }));

        let mw = RetryMiddleware::builder()
            .max_attempts(1)
            .sleeper(sleeper.clone())
            .build_with(inner);

        let result = mw.process_request(serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(sleeper.recorded_delays().is_empty(), "no sleep for single attempt");
    }

    // ── Builder panics on zero max_attempts ──

    #[test]
    #[should_panic(expected = "max_attempts must be at least 1")]
    fn test_builder_panics_on_zero_max_attempts() {
        RetryMiddleware::builder().max_attempts(0);
    }

    // ── Builder default ──

    #[test]
    fn test_builder_default_matches_new() {
        let b = RetryMiddlewareBuilder::default();
        assert_eq!(b.max_attempts, 3);
        assert!(matches!(
            b.backoff,
            BackoffStrategy::Exponential { jitter: true, .. }
        ));
    }
}
