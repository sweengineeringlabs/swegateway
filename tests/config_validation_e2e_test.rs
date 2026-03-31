//! End-to-end tests for configuration validation and environment variable
//! expansion (BL-006).

use swe_gateway::saf::{expand_env_vars, load_config_from_str, ConfigError, GatewayConfig};

// =============================================================================
// expand_env_vars — unit-level coverage via integration test binary
// =============================================================================

#[test]
fn test_expand_env_vars_existing_var_returns_value() {
    std::env::set_var("SWE_CFG_TEST_HOST", "db.example.com");
    let result = expand_env_vars("host=${SWE_CFG_TEST_HOST}").unwrap();
    assert_eq!(result, "host=db.example.com");
    std::env::remove_var("SWE_CFG_TEST_HOST");
}

#[test]
fn test_expand_env_vars_default_used_when_var_missing() {
    std::env::remove_var("SWE_CFG_TEST_ABSENT");
    let result = expand_env_vars("port=${SWE_CFG_TEST_ABSENT:-5432}").unwrap();
    assert_eq!(result, "port=5432");
}

#[test]
fn test_expand_env_vars_no_default_missing_var_returns_error() {
    std::env::remove_var("SWE_CFG_TEST_REQUIRED");
    let err = expand_env_vars("key=${SWE_CFG_TEST_REQUIRED}").unwrap_err();
    match err {
        ConfigError::EnvVar(name) => assert_eq!(name, "SWE_CFG_TEST_REQUIRED"),
        other => panic!("expected ConfigError::EnvVar, got: {other}"),
    }
}

#[test]
fn test_expand_env_vars_multiple_vars_in_one_string() {
    std::env::set_var("SWE_CFG_TEST_USER", "admin");
    std::env::set_var("SWE_CFG_TEST_PASS", "s3cret");
    let result =
        expand_env_vars("postgres://${SWE_CFG_TEST_USER}:${SWE_CFG_TEST_PASS}@localhost").unwrap();
    assert_eq!(result, "postgres://admin:s3cret@localhost");
    std::env::remove_var("SWE_CFG_TEST_USER");
    std::env::remove_var("SWE_CFG_TEST_PASS");
}

#[test]
fn test_expand_env_vars_no_placeholders_returns_input_unchanged() {
    let input = "plain text without any vars";
    let result = expand_env_vars(input).unwrap();
    assert_eq!(result, input);
}

#[test]
fn test_expand_env_vars_malformed_no_closing_brace_reproduced_literally() {
    let input = "prefix${UNCLOSED";
    let result = expand_env_vars(input).unwrap();
    assert_eq!(result, "prefix${UNCLOSED");
}

#[test]
fn test_expand_env_vars_existing_var_ignores_default() {
    std::env::set_var("SWE_CFG_TEST_OVERRIDE", "real_value");
    let result = expand_env_vars("${SWE_CFG_TEST_OVERRIDE:-fallback}").unwrap();
    assert_eq!(result, "real_value");
    std::env::remove_var("SWE_CFG_TEST_OVERRIDE");
}

// =============================================================================
// validate() — valid configs
// =============================================================================

#[test]
fn test_validate_default_config_passes() {
    let config = GatewayConfig::default();
    config.validate().expect("default config should be valid");
}

#[test]
fn test_validate_postgres_with_connection_string_passes() {
    let toml = r#"
[database]
database_type = "postgres"
connection_string = "postgres://localhost/mydb"
"#;
    let config = load_config_from_str(toml).unwrap();
    config.validate().expect("postgres with connection_string should be valid");
}

#[test]
fn test_validate_postgres_with_host_and_database_passes() {
    let toml = r#"
[database]
database_type = "postgres"
host = "localhost"
database = "mydb"
"#;
    let config = load_config_from_str(toml).unwrap();
    config.validate().expect("postgres with host+database should be valid");
}

#[test]
fn test_validate_s3_with_region_passes() {
    let toml = r#"
[file]
storage_type = "s3"
base_path = "my-bucket"
region = "us-east-1"
"#;
    let config = load_config_from_str(toml).unwrap();
    config.validate().expect("s3 with region should be valid");
}

#[test]
fn test_validate_stripe_with_api_key_passes() {
    let toml = r#"
[payment]
provider = "stripe"
api_key = "sk_test_abc123"
"#;
    let config = load_config_from_str(toml).unwrap();
    config.validate().expect("stripe with api_key should be valid");
}

#[test]
fn test_validate_file_sink_with_path_passes() {
    let toml = r#"
[sink]
sink_type = "file"
path = "/tmp/report.json"
"#;
    let config = load_config_from_str(toml).unwrap();
    config.validate().expect("file sink with path should be valid");
}

// =============================================================================
// validate() — invalid configs with clear error messages
// =============================================================================

#[test]
fn test_validate_postgres_without_connection_info_fails() {
    let toml = r#"
[database]
database_type = "postgres"
"#;
    let config = load_config_from_str(toml).unwrap();
    let err = config.validate().unwrap_err();
    match &err {
        ConfigError::Validation(msg) => {
            assert!(
                msg.contains("[database]"),
                "error should mention [database] section, got: {msg}"
            );
            assert!(
                msg.contains("connection_string"),
                "error should mention connection_string, got: {msg}"
            );
        }
        other => panic!("expected ConfigError::Validation, got: {other}"),
    }
}

