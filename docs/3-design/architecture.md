# swe-gateway Architecture

**Audience**: Developers, architects

> **TLDR**: swe-gateway provides a hexagonal architecture (ports-and-adapters) abstraction for external dependencies. Each gateway family defines an **Inbound** trait (reads), an **Outbound** trait (writes), and a **Combined** trait (both). The crate follows SEA layering — all types flow through `saf/`, and `api/`/`core/` are private.

## Version

| Field        | Value              |
|--------------|--------------------|
| Version      | 0.3.0              |
| Status       | Active             |
| Last Updated | 2026-03-24         |

---

## 1. Inbound/Outbound Pattern

### 1.1 What Inbound and Outbound Mean

The terms describe **data flow direction relative to the application boundary**:

```
                        ┌─────────────────────┐
                        │    Application       │
   Inbound              │                     │              Outbound
   (reads)              │    Business Logic    │              (writes)
  ◄─────────────────────┤                     ├─────────────────────►
   External resource    │                     │    External resource
   INTO the system      └─────────────────────┘    FROM the system
```

- **Inbound** — data flows IN from an external resource to the application. These are read/query operations: `SELECT`, file read, HTTP GET, check status, list records.
- **Outbound** — data flows OUT from the application to an external resource. These are write/mutation operations: `INSERT`, file write, HTTP POST, send notification, create payment.
- **Gateway** — combined supertrait extending both Inbound and Outbound. Use when a consumer needs full access.

This is the hexagonal architecture naming convention where the application sits at the center, and "ports" face external dependencies.

### 1.2 Inbound/Outbound vs Input/Output

These are **different axes** and do not conflict:

| Concept | Layer | Direction relative to | Purpose |
|---------|-------|----------------------|---------|
| **Input/Output** | llmboot gateway (L1/L6) | The **end user** | Request lifecycle: validation, guardrails, formatting |
| **Inbound/Outbound** | swe-gateway | An **external dependency** | Resource access: reads vs writes |

An agent processing a user **input** might use an **Inbound** gateway to read from a database and an **Outbound** gateway to write results, then return the response through the **output** pipeline.

```
User ──Input──► [Input guardrails] ──► Agent ──► [Output guardrails] ──Output──► User
                                         │
                                         ├── Inbound ◄── Database ──► Outbound
                                         ├── Inbound ◄── File     ──► Outbound
                                         └── Inbound ◄── HTTP API ──► Outbound
```

---

## 2. Gateway Families

Six gateway families, each with Inbound + Outbound + Combined traits:

| Family | Abstracts | Inbound (read) | Outbound (write) | Default Impl |
|--------|-----------|-----------------|-------------------|--------------|
| Database | SQL, NoSQL, in-memory | `query`, `query_stream`, `get_by_id`, `exists`, `count` | `insert`, `update`, `delete`, `batch_insert`, `update_where`, `delete_where` | `MemoryDatabase` |
| File | Local FS, S3, GCS | `read`, `metadata`, `list`, `list_stream`, `exists` | `write`, `delete`, `copy`, `rename`, `create_directory`, `delete_directory` | `LocalFileGateway` |
| HTTP | REST, GraphQL, gRPC | `handle` (receive request) | `send`, `get`, `post_json`, `put_json`, `delete` | `RestClient` |
| Notification | Email, SMS, push, console | `get_status`, `list_sent` | `send`, `send_batch`, `cancel` | `ConsoleNotifier` |
| Payment | Stripe, PayPal, Square | `get_payment`, `list_payments`, `get_customer` | `create_payment`, `capture_payment`, `create_refund` | `MockPaymentGateway` |
| gRPC | Unary RPC calls | `handle_unary` | `call_unary` | — (implement via spi) |

Every Inbound trait includes a `health_check()` method for monitoring.

---

## 3. SEA Layering

