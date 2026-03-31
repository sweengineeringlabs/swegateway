//! Generic request/response pipeline.
//!
//! Generic over `Req`, `Resp`, and `Err`. All default to swe-gateway types.

use async_trait::async_trait;
use std::sync::Arc;
use futures::future::BoxFuture;

use crate::api::middleware::{MiddlewareAction, RequestMiddleware, ResponseMiddleware};
use crate::api::types::GatewayError;

/// Router trait — dispatches a request to produce a response.
///
/// Generic over `Req`, `Resp`, `Err`.
#[async_trait]
pub trait Router<
    Req: Send + Sync + 'static = serde_json::Value,
    Resp: Send + Sync + 'static = serde_json::Value,
    Err: Send + Sync + 'static = GatewayError,
>: Send + Sync {
    async fn dispatch(&self, request: &Req) -> Result<Resp, Err>;
}


/// Async closure-based router.
///
/// Use this when your dispatch logic is async — e.g. calling an external service.
/// The handler captures whatever dependencies it needs at construction time,
/// keeping the router free of domain knowledge.
///
/// # Example
///
/// ```rust,ignore
/// let mgmt = Arc::clone(&mgmt);
/// PipelineRouter::new(move |req: &MyReq| {
///     let mgmt = Arc::clone(&mgmt);
///     let input = req.sanitized.clone();
///     Box::pin(async move { mgmt.process(&input).await.map_err(into_err) })
/// })
/// ```
pub struct PipelineRouter<F, Req = serde_json::Value, Resp = serde_json::Value, Err = GatewayError>
where
    F: for<'a> Fn(&'a Req) -> BoxFuture<'a, Result<Resp, Err>> + Send + Sync,
{
    handler: F,
    _phantom: std::marker::PhantomData<(Req, Resp, Err)>,
}

impl<F, Req, Resp, Err> PipelineRouter<F, Req, Resp, Err>
where
    F: for<'a> Fn(&'a Req) -> BoxFuture<'a, Result<Resp, Err>> + Send + Sync,
{
    pub fn new(handler: F) -> Self {
        Self { handler, _phantom: std::marker::PhantomData }
    }
}

#[async_trait]
impl<F, Req, Resp, Err> Router<Req, Resp, Err> for PipelineRouter<F, Req, Resp, Err>
where
    F: for<'a> Fn(&'a Req) -> BoxFuture<'a, Result<Resp, Err>> + Send + Sync,
    Req: Send + Sync + 'static,
    Resp: Send + Sync + 'static,
    Err: Send + Sync + 'static,
{
    async fn dispatch(&self, request: &Req) -> Result<Resp, Err> {
        (self.handler)(request).await
    }
}

// ============================================================================
// Pipeline trait (API)
// ============================================================================

/// Pipeline — executes a request through an ordered chain of stages.
///
/// Generic over `Req`, `Resp`, `Err`.
///
/// The default implementation is [`DefaultPipeline`], which composes
/// pre-middleware, a router, and post-middleware with short-circuit support.
/// Implement this trait directly for custom execution strategies (e.g. metered,
/// cached, or fan-out pipelines).
#[async_trait]
pub trait Pipeline<
    Req: Send + Sync + 'static = serde_json::Value,
    Resp: Send + Sync + 'static = serde_json::Value,
    Err: Send + Sync + 'static = GatewayError,
>: Send + Sync {
    async fn execute(&self, request: Req) -> Result<Resp, Err>;
}

// ============================================================================
// DefaultPipeline (Core)
// ============================================================================

/// Standard pre → route → post pipeline.
///
/// # Short-Circuit
///
/// Pre-middleware may return [`MiddlewareAction::ShortCircuit`] from
/// [`RequestMiddleware::process_request_action`] to skip remaining
/// pre-middleware and the router. Post-middleware still runs on the
/// short-circuited response (useful for logging, metrics, headers, etc.).
pub struct DefaultPipeline<
    Req: Send + Sync + 'static = serde_json::Value,
    Resp: Send + Sync + 'static = serde_json::Value,
    Err: Send + Sync + 'static = GatewayError,
> {
    pre: Vec<Arc<dyn RequestMiddleware<Req, Err, Resp>>>,
    router: Arc<dyn Router<Req, Resp, Err>>,
    post: Vec<Arc<dyn ResponseMiddleware<Resp, Err>>>,
}

