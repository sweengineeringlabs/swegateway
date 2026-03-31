# swe-gateway Backlog

## Open

### BL-001: Review HttpInbound/HttpOutbound naming

**Priority:** Low
**Added:** 2026-03-07
**Context:** [architecture.md](3-design/architecture.md) section 2 (Gateway Families)

HTTP is already a request/response protocol, so `HttpInbound` (reads) and `HttpOutbound` (writes) can be confusing:

- "Inbound GET" reads naturally within the gateway abstraction (system is always the client calling out)
- But someone thinking about HTTP server inbound (receiving requests) vs HTTP client outbound (sending requests) may misread the intent
- `HttpInbound::handle()` receives a request — this overlaps with server-side semantics even though the gateway models external dependency access

**Options to evaluate:**
1. Keep as-is — naming is consistent across all 5 gateway families, and the architecture doc clarifies the distinction
2. Rename to `HttpReader` / `HttpWriter` — breaks the `*Inbound` / `*Outbound` pattern but removes HTTP-specific ambiguity
3. Add doc comments on `HttpInbound` / `HttpOutbound` explicitly noting this is gateway-direction (data flowing into/out of the application), not HTTP-direction (request/response)
4. Split HTTP gateway differently — e.g., `HttpClient` (outbound requests) and `HttpHandler` (inbound request processing) with `HttpGateway` combining both

**Decision:** Deferred — current naming is correct within the gateway abstraction. Revisit if users report confusion.

## Closed

### BL-002: Add built-in retry logic

**Priority:** Medium
**Added:** 2026-03-24
**Closed:** 2026-03-24
**Commit:** `34f4021`

Implemented `RetryMiddleware` with configurable backoff (fixed, exponential with jitter), custom retry predicate, and `RetryMiddlewareBuilder`. 8 e2e + 11 unit tests.

---

### BL-003: Add built-in rate limiting

**Priority:** Medium
**Added:** 2026-03-24
**Closed:** 2026-03-24
**Commit:** `34f4021`

Implemented token-bucket `RateLimiter` as `RequestMiddleware` with `RateLimiterBuilder`. Thread-safe via `parking_lot::Mutex`. 7 e2e + 8 unit tests.

---

### BL-004: MemoryDatabase sorting and filtering limitations

**Priority:** Low
**Added:** 2026-03-24
**Closed:** 2026-03-24
**Commit:** `34f4021`

Type-aware numeric sorting, nulls-last ordering, and comparison operator filtering (`__gt`, `__lt`, `__gte`, `__lte`, `__like`, `__in`). Backward compatible. 18 e2e tests.

---

### BL-005: Add async streaming support for large result sets

**Priority:** Medium
**Added:** 2026-03-24
**Closed:** 2026-03-24
**Commit:** `34f4021`

Added `query_stream` on `DatabaseInbound` and `list_stream` on `FileInbound` with default Vec-to-stream implementations. `GatewayStream<T>` type alias and `StreamExt` re-exported via saf. 8 e2e tests.

---

### BL-006: Configuration validation and environment variable expansion

**Priority:** Medium
**Added:** 2026-03-24
**Closed:** 2026-03-24
**Commit:** `34f4021`

Added `GatewayConfig::validate()` and `expand_env_vars()` supporting `${VAR}` and `${VAR:-default}` syntax. 22 e2e tests.

---

### BL-007: DaemonRunner observability fallback

**Priority:** Low
**Added:** 2026-03-24
**Closed:** 2026-03-24
**Commit:** `34f4021`

Added `DaemonRunner::without_observability()` and `lightweight_daemon()` builder. Skips MDC context and sidecar spawn, sets backend to `"none"`. 6 e2e + 3 unit tests.

---

### BL-008: Pipeline middleware short-circuit support

**Priority:** Low
**Added:** 2026-03-24
**Closed:** 2026-03-24
**Commit:** `34f4021`

Added `MiddlewareAction::ShortCircuit(Resp)` enum and `process_request_action` method on `RequestMiddleware`. Post-middleware still runs on short-circuited responses. Backward compatible via default `Resp` type parameter. 8 e2e tests.