```
┌──────────────────────────────────────────────────────────────────┐
│  saf/          (public surface — only module visible to consumers)│
│  ├── mod.rs       Re-exports all public types and traits          │
│  └── builders.rs  Factory functions for creating gateway instances│
├──────────────────────────────────────────────────────────────────┤
│  api/          (private — types and trait definitions)            │
│  ├── types.rs     GatewayError, GatewayErrorCode, HealthCheck,   │
│  │                Pagination, IntoGatewayError, ResultGatewayExt  │
│  ├── traits.rs    18 gateway traits (6 families x 3 traits)       │
│  │                + query_stream, list_stream (default impls)     │
│  ├── middleware.rs RequestMiddleware, ResponseMiddleware,          │
│  │                MiddlewareAction (Continue, ShortCircuit)        │
│  ├── database.rs  Record, QueryParams, WriteResult, DatabaseConfig│
│  ├── file.rs      FileInfo, UploadOptions, ListOptions            │
│  ├── http.rs      HttpRequest, HttpResponse, HttpAuth             │
│  ├── notification.rs  Notification, NotificationReceipt           │
│  ├── payment.rs   Payment, Money, Customer, Refund                │
│  └── grpc.rs      GrpcRequest, GrpcResponse, GrpcStatusCode      │
├──────────────────────────────────────────────────────────────────┤
│  core/         (private — default implementations)               │
│  ├── database/    MemoryDatabase (type-aware sorting, operators)  │
│  ├── file/        LocalFileGateway (tokio::fs)                    │
│  ├── http/        RestClient (configurable base URL + auth)       │
│  ├── notification/ ConsoleNotifier (stdout/silent)                │
│  ├── payment/     MockPaymentGateway (in-memory mock)             │
│  ├── pipeline.rs  Pipeline (pre → router → post, short-circuit)   │
│  ├── retry.rs     RetryMiddleware, BackoffStrategy, builder       │
│  ├── rate_limit.rs RateLimiter (token-bucket), builder            │
│  ├── daemon.rs    DaemonRunner (with/without observability)       │
│  └── metrics_bridge.rs  MetricsResponseMiddleware                 │
├──────────────────────────────────────────────────────────────────┤
│  spi.rs        (public — extension point for custom impls)        │
│                Re-exports traits + domain types for implementors  │
├──────────────────────────────────────────────────────────────────┤
│  provider.rs   (private — re-exported via saf)                   │
│                StatelessProvider, StatefulProvider,                │
│                LazyInit, LazyInitWithConfig                       │
├──────────────────────────────────────────────────────────────────┤
│  state.rs      (private — re-exported via saf)                   │
│                CachedService, ConfiguredCache                     │
└──────────────────────────────────────────────────────────────────┘
```

### Access Rules

- Consumers import from `swe_gateway::saf::*` or `swe_gateway::prelude::*`
- Custom gateway implementors import from `swe_gateway::spi::*`
- `api/` and `core/` are never directly accessible outside the crate
- `lib.rs` delegates everything: `pub use saf::*`

---

## 4. Error Model

Two complementary error representations:

### 4.1 GatewayError (enum — rich variant set)

16 variants covering all failure modes:

| Variant | Code | Retryable |
|---------|------|-----------|
| `InternalError` | Internal | No |
| `ValidationError` | InvalidInput | No |
| `NotFound` | NotFound | No |
| `AlreadyExists` | AlreadyExists | No |
| `Conflict` | AlreadyExists | No |
| `PermissionDenied` | PermissionDenied | No |
| `AuthenticationFailed` | PermissionDenied | No |
| `Timeout` | Timeout | Yes |
| `ConnectionFailed` | Unavailable | Yes |
| `RateLimitExceeded` | Unavailable | Yes |
| `Unavailable` | Unavailable | Yes |
| `Configuration` | Configuration | No |
| `NotSupported` | Configuration | No |
| `IoError` | Internal | No |
| `SerializationError` | InvalidInput | No |
| `BackendError` | Internal | No |

### 4.2 GatewayErrorCode (enum — 8 categories)

Coarse categorization for consumers that don't need variant-level detail:

```rust
GatewayErrorCode::Internal | InvalidInput | NotFound | AlreadyExists
                 | PermissionDenied | Timeout | Unavailable | Configuration
```

