// @allow: no_mocks_in_integration — mocks gRPC gateway boundary for e2e trait verification
//! End-to-end tests for the gRPC gateway traits (GrpcInbound, GrpcOutbound, GrpcGateway).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use swe_gateway::saf::grpc::{GrpcMetadata, GrpcRequest, GrpcResponse, GrpcStatusCode};
use swe_gateway::saf::{
    GatewayError, GatewayErrorCode, GatewayResult, GrpcGateway, GrpcInbound, GrpcOutbound,
    HealthCheck, HealthStatus,
};

// =============================================================================
// Mock gRPC Gateway
// =============================================================================

/// Behavior to inject into the mock for controlling responses and errors.
#[derive(Clone)]
enum MockBehavior {
    /// Return a successful response with the given body.
    Success(Vec<u8>),
    /// Return a GatewayError based on a gRPC status code.
    GrpcError(GrpcStatusCode),
    /// Echo the request body back as the response body.
    Echo,
}

/// Mock implementation of GrpcInbound + GrpcOutbound for testing.
struct MockGrpcGateway {
    /// Controls what `handle_unary` returns.
    inbound_behavior: Mutex<MockBehavior>,
    /// Controls what `call_unary` returns.
    outbound_behavior: Mutex<MockBehavior>,
    /// Captured inbound requests for assertion.
    captured_inbound: Mutex<Vec<GrpcRequest>>,
    /// Captured outbound calls (endpoint, request) for assertion.
    captured_outbound: Mutex<Vec<(String, GrpcRequest)>>,
    /// Whether health check should report healthy.
    healthy: Mutex<bool>,
}

impl MockGrpcGateway {
    fn new() -> Self {
        Self {
            inbound_behavior: Mutex::new(MockBehavior::Echo),
            outbound_behavior: Mutex::new(MockBehavior::Echo),
            captured_inbound: Mutex::new(Vec::new()),
            captured_outbound: Mutex::new(Vec::new()),
            healthy: Mutex::new(true),
        }
    }

    fn with_inbound_behavior(self, behavior: MockBehavior) -> Self {
        *self.inbound_behavior.lock().unwrap() = behavior;
        self
    }

    fn with_outbound_behavior(self, behavior: MockBehavior) -> Self {
        *self.outbound_behavior.lock().unwrap() = behavior;
        self
    }

    fn set_healthy(&self, healthy: bool) {
        *self.healthy.lock().unwrap() = healthy;
    }

    fn last_inbound_request(&self) -> Option<GrpcRequest> {
        self.captured_inbound.lock().unwrap().last().cloned()
    }

    fn last_outbound_call(&self) -> Option<(String, GrpcRequest)> {
        self.captured_outbound.lock().unwrap().last().cloned()
    }

    /// Execute the behavior, producing a response or error. Metadata from the
    /// request is echoed into the response so tests can verify passthrough.
    fn execute_behavior(
        behavior: &MockBehavior,
        request: &GrpcRequest,
    ) -> GatewayResult<GrpcResponse> {
        match behavior {
            MockBehavior::Success(body) => Ok(GrpcResponse {
                body: body.clone(),
                metadata: request.metadata.clone(),
            }),
            MockBehavior::Echo => Ok(GrpcResponse {
                body: request.body.clone(),
                metadata: request.metadata.clone(),
            }),
            MockBehavior::GrpcError(code) => Err(grpc_status_to_gateway_error(*code)),
        }
    }
}

