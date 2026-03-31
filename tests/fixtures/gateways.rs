//! Gateway factory helpers for tests.
//!
//! Wraps `saf::*` builders with test-friendly defaults and
//! provides `TempFileGateway` for isolated file operations.

use swe_gateway::saf;

/// An isolated file gateway backed by a temporary directory.
///
/// The directory is automatically cleaned up when this struct is dropped.
/// Call `.gateway()` for a fresh gateway reference each time (avoids lifetime issues).
pub struct TempFileGateway {
    dir: tempfile::TempDir,
}

impl TempFileGateway {
    /// Create a new temp directory for file gateway tests.
    pub fn new() -> Self {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        Self { dir }
    }

    /// Create a fresh `LocalFileGateway` pointing at this temp directory.
    ///
    /// Returns an owned gateway — call this wherever you need the gateway.
    pub fn gateway(&self) -> impl swe_gateway::saf::FileGateway {
        saf::local_file_gateway(self.dir.path().to_path_buf())
    }

    /// Return the temp directory path (for assertions / external inspection).
    pub fn path(&self) -> &std::path::Path {
        self.dir.path()
    }
}

/// Create an in-memory database with no tables.
pub fn memory_db() -> impl swe_gateway::saf::DatabaseGateway {
    saf::memory_database()
}

/// Create an in-memory database with the given table names pre-registered.
pub fn memory_db_with_tables(tables: &[&str]) -> impl swe_gateway::saf::DatabaseGateway {
    saf::memory_database_with_tables(tables.to_vec())
}

/// Create a silent notification gateway (no stdout output).
pub fn notifier() -> impl swe_gateway::saf::NotificationGateway {
    saf::silent_notifier()
}

/// Create a mock payment gateway.
pub fn payments() -> impl swe_gateway::saf::PaymentGateway {
    saf::mock_payment_gateway()
}

/// Create a REST client pointed at the given base URL.
pub fn http_client(base_url: impl Into<String>) -> impl swe_gateway::saf::HttpGateway {
    saf::rest_client_with_base_url(base_url)
}