### 4.3 Convenience API

```rust
// Construction
GatewayError::new(GatewayErrorCode::NotFound, "user not found")
GatewayError::internal("unexpected failure")
GatewayError::not_found("user 123")

// Enrichment
err.with_details("query returned empty set")

// Inspection
err.code()          // -> GatewayErrorCode
err.is_retryable()  // -> bool
err.is_not_found()  // -> bool

// Extension traits
result.gateway_err("context message")  // Result<T, E: Error> -> GatewayResult<T>
result.log_error("operation_name")     // logs error via tracing, returns self
```

---

## 5. Provider Infrastructure

### 5.1 Provider Traits

| Trait | Purpose | Bounds |
|-------|---------|--------|
| `StatelessProvider` | Marker for request-scoped providers (no state) | `Default + Clone + Send + Sync + 'static` |
| `StatefulProvider` | Marker for shared providers (cached state) | `Send + Sync + 'static` |
| `LazyInit<S>` | Lazy service initialization pattern | `get_service()`, `is_initialized()`, `reset()` |
| `LazyInitWithConfig<S, C>` | Config-dependent lazy initialization | `get_service_with_config(&C)` |

### 5.2 State Management

**`CachedService<S>`** — lazy-initialized singleton with double-checked locking:

```rust
struct MyGateway {
    service: CachedService<dyn MyService + Send + Sync>,
}

// Fast path: read lock check. Slow path: init + write lock store.
// Locks are never held across .await points (Send-safe).
let svc = self.service.get_or_init(|| async { /* expensive init */ }).await?;
```

**`ConfiguredCache<S, C>`** — re-initializes when config changes:

```rust
struct MyGateway {
    service: ConfiguredCache<dyn MyService, String>,  // String = working_dir
}

// Returns cached if config matches, re-inits if config differs.
let svc = self.service.get_or_init_with_config(&working_dir, |dir| async { ... }).await?;
```

---

## 6. Factory Functions (SAF)

All gateway instances are created via factory functions in `saf::builders`:

```rust
use swe_gateway::saf;

// Database
let db = saf::memory_database();

// File
let files = saf::local_file_gateway("./data");

// HTTP
let client = saf::rest_client_with_base_url("https://api.example.com");

// Notification
let notifier = saf::console_notifier();
let silent = saf::silent_notifier();

// Payment
let payments = saf::mock_payment_gateway();
let failing = saf::mock_payment_gateway_with_failure(
    MockFailureMode::FailOverAmount(1000)
);
```

Consumers receive `impl XxxGateway` — they depend on traits, never on concrete types. This is the hexagonal pattern: swap implementations without changing consumer code.

---

## 7. Feature Flags

| Flag | Enables | Optional Dep |
|------|---------|-------------|
| `postgres` | PostgreSQL database backend | `sqlx` |
| `mysql` | MySQL/MariaDB database backend | `sqlx` |
| `sqlite` | SQLite database backend | `sqlx` |
| `s3` | Amazon S3 file storage | `aws-sdk-s3` |
| `graphql` | GraphQL HTTP backend | `graphql_client` |
| `email` | Email notifications via SMTP | `lettre` |
| `stripe` | Stripe payment processing | `async-stripe` |
| `tauri` | Tauri desktop app integration | `tauri` |
| `full` | postgres + s3 + email + stripe | — |

Default features: none. All backends are opt-in.

---

## 8. Middleware & Pipeline

### 8.1 Pipeline Architecture

The `Pipeline` composes pre-middleware, a router, and post-middleware into a single execution chain:

```
Pre-middleware₁ → Pre-middleware₂ → ... → Router → Post-middleware₁ → Post-middleware₂ → ...
```

Generic over `Req`, `Resp`, `Err` (defaults: `serde_json::Value`, `serde_json::Value`, `GatewayError`).

### 8.2 RequestMiddleware Trait

