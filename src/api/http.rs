//! HTTP gateway types and configuration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl Default for HttpMethod {
    fn default() -> Self {
        Self::Get
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Get => write!(f, "GET"),
            Self::Post => write!(f, "POST"),
            Self::Put => write!(f, "PUT"),
            Self::Patch => write!(f, "PATCH"),
            Self::Delete => write!(f, "DELETE"),
            Self::Head => write!(f, "HEAD"),
            Self::Options => write!(f, "OPTIONS"),
        }
    }
}

/// HTTP client configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpConfig {
    /// Base URL for requests.
    pub base_url: Option<String>,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Connection timeout in seconds.
    pub connect_timeout_secs: u64,
    /// Maximum number of retries.
    pub max_retries: u32,
    /// Default headers to include in all requests.
    #[serde(default)]
    pub default_headers: HashMap<String, String>,
    /// Whether to follow redirects.
    pub follow_redirects: bool,
    /// Maximum number of redirects to follow.
    pub max_redirects: u32,
    /// User agent string.
    pub user_agent: Option<String>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            base_url: None,
            timeout_secs: 30,
            connect_timeout_secs: 10,
            max_retries: 3,
            default_headers: HashMap::new(),
            follow_redirects: true,
            max_redirects: 10,
            user_agent: Some("swe-gateway/0.1.0".to_string()),
        }
    }
}

impl HttpConfig {
    /// Creates a configuration with a base URL.
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            base_url: Some(base_url.into()),
            ..Default::default()
        }
    }

    /// Adds a default header.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.default_headers.insert(name.into(), value.into());
        self
    }

    /// Sets the timeout.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
}

/// An HTTP request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequest {
    /// HTTP method.
    pub method: HttpMethod,
    /// Request URL (relative to base_url if configured).
    pub url: String,
    /// Request headers.
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Query parameters.
    #[serde(default)]
    pub query: HashMap<String, String>,
    /// Request body.
    pub body: Option<HttpBody>,
    /// Request timeout override.
    pub timeout: Option<Duration>,
}

impl HttpRequest {
    /// Creates a GET request.
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Get,
            url: url.into(),
            headers: HashMap::new(),
            query: HashMap::new(),
            body: None,
            timeout: None,
        }
    }

    /// Creates a POST request.
    pub fn post(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Post,
            url: url.into(),
            headers: HashMap::new(),
            query: HashMap::new(),
            body: None,
            timeout: None,
        }
    }

    /// Creates a PUT request.
    pub fn put(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Put,
            url: url.into(),
            headers: HashMap::new(),
            query: HashMap::new(),
            body: None,
            timeout: None,
        }
    }

    /// Creates a DELETE request.
    pub fn delete(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Delete,
            url: url.into(),
            headers: HashMap::new(),
            query: HashMap::new(),
            body: None,
            timeout: None,
        }
    }

    /// Adds a header.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    /// Adds a query parameter.
    pub fn with_query(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.query.insert(name.into(), value.into());
        self
    }

    /// Sets a JSON body.
    pub fn with_json<T: Serialize>(mut self, body: &T) -> Result<Self, serde_json::Error> {
        self.body = Some(HttpBody::Json(serde_json::to_value(body)?));
        self.headers
            .insert("Content-Type".to_string(), "application/json".to_string());
        Ok(self)
    }

    /// Sets a raw body.
    pub fn with_body(mut self, body: Vec<u8>, content_type: impl Into<String>) -> Self {
        self.body = Some(HttpBody::Raw(body));
        self.headers.insert("Content-Type".to_string(), content_type.into());
        self
    }

    /// Sets a form body.
    pub fn with_form(mut self, form: HashMap<String, String>) -> Self {
        self.body = Some(HttpBody::Form(form));
        self.headers.insert(
            "Content-Type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        );
        self
    }

    /// Sets the timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

/// HTTP request body types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum HttpBody {
    /// JSON body.
    Json(serde_json::Value),
    /// Raw bytes.
    Raw(Vec<u8>),
    /// Form data.
    Form(HashMap<String, String>),
    /// Multipart form data.
    Multipart(Vec<FormPart>),
}

/// A part of a multipart form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormPart {
    /// Part name.
    pub name: String,
    /// Part filename (for file uploads).
    pub filename: Option<String>,
    /// Content type.
    pub content_type: Option<String>,
    /// Part data.
    pub data: Vec<u8>,
}

