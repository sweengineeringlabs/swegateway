//! Factory functions for creating gateway instances.
//!
//! This module provides convenience functions for creating gateway instances
//! with common configurations.

use crate::api::{
    database::DatabaseConfig,
    file::FileStorageConfig,
    http::HttpConfig,
    input::InputSource,
    notification::NotificationConfig,
    output::OutputSink,
    payment::PaymentConfig,
    traits::{
        DatabaseGateway, FileGateway, HttpGateway, NotificationGateway, PaymentGateway,
    },
};
use crate::core::{
    database::MemoryDatabase,
    file::LocalFileGateway,
    http::RestClient,
    input::LocalInputSource,
    notification::ConsoleNotifier,
    output::{ConfiguredOutputSink, FileSink, StdoutSink},
    payment::MockPaymentGateway,
};
use crate::saf::config::GatewayConfig;

// =============================================================================
// Database Gateway Builders
// =============================================================================

/// Creates an in-memory database gateway.
pub fn memory_database() -> impl DatabaseGateway {
    MemoryDatabase::new()
}

/// Creates an in-memory database with predefined tables.
pub fn memory_database_with_tables(tables: Vec<&str>) -> impl DatabaseGateway {
    MemoryDatabase::with_tables(tables)
}

// =============================================================================
// File Gateway Builders
// =============================================================================

/// Creates a local file gateway with the given base path.
pub fn local_file_gateway(base_path: impl Into<std::path::PathBuf>) -> impl FileGateway {
    LocalFileGateway::new(base_path)
}

/// Creates a local file gateway using the current directory.
pub fn local_file_gateway_current_dir() -> std::io::Result<impl FileGateway> {
    LocalFileGateway::current_dir()
}

// =============================================================================
// HTTP Gateway Builders
// =============================================================================

/// Creates a REST client with the given configuration.
pub fn rest_client(config: HttpConfig) -> impl HttpGateway {
    RestClient::new(config)
}

/// Creates a REST client with a base URL.
pub fn rest_client_with_base_url(base_url: impl Into<String>) -> impl HttpGateway {
    RestClient::with_base_url(base_url)
}

// =============================================================================
// Notification Gateway Builders
// =============================================================================

/// Creates a console notifier for development/testing.
pub fn console_notifier() -> impl NotificationGateway {
    ConsoleNotifier::new()
}

/// Creates a silent console notifier (doesn't print to stdout).
pub fn silent_notifier() -> impl NotificationGateway {
    ConsoleNotifier::silent()
}

// =============================================================================
// Payment Gateway Builders
// =============================================================================

/// Creates a mock payment gateway for testing.
pub fn mock_payment_gateway() -> impl PaymentGateway {
    MockPaymentGateway::new()
}

/// Creates a mock payment gateway with a failure mode for testing.
pub fn mock_payment_gateway_with_failure(
    mode: crate::api::types::MockFailureMode,
) -> impl PaymentGateway {
    MockPaymentGateway::new().with_failure_mode(mode)
}

// =============================================================================
// Rate Limiter Builders
// =============================================================================

/// Creates a rate limiter with the given capacity and refill rate.
///
/// - `capacity` — maximum burst size (tokens in the bucket).
/// - `refill_rate` — tokens added per second.
pub fn rate_limiter(capacity: u64, refill_rate: f64) -> crate::core::rate_limit::RateLimiter {
    crate::core::rate_limit::RateLimiter::new(capacity, refill_rate)
}

/// Returns a [`RateLimiterBuilder`](crate::core::rate_limit::RateLimiterBuilder)
/// for step-by-step configuration.
pub fn rate_limiter_builder() -> crate::core::rate_limit::RateLimiterBuilder {
    crate::core::rate_limit::RateLimiterBuilder::new()
}

// =============================================================================
// Daemon Runner Builders
// =============================================================================

