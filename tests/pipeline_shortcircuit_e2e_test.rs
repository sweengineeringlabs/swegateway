//! End-to-end tests for pipeline middleware short-circuit support (BL-008).
//!
//! Verifies that:
//! - Middleware returning `Continue` passes requests through normally.
//! - Middleware returning `ShortCircuit` skips remaining pre-middleware and router.
//! - Post-middleware still runs on short-circuited responses.
//! - Existing pipeline behavior is unchanged when no middleware short-circuits.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use swe_gateway::saf::{
    ClosureRouter, MiddlewareAction, Pipeline, RequestMiddleware,
    ResponseMiddleware, Router,
};

// ── Shared test types ────────────────────────────────────────────────────────

type Req = serde_json::Value;
type Resp = serde_json::Value;

/// Error type for tests — wraps a string message.
#[derive(Debug)]
struct TestError(String);

// ── Test middleware: continue (passthrough with call counter) ─────────────────

struct CountingPassthrough {
    call_count: AtomicUsize,
    label: &'static str,
}

impl CountingPassthrough {
    fn new(label: &'static str) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            label,
        }
    }

    fn call_count(&self) -> usize {
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

// ── Test middleware: short-circuit (returns early response) ───────────────────

struct ShortCircuitMiddleware {
    call_count: AtomicUsize,
    response: Resp,
}

impl ShortCircuitMiddleware {
    fn new(response: Resp) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            response,
        }
    }

    fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl RequestMiddleware<Req, TestError, Resp> for ShortCircuitMiddleware {
    async fn process_request(&self, _request: Req) -> Result<Req, TestError> {
        unreachable!(
            "process_request should not be called when process_request_action is overridden"
        );
    }

    async fn process_request_action(
        &self,
        _request: Req,
    ) -> Result<MiddlewareAction<Req, Resp>, TestError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(MiddlewareAction::ShortCircuit(self.response.clone()))
    }
}

// ── Test post-middleware: stamps response with a marker ──────────────────────

struct StampingPostMiddleware {
    call_count: AtomicUsize,
    marker: &'static str,
}

impl StampingPostMiddleware {
    fn new(marker: &'static str) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            marker,
        }
    }

    fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ResponseMiddleware<Resp, TestError> for StampingPostMiddleware {
    async fn process_response(&self, mut response: Resp) -> Result<Resp, TestError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        response["post_marker"] = serde_json::json!(self.marker);
        Ok(response)
    }
}

// ── Test router: echo with marker ────────────────────────────────────────────

struct EchoRouter {
    call_count: AtomicUsize,
}

impl EchoRouter {
    fn new() -> Self {
        Self {
            call_count: AtomicUsize::new(0),
        }
    }

