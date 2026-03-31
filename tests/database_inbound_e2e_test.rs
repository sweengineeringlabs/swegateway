//! End-to-end tests for DatabaseInbound sub-trait.
//!
//! Exercises only the DatabaseInbound read operations through the SAF factory.
//! Data is seeded using the combined gateway (which implements both sub-traits),
//! then only DatabaseInbound methods are exercised.

use swe_gateway::prelude::*;
use swe_gateway::saf::database::QueryParams;
use swe_gateway::saf;

fn make_record(id: &str, name: &str, category: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut r = serde_json::Map::new();
    r.insert("id".into(), serde_json::json!(id));
    r.insert("name".into(), serde_json::json!(name));
    r.insert("category".into(), serde_json::json!(category));
    r
}

#[tokio::test]
async fn e2e_database_inbound_query_and_get_by_id() {
    // The SAF builder returns impl DatabaseGateway which implements both sub-traits.
    // We seed data using the full gateway, then call only DatabaseInbound methods.
    let db = saf::memory_database();

    // Seed data via the combined gateway (Outbound side)
    db.insert("products", make_record("p1", "Widget A", "widgets")).await.unwrap();
    db.insert("products", make_record("p2", "Widget B", "widgets")).await.unwrap();
    db.insert("products", make_record("p3", "Gadget X", "gadgets")).await.unwrap();

    // --- DatabaseInbound: query ---
    let all = db.query("products", QueryParams::new()).await.unwrap();
    assert_eq!(all.len(), 3);

    let widgets = db
        .query("products", QueryParams::new().filter("category", "widgets"))
        .await
        .unwrap();
    assert_eq!(widgets.len(), 2);
    for w in &widgets {
        assert_eq!(w.get("category").unwrap(), "widgets");
    }

    // --- DatabaseInbound: get_by_id ---
    let p1 = db.get_by_id("products", "p1").await.unwrap();
    assert!(p1.is_some());
    let p1 = p1.unwrap();
    assert_eq!(p1.get("name").unwrap(), "Widget A");

    // Missing ID returns None
    let missing = db.get_by_id("products", "does-not-exist").await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn e2e_database_inbound_exists_and_count() {
    let db = saf::memory_database();

    // Seed data
    for i in 1..=5 {
        db.insert(
            "orders",
            make_record(&format!("o{}", i), &format!("Order {}", i), "pending"),
        )
        .await
        .unwrap();
    }
    db.insert("orders", make_record("o6", "Order 6", "shipped")).await.unwrap();

    // --- DatabaseInbound: exists ---
    assert!(db.exists("orders", "o1").await.unwrap());
    assert!(db.exists("orders", "o6").await.unwrap());
    assert!(!db.exists("orders", "o99").await.unwrap());

    // --- DatabaseInbound: count ---
    let total = db.count("orders", QueryParams::new()).await.unwrap();
    assert_eq!(total, 6);

    let pending_count = db
        .count("orders", QueryParams::new().filter("category", "pending"))
        .await
        .unwrap();
    assert_eq!(pending_count, 5);

    let shipped_count = db
        .count("orders", QueryParams::new().filter("category", "shipped"))
        .await
        .unwrap();
    assert_eq!(shipped_count, 1);
}

#[tokio::test]
async fn e2e_database_inbound_query_pagination_and_health_check() {
    let db = saf::memory_database();

    // Seed 12 records
    for i in 1..=12 {
        db.insert(
            "items",
            make_record(&format!("{:02}", i), &format!("Item {:02}", i), "active"),
        )
        .await
        .unwrap();
    }

    // --- DatabaseInbound: query with pagination ---
    let first_page = db
        .query("items", QueryParams::new().paginate(0, 5))
        .await
        .unwrap();
    assert_eq!(first_page.len(), 5);

    let second_page = db
        .query("items", QueryParams::new().paginate(5, 5))
        .await
        .unwrap();
    assert_eq!(second_page.len(), 5);

    let last_page = db
        .query("items", QueryParams::new().paginate(10, 5))
        .await
        .unwrap();
    assert_eq!(last_page.len(), 2);

    // Pages must not overlap — first record of second page must differ from last of first
    let first_page_last = first_page.last().unwrap().get("id").unwrap().clone();
    let second_page_first = second_page.first().unwrap().get("id").unwrap().clone();
    assert_ne!(first_page_last, second_page_first);

    // --- DatabaseInbound: health_check ---
    let health = db.health_check().await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
}
