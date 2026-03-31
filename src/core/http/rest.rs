//! REST HTTP client implementation.

use futures::future::BoxFuture;
use std::collections::HashMap;

use crate::api::{
    http::{HttpAuth, HttpConfig, HttpRequest, HttpResponse},
    traits::{HttpGateway, HttpInbound, HttpOutbound},
    types::{GatewayResult, HealthCheck},
};

/// REST client implementation.
///
/// This is a mock/stub implementation. The full implementation requires
/// the `reqwest` feature to be enabled.
#[derive(Debug, Clone)]
pub(crate) struct RestClient {
    config: HttpConfig,
    auth: HttpAuth,
}

impl RestClient {
    /// Creates a new REST client with the given configuration.
    pub fn new(config: HttpConfig) -> Self {
        Self {
            config,
            auth: HttpAuth::None,
        }
    }

    /// Creates a REST client with a base URL.
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self::new(HttpConfig::with_base_url(base_url))
    }

    /// Sets the authentication method.
    pub fn with_auth(mut self, auth: HttpAuth) -> Self {
        self.auth = auth;
        self
    }

    /// Resolves a URL relative to the base URL.
    fn resolve_url(&self, url: &str) -> String {
        match &self.config.base_url {
            Some(base) => {
                if url.starts_with("http://") || url.starts_with("https://") {
                    url.to_string()
                } else {
                    let base = base.trim_end_matches('/');
                    let path = url.trim_start_matches('/');
                    format!("{}/{}", base, path)
                }
            }
            None => url.to_string(),
        }
    }

    /// Applies authentication to headers.
    fn apply_auth(&self, headers: &mut HashMap<String, String>) {
        match &self.auth {
            HttpAuth::None => {}
            HttpAuth::Bearer { token } => {
                headers.insert("Authorization".to_string(), format!("Bearer {}", token));
            }
            HttpAuth::Basic { username, password } => {
                use base64::Engine;
                let credentials = format!("{}:{}", username, password);
                let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
                headers.insert("Authorization".to_string(), format!("Basic {}", encoded));
            }
            HttpAuth::ApiKey { header, key } => {
                headers.insert(header.clone(), key.clone());
            }
        }
    }

    /// Builds a mock response for testing without reqwest.
    fn mock_response(&self, request: &HttpRequest) -> HttpResponse {
        // This is a mock implementation
        // The real implementation would use reqwest
        let body = serde_json::json!({
            "mock": true,
            "method": request.method.to_string(),
            "url": self.resolve_url(&request.url),
            "message": "This is a mock response. Enable the 'reqwest' feature for real HTTP requests."
        });

        HttpResponse {
            status: 200,
            headers: {
                let mut h = HashMap::new();
                h.insert("content-type".to_string(), "application/json".to_string());
                h
            },
            body: serde_json::to_vec(&body).unwrap_or_default(),
        }
    }
}

impl HttpInbound for RestClient {
    fn handle(&self, request: HttpRequest) -> BoxFuture<'_, GatewayResult<HttpResponse>> {
        Box::pin(async move {
            // For the REST client, "handling" an inbound request means
            // processing it as if we're a server. This is primarily used
            // for testing or local routing scenarios.

            // In a real implementation, this could route to handlers
            // For now, we echo back the request info
            let body = serde_json::json!({
                "received": {
                    "method": request.method.to_string(),
                    "url": request.url,
                    "headers": request.headers,
                    "query": request.query,
                }
            });

            Ok(HttpResponse {
                status: 200,
                headers: {
                    let mut h = HashMap::new();
                    h.insert("content-type".to_string(), "application/json".to_string());
                    h
                },
                body: serde_json::to_vec(&body).unwrap_or_default(),
            })
        })
    }

    fn health_check(&self) -> BoxFuture<'_, GatewayResult<HealthCheck>> {
        Box::pin(async move { Ok(HealthCheck::healthy()) })
    }
}

impl HttpOutbound for RestClient {
    fn send(&self, mut request: HttpRequest) -> BoxFuture<'_, GatewayResult<HttpResponse>> {
        // Apply default headers
        for (key, value) in &self.config.default_headers {
            request.headers.entry(key.clone()).or_insert_with(|| value.clone());
        }

        // Apply authentication
        self.apply_auth(&mut request.headers);

