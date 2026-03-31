//! Token-bucket rate limiter middleware.
//!
//! Implements [`RequestMiddleware`] using a classic token-bucket algorithm.
//! Thread-safe via `parking_lot::Mutex` — suitable for shared use behind
//! `Arc` in concurrent pipelines.

use async_trait::async_trait;
use parking_lot::Mutex;
use std::time::Instant;

use crate::api::middleware::RequestMiddleware;
use crate::api::types::{GatewayError, GatewayResult};

// ── Constants ───────────────────────────────────────────────────────────────

/// Default bucket capacity (maximum burst size).
const DEFAULT_CAPACITY: u64 = 100;

/// Default refill rate in tokens per second.
const DEFAULT_REFILL_RATE: f64 = 10.0;

/// Minimum permitted refill rate to prevent division-by-zero or starvation.
const MIN_REFILL_RATE: f64 = 0.001;

// ── Builder ─────────────────────────────────────────────────────────────────

/// Builder for constructing a [`RateLimiter`] with custom parameters.
///
/// # Example
///
/// ```rust
/// use swe_gateway::saf::RateLimiterBuilder;
///
/// let limiter = RateLimiterBuilder::new()
///     .capacity(50)
///     .refill_rate(20.0)
///     .build();
/// ```
pub struct RateLimiterBuilder {
    capacity: u64,
    refill_rate: f64,
}

impl RateLimiterBuilder {
    /// Create a new builder with default values.
    pub fn new() -> Self {
        Self {
            capacity: DEFAULT_CAPACITY,
            refill_rate: DEFAULT_REFILL_RATE,
        }
    }

    /// Set the maximum number of tokens the bucket can hold (burst capacity).
    ///
    /// Must be at least 1.
    pub fn capacity(mut self, capacity: u64) -> Self {
        self.capacity = capacity.max(1);
        self
    }

    /// Set the refill rate in tokens per second.
    ///
    /// Clamped to a minimum of [`MIN_REFILL_RATE`] to prevent starvation.
    pub fn refill_rate(mut self, rate: f64) -> Self {
        self.refill_rate = rate.max(MIN_REFILL_RATE);
        self
    }

    /// Build the [`RateLimiter`].
    pub fn build(self) -> RateLimiter {
        RateLimiter {
            state: Mutex::new(BucketState {
                tokens: self.capacity as f64,
                last_refill: Instant::now(),
            }),
            capacity: self.capacity,
            refill_rate: self.refill_rate,
        }
    }
}

impl Default for RateLimiterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ── Bucket state ────────────────────────────────────────────────────────────

/// Internal mutable state of the token bucket.
struct BucketState {
    /// Current number of available tokens (fractional to allow sub-second refill).
    tokens: f64,
    /// Timestamp of the last refill calculation.
    last_refill: Instant,
}

// ── RateLimiter ─────────────────────────────────────────────────────────────

/// Token-bucket rate limiter that implements [`RequestMiddleware`].
///
/// Each call to [`process_request`](RequestMiddleware::process_request)
/// consumes one token. When the bucket is empty the middleware short-circuits
/// the pipeline with [`GatewayError::RateLimitExceeded`].
///
/// Thread-safe: the internal state is protected by a `parking_lot::Mutex`,
/// so the limiter can be wrapped in `Arc` and shared across tasks.
pub struct RateLimiter {
    state: Mutex<BucketState>,
    capacity: u64,
    refill_rate: f64,
}

impl RateLimiter {
    /// Create a rate limiter with the given capacity and refill rate.
    pub fn new(capacity: u64, refill_rate: f64) -> Self {
        RateLimiterBuilder::new()
            .capacity(capacity)
            .refill_rate(refill_rate)
            .build()
    }

    /// Attempt to acquire a single token.
    ///
    /// Returns `Ok(())` if a token was available, or
    /// `Err(GatewayError::RateLimitExceeded)` if the bucket is empty.
    pub fn try_acquire(&self) -> GatewayResult<()> {
        let mut state = self.state.lock();
        let now = Instant::now();
        let elapsed = now.duration_since(state.last_refill).as_secs_f64();

        // Refill tokens based on elapsed time, capped at capacity.
        let new_tokens = state.tokens + elapsed * self.refill_rate;
        state.tokens = new_tokens.min(self.capacity as f64);
        state.last_refill = now;

        if state.tokens >= 1.0 {
            state.tokens -= 1.0;
            Ok(())
        } else {
            Err(GatewayError::RateLimitExceeded(format!(
                "capacity {}, refill rate {}/s — try again shortly",
                self.capacity, self.refill_rate
            )))
        }
    }

