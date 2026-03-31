//! gRPC gateway domain types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Metadata for a gRPC request (headers, method info).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GrpcMetadata {
    /// Key-value headers/metadata.
    pub headers: HashMap<String, String>,
}

/// A gRPC request envelope.
#[derive(Debug, Clone)]
pub struct GrpcRequest {
    /// Fully qualified method name (e.g., "xkvm.v1.VmService/CreateVm").
    pub method: String,
    /// Serialized protobuf request body.
    pub body: Vec<u8>,
    /// Request metadata (headers).
    pub metadata: GrpcMetadata,
}

/// A gRPC response envelope.
#[derive(Debug, Clone)]
pub struct GrpcResponse {
    /// Serialized protobuf response body.
    pub body: Vec<u8>,
    /// Response metadata (trailing headers).
    pub metadata: GrpcMetadata,
}

/// A gRPC status code (mirrors tonic/gRPC standard codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrpcStatusCode {
    /// Success.
    Ok,
    /// Client cancelled.
    Cancelled,
    /// Unknown error.
    Unknown,
    /// Invalid argument.
    InvalidArgument,
    /// Deadline exceeded.
    DeadlineExceeded,
    /// Resource not found.
    NotFound,
    /// Resource already exists.
    AlreadyExists,
    /// Permission denied.
    PermissionDenied,
    /// Resource exhausted.
    ResourceExhausted,
    /// Failed precondition.
    FailedPrecondition,
    /// Aborted.
    Aborted,
    /// Out of range.
    OutOfRange,
    /// Unimplemented.
    Unimplemented,
    /// Internal error.
    Internal,
    /// Service unavailable.
    Unavailable,
    /// Data loss.
    DataLoss,
    /// Unauthenticated.
    Unauthenticated,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// @covers: GrpcRequest construction with all fields populated
    #[test]
    fn test_grpc_request_construction() {
        let mut headers = HashMap::new();
        headers.insert("authorization".to_string(), "Bearer tok".to_string());

        let req = GrpcRequest {
            method: "xkvm.v1.VmService/CreateVm".to_string(),
            body: vec![0x08, 0x01],
            metadata: GrpcMetadata { headers },
        };

        assert_eq!(req.method, "xkvm.v1.VmService/CreateVm");
        assert_eq!(req.body, vec![0x08, 0x01]);
        assert_eq!(
            req.metadata.headers.get("authorization"),
            Some(&"Bearer tok".to_string())
        );
    }

    /// @covers: GrpcMetadata::default produces empty headers
    #[test]
    fn test_grpc_metadata_default() {
        let meta = GrpcMetadata::default();
        assert!(
            meta.headers.is_empty(),
            "default metadata must have empty headers"
        );
    }

    /// @covers: GrpcStatusCode variant identity (ensures all 17 codes are distinct)
    #[test]
    fn test_grpc_status_code_variants() {
        let codes = [
            GrpcStatusCode::Ok,
            GrpcStatusCode::Cancelled,
            GrpcStatusCode::Unknown,
            GrpcStatusCode::InvalidArgument,
            GrpcStatusCode::DeadlineExceeded,
            GrpcStatusCode::NotFound,
            GrpcStatusCode::AlreadyExists,
            GrpcStatusCode::PermissionDenied,
            GrpcStatusCode::ResourceExhausted,
            GrpcStatusCode::FailedPrecondition,
            GrpcStatusCode::Aborted,
            GrpcStatusCode::OutOfRange,
            GrpcStatusCode::Unimplemented,
            GrpcStatusCode::Internal,
            GrpcStatusCode::Unavailable,
            GrpcStatusCode::DataLoss,
            GrpcStatusCode::Unauthenticated,
        ];

        // All 17 standard gRPC codes are present.
        assert_eq!(codes.len(), 17);

        // Each code is distinct from every other.
        for (i, a) in codes.iter().enumerate() {
            for (j, b) in codes.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "codes at index {i} and {j} must differ");
                }
            }
        }

        // Spot-check identity.
        assert_eq!(GrpcStatusCode::Ok, GrpcStatusCode::Ok);
        assert_ne!(GrpcStatusCode::Ok, GrpcStatusCode::Internal);
    }
}