    fn call_count(&self) -> usize {
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

// ── Helper: build pipeline ───────────────────────────────────────────────────

fn make_pipeline(
    pre: Vec<Arc<dyn RequestMiddleware<Req, TestError, Resp>>>,
    router: Arc<dyn Router<Req, Resp, TestError>>,
    post: Vec<Arc<dyn ResponseMiddleware<Resp, TestError>>>,
) -> Pipeline<Req, Resp, TestError> {
    Pipeline::new(pre, router, post)
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

/// Continue middleware passes request through to router.
#[tokio::test]
async fn test_execute_continue_middleware_passes_request_to_router() {
    let mw = Arc::new(CountingPassthrough::new("pre1"));
    let router = Arc::new(EchoRouter::new());
    let pipeline = make_pipeline(vec![mw.clone()], router.clone(), vec![]);

    let input = serde_json::json!({"model": "gpt-4"});
    let output = pipeline.execute(input).await.unwrap();

    assert_eq!(mw.call_count(), 1, "pre-middleware should be called once");
    assert_eq!(router.call_count(), 1, "router should be called once");
    assert_eq!(output["routed"], true, "response should come from router");
    assert_eq!(
        output["trail"], ">pre1",
        "request should carry pre-middleware stamp"
    );
}

/// Short-circuit middleware returns early response, skips router.
#[tokio::test]
async fn test_execute_shortcircuit_returns_early_response_skips_router() {
    let cached_response = serde_json::json!({"cached": true, "data": "from-cache"});
    let mw = Arc::new(ShortCircuitMiddleware::new(cached_response.clone()));
    let router = Arc::new(EchoRouter::new());
    let pipeline = make_pipeline(vec![mw.clone()], router.clone(), vec![]);

    let input = serde_json::json!({"model": "gpt-4"});
    let output = pipeline.execute(input).await.unwrap();

    assert_eq!(mw.call_count(), 1, "short-circuit middleware should be called");
    assert_eq!(router.call_count(), 0, "router must NOT be called on short-circuit");
    assert_eq!(output["cached"], true, "output should be the short-circuit response");
    assert_eq!(output["data"], "from-cache");
}

/// Short-circuit skips subsequent pre-middleware.
#[tokio::test]
async fn test_execute_shortcircuit_skips_subsequent_pre_middleware() {
    let pre1 = Arc::new(CountingPassthrough::new("pre1"));
    let short = Arc::new(ShortCircuitMiddleware::new(
        serde_json::json!({"short": true}),
    ));
    let pre3 = Arc::new(CountingPassthrough::new("pre3"));
    let router = Arc::new(EchoRouter::new());

    let pipeline = make_pipeline(
        vec![
            pre1.clone() as Arc<dyn RequestMiddleware<Req, TestError, Resp>>,
            short.clone(),
            pre3.clone(),
        ],
        router.clone(),
        vec![],
    );

    let output = pipeline.execute(serde_json::json!({})).await.unwrap();

    assert_eq!(pre1.call_count(), 1, "pre1 runs before short-circuit");
    assert_eq!(short.call_count(), 1, "short-circuit middleware runs");
    assert_eq!(pre3.call_count(), 0, "pre3 must NOT run after short-circuit");
    assert_eq!(router.call_count(), 0, "router must NOT run after short-circuit");
    assert_eq!(output["short"], true);
}

/// Post-middleware still runs on short-circuited response.
#[tokio::test]
async fn test_execute_shortcircuit_post_middleware_still_runs() {
    let short = Arc::new(ShortCircuitMiddleware::new(
        serde_json::json!({"origin": "cache"}),
    ));
    let post = Arc::new(StampingPostMiddleware::new("stamped"));
    let router = Arc::new(EchoRouter::new());

    let pipeline = make_pipeline(
        vec![short.clone()],
        router.clone(),
        vec![post.clone() as Arc<dyn ResponseMiddleware<Resp, TestError>>],
    );

    let output = pipeline.execute(serde_json::json!({})).await.unwrap();

    assert_eq!(post.call_count(), 1, "post-middleware must run on short-circuited response");
    assert_eq!(output["origin"], "cache", "original short-circuit data preserved");
    assert_eq!(
        output["post_marker"], "stamped",
        "post-middleware must have stamped the response"
    );
}

/// No short-circuit: existing pipeline behavior unchanged.
#[tokio::test]
async fn test_execute_no_shortcircuit_preserves_existing_behavior() {
    let pre1 = Arc::new(CountingPassthrough::new("A"));
    let pre2 = Arc::new(CountingPassthrough::new("B"));
    let post = Arc::new(StampingPostMiddleware::new("done"));
    let router = Arc::new(EchoRouter::new());

    let pipeline = make_pipeline(
        vec![
            pre1.clone() as Arc<dyn RequestMiddleware<Req, TestError, Resp>>,
            pre2.clone(),
        ],
        router.clone(),
        vec![post.clone() as Arc<dyn ResponseMiddleware<Resp, TestError>>],
    );

    let output = pipeline
        .execute(serde_json::json!({"trail": ""}))
        .await
        .unwrap();

    assert_eq!(pre1.call_count(), 1);
    assert_eq!(pre2.call_count(), 1);
    assert_eq!(router.call_count(), 1);
    assert_eq!(post.call_count(), 1);
    assert_eq!(output["trail"], ">A>B", "middleware executed in order");
    assert_eq!(output["routed"], true, "router executed");
    assert_eq!(output["post_marker"], "done", "post-middleware executed");
}

/// Default-typed pipeline with short-circuit.
#[tokio::test]
async fn test_execute_shortcircuit_with_default_types() {
    use swe_gateway::saf::GatewayError;

    struct CacheHit;

    #[async_trait]
    impl RequestMiddleware for CacheHit {
        async fn process_request(
            &self,
            _request: serde_json::Value,
        ) -> Result<serde_json::Value, GatewayError> {
            unreachable!("should not be called");
        }

        async fn process_request_action(
            &self,
            _request: serde_json::Value,
        ) -> Result<MiddlewareAction<serde_json::Value, serde_json::Value>, GatewayError> {
            Ok(MiddlewareAction::ShortCircuit(
                serde_json::json!({"hit": true}),
            ))
        }
    }

    let router: Arc<dyn Router> = Arc::new(ClosureRouter::new(|_req: &serde_json::Value| {
        Ok(serde_json::json!({"hit": false}))
    }));

    let pipeline = Pipeline::new(
        vec![Arc::new(CacheHit) as Arc<dyn RequestMiddleware>],
        router,
        vec![],
    );

    let output = pipeline.execute(serde_json::json!({})).await.unwrap();
    assert_eq!(output["hit"], true, "short-circuit response used, not router");
}

/// Short-circuit middleware error propagates.
#[tokio::test]
async fn test_execute_shortcircuit_middleware_error_propagates() {
    struct ErrorMiddleware;

    #[async_trait]
    impl RequestMiddleware<Req, TestError, Resp> for ErrorMiddleware {
        async fn process_request(&self, _request: Req) -> Result<Req, TestError> {
            unreachable!();
        }

        async fn process_request_action(
            &self,
            _request: Req,
        ) -> Result<MiddlewareAction<Req, Resp>, TestError> {
            Err(TestError("auth failed".into()))
        }
    }

    let router = Arc::new(EchoRouter::new());
    let pipeline = make_pipeline(
        vec![Arc::new(ErrorMiddleware)],
        router.clone(),
        vec![],
    );

    let result = pipeline.execute(serde_json::json!({})).await;
    assert!(result.is_err(), "error from short-circuit middleware should propagate");
    assert_eq!(router.call_count(), 0, "router should not run on error");
}

/// Empty pipeline routes directly.
#[tokio::test]
async fn test_execute_empty_pre_middleware_routes_directly() {
    let router = Arc::new(EchoRouter::new());
    let pipeline: Pipeline<Req, Resp, TestError> =
        Pipeline::new(vec![], router.clone(), vec![]);

    let output = pipeline
        .execute(serde_json::json!({"direct": true}))
        .await
        .unwrap();

    assert_eq!(router.call_count(), 1);
    assert_eq!(output["direct"], true);
    assert_eq!(output["routed"], true);
}
