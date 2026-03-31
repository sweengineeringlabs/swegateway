//! State management patterns for gateway providers.
//!
//! Provides utilities for lazy initialization and caching of expensive
//! services within gateway providers.

use parking_lot::RwLock;
use std::future::Future;
use std::sync::Arc;

/// Lazy-initialized service wrapper using parking_lot for synchronization.
///
/// Designed for use within stateful providers that need to cache
/// expensive service instances.
pub struct CachedService<S: ?Sized> {
    inner: RwLock<Option<Arc<S>>>,
}

impl<S: ?Sized> CachedService<S> {
    /// Create a new uninitialized cached service.
    pub const fn new() -> Self {
        Self {
            inner: RwLock::new(None),
        }
    }

    /// Get the cached service or initialize it with the provided factory.
    ///
    /// Uses double-checked locking for efficiency:
    /// - Fast path: Read lock to check if initialized
    /// - Slow path: Initialize without lock, then write lock to store
    ///
    /// Note: Locks are never held across `.await` points to ensure the
    /// returned future is `Send`.
    pub async fn get_or_init<F, Fut>(&self, init: F) -> crate::GatewayResult<Arc<S>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = crate::GatewayResult<Arc<S>>>,
    {
        // Fast path: check with read lock (no await while holding)
        {
            let guard = self.inner.read();
            if let Some(ref service) = *guard {
                return Ok(Arc::clone(service));
            }
        }
        // Lock is dropped here before any await

        // Initialize service (no lock held)
        let new_service = init().await?;

        // Acquire write lock and store (double-check pattern)
        {
            let mut guard = self.inner.write();
            // Another task may have initialized while we were awaiting
            if guard.is_none() {
                *guard = Some(Arc::clone(&new_service));
            }
            // Return the cached value (may be from another task)
            Ok(Arc::clone(guard.as_ref().unwrap()))
        }
    }

    /// Check if the service has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.inner.read().is_some()
    }

    /// Reset the service, forcing re-initialization on next access.
    pub fn reset(&self) {
        let mut guard = self.inner.write();
        *guard = None;
    }

    /// Get the cached service if initialized, without attempting initialization.
    pub fn get(&self) -> Option<Arc<S>> {
        self.inner.read().as_ref().map(Arc::clone)
    }
}

impl<S: ?Sized> Default for CachedService<S> {
    fn default() -> Self {
        Self::new()
    }
}

/// Cached service with configuration key for parameterized initialization.
///
/// Caches a service along with the configuration used to create it,
/// allowing re-initialization when configuration changes.
pub struct ConfiguredCache<S: ?Sized, C: PartialEq + Clone> {
    service: RwLock<Option<Arc<S>>>,
    config: RwLock<Option<C>>,
}

impl<S: ?Sized, C: PartialEq + Clone> ConfiguredCache<S, C> {
    /// Create a new uninitialized configured cache.
    pub const fn new() -> Self {
        Self {
            service: RwLock::new(None),
            config: RwLock::new(None),
        }
    }

    /// Get the cached service or initialize it with the provided configuration.
    ///
    /// If the configuration has changed since last initialization, the service
    /// is re-initialized with the new configuration.
    ///
    /// Note: Locks are never held across `.await` points to ensure the
    /// returned future is `Send`.
    pub async fn get_or_init_with_config<F, Fut>(
        &self,
        config: &C,
        init: F,
    ) -> crate::GatewayResult<Arc<S>>
    where
        F: FnOnce(C) -> Fut,
        Fut: Future<Output = crate::GatewayResult<Arc<S>>>,
    {
        // Fast path: check if we have a cached service with matching config
        {
            let service_guard = self.service.read();
            let config_guard = self.config.read();
            if let (Some(ref service), Some(ref cached_config)) = (&*service_guard, &*config_guard)
            {
                if cached_config == config {
                    return Ok(Arc::clone(service));
                }
            }
        }
        // Locks are dropped here before any await

        // Initialize with the new config (no locks held)
        let new_service = init(config.clone()).await?;

        // Acquire write locks and store
        {
            let mut service_guard = self.service.write();
            let mut config_guard = self.config.write();
            *service_guard = Some(Arc::clone(&new_service));
            *config_guard = Some(config.clone());
        }

        Ok(new_service)
    }

    /// Check if the cache has a service.
    pub fn is_initialized(&self) -> bool {
        self.service.read().is_some()
    }

    /// Get the current configuration if set.
    pub fn current_config(&self) -> Option<C> {
        self.config.read().clone()
    }

    /// Reset the cache.
    pub fn reset(&self) {
        let mut service_guard = self.service.write();
        let mut config_guard = self.config.write();
        *service_guard = None;
        *config_guard = None;
    }
}

