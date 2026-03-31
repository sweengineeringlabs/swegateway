# Integrating swe-gateway with a Consumer Application

This guide walks through everything needed to wire `swe-gateway` into a real application — from adding the dependency to running requests through a full middleware pipeline.

---

## What swe-gateway gives you

`swe-gateway` is an infrastructure library. It gives you:

- A **typed request/response envelope** (`InputRequest<Req>` / `OutputResponse<Resp>`)
- A **pipeline model** (pre-middleware → router → post-middleware) with short-circuit support
- A **`Gateway` trait** your app can depend on without knowing the implementation
- **Built-in middleware** (retry, rate-limiting)
- **Error types** with retryability classification and domain-error conversion
- **Health check** support

What it does *not* give you is business logic. Your app owns the router (what actually processes the request) and any domain-specific middleware.

---

## Dependency

In your `Cargo.toml`:

```toml
[dependencies]
swe-gateway = { version = "0.6.0", path = "../rustratify/crates/swe-gateway" }
async-trait = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

Import exclusively from the SAF module — never from internal paths:

```rust
use swe_gateway::saf::*;
```

---

## The three roles

Every swe-gateway integration involves three things:

| Role | Trait | Who writes it |
|------|-------|---------------|
| **API** | `Gateway<Req, Resp>` | swe-gateway (you depend on it) |
| **Pipeline** | `PipelineGateway<Req, Resp, PReq, PResp>` | You (your concrete gateway struct) |
| **SAF** | A builder / factory function | You (composition root) |

`Gateway` is the public contract — everything upstream depends on `Arc<dyn Gateway<Req, Resp>>`.

`PipelineGateway` is the implementor's helper — it wires the pipeline and provides `process_via_pipeline()` for free.

---

## Step 1 — Define your domain types

Define the request and response types your gateway speaks. Then mark them as pipeline-safe:

```rust
// Your domain request — whatever the upstream sends in
pub struct ChatRequest {
    pub messages: Vec<Message>,
    pub session_id: Option<String>,
}

// Your domain response — whatever the upstream gets back
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub tokens_used: u32,
}

// Pipeline-internal types (private to your gateway crate)
// These are what flow between middleware stages
struct PipeReq {
    messages: Vec<Message>,
    session_id: Option<String>,
    sanitized: bool,
}

struct PipeResp {
    content: String,
    model: String,
    tokens_used: u32,
}

// Mark them as safe to cross pipeline stage boundaries
impl PipelineReq for PipeReq {}
impl PipelineResp for PipeResp {}
```

`PipelineReq` / `PipelineResp` are marker traits — they have no methods. They exist to make the type system enforce that only your designated types flow through your pipeline, not arbitrary values.

---

## Step 2 — Write your middleware

Middleware transforms the request or response at a named stage. Each piece of middleware has one job.

### Request middleware (pre-stage)

```rust
use swe_gateway::saf::*;
use async_trait::async_trait;

struct AuthMiddleware {
    secret: String,
}

#[async_trait]
impl RequestMiddleware<PipeReq, GatewayError, PipeResp> for AuthMiddleware {
    async fn process_request(&self, req: PipeReq) -> GatewayResult<PipeReq> {
        // validate session, check token, etc.
        // return Err(...) to reject the request
        Ok(req)
    }
}
```

To **short-circuit** (skip the router and return early without an error — e.g. for cached responses):

```rust
#[async_trait]
impl RequestMiddleware<PipeReq, GatewayError, PipeResp> for CacheMiddleware {
    async fn process_request(&self, req: PipeReq) -> GatewayResult<PipeReq> {
        Ok(req) // default — no short-circuit from this method
    }

    async fn process_request_action(
        &self,
        req: PipeReq,
    ) -> GatewayResult<MiddlewareAction<PipeReq, PipeResp>> {
        if let Some(cached) = self.cache.get(&req.session_id) {
            // Skip router entirely — return cached response
            return Ok(MiddlewareAction::ShortCircuit(cached));
        }
        Ok(MiddlewareAction::Continue(req))
    }
}
```

### Response middleware (post-stage)

Post-middleware always runs — even when the pipeline was short-circuited. Use it for logging, metrics, and response enrichment.

```rust
struct LoggingMiddleware;

#[async_trait]
impl ResponseMiddleware<PipeResp, GatewayError> for LoggingMiddleware {
    async fn process_response(&self, resp: PipeResp) -> GatewayResult<PipeResp> {
        tracing::info!(tokens = resp.tokens_used, model = %resp.model, "request completed");
        Ok(resp)
    }
}
```

---

## Step 3 — Write the router

The router is the core of the pipeline — it does the actual work. It takes the pipeline request and returns a pipeline response.

```rust
use swe_gateway::saf::{PipelineRouter, GatewayError};
use futures::future::BoxFuture;
use std::sync::Arc;

