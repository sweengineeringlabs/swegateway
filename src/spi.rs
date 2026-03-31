//! Service Provider Interface (SPI) module.
//!
//! This module re-exports the gateway traits for extension purposes.
//! Implement these traits to create custom gateway adapters.

// Re-export the unified process gateway trait and envelope types
pub use crate::api::process::{
    Gateway, InputRequest, OutputResponse, PipelineGateway, PipelineReq, PipelineResp,
    ProcessStatus, RequestMetadata, ResponseMetadata,
};

// Re-export all gateway traits
pub use crate::api::traits::{
    // Database
    DatabaseGateway,
    DatabaseInbound,
    DatabaseOutbound,
    // File
    FileGateway,
    FileInbound,
    FileOutbound,
    // HTTP
    HttpGateway,
    HttpInbound,
    HttpOutbound,
    // Notification
    NotificationGateway,
    NotificationInbound,
    NotificationOutbound,
    // Payment
    PaymentGateway,
    PaymentInbound,
    PaymentOutbound,
    // gRPC
    GrpcGateway,
    GrpcInbound,
    GrpcOutbound,
};

// Re-export middleware traits
pub use crate::api::middleware::{RequestMiddleware, ResponseMiddleware};

// Re-export common types needed for implementations
pub use crate::api::types::{
    GatewayError, GatewayErrorCode, GatewayResult, HealthCheck, HealthStatus,
    IntoGatewayError, ResultGatewayExt,
};

// Re-export domain types for each gateway
pub mod database {
    pub use crate::api::database::*;
}

pub mod file {
    pub use crate::api::file::*;
}

pub mod http {
    pub use crate::api::http::*;
}

pub mod notification {
    pub use crate::api::notification::*;
}

pub mod payment {
    pub use crate::api::payment::*;
}

pub mod grpc {
    pub use crate::api::grpc::*;
}

#[cfg(test)]
mod tests {
    // Trait-only module; tested via integration tests.
}
