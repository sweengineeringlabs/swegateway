//! In-memory database implementation.

use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::RwLock;

use crate::api::{
    database::{QueryParams, Record, WriteResult},
    traits::{DatabaseGateway, DatabaseInbound, DatabaseOutbound},
    types::{GatewayError, GatewayResult, HealthCheck},
};

/// In-memory database implementation for testing and development.
#[derive(Debug, Default)]
pub(crate) struct MemoryDatabase {
    /// Tables stored as table_name -> (id -> record).
    tables: RwLock<HashMap<String, HashMap<String, Record>>>,
}

impl MemoryDatabase {
    /// Creates a new empty in-memory database.
    pub fn new() -> Self {
        Self {
            tables: RwLock::new(HashMap::new()),
        }
    }

    /// Creates a database with predefined tables.
    pub fn with_tables(tables: Vec<&str>) -> Self {
        let mut map = HashMap::new();
        for table in tables {
            map.insert(table.to_string(), HashMap::new());
        }
        Self {
            tables: RwLock::new(map),
        }
    }

    /// Gets or creates a table.
    fn ensure_table(&self, table: &str) {
        let mut tables = self.tables.write().unwrap();
        tables.entry(table.to_string()).or_insert_with(HashMap::new);
    }

    /// Extracts the ID from a record.
    fn extract_id(record: &Record) -> Option<String> {
        record
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                record
                    .get("_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
    }

    /// Extracts a numeric value from a JSON value for comparison.
    /// Returns f64 for Number values, None for anything else.
    fn as_f64(value: &serde_json::Value) -> Option<f64> {
        value.as_f64()
    }

    /// Compares two JSON values numerically if both are numbers,
    /// otherwise falls back to string comparison. Null values sort last.
    fn compare_values(
        a: Option<&serde_json::Value>,
        b: Option<&serde_json::Value>,
    ) -> std::cmp::Ordering {
        match (a, b) {
            (None | Some(serde_json::Value::Null), None | Some(serde_json::Value::Null)) => {
                std::cmp::Ordering::Equal
            }
            (Some(serde_json::Value::Null) | None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), Some(serde_json::Value::Null) | None) => std::cmp::Ordering::Less,
            (Some(av), Some(bv)) => {
                // Try numeric comparison first
                if let (Some(an), Some(bn)) = (Self::as_f64(av), Self::as_f64(bv)) {
                    an.partial_cmp(&bn).unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    // Fall back to string comparison
                    let a_str = match av.as_str() {
                        Some(s) => s.to_string(),
                        None => av.to_string(),
                    };
                    let b_str = match bv.as_str() {
                        Some(s) => s.to_string(),
                        None => bv.to_string(),
                    };
                    a_str.cmp(&b_str)
                }
            }
        }
    }

    /// Parses a filter key into (field_name, operator).
    /// Supported operators: __gt, __lt, __gte, __lte, __like, __in.
    /// If no operator suffix is found, returns the key as-is with "eq" operator.
    fn parse_filter_key(key: &str) -> (&str, &str) {
        for suffix in &["__gte", "__lte", "__gt", "__lt", "__like", "__in"] {
            if let Some(field) = key.strip_suffix(suffix) {
                // Strip the leading "__" from suffix to get the operator name
                return (field, &suffix[2..]);
            }
        }
        (key, "eq")
    }

    /// Checks if a single field matches a filter condition.
    fn matches_filter_condition(
        field_value: Option<&serde_json::Value>,
        operator: &str,
        filter_value: &serde_json::Value,
    ) -> bool {
        match operator {
            "eq" => match field_value {
                Some(fv) => fv == filter_value,
                None => false,
            },
            "gt" | "lt" | "gte" | "lte" => {
                let fv = match field_value {
                    Some(v) if !v.is_null() => v,
                    _ => return false,
                };
                let ord = Self::compare_values(Some(fv), Some(filter_value));
                match operator {
                    "gt" => ord == std::cmp::Ordering::Greater,
                    "lt" => ord == std::cmp::Ordering::Less,
                    "gte" => ord != std::cmp::Ordering::Less,
                    "lte" => ord != std::cmp::Ordering::Greater,
                    _ => unreachable!(),
                }
            }
            "like" => {
                let fv = match field_value {
                    Some(v) => v,
                    None => return false,
                };
                let field_str = match fv.as_str() {
                    Some(s) => s,
                    None => return false,
                };
                let pattern = match filter_value.as_str() {
                    Some(s) => s,
                    None => return false,
                };
                field_str.to_lowercase().contains(&pattern.to_lowercase())
            }
            "in" => {
                let fv = match field_value {
                    Some(v) => v,
                    None => return false,
                };
                match filter_value.as_array() {
                    Some(arr) => arr.contains(fv),
                    None => fv == filter_value,
                }
            }
            _ => false,
        }
    }

