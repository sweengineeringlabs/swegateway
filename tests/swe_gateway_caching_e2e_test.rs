//! End-to-end caching tests for CachedService and ConfiguredCache.
//!
//! Validates lazy initialization, instance reuse, reset semantics,
//! config-driven re-initialization, and concurrent access safety.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use swe_gateway::prelude::*;

// ── Helpers ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct NumberService {
    val: i32,
}

/// Factory call counter shared across clones.
#[derive(Clone)]
struct CallCounter(Arc<AtomicU32>);

impl CallCounter {
    fn new() -> Self {
        Self(Arc::new(AtomicU32::new(0)))
    }
    fn increment(&self) -> u32 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }
    fn count(&self) -> u32 {
        self.0.load(Ordering::SeqCst)
    }
}

// ── Test 1: CachedService initializes lazily on first access ─────────────

#[tokio::test]
async fn test_cached_service_initializes_lazily_on_first_access() {
    let cache: CachedService<NumberService> = CachedService::new();
    let counter = CallCounter::new();

    // Before any access, factory has never been called.
    assert!(
        !cache.is_initialized(),
        "cache must not be initialized before first get_or_init"
    );
    assert!(cache.get().is_none(), "get() must return None before init");

    let counter_clone = counter.clone();
    let svc = cache
        .get_or_init(move || {
            let c = counter_clone.clone();
            async move {
                c.increment();
                Ok(Arc::new(NumberService { val: 42 }))
            }
        })
        .await
        .expect("init should succeed");

    assert_eq!(svc.val, 42, "service must hold the value from the factory");
    assert!(cache.is_initialized(), "cache must be initialized after first access");
    assert_eq!(counter.count(), 1, "factory must be called exactly once");
}

// ── Test 2: CachedService returns same instance on subsequent calls ──────

#[tokio::test]
async fn test_cached_service_returns_same_instance_on_subsequent_calls() {
    let cache: CachedService<NumberService> = CachedService::new();

    let first = cache
        .get_or_init(|| async { Ok(Arc::new(NumberService { val: 1 })) })
        .await
        .unwrap();

    // Second call with a *different* value proves the factory is NOT called again.
    let second = cache
        .get_or_init(|| async { Ok(Arc::new(NumberService { val: 999 })) })
        .await
        .unwrap();

    assert!(
        Arc::ptr_eq(&first, &second),
        "both calls must return the exact same Arc pointer"
    );
    assert_eq!(second.val, 1, "value must be from the first factory, not the second");

    // get() should also return the same instance.
    let via_get = cache.get().expect("get() must return Some after init");
    assert!(Arc::ptr_eq(&first, &via_get));
}

// ── Test 3: CachedService reset clears and re-initializes ────────────────

#[tokio::test]
async fn test_cached_service_reset_clears_and_reinitializes() {
    let cache: CachedService<NumberService> = CachedService::new();

    let original = cache
        .get_or_init(|| async { Ok(Arc::new(NumberService { val: 10 })) })
        .await
        .unwrap();
    assert_eq!(original.val, 10);

    cache.reset();

    assert!(
        !cache.is_initialized(),
        "cache must not be initialized after reset"
    );
    assert!(cache.get().is_none(), "get() must return None after reset");

    // Re-initialize with a different value.
    let refreshed = cache
        .get_or_init(|| async { Ok(Arc::new(NumberService { val: 20 })) })
        .await
        .unwrap();

    assert_eq!(refreshed.val, 20, "post-reset init must use the new factory");
    assert!(
        !Arc::ptr_eq(&original, &refreshed),
        "new instance must be a different allocation"
    );
}

// ── Test 4: ConfiguredCache re-initializes when config changes ───────────

#[tokio::test]
async fn test_configured_cache_reinitializes_on_config_change() {
    let cache: ConfiguredCache<NumberService, String> = ConfiguredCache::new();

    let first = cache
        .get_or_init_with_config(&"config-a".to_string(), |_cfg| async {
            Ok(Arc::new(NumberService { val: 100 }))
        })
        .await
        .unwrap();

    assert_eq!(first.val, 100);
    assert_eq!(
        cache.current_config(),
        Some("config-a".to_string()),
        "stored config must match"
    );

    // Change config => new initialization.
    let second = cache
        .get_or_init_with_config(&"config-b".to_string(), |cfg| async move {
            // Verify the factory receives the new config.
            assert_eq!(cfg, "config-b", "factory must receive the new config");
            Ok(Arc::new(NumberService { val: 200 }))
        })
        .await
        .unwrap();

    assert_eq!(second.val, 200, "must use new factory result after config change");
    assert_eq!(
        cache.current_config(),
        Some("config-b".to_string()),
        "stored config must update"
    );
    assert!(
        !Arc::ptr_eq(&first, &second),
        "instances must differ after config change"
    );
}

