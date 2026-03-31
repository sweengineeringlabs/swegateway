//! Core types for the gateway abstraction layer.
//!
//! This module provides common types used across all gateways including
//! error handling, result types, and health check structures.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Result type for gateway operations.
pub type GatewayResult<T> = Result<T, GatewayError>;

/// Standard error codes for gateway operations.
///
/// These codes provide a consistent way to categorize errors across
/// all gateway implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayErrorCode {
    /// Internal error (unexpected failure)
    Internal,
    /// Invalid input from caller
    InvalidInput,
    /// Resource not found
    NotFound,
    /// Resource already exists
    AlreadyExists,
    /// Permission denied
    PermissionDenied,
    /// Operation timed out
    Timeout,
    /// Service unavailable
    Unavailable,
    /// Configuration error
    Configuration,
}

/// Comprehensive error type for gateway operations.
#[derive(Debug, Error)]
pub enum GatewayError {
    /// Connection to the backend failed.
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    /// Authentication or authorization error.
    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),

    /// The requested resource was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// The operation conflicts with existing state.
    #[error("conflict: {0}")]
    Conflict(String),

    /// Input validation failed.
    #[error("validation error: {0}")]
    ValidationError(String),

    /// Rate limit exceeded.
    #[error("rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    /// Operation timed out.
    #[error("timeout: {0}")]
    Timeout(String),

    /// The gateway or operation is not supported.
    #[error("not supported: {0}")]
    NotSupported(String),

    /// I/O error during operation.
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    /// Serialization/deserialization error.
    #[error("serialization error: {0}")]
    SerializationError(String),

    /// Backend-specific error.
    #[error("backend error: {0}")]
    BackendError(String),

    /// Internal gateway error.
    #[error("internal error: {0}")]
    InternalError(String),

    /// Resource already exists.
    #[error("already exists: {0}")]
    AlreadyExists(String),

    /// Permission denied.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// Service unavailable.
    #[error("unavailable: {0}")]
    Unavailable(String),

    /// Configuration error.
    #[error("configuration error: {0}")]
    Configuration(String),
}

impl GatewayError {
    /// Create a new gateway error from a code and message.
    ///
    /// Maps `GatewayErrorCode` to the corresponding enum variant.
    pub fn new(code: GatewayErrorCode, message: impl Into<String>) -> Self {
        let msg = message.into();
        match code {
            GatewayErrorCode::Internal => GatewayError::InternalError(msg),
            GatewayErrorCode::InvalidInput => GatewayError::ValidationError(msg),
            GatewayErrorCode::NotFound => GatewayError::NotFound(msg),
            GatewayErrorCode::AlreadyExists => GatewayError::AlreadyExists(msg),
            GatewayErrorCode::PermissionDenied => GatewayError::PermissionDenied(msg),
            GatewayErrorCode::Timeout => GatewayError::Timeout(msg),
            GatewayErrorCode::Unavailable => GatewayError::Unavailable(msg),
            GatewayErrorCode::Configuration => GatewayError::Configuration(msg),
        }
    }