/// An HTTP response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response headers.
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Response body.
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Creates a new response.
    pub fn new(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: HashMap::new(),
            body,
        }
    }

    /// Returns true if the status indicates success (2xx).
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Returns true if the status indicates a client error (4xx).
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status)
    }

    /// Returns true if the status indicates a server error (5xx).
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status)
    }

    /// Parses the body as JSON.
    pub fn json<T: for<'de> Deserialize<'de>>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }

    /// Returns the body as a UTF-8 string.
    pub fn text(&self) -> Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.body.clone())
    }

    /// Gets a header value.
    pub fn header(&self, name: &str) -> Option<&String> {
        self.headers.get(name).or_else(|| self.headers.get(&name.to_lowercase()))
    }
}

/// Authentication method for HTTP requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HttpAuth {
    /// No authentication.
    None,
    /// Bearer token authentication.
    Bearer { token: String },
    /// Basic authentication.
    Basic { username: String, password: String },
    /// API key authentication.
    ApiKey { header: String, key: String },
}

impl Default for HttpAuth {
    fn default() -> Self {
        Self::None
    }
}

impl HttpAuth {
    /// Creates bearer token auth.
    pub fn bearer(token: impl Into<String>) -> Self {
        Self::Bearer {
            token: token.into(),
        }
    }

    /// Creates basic auth.
    pub fn basic(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::Basic {
            username: username.into(),
            password: password.into(),
        }
    }