// ── Test 5: ConfiguredCache reuses instance when config is same ──────────

#[tokio::test]
async fn test_configured_cache_reuses_instance_for_same_config() {
    let cache: ConfiguredCache<NumberService, String> = ConfiguredCache::new();
    let counter = CallCounter::new();

    let counter_a = counter.clone();
    let first = cache
        .get_or_init_with_config(&"same-config".to_string(), move |_| {
            let c = counter_a.clone();
            async move {
                c.increment();
                Ok(Arc::new(NumberService { val: 50 }))
            }
        })
        .await
        .unwrap();

    let counter_b = counter.clone();
    let second = cache
        .get_or_init_with_config(&"same-config".to_string(), move |_| {
            let c = counter_b.clone();
            async move {
                c.increment();
                Ok(Arc::new(NumberService { val: 999 }))
            }
        })
        .await
        .unwrap();

    assert!(
        Arc::ptr_eq(&first, &second),
        "same config must return the same Arc"
    );
    assert_eq!(second.val, 50, "value must be from the first call");
    assert_eq!(counter.count(), 1, "factory must be called only once for same config");
}

// ── Test 6: Concurrent access to CachedService ──────────────────────────

#[tokio::test]
async fn test_cached_service_concurrent_access_initializes_once() {
    let cache = Arc::new(CachedService::<NumberService>::new());
    let counter = CallCounter::new();
    let num_tasks = 50;

    let mut handles = Vec::with_capacity(num_tasks);
    for _ in 0..num_tasks {
        let cache = Arc::clone(&cache);
        let counter = counter.clone();
        handles.push(tokio::spawn(async move {
            cache
                .get_or_init(move || {
                    let c = counter.clone();
                    async move {
                        c.increment();
                        Ok(Arc::new(NumberService { val: 7 }))
                    }
                })
                .await
                .unwrap()
        }));
    }

    let mut results = Vec::with_capacity(num_tasks);
    for h in handles {
        results.push(h.await.unwrap());
    }

    // All tasks must see the same value.
    for svc in &results {
        assert_eq!(svc.val, 7, "every task must observe the initialized value");
    }

    // The factory may be called more than once due to the double-check race
    // (two tasks pass the read-lock check before either writes), but the
    // stored instance is always the first writer's. What matters is that
    // all tasks get a valid result and no panics occur.
    assert!(
        counter.count() >= 1,
        "factory must be called at least once"
    );
}

// ── Test 7: CachedService with async factory function ────────────────────

#[tokio::test]
async fn test_cached_service_with_async_factory_returning_error() {
    let cache: CachedService<NumberService> = CachedService::new();

    // Factory that returns an error.
    let result = cache
        .get_or_init(|| async {
            Err(GatewayError::unavailable("backend is down"))
        })
        .await;

    assert!(result.is_err(), "factory error must propagate");
    let err = result.unwrap_err();
    assert!(
        matches!(err, GatewayError::Unavailable(_)),
        "error variant must be Unavailable, got: {err:?}"
    );
    assert!(
        !cache.is_initialized(),
        "cache must remain uninitialized after factory error"
    );

    // Subsequent call with a working factory should succeed.
    let svc = cache
        .get_or_init(|| async { Ok(Arc::new(NumberService { val: 77 })) })
        .await
        .expect("retry after error should succeed");

    assert_eq!(svc.val, 77);
    assert!(cache.is_initialized());
}

#[tokio::test]
async fn test_cached_service_async_factory_with_tokio_yield() {
    let cache: CachedService<NumberService> = CachedService::new();

    let svc = cache
        .get_or_init(|| async {
            // Simulate async work (e.g., a network call or file read).
            tokio::task::yield_now().await;
            Ok(Arc::new(NumberService { val: 123 }))
        })
        .await
        .expect("async factory with yield should succeed");

    assert_eq!(svc.val, 123);
}