/// Creates a lightweight `DaemonRunner` with observability disabled.
///
/// This is a convenience shorthand for:
/// ```ignore
/// DaemonRunner::new(service_name).without_observability()
/// ```
///
/// Useful for CLI tools, test harnesses, or any process that does not
/// need MDC logging context or the obsrv sidecar.
pub fn lightweight_daemon(service_name: impl Into<String>) -> crate::core::daemon::DaemonRunner {
    crate::core::daemon::DaemonRunner::new(service_name).without_observability()
}

// =============================================================================
// Configuration Builders
// =============================================================================

/// Creates a default database configuration for in-memory storage.
pub fn database_config_memory() -> DatabaseConfig {
    DatabaseConfig::memory()
}

/// Creates a PostgreSQL database configuration.
pub fn database_config_postgres(connection_string: impl Into<String>) -> DatabaseConfig {
    DatabaseConfig::postgres(connection_string)
}

/// Creates a local file storage configuration.
pub fn file_storage_config_local(base_path: impl Into<String>) -> FileStorageConfig {
    FileStorageConfig::local(base_path)
}

/// Creates an S3 file storage configuration.
pub fn file_storage_config_s3(bucket: impl Into<String>, region: impl Into<String>) -> FileStorageConfig {
    FileStorageConfig::s3(bucket, region)
}

/// Creates a default HTTP configuration.
pub fn http_config() -> HttpConfig {
    HttpConfig::default()
}

/// Creates an HTTP configuration with a base URL.
pub fn http_config_with_base_url(base_url: impl Into<String>) -> HttpConfig {
    HttpConfig::with_base_url(base_url)
}

/// Creates a default notification configuration (console).
pub fn notification_config() -> NotificationConfig {
    NotificationConfig::default()
}

/// Creates a default payment configuration (mock).
pub fn payment_config() -> PaymentConfig {
    PaymentConfig::default()
}

/// Creates a mock payment configuration.
pub fn payment_config_mock() -> PaymentConfig {
    PaymentConfig::mock()
}

// =============================================================================
// Input Source Builders
// =============================================================================

/// Creates a local filesystem input source with default config.
pub fn input_source() -> impl InputSource {
    LocalInputSource::new(GatewayConfig::default())
}

/// Creates an input source configured from the given `GatewayConfig`.
pub fn configured_input_source(config: GatewayConfig) -> impl InputSource {
    LocalInputSource::new(config)
}

// =============================================================================
// Output Sink Builders
// =============================================================================

/// Creates an output sink driven by `SinkConfig`.
///
/// Consumers should use this instead of `stdout_sink()` or `file_sink()` directly.
/// The dispatch is handled internally — callers only need to know about `SinkConfig`.
pub fn sink(config: &crate::saf::config::SinkConfig) -> Box<dyn OutputSink> {
    match config.sink_type {
        crate::saf::config::SinkType::File => {
            let path = config.path.clone().unwrap_or_default();
            Box::new(FileSink::new(path))
        }
        crate::saf::config::SinkType::Stdout => Box::new(StdoutSink),
    }
}

/// Creates a stdout output sink.
pub fn stdout_sink() -> impl OutputSink {
    StdoutSink
}

/// Creates a file output sink that writes to the given path.
pub fn file_sink(path: impl Into<std::path::PathBuf>) -> impl OutputSink {
    FileSink::new(path.into())
}

/// Creates a config-driven output sink that dispatches based on `gateway.toml`.
///
/// - `sink_type = "stdout"` → writes to console
/// - `sink_type = "file"` + `path = "..."` → writes to the specified file
pub fn configured_sink(config: GatewayConfig) -> impl OutputSink {
    ConfiguredOutputSink::new(config)
}

// =============================================================================
// Retry Middleware Builders
// =============================================================================

