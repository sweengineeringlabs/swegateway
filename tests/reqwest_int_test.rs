//! Integration tests for reqwest dependency usage.

use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};

#[test]
fn test_reqwest_client_builder() {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("client should build successfully");
    drop(client);
}

#[test]
fn test_reqwest_header_map() {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    assert_eq!(headers.get(CONTENT_TYPE).unwrap(), "application/json");
}