    /// Checks if a record matches the query filters.
    fn matches_filters(record: &Record, filters: &HashMap<String, serde_json::Value>) -> bool {
        for (key, value) in filters {
            let (field, operator) = Self::parse_filter_key(key);
            if !Self::matches_filter_condition(record.get(field), operator, value) {
                return false;
            }
        }
        true
    }

    /// Applies pagination and ordering to results.
    fn apply_pagination(
        mut records: Vec<Record>,
        params: &QueryParams,
    ) -> Vec<Record> {
        // Apply ordering with type-aware comparison.
        // Null/missing values always sort last regardless of direction.
        if let Some(order_by) = &params.order_by {
            records.sort_by(|a, b| {
                let a_val = a.get(order_by);
                let b_val = b.get(order_by);

                let a_is_null = matches!(a_val, None | Some(serde_json::Value::Null));
                let b_is_null = matches!(b_val, None | Some(serde_json::Value::Null));

                match (a_is_null, b_is_null) {
                    (true, true) => std::cmp::Ordering::Equal,
                    (true, false) => std::cmp::Ordering::Greater,  // nulls last
                    (false, true) => std::cmp::Ordering::Less,     // nulls last
                    (false, false) => {
                        let cmp = Self::compare_values(a_val, b_val);
                        if params.order_desc { cmp.reverse() } else { cmp }
                    }
                }
            });
        }

        // Apply offset
        if let Some(offset) = params.offset {
            if offset < records.len() {
                records = records.into_iter().skip(offset).collect();
            } else {
                records = Vec::new();
            }
        }

        // Apply limit
        if let Some(limit) = params.limit {
            records.truncate(limit);
        }

        records
    }

    /// Applies field selection to a record.
    fn select_fields(record: Record, select: &[String]) -> Record {
        if select.is_empty() {
            return record;
        }
        let mut result = serde_json::Map::new();
        for field in select {
            if let Some(value) = record.get(field) {
                result.insert(field.clone(), value.clone());
            }
        }
        result
    }
}

impl DatabaseInbound for MemoryDatabase {
    fn query(
        &self,
        table: &str,
        params: QueryParams,
    ) -> BoxFuture<'_, GatewayResult<Vec<Record>>> {
        let table = table.to_string();
        Box::pin(async move {
            let tables = self.tables.read().unwrap();
            let records = match tables.get(&table) {
                Some(table_data) => {
                    let filtered: Vec<Record> = table_data
                        .values()
                        .filter(|r| Self::matches_filters(r, &params.filters))
                        .cloned()
                        .collect();

                    let paginated = Self::apply_pagination(filtered, &params);

                    paginated
                        .into_iter()
                        .map(|r| Self::select_fields(r, &params.select))
                        .collect()
                }
                None => Vec::new(),
            };
            Ok(records)
        })
    }

    fn get_by_id(
        &self,
        table: &str,
        id: &str,
    ) -> BoxFuture<'_, GatewayResult<Option<Record>>> {
        let table = table.to_string();
        let id = id.to_string();
        Box::pin(async move {
            let tables = self.tables.read().unwrap();
            Ok(tables
                .get(&table)
                .and_then(|t| t.get(&id))
                .cloned())
        })
    }

    fn exists(&self, table: &str, id: &str) -> BoxFuture<'_, GatewayResult<bool>> {
        let table = table.to_string();
        let id = id.to_string();
        Box::pin(async move {
            let tables = self.tables.read().unwrap();
            Ok(tables
                .get(&table)
                .map(|t| t.contains_key(&id))
                .unwrap_or(false))
        })
    }

    fn count(&self, table: &str, params: QueryParams) -> BoxFuture<'_, GatewayResult<u64>> {
        let table = table.to_string();
        Box::pin(async move {
            let tables = self.tables.read().unwrap();
            let count = match tables.get(&table) {
                Some(table_data) => table_data
                    .values()
                    .filter(|r| Self::matches_filters(r, &params.filters))
                    .count() as u64,
                None => 0,
            };
            Ok(count)
        })
    }

    fn health_check(&self) -> BoxFuture<'_, GatewayResult<HealthCheck>> {
        Box::pin(async move { Ok(HealthCheck::healthy()) })
    }
}

