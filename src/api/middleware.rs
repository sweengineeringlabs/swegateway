//! Generic middleware traits for request/response pipeline processing.
//!
//! Generic over `Req`, `Resp`, and `Err` — downstream crates supply their
//! own types with full compile-time safety. All default to swe-gateway's
//! types so existing untyped code works unchanged.
//!
//! # Short-Circuit Support
//!
//! Request middleware can short-circuit the pipeline by returning
//! [`MiddlewareAction::ShortCircuit`] from [`RequestMiddleware::process_request_action`].
//! When a middleware short-circuits, remaining pre-middleware and the router
//! are skipped. Post-middleware still runs on the short-circuited response
//! (useful for logging, metrics, header injection, etc.).

use async_trait::async_trait;
use crate::api::types::GatewayError;

/// Action returned by request middleware to control pipeline flow.
///
/// - `Continue(Req)` — pass the (possibly modified) request to the next
///   middleware or the router.
/// - `ShortCircuit(Resp)` — skip remaining pre-middleware and the router,
///   returning the provided response directly. Post-middleware still runs.
pub enum MiddlewareAction<Req, Resp> {
    /// Continue processing with the (possibly modified) request.
    Continue(Req),
    /// Short-circuit the pipeline, returning this response immediately.
    ///
    /// Remaining pre-middleware and the router are skipped.
    /// Post-middleware still runs on this response.
    ShortCircuit(Resp),
}

/// Middleware that intercepts and optionally transforms an inbound request
/// before it reaches the router.
///
/// Generic over `Req`, `Err`, and `Resp`. Defaults:
/// - `Req` = `serde_json::Value`
/// - `Err` = `GatewayError`
/// - `Resp` = `serde_json::Value`
///
/// # Short-Circuit
///
/// Override [`process_request_action`](Self::process_request_action) to return
/// [`MiddlewareAction::ShortCircuit`] and skip the router entirely.
/// The default implementation delegates to [`process_request`](Self::process_request)
/// and wraps the result in [`MiddlewareAction::Continue`], so existing
/// implementations compile without changes.
///
/// # Backward Compatibility
///
/// The `Resp` parameter is appended after `Err` with a default of
/// `serde_json::Value`, so existing code using `RequestMiddleware`,
/// `RequestMiddleware<MyReq, MyErr>`, etc. continues to work unchanged.
///
/// # Object Safety
///
/// Object-safe: `Arc<dyn RequestMiddleware<MyReq, MyErr, MyResp>>`.
#[async_trait]
pub trait RequestMiddleware<
    Req: Send + Sync + 'static = serde_json::Value,
    Err: Send + Sync + 'static = GatewayError,
    Resp: Send + Sync + 'static = serde_json::Value,
>: Send + Sync {
    /// Process an inbound request, returning the (possibly modified) request
    /// or an error to abort the pipeline.
    ///
    /// This is the primary method to implement. For short-circuit support,
    /// override [`process_request_action`](Self::process_request_action) instead.
    async fn process_request(&self, request: Req) -> Result<Req, Err>;

    /// Process an inbound request with short-circuit support.
    ///
    /// The default implementation delegates to [`process_request`](Self::process_request)
    /// and wraps the result in [`MiddlewareAction::Continue`].
    ///
    /// Override this method to return [`MiddlewareAction::ShortCircuit`] when the
    /// middleware wants to skip the remaining pipeline and return a response directly.
    async fn process_request_action(&self, request: Req) -> Result<MiddlewareAction<Req, Resp>, Err> {
        self.process_request(request).await.map(MiddlewareAction::Continue)
    }
}

/// Middleware that intercepts and optionally transforms an outbound response
/// after the router has produced it.
///
/// Generic over `Resp` and `Err`. Defaults:
/// - `Resp` = `serde_json::Value`
/// - `Err` = `GatewayError`
///
/// # Object Safety
///
/// Object-safe: `Arc<dyn ResponseMiddleware<MyResponse, MyError>>`.
#[async_trait]
pub trait ResponseMiddleware<
    Resp: Send + Sync + 'static = serde_json::Value,
    Err: Send + Sync + 'static = GatewayError,
>: Send + Sync {
    /// Process an outbound response, returning the (possibly modified) response
    /// or an error.
    async fn process_response(&self, response: Resp) -> Result<Resp, Err>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::GatewayResult;
    use std::sync::Arc;

    // === Default types (backward compat) ===

    #[test]
    fn test_request_middleware_is_object_safe() {
        fn _accepts(_m: &dyn RequestMiddleware) {}
        fn _accepts_arc(_m: Arc<dyn RequestMiddleware>) {}
    }

    #[test]
    fn test_response_middleware_is_object_safe() {
        fn _accepts(_m: &dyn ResponseMiddleware) {}
        fn _accepts_arc(_m: Arc<dyn ResponseMiddleware>) {}
    }

    struct PassthroughRequest;

    #[async_trait]
    impl RequestMiddleware for PassthroughRequest {
        async fn process_request(&self, request: serde_json::Value) -> GatewayResult<serde_json::Value> {
            Ok(request)
        }
    }

    struct PassthroughResponse;

    #[async_trait]
    impl ResponseMiddleware for PassthroughResponse {
        async fn process_response(&self, response: serde_json::Value) -> GatewayResult<serde_json::Value> {
            Ok(response)
        }
    }

    #[tokio::test]
    async fn test_passthrough_preserves_value() {
        let mw = PassthroughRequest;
        let input = serde_json::json!({"model": "gpt-4"});
        let output = mw.process_request(input.clone()).await.unwrap();
        assert_eq!(input, output);
    }

    // === Custom types + custom error ===

    #[derive(Debug, Clone)]
    struct MyRequest { model: String }

    #[derive(Debug, Clone)]
    struct MyResponse { content: String }

    #[derive(Debug)]
    struct MyError(String);

    #[test]
    fn test_typed_with_custom_error_is_object_safe() {
        fn _accepts(_m: &dyn RequestMiddleware<MyRequest, MyError>) {}
        fn _accepts_arc(_m: Arc<dyn RequestMiddleware<MyRequest, MyError>>) {}
        fn _accepts_resp(_m: Arc<dyn ResponseMiddleware<MyResponse, MyError>>) {}
    }

    struct TypedMiddleware;

    #[async_trait]
    impl RequestMiddleware<MyRequest, MyError> for TypedMiddleware {
        async fn process_request(&self, request: MyRequest) -> Result<MyRequest, MyError> {
            Ok(request)
        }
    }

    #[async_trait]
    impl ResponseMiddleware<MyResponse, MyError> for TypedMiddleware {
        async fn process_response(&self, response: MyResponse) -> Result<MyResponse, MyError> {
            Ok(response)
        }
    }

    #[tokio::test]
    async fn test_typed_request_with_custom_error() {
        let mw = TypedMiddleware;
        let output = mw.process_request(MyRequest { model: "gpt-4".into() }).await.unwrap();
        assert_eq!(output.model, "gpt-4");
    }

    #[tokio::test]
    async fn test_typed_response_with_custom_error() {
        let mw = TypedMiddleware;
        let output = mw.process_response(MyResponse { content: "hello".into() }).await.unwrap();
        assert_eq!(output.content, "hello");
    }
}
