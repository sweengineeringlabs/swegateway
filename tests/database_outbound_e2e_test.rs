//! End-to-end tests for DatabaseOutbound sub-trait.
//!
//! Exercises only the DatabaseOutbound write operations through the SAF factory.
//! Verification uses DatabaseInbound methods (get_by_id, exists, count) on the
//! same combined gateway instance.

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
async fn e2e_database_outbound_insert_and_update() {
    let db = saf::memory_database();

    // --- DatabaseOutbound: insert ---
    let result = db.insert("users", make_record("u1", "Alice", "active")).await.unwrap();
    assert_eq!(result.rows_affected, 1);
    assert_eq!(result.inserted_id, Some("u1".to_string()));

    db.insert("users", make_record("u2", "Bob", "active")).await.unwrap();
    db.insert("users", make_record("u3", "Carol", "inactive")).await.unwrap();

    // Verify via Inbound
    let total = db.count("users", QueryParams::new()).await.unwrap();
    assert_eq!(total, 3);

    // --- DatabaseOutbound: update ---
    let mut updates = serde_json::Map::new();
    updates.insert("status".into(), serde_json::json!("suspended"));
    let update_result = db.update("users", "u2", updates).await.unwrap();
    assert_eq!(update_result.rows_affected, 1);

    // Verify the update took effect
    let bob = db.get_by_id("users", "u2").await.unwrap().unwrap();
    assert_eq!(bob.get("status").unwrap(), "suspended");

    // Other records unaffected
    let alice = db.get_by_id("users", "u1").await.unwrap().unwrap();
    assert_eq!(alice.get("status").unwrap(), "active");
}

#[tokio::test]
async fn e2e_database_outbound_delete_and_batch_insert() {
    let db = saf::memory_database();

    // --- DatabaseOutbound: batch_insert ---
    let records: Vec<_> = (1..=8)
        .map(|i| make_record(&format!("r{}", i), &format!("Record {}", i), "new"))
        .collect();
    let batch_result = db.batch_insert("records", records).await.unwrap();
    assert_eq!(batch_result.rows_affected, 8);

    // Verify all inserted
    let count = db.count("records", QueryParams::new()).await.unwrap();
    assert_eq!(count, 8);

    // --- DatabaseOutbound: delete ---
    let del_result = db.delete("records", "r3").await.unwrap();
    assert_eq!(del_result.rows_affected, 1);

    // Verify deletion
    assert!(!db.exists("records", "r3").await.unwrap());

    let remaining = db.count("records", QueryParams::new()).await.unwrap();
    assert_eq!(remaining, 7);

    // Delete non-existent record
    let del_missing = db.delete("records", "r99").await.unwrap();
    assert_eq!(del_missing.rows_affected, 0);
}

#[tokio::test]
async fn e2e_database_outbound_update_where_and_delete_where() {
    let db = saf::memory_database();

    // Seed data
    for i in 1..=6 {
        let status = if i <= 3 { "draft" } else { "published" };
        db.insert(
            "posts",
            make_record(&format!("post{}", i), &format!("Post {}", i), status),
        )
        .await
        .unwrap();
    }

    // --- DatabaseOutbound: update_where ---
    let mut archive_update = serde_json::Map::new();
    archive_update.insert("status".into(), serde_json::json!("archived"));
    let updated = db
        .update_where("posts", QueryParams::new().filter("status", "draft"), archive_update)
        .await
        .unwrap();
    assert_eq!(updated.rows_affected, 3);

    // Verify: no more drafts, 3 archived
    let draft_count = db
        .count("posts", QueryParams::new().filter("status", "draft"))
        .await
        .unwrap();
    assert_eq!(draft_count, 0);

    let archived_count = db
        .count("posts", QueryParams::new().filter("status", "archived"))
        .await
        .unwrap();
    assert_eq!(archived_count, 3);

    // --- DatabaseOutbound: delete_where ---
    let deleted = db
        .delete_where("posts", QueryParams::new().filter("status", "archived"))
        .await
        .unwrap();
    assert_eq!(deleted.rows_affected, 3);

    // Only the published posts remain
    let remaining = db.count("posts", QueryParams::new()).await.unwrap();
    assert_eq!(remaining, 3);
}