        Box::pin(async move {
            // Mock implementation - return a mock response
            Ok(self.mock_response(&request))
        })
    }

    fn get(&self, url: &str) -> BoxFuture<'_, GatewayResult<HttpResponse>> {
        let request = HttpRequest::get(url);
        self.send(request)
    }

    fn post_json(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> BoxFuture<'_, GatewayResult<HttpResponse>> {
        let request = HttpRequest::post(url)
            .with_json(&body)
            .unwrap_or_else(|_| HttpRequest::post(url));
        self.send(request)
    }

    fn put_json(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> BoxFuture<'_, GatewayResult<HttpResponse>> {
        let request = HttpRequest::put(url)
            .with_json(&body)
            .unwrap_or_else(|_| HttpRequest::put(url));
        self.send(request)
    }

    fn delete(&self, url: &str) -> BoxFuture<'_, GatewayResult<HttpResponse>> {
        let request = HttpRequest::delete(url);
        self.send(request)
    }
}

impl HttpGateway for RestClient {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_get() {
        let client = RestClient::with_base_url("https://api.example.com");
        let response = client.get("/users").await.unwrap();

        assert!(response.is_success());
        assert_eq!(response.status, 200);
    }

    #[tokio::test]
    async fn test_mock_post() {
        let client = RestClient::with_base_url("https://api.example.com");
        let body = serde_json::json!({"name": "test"});
        let response = client.post_json("/users", body).await.unwrap();

        assert!(response.is_success());
    }

    #[tokio::test]
    async fn test_url_resolution() {
        let client = RestClient::with_base_url("https://api.example.com/v1");

        assert_eq!(
            client.resolve_url("/users"),
            "https://api.example.com/v1/users"
        );
        assert_eq!(
            client.resolve_url("https://other.com/path"),
            "https://other.com/path"
        );
    }

    #[tokio::test]
    async fn test_auth_header() {
        let client = RestClient::with_base_url("https://api.example.com")
            .with_auth(HttpAuth::bearer("test-token"));

        let mut headers = HashMap::new();
        client.apply_auth(&mut headers);

        assert_eq!(
            headers.get("Authorization"),
            Some(&"Bearer test-token".to_string())
        );
    }

    /// @covers: with_base_url
    #[test]
    fn test_with_base_url_sets_config() {
        let client = RestClient::with_base_url("http://example.com");
        assert_eq!(
            client.resolve_url("/api/test"),
            "http://example.com/api/test",
            "with_base_url should configure the base URL for URL resolution"
        );

        // Absolute URLs should pass through unchanged
        assert_eq!(
            client.resolve_url("https://other.com/path"),
            "https://other.com/path",
            "absolute URLs should not be prefixed with base URL"
        );
    }

    /// @covers: with_base_url
    #[tokio::test]
    async fn test_with_base_url() {
        let client = RestClient::with_base_url("http://example.com");

        // Verify base URL is used by checking resolved URL via a GET request
        let response = client.get("/api/test").await.unwrap();
        assert_eq!(response.status, 200);

        // Parse mock response body to confirm the URL was resolved with the base
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["url"], "http://example.com/api/test");
    }

    /// @covers: with_auth
    #[test]
    fn test_with_auth_sets_bearer() {
        let client = RestClient::with_base_url("http://x.com")
            .with_auth(HttpAuth::bearer("my-token"));

        let mut headers = HashMap::new();
        client.apply_auth(&mut headers);
        assert_eq!(
            headers.get("Authorization"),
            Some(&"Bearer my-token".to_string()),
            "with_auth(bearer) should configure bearer auth"
        );
    }

    /// @covers: with_auth
    #[test]
    fn test_with_auth_sets_api_key() {
        let client = RestClient::with_base_url("http://x.com")
            .with_auth(HttpAuth::ApiKey {
                header: "X-Api-Key".to_string(),
                key: "secret".to_string(),
            });

        let mut headers = HashMap::new();
        client.apply_auth(&mut headers);
        assert_eq!(
            headers.get("X-Api-Key"),
            Some(&"secret".to_string()),
            "with_auth(api_key) should configure API key header"
        );
    }

    /// @covers: with_auth
    #[tokio::test]
    async fn test_with_auth() {
        let client = RestClient::with_base_url("http://x.com")
            .with_auth(HttpAuth::bearer("tok"));

        let mut headers = HashMap::new();
        client.apply_auth(&mut headers);

        assert_eq!(
            headers.get("Authorization"),
            Some(&"Bearer tok".to_string()),
            "with_auth(bearer) should set Authorization header"
        );
    }
}
