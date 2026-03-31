//! `AuthMiddleware` — a `RequestMiddleware` that authenticates (and optionally authorizes)
//! every request before it reaches the router.

use std::sync::Arc;
use async_trait::async_trait;

use sst_sdk::{Authenticator, Authorizer, AuthnResult, AuthContext, Permission, AuthResult};
use crate::api::auth::{CredentialExtractor, AuthClaims};
use crate::api::middleware::{RequestMiddleware, MiddlewareAction};
use crate::api::types::{GatewayError, GatewayResult};

/// Middleware that authenticates every request via `Arc<dyn Authenticator>`.
///
/// Optionally enforces a permission check via `Arc<dyn Authorizer>`.
/// On auth failure the request is short-circuited with `GatewayError::permission_denied`.
///
/// # Type parameters
/// - `PReq` — your pipeline request type (must implement `Send + Sync + 'static`)
/// - `PResp` — your pipeline response type (must implement `Send + Sync + 'static`)
pub struct AuthMiddleware<PReq, PResp> {
    authenticator: Arc<dyn Authenticator>,
    authorizer: Option<Arc<dyn Authorizer>>,
    extractor: Arc<dyn CredentialExtractor<PReq>>,
    required_permission: Option<Permission>,
    _resp: std::marker::PhantomData<PResp>,
}

impl<PReq, PResp> AuthMiddleware<PReq, PResp>
where
    PReq: Send + Sync + 'static,
    PResp: Send + Sync + 'static,
{
    /// Create an authentication-only middleware (no authorization check).
    pub fn new(
        authenticator: Arc<dyn Authenticator>,
        extractor: Arc<dyn CredentialExtractor<PReq>>,
    ) -> Self {
        Self {
            authenticator,
            authorizer: None,
            extractor,
            required_permission: None,
            _resp: std::marker::PhantomData,
        }
    }

    /// Create a middleware that authenticates AND enforces a specific permission.
    pub fn with_authz(
        authenticator: Arc<dyn Authenticator>,
        authorizer: Arc<dyn Authorizer>,
        extractor: Arc<dyn CredentialExtractor<PReq>>,
        required_permission: Permission,
    ) -> Self {
        Self {
            authenticator,
            authorizer: Some(authorizer),
            extractor,
            required_permission: Some(required_permission),
            _resp: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<PReq, PResp> RequestMiddleware<PReq, GatewayError, PResp> for AuthMiddleware<PReq, PResp>
where
    PReq: Send + Sync + 'static,
    PResp: Send + Sync + 'static,
{
    async fn process_request(&self, req: PReq) -> GatewayResult<PReq> {
        // Delegate to process_request_action and unwrap Continue
        match self.process_request_action(req).await? {
            MiddlewareAction::Continue(r) => Ok(r),
            MiddlewareAction::ShortCircuit(_) => {
                Err(GatewayError::permission_denied("auth short-circuit"))
            }
        }
    }

    async fn process_request_action(
        &self,
        req: PReq,
    ) -> GatewayResult<MiddlewareAction<PReq, PResp>> {
        // 1. Extract credentials — if none, deny
        let creds = match self.extractor.extract(&req) {
            Some(c) => c,
            None => {
                return Err(GatewayError::permission_denied("missing credentials"));
            }
        };

        // 2. Authenticate (sync — Authenticator::authenticate is not async)
        let claims = match self.authenticator.authenticate(&creds) {
            AuthnResult::Success { subject, claims } => AuthClaims { subject, claims },
            AuthnResult::Failure(err) => {
                return Err(GatewayError::permission_denied(format!(
                    "authentication failed: {err}"
                )));
            }
        };

        // 3. Authorize (if configured)
        if let (Some(authorizer), Some(permission)) =
            (self.authorizer.as_ref(), self.required_permission.as_ref())
        {
            let context = AuthContext {
                subject: claims.subject.clone(),
                resource: String::new(),
                action: String::new(),
                environment: std::collections::HashMap::new(),
            };
            match authorizer.authorize(&context, permission) {
                AuthResult::Allow => {}
                _ => {
                    return Err(GatewayError::permission_denied(format!(
                        "subject '{}' does not have permission '{}'",
                        claims.subject, permission.0
                    )));
                }
            }
        }

        Ok(MiddlewareAction::Continue(req))
    }
}
