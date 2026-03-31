//! Edge-case tests for pipeline middleware behaviour.
//!
//! Validates non-happy-path scenarios:
//! - Zero middleware (empty pipeline)
//! - Many chained pre-middleware (ordering guarantee)
//! - Request-transforming middleware
//! - Multiple short-circuits (only first takes effect)
//! - Short-circuit in last position
//! - Post-middleware error after short-circuit
//! - MetricsResponseMiddleware integration in a pipeline
//! - Nested pipelines (pipeline-as-router)
//! - Router error propagation through post-middleware

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use swe_gateway::saf::{
    ClosureRouter, FieldExtractor, MetricFields,
    MetricsCollector, MetricsResponseMiddleware, MiddlewareAction, Pipeline,
    RequestMiddleware, ResponseMiddleware, Router,
};

// ── Shared types ─────────────────────────────────────────────────────────────

type Req = serde_json::Value;
type Resp = serde_json::Value;

#[derive(Debug)]
struct TestError(String);

// ── Reusable test components ─────────────────────────────────────────────────

struct CountingPassthrough {
    call_count: AtomicUsize,
    label: String,
}

impl CountingPassthrough {
    fn new(label: impl Into<String>) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            label: label.into(),
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

struct StampingPostMiddleware {
    call_count: AtomicUsize,
    marker: String,
}

impl StampingPostMiddleware {
    fn new(marker: impl Into<String>) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            marker: marker.into(),
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
        response["post_marker"] = serde_json::json!(&self.marker);
        Ok(response)
    }
}

fn make_pipeline(
    pre: Vec<Arc<dyn RequestMiddleware<Req, TestError, Resp>>>,
    router: Arc<dyn Router<Req, Resp, TestError>>,
    post: Vec<Arc<dyn ResponseMiddleware<Resp, TestError>>>,
) -> Pipeline<Req, Resp, TestError> {
    Pipeline::new(pre, router, post)
}

// ===========================================================================
// 1. Pipeline with zero pre-middleware and zero post-middleware
// ===========================================================================

#[tokio::test]
async fn test_execute_zero_middleware_routes_directly_to_router() {
    let router = Arc::new(EchoRouter::new());
    let pipeline: Pipeline<Req, Resp, TestError> =
        Pipeline::new(vec![], router.clone(), vec![]);

    assert_eq!(pipeline.pre_middleware_count(), 0);
    assert_eq!(pipeline.post_middleware_count(), 0);

    let input = serde_json::json!({"key": "value"});
    let output = pipeline.execute(input.clone()).await.unwrap();

    assert_eq!(router.call_count(), 1, "router should be called exactly once");
    assert_eq!(output["key"], "value", "request data should pass through");
    assert_eq!(output["routed"], true, "response should be stamped by router");
}

// ===========================================================================
// 2. Pipeline with 20 pre-middleware chained — all execute in order
// ===========================================================================

#[tokio::test]
async fn test_execute_20_pre_middleware_all_execute_in_order() {
    let middlewares: Vec<Arc<dyn RequestMiddleware<Req, TestError, Resp>>> = (0..20)
        .map(|i| {
            Arc::new(CountingPassthrough::new(format!("{i}")))
                as Arc<dyn RequestMiddleware<Req, TestError, Resp>>
        })
        .collect();

    let router = Arc::new(EchoRouter::new());
    let pipeline = make_pipeline(middlewares.clone(), router.clone(), vec![]);

    assert_eq!(pipeline.pre_middleware_count(), 20);

    let output = pipeline
        .execute(serde_json::json!({"trail": ""}))
        .await
        .unwrap();

    assert_eq!(router.call_count(), 1, "router should be called once");

    // Verify all 20 middleware executed by checking call counts.
    for (i, mw) in middlewares.iter().enumerate() {
        // Downcast not possible through trait object, but we can verify via the trail.
        let _ = (i, mw); // used for iteration index
    }

    // The trail should be ">0>1>2>...>19".
    let expected_trail: String = (0..20).map(|i| format!(">{i}")).collect();
    assert_eq!(
        output["trail"].as_str().unwrap(),
        expected_trail,
        "trail should show all 20 middleware executed in order"
    );
    assert_eq!(output["routed"], true);
}