/// Map a GrpcStatusCode to a GatewayError, mirroring the mapping a real
/// gRPC adapter would perform.
fn grpc_status_to_gateway_error(code: GrpcStatusCode) -> GatewayError {
    match code {
        GrpcStatusCode::NotFound => GatewayError::not_found("gRPC: resource not found"),
        GrpcStatusCode::InvalidArgument => {
            GatewayError::invalid_input("gRPC: invalid argument")
        }
        GrpcStatusCode::PermissionDenied => {
            GatewayError::permission_denied("gRPC: permission denied")
        }
        GrpcStatusCode::Unauthenticated => {
            GatewayError::AuthenticationFailed("gRPC: unauthenticated".into())
        }
        GrpcStatusCode::AlreadyExists => {
            GatewayError::already_exists("gRPC: already exists")
        }
        GrpcStatusCode::DeadlineExceeded => {
            GatewayError::timeout("gRPC: deadline exceeded")
        }
        GrpcStatusCode::Unavailable => {
            GatewayError::unavailable("gRPC: service unavailable")
        }
        GrpcStatusCode::Internal => GatewayError::internal("gRPC: internal error"),
        GrpcStatusCode::Unimplemented => {
            GatewayError::NotSupported("gRPC: unimplemented".into())
        }
        GrpcStatusCode::ResourceExhausted => {
            GatewayError::RateLimitExceeded("gRPC: resource exhausted".into())
        }
        GrpcStatusCode::Cancelled => GatewayError::internal("gRPC: cancelled"),
        GrpcStatusCode::Unknown => GatewayError::internal("gRPC: unknown error"),
        GrpcStatusCode::FailedPrecondition => {
            GatewayError::ValidationError("gRPC: failed precondition".into())
        }
        GrpcStatusCode::Aborted => GatewayError::internal("gRPC: aborted"),
        GrpcStatusCode::OutOfRange => {
            GatewayError::ValidationError("gRPC: out of range".into())
        }
        GrpcStatusCode::DataLoss => GatewayError::internal("gRPC: data loss"),
        GrpcStatusCode::Ok => {
            unreachable!("Ok is not an error status")
        }
    }
}

impl GrpcInbound for MockGrpcGateway {
    fn handle_unary(
        &self,
        request: GrpcRequest,
    ) -> BoxFuture<'_, GatewayResult<GrpcResponse>> {
        self.captured_inbound.lock().unwrap().push(request.clone());
        let behavior = self.inbound_behavior.lock().unwrap().clone();
        Box::pin(async move { Self::execute_behavior(&behavior, &request) })
    }

    fn health_check(&self) -> BoxFuture<'_, GatewayResult<HealthCheck>> {
        let healthy = *self.healthy.lock().unwrap();
        Box::pin(async move {
            if healthy {
                Ok(HealthCheck::healthy())
            } else {
                Ok(HealthCheck::unhealthy("gRPC service is down"))
            }
        })
    }
}

impl GrpcOutbound for MockGrpcGateway {
    fn call_unary(
        &self,
        endpoint: &str,
        request: GrpcRequest,
    ) -> BoxFuture<'_, GatewayResult<GrpcResponse>> {
        self.captured_outbound
            .lock()
            .unwrap()
            .push((endpoint.to_string(), request.clone()));
        let behavior = self.outbound_behavior.lock().unwrap().clone();
        Box::pin(async move { Self::execute_behavior(&behavior, &request) })
    }

    fn health_check(&self) -> BoxFuture<'_, GatewayResult<HealthCheck>> {
        let healthy = *self.healthy.lock().unwrap();
        Box::pin(async move {
            if healthy {
                Ok(HealthCheck::healthy())
            } else {
                Ok(HealthCheck::unhealthy("gRPC outbound is down"))
            }
        })
    }
}

impl GrpcGateway for MockGrpcGateway {}

// =============================================================================
// Helper Builders
// =============================================================================

fn build_request(method: &str, body: Vec<u8>) -> GrpcRequest {
    GrpcRequest {
        method: method.to_string(),
        body,
        metadata: GrpcMetadata::default(),
    }
}

fn build_request_with_metadata(
    method: &str,
    body: Vec<u8>,
    headers: HashMap<String, String>,
) -> GrpcRequest {
    GrpcRequest {
        method: method.to_string(),
        body,
        metadata: GrpcMetadata { headers },
    }
}

// =============================================================================
// 1. handle_unary receives request and returns response
// =============================================================================

mod inbound_tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_unary_echo_returns_request_body_as_response() {
        let gw = MockGrpcGateway::new();
        let body = vec![0x08, 0x01, 0x10, 0x02];
        let req = build_request("test.v1.Service/Echo", body.clone());

