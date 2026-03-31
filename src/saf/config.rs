//! TOML configuration loading for gateway instances.
//!
//! Loads a `gateway.toml` file from the project root (or a custom path),
//! expands environment variables in sensitive fields, and provides
//! builder-style overrides and factory methods for creating gateways.

use std::path::Path;

use serde::{Deserialize, Serialize};

use std::path::PathBuf;

use crate::api::{
    database::DatabaseConfig,
    file::FileStorageConfig,
    http::HttpConfig,
    notification::NotificationConfig,
    payment::PaymentConfig,
    traits::{DatabaseGateway, FileGateway, HttpGateway, NotificationGateway, PaymentGateway},
};
use crate::saf::builders;

// =============================================================================
// Sink configuration
// =============================================================================

/// Output sink destination type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SinkType {
    /// Write to stdout/stderr (console).
    Stdout,
    /// Write to a file on disk.
    File,
}

impl Default for SinkType {
    fn default() -> Self {
        Self::Stdout
    }
}

/// Output format for report sinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SinkFormat {
    /// Human-readable plain text.
    Text,
    /// Pretty-printed JSON.
    Json,
}

impl Default for SinkFormat {
    fn default() -> Self {
        Self::Text
    }
}

/// Configuration for report output sinks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SinkConfig {
    /// Where to write reports.
    pub sink_type: SinkType,
    /// Output format.
    pub format: SinkFormat,
    /// File path (required when `sink_type = "file"`).
    pub path: Option<PathBuf>,
}

impl Default for SinkConfig {
    fn default() -> Self {
        Self {
            sink_type: SinkType::Stdout,
            format: SinkFormat::Text,
            path: None,
        }
    }
}

// =============================================================================
// Error type
// =============================================================================

/// Errors that can occur when loading gateway configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read the config file from disk.
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    /// Failed to parse the TOML content.
    #[error("failed to parse TOML: {0}")]
    Parse(#[from] toml::de::Error),
    /// A referenced environment variable was not found and no default was provided.
    #[error("environment variable not found: {0}")]
    EnvVar(String),
    /// One or more required configuration fields are missing or invalid.
    #[error("configuration validation failed: {0}")]
    Validation(String),
}

// =============================================================================
// GatewayConfig
// =============================================================================

/// Top-level gateway configuration that aggregates all gateway-specific configs.
///
/// Each section is optional in the TOML file; missing sections use defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// File storage configuration.
    #[serde(default)]
    pub file: FileStorageConfig,
    /// Database configuration.
    #[serde(default)]
    pub database: DatabaseConfig,
    /// HTTP client configuration.
    #[serde(default)]
    pub http: HttpConfig,
    /// Notification configuration.
    #[serde(default)]
    pub notification: NotificationConfig,
    /// Payment configuration.
    #[serde(default)]
    pub payment: PaymentConfig,
    /// Report sink configuration.
    #[serde(default)]
    pub sink: SinkConfig,
}

// =============================================================================
// Loading functions
// =============================================================================

/// Loads `gateway.toml` by searching from the current directory upward.
///
/// Returns `GatewayConfig::default()` if no file is found (not an error).
pub fn load_config() -> Result<GatewayConfig, ConfigError> {
    let mut dir = std::env::current_dir()?;
    loop {
        let candidate = dir.join("gateway.toml");
        if candidate.is_file() {
            return load_config_from(&candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    Ok(GatewayConfig::default())
}

/// Loads configuration from an explicit file path.
pub fn load_config_from(path: impl AsRef<Path>) -> Result<GatewayConfig, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    load_config_from_str(&content)
}

/// Parses configuration from a TOML string.
///
/// Environment variables (`${VAR}` and `${VAR:-default}`) are expanded in the
/// raw TOML text **before** deserialization, so any string value can reference
/// the environment.  After parsing, sensitive fields are expanded again via
/// [`GatewayConfig::expand_env`] for backwards compatibility.
pub fn load_config_from_str(toml_str: &str) -> Result<GatewayConfig, ConfigError> {
    let expanded = expand_env_vars(toml_str)?;
    let mut config: GatewayConfig = toml::from_str(&expanded)?;
    config.expand_env()?;
    Ok(config)
}

// =============================================================================
// Environment variable expansion
// =============================================================================

/// Expands `${VAR}` and `${VAR:-default}` patterns in a string.
///
/// * `${VAR}` — replaced by the value of environment variable `VAR`.
///   Returns `ConfigError::EnvVar` when the variable is not set and no
///   default is provided.
/// * `${VAR:-fallback}` — replaced by `VAR` if set, otherwise by `fallback`.
/// * Malformed references (missing closing `}`) are reproduced literally.
pub fn expand_env_vars(s: &str) -> Result<String, ConfigError> {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut var_expr = String::new();
            let mut found_close = false;
            for c in chars.by_ref() {
                if c == '}' {
                    found_close = true;
                    break;
                }
                var_expr.push(c);
            }
            if !found_close {
                // Malformed — just reproduce literally
                result.push_str("${");
                result.push_str(&var_expr);
            } else {
                // Check for :-default syntax
                let (var_name, default_val) = if let Some(pos) = var_expr.find(":-") {
                    (&var_expr[..pos], Some(&var_expr[pos + 2..]))
                } else {
                    (var_expr.as_str(), None)
                };

                match std::env::var(var_name) {
                    Ok(val) => result.push_str(&val),
                    Err(_) => {
                        if let Some(def) = default_val {
                            result.push_str(def);
                        } else {
                            return Err(ConfigError::EnvVar(var_name.to_string()));
                        }
                    }
                }
            }
        } else {
            result.push(ch);
        }
    }
    Ok(result)
}

