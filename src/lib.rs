//! # swe-gateway
//!
//! A gateway abstraction layer with inbound + outbound support for common integrations.
//!
//! ## Overview
//!
//! This crate provides a hexagonal architecture pattern for abstracting external dependencies
//! behind clean interfaces. Each gateway type has three traits:
//!
//! - **Inbound**: Read/query operations
//! - **Outbound**: Write/mutation operations
//! - **Gateway**: Combined trait (extends both Inbound and Outbound)
//!
//! ## Gateway Types
//!
//! | Gateway | Abstracts | Default Impl |
//! |---------|-----------|--------------|
//! | DatabaseGateway | SQL, NoSQL, in-memory | MemoryDatabase |
//! | FileGateway | Local FS, S3, GCS | LocalFileGateway |
//! | HttpGateway | REST, GraphQL, gRPC | RestClient |
//! | NotificationGateway | Email, SMS, Push | ConsoleNotifier |
//! | PaymentGateway | Stripe, PayPal, Square | MockPaymentGateway |
//!
//! ## Quick Start
//!
//! ```rust
//! use swe_gateway::prelude::*;
//! use swe_gateway::saf;
//!
//! // Create a memory database
//! let db = saf::memory_database();
//!
//! // Create a local file gateway
//! let files = saf::local_file_gateway("./data");
//!
//! // Create a mock payment gateway
//! let payments = saf::mock_payment_gateway();
//! ```
//!
//! ## Feature Flags
//!
//! Additional backends can be enabled via feature flags:
//!
//! - `postgres` - PostgreSQL database support
//! - `mysql` - MySQL/MariaDB database support
//! - `s3` - Amazon S3 file storage
//! - `email` - Email notifications via SMTP
//! - `stripe` - Stripe payment processing
//! - `full` - Enable postgres, s3, email, and stripe
//!
//! ## Implementing Custom Gateways
//!
//! Use the `spi` module to implement custom gateways:
//!
//! ```rust
//! use swe_gateway::spi::*;
//! use futures::future::BoxFuture;
//!
//! struct MyDatabase;
//!
//! impl DatabaseInbound for MyDatabase {
//!     fn query(
//!         &self,
//!         table: &str,
//!         params: database::QueryParams,
//!     ) -> BoxFuture<'_, GatewayResult<Vec<database::Record>>> {
//!         Box::pin(async move {
//!             // Your implementation here
//!             Ok(vec![])
//!         })
//!     }
//!
//!     // ... implement other methods
//!     # fn get_by_id(&self, _: &str, _: &str) -> BoxFuture<'_, GatewayResult<Option<database::Record>>> { Box::pin(async { Ok(None) }) }
//!     # fn exists(&self, _: &str, _: &str) -> BoxFuture<'_, GatewayResult<bool>> { Box::pin(async { Ok(false) }) }
//!     # fn count(&self, _: &str, _: database::QueryParams) -> BoxFuture<'_, GatewayResult<u64>> { Box::pin(async { Ok(0) }) }
//!     # fn health_check(&self) -> BoxFuture<'_, GatewayResult<HealthCheck>> { Box::pin(async { Ok(HealthCheck { status: HealthStatus::Healthy, message: None, latency_ms: None, metadata: std::collections::HashMap::new(), checked_at: chrono::Utc::now() }) }) }
//! }
//! ```

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

// ── Private layer modules (SEA: all layers are private) ──
mod api;
pub(crate) mod core;
mod provider;
mod state;

// ── Public modules ──
pub mod saf;
pub mod spi;

// ── Public surface delegated via saf (SEA rule §7) ──
pub use saf::*;

/// Prelude module with commonly used imports.
pub mod prelude {
    pub use crate::saf::*;
}