        let resp = gw.handle_unary(req).await.unwrap();

        assert_eq!(
            resp.body, body,
            "echo behavior must return the same body that was sent"
        );
    }

    #[tokio::test]
    async fn test_handle_unary_success_returns_configured_body() {
        let gw = MockGrpcGateway::new()
            .with_inbound_behavior(MockBehavior::Success(vec![0xCA, 0xFE]));
        let req = build_request("test.v1.Service/Get", vec![0x01]);

        let resp = gw.handle_unary(req).await.unwrap();

        assert_eq!(
            resp.body,
            vec![0xCA, 0xFE],
            "must return the configured success body"
        );
    }

    #[tokio::test]
    async fn test_handle_unary_captures_request_for_inspection() {
        let gw = MockGrpcGateway::new();
        let req = build_request("test.v1.Service/Create", vec![0x42]);

        gw.handle_unary(req).await.unwrap();

        let captured = gw.last_inbound_request().expect("must capture request");
        assert_eq!(captured.method, "test.v1.Service/Create");
        assert_eq!(captured.body, vec![0x42]);
    }
}

// =============================================================================
// 2. call_unary sends request and gets response
// =============================================================================

mod outbound_tests {
    use super::*;

    #[tokio::test]
    async fn test_call_unary_echo_returns_request_body_as_response() {
        let gw = MockGrpcGateway::new();
        let body = vec![0xDE, 0xAD];
        let req = build_request("test.v1.Service/Ping", body.clone());

        let resp = gw.call_unary("localhost:50051", req).await.unwrap();

        assert_eq!(
            resp.body, body,
            "echo behavior must return the same body that was sent"
        );
    }

    #[tokio::test]
    async fn test_call_unary_success_returns_configured_body() {
        let gw = MockGrpcGateway::new()
            .with_outbound_behavior(MockBehavior::Success(vec![0xBE, 0xEF]));
        let req = build_request("test.v1.Service/Fetch", vec![]);

        let resp = gw.call_unary("remote:443", req).await.unwrap();

        assert_eq!(resp.body, vec![0xBE, 0xEF]);
    }

    #[tokio::test]
    async fn test_call_unary_captures_endpoint_and_request() {
        let gw = MockGrpcGateway::new();
        let req = build_request("test.v1.Service/Send", vec![0x01, 0x02]);

        gw.call_unary("backend.svc:8080", req).await.unwrap();

        let (endpoint, captured) = gw.last_outbound_call().expect("must capture outbound call");
        assert_eq!(endpoint, "backend.svc:8080");
        assert_eq!(captured.method, "test.v1.Service/Send");
        assert_eq!(captured.body, vec![0x01, 0x02]);
    }
}

// =============================================================================
// 3. health_check returns healthy status
// =============================================================================

mod health_check_tests {
    use super::*;

    #[tokio::test]
    async fn test_inbound_health_check_healthy_returns_healthy_status() {
        let gw = MockGrpcGateway::new();

        let health = GrpcInbound::health_check(&gw).await.unwrap();

        assert_eq!(
            health.status,
            HealthStatus::Healthy,
            "default mock must report healthy"
        );
    }

