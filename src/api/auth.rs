//! Auth abstractions for pipeline-based authentication and authorization.
//!
//! Wire `AuthMiddleware` into your pre-middleware chain. It calls `Arc<dyn Authenticator>`
//! to verify credentials and, optionally, `Arc<dyn Authorizer>` to enforce permissions.
//!
//! Implement `CredentialExtractor` to pull credentials out of your `PipeReq` type.

use std::collections::HashMap;

pub use sst_sdk::{Authenticator, Authorizer, Credentials, AuthnResult, AuthContext, Permission, AuthResult};

/// Extract credentials from a pipeline request type.
///
/// Implement this on a struct that knows how your `PipeReq` carries auth data.
pub trait CredentialExtractor<PReq>: Send + Sync {
    /// Pull credentials out of the pipeline request, or return `None` to skip auth.
    fn extract(&self, req: &PReq) -> Option<Credentials>;
}

/// Claims returned on successful authentication.
#[derive(Debug, Clone)]
pub struct AuthClaims {
    /// The authenticated subject (user id / service account).
    pub subject: String,
    /// Provider-supplied claims (email, roles, etc.)
    pub claims: HashMap<String, String>,
}