```rust
#[async_trait]
pub trait RequestMiddleware<
    Req = serde_json::Value,
    Err = GatewayError,
    Resp = serde_json::Value,  // added in 0.3.0 for short-circuit support
>: Send + Sync {
    async fn process_request(&self, request: Req) -> Result<Req, Err>;
    async fn process_request_action(&self, request: Req) -> Result<MiddlewareAction<Req, Resp>, Err> {
        self.process_request(request).await.map(MiddlewareAction::Continue)
    }
}
```

The `Resp` parameter defaults to `serde_json::Value`, so existing two-parameter implementations (`RequestMiddleware<MyReq, MyErr>`) compile without changes.

### 8.3 Short-Circuit

Override `process_request_action` to return `MiddlewareAction::ShortCircuit(response)`. When short-circuited:
- Remaining pre-middleware is skipped
- Router is skipped
- Post-middleware **still runs** (for logging, metrics, headers)

### 8.4 RetryMiddleware

```rust
let retry = saf::retry_middleware()
    .max_attempts(5)
    .exponential_backoff(Duration::from_millis(200), true) // base, jitter
    .build();
```

Backoff strategies: `BackoffStrategy::Fixed { delay }`, `BackoffStrategy::Exponential { base, jitter }`. Uses `GatewayError::is_retryable()` by default; override with `.retry_predicate(Arc::new(|e| ...))`.

### 8.5 RateLimiter

```rust
let limiter = saf::rate_limiter(100, 10.0); // 100 burst capacity, 10 tokens/sec refill
```

Token-bucket algorithm, thread-safe via `parking_lot::Mutex`. Returns `GatewayError::RateLimitExceeded` when throttled.

---

## 9. Async Streaming

`DatabaseInbound::query_stream()` and `FileInbound::list_stream()` return `GatewayStream<'a, T>` — a `Pin<Box<dyn Stream<Item = GatewayResult<T>> + Send>>`.

Default implementations call the Vec-returning method and convert via `futures::stream::iter`. Backends with cursor support can override for true incremental streaming.

```rust
use swe_gateway::saf::StreamExt; // re-exported from futures

let mut stream = db.query_stream("users", params).await?;
while let Some(record) = stream.next().await {
    process(record?);
}
```

---

## 10. Configuration

### 10.1 Environment Variable Expansion

All TOML string values support `${VAR}` and `${VAR:-default}` syntax. Expansion happens before TOML parsing via `expand_env_vars()`.

```toml
[database]
connection_string = "${DATABASE_URL}"
password = "${DB_PASSWORD:-changeme}"
```

### 10.2 Validation

`GatewayConfig::validate()` checks required fields per active backend:
- Postgres/MySQL/MongoDB/SQLite: requires `connection_string` or `host` + `database`
- S3/GCS/Azure: requires `region`
- Non-mock payment: requires `api_key`
- File sink: requires `path`

All violations are collected and reported in a single `ConfigError::Validation`.

---

## 11. DaemonRunner

Shared startup sequence for gateway daemons. Handles MDC logging context, sidecar lifecycle, and context injection.

```rust
// Standard: MDC context + obsrv sidecar
DaemonRunner::new("my-service")
    .with_bind("0.0.0.0:9000")
    .with_backend("sidecar")
    .run(|ctx| async { server(ctx).await })
    .await?;

// Lightweight: no observability, backend="none", obsrv_port=0
saf::lightweight_daemon("my-cli")
    .run(|ctx| async { cli(ctx).await })
    .await?;
```

---

## 12. MemoryDatabase Advanced Filtering

Filter keys support operator suffixes for comparison queries:

| Suffix | Operation | Example |
|--------|-----------|---------|
| (none) | Equality | `filter("status", "active")` |
| `__gt` | Greater than | `filter("price__gt", 10)` |
| `__lt` | Less than | `filter("price__lt", 100)` |
| `__gte` | Greater or equal | `filter("age__gte", 18)` |
| `__lte` | Less or equal | `filter("age__lte", 65)` |
| `__like` | Case-insensitive substring | `filter("name__like", "alice")` |
| `__in` | Value in JSON array | `filter("status__in", json!(["active", "pending"]))` |

Sorting is type-aware: numeric values compare numerically, null/missing values sort last regardless of direction.