    /// Create an internal error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(GatewayErrorCode::Internal, message)
    }

    /// Create a not found error.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(GatewayErrorCode::NotFound, message)
    }

    /// Create an invalid input error.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(GatewayErrorCode::InvalidInput, message)
    }

    /// Create an unavailable error.
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(GatewayErrorCode::Unavailable, message)
    }

    /// Create an already exists error.
    pub fn already_exists(message: impl Into<String>) -> Self {
        Self::new(GatewayErrorCode::AlreadyExists, message)
    }

    /// Create a permission denied error.
    pub fn permission_denied(message: impl Into<String>) -> Self {
        Self::new(GatewayErrorCode::PermissionDenied, message)
    }

    /// Create a timeout error.
    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(GatewayErrorCode::Timeout, message)
    }

    /// Create a configuration error.
    pub fn configuration(message: impl Into<String>) -> Self {
        Self::new(GatewayErrorCode::Configuration, message)
    }

    /// Add details to the error by appending `[details]` to the inner string.
    pub fn with_details(self, details: impl Into<String>) -> Self {
        let details = details.into();
        match self {
            GatewayError::ConnectionFailed(m) => GatewayError::ConnectionFailed(format!("{m} [{details}]")),
            GatewayError::AuthenticationFailed(m) => GatewayError::AuthenticationFailed(format!("{m} [{details}]")),
            GatewayError::NotFound(m) => GatewayError::NotFound(format!("{m} [{details}]")),
            GatewayError::Conflict(m) => GatewayError::Conflict(format!("{m} [{details}]")),
            GatewayError::ValidationError(m) => GatewayError::ValidationError(format!("{m} [{details}]")),
            GatewayError::RateLimitExceeded(m) => GatewayError::RateLimitExceeded(format!("{m} [{details}]")),
            GatewayError::Timeout(m) => GatewayError::Timeout(format!("{m} [{details}]")),
            GatewayError::NotSupported(m) => GatewayError::NotSupported(format!("{m} [{details}]")),
            GatewayError::IoError(e) => GatewayError::InternalError(format!("io error: {e} [{details}]")),
            GatewayError::SerializationError(m) => GatewayError::SerializationError(format!("{m} [{details}]")),
            GatewayError::BackendError(m) => GatewayError::BackendError(format!("{m} [{details}]")),
            GatewayError::InternalError(m) => GatewayError::InternalError(format!("{m} [{details}]")),
            GatewayError::AlreadyExists(m) => GatewayError::AlreadyExists(format!("{m} [{details}]")),
            GatewayError::PermissionDenied(m) => GatewayError::PermissionDenied(format!("{m} [{details}]")),
            GatewayError::Unavailable(m) => GatewayError::Unavailable(format!("{m} [{details}]")),
            GatewayError::Configuration(m) => GatewayError::Configuration(format!("{m} [{details}]")),
        }
    }

    /// Get the error code for this error.
    pub fn code(&self) -> GatewayErrorCode {
        match self {
            GatewayError::InternalError(_) | GatewayError::BackendError(_) | GatewayError::IoError(_) => GatewayErrorCode::Internal,
            GatewayError::ValidationError(_) | GatewayError::SerializationError(_) => GatewayErrorCode::InvalidInput,
            GatewayError::NotFound(_) => GatewayErrorCode::NotFound,
            GatewayError::AlreadyExists(_) | GatewayError::Conflict(_) => GatewayErrorCode::AlreadyExists,
            GatewayError::PermissionDenied(_) | GatewayError::AuthenticationFailed(_) => GatewayErrorCode::PermissionDenied,
            GatewayError::Timeout(_) => GatewayErrorCode::Timeout,
            GatewayError::Unavailable(_) | GatewayError::ConnectionFailed(_) | GatewayError::RateLimitExceeded(_) => GatewayErrorCode::Unavailable,
            GatewayError::Configuration(_) | GatewayError::NotSupported(_) => GatewayErrorCode::Configuration,
        }
    }

    /// Returns true if this error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            GatewayError::ConnectionFailed(_)
                | GatewayError::RateLimitExceeded(_)
                | GatewayError::Timeout(_)
                | GatewayError::Unavailable(_)
        )
    }

    /// Returns true if this error indicates the resource doesn't exist.
    pub fn is_not_found(&self) -> bool {
        matches!(self, GatewayError::NotFound(_))
    }
}

/// Trait for converting domain errors to gateway errors.
///
/// Implement this trait for your domain error types to enable
/// automatic conversion using the `?` operator.
pub trait IntoGatewayError {
    /// Convert to a gateway error.
    fn into_gateway_error(self) -> GatewayError;
}

/// Extension trait for mapping errors to gateway errors.
pub trait ResultGatewayExt<T> {
    /// Map the error to a gateway error with additional context.
    fn gateway_err(self, context: impl Into<String>) -> GatewayResult<T>;

    /// Log and return the error.
    fn log_error(self, operation: &str) -> Self;
}

impl<T, E: std::error::Error> ResultGatewayExt<T> for Result<T, E> {
    fn gateway_err(self, context: impl Into<String>) -> GatewayResult<T> {
        self.map_err(|e| GatewayError::internal(context).with_details(e.to_string()))
    }

    fn log_error(self, operation: &str) -> Self {
        if let Err(ref e) = self {
            tracing::error!(
                operation = %operation,
                error = %e,
                "Gateway operation failed"
            );
        }
        self
    }
}

/// Modes for simulating payment failures in tests.
#[derive(Debug, Clone)]
pub enum MockFailureMode {
    /// Fail all payments.
    FailAllPayments(String),
    /// Fail payments over a certain amount.
    FailOverAmount(i64),
    /// Fail specific payment IDs.
    FailPaymentIds(Vec<String>),
}

