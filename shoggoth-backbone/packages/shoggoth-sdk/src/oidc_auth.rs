// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/oidc_auth.rs — OIDC/SSO authentication provider.
//
// Integrates the Shoggoth Dashboard and REST API with enterprise identity
// providers via OpenID Connect (OIDC). Supports:
//   • Google Workspace (G Suite).
//   • GitHub OAuth.
//   • Microsoft Entra ID (Azure AD).
//   • Keycloak (self-hosted).
//   • Any OIDC-compliant provider.
//
// Flow:
//   1. User clicks "Sign in with Google" on the Launchpad dashboard.
//   2. Dashboard redirects to the OIDC provider's authorization endpoint.
//   3. User authenticates → provider redirects back with authorization code.
//   4. Dashboard exchanges code for ID token + access token.
//   5. ID token claims are validated (issuer, audience, expiry, nonce).
//   6. User's email/group is mapped to a Shoggoth ApiRole.
//   7. Session JWT is issued for subsequent API calls.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ── Types ──────────────────────────────────────────────────────────────────────

/// Supported OIDC providers with pre-configured endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OidcProvider {
    Google,
    GitHub,
    Microsoft,
    Keycloak,
    Custom,
}

/// OIDC provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    /// Which provider.
    pub provider: OidcProvider,
    /// OAuth 2.0 client ID.
    pub client_id: String,
    /// OAuth 2.0 client secret.
    pub client_secret: String,
    /// Authorization endpoint URL.
    pub auth_url: String,
    /// Token endpoint URL.
    pub token_url: String,
    /// UserInfo endpoint URL.
    pub userinfo_url: String,
    /// JWKS (JSON Web Key Set) URL for ID token verification.
    pub jwks_url: String,
    /// Redirect URI after authentication.
    pub redirect_uri: String,
    /// OIDC scopes (space-separated: "openid profile email").
    pub scopes: String,
}

impl OidcConfig {
    /// Creates a pre-configured Google Workspace OIDC config.
    pub fn google(client_id: &str, client_secret: &str, redirect_uri: &str) -> Self {
        Self {
            provider: OidcProvider::Google,
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            userinfo_url: "https://openidconnect.googleapis.com/v1/userinfo".into(),
            jwks_url: "https://www.googleapis.com/oauth2/v3/certs".into(),
            redirect_uri: redirect_uri.into(),
            scopes: "openid profile email".into(),
        }
    }

    /// Creates a pre-configured GitHub OIDC config.
    pub fn github(client_id: &str, client_secret: &str, redirect_uri: &str) -> Self {
        Self {
            provider: OidcProvider::GitHub,
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            auth_url: "https://github.com/login/oauth/authorize".into(),
            token_url: "https://github.com/login/oauth/access_token".into(),
            userinfo_url: "https://api.github.com/user".into(),
            jwks_url: "".into(), // GitHub uses a different token format.
            redirect_uri: redirect_uri.into(),
            scopes: "read:user user:email".into(),
        }
    }

    /// Creates a pre-configured Microsoft Entra ID config.
    pub fn microsoft(tenant_id: &str, client_id: &str, client_secret: &str, redirect_uri: &str) -> Self {
        Self {
            provider: OidcProvider::Microsoft,
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            auth_url: format!("https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/authorize"),
            token_url: format!("https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/token"),
            userinfo_url: "https://graph.microsoft.com/oidc/userinfo".into(),
            jwks_url: format!("https://login.microsoftonline.com/{tenant_id}/discovery/v2.0/keys"),
            redirect_uri: redirect_uri.into(),
            scopes: "openid profile email".into(),
        }
    }
}

// ── User Session ──────────────────────────────────────────────────────────────

/// A Shoggoth user session derived from OIDC authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    /// UUID session ID.
    pub session_id: String,
    /// User's email address.
    pub email: String,
    /// User's display name.
    pub name: String,
    /// Mapped Shoggoth API role.
    pub role: crate::auth::ApiRole,
    /// OIDC provider used for authentication.
    pub provider: OidcProvider,
    /// When the session was created.
    pub created_at: u64,
    /// Session expiry (Unix timestamp).
    pub expires_at: u64,
}