    #[tokio::test]
    async fn test_outbound_health_check_healthy_returns_healthy_status() {
        let gw = MockGrpcGateway::new();

        let health = GrpcOutbound::health_check(&gw).await.unwrap();

        assert_eq!(health.status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_inbound_health_check_unhealthy_returns_unhealthy_with_message() {
        let gw = MockGrpcGateway::new();
        gw.set_healthy(false);

        let health = GrpcInbound::health_check(&gw).await.unwrap();

        assert_eq!(health.status, HealthStatus::Unhealthy);
        assert_eq!(
            health.message.as_deref(),
            Some("gRPC service is down"),
            "unhealthy check must include a descriptive message"
        );
    }

    #[tokio::test]
    async fn test_outbound_health_check_unhealthy_returns_unhealthy_with_message() {
        let gw = MockGrpcGateway::new();
        gw.set_healthy(false);

        let health = GrpcOutbound::health_check(&gw).await.unwrap();

        assert_eq!(health.status, HealthStatus::Unhealthy);
        assert_eq!(health.message.as_deref(), Some("gRPC outbound is down"));
    }
}

// =============================================================================
// 4. Error propagation (not found, internal error)
// =============================================================================

mod error_propagation_tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_unary_not_found_propagates_not_found_error() {
        let gw = MockGrpcGateway::new()
            .with_inbound_behavior(MockBehavior::GrpcError(GrpcStatusCode::NotFound));
        let req = build_request("test.v1.Service/Get", vec![0x01]);

        let err = gw.handle_unary(req).await.unwrap_err();

        assert!(err.is_not_found(), "must propagate as NotFound error");
        assert_eq!(err.code(), GatewayErrorCode::NotFound);
    }

    #[tokio::test]
    async fn test_handle_unary_internal_error_propagates_internal_error() {
        let gw = MockGrpcGateway::new()
            .with_inbound_behavior(MockBehavior::GrpcError(GrpcStatusCode::Internal));
        let req = build_request("test.v1.Service/Get", vec![]);

        let err = gw.handle_unary(req).await.unwrap_err();

        assert_eq!(err.code(), GatewayErrorCode::Internal);
        assert!(
            err.to_string().contains("internal"),
            "error message must mention internal: got '{}'",
            err
        );
    }

    #[tokio::test]
    async fn test_call_unary_not_found_propagates_not_found_error() {
        let gw = MockGrpcGateway::new()
            .with_outbound_behavior(MockBehavior::GrpcError(GrpcStatusCode::NotFound));
        let req = build_request("test.v1.Service/Lookup", vec![]);

        let err = gw.call_unary("remote:443", req).await.unwrap_err();

        assert!(err.is_not_found());
    }

    #[tokio::test]
    async fn test_call_unary_internal_error_propagates_internal_error() {
        let gw = MockGrpcGateway::new()
            .with_outbound_behavior(MockBehavior::GrpcError(GrpcStatusCode::Internal));
        let req = build_request("test.v1.Service/Process", vec![]);

        let err = gw.call_unary("remote:443", req).await.unwrap_err();

        assert_eq!(err.code(), GatewayErrorCode::Internal);
    }

    #[tokio::test]
    async fn test_handle_unary_permission_denied_propagates_permission_denied() {
        let gw = MockGrpcGateway::new()
            .with_inbound_behavior(MockBehavior::GrpcError(GrpcStatusCode::PermissionDenied));
        let req = build_request("test.v1.Service/Admin", vec![]);

        let err = gw.handle_unary(req).await.unwrap_err();

        assert_eq!(err.code(), GatewayErrorCode::PermissionDenied);
    }
}

// =============================================================================
// 5. Metadata passthrough in requests/responses
// =============================================================================

mod metadata_tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_unary_metadata_echoed_in_response() {
        let gw = MockGrpcGateway::new();
        let mut headers = HashMap::new();
        headers.insert("authorization".to_string(), "Bearer secret-token".to_string());
        headers.insert("x-request-id".to_string(), "req-42".to_string());
        let req = build_request_with_metadata("test.v1.Service/Secure", vec![0x01], headers);

        let resp = gw.handle_unary(req).await.unwrap();

        assert_eq!(
            resp.metadata.headers.get("authorization"),
            Some(&"Bearer secret-token".to_string()),
            "authorization header must pass through"
        );
        assert_eq!(
            resp.metadata.headers.get("x-request-id"),
            Some(&"req-42".to_string()),
            "x-request-id header must pass through"
        );
    }

    #[tokio::test]
    async fn test_call_unary_metadata_echoed_in_response() {
        let gw = MockGrpcGateway::new();
        let mut headers = HashMap::new();
        headers.insert("x-trace-id".to_string(), "trace-abc".to_string());
        let req = build_request_with_metadata("test.v1.Service/Traced", vec![], headers);

        let resp = gw.call_unary("remote:443", req).await.unwrap();

        assert_eq!(
            resp.metadata.headers.get("x-trace-id"),
            Some(&"trace-abc".to_string()),
            "trace header must pass through in outbound call"
        );
    }