impl<S: ?Sized, C: PartialEq + Clone> Default for ConfiguredCache<S, C> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestService {
        value: i32,
    }

    /// @covers: get_or_init
    #[test]
    fn test_get_or_init_with_runtime() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cache: CachedService<TestService> = CachedService::new();

        let result = rt.block_on(async {
            cache
                .get_or_init(|| async { Ok(Arc::new(TestService { value: 42 })) })
                .await
        });

        assert!(result.is_ok());
        assert_eq!(result.unwrap().value, 42);
        assert!(cache.is_initialized());
    }

    /// @covers: is_initialized
    #[test]
    fn test_is_initialized_sync() {
        let cache: CachedService<TestService> = CachedService::new();
        assert!(!cache.is_initialized(), "new cache should not be initialized");
    }

    /// @covers: reset
    #[test]
    fn test_reset_sync() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cache: CachedService<TestService> = CachedService::new();

        rt.block_on(async {
            cache
                .get_or_init(|| async { Ok(Arc::new(TestService { value: 1 })) })
                .await
                .unwrap();
        });

        assert!(cache.is_initialized());
        cache.reset();
        assert!(!cache.is_initialized(), "reset should clear the cached service");
    }

    /// @covers: get
    #[test]
    fn test_get_sync() {
        let cache: CachedService<TestService> = CachedService::new();
        assert!(cache.get().is_none(), "get on empty cache should return None");
    }

    /// @covers: get_or_init_with_config
    #[test]
    fn test_get_or_init_with_config_sync() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cache: ConfiguredCache<TestService, String> = ConfiguredCache::new();

        let result = rt.block_on(async {
            cache
                .get_or_init_with_config(&"cfg1".to_string(), |_| async {
                    Ok(Arc::new(TestService { value: 10 }))
                })
                .await
        });

        assert!(result.is_ok());
        assert_eq!(result.unwrap().value, 10);
        assert!(cache.is_initialized());
    }

    /// @covers: current_config
    #[test]
    fn test_current_config_sync() {
        let cache: ConfiguredCache<TestService, String> = ConfiguredCache::new();
        assert!(
            cache.current_config().is_none(),
            "new configured cache should have no config"
        );
    }

    /// @covers: get_or_init
    #[tokio::test]
    async fn test_get_or_init() {
        let cache: CachedService<TestService> = CachedService::new();
        assert!(!cache.is_initialized());

        let result = cache
            .get_or_init(|| async { Ok(Arc::new(TestService { value: 42 })) })
            .await;

        assert!(result.is_ok());
        assert!(cache.is_initialized());
        assert_eq!(result.unwrap().value, 42);
    }

    /// @covers: get_or_init
    #[tokio::test]
    async fn test_get_or_init_returns_same_instance() {
        let cache: CachedService<TestService> = CachedService::new();

        let first = cache
            .get_or_init(|| async { Ok(Arc::new(TestService { value: 1 })) })
            .await
            .unwrap();

        let second = cache
            .get_or_init(|| async { Ok(Arc::new(TestService { value: 2 })) })
            .await
            .unwrap();

        // Should return the first instance, not create a new one
        assert_eq!(first.value, 1);
        assert_eq!(second.value, 1);
        assert!(Arc::ptr_eq(&first, &second));
    }

    /// @covers: is_initialized
    #[tokio::test]
    async fn test_is_initialized() {
        let cache: CachedService<TestService> = CachedService::new();
        assert!(!cache.is_initialized());

        cache
            .get_or_init(|| async { Ok(Arc::new(TestService { value: 1 })) })
            .await
            .unwrap();

        assert!(cache.is_initialized());
    }

    /// @covers: reset
    #[tokio::test]
    async fn test_reset() {
        let cache: CachedService<TestService> = CachedService::new();

        cache
            .get_or_init(|| async { Ok(Arc::new(TestService { value: 1 })) })
            .await
            .unwrap();

        assert!(cache.is_initialized());
        cache.reset();
        assert!(!cache.is_initialized());
    }

    /// @covers: get
    #[tokio::test]
    async fn test_get() {
        let cache: CachedService<TestService> = CachedService::new();
        assert!(cache.get().is_none());

        cache
            .get_or_init(|| async { Ok(Arc::new(TestService { value: 7 })) })
            .await
            .unwrap();

        let svc = cache.get();
        assert!(svc.is_some());
        assert_eq!(svc.unwrap().value, 7);
    }

    /// @covers: get_or_init_with_config
    #[tokio::test]
    async fn test_get_or_init_with_config() {
        let cache: ConfiguredCache<TestService, String> = ConfiguredCache::new();

        let first = cache
            .get_or_init_with_config(&"config1".to_string(), |_| async {
                Ok(Arc::new(TestService { value: 1 }))
            })
            .await
            .unwrap();

        assert_eq!(first.value, 1);

        // Same config should return same instance
        let second = cache
            .get_or_init_with_config(&"config1".to_string(), |_| async {
                Ok(Arc::new(TestService { value: 2 }))
            })
            .await
            .unwrap();

        assert_eq!(second.value, 1);

        // Different config should re-initialize
        let third = cache
            .get_or_init_with_config(&"config2".to_string(), |_| async {
                Ok(Arc::new(TestService { value: 3 }))
            })
            .await
            .unwrap();

        assert_eq!(third.value, 3);
    }

    /// @covers: current_config
    #[tokio::test]
    async fn test_current_config() {
        let cache: ConfiguredCache<TestService, String> = ConfiguredCache::new();
        assert!(cache.current_config().is_none());

        cache
            .get_or_init_with_config(&"my_config".to_string(), |_| async {
                Ok(Arc::new(TestService { value: 1 }))
            })
            .await
            .unwrap();

        assert_eq!(cache.current_config(), Some("my_config".to_string()));
    }

    /// @covers: is_initialized
    #[tokio::test]
    async fn test_configured_cache_is_initialized() {
        let cache: ConfiguredCache<TestService, String> = ConfiguredCache::new();
        assert!(!cache.is_initialized());

        cache
            .get_or_init_with_config(&"cfg".to_string(), |_| async {
                Ok(Arc::new(TestService { value: 1 }))
            })
            .await
            .unwrap();

        assert!(cache.is_initialized());
    }

    /// @covers: reset
    #[tokio::test]
    async fn test_configured_cache_reset() {
        let cache: ConfiguredCache<TestService, String> = ConfiguredCache::new();

        cache
            .get_or_init_with_config(&"cfg".to_string(), |_| async {
                Ok(Arc::new(TestService { value: 1 }))
            })
            .await
            .unwrap();

        assert!(cache.is_initialized());
        cache.reset();
        assert!(!cache.is_initialized());
        assert!(cache.current_config().is_none());
    }
}