impl DatabaseOutbound for MemoryDatabase {
    fn insert(
        &self,
        table: &str,
        record: Record,
    ) -> BoxFuture<'_, GatewayResult<WriteResult>> {
        let table = table.to_string();
        Box::pin(async move {
            self.ensure_table(&table);

            let id = Self::extract_id(&record).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

            let mut tables = self.tables.write().unwrap();
            let table_data = tables.get_mut(&table).unwrap();

            if table_data.contains_key(&id) {
                return Err(GatewayError::Conflict(format!(
                    "Record with id '{}' already exists in table '{}'",
                    id, table
                )));
            }

            let mut record = record;
            record.insert("id".to_string(), serde_json::Value::String(id.clone()));
            table_data.insert(id.clone(), record);

            Ok(WriteResult::inserted(id))
        })
    }

    fn update(
        &self,
        table: &str,
        id: &str,
        record: Record,
    ) -> BoxFuture<'_, GatewayResult<WriteResult>> {
        let table = table.to_string();
        let id = id.to_string();
        Box::pin(async move {
            let mut tables = self.tables.write().unwrap();

            let table_data = tables.get_mut(&table).ok_or_else(|| {
                GatewayError::NotFound(format!("Table '{}' not found", table))
            })?;

            let existing = table_data.get_mut(&id).ok_or_else(|| {
                GatewayError::NotFound(format!("Record '{}' not found in table '{}'", id, table))
            })?;

            // Merge updates into existing record
            for (key, value) in record {
                existing.insert(key, value);
            }

            Ok(WriteResult::new(1))
        })
    }

    fn delete(&self, table: &str, id: &str) -> BoxFuture<'_, GatewayResult<WriteResult>> {
        let table = table.to_string();
        let id = id.to_string();
        Box::pin(async move {
            let mut tables = self.tables.write().unwrap();

            let table_data = tables.get_mut(&table).ok_or_else(|| {
                GatewayError::NotFound(format!("Table '{}' not found", table))
            })?;

            match table_data.remove(&id) {
                Some(_) => Ok(WriteResult::new(1)),
                None => Ok(WriteResult::new(0)),
            }
        })
    }

    fn batch_insert(
        &self,
        table: &str,
        records: Vec<Record>,
    ) -> BoxFuture<'_, GatewayResult<WriteResult>> {
        let table = table.to_string();
        Box::pin(async move {
            self.ensure_table(&table);

            let mut tables = self.tables.write().unwrap();
            let table_data = tables.get_mut(&table).unwrap();

            let mut count = 0u64;
            for record in records {
                let id = Self::extract_id(&record).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

                if !table_data.contains_key(&id) {
                    let mut record = record;
                    record.insert("id".to_string(), serde_json::Value::String(id.clone()));
                    table_data.insert(id, record);
                    count += 1;
                }
            }

            Ok(WriteResult::new(count))
        })
    }

    fn update_where(
        &self,
        table: &str,
        params: QueryParams,
        updates: Record,
    ) -> BoxFuture<'_, GatewayResult<WriteResult>> {
        let table = table.to_string();
        Box::pin(async move {
            let mut tables = self.tables.write().unwrap();

            let table_data = tables.get_mut(&table).ok_or_else(|| {
                GatewayError::NotFound(format!("Table '{}' not found", table))
            })?;

            let mut count = 0u64;
            for record in table_data.values_mut() {
                if Self::matches_filters(record, &params.filters) {
                    for (key, value) in &updates {
                        record.insert(key.clone(), value.clone());
                    }
                    count += 1;
                }
            }

            Ok(WriteResult::new(count))
        })
    }

    fn delete_where(
        &self,
        table: &str,
        params: QueryParams,
    ) -> BoxFuture<'_, GatewayResult<WriteResult>> {
        let table = table.to_string();
        Box::pin(async move {
            let mut tables = self.tables.write().unwrap();

            let table_data = tables.get_mut(&table).ok_or_else(|| {
                GatewayError::NotFound(format!("Table '{}' not found", table))
            })?;

            let ids_to_delete: Vec<String> = table_data
                .iter()
                .filter(|(_, record)| Self::matches_filters(record, &params.filters))
                .map(|(id, _)| id.clone())
                .collect();

            let count = ids_to_delete.len() as u64;
            for id in ids_to_delete {
                table_data.remove(&id);
            }

            Ok(WriteResult::new(count))
        })
    }
}

