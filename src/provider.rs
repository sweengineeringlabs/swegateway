//! Gateway provider traits.
//!
//! Provides the foundation for building gateway plugins using
//! the SPI (Service Provider Interface) pattern.

use std::future::Future;
use std::sync::Arc;

/// Marker trait for stateless gateway providers.
///
/// Stateless providers are created fresh for each request and do not
/// maintain any internal state. They are cheap to construct.
pub trait StatelessProvider: Default + Clone + Send + Sync + 'static {}

/// Marker trait for stateful gateway providers.
///
/// Stateful providers maintain internal state (caches, connections)
/// and should be wrapped in `Arc` for sharing across requests.
pub trait StatefulProvider: Send + Sync + 'static {}

/// Trait for providers that support lazy service initialization.
///
/// Implementations cache expensive services and initialize them on first use.
pub trait LazyInit<S: ?Sized>: Send + Sync {
    /// Get or initialize the cached service.
    fn get_service(&self) -> impl Future<Output = crate::GatewayResult<Arc<S>>> + Send;

    /// Check if the service has been initialized.
    fn is_initialized(&self) -> bool;

    /// Reset the cached service, forcing re-initialization on next access.
    fn reset(&self);
}

/// Trait for providers that support parameterized initialization.
///
/// Some providers need different configurations for different contexts
/// (e.g., different working directories).
pub trait LazyInitWithConfig<S: ?Sized, C>: Send + Sync {
    /// Get or initialize the cached service with configuration.
    fn get_service_with_config(
        &self,
        config: &C,
    ) -> impl Future<Output = crate::GatewayResult<Arc<S>>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default, Clone)]
    struct TestStateless;
    impl StatelessProvider for TestStateless {}

    struct TestStateful;
    impl StatefulProvider for TestStateful {}

    #[test]
    fn test_stateless_provider_impl() {
        let p = TestStateless::default();
        let _clone = p.clone();
    }

    #[test]
    fn test_stateful_provider_impl() {
        let _p = TestStateful;
    }
}