fn build_router(llm: Arc<dyn LlmClient>) -> Arc<PipelineRouter<impl Fn(&PipeReq) -> BoxFuture<'_, GatewayResult<PipeResp>> + Send + Sync, PipeReq, PipeResp, GatewayError>> {
    Arc::new(PipelineRouter::new(move |req: &PipeReq| {
        let llm = Arc::clone(&llm);
        let messages = req.messages.clone();
        Box::pin(async move {
            let result = llm.complete(&messages).await
                .map_err(|e| GatewayError::internal(e.to_string()))?;
            Ok(PipeResp {
                content: result.text,
                model: result.model,
                tokens_used: result.tokens,
            })
        })
    }))
}
```

`PipelineRouter` takes a closure `for<'a> Fn(&'a Req) -> BoxFuture<'a, Result<Resp, Err>>`. The closure receives a reference to the request — clone anything you need before the async move block.

---

## Step 4 — Implement the gateway

Your gateway struct holds the pipeline. `PipelineGateway` tells the framework how to use it.

```rust
use swe_gateway::saf::*;
use std::sync::Arc;

pub(crate) struct DefaultChatGateway {
    pipeline: DefaultPipeline<PipeReq, PipeResp, GatewayError>,
}

impl StatefulProvider for DefaultChatGateway {}

impl DefaultChatGateway {
    pub(crate) fn new(llm: Arc<dyn LlmClient>) -> Self {
        let pipeline = DefaultPipeline::new(
            // pre-middleware (in order)
            vec![
                Arc::new(AuthMiddleware { secret: "...".into() }),
                Arc::new(CacheMiddleware::new()),
            ],
            // router
            build_router(llm),
            // post-middleware (in order, always runs)
            vec![
                Arc::new(LoggingMiddleware),
            ],
        );
        Self { pipeline }
    }
}

// Tell PipelineGateway how to use your pipeline
impl PipelineGateway<ChatRequest, ChatResponse, PipeReq, PipeResp> for DefaultChatGateway {
    fn pipeline(&self) -> &dyn Pipeline<PipeReq, PipeResp, GatewayError> {
        &self.pipeline
    }

    fn into_pipeline_req(request: InputRequest<ChatRequest>) -> PipeReq {
        PipeReq {
            messages: request.payload.messages,
            session_id: request.payload.session_id,
            sanitized: false,
        }
    }

    fn into_response(
        resp: PipeResp,
        request_id: String,
        duration_ms: u64,
    ) -> GatewayResult<OutputResponse<ChatResponse>> {
        Ok(OutputResponse::ok(request_id, duration_ms, ChatResponse {
            content: resp.content,
            model: resp.model,
            tokens_used: resp.tokens_used,
        }))
    }
}

// Implement Gateway by delegating to process_via_pipeline
#[async_trait]
impl Gateway<ChatRequest, ChatResponse> for DefaultChatGateway {
    async fn process(
        &self,
        request: InputRequest<ChatRequest>,
    ) -> GatewayResult<OutputResponse<ChatResponse>> {
        self.process_via_pipeline(request).await
    }

    async fn health_check(&self) -> GatewayResult<HealthCheck> {
        Ok(HealthCheck::healthy())
    }
}
```

---

## Step 5 — Build it (SAF / composition root)

The gateway struct is `pub(crate)`. The outside world never constructs it directly. Expose a builder or factory function:

```rust
// Public builder — the only way to create the gateway
pub struct ChatGatewayBuilder {
    llm: Arc<dyn LlmClient>,
}

impl ChatGatewayBuilder {
    pub fn new(llm: Arc<dyn LlmClient>) -> Self {
        Self { llm }
    }

    pub fn build(self) -> Arc<dyn Gateway<ChatRequest, ChatResponse>> {
        Arc::new(DefaultChatGateway::new(self.llm))
    }
}
```

In your application's composition root (e.g. `main.rs` or `bootstrap.rs`):

```rust
let llm: Arc<dyn LlmClient> = Arc::new(AnthropicClient::new(api_key));
let gateway: Arc<dyn Gateway<ChatRequest, ChatResponse>> =
    ChatGatewayBuilder::new(llm).build();
```

Everything upstream depends on `Arc<dyn Gateway<ChatRequest, ChatResponse>>` — never on `DefaultChatGateway`.

---

## Step 6 — Call the gateway

```rust
use swe_gateway::saf::{InputRequest, RequestMetadata};

// Simple call
let request = InputRequest::new(ChatRequest {
    messages: vec![Message::user("Hello")],
    session_id: None,
});

let resp = gateway.process(request).await?;
println!("{}", resp.payload.content);
println!("Took {}ms, status: {:?}", resp.metadata.duration_ms, resp.metadata.status);

// With session tracking
let request = InputRequest::with_metadata(
    ChatRequest { messages, session_id: Some(session_id.clone()) },
    RequestMetadata::new().with_session(session_id),
);
```

`process()` returns `GatewayResult<OutputResponse<ChatResponse>>`:
- `Ok(resp)` — `resp.payload` is your domain response, `resp.metadata` has `request_id`, `duration_ms`, `status`
- `Err(GatewayError)` — pipeline failed (middleware rejected, router errored, etc.)