// ===========================================================================
// 3. Middleware that modifies request significantly (type transformation)
// ===========================================================================

struct RequestTransformer;

#[async_trait]
impl RequestMiddleware<Req, TestError, Resp> for RequestTransformer {
    async fn process_request(&self, _request: Req) -> Result<Req, TestError> {
        // Completely replace the request with a different structure.
        Ok(serde_json::json!({
            "transformed": true,
            "original_discarded": true,
            "new_field": "injected_by_middleware"
        }))
    }
}

#[tokio::test]
async fn test_execute_middleware_replaces_request_entirely() {
    let router = Arc::new(EchoRouter::new());
    let pipeline = make_pipeline(
        vec![Arc::new(RequestTransformer) as Arc<dyn RequestMiddleware<Req, TestError, Resp>>],
        router.clone(),
        vec![],
    );

    let original = serde_json::json!({"model": "gpt-4", "prompt": "hello"});
    let output = pipeline.execute(original).await.unwrap();

    // Router receives the transformed request, not the original.
    assert_eq!(output["transformed"], true, "request should be replaced by middleware");
    assert_eq!(
        output["new_field"], "injected_by_middleware",
        "new field should be present from middleware"
    );
    assert!(
        output.get("model").is_none(),
        "original 'model' field should be gone after transformation"
    );
    assert!(
        output.get("prompt").is_none(),
        "original 'prompt' field should be gone after transformation"
    );
    assert_eq!(output["routed"], true, "router should have processed the request");
}

// ===========================================================================
// 4. Multiple short-circuits — only first one takes effect
// ===========================================================================

#[tokio::test]
async fn test_execute_multiple_shortcircuits_only_first_takes_effect() {
    let sc1 = Arc::new(ShortCircuitMiddleware::new(
        serde_json::json!({"source": "first_cache"}),
    ));
    let sc2 = Arc::new(ShortCircuitMiddleware::new(
        serde_json::json!({"source": "second_cache"}),
    ));
    let router = Arc::new(EchoRouter::new());

    let pipeline = make_pipeline(
        vec![
            sc1.clone() as Arc<dyn RequestMiddleware<Req, TestError, Resp>>,
            sc2.clone(),
        ],
        router.clone(),
        vec![],
    );

    let output = pipeline.execute(serde_json::json!({})).await.unwrap();

    assert_eq!(sc1.call_count(), 1, "first short-circuit should execute");
    assert_eq!(sc2.call_count(), 0, "second short-circuit should NOT execute");
    assert_eq!(router.call_count(), 0, "router should NOT execute");
    assert_eq!(
        output["source"], "first_cache",
        "response should come from the first short-circuit, not the second"
    );
}

// ===========================================================================
// 5. Short-circuit in last pre-middleware — router still skipped
// ===========================================================================

#[tokio::test]
async fn test_execute_shortcircuit_in_last_pre_middleware_skips_router() {
    let pass1 = Arc::new(CountingPassthrough::new("A"));
    let pass2 = Arc::new(CountingPassthrough::new("B"));
    let sc_last = Arc::new(ShortCircuitMiddleware::new(
        serde_json::json!({"source": "last_middleware_cache"}),
    ));
    let router = Arc::new(EchoRouter::new());

    let pipeline = make_pipeline(
        vec![
            pass1.clone() as Arc<dyn RequestMiddleware<Req, TestError, Resp>>,
            pass2.clone(),
            sc_last.clone(),
        ],
        router.clone(),
        vec![],
    );

    let output = pipeline.execute(serde_json::json!({"trail": ""})).await.unwrap();

    assert_eq!(pass1.call_count(), 1, "first passthrough should execute");
    assert_eq!(pass2.call_count(), 1, "second passthrough should execute");
    assert_eq!(sc_last.call_count(), 1, "last (short-circuit) should execute");
    assert_eq!(router.call_count(), 0, "router should NOT execute even when short-circuit is last");
    assert_eq!(output["source"], "last_middleware_cache");
}