/// Health status of a gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    /// Gateway is fully operational.
    Healthy,
    /// Gateway is operational but experiencing issues.
    Degraded,
    /// Gateway is not operational.
    Unhealthy,
    /// Health status is unknown.
    Unknown,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Health check result for a gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Overall health status.
    pub status: HealthStatus,
    /// Human-readable message about the health status.
    pub message: Option<String>,
    /// Latency of the health check in milliseconds.
    pub latency_ms: Option<u64>,
    /// Additional metadata about the health check.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    /// Timestamp of the health check.
    pub checked_at: chrono::DateTime<chrono::Utc>,
}

impl HealthCheck {
    /// Creates a healthy status.
    pub fn healthy() -> Self {
        Self {
            status: HealthStatus::Healthy,
            message: None,
            latency_ms: None,
            metadata: HashMap::new(),
            checked_at: chrono::Utc::now(),
        }
    }

    /// Creates a healthy status with latency.
    pub fn healthy_with_latency(latency_ms: u64) -> Self {
        Self {
            status: HealthStatus::Healthy,
            message: None,
            latency_ms: Some(latency_ms),
            metadata: HashMap::new(),
            checked_at: chrono::Utc::now(),
        }
    }

    /// Creates an unhealthy status.
    pub fn unhealthy(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Unhealthy,
            message: Some(message.into()),
            latency_ms: None,
            metadata: HashMap::new(),
            checked_at: chrono::Utc::now(),
        }
    }

    /// Creates a degraded status.
    pub fn degraded(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Degraded,
            message: Some(message.into()),
            latency_ms: None,
            metadata: HashMap::new(),
            checked_at: chrono::Utc::now(),
        }
    }

    /// Adds metadata to the health check.
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

/// Pagination parameters for list operations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Pagination {
    /// Number of items to skip.
    pub offset: usize,
    /// Maximum number of items to return.
    pub limit: usize,
}

impl Pagination {
    /// Creates a new pagination with the given offset and limit.
    pub fn new(offset: usize, limit: usize) -> Self {
        Self { offset, limit }
    }

    /// Creates pagination for the first page with the given limit.
    pub fn first(limit: usize) -> Self {
        Self { offset: 0, limit }
    }
}

/// A paginated response containing items and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    /// The items in this page.
    pub items: Vec<T>,
    /// Total number of items across all pages.
    pub total: usize,
    /// The offset used for this page.
    pub offset: usize,
    /// The limit used for this page.
    pub limit: usize,
    /// Whether there are more items after this page.
    pub has_more: bool,
}