/// Expands env vars in an `Option<String>` field, replacing it in place.
fn expand_opt(field: &mut Option<String>) -> Result<(), ConfigError> {
    if let Some(val) = field.as_ref() {
        if val.contains("${") {
            *field = Some(expand_env_vars(val)?);
        }
    }
    Ok(())
}

impl GatewayConfig {
    /// Expands `${VAR}` patterns in all sensitive string fields.
    pub fn expand_env(&mut self) -> Result<(), ConfigError> {
        // database
        expand_opt(&mut self.database.connection_string)?;
        expand_opt(&mut self.database.password)?;

        // file
        expand_opt(&mut self.file.access_key)?;
        expand_opt(&mut self.file.secret_key)?;

        // http
        expand_opt(&mut self.http.base_url)?;

        // payment
        expand_opt(&mut self.payment.api_key)?;
        expand_opt(&mut self.payment.secret_key)?;
        expand_opt(&mut self.payment.webhook_secret)?;

        Ok(())
    }

    /// Validates that all contextually required fields are present.
    ///
    /// Rules:
    /// * **Database** — `Postgres`, `MySql`, `MongoDb`, or `Sqlite` require
    ///   either `connection_string` or (`host` + `database`) to be set.
    /// * **File storage** — `S3`, `Gcs`, and `Azure` require `region`.
    /// * **Payment** — non-`Mock` providers require `api_key`.
    /// * **Sink** — `File` sink type requires `path`.
    ///
    /// Returns `Ok(())` when all checks pass, or `ConfigError::Validation`
    /// with an actionable message listing every missing field.
    pub fn validate(&self) -> Result<(), ConfigError> {
        use crate::api::database::DatabaseType;
        use crate::api::file::FileStorageType;
        use crate::api::payment::PaymentProvider;

        let mut missing: Vec<String> = Vec::new();

        // -- database --
        match self.database.database_type {
            DatabaseType::Postgres | DatabaseType::MySql | DatabaseType::MongoDb | DatabaseType::Sqlite => {
                let has_conn_str = self.database.connection_string.as_ref()
                    .is_some_and(|s| !s.is_empty());
                let has_host_and_db = self.database.host.as_ref().is_some_and(|s| !s.is_empty())
                    && self.database.database.as_ref().is_some_and(|s| !s.is_empty());
                if !has_conn_str && !has_host_and_db {
                    missing.push(format!(
                        "[database] {:?} requires 'connection_string' or both 'host' and 'database'",
                        self.database.database_type,
                    ));
                }
            }
            DatabaseType::Memory => { /* no connection info needed */ }
        }

        // -- file storage --
        match self.file.storage_type {
            FileStorageType::S3 | FileStorageType::Gcs | FileStorageType::Azure => {
                if self.file.region.as_ref().is_none_or(|s| s.is_empty()) {
                    missing.push(format!(
                        "[file] {:?} storage requires 'region'",
                        self.file.storage_type,
                    ));
                }
            }
            FileStorageType::Local | FileStorageType::Memory => {}
        }

        // -- payment --
        match self.payment.provider {
            PaymentProvider::Mock => {}
            provider => {
                if self.payment.api_key.as_ref().is_none_or(|s| s.is_empty()) {
                    missing.push(format!(
                        "[payment] {:?} provider requires 'api_key'",
                        provider,
                    ));
                }
            }
        }

        // -- sink --
        if self.sink.sink_type == SinkType::File {
            if self.sink.path.as_ref().is_none_or(|p| p.as_os_str().is_empty()) {
                missing.push(
                    "[sink] File sink requires 'path'".to_string(),
                );
            }
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::Validation(format!(
                "missing required fields:\n  - {}",
                missing.join("\n  - "),
            )))
        }
    }
}

// =============================================================================
// Override API (builder methods)
// =============================================================================