    /// Returns the current (approximate) number of available tokens.
    ///
    /// This performs a refill calculation, so the value is accurate at the
    /// time of the call but may change immediately after.
    pub fn available_tokens(&self) -> u64 {
        let mut state = self.state.lock();
        let now = Instant::now();
        let elapsed = now.duration_since(state.last_refill).as_secs_f64();
        let new_tokens = state.tokens + elapsed * self.refill_rate;
        state.tokens = new_tokens.min(self.capacity as f64);
        state.last_refill = now;
        state.tokens as u64
    }
}

#[async_trait]
impl RequestMiddleware for RateLimiter {
    async fn process_request(
        &self,
        request: serde_json::Value,
    ) -> GatewayResult<serde_json::Value> {
        self.try_acquire()?;
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_try_acquire_within_capacity_succeeds() {
        let limiter = RateLimiterBuilder::new()
            .capacity(3)
            .refill_rate(0.0)
            .build();

        assert!(limiter.try_acquire().is_ok(), "1st token should succeed");
        assert!(limiter.try_acquire().is_ok(), "2nd token should succeed");
        assert!(limiter.try_acquire().is_ok(), "3rd token should succeed");
    }

    #[test]
    fn test_try_acquire_exhausted_returns_rate_limit_exceeded() {
        let limiter = RateLimiter::new(2, MIN_REFILL_RATE);

        limiter.try_acquire().unwrap();
        limiter.try_acquire().unwrap();

        let err = limiter.try_acquire().unwrap_err();
        assert!(
            matches!(err, GatewayError::RateLimitExceeded(_)),
            "expected RateLimitExceeded, got: {err:?}"
        );
        assert!(err.is_retryable(), "rate-limit errors should be retryable");
    }

    #[test]
    fn test_tokens_refill_over_time() {
        let limiter = RateLimiterBuilder::new()
            .capacity(2)
            .refill_rate(100.0) // 100 tokens/s => 1 token per 10ms
            .build();

        // Exhaust all tokens.
        limiter.try_acquire().unwrap();
        limiter.try_acquire().unwrap();
        assert!(limiter.try_acquire().is_err(), "bucket should be empty");

        // Wait enough time for at least 1 token to refill.
        thread::sleep(Duration::from_millis(50));

        assert!(
            limiter.try_acquire().is_ok(),
            "should have refilled at least 1 token after 50ms at 100/s"
        );
    }

    #[test]
    fn test_refill_does_not_exceed_capacity() {
        let limiter = RateLimiterBuilder::new()
            .capacity(3)
            .refill_rate(1000.0)
            .build();

        // Wait for a generous refill window.
        thread::sleep(Duration::from_millis(50));

        // Should only be able to consume `capacity` tokens.
        assert!(limiter.try_acquire().is_ok());
        assert!(limiter.try_acquire().is_ok());
        assert!(limiter.try_acquire().is_ok());
        assert!(
            limiter.try_acquire().is_err(),
            "tokens should be capped at capacity (3)"
        );
    }

    #[test]
    fn test_builder_clamps_capacity_minimum() {
        let limiter = RateLimiterBuilder::new().capacity(0).build();
        // Capacity 0 is clamped to 1.
        assert!(limiter.try_acquire().is_ok(), "clamped capacity of 1 should allow one request");
        assert!(limiter.try_acquire().is_err());
    }

    #[test]
    fn test_builder_clamps_refill_rate_minimum() {
        let limiter = RateLimiterBuilder::new()
            .refill_rate(-5.0)
            .build();
        // Negative rate should be clamped to MIN_REFILL_RATE.
        assert!(limiter.refill_rate >= MIN_REFILL_RATE);
    }

    #[tokio::test]
    async fn test_process_request_passes_through_on_success() {
        let limiter = RateLimiter::new(10, 10.0);
        let input = serde_json::json!({"model": "gpt-4"});
        let output = limiter.process_request(input.clone()).await.unwrap();
        assert_eq!(input, output, "middleware should pass through the request unchanged");
    }

    #[tokio::test]
    async fn test_process_request_rejects_when_exhausted() {
        let limiter = RateLimiter::new(1, MIN_REFILL_RATE);
        let input = serde_json::json!({"model": "gpt-4"});

        limiter.process_request(input.clone()).await.unwrap();

        let err = limiter.process_request(input).await.unwrap_err();
        assert!(matches!(err, GatewayError::RateLimitExceeded(_)));
    }
}