#[test]
fn test_validate_s3_without_region_fails() {
    let toml = r#"
[file]
storage_type = "s3"
base_path = "my-bucket"
"#;
    let config = load_config_from_str(toml).unwrap();
    let err = config.validate().unwrap_err();
    match &err {
        ConfigError::Validation(msg) => {
            assert!(
                msg.contains("[file]"),
                "error should mention [file] section, got: {msg}"
            );
            assert!(
                msg.contains("region"),
                "error should mention region, got: {msg}"
            );
        }
        other => panic!("expected ConfigError::Validation, got: {other}"),
    }
}

#[test]
fn test_validate_stripe_without_api_key_fails() {
    let toml = r#"
[payment]
provider = "stripe"
"#;
    let config = load_config_from_str(toml).unwrap();
    let err = config.validate().unwrap_err();
    match &err {
        ConfigError::Validation(msg) => {
            assert!(
                msg.contains("[payment]"),
                "error should mention [payment] section, got: {msg}"
            );
            assert!(
                msg.contains("api_key"),
                "error should mention api_key, got: {msg}"
            );
        }
        other => panic!("expected ConfigError::Validation, got: {other}"),
    }
}

#[test]
fn test_validate_file_sink_without_path_fails() {
    let toml = r#"
[sink]
sink_type = "file"
"#;
    let config = load_config_from_str(toml).unwrap();
    let err = config.validate().unwrap_err();
    match &err {
        ConfigError::Validation(msg) => {
            assert!(
                msg.contains("[sink]"),
                "error should mention [sink] section, got: {msg}"
            );
            assert!(
                msg.contains("path"),
                "error should mention path, got: {msg}"
            );
        }
        other => panic!("expected ConfigError::Validation, got: {other}"),
    }
}

#[test]
fn test_validate_multiple_errors_reported_together() {
    let toml = r#"
[database]
database_type = "postgres"

[file]
storage_type = "s3"
base_path = "bucket"

[payment]
provider = "stripe"

[sink]
sink_type = "file"
"#;
    let config = load_config_from_str(toml).unwrap();
    let err = config.validate().unwrap_err();
    match &err {
        ConfigError::Validation(msg) => {
            assert!(msg.contains("[database]"), "should report database error, got: {msg}");
            assert!(msg.contains("[file]"), "should report file error, got: {msg}");
            assert!(msg.contains("[payment]"), "should report payment error, got: {msg}");
            assert!(msg.contains("[sink]"), "should report sink error, got: {msg}");
        }
        other => panic!("expected ConfigError::Validation, got: {other}"),
    }
}

// =============================================================================
// load_config_from_str — pre-deserialization env var expansion
// =============================================================================

#[test]
fn test_load_config_from_str_expands_env_vars_in_toml() {
    std::env::set_var("SWE_CFG_E2E_BASE_PATH", "/mnt/data");
    let toml = r#"
[file]
storage_type = "local"
base_path = "${SWE_CFG_E2E_BASE_PATH}"
"#;
    let config = load_config_from_str(toml).unwrap();
    assert_eq!(config.file.base_path, "/mnt/data");
    std::env::remove_var("SWE_CFG_E2E_BASE_PATH");
}

#[test]
fn test_load_config_from_str_expands_env_vars_with_default_in_toml() {
    std::env::remove_var("SWE_CFG_E2E_TIMEOUT");
    let toml = r#"
[http]
timeout_secs = ${SWE_CFG_E2E_TIMEOUT:-45}
"#;
    let config = load_config_from_str(toml).unwrap();
    assert_eq!(config.http.timeout_secs, 45);
}

#[test]
fn test_load_config_from_str_env_var_missing_no_default_returns_error() {
    std::env::remove_var("SWE_CFG_E2E_SECRET");
    let toml = r#"
[payment]
provider = "stripe"
api_key = "${SWE_CFG_E2E_SECRET}"
"#;
    let err = load_config_from_str(toml).unwrap_err();
    match err {
        ConfigError::EnvVar(name) => assert_eq!(name, "SWE_CFG_E2E_SECRET"),
        other => panic!("expected ConfigError::EnvVar, got: {other}"),
    }
}

#[test]
fn test_load_config_from_str_combined_env_expansion_and_validation() {
    std::env::set_var("SWE_CFG_E2E_CONN", "postgres://localhost/prod");
    let toml = r#"
[database]
database_type = "postgres"
connection_string = "${SWE_CFG_E2E_CONN}"

[file]
storage_type = "s3"
base_path = "my-bucket"
region = "${SWE_CFG_E2E_REGION:-us-west-2}"
"#;
    let config = load_config_from_str(toml).unwrap();
    // Env var expanded correctly
    assert_eq!(
        config.database.connection_string,
        Some("postgres://localhost/prod".to_string())
    );
    assert_eq!(config.file.region, Some("us-west-2".to_string()));
    // Validation should pass
    config.validate().expect("fully configured config should validate");
    std::env::remove_var("SWE_CFG_E2E_CONN");
}
