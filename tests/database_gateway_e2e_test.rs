//! End-to-end tests for DatabaseGateway.
//!
//! Exercises the full lifecycle through the combined DatabaseGateway trait:
//! insert -> query with filters -> update -> count -> delete -> verify.

use swe_gateway::prelude::*;
use swe_gateway::saf::database::QueryParams;
use swe_gateway::saf;

fn make_record(id: &str, name: &str, status: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut r = serde_json::Map::new();
    r.insert("id".into(), serde_json::json!(id));
    r.insert("name".into(), serde_json::json!(name));
    r.insert("status".into(), serde_json::json!(status));
    r
}

#[tokio::test]
async fn e2e_database_full_crud_lifecycle() {
    let db = saf::memory_database();

    // Insert records
    let r1 = db.insert("users", make_record("1", "Alice", "active")).await.unwrap();
    assert_eq!(r1.inserted_id, Some("1".to_string()));

    let r2 = db.insert("users", make_record("2", "Bob", "inactive")).await.unwrap();
    assert_eq!(r2.inserted_id, Some("2".to_string()));

    db.insert("users", make_record("3", "Carol", "active")).await.unwrap();

    // Count all
    let total = db.count("users", QueryParams::new()).await.unwrap();
    assert_eq!(total, 3);

    // Query with filter
    let active = db.query("users", QueryParams::new().filter("status", "active")).await.unwrap();
    assert_eq!(active.len(), 2);

    // Get by ID
    let alice = db.get_by_id("users", "1").await.unwrap().unwrap();
    assert_eq!(alice.get("name").unwrap(), "Alice");

    // Exists
    assert!(db.exists("users", "2").await.unwrap());
    assert!(!db.exists("users", "999").await.unwrap());

    // Update
    let mut updates = serde_json::Map::new();
    updates.insert("status".into(), serde_json::json!("inactive"));
    db.update("users", "1", updates).await.unwrap();

    let alice = db.get_by_id("users", "1").await.unwrap().unwrap();
    assert_eq!(alice.get("status").unwrap(), "inactive");

    // Count active after update — should now be 1
    let active_count = db
        .count("users", QueryParams::new().filter("status", "active"))
        .await
        .unwrap();
    assert_eq!(active_count, 1);

    // Delete single record
    let del = db.delete("users", "2").await.unwrap();
    assert_eq!(del.rows_affected, 1);
    assert!(!db.exists("users", "2").await.unwrap());

    // Final count
    let remaining = db.count("users", QueryParams::new()).await.unwrap();
    assert_eq!(remaining, 2);

    // Health check
    let health = db.health_check().await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
}

#[tokio::test]
async fn e2e_database_batch_and_bulk_operations() {
    let db = saf::memory_database();

    // Batch insert
    let records: Vec<_> = (1..=10)
        .map(|i| make_record(&i.to_string(), &format!("User{}", i), "new"))
        .collect();
    let batch = db.batch_insert("items", records).await.unwrap();
    assert_eq!(batch.rows_affected, 10);

    // Bulk update via update_where
    let mut set_active = serde_json::Map::new();
    set_active.insert("status".into(), serde_json::json!("active"));
    let updated = db
        .update_where("items", QueryParams::new().filter("status", "new"), set_active)
        .await
        .unwrap();
    assert_eq!(updated.rows_affected, 10);

    // Verify all are now active
    let active = db
        .query("items", QueryParams::new().filter("status", "active"))
        .await
        .unwrap();
    assert_eq!(active.len(), 10);

    // Delete where
    let deleted = db
        .delete_where("items", QueryParams::new().filter("name", "User5"))
        .await
        .unwrap();
    assert_eq!(deleted.rows_affected, 1);

    let remaining = db.count("items", QueryParams::new()).await.unwrap();
    assert_eq!(remaining, 9);
}

#[tokio::test]
async fn e2e_database_query_pagination_and_ordering() {
    let db = saf::memory_database();

    for i in 1..=20 {
        db.insert(
            "products",
            make_record(&format!("{:03}", i), &format!("Product{:02}", i), "available"),
        )
        .await
        .unwrap();
    }

    // Paginate: skip 5, take 3
    let page = db
        .query("products", QueryParams::new().paginate(5, 3))
        .await
        .unwrap();
    assert_eq!(page.len(), 3);

    // Select specific fields
    let selected = db
        .query(
            "products",
            QueryParams::new().select(["name"]).paginate(0, 2),
        )
        .await
        .unwrap();
    assert_eq!(selected.len(), 2);
    for record in &selected {
        assert!(record.contains_key("name"));
        // "id" and "status" should be excluded
        assert!(!record.contains_key("status"));
    }
}

#[tokio::test]
async fn e2e_database_duplicate_insert_rejected() {
    let db = saf::memory_database();

    db.insert("users", make_record("1", "Alice", "active"))
        .await
        .unwrap();

    // Duplicate ID should fail
    let result = db.insert("users", make_record("1", "Duplicate", "active")).await;
    assert!(result.is_err());
}
