# Changelog

## [0.3.0] - 2026-03-24

### Added
- `RetryMiddleware` with `BackoffStrategy` (Fixed, Exponential with jitter) and `RetryMiddlewareBuilder`. Respects `GatewayError::is_retryable()` by default, supports custom predicates. Factory: `saf::retry_middleware()`. (BL-002)
- `RateLimiter` token-bucket implementation as `RequestMiddleware`. Thread-safe via `parking_lot::Mutex`. Factory: `saf::rate_limiter()`, `saf::rate_limiter_builder()`. (BL-003)
- `MemoryDatabase` type-aware numeric sorting (no longer string-based) and comparison operator filtering: `__gt`, `__lt`, `__gte`, `__lte`, `__like` (case-insensitive substring), `__in` (value-in-array). Nulls sort last. Backward compatible — plain keys remain equality. (BL-004)
- `DatabaseInbound::query_stream()` and `FileInbound::list_stream()` for async streaming of large result sets. Default implementations convert Vec to Stream. `GatewayStream<'a, T>` type alias and `StreamExt` re-exported via saf. (BL-005)
- `GatewayConfig::validate()` for checking required fields per active backend. `expand_env_vars()` supporting `${VAR}` and `${VAR:-default}` syntax in TOML config strings. (BL-006)
- `DaemonRunner::without_observability()` for lightweight mode (skips MDC context and sidecar). `saf::lightweight_daemon()` convenience builder. (BL-007)
- `MiddlewareAction` enum (`Continue`, `ShortCircuit`) and `RequestMiddleware::process_request_action()` for pipeline short-circuit support. Post-middleware still runs on short-circuited responses. Backward compatible via default `Resp` type parameter. (BL-008)
- `GrpcGateway`, `GrpcInbound`, `GrpcOutbound` traits with `handle_unary` / `call_unary` methods and domain types (`GrpcRequest`, `GrpcResponse`, `GrpcMetadata`, `GrpcStatusCode`).
- Shared test fixtures module (`tests/fixtures/`) with record builders, gateway factories (`TempFileGateway`), mock middleware, and seed data helpers.

### Test Coverage
- 570 tests (253 unit + 317 integration/e2e), 0 failures.
- New test suites: gRPC (36), security (21), SAF surface (27), stress/concurrency (10), load (6), perf (6), caching (8), streaming edge (8), middleware edge (12), cross-gateway integration (6), fixture smoke (10).

## [0.2.0] - 2026-03-07

### Added
- `GatewayErrorCode` enum for categorizing errors by code.
- New `GatewayError` variants: `AlreadyExists`, `PermissionDenied`, `Unavailable`, `Configuration`.
- Convenience constructors: `GatewayError::new()`, `::internal()`, `::not_found()`, `::invalid_input()`, `::unavailable()`, `::already_exists()`, `::permission_denied()`, `::timeout()`, `::configuration()`.
- `GatewayError::with_details()` for appending context to errors.
- `GatewayError::code()` for mapping variants back to `GatewayErrorCode`.
- `IntoGatewayError` trait for domain error conversion.
- `ResultGatewayExt` trait with `gateway_err()` and `log_error()` extension methods.
- `StatelessProvider` and `StatefulProvider` marker traits.
- `LazyInit` and `LazyInitWithConfig` traits for lazy service initialization.
- `CachedService` and `ConfiguredCache` state management utilities.
- `Unavailable` variant now included in `is_retryable()`.
- `tauri` optional feature flag.
- `parking_lot` dependency for synchronization primitives.

### Changed
- Standalone crate (no longer uses workspace-inherited metadata).
- All dependencies use explicit versions.

## [0.1.0] - 2026-02-01

### Added
- Initial gateway abstraction with 5 gateway families (Database, File, HTTP, Notification, Payment).
- Inbound/Outbound/Combined trait pattern.
- Default implementations: MemoryDatabase, LocalFileGateway, RestClient, ConsoleNotifier, MockPaymentGateway.
- SAF factory functions.
- Health check infrastructure.
- Pagination support.