impl DatabaseGateway for MemoryDatabase {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_insert_and_get() {
        let db = MemoryDatabase::new();

        let mut record = serde_json::Map::new();
        record.insert("id".to_string(), serde_json::json!("1"));
        record.insert("name".to_string(), serde_json::json!("Alice"));

        let result = db.insert("users", record).await.unwrap();
        assert_eq!(result.inserted_id, Some("1".to_string()));

        let retrieved = db.get_by_id("users", "1").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().get("name").unwrap(), "Alice");
    }

    #[tokio::test]
    async fn test_query_with_filters() {
        let db = MemoryDatabase::new();

        for i in 1..=5 {
            let mut record = serde_json::Map::new();
            record.insert("id".to_string(), serde_json::json!(i.to_string()));
            record.insert("status".to_string(), serde_json::json!(if i % 2 == 0 { "active" } else { "inactive" }));
            db.insert("items", record).await.unwrap();
        }

        let params = QueryParams::new().filter("status", "active");
        let results = db.query("items", params).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_update() {
        let db = MemoryDatabase::new();

        let mut record = serde_json::Map::new();
        record.insert("id".to_string(), serde_json::json!("1"));
        record.insert("name".to_string(), serde_json::json!("Alice"));
        db.insert("users", record).await.unwrap();

        let mut updates = serde_json::Map::new();
        updates.insert("name".to_string(), serde_json::json!("Bob"));
        db.update("users", "1", updates).await.unwrap();

        let retrieved = db.get_by_id("users", "1").await.unwrap().unwrap();
        assert_eq!(retrieved.get("name").unwrap(), "Bob");
    }

    #[tokio::test]
    async fn test_delete() {
        let db = MemoryDatabase::new();

        let mut record = serde_json::Map::new();
        record.insert("id".to_string(), serde_json::json!("1"));
        db.insert("users", record).await.unwrap();

        assert!(db.exists("users", "1").await.unwrap());

        db.delete("users", "1").await.unwrap();

        assert!(!db.exists("users", "1").await.unwrap());
    }

    /// @covers: with_tables
    #[test]
    fn test_with_tables_sync() {
        let db = MemoryDatabase::with_tables(vec!["users", "orders"]);

        // Verify tables exist by checking the internal state via RwLock
        let tables = db.tables.read().unwrap();
        assert!(tables.contains_key("users"), "should have 'users' table");
        assert!(tables.contains_key("orders"), "should have 'orders' table");
        assert_eq!(tables.len(), 2, "should have exactly 2 tables");

        // Verify tables are empty
        assert!(tables["users"].is_empty(), "'users' table should be empty");
        assert!(tables["orders"].is_empty(), "'orders' table should be empty");
    }

    /// @covers: with_tables
    #[tokio::test]
    async fn test_with_tables() {
        let db = MemoryDatabase::with_tables(vec!["users", "orders"]);

        // Predefined table "users" should return empty vec, not error
        let results = db.query("users", QueryParams::new()).await.unwrap();
        assert_eq!(results.len(), 0, "predefined table 'users' should be empty");

        // Predefined table "orders" should also return empty vec
        let results = db.query("orders", QueryParams::new()).await.unwrap();
        assert_eq!(results.len(), 0, "predefined table 'orders' should be empty");

        // Non-predefined table "products" should also return empty vec (not error)
        let results = db.query("products", QueryParams::new()).await.unwrap();
        assert_eq!(results.len(), 0, "non-predefined table should return empty vec");

        // Verify we can insert into a predefined table
        let mut record = serde_json::Map::new();
        record.insert("id".to_string(), serde_json::json!("1"));
        record.insert("name".to_string(), serde_json::json!("Alice"));
        let result = db.insert("users", record).await.unwrap();
        assert_eq!(result.inserted_id, Some("1".to_string()));

        let results = db.query("users", QueryParams::new()).await.unwrap();
        assert_eq!(results.len(), 1, "should have one record after insert");
    }
}