/// Creates a [`RetryMiddlewareBuilder`] with production defaults.
///
/// Defaults: 3 max attempts, exponential backoff (200ms base, jitter enabled),
/// retries only `GatewayError::is_retryable()` errors.
pub fn retry_middleware() -> crate::core::retry::RetryMiddlewareBuilder {
    crate::core::retry::RetryMiddlewareBuilder::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::database::DatabaseType;
    use crate::api::file::FileStorageType;
    use crate::api::notification::NotificationChannel;
    use crate::api::payment::PaymentProvider;
    use crate::api::types::MockFailureMode;

    /// @covers: memory_database
    #[test]
    fn test_memory_database() {
        let _db = memory_database();
    }

    /// @covers: memory_database_with_tables
    #[test]
    fn test_memory_database_with_tables() {
        let _db = memory_database_with_tables(vec!["users", "orders"]);
    }

    /// @covers: local_file_gateway
    #[test]
    fn test_local_file_gateway() {
        let _gw = local_file_gateway("/tmp");
    }

    /// @covers: local_file_gateway_current_dir
    #[test]
    fn test_local_file_gateway_current_dir() {
        let result = local_file_gateway_current_dir();
        assert!(result.is_ok(), "local_file_gateway_current_dir should succeed");
    }

    /// @covers: rest_client
    #[test]
    fn test_rest_client() {
        let _client = rest_client(HttpConfig::default());
    }

    /// @covers: rest_client_with_base_url
    #[test]
    fn test_rest_client_with_base_url() {
        let _client = rest_client_with_base_url("http://example.com");
    }

    /// @covers: console_notifier
    #[test]
    fn test_console_notifier() {
        let _n = console_notifier();
    }

    /// @covers: silent_notifier
    #[test]
    fn test_silent_notifier() {
        let _n = silent_notifier();
    }

    /// @covers: mock_payment_gateway
    #[test]
    fn test_mock_payment_gateway() {
        let _gw = mock_payment_gateway();
    }

    /// @covers: mock_payment_gateway_with_failure
    #[test]
    fn test_mock_payment_gateway_with_failure() {
        let _gw = mock_payment_gateway_with_failure(MockFailureMode::FailAllPayments("test".into()));
    }

    /// @covers: database_config_memory
    #[test]
    fn test_database_config_memory() {
        let c = database_config_memory();
        assert_eq!(c.database_type, DatabaseType::Memory);
    }

    /// @covers: database_config_postgres
    #[test]
    fn test_database_config_postgres() {
        let c = database_config_postgres("postgres://localhost/test");
        assert_eq!(c.database_type, DatabaseType::Postgres);
        assert_eq!(c.connection_string, Some("postgres://localhost/test".into()));
    }

    /// @covers: file_storage_config_local
    #[test]
    fn test_file_storage_config_local() {
        let c = file_storage_config_local("/data");
        assert_eq!(c.storage_type, FileStorageType::Local);
    }

    /// @covers: file_storage_config_s3
    #[test]
    fn test_file_storage_config_s3() {
        let c = file_storage_config_s3("bucket", "us-east-1");
        assert_eq!(c.storage_type, FileStorageType::S3);
        assert_eq!(c.region, Some("us-east-1".into()));
    }

    /// @covers: http_config
    #[test]
    fn test_http_config() {
        let c = http_config();
        assert_eq!(c.timeout_secs, 30);
    }

    /// @covers: http_config_with_base_url
    #[test]
    fn test_http_config_with_base_url() {
        let c = http_config_with_base_url("http://api.example.com");
        assert_eq!(c.base_url, Some("http://api.example.com".into()));
    }

    /// @covers: notification_config
    #[test]
    fn test_notification_config() {
        let c = notification_config();
        assert_eq!(c.default_channel, NotificationChannel::Console);
    }

    /// @covers: payment_config
    #[test]
    fn test_payment_config() {
        let c = payment_config();
        assert!(c.sandbox);
    }

    /// @covers: payment_config_mock
    #[test]
    fn test_payment_config_mock() {
        let c = payment_config_mock();
        assert_eq!(c.provider, PaymentProvider::Mock);
    }
}