// ── Session Store ─────────────────────────────────────────────────────────────

/// In-memory session store with TTL-based expiry.
#[derive(Debug)]
pub struct SessionStore {
    sessions: Arc<DashMap<String, UserSession>>,
    session_ttl_secs: u64,
}

impl SessionStore {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            session_ttl_secs: ttl_secs,
        }
    }

    /// Creates a new session for an authenticated user.
    pub fn create_session(
        &self,
        email: &str,
        name: &str,
        role: crate::auth::ApiRole,
        provider: OidcProvider,
    ) -> String {
        let session_id = uuid::Uuid::new_v4().to_string();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session = UserSession {
            session_id: session_id.clone(),
            email: email.into(),
            name: name.into(),
            role,
            provider,
            created_at: now,
            expires_at: now + self.session_ttl_secs,
        };

        self.sessions.insert(session_id.clone(), session);
        session_id
    }

    /// Validates a session by ID.
    pub fn validate(&self, session_id: &str) -> Option<UserSession> {
        let session = self.sessions.get(session_id)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now > session.expires_at {
            self.sessions.remove(session_id);
            return None;
        }

        Some(session.value().clone())
    }

    /// Invalidates (logs out) a session.
    pub fn invalidate(&self, session_id: &str) -> bool {
        self.sessions.remove(session_id).is_some()
    }

    /// Returns the number of active sessions.
    pub fn active_session_count(&self) -> usize {
        self.sessions.len()
    }
}

// ── Role Mapping ──────────────────────────────────────────────────────────────

/// Maps an OIDC user's email domain or group to a Shoggoth API role.
pub fn map_email_to_role(email: &str, admin_domains: &[String]) -> crate::auth::ApiRole {
    let domain = email.split('@').nth(1).unwrap_or("");

    if admin_domains.iter().any(|d| domain == d.as_str()) {
        crate::auth::ApiRole::Admin
    } else {
        crate::auth::ApiRole::Operator
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::ApiRole;

    #[test]
    fn test_session_create_and_validate() {
        let store = SessionStore::new(3600);
        let id = store.create_session("ops@shoggoth.local", "Operator", ApiRole::Operator, OidcProvider::Google);
        let session = store.validate(&id);
        assert!(session.is_some());
        assert_eq!(session.unwrap().email, "ops@shoggoth.local");
    }

    #[test]
    fn test_session_expiry() {
        let store = SessionStore::new(0); // Immediate expiry.
        let id = store.create_session("user@test.com", "User", ApiRole::ReadOnly, OidcProvider::Google);
        assert!(store.validate(&id).is_none());
    }

    #[test]
    fn test_session_invalidation() {
        let store = SessionStore::new(3600);
        let id = store.create_session("user@test.com", "User", ApiRole::Operator, OidcProvider::GitHub);
        assert!(store.invalidate(&id));
        assert!(store.validate(&id).is_none());
    }

    #[test]
    fn test_role_mapping_admin() {
        let role = map_email_to_role("admin@shoggoth.local", &["shoggoth.local".into()]);
        assert_eq!(role, ApiRole::Admin);
    }

    #[test]
    fn test_role_mapping_operator() {
        let role = map_email_to_role("dev@other.com", &["shoggoth.local".into()]);
        assert_eq!(role, ApiRole::Operator);
    }

    #[test]
    fn test_oidc_configs() {
        let google = OidcConfig::google("id", "secret", "http://localhost/cb");
        assert_eq!(google.provider, OidcProvider::Google);
        assert!(google.auth_url.contains("google"));

        let github = OidcConfig::github("id", "secret", "http://localhost/cb");
        assert_eq!(github.provider, OidcProvider::GitHub);

        let ms = OidcConfig::microsoft("tenant", "id", "secret", "http://localhost/cb");
        assert_eq!(ms.provider, OidcProvider::Microsoft);
        assert!(ms.auth_url.contains("microsoftonline"));
    }
}
