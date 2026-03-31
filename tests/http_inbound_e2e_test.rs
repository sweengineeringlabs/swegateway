//! End-to-end tests for HttpInbound sub-trait.
//!
//! Exercises only the HttpInbound operations through the SAF factory.
//! The RestClient's inbound handler echoes requests back, allowing
//! verification of the handle() and health_check() methods.

use swe_gateway::prelude::*;
use swe_gateway::saf::http::HttpRequest;
use swe_gateway::saf;

#[tokio::test]
async fn e2e_http_inbound_handle_get_request() {
    // impl HttpGateway implements both HttpInbound and HttpOutbound
    let client = saf::rest_client_with_base_url("https://api.example.com/v1");

    // --- HttpInbound: handle (GET) ---
    let request = HttpRequest::get("/users/42")
        .with_header("Accept", "application/json")
        .with_query("include", "profile");

    let response = client.handle(request).await.unwrap();

    assert_eq!(response.status, 200);
    let body: serde_json::Value = response.json().unwrap();

    // The mock inbound handler echoes the received request
    assert!(body.get("received").is_some(), "Handle response should contain 'received' key");
    let received = &body["received"];
    assert_eq!(received["method"], "GET");
    assert_eq!(received["url"], "/users/42");
}

#[tokio::test]
async fn e2e_http_inbound_handle_post_with_body() {
    let client = saf::rest_client_with_base_url("https://api.example.com");

    // --- HttpInbound: handle (POST) ---
    let request = HttpRequest::post("/webhooks/events")
        .with_header("X-Webhook-Signature", "sha256=abc123")
        .with_header("Content-Type", "application/json");

    let response = client.handle(request).await.unwrap();

    assert_eq!(response.status, 200);
    let body: serde_json::Value = response.json().unwrap();

    assert!(body.get("received").is_some());
    let received = &body["received"];
    assert_eq!(received["method"], "POST");
    assert_eq!(received["url"], "/webhooks/events");
}

#[tokio::test]
async fn e2e_http_inbound_handle_multiple_methods_and_health_check() {
    let client = saf::rest_client_with_base_url("https://api.example.com");

    // --- HttpInbound: handle PUT ---
    let put_request = HttpRequest::put("/resources/r1")
        .with_header("Authorization", "Bearer token123");
    let put_resp = client.handle(put_request).await.unwrap();
    assert_eq!(put_resp.status, 200);
    let put_body: serde_json::Value = put_resp.json().unwrap();
    assert_eq!(put_body["received"]["method"], "PUT");

    // --- HttpInbound: handle DELETE ---
    let del_request = HttpRequest::delete("/resources/r1");
    let del_resp = client.handle(del_request).await.unwrap();
    assert_eq!(del_resp.status, 200);
    let del_body: serde_json::Value = del_resp.json().unwrap();
    assert_eq!(del_body["received"]["method"], "DELETE");

    // --- HttpInbound: health_check ---
    let health = client.health_check().await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
}
