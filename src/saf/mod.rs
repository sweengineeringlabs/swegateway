//! Service Abstraction Framework (SAF) module.
//!
//! This module is the **only** public surface of the crate.
//! All public types, traits, and factory functions are re-exported here.

pub mod builders;
pub mod config;

pub use builders::*;
pub use config::{
    expand_env_vars, load_config, load_config_from, load_config_from_str, ConfigError,
    GatewayConfig, SinkConfig, SinkFormat, SinkType,
};

// ── Unified process gateway (from api layer) ──
pub use crate::api::process::{
    Gateway, InputRequest, OutputResponse, PipelineGateway, PipelineReq, PipelineResp,
    ProcessStatus, RequestMetadata, ResponseMetadata,
};

// ── Input/output traits (from api layer) ──
pub use crate::api::input::InputSource;
pub use crate::api::output::OutputSink;

// ── Gateway traits (from api layer) ──
pub use crate::api::traits::DatabaseGateway;
pub use crate::api::traits::DatabaseInbound;
pub use crate::api::traits::DatabaseOutbound;
pub use crate::api::traits::FileGateway;
pub use crate::api::traits::FileInbound;
pub use crate::api::traits::FileOutbound;
pub use crate::api::traits::HttpGateway;
pub use crate::api::traits::HttpInbound;
pub use crate::api::traits::HttpOutbound;
pub use crate::api::traits::NotificationGateway;
pub use crate::api::traits::NotificationInbound;
pub use crate::api::traits::NotificationOutbound;
pub use crate::api::traits::PaymentGateway;
pub use crate::api::traits::PaymentInbound;
pub use crate::api::traits::PaymentOutbound;
pub use crate::api::traits::GrpcGateway;
pub use crate::api::traits::GrpcInbound;
pub use crate::api::traits::GrpcOutbound;

// ── Middleware traits (from api layer) ──
pub use crate::api::middleware::MiddlewareAction;
pub use crate::api::middleware::RequestMiddleware;
pub use crate::api::middleware::ResponseMiddleware;

// ── Daemon runner (from core layer) ──
pub use crate::core::daemon::{DaemonContext, DaemonRunner};

// ── Retry middleware (from core layer) ──
pub use crate::core::retry::{
    BackoffStrategy, RetryMiddleware, RetryMiddlewareBuilder, RetryMiddlewareSpec, RetryPredicate,
};

// ── Rate limiter (from core layer) ──
pub use crate::core::rate_limit::{RateLimiter, RateLimiterBuilder};

// ── Pipeline (from core layer) ──
pub use crate::core::pipeline::{PipelineRouter, DefaultPipeline, Pipeline, Router};
pub use crate::core::metrics_bridge::{
    FieldExtractor, MetricFields, MetricsCollector, MetricsResponseMiddleware,
};

// ── Common types (from api layer) ──
pub use crate::api::types::GatewayError;
pub use crate::api::types::GatewayErrorCode;
pub use crate::api::types::GatewayResult;
pub use crate::api::types::HealthCheck;
pub use crate::api::types::HealthStatus;
pub use crate::api::types::IntoGatewayError;
pub use crate::api::types::MockFailureMode;
pub use crate::api::types::PaginatedResponse;
pub use crate::api::types::Pagination;
pub use crate::api::types::ResultGatewayExt;

// ── Domain types (from api layer) ──
pub mod database {
    //! Database domain types.
    pub use crate::api::database::*;
}

pub mod file {
    //! File domain types.
    pub use crate::api::file::*;
}

pub mod http {
    //! HTTP domain types.
    pub use crate::api::http::*;
}

pub mod notification {
    //! Notification domain types.
    pub use crate::api::notification::*;
}

pub mod payment {
    //! Payment domain types.
    pub use crate::api::payment::*;
}

pub mod grpc {
    //! gRPC domain types.
    pub use crate::api::grpc::*;
}

// ── Provider traits ──
pub use crate::provider::{LazyInit, LazyInitWithConfig, StatefulProvider, StatelessProvider};

// ── State management ──
pub use crate::state::{CachedService, ConfiguredCache};

// ── Async-to-sync bridge ──

/// Run an async future synchronously on a shared single-threaded tokio runtime.
///
/// This is the canonical async→sync bridge for consumer crates that use
/// `OutputSink` or other async gateway traits from synchronous code.
/// The runtime is created once and reused for the lifetime of the process.
pub fn block_on<F: std::future::Future>(f: F) -> F::Output {
    use std::sync::OnceLock;
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let rt = RT.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create swe-gateway tokio runtime")
    });
    rt.block_on(f)
}

// ── Streaming support ──
/// A boxed async stream of gateway results.
pub type GatewayStream<'a, T> =
    std::pin::Pin<Box<dyn futures::stream::Stream<Item = GatewayResult<T>> + Send + 'a>>;
pub use futures::stream::Stream;
pub use futures::stream::StreamExt;

// ── Async trait re-export ──
pub use async_trait::async_trait;

// ── Auth (sst-sdk backed) ──
#[cfg(feature = "auth")]
pub use crate::api::auth::{AuthClaims, CredentialExtractor};
#[cfg(feature = "auth")]
pub use crate::core::auth_middleware::AuthMiddleware;
#[cfg(feature = "auth")]
pub use sst_sdk::{Authenticator, Authorizer, Credentials, AuthnResult, AuthContext, Permission, AuthResult};