    /// Creates API key auth.
    pub fn api_key(header: impl Into<String>, key: impl Into<String>) -> Self {
        Self::ApiKey {
            header: header.into(),
            key: key.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// @covers: get
    #[test]
    fn test_http_request_get() {
        let req = HttpRequest::get("https://example.com/api")
            .with_header("Accept", "application/json")
            .with_query("page", "1");
        assert_eq!(req.method, HttpMethod::Get);
        assert_eq!(req.url, "https://example.com/api");
        assert_eq!(req.headers.get("Accept"), Some(&"application/json".to_string()));
        assert_eq!(req.query.get("page"), Some(&"1".to_string()));
    }

    /// @covers: post
    #[test]
    fn test_http_request_post_with_json() {
        let body = serde_json::json!({"key": "value"});
        let req = HttpRequest::post("https://example.com/api")
            .with_json(&body)
            .expect("json serialization should succeed");
        assert_eq!(req.method, HttpMethod::Post);
        assert!(matches!(req.body, Some(HttpBody::Json(_))));
        assert_eq!(req.headers.get("Content-Type"), Some(&"application/json".to_string()));
    }

    #[test]
    fn test_http_method_display() {
        assert_eq!(HttpMethod::Get.to_string(), "GET");
        assert_eq!(HttpMethod::Post.to_string(), "POST");
        assert_eq!(HttpMethod::Delete.to_string(), "DELETE");
    }

    /// @covers: with_base_url
    #[test]
    fn test_with_base_url() {
        let cfg = HttpConfig::with_base_url("http://x.com");
        assert_eq!(cfg.base_url, Some("http://x.com".to_string()));
    }

    /// @covers: with_header
    #[test]
    fn test_with_header_config() {
        let cfg = HttpConfig::default().with_header("X-Key", "val");
        assert_eq!(cfg.default_headers.get("X-Key"), Some(&"val".to_string()));
    }

    /// @covers: with_timeout
    #[test]
    fn test_with_timeout_config() {
        let cfg = HttpConfig::default().with_timeout(60);
        assert_eq!(cfg.timeout_secs, 60);
    }

    /// @covers: put
    #[test]
    fn test_put() {
        let req = HttpRequest::put("/x");
        assert_eq!(req.method, HttpMethod::Put);
        assert_eq!(req.url, "/x");
    }

    /// @covers: delete
    #[test]
    fn test_delete() {
        let req = HttpRequest::delete("/x");
        assert_eq!(req.method, HttpMethod::Delete);
        assert_eq!(req.url, "/x");
    }

    /// @covers: with_header
    #[test]
    fn test_with_header_request() {
        let req = HttpRequest::get("/x").with_header("Accept", "json");
        assert_eq!(req.headers.get("Accept"), Some(&"json".to_string()));
    }

    /// @covers: with_query
    #[test]
    fn test_with_query() {
        let req = HttpRequest::get("/x").with_query("page", "1");
        assert_eq!(req.query.get("page"), Some(&"1".to_string()));
    }

    /// @covers: with_body
    #[test]
    fn test_with_body() {
        let req = HttpRequest::post("/x").with_body(vec![1, 2, 3], "application/octet-stream");
        assert!(matches!(req.body, Some(HttpBody::Raw(ref data)) if data == &[1, 2, 3]));
        assert_eq!(
            req.headers.get("Content-Type"),
            Some(&"application/octet-stream".to_string())
        );
    }

    /// @covers: with_form
    #[test]
    fn test_with_form() {
        let form = HashMap::from([("k".to_string(), "v".to_string())]);
        let req = HttpRequest::post("/x").with_form(form);
        match &req.body {
            Some(HttpBody::Form(data)) => {
                assert_eq!(data.get("k"), Some(&"v".to_string()));
            }
            other => panic!("expected Form body, got {:?}", other),
        }
    }

    /// @covers: with_timeout
    #[test]
    fn test_with_timeout_request() {
        let req = HttpRequest::get("/x").with_timeout(Duration::from_secs(5));
        assert_eq!(req.timeout, Some(Duration::from_secs(5)));
    }

    /// @covers: is_success
    #[test]
    fn test_is_success() {
        assert!(HttpResponse::new(200, vec![]).is_success());
        assert!(HttpResponse::new(299, vec![]).is_success());
        assert!(!HttpResponse::new(199, vec![]).is_success());
        assert!(!HttpResponse::new(404, vec![]).is_success());
    }

    /// @covers: is_client_error
    #[test]
    fn test_is_client_error() {
        assert!(HttpResponse::new(400, vec![]).is_client_error());
        assert!(HttpResponse::new(404, vec![]).is_client_error());
        assert!(HttpResponse::new(499, vec![]).is_client_error());
        assert!(!HttpResponse::new(200, vec![]).is_client_error());
        assert!(!HttpResponse::new(500, vec![]).is_client_error());
    }

    /// @covers: is_server_error
    #[test]
    fn test_is_server_error() {
        assert!(HttpResponse::new(500, vec![]).is_server_error());
        assert!(HttpResponse::new(503, vec![]).is_server_error());
        assert!(!HttpResponse::new(200, vec![]).is_server_error());
        assert!(!HttpResponse::new(404, vec![]).is_server_error());
    }

    /// @covers: text
    #[test]
    fn test_text() {
        let resp = HttpResponse::new(200, b"hello".to_vec());
        assert_eq!(resp.text().unwrap(), "hello");
    }

    /// @covers: header
    #[test]
    fn test_header() {
        let mut resp = HttpResponse::new(200, vec![]);
        resp.headers.insert("Content-Type".to_string(), "text/html".to_string());
        assert_eq!(resp.header("Content-Type"), Some(&"text/html".to_string()));
        assert!(resp.header("X-Missing").is_none());
    }

    /// @covers: bearer
    #[test]
    fn test_bearer() {
        let auth = HttpAuth::bearer("tok");
        assert!(matches!(auth, HttpAuth::Bearer { ref token } if token == "tok"));
    }

    /// @covers: basic
    #[test]
    fn test_basic() {
        let auth = HttpAuth::basic("user", "pass");
        assert!(matches!(
            auth,
            HttpAuth::Basic { ref username, ref password }
                if username == "user" && password == "pass"
        ));
    }

    /// @covers: api_key
    #[test]
    fn test_api_key() {
        let auth = HttpAuth::api_key("X-Api-Key", "secret");
        assert!(matches!(
            auth,
            HttpAuth::ApiKey { ref header, ref key }
                if header == "X-Api-Key" && key == "secret"
        ));
    }

    /// @covers: with_json
    #[test]
    fn test_with_json() {
        let body = serde_json::json!({"key": "value"});
        let req = HttpRequest::post("/x").with_json(&body).unwrap();
        assert!(matches!(req.body, Some(HttpBody::Json(_))));
        assert_eq!(req.headers.get("Content-Type"), Some(&"application/json".to_string()));
    }

    /// @covers: json
    #[test]
    fn test_json() {
        let data = serde_json::json!({"name": "test"});
        let resp = HttpResponse::new(200, serde_json::to_vec(&data).unwrap());
        let parsed: serde_json::Value = resp.json().unwrap();
        assert_eq!(parsed["name"], "test");
    }
}