    #[tokio::test]
    async fn test_handle_unary_empty_metadata_produces_empty_response_metadata() {
        let gw = MockGrpcGateway::new();
        let req = build_request("test.v1.Service/NoMeta", vec![]);

        let resp = gw.handle_unary(req).await.unwrap();

        assert!(
            resp.metadata.headers.is_empty(),
            "response metadata must be empty when request metadata is empty"
        );
    }

    #[tokio::test]
    async fn test_handle_unary_many_metadata_entries_all_preserved() {
        let gw = MockGrpcGateway::new();
        let mut headers = HashMap::new();
        for i in 0..50 {
            headers.insert(format!("x-header-{}", i), format!("value-{}", i));
        }
        let req =
            build_request_with_metadata("test.v1.Service/ManyHeaders", vec![], headers.clone());

        let resp = gw.handle_unary(req).await.unwrap();

        assert_eq!(
            resp.metadata.headers.len(),
            50,
            "all 50 metadata entries must be preserved"
        );
        for (key, value) in &headers {
            assert_eq!(
                resp.metadata.headers.get(key),
                Some(value),
                "metadata entry '{}' must be preserved",
                key
            );
        }
    }
}

// =============================================================================
// 6. GrpcStatusCode mapping to GatewayError
// =============================================================================

mod status_code_mapping_tests {
    use super::*;

    /// Table-driven test: each gRPC status code maps to the expected GatewayErrorCode.
    #[tokio::test]
    async fn test_grpc_status_codes_map_to_expected_gateway_error_codes() {
        let cases: Vec<(GrpcStatusCode, GatewayErrorCode)> = vec![
            (GrpcStatusCode::NotFound, GatewayErrorCode::NotFound),
            (GrpcStatusCode::InvalidArgument, GatewayErrorCode::InvalidInput),
            (GrpcStatusCode::PermissionDenied, GatewayErrorCode::PermissionDenied),
            (GrpcStatusCode::Unauthenticated, GatewayErrorCode::PermissionDenied),
            (GrpcStatusCode::AlreadyExists, GatewayErrorCode::AlreadyExists),
            (GrpcStatusCode::DeadlineExceeded, GatewayErrorCode::Timeout),
            (GrpcStatusCode::Unavailable, GatewayErrorCode::Unavailable),
            (GrpcStatusCode::Internal, GatewayErrorCode::Internal),
            (GrpcStatusCode::Unimplemented, GatewayErrorCode::Configuration),
            (GrpcStatusCode::ResourceExhausted, GatewayErrorCode::Unavailable),
            (GrpcStatusCode::Cancelled, GatewayErrorCode::Internal),
            (GrpcStatusCode::Unknown, GatewayErrorCode::Internal),
            (GrpcStatusCode::FailedPrecondition, GatewayErrorCode::InvalidInput),
            (GrpcStatusCode::Aborted, GatewayErrorCode::Internal),
            (GrpcStatusCode::OutOfRange, GatewayErrorCode::InvalidInput),
            (GrpcStatusCode::DataLoss, GatewayErrorCode::Internal),
        ];

        for (grpc_code, expected_gw_code) in cases {
            let err = grpc_status_to_gateway_error(grpc_code);
            assert_eq!(
                err.code(),
                expected_gw_code,
                "GrpcStatusCode::{:?} should map to GatewayErrorCode::{:?}, got {:?}",
                grpc_code,
                expected_gw_code,
                err.code()
            );
        }
    }

    #[tokio::test]
    async fn test_deadline_exceeded_is_retryable() {
        let err = grpc_status_to_gateway_error(GrpcStatusCode::DeadlineExceeded);
        assert!(
            err.is_retryable(),
            "DeadlineExceeded should produce a retryable timeout error"
        );
    }

    #[tokio::test]
    async fn test_unavailable_is_retryable() {
        let err = grpc_status_to_gateway_error(GrpcStatusCode::Unavailable);
        assert!(
            err.is_retryable(),
            "Unavailable should produce a retryable error"
        );
    }

