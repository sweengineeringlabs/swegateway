//! Smoke tests verifying the fixtures module compiles and works.

mod fixtures;

use fixtures::gateways::TempFileGateway;
use fixtures::records;
use fixtures::seed;
use swe_gateway::prelude::*;
use swe_gateway::saf;
use swe_gateway::saf::database::QueryParams;
use swe_gateway::saf::file::ListOptions;

#[test]
fn test_record_builders() {
    let r = records::record("1", "Alice");
    assert_eq!(r["id"], "1");
    assert_eq!(r["name"], "Alice");

    let r = records::record_with_status("2", "Bob", "active");
    assert_eq!(r["status"], "active");

    let r = records::record_with_category("3", "Carol", "admin");
    assert_eq!(r["category"], "admin");

    let r = records::product("4", "Widget", 9.99);
    assert_eq!(r["price"], 9.99);

    let r = records::product_with_category("5", "Gadget", 19.99, "electronics");
    assert_eq!(r["category"], "electronics");

    let r = records::numbered_record(42, "test-payload");
    assert_eq!(r["id"], "42");
    assert_eq!(r["name"], "item-42");
    assert_eq!(r["payload"], "test-payload");

    let r = records::record_with("x", |r| {
        r.insert("custom".into(), serde_json::json!(true));
    });
    assert_eq!(r["custom"], true);
}

#[test]
fn test_notification_builders() {
    let n = records::notification("user@test.com", "Hello");
    assert_eq!(n.recipient, "user@test.com");
    assert_eq!(n.body, "Hello");

    let n = records::email_notification("a@b.com", "Subject", "Body");
    assert_eq!(n.subject, Some("Subject".into()));
}

#[test]
fn test_payment_builders() {
    let m = records::usd(1000);
    assert_eq!(m.amount, 1000);

    let c = records::customer("c1", "Alice", "alice@test.com");
    assert_eq!(c.id, "c1");
    assert_eq!(c.name, Some("Alice".into()));
}

#[test]
fn test_temp_file_gateway_creates_and_drops() {
    let fg = TempFileGateway::new();
    assert!(fg.path().exists());
    // gateway is usable (returns owned impl)
    let _gw = fg.gateway();
}

#[test]
fn test_gateway_factories() {
    let _db = fixtures::gateways::memory_db();
    let _db2 = fixtures::gateways::memory_db_with_tables(&["users", "orders"]);
    let _n = fixtures::gateways::notifier();
    let _p = fixtures::gateways::payments();
}

#[tokio::test]
async fn test_seed_insert_numbered_records() {
    let db = saf::memory_database();
    seed::insert_numbered_records(&db, "items", 10, "data").await;
    let count = db.count("items", QueryParams::new()).await.unwrap();
    assert_eq!(count, 10);
}

#[tokio::test]
async fn test_seed_insert_users() {
    let db = saf::memory_database();
    seed::insert_users(&db, "users", &[("1", "Alice"), ("2", "Bob")]).await;
    let count = db.count("users", QueryParams::new()).await.unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn test_seed_write_numbered_files() {
    let fg = TempFileGateway::new();
    let gw = fg.gateway();
    seed::write_numbered_files(&gw, 5, b"content").await;
    let list = gw.list(ListOptions::default()).await.unwrap();
    assert_eq!(list.files.len(), 5);
}

#[tokio::test]
async fn test_seed_write_named_files() {
    let fg = TempFileGateway::new();
    let gw = fg.gateway();
    seed::write_named_files(&gw, &["a.txt", "b.txt"], b"data").await;
    assert!(gw.exists("a.txt").await.unwrap());
    assert!(gw.exists("b.txt").await.unwrap());
}

#[tokio::test]
async fn test_middleware_fixtures() {
    use std::sync::Arc;
    use fixtures::middleware::{CountingPassthrough, EchoRouter, StampingPost, pipeline};

    let pre = Arc::new(CountingPassthrough::new("tag"));
    let router = Arc::new(EchoRouter::new());
    let post = Arc::new(StampingPost::new("done"));

    let p = pipeline(vec![pre.clone()], router.clone(), vec![post.clone()]);
    let out = p.execute(serde_json::json!({"x": 1})).await.unwrap();

    assert_eq!(pre.call_count(), 1);
    assert_eq!(router.call_count(), 1);
    assert_eq!(post.call_count(), 1);
    assert_eq!(out["routed"], true);
    assert_eq!(out["post_marker"], "done");
}