// ===========================================================================
// 6. Post-middleware error after short-circuit — error propagates
// ===========================================================================

struct FailingPostMiddleware;

#[async_trait]
impl ResponseMiddleware<Resp, TestError> for FailingPostMiddleware {
    async fn process_response(&self, _response: Resp) -> Result<Resp, TestError> {
        Err(TestError("post-middleware exploded".into()))
    }
}

#[tokio::test]
async fn test_execute_post_middleware_error_after_shortcircuit_propagates() {
    let sc = Arc::new(ShortCircuitMiddleware::new(
        serde_json::json!({"cached": true}),
    ));
    let router = Arc::new(EchoRouter::new());
    let failing_post = Arc::new(FailingPostMiddleware);

    let pipeline = make_pipeline(
        vec![sc.clone() as Arc<dyn RequestMiddleware<Req, TestError, Resp>>],
        router.clone(),
        vec![failing_post as Arc<dyn ResponseMiddleware<Resp, TestError>>],
    );

    let result = pipeline.execute(serde_json::json!({})).await;

    assert!(
        result.is_err(),
        "pipeline should return Err when post-middleware fails after short-circuit"
    );
    let err = result.unwrap_err();
    assert_eq!(err.0, "post-middleware exploded");
    assert_eq!(sc.call_count(), 1, "short-circuit should have executed");
    assert_eq!(router.call_count(), 0, "router should not have executed");
}

// ===========================================================================
// 7. MetricsResponseMiddleware integration in a pipeline
// ===========================================================================

struct InMemoryCollector {
    events: parking_lot::Mutex<Vec<MetricFields>>,
}

impl InMemoryCollector {
    fn new() -> Self {
        Self {
            events: parking_lot::Mutex::new(Vec::new()),
        }
    }

    fn recorded_events(&self) -> Vec<MetricFields> {
        self.events.lock().clone()
    }
}

impl MetricsCollector for InMemoryCollector {
    fn record_completion(
        &self,
        provider: &str,
        model: &str,
        status: &str,
        latency_secs: f64,
        input_tokens: u64,
        output_tokens: u64,
    ) {
        self.events.lock().push(MetricFields {
            provider: provider.to_string(),
            model: model.to_string(),
            status: status.to_string(),
            latency_secs,
            input_tokens,
            output_tokens,
        });
    }
}

fn sample_extractor() -> FieldExtractor {
    Arc::new(|response: &serde_json::Value| {
        Some(MetricFields {
            provider: response["provider"].as_str()?.to_string(),
            model: response["model"].as_str()?.to_string(),
            status: "ok".to_string(),
            latency_secs: response["latency_ms"].as_f64()? / 1000.0,
            input_tokens: response["usage"]["prompt_tokens"].as_u64()?,
            output_tokens: response["usage"]["completion_tokens"].as_u64()?,
        })
    })
}

#[tokio::test]
async fn test_execute_metrics_middleware_records_after_router() {
    let collector = Arc::new(InMemoryCollector::new());
    let metrics_mw = Arc::new(MetricsResponseMiddleware::new(
        collector.clone(),
        sample_extractor(),
    ));

    // Router that returns a metrics-compatible response.
    let router: Arc<dyn Router> = Arc::new(ClosureRouter::new(
        |_req: &serde_json::Value| {
            Ok(serde_json::json!({
                "provider": "anthropic",
                "model": "claude-3",
                "latency_ms": 150,
                "usage": {
                    "prompt_tokens": 42,
                    "completion_tokens": 18
                }
            }))
        },
    ));

    let pipeline: Pipeline = Pipeline::new(
        vec![],
        router,
        vec![metrics_mw as Arc<dyn ResponseMiddleware>],
    );

    let output = pipeline.execute(serde_json::json!({})).await.unwrap();

    // Response passes through unchanged.
    assert_eq!(output["provider"], "anthropic");
    assert_eq!(output["model"], "claude-3");

    // Metrics recorded.
    let events = collector.recorded_events();
    assert_eq!(events.len(), 1, "one completion event should be recorded");
    assert_eq!(events[0].provider, "anthropic");
    assert_eq!(events[0].model, "claude-3");
    assert_eq!(events[0].input_tokens, 42);
    assert_eq!(events[0].output_tokens, 18);
    assert!((events[0].latency_secs - 0.15).abs() < f64::EPSILON);
}

