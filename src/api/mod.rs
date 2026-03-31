//! API module containing traits and domain types.
//!
//! This module provides the public interface for all gateways:
//! - Gateway traits (Inbound, Outbound, Combined)
//! - Domain types for each gateway
//! - Common types (errors, results, health checks)

pub mod database;
pub mod file;
pub mod grpc;
pub mod http;
pub mod input;
pub mod middleware;
pub mod notification;
pub mod output;
pub mod payment;
pub mod process;
pub mod traits;
pub mod types;

#[cfg(feature = "auth")]
pub mod auth;

// Re-export commonly used items
pub use traits::*;
pub use types::*;
