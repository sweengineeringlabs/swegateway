//! Core module containing gateway implementations.
//!
//! This module provides default implementations for all gateways:
//! - MemoryDatabase: In-memory database for testing
//! - LocalFileGateway: Local filesystem storage
//! - RestClient: HTTP REST client
//! - ConsoleNotifier: Console output for notifications
//! - MockPaymentGateway: Mock payment processing

pub(crate) mod daemon;
pub(crate) mod database;
pub(crate) mod file;
pub(crate) mod http;
pub(crate) mod input;
pub(crate) mod metrics_bridge;
pub(crate) mod notification;
pub(crate) mod output;
pub(crate) mod payment;
pub(crate) mod pipeline;
pub(crate) mod rate_limit;
pub(crate) mod retry;

#[cfg(feature = "auth")]
pub(crate) mod auth_middleware;

// Re-export default implementations (crate-internal only)
pub(crate) use database::MemoryDatabase;
pub(crate) use file::LocalFileGateway;
pub(crate) use http::RestClient;
pub(crate) use notification::ConsoleNotifier;
pub(crate) use input::LocalInputSource;
pub(crate) use output::{ConfiguredOutputSink, FileSink, StdoutSink};
pub(crate) use payment::{MockFailureMode, MockPaymentGateway};