---

## Error handling

Map your domain errors to `GatewayError` using `IntoGatewayError`:

```rust
#[derive(thiserror::Error, Debug)]
pub enum LlmError {
    #[error("rate limit: {0}")]
    RateLimit(String),
    #[error("model not found: {0}")]
    ModelNotFound(String),
    #[error("api error: {0}")]
    Api(String),
}

impl IntoGatewayError for LlmError {
    fn into_gateway_error(self) -> GatewayError {
        match self {
            LlmError::RateLimit(msg) => GatewayError::unavailable(msg),
            LlmError::ModelNotFound(msg) => GatewayError::not_found(msg),
            LlmError::Api(msg) => GatewayError::internal(msg),
        }
    }
}

// Then in your router:
llm.complete(&messages).await
    .map_err(|e| e.into_gateway_error())?;
```

Use the right error constructor for retryability to work correctly:

| Condition | Constructor | `is_retryable()` |
|-----------|-------------|-----------------|
| Unexpected failure | `GatewayError::internal(msg)` | false |
| Bad input | `GatewayError::invalid_input(msg)` | false |
| Not found | `GatewayError::not_found(msg)` | false |
| Rate limited | `GatewayError::unavailable(msg)` | **true** |
| Timeout | `GatewayError::timeout(msg)` | **true** |
| Service down | `GatewayError::unavailable(msg)` | **true** |
| Config error | `GatewayError::configuration(msg)` | false |

---

## Built-in middleware

### Retry

```rust
use swe_gateway::saf::{RetryMiddleware, BackoffStrategy};

let retry = RetryMiddleware::builder()
    .max_attempts(3)
    .exponential_backoff(std::time::Duration::from_millis(200), true)
    .retry_predicate(|e| e.is_retryable())
    .build_with(Arc::new(your_middleware));
```

Defaults: 3 attempts, exponential backoff starting at 200ms with jitter, retries on `is_retryable()` errors.

### Rate limiting

```rust
use swe_gateway::saf::rate_limiter;

let limiter = rate_limiter(); // 100 capacity, 10 tokens/sec

// Or customised:
use swe_gateway::saf::rate_limiter_builder;
let limiter = rate_limiter_builder()
    .capacity(50)
    .refill_rate(5.0)
    .build();
```

Both implement `RequestMiddleware` — add to your pre-middleware vec.

---

## Full data flow

```
Caller
  │
  ▼
InputRequest<ChatRequest>
  │
  ▼
Gateway::process()
  │
  ▼
PipelineGateway::process_via_pipeline()
  ├─ capture start time, extract request_id
  ├─ set MDC log context (session = request_id)
  ├─ into_pipeline_req() → PipeReq
  │
  ▼
DefaultPipeline::execute(PipeReq)
  ├─ pre[0]: AuthMiddleware       → Continue(PipeReq) or Err
  ├─ pre[1]: CacheMiddleware      → Continue(PipeReq) or ShortCircuit(PipeResp)
  ├─ router: PipelineRouter       → PipeResp or Err   (skipped on ShortCircuit)
  └─ post[0]: LoggingMiddleware   → PipeResp          (always runs)
  │
  ▼
into_response(PipeResp, request_id, duration_ms)
  │
  ▼
OutputResponse<ChatResponse>
  ├─ payload: ChatResponse
  └─ metadata: { request_id, duration_ms, status }
  │
  ▼
Caller
```

---

## llmboot — reference integration

`llmboot` is the canonical real-world consumer. Its gateway at `main/features/gateway/src/gateway.rs` demonstrates:

- **Two middleware adapters** wrapping `InputGuardrail` and `OutputGuardrail` (domain services that don't know about swe-gateway)
- **Private pipeline types** (`PipelineReq` / `PipelineResp`) that carry enriched state between stages
- **Error conversion functions** (`input_to_gateway`, `runtime_to_gateway`) for each domain error type
- **`GatewayBuilder`** as the SAF — the only public way to create the gateway
- **`DefaultGateway`** as `pub(crate)` — never exposed directly

```
InputRequest<String>
  ├─ pre:  InputGuardrailMiddleware  — validates, sanitizes, PII-masks
  ├─ route: PipelineRouter          — calls AgentOrchestration::process()
  └─ post: OutputGuardrailMiddleware — checks toxicity, hallucination

OutputResponse<GatewayResponse>
  └─ payload.content  — guardrail-sanitized LLM output
```

---

## Checklist

- [ ] Import only from `swe_gateway::saf::*`
- [ ] Domain struct is `pub(crate)` — builder/factory is the only public constructor
- [ ] Pipeline types implement `PipelineReq` / `PipelineResp`
- [ ] `PipelineGateway` is implemented — `Gateway::process` delegates to `process_via_pipeline`
- [ ] Domain errors implement `IntoGatewayError` with correct error codes
- [ ] Post-middleware handles observability (logging, metrics) — not the router
- [ ] Upstream code depends on `Arc<dyn Gateway<Req, Resp>>` — never on the concrete type