impl<Req: Send + Sync + 'static, Resp: Send + Sync + 'static, Err: Send + Sync + 'static>
    DefaultPipeline<Req, Resp, Err>
{
    pub fn new(
        pre: Vec<Arc<dyn RequestMiddleware<Req, Err, Resp>>>,
        router: Arc<dyn Router<Req, Resp, Err>>,
        post: Vec<Arc<dyn ResponseMiddleware<Resp, Err>>>,
    ) -> Self {
        Self { pre, router, post }
    }

    pub fn pre_middleware_count(&self) -> usize { self.pre.len() }
    pub fn post_middleware_count(&self) -> usize { self.post.len() }
}

#[async_trait]
impl<Req: Send + Sync + 'static, Resp: Send + Sync + 'static, Err: Send + Sync + 'static>
    Pipeline<Req, Resp, Err> for DefaultPipeline<Req, Resp, Err>
{
    async fn execute(&self, request: Req) -> Result<Resp, Err> {
        // Run pre-middleware with short-circuit support.
        let mut state: MiddlewareAction<Req, Resp> = MiddlewareAction::Continue(request);

        for mw in &self.pre {
            match state {
                MiddlewareAction::Continue(req) => {
                    state = mw.process_request_action(req).await?;
                }
                MiddlewareAction::ShortCircuit(_) => break,
            }
        }

        // If short-circuited, skip the router; otherwise dispatch normally.
        let mut response = match state {
            MiddlewareAction::ShortCircuit(resp) => resp,
            MiddlewareAction::Continue(req) => self.router.dispatch(&req).await?,
        };

        // Post-middleware always runs, even on short-circuited responses.
        for mw in &self.post {
            response = mw.process_response(response).await?;
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::GatewayResult;

    // === Default types ===

    struct EchoRouter;

    #[async_trait]
    impl Router for EchoRouter {
        async fn dispatch(&self, request: &serde_json::Value) -> GatewayResult<serde_json::Value> {
            Ok(request.clone())
        }
    }

    #[tokio::test]
    async fn test_default_pipeline() {
        let pipeline = DefaultPipeline::new(vec![], Arc::new(EchoRouter), vec![]);
        let input = serde_json::json!({"model": "gpt-4"});
        let output = pipeline.execute(input.clone()).await.unwrap();
        assert_eq!(input, output);
    }

    #[tokio::test]
    async fn test_async_closure_router() {
        let router = Arc::new(PipelineRouter::new(|req: &serde_json::Value| {
            let val = req.clone();
            Box::pin(async move { Ok(val) })
        }));
        let pipeline = DefaultPipeline::new(vec![], router as Arc<dyn Router>, vec![]);
        let output = pipeline.execute(serde_json::json!({"x": 1})).await.unwrap();
        assert_eq!(output["x"], 1);
    }

    // === Custom types + custom error ===

    #[derive(Debug, Clone)]
    struct Req { model: String }

    #[derive(Debug, Clone)]
    struct Resp { content: String }

    #[derive(Debug)]
    struct Err(String);

    struct TypedRouter;

    #[async_trait]
    impl Router<Req, Resp, Err> for TypedRouter {
        async fn dispatch(&self, request: &Req) -> Result<Resp, Err> {
            Ok(Resp { content: format!("hello from {}", request.model) })
        }
    }

    struct TypedPre;

    #[async_trait]
    impl RequestMiddleware<Req, Err, Resp> for TypedPre {
        async fn process_request(&self, mut request: Req) -> Result<Req, Err> {
            request.model = format!("pre_{}", request.model);
            Ok(request)
        }
    }

    struct TypedPost;

    #[async_trait]
    impl ResponseMiddleware<Resp, Err> for TypedPost {
        async fn process_response(&self, mut response: Resp) -> Result<Resp, Err> {
            response.content = format!("{}_post", response.content);
            Ok(response)
        }
    }

    #[tokio::test]
    async fn test_typed_pipeline_with_custom_error() {
        let pre: Arc<dyn RequestMiddleware<Req, Err, Resp>> = Arc::new(TypedPre);
        let post: Arc<dyn ResponseMiddleware<Resp, Err>> = Arc::new(TypedPost);
        let router: Arc<dyn Router<Req, Resp, Err>> = Arc::new(TypedRouter);
        let pipeline = DefaultPipeline::new(vec![pre], router, vec![post]);

        let output = pipeline.execute(Req { model: "gpt-4".into() }).await.unwrap();
        assert_eq!(output.content, "hello from pre_gpt-4_post");
    }
}
