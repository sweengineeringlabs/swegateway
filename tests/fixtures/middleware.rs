//! Reusable mock middleware and routers for pipeline tests.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use swe_gateway::saf::{
    ClosureRouter, GatewayError, MiddlewareAction, Pipeline, RequestMiddleware,
    ResponseMiddleware, Router,
};

type Req = serde_json::Value;
type Resp = serde_json::Value;

// =============================================================================
// Test Error
// =============================================================================

/// Lightweight error type for pipeline tests.
#[derive(Debug)]
pub struct TestError(pub String);

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for TestError {}

// =============================================================================
// Counting Passthrough Middleware
// =============================================================================

/// Pre-middleware that counts calls and stamps a trail on the request.
pub struct CountingPassthrough {
    call_count: AtomicUsize,
    label: String,
}

impl CountingPassthrough {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            label: label.into(),
        }
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl RequestMiddleware<Req, TestError, Resp> for CountingPassthrough {
    async fn process_request(&self, mut request: Req) -> Result<Req, TestError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let trail = request
            .get("trail")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        request["trail"] = serde_json::json!(format!("{trail}>{}", self.label));
        Ok(request)
    }
}

/// Pre-middleware that counts calls and passes through with default types.
pub struct DefaultPassthrough {
    call_count: AtomicUsize,
}

impl DefaultPassthrough {
    pub fn new() -> Self {
        Self {
            call_count: AtomicUsize::new(0),
        }
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl RequestMiddleware for DefaultPassthrough {
    async fn process_request(
        &self,
        request: serde_json::Value,
    ) -> Result<serde_json::Value, GatewayError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(request)
    }
}

// =============================================================================
// Short-Circuit Middleware
// =============================================================================

/// Pre-middleware that short-circuits with a fixed response.
pub struct ShortCircuitMiddleware {
    call_count: AtomicUsize,
    response: Resp,
}

impl ShortCircuitMiddleware {
    pub fn new(response: Resp) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            response,
        }
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl RequestMiddleware<Req, TestError, Resp> for ShortCircuitMiddleware {
    async fn process_request(&self, _request: Req) -> Result<Req, TestError> {
        unreachable!("process_request should not be called when process_request_action is overridden");
    }

    async fn process_request_action(
        &self,
        _request: Req,
    ) -> Result<MiddlewareAction<Req, Resp>, TestError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(MiddlewareAction::ShortCircuit(self.response.clone()))
    }
}

// =============================================================================
// Stamping Post-Middleware
// =============================================================================

/// Post-middleware that stamps a marker field on the response.
pub struct StampingPost {
    call_count: AtomicUsize,
    marker: String,
}

impl StampingPost {
    pub fn new(marker: impl Into<String>) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            marker: marker.into(),
        }
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ResponseMiddleware<Resp, TestError> for StampingPost {
    async fn process_response(&self, mut response: Resp) -> Result<Resp, TestError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        response["post_marker"] = serde_json::json!(&self.marker);
        Ok(response)
    }
}

// =============================================================================
// Echo Router
// =============================================================================

/// Router that echoes the request as the response with a `"routed": true` flag.
pub struct EchoRouter {
    call_count: AtomicUsize,
}

impl EchoRouter {
    pub fn new() -> Self {
        Self {
            call_count: AtomicUsize::new(0),
        }
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Router<Req, Resp, TestError> for EchoRouter {
    async fn dispatch(&self, request: &Req) -> Result<Resp, TestError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let mut resp = request.clone();
        resp["routed"] = serde_json::json!(true);
        Ok(resp)
    }
}

// =============================================================================
// Pipeline Builder Helper
// =============================================================================

/// Build a typed pipeline with TestError.
pub fn pipeline(
    pre: Vec<Arc<dyn RequestMiddleware<Req, TestError, Resp>>>,
    router: Arc<dyn Router<Req, Resp, TestError>>,
    post: Vec<Arc<dyn ResponseMiddleware<Resp, TestError>>>,
) -> Pipeline<Req, Resp, TestError> {
    Pipeline::new(pre, router, post)
}

/// Build a default-typed pipeline (serde_json::Value, GatewayError).
pub fn default_pipeline(
    pre: Vec<Arc<dyn RequestMiddleware>>,
    router: Arc<dyn Router>,
    post: Vec<Arc<dyn ResponseMiddleware>>,
) -> Pipeline {
    Pipeline::new(pre, router, post)
}

/// Create a closure-based echo router for default-typed pipelines.
pub fn echo_closure_router() -> Arc<dyn Router> {
    Arc::new(ClosureRouter::new(|req: &serde_json::Value| {
        let mut resp = req.clone();
        resp["routed"] = serde_json::json!(true);
        Ok(resp)
    }))
}