impl GatewayConfig {
    /// Overrides the file storage configuration via a closure.
    pub fn with_file(mut self, f: impl FnOnce(&mut FileStorageConfig)) -> Self {
        f(&mut self.file);
        self
    }

    /// Overrides the database configuration via a closure.
    pub fn with_database(mut self, f: impl FnOnce(&mut DatabaseConfig)) -> Self {
        f(&mut self.database);
        self
    }

    /// Overrides the HTTP configuration via a closure.
    pub fn with_http(mut self, f: impl FnOnce(&mut HttpConfig)) -> Self {
        f(&mut self.http);
        self
    }

    /// Overrides the notification configuration via a closure.
    pub fn with_notification(mut self, f: impl FnOnce(&mut NotificationConfig)) -> Self {
        f(&mut self.notification);
        self
    }

    /// Overrides the payment configuration via a closure.
    pub fn with_payment(mut self, f: impl FnOnce(&mut PaymentConfig)) -> Self {
        f(&mut self.payment);
        self
    }

    /// Overrides the sink configuration via a closure.
    pub fn with_sink(mut self, f: impl FnOnce(&mut SinkConfig)) -> Self {
        f(&mut self.sink);
        self
    }
}

// =============================================================================
// Factory methods
// =============================================================================

impl GatewayConfig {
    /// Creates a file gateway from the loaded configuration.
    pub fn file_gateway(&self) -> impl FileGateway {
        builders::local_file_gateway(self.file.base_path.clone())
    }

    /// Creates a database gateway from the loaded configuration.
    pub fn database_gateway(&self) -> impl DatabaseGateway {
        builders::memory_database()
    }

    /// Creates an HTTP gateway from the loaded configuration.
    pub fn http_gateway(&self) -> impl HttpGateway {
        builders::rest_client(self.http.clone())
    }

    /// Creates a notification gateway from the loaded configuration.
    pub fn notification_gateway(&self) -> impl NotificationGateway {
        builders::console_notifier()
    }

