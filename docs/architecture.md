# swe-gateway Architecture

## SEA Layering

```
saf/           (L4) Factory functions, public re-exports — only public surface
api/           (L2) Traits, domain types, error types, middleware — private
core/          (L3) Default implementations, pipeline, retry, rate limiter — private
spi/           (L1) Extension re-exports for custom implementations
provider.rs         Provider marker traits — private, re-exported via saf
state.rs            State management (CachedService, ConfiguredCache) — private, re-exported via saf
```

## Gateway Pattern

Six gateway families, each following the Inbound/Outbound/Combined pattern:

- **Inbound** — read/query operations (includes `health_check()`, optional `query_stream`/`list_stream`)
- **Outbound** — write/mutation operations
- **Gateway** — combines both (supertrait)

Families: Database, File, HTTP, Notification, Payment, gRPC.

## Middleware & Pipeline

`RequestMiddleware` → `Router` → `ResponseMiddleware`, composed via `Pipeline`.

- **RetryMiddleware** — configurable backoff (fixed, exponential, jitter)
- **RateLimiter** — token-bucket rate limiting
- **MiddlewareAction::ShortCircuit** — early pipeline exit, post-middleware still runs

## Configuration

TOML-based with `${ENV_VAR:-default}` expansion and `GatewayConfig::validate()`.

See [docs/3-design/architecture.md](3-design/architecture.md) for full details.
