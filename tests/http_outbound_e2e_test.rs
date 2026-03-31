//! End-to-end tests for HttpOutbound sub-trait.
//!
//! Exercises only the HttpOutbound operations through the SAF factory.
//! The RestClient operates in mock mode — requests are recorded and
//! mock 200 responses are returned, allowing multi-step flow verification.

use swe_gateway::prelude::*;
use swe_gateway::saf::http::HttpRequest;
use swe_gateway::saf;

#[tokio::test]
async fn e2e_http_outbound_send_get_post_json() {
    // impl HttpGateway implements both HttpInbound and HttpOutbound
    let client = saf::rest_client_with_base_url("https://api.example.com/v1");

    // --- HttpOutbound: send (custom request) ---
    let custom_request = HttpRequest::get("/health")
        .with_header("Accept", "application/json")
        .with_query("verbose", "true");
    let send_resp = client.send(custom_request).await.unwrap();
    assert!(send_resp.is_success());
    let send_body: serde_json::Value = send_resp.json().unwrap();
    assert_eq!(send_body["mock"], true);

    // --- HttpOutbound: get ---
    let get_resp = client.get("/users").await.unwrap();
    assert!(get_resp.is_success());
    assert_eq!(get_resp.status, 200);

    // --- HttpOutbound: post_json ---
    let payload = serde_json::json!({
        "name": "Alice",
        "email": "alice@example.com",
        "role": "admin"
    });
    let post_resp = client.post_json("/users", payload).await.unwrap();
    assert!(post_resp.is_success());
}

#[tokio::test]
async fn e2e_http_outbound_put_json_and_delete() {
    let client = saf::rest_client_with_base_url("https://api.example.com/v1");

    // Simulate create -> update -> delete resource lifecycle via Outbound
    // Step 1: create
    let create_body = serde_json::json!({"title": "Draft Post", "status": "draft"});
    let create_resp = client.post_json("/posts", create_body).await.unwrap();
    assert!(create_resp.is_success());

    // --- HttpOutbound: put_json ---
    let update_body = serde_json::json!({"title": "Published Post", "status": "published"});
    let put_resp = client.put_json("/posts/1", update_body).await.unwrap();
    assert!(put_resp.is_success());
    assert_eq!(put_resp.status, 200);

    // --- HttpOutbound: delete ---
    let del_resp = client.delete("/posts/1").await.unwrap();
    assert!(del_resp.is_success());
    assert_eq!(del_resp.status, 200);
}

#[tokio::test]
async fn e2e_http_outbound_sequential_api_workflow() {
    let client = saf::rest_client_with_base_url("https://payments.example.com/api");

    // Multi-step outbound flow simulating a typical REST API interaction:
    // 1. Fetch resource list
    let list_resp = client.get("/orders").await.unwrap();
    assert!(list_resp.is_success());

    // 2. Create a new order
    let order_payload = serde_json::json!({
        "item_id": "sku-999",
        "quantity": 3,
        "currency": "USD"
    });
    let order_resp = client.post_json("/orders", order_payload).await.unwrap();
    assert!(order_resp.is_success());

    // 3. Update the order
    let update_payload = serde_json::json!({"quantity": 5});
    let update_resp = client.put_json("/orders/ord_1", update_payload).await.unwrap();
    assert!(update_resp.is_success());

    // 4. Fetch individual order
    let fetch_resp = client.get("/orders/ord_1").await.unwrap();
    assert!(fetch_resp.is_success());

    // 5. Cancel the order
    let cancel_resp = client.delete("/orders/ord_1").await.unwrap();
    assert!(cancel_resp.is_success());

    // Verify all responses returned mock JSON with mock=true
    for resp in [list_resp, order_resp, update_resp, fetch_resp, cancel_resp] {
        let body: serde_json::Value = resp.json().unwrap();
        assert_eq!(body["mock"], true);
    }
}
