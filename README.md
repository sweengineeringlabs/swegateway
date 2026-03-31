# swe-gateway

Hexagonal architecture (ports-and-adapters) abstraction layer for external dependencies. Provides inbound (read) + outbound (write) gateway traits for database, file, HTTP, notification, payment, and gRPC gateways.

## Usage

```rust
use swe_gateway::prelude::*;
use swe_gateway::saf;

// Gateways
let db = saf::memory_database();
let files = saf::local_file_gateway("./data");
let payments = saf::mock_payment_gateway();

// Middleware
let retry = saf::retry_middleware().max_attempts(3).build();
let limiter = saf::rate_limiter(100, 10.0); // 100 burst, 10/sec refill
```

## Gateway Types

| Gateway | Abstracts | Default Impl |
|---------|-----------|--------------|
| DatabaseGateway | SQL, NoSQL, in-memory | MemoryDatabase |
| FileGateway | Local FS, S3, GCS | LocalFileGateway |
| HttpGateway | REST, GraphQL, gRPC | RestClient |
| NotificationGateway | Email, SMS, Push | ConsoleNotifier |
| PaymentGateway | Stripe, PayPal, Square | MockPaymentGateway |
| GrpcGateway | gRPC unary calls | — (implement via spi) |

## Middleware & Pipeline

Built-in middleware composable with the `Pipeline` infrastructure:

- **RetryMiddleware** — configurable max attempts, fixed/exponential/jitter backoff, respects `is_retryable()`
- **RateLimiter** — token-bucket algorithm, thread-safe via `parking_lot`
- **MiddlewareAction::ShortCircuit** — early return from pipeline (cache hits, auth rejection)

```rust
use swe_gateway::saf::*;

// Short-circuit: override process_request_action to return ShortCircuit
// Post-middleware still runs on short-circuited responses
```

## Async Streaming

Large result sets can be consumed incrementally:

```rust
let mut stream = db.query_stream("users", QueryParams::new()).await?;
while let Some(record) = stream.next().await {
    let record = record?;
    // process one record at a time
}
```

Methods: `DatabaseInbound::query_stream()`, `FileInbound::list_stream()`. Type alias: `GatewayStream<'a, T>`.

## Configuration

TOML-based config with environment variable expansion and validation:

```toml
[database]
connection_string = "${DATABASE_URL}"
password = "${DB_PASSWORD:-default_pass}"
```

```rust
let config = saf::load_config_from_str(toml_str)?;
config.validate()?; // checks required fields per active backend
```

## Database Filtering

MemoryDatabase supports comparison operators via filter key suffixes:

```rust
let params = QueryParams::new()
    .filter("price__gte", 10.0)
    .filter("price__lt", 100.0)
    .filter("category__in", json!(["electronics", "books"]))
    .filter("name__like", "widget");
```

Operators: `__gt`, `__lt`, `__gte`, `__lte`, `__like`, `__in`. Plain keys use equality.

## DaemonRunner

Shared startup sequence for gateway daemons with optional observability:

```rust
// Full observability (MDC context + sidecar)
DaemonRunner::new("my-service").run(|ctx| async { Ok(()) }).await?;

// Lightweight (no sidecar, no MDC)
saf::lightweight_daemon("my-cli").run(|ctx| async { Ok(()) }).await?;
```

## Feature Flags

- `postgres` — PostgreSQL database support
- `mysql` — MySQL/MariaDB database support
- `sqlite` — SQLite database support
- `s3` — Amazon S3 file storage
- `graphql` — GraphQL HTTP backend
- `email` — Email notifications via SMTP
- `stripe` — Stripe payment processing
- `tauri` — Tauri integration
- `full` — Enable postgres, s3, email, and stripe

Default features: none. All backends are opt-in.

## License

MIT
