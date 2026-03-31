//! End-to-end tests for the token-bucket rate limiter middleware.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use swe_gateway::prelude::*;

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Extremely low refill rate so tests that exhaust tokens don't accidentally
/// refill during assertion windows.
const NEAR_ZERO_REFILL: f64 = 0.001;

// ─── Capacity tests ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_process_request_within_capacity_passes_through() {
    let limiter = swe_gateway::saf::rate_limiter(5, NEAR_ZERO_REFILL);

    for i in 0..5 {
        let input = serde_json::json!({"seq": i});
        let output = limiter.process_request(input.clone()).await.unwrap();
        assert_eq!(
            input, output,
            "request {i} should pass through unchanged"
        );
    }
}

#[tokio::test]
async fn test_process_request_rejects_when_tokens_exhausted() {
    let limiter = swe_gateway::saf::rate_limiter(2, NEAR_ZERO_REFILL);
    let payload = serde_json::json!({"model": "gpt-4"});

    // Consume all tokens.
    limiter.process_request(payload.clone()).await.unwrap();
    limiter.process_request(payload.clone()).await.unwrap();

    // Third request must fail.
    let err = limiter.process_request(payload).await.unwrap_err();
    assert!(
        matches!(err, GatewayError::RateLimitExceeded(_)),
        "expected RateLimitExceeded, got: {err:?}"
    );
    assert!(err.is_retryable(), "rate-limit errors must be retryable");
}

// ─── Refill tests ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_tokens_refill_over_time() {
    // 100 tokens/s => 1 token every 10ms.  With capacity 1, exhaust then wait.
    let limiter = swe_gateway::saf::rate_limiter(1, 100.0);
    let payload = serde_json::json!({"x": 1});

    limiter.process_request(payload.clone()).await.unwrap();
    assert!(
        limiter.process_request(payload.clone()).await.is_err(),
        "should be exhausted immediately after consuming the single token"
    );

    // Wait 50ms — at 100/s that refills ~5 tokens, capped to 1 (capacity).
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(
        limiter.process_request(payload).await.is_ok(),
        "should succeed after refill window"
    );
}

// ─── Builder tests ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_builder_produces_working_limiter() {
    let limiter = swe_gateway::saf::rate_limiter_builder()
        .capacity(3)
        .refill_rate(NEAR_ZERO_REFILL)
        .build();

    let payload = serde_json::json!({"test": true});

    for _ in 0..3 {
        limiter.process_request(payload.clone()).await.unwrap();
    }

    assert!(
        limiter.process_request(payload).await.is_err(),
        "4th request should be rejected with capacity=3"
    );
}

// ─── Pipeline integration ───────────────────────────────────────────────────

#[tokio::test]
async fn test_rate_limiter_in_pipeline_rejects_excess_requests() {
    let limiter: Arc<dyn RequestMiddleware> =
        Arc::new(swe_gateway::saf::rate_limiter(2, NEAR_ZERO_REFILL));

    let router: Arc<dyn Router> = Arc::new(ClosureRouter::new(|req: &serde_json::Value| {
        Ok(req.clone())
    }));

    let pipeline = Pipeline::new(vec![limiter], router, vec![]);

    let payload = serde_json::json!({"model": "claude-3"});

    assert!(pipeline.execute(payload.clone()).await.is_ok());
    assert!(pipeline.execute(payload.clone()).await.is_ok());

    let err = pipeline.execute(payload).await.unwrap_err();
    assert!(matches!(err, GatewayError::RateLimitExceeded(_)));
}

// ─── Concurrent access ─────────────────────────────────────────────────────

#[test]
fn test_concurrent_access_total_grants_equals_capacity() {
    let capacity = 50u64;
    let limiter = Arc::new(swe_gateway::saf::rate_limiter(capacity, NEAR_ZERO_REFILL));
    let success_count = Arc::new(std::sync::atomic::AtomicU64::new(0));

    let mut handles = Vec::new();
    // Spawn more threads than capacity to guarantee contention.
    let thread_count = 100;
    let attempts_per_thread = 5;

    for _ in 0..thread_count {
        let limiter = Arc::clone(&limiter);
        let counter = Arc::clone(&success_count);
        handles.push(thread::spawn(move || {
            for _ in 0..attempts_per_thread {
                if limiter.try_acquire().is_ok() {
                    counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
        }));
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    let total_granted = success_count.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        total_granted, capacity,
        "exactly {capacity} tokens should be granted across all threads, got {total_granted}"
    );
}

#[tokio::test]
async fn test_concurrent_async_tasks_respect_capacity() {
    let capacity = 20u64;
    let limiter = Arc::new(swe_gateway::saf::rate_limiter(capacity, NEAR_ZERO_REFILL));
    let success_count = Arc::new(std::sync::atomic::AtomicU64::new(0));

    let mut tasks = Vec::new();
    let task_count = 50;

    for _ in 0..task_count {
        let limiter = Arc::clone(&limiter);
        let counter = Arc::clone(&success_count);
        tasks.push(tokio::spawn(async move {
            let payload = serde_json::json!({"t": true});
            if limiter.process_request(payload).await.is_ok() {
                counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }));
    }

    for t in tasks {
        t.await.expect("task panicked");
    }

    let total_granted = success_count.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        total_granted, capacity,
        "exactly {capacity} tokens should be granted across async tasks, got {total_granted}"
    );
}