    #[tokio::test]
    async fn test_not_found_is_not_retryable() {
        let err = grpc_status_to_gateway_error(GrpcStatusCode::NotFound);
        assert!(
            !err.is_retryable(),
            "NotFound should NOT be retryable"
        );
    }

    #[tokio::test]
    async fn test_permission_denied_is_not_retryable() {
        let err = grpc_status_to_gateway_error(GrpcStatusCode::PermissionDenied);
        assert!(
            !err.is_retryable(),
            "PermissionDenied should NOT be retryable"
        );
    }

    #[tokio::test]
    async fn test_resource_exhausted_is_retryable() {
        let err = grpc_status_to_gateway_error(GrpcStatusCode::ResourceExhausted);
        assert!(
            err.is_retryable(),
            "ResourceExhausted (rate limit) should be retryable"
        );
    }
}

// =============================================================================
// 7. Combined GrpcGateway trait works (both inbound + outbound)
// =============================================================================

mod combined_gateway_tests {
    use super::*;

    /// Verify that a single struct implementing GrpcGateway can serve both
    /// inbound and outbound roles simultaneously.
    #[tokio::test]
    async fn test_grpc_gateway_handles_inbound_and_outbound_on_same_instance() {
        let gw = Arc::new(MockGrpcGateway::new());

        // Inbound: handle a request
        let inbound_req = build_request("test.v1.Service/HandleMe", vec![0xAA]);
        let inbound_resp = gw.handle_unary(inbound_req).await.unwrap();
        assert_eq!(inbound_resp.body, vec![0xAA]);

        // Outbound: make a call
        let outbound_req = build_request("test.v1.Service/CallMe", vec![0xBB]);
        let outbound_resp = gw.call_unary("peer:9090", outbound_req).await.unwrap();
        assert_eq!(outbound_resp.body, vec![0xBB]);

        // Both were captured independently
        let inbound_captured = gw.last_inbound_request().unwrap();
        assert_eq!(inbound_captured.method, "test.v1.Service/HandleMe");

        let (endpoint, outbound_captured) = gw.last_outbound_call().unwrap();
        assert_eq!(endpoint, "peer:9090");
        assert_eq!(outbound_captured.method, "test.v1.Service/CallMe");
    }

    /// Verify the mock can be used behind a trait object.
    #[tokio::test]
    async fn test_grpc_gateway_works_as_dyn_trait_object() {
        let gw: Arc<dyn GrpcInbound> = Arc::new(MockGrpcGateway::new());
        let req = build_request("test.v1.Service/DynCall", vec![0x01]);

        let resp = gw.handle_unary(req).await.unwrap();

        assert_eq!(resp.body, vec![0x01], "trait object dispatch must work");
    }

    /// Verify the outbound trait also works behind a trait object.
    #[tokio::test]
    async fn test_grpc_outbound_works_as_dyn_trait_object() {
        let gw: Arc<dyn GrpcOutbound> = Arc::new(MockGrpcGateway::new());
        let req = build_request("test.v1.Service/DynOutbound", vec![0x02]);

        let resp = gw.call_unary("dynamic:443", req).await.unwrap();

        assert_eq!(resp.body, vec![0x02]);
    }

    /// Verify independent error behavior for inbound vs outbound on the same instance.
    #[tokio::test]
    async fn test_grpc_gateway_independent_error_modes_for_inbound_and_outbound() {
        let gw = MockGrpcGateway::new()
            .with_inbound_behavior(MockBehavior::GrpcError(GrpcStatusCode::NotFound))
            .with_outbound_behavior(MockBehavior::Success(vec![0xFF]));

        // Inbound fails
        let inbound_req = build_request("test.v1.Service/Missing", vec![]);
        let inbound_err = gw.handle_unary(inbound_req).await.unwrap_err();
        assert!(inbound_err.is_not_found());

        // Outbound succeeds
        let outbound_req = build_request("test.v1.Service/Exists", vec![]);
        let outbound_resp = gw.call_unary("remote:443", outbound_req).await.unwrap();
        assert_eq!(outbound_resp.body, vec![0xFF]);
    }
}

