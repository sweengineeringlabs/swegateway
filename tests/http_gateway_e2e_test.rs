//! End-to-end tests for HttpGateway.
//!
//! Exercises the combined HttpGateway trait through realistic multi-step flows:
//! configure client -> send requests -> verify responses -> handle inbound.

use swe_gateway::prelude::*;
use swe_gateway::saf::http::{HttpConfig, HttpRequest};
use swe_gateway::saf;

#[tokio::test]
async fn e2e_http_outbound_request_lifecycle() {
    let client = saf::rest_client_with_base_url("https://api.example.com/v1");

    // GET request
    let get_resp = client.get("/users").await.unwrap();
    assert!(get_resp.is_success());
    assert_eq!(get_resp.status, 200);

    // POST with JSON body
    let body = serde_json::json!({"name": "Alice", "email": "alice@example.com"});
    let post_resp = client.post_json("/users", body).await.unwrap();
    assert!(post_resp.is_success());

    // PUT with JSON body
    let update = serde_json::json!({"name": "Alice Updated"});
    let put_resp = client.put_json("/users/1", update).await.unwrap();
    assert!(put_resp.is_success());

    // DELETE
    let del_resp = client.delete("/users/1").await.unwrap();
    assert!(del_resp.is_success());
}

#[tokio::test]
async fn e2e_http_inbound_request_handling() {
    let client = saf::rest_client_with_base_url("https://api.example.com");

    // Handle an inbound request (echo mode)
    let request = HttpRequest::post("/webhooks")
        .with_header("X-Webhook-Id", "wh_123");
    let response = client.handle(request).await.unwrap();

    assert_eq!(response.status, 200);

    // Parse the echo response
    let body: serde_json::Value = response.json().unwrap();
    assert!(body.get("received").is_some());
    let received = &body["received"];
    assert_eq!(received["method"], "POST");
    assert_eq!(received["url"], "/webhooks");
}

#[tokio::test]
async fn e2e_http_send_with_custom_request() {
    let client = saf::rest_client(HttpConfig::with_base_url("https://api.example.com"));

    let request = HttpRequest::get("/health")
        .with_header("Accept", "application/json")
        .with_query("format", "detailed");

    let response = client.send(request).await.unwrap();
    assert!(response.is_success());

    // Verify mock response has expected structure
    let body: serde_json::Value = response.json().unwrap();
    assert_eq!(body["mock"], true);
}

#[tokio::test]
async fn e2e_http_health_check() {
    let client = saf::rest_client_with_base_url("https://api.example.com");

    let health = client.health_check().await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
}

#[tokio::test]
async fn e2e_http_multiple_sequential_requests() {
    let client = saf::rest_client_with_base_url("https://api.example.com");

    // Simulate a typical API workflow:
    // 1. Create a resource
    let create_resp = client
        .post_json("/orders", serde_json::json!({"item": "widget", "qty": 5}))
        .await
        .unwrap();
    assert!(create_resp.is_success());

    // 2. Fetch it
    let get_resp = client.get("/orders/1").await.unwrap();
    assert!(get_resp.is_success());

    // 3. Update it
    let update_resp = client
        .put_json("/orders/1", serde_json::json!({"qty": 10}))
        .await
        .unwrap();
    assert!(update_resp.is_success());

    // 4. Delete it
    let del_resp = client.delete("/orders/1").await.unwrap();
    assert!(del_resp.is_success());
}