#[tokio::test]
async fn test_execute_metrics_middleware_skips_when_extractor_returns_none() {
    let collector = Arc::new(InMemoryCollector::new());
    let metrics_mw = Arc::new(MetricsResponseMiddleware::new(
        collector.clone(),
        sample_extractor(),
    ));

    // Router returns a response missing required fields.
    let router: Arc<dyn Router> = Arc::new(ClosureRouter::new(
        |_req: &serde_json::Value| Ok(serde_json::json!({"status": "ok"})),
    ));

    let pipeline: Pipeline = Pipeline::new(
        vec![],
        router,
        vec![metrics_mw as Arc<dyn ResponseMiddleware>],
    );

    let output = pipeline.execute(serde_json::json!({})).await.unwrap();
    assert_eq!(output["status"], "ok", "response should pass through");
    assert!(
        collector.recorded_events().is_empty(),
        "no metrics should be recorded when extractor returns None"
    );
}

// ===========================================================================
// 8. Nested pipelines (pipeline as router for another pipeline)
// ===========================================================================

/// Adapter that wraps a Pipeline as a Router, enabling nesting.
struct PipelineRouter {
    inner: Pipeline<Req, Resp, TestError>,
}

impl PipelineRouter {
    fn new(inner: Pipeline<Req, Resp, TestError>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl Router<Req, Resp, TestError> for PipelineRouter {
    async fn dispatch(&self, request: &Req) -> Result<Resp, TestError> {
        self.inner.execute(request.clone()).await
    }
}

#[tokio::test]
async fn test_execute_nested_pipeline_as_router_executes_full_chain() {
    // Inner pipeline: adds "inner" marker.
    let inner_pre = Arc::new(CountingPassthrough::new("inner_pre"));
    let inner_router = Arc::new(EchoRouter::new());
    let inner_post = Arc::new(StampingPostMiddleware::new("inner_post"));

    let inner_pipeline = make_pipeline(
        vec![inner_pre.clone() as Arc<dyn RequestMiddleware<Req, TestError, Resp>>],
        inner_router.clone(),
        vec![inner_post.clone() as Arc<dyn ResponseMiddleware<Resp, TestError>>],
    );

    // Outer pipeline: wraps inner pipeline as router.
    let outer_pre = Arc::new(CountingPassthrough::new("outer_pre"));
    let outer_post = Arc::new(StampingPostMiddleware::new("outer_post"));
    let nested_router = Arc::new(PipelineRouter::new(inner_pipeline));

    let outer_pipeline = make_pipeline(
        vec![outer_pre.clone() as Arc<dyn RequestMiddleware<Req, TestError, Resp>>],
        nested_router,
        vec![outer_post.clone() as Arc<dyn ResponseMiddleware<Resp, TestError>>],
    );

    let output = outer_pipeline
        .execute(serde_json::json!({"trail": ""}))
        .await
        .unwrap();

    assert_eq!(outer_pre.call_count(), 1, "outer pre should execute");
    assert_eq!(inner_pre.call_count(), 1, "inner pre should execute");
    assert_eq!(inner_router.call_count(), 1, "inner router should execute");
    assert_eq!(inner_post.call_count(), 1, "inner post should execute");
    assert_eq!(outer_post.call_count(), 1, "outer post should execute");

    // Trail should show outer_pre then inner_pre.
    assert_eq!(
        output["trail"], ">outer_pre>inner_pre",
        "trail should reflect middleware from both pipelines in order"
    );
    assert_eq!(output["routed"], true, "inner router should have set routed=true");

    // The outer post-middleware runs last, so its marker overwrites inner's.
    assert_eq!(
        output["post_marker"], "outer_post",
        "outer post-middleware should be the last to stamp the response"
    );
}

#[tokio::test]
async fn test_execute_nested_pipeline_inner_shortcircuit_propagates() {
    // Inner pipeline short-circuits.
    let inner_sc = Arc::new(ShortCircuitMiddleware::new(
        serde_json::json!({"inner_cached": true}),
    ));
    let inner_router = Arc::new(EchoRouter::new());
    let inner_pipeline = make_pipeline(
        vec![inner_sc.clone() as Arc<dyn RequestMiddleware<Req, TestError, Resp>>],
        inner_router.clone(),
        vec![],
    );

    // Outer pipeline wraps it.
    let outer_post = Arc::new(StampingPostMiddleware::new("outer_stamp"));
    let nested_router = Arc::new(PipelineRouter::new(inner_pipeline));
    let outer_pipeline = make_pipeline(
        vec![],
        nested_router,
        vec![outer_post.clone() as Arc<dyn ResponseMiddleware<Resp, TestError>>],
    );

    let output = outer_pipeline.execute(serde_json::json!({})).await.unwrap();

    assert_eq!(inner_sc.call_count(), 1, "inner short-circuit should execute");
    assert_eq!(inner_router.call_count(), 0, "inner router should be skipped");
    assert_eq!(output["inner_cached"], true, "short-circuit response should propagate");
    assert_eq!(
        output["post_marker"], "outer_stamp",
        "outer post-middleware should still run on nested short-circuit response"
    );
}

// ===========================================================================
// 9. Error in router propagates through post-middleware
// ===========================================================================

struct FailingRouter;

#[async_trait]
impl Router<Req, Resp, TestError> for FailingRouter {
    async fn dispatch(&self, _request: &Req) -> Result<Resp, TestError> {
        Err(TestError("router_exploded".into()))
    }
}

#[tokio::test]
async fn test_execute_router_error_skips_post_middleware_and_propagates() {
    let post = Arc::new(StampingPostMiddleware::new("should_not_run"));
    let pipeline = make_pipeline(
        vec![],
        Arc::new(FailingRouter),
        vec![post.clone() as Arc<dyn ResponseMiddleware<Resp, TestError>>],
    );

    let result = pipeline.execute(serde_json::json!({})).await;

    assert!(result.is_err(), "router error should propagate");
    let err = result.unwrap_err();
    assert_eq!(err.0, "router_exploded", "error message should match");
    // Post-middleware should NOT run because the router returned Err and
    // Pipeline::execute returns early via `?` on the router result.
    assert_eq!(
        post.call_count(),
        0,
        "post-middleware should not run when router returns Err (the ? operator propagates)"
    );
}

#[tokio::test]
async fn test_execute_pre_middleware_error_skips_router_and_post_middleware() {
    struct FailingPreMiddleware;

    #[async_trait]
    impl RequestMiddleware<Req, TestError, Resp> for FailingPreMiddleware {
        async fn process_request(&self, _request: Req) -> Result<Req, TestError> {
            Err(TestError("pre_middleware_auth_failure".into()))
        }
    }

    let router = Arc::new(EchoRouter::new());
    let post = Arc::new(StampingPostMiddleware::new("post"));

    let pipeline = make_pipeline(
        vec![Arc::new(FailingPreMiddleware) as Arc<dyn RequestMiddleware<Req, TestError, Resp>>],
        router.clone(),
        vec![post.clone() as Arc<dyn ResponseMiddleware<Resp, TestError>>],
    );

    let result = pipeline.execute(serde_json::json!({})).await;

    assert!(result.is_err(), "pre-middleware error should propagate");
    assert_eq!(result.unwrap_err().0, "pre_middleware_auth_failure");
    assert_eq!(router.call_count(), 0, "router should not execute after pre-middleware error");
    assert_eq!(post.call_count(), 0, "post-middleware should not execute after pre-middleware error");
}