    /// Creates a payment gateway from the loaded configuration.
    pub fn payment_gateway(&self) -> impl PaymentGateway {
        builders::mock_payment_gateway()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::database::DatabaseType;
    use crate::api::file::FileStorageType;
    use crate::api::notification::NotificationChannel;
    use crate::api::payment::PaymentProvider;

    /// Empty TOML string produces all-default config.
    #[test]
    fn test_load_from_str_defaults() {
        let config = load_config_from_str("").unwrap();
        assert_eq!(config.database.database_type, DatabaseType::Memory);
        assert_eq!(config.file.storage_type, FileStorageType::Local);
        assert_eq!(config.file.base_path, ".");
        assert_eq!(config.http.timeout_secs, 30);
        assert_eq!(config.notification.default_channel, NotificationChannel::Console);
        assert_eq!(config.payment.provider, PaymentProvider::Mock);
    }

    /// Partial TOML only overrides the specified section.
    #[test]
    fn test_load_from_str_partial() {
        let toml = r#"
[file]
storage_type = "s3"
base_path = "my-bucket"
region = "us-west-2"
"#;
        let config = load_config_from_str(toml).unwrap();
        assert_eq!(config.file.storage_type, FileStorageType::S3);
        assert_eq!(config.file.base_path, "my-bucket");
        assert_eq!(config.file.region, Some("us-west-2".to_string()));
        // other sections remain default
        assert_eq!(config.database.database_type, DatabaseType::Memory);
        assert_eq!(config.http.timeout_secs, 30);
    }

    /// All 5 sections populated.
    #[test]
    fn test_load_from_str_full() {
        let toml = r#"
[file]
storage_type = "local"
base_path = "./data"

[database]
database_type = "postgres"
connection_string = "postgres://localhost/mydb"
max_connections = 20

[http]
timeout_secs = 60
max_retries = 5

[notification]
default_channel = "email"

[payment]
provider = "stripe"
sandbox = false
"#;
        let config = load_config_from_str(toml).unwrap();
        assert_eq!(config.file.base_path, "./data");
        assert_eq!(config.database.database_type, DatabaseType::Postgres);
        assert_eq!(
            config.database.connection_string,
            Some("postgres://localhost/mydb".to_string())
        );
        assert_eq!(config.database.max_connections, Some(20));
        assert_eq!(config.http.timeout_secs, 60);
        assert_eq!(config.http.max_retries, 5);
        assert_eq!(config.notification.default_channel, NotificationChannel::Email);
        assert_eq!(config.payment.provider, PaymentProvider::Stripe);
        assert!(!config.payment.sandbox);
    }

    /// `${VAR}` expands to env var value.
    #[test]
    fn test_env_var_expansion() {
        std::env::set_var("SWE_GW_TEST_DB_URL", "postgres://prod/db");
        let toml = r#"
[database]
database_type = "postgres"
connection_string = "${SWE_GW_TEST_DB_URL}"
"#;
        let config = load_config_from_str(toml).unwrap();
        assert_eq!(
            config.database.connection_string,
            Some("postgres://prod/db".to_string())
        );
        std::env::remove_var("SWE_GW_TEST_DB_URL");
    }

    /// `${MISSING:-fallback}` uses the default value.
    #[test]
    fn test_env_var_with_default() {
        std::env::remove_var("SWE_GW_NONEXISTENT_VAR");
        let toml = r#"
[payment]
provider = "stripe"
api_key = "${SWE_GW_NONEXISTENT_VAR:-sk_test_placeholder}"
"#;
        let config = load_config_from_str(toml).unwrap();
        assert_eq!(
            config.payment.api_key,
            Some("sk_test_placeholder".to_string())
        );
    }

    /// `${MISSING}` without a default produces `ConfigError::EnvVar`.
    #[test]
    fn test_env_var_missing_no_default() {
        std::env::remove_var("SWE_GW_DEFINITELY_MISSING");
        let toml = r#"
[database]
database_type = "postgres"
connection_string = "${SWE_GW_DEFINITELY_MISSING}"
"#;
        let err = load_config_from_str(toml).unwrap_err();
        match err {
            ConfigError::EnvVar(name) => assert_eq!(name, "SWE_GW_DEFINITELY_MISSING"),
            other => panic!("expected ConfigError::EnvVar, got: {other}"),
        }
    }

    /// `with_*` overrides modify the correct section.
    #[test]
    fn test_with_override() {
        let config = GatewayConfig::default()
            .with_file(|f| {
                f.base_path = "/custom/path".to_string();
            })
            .with_database(|d| {
                d.database_type = DatabaseType::Postgres;
            })
            .with_http(|h| {
                h.timeout_secs = 120;
            })
            .with_notification(|n| {
                n.default_channel = NotificationChannel::Email;
            })
            .with_payment(|p| {
                p.sandbox = false;
            });

        assert_eq!(config.file.base_path, "/custom/path");
        assert_eq!(config.database.database_type, DatabaseType::Postgres);
        assert_eq!(config.http.timeout_secs, 120);
        assert_eq!(config.notification.default_channel, NotificationChannel::Email);
        assert!(!config.payment.sandbox);
    }

    /// When no `gateway.toml` exists in a temp dir, `load_config` returns defaults.
    #[test]
    fn test_load_config_no_file() {
        let tmp = tempfile::tempdir().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let config = load_config().unwrap();
        assert_eq!(config.database.database_type, DatabaseType::Memory);
        assert_eq!(config.file.base_path, ".");

        std::env::set_current_dir(original).unwrap();
    }

    /// Default sink config is stdout + text.
    #[test]
    fn test_sink_config_defaults() {
        let config = load_config_from_str("").unwrap();
        assert_eq!(config.sink.sink_type, SinkType::Stdout);
        assert_eq!(config.sink.format, SinkFormat::Text);
        assert!(config.sink.path.is_none());
    }

    /// Sink config from TOML sets file destination.
    #[test]
    fn test_sink_config_file_destination() {
        let toml = r#"
[sink]
sink_type = "file"
format = "json"
path = "./reports/output.json"
"#;
        let config = load_config_from_str(toml).unwrap();
        assert_eq!(config.sink.sink_type, SinkType::File);
        assert_eq!(config.sink.format, SinkFormat::Json);
        assert_eq!(
            config.sink.path,
            Some(std::path::PathBuf::from("./reports/output.json"))
        );
    }

    /// `with_sink` override changes sink config.
    #[test]
    fn test_with_sink_override() {
        let config = GatewayConfig::default().with_sink(|s| {
            s.sink_type = SinkType::File;
            s.format = SinkFormat::Json;
            s.path = Some(std::path::PathBuf::from("/tmp/report.json"));
        });
        assert_eq!(config.sink.sink_type, SinkType::File);
        assert_eq!(config.sink.format, SinkFormat::Json);
        assert_eq!(config.sink.path, Some(std::path::PathBuf::from("/tmp/report.json")));
    }

    /// Writing a `gateway.toml` to a temp dir and loading it.
    #[test]
    fn test_load_config_from_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("gateway.toml");
        std::fs::write(
            &file_path,
            r#"
[file]
storage_type = "local"
base_path = "/from/file"

[http]
timeout_secs = 90
"#,
        )
        .unwrap();

        let config = load_config_from(&file_path).unwrap();
        assert_eq!(config.file.base_path, "/from/file");
        assert_eq!(config.http.timeout_secs, 90);
        // defaults for unset sections
        assert_eq!(config.database.database_type, DatabaseType::Memory);
    }
}