// =============================================================================
// 8. Empty body handling
// =============================================================================

mod empty_body_tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_unary_empty_body_returns_empty_body() {
        let gw = MockGrpcGateway::new();
        let req = build_request("test.v1.Service/Empty", vec![]);

        let resp = gw.handle_unary(req).await.unwrap();

        assert!(
            resp.body.is_empty(),
            "echo of empty request must produce empty response body"
        );
    }

    #[tokio::test]
    async fn test_call_unary_empty_body_returns_empty_body() {
        let gw = MockGrpcGateway::new();
        let req = build_request("test.v1.Service/EmptyCall", vec![]);

        let resp = gw.call_unary("remote:443", req).await.unwrap();

        assert!(resp.body.is_empty());
    }

    #[tokio::test]
    async fn test_handle_unary_empty_body_with_metadata_preserves_metadata() {
        let gw = MockGrpcGateway::new();
        let mut headers = HashMap::new();
        headers.insert("x-empty-test".to_string(), "yes".to_string());
        let req = build_request_with_metadata("test.v1.Service/EmptyWithMeta", vec![], headers);

        let resp = gw.handle_unary(req).await.unwrap();

        assert!(resp.body.is_empty());
        assert_eq!(
            resp.metadata.headers.get("x-empty-test"),
            Some(&"yes".to_string()),
            "metadata must be preserved even when body is empty"
        );
    }
}

// =============================================================================
// 9. Large payload handling
// =============================================================================

mod large_payload_tests {
    use super::*;

    /// 1 MB payload — verifies no silent truncation or overflow.
    #[tokio::test]
    async fn test_handle_unary_one_megabyte_payload_echoed_correctly() {
        let gw = MockGrpcGateway::new();
        let large_body: Vec<u8> = (0u8..=255).cycle().take(1_048_576).collect();
        let req = build_request("test.v1.Service/BigUpload", large_body.clone());

        let resp = gw.handle_unary(req).await.unwrap();

        assert_eq!(
            resp.body.len(),
            1_048_576,
            "response body length must match 1 MB input"
        );
        assert_eq!(
            resp.body, large_body,
            "response body content must match 1 MB input byte-for-byte"
        );
    }

    /// 4 MB payload through outbound call.
    #[tokio::test]
    async fn test_call_unary_four_megabyte_payload_echoed_correctly() {
        let gw = MockGrpcGateway::new();
        let large_body: Vec<u8> = vec![0xAB; 4 * 1_048_576];
        let req = build_request("test.v1.Service/BigCall", large_body.clone());

        let resp = gw.call_unary("remote:443", req).await.unwrap();

        assert_eq!(resp.body.len(), 4 * 1_048_576);
        assert_eq!(resp.body, large_body);
    }

    /// Large payload with configured success body (not echo) to verify the
    /// gateway doesn't conflate request size with response size.
    #[tokio::test]
    async fn test_handle_unary_large_request_small_response() {
        let gw = MockGrpcGateway::new()
            .with_inbound_behavior(MockBehavior::Success(vec![0x01]));
        let large_body: Vec<u8> = vec![0xFF; 2 * 1_048_576];
        let req = build_request("test.v1.Service/Compress", large_body);

        let resp = gw.handle_unary(req).await.unwrap();

        assert_eq!(
            resp.body,
            vec![0x01],
            "response body must be the configured small body, not the large request"
        );
    }

    /// Large payload still fails correctly when error behavior is configured.
    #[tokio::test]
    async fn test_handle_unary_large_payload_with_error_still_propagates_error() {
        let gw = MockGrpcGateway::new()
            .with_inbound_behavior(MockBehavior::GrpcError(GrpcStatusCode::Internal));
        let large_body: Vec<u8> = vec![0x00; 1_048_576];
        let req = build_request("test.v1.Service/FailBig", large_body);

        let err = gw.handle_unary(req).await.unwrap_err();

        assert_eq!(
            err.code(),
            GatewayErrorCode::Internal,
            "error must propagate even with a large payload"
        );
    }
}
