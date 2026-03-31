//! Database gateway types and configuration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Database backend type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    /// In-memory database (for testing/development).
    Memory,
    /// PostgreSQL database.
    Postgres,
    /// MySQL/MariaDB database.
    MySql,
    /// SQLite database.
    Sqlite,
    /// MongoDB database.
    MongoDb,
}

impl Default for DatabaseType {
    fn default() -> Self {
        Self::Memory
    }
}

/// Configuration for database connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    /// Database backend type.
    pub database_type: DatabaseType,
    /// Connection string or URL.
    pub connection_string: Option<String>,
    /// Database host.
    pub host: Option<String>,
    /// Database port.
    pub port: Option<u16>,
    /// Database name.
    pub database: Option<String>,
    /// Username for authentication.
    pub username: Option<String>,
    /// Password for authentication (sensitive).
    #[serde(skip_serializing)]
    pub password: Option<String>,
    /// Maximum number of connections in the pool.
    pub max_connections: Option<u32>,
    /// Connection timeout in seconds.
    pub connect_timeout_secs: Option<u64>,
    /// Additional options.
    #[serde(default)]
    pub options: HashMap<String, String>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            database_type: DatabaseType::Memory,
            connection_string: None,
            host: None,
            port: None,
            database: None,
            username: None,
            password: None,
            max_connections: Some(10),
            connect_timeout_secs: Some(30),
            options: HashMap::new(),
        }
    }
}

impl DatabaseConfig {
    /// Creates a new in-memory database configuration.
    pub fn memory() -> Self {
        Self::default()
    }

    /// Creates a PostgreSQL configuration.
    pub fn postgres(connection_string: impl Into<String>) -> Self {
        Self {
            database_type: DatabaseType::Postgres,
            connection_string: Some(connection_string.into()),
            ..Default::default()
        }
    }

    /// Creates a MySQL configuration.
    pub fn mysql(connection_string: impl Into<String>) -> Self {
        Self {
            database_type: DatabaseType::MySql,
            connection_string: Some(connection_string.into()),
            ..Default::default()
        }
    }
}

/// A database record represented as a JSON object.
pub type Record = serde_json::Map<String, serde_json::Value>;

/// Query parameters for database operations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryParams {
    /// Filter conditions as key-value pairs.
    #[serde(default)]
    pub filters: HashMap<String, serde_json::Value>,
    /// Fields to select (empty means all).
    #[serde(default)]
    pub select: Vec<String>,
    /// Field to order by.
    pub order_by: Option<String>,
    /// Order direction.
    pub order_desc: bool,
    /// Number of records to skip.
    pub offset: Option<usize>,
    /// Maximum number of records to return.
    pub limit: Option<usize>,
}

impl QueryParams {
    /// Creates a new empty query params.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a filter condition.
    pub fn filter(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.filters.insert(key.into(), value.into());
        self
    }

    /// Sets the fields to select.
    pub fn select(mut self, fields: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.select = fields.into_iter().map(Into::into).collect();
        self
    }

    /// Sets ordering.
    pub fn order_by(mut self, field: impl Into<String>, desc: bool) -> Self {
        self.order_by = Some(field.into());
        self.order_desc = desc;
        self
    }

    /// Sets pagination.
    pub fn paginate(mut self, offset: usize, limit: usize) -> Self {
        self.offset = Some(offset);
        self.limit = Some(limit);
        self
    }
}

/// Result of a write operation (insert/update/delete).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteResult {
    /// Number of rows affected.
    pub rows_affected: u64,
    /// The ID of the inserted record (for inserts).
    pub inserted_id: Option<String>,
}

impl WriteResult {
    /// Creates a new write result with the given rows affected.
    pub fn new(rows_affected: u64) -> Self {
        Self {
            rows_affected,
            inserted_id: None,
        }
    }

    /// Creates a write result for an insert operation.
    pub fn inserted(id: impl Into<String>) -> Self {
        Self {
            rows_affected: 1,
            inserted_id: Some(id.into()),
        }
    }
}

/// Transaction isolation level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationLevel {
    /// Read uncommitted.
    ReadUncommitted,
    /// Read committed.
    #[default]
    ReadCommitted,
    /// Repeatable read.
    RepeatableRead,
    /// Serializable.
    Serializable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_record_with_fields() {
        let mut record = Record::new();
        record.insert("name".to_string(), serde_json::Value::String("Alice".to_string()));
        record.insert("age".to_string(), serde_json::Value::Number(30.into()));
        assert_eq!(record.len(), 2);
        assert_eq!(record["name"], serde_json::Value::String("Alice".to_string()));
    }

    /// @covers: filter, select, order_by, paginate
    #[test]
    fn test_query_params_builder() {
        let params = QueryParams::new()
            .filter("status", "active")
            .select(["name", "email"])
            .order_by("name", false)
            .paginate(0, 10);
        assert_eq!(params.filters.len(), 1);
        assert_eq!(params.select, vec!["name", "email"]);
        assert_eq!(params.order_by, Some("name".to_string()));
        assert!(!params.order_desc);
        assert_eq!(params.offset, Some(0));
        assert_eq!(params.limit, Some(10));
    }

    /// @covers: inserted
    #[test]
    fn test_write_result_inserted() {
        let result = WriteResult::inserted("abc-123");
        assert_eq!(result.rows_affected, 1);
        assert_eq!(result.inserted_id, Some("abc-123".to_string()));
    }

    /// @covers: memory
    #[test]
    fn test_database_config_memory_default() {
        let config = DatabaseConfig::memory();
        assert_eq!(config.database_type, DatabaseType::Memory);
        assert_eq!(config.max_connections, Some(10));
    }

    /// @covers: postgres
    #[test]
    fn test_postgres() {
        let config = DatabaseConfig::postgres("postgres://localhost/test");
        assert_eq!(config.database_type, DatabaseType::Postgres);
        assert_eq!(config.connection_string, Some("postgres://localhost/test".to_string()));
    }

    /// @covers: mysql
    #[test]
    fn test_mysql() {
        let config = DatabaseConfig::mysql("mysql://localhost/test");
        assert_eq!(config.database_type, DatabaseType::MySql);
        assert_eq!(config.connection_string, Some("mysql://localhost/test".to_string()));
    }

    /// @covers: filter
    #[test]
    fn test_filter() {
        let params = QueryParams::new().filter("key", "val");
        assert!(params.filters.contains_key("key"));
        assert_eq!(params.filters["key"], serde_json::Value::String("val".to_string()));
    }

    /// @covers: select
    #[test]
    fn test_select() {
        let params = QueryParams::new().select(["a", "b"]);
        assert_eq!(params.select, vec!["a".to_string(), "b".to_string()]);
    }

    /// @covers: order_by
    #[test]
    fn test_order_by() {
        let params = QueryParams::new().order_by("name", true);
        assert_eq!(params.order_by, Some("name".to_string()));
        assert!(params.order_desc);
    }

    /// @covers: paginate
    #[test]
    fn test_paginate() {
        let params = QueryParams::new().paginate(5, 10);
        assert_eq!(params.offset, Some(5));
        assert_eq!(params.limit, Some(10));
    }
}