impl<T> PaginatedResponse<T> {
    /// Creates a new paginated response.
    pub fn new(items: Vec<T>, total: usize, offset: usize, limit: usize) -> Self {
        let has_more = offset + items.len() < total;
        Self {
            items,
            total,
            offset,
            limit,
            has_more,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// @covers: is_retryable
    #[test]
    fn test_is_retryable() {
        assert!(GatewayError::ConnectionFailed("test".into()).is_retryable());
        assert!(GatewayError::RateLimitExceeded("test".into()).is_retryable());
        assert!(GatewayError::Timeout("test".into()).is_retryable());
        assert!(GatewayError::Unavailable("test".into()).is_retryable());
        assert!(!GatewayError::NotFound("test".into()).is_retryable());
        assert!(!GatewayError::ValidationError("test".into()).is_retryable());
    }

    /// @covers: is_not_found
    #[test]
    fn test_is_not_found() {
        assert!(GatewayError::NotFound("x".into()).is_not_found());
        assert!(!GatewayError::InternalError("x".into()).is_not_found());
    }

    #[test]
    fn test_new() {
        let err = GatewayError::new(GatewayErrorCode::InvalidInput, "bad");
        assert_eq!(err.code(), GatewayErrorCode::InvalidInput);
        assert!(err.to_string().contains("bad"));
    }

    /// @covers: internal
    #[test]
    fn test_internal() {
        let err = GatewayError::internal("test error");
        assert_eq!(err.code(), GatewayErrorCode::Internal);
        assert!(err.to_string().contains("test error"));
    }

    /// @covers: not_found
    #[test]
    fn test_not_found() {
        let err = GatewayError::not_found("resource");
        assert_eq!(err.code(), GatewayErrorCode::NotFound);
    }

    /// @covers: invalid_input
    #[test]
    fn test_invalid_input() {
        let err = GatewayError::invalid_input("bad input");
        assert_eq!(err.code(), GatewayErrorCode::InvalidInput);
    }

    /// @covers: unavailable
    #[test]
    fn test_unavailable() {
        let err = GatewayError::unavailable("service down");
        assert_eq!(err.code(), GatewayErrorCode::Unavailable);
    }

    /// @covers: already_exists
    #[test]
    fn test_already_exists() {
        let err = GatewayError::already_exists("duplicate");
        assert_eq!(err.code(), GatewayErrorCode::AlreadyExists);
    }

    /// @covers: permission_denied
    #[test]
    fn test_permission_denied() {
        let err = GatewayError::permission_denied("forbidden");
        assert_eq!(err.code(), GatewayErrorCode::PermissionDenied);
    }

    /// @covers: timeout
    #[test]
    fn test_timeout() {
        let err = GatewayError::timeout("took too long");
        assert_eq!(err.code(), GatewayErrorCode::Timeout);
    }

    /// @covers: configuration
    #[test]
    fn test_configuration() {
        let err = GatewayError::configuration("bad config");
        assert_eq!(err.code(), GatewayErrorCode::Configuration);
    }

    /// @covers: with_details
    #[test]
    fn test_with_details() {
        let err = GatewayError::not_found("resource").with_details("id=123");
        assert!(err.to_string().contains("resource"));
        assert!(err.to_string().contains("[id=123]"));
    }

    /// @covers: code
    #[test]
    fn test_code() {
        assert_eq!(GatewayError::InternalError("x".into()).code(), GatewayErrorCode::Internal);
        assert_eq!(GatewayError::NotFound("x".into()).code(), GatewayErrorCode::NotFound);
        assert_eq!(GatewayError::Conflict("x".into()).code(), GatewayErrorCode::AlreadyExists);
        assert_eq!(GatewayError::ConnectionFailed("x".into()).code(), GatewayErrorCode::Unavailable);
        assert_eq!(GatewayError::NotSupported("x".into()).code(), GatewayErrorCode::Configuration);
    }

    #[test]
    fn test_gateway_err() {
        let result: Result<(), std::io::Error> =
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"));

        let gateway_result = result.gateway_err("Failed to read file");
        assert!(gateway_result.is_err());

        let err = gateway_result.unwrap_err();
        assert_eq!(err.code(), GatewayErrorCode::Internal);
    }

    #[test]
    fn test_log_error() {
        let result: Result<(), std::io::Error> =
            Err(std::io::Error::new(std::io::ErrorKind::Other, "disk full"));
        let result = result.log_error("write_file");
        assert!(result.is_err());

        let ok_result: Result<i32, std::io::Error> = Ok(42);
        let ok_result = ok_result.log_error("read_file");
        assert_eq!(ok_result.unwrap(), 42);
    }

    /// @covers: healthy
    #[test]
    fn test_healthy() {
        let h = HealthCheck::healthy();
        assert_eq!(h.status, HealthStatus::Healthy);
        assert!(h.message.is_none());
    }

    /// @covers: healthy_with_latency
    #[test]
    fn test_healthy_with_latency() {
        let h = HealthCheck::healthy_with_latency(42);
        assert_eq!(h.status, HealthStatus::Healthy);
        assert_eq!(h.latency_ms, Some(42));
    }

    /// @covers: unhealthy
    #[test]
    fn test_unhealthy() {
        let h = HealthCheck::unhealthy("connection failed");
        assert_eq!(h.status, HealthStatus::Unhealthy);
        assert_eq!(h.message, Some("connection failed".to_string()));
    }

    /// @covers: degraded
    #[test]
    fn test_degraded() {
        let h = HealthCheck::degraded("high latency");
        assert_eq!(h.status, HealthStatus::Degraded);
    }

    /// @covers: with_metadata
    #[test]
    fn test_with_metadata() {
        let h = HealthCheck::healthy().with_metadata("version", serde_json::json!("1.0"));
        assert_eq!(h.metadata.get("version"), Some(&serde_json::json!("1.0")));
    }

    #[test]
    fn test_pagination_new() {
        let p = Pagination::new(10, 25);
        assert_eq!(p.offset, 10);
        assert_eq!(p.limit, 25);
    }

    /// @covers: first
    #[test]
    fn test_pagination_first() {
        let p = Pagination::first(50);
        assert_eq!(p.offset, 0);
        assert_eq!(p.limit, 50);
    }

    #[test]
    fn test_paginated_response_new() {
        let response: PaginatedResponse<i32> = PaginatedResponse::new(vec![1, 2, 3], 10, 0, 3);
        assert!(response.has_more);
        assert_eq!(response.total, 10);

        let last_page: PaginatedResponse<i32> = PaginatedResponse::new(vec![10], 10, 9, 3);
        assert!(!last_page.has_more);
    }
}
