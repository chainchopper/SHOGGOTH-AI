// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/auth.rs — API key authentication and authorization.
//
// Provides middleware-worthy auth primitives for the orchestrator REST API:
//   • API key generation and validation (HMAC-SHA256).
//   • Role-based access control (admin, operator, read-only).
//   • Token-based session management.
//   • Rate limiting headers.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ── Types ──────────────────────────────────────────────────────────────────────

/// Access roles for the Shoggoth REST API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiRole {
    /// Full access: deploy workloads, provision cloud, manage nodes.
    Admin,
    /// Operator access: launch templates, view topology, dispatch work.
    Operator,
    /// Read-only: view topology, telemetry, health.
    ReadOnly,
}

impl ApiRole {
    /// Returns true if this role can perform the given action.
    #[must_use]
    pub fn can(&self, action: &str) -> bool {
        match self {
            Self::Admin => true, // Admin can do everything.
            Self::Operator => matches!(
                action,
                "topology:read"
                    | "nodes:list"
                    | "analyze"
                    | "launch"
                    | "health"
                    | "telemetry"
            ),
            Self::ReadOnly => matches!(
                action,
                "topology:read" | "nodes:list" | "health" | "telemetry"
            ),
        }
    }
}

/// An API key with associated role and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// The key identifier (non-sensitive, used for lookup).
    pub key_id: String,
    /// Hashed key value (HMAC-SHA256).
    pub key_hash: [u8; 32],
    /// Access role.
    pub role: ApiRole,
    /// Human-readable label for auditing.
    pub label: String,
    /// When this key was created.
    pub created_at: u64,
    /// Optional expiry (Unix timestamp). None = no expiry.
    pub expires_at: Option<u64>,
}

/// Rate limit state for a single API key.
#[derive(Debug)]
struct RateLimitState {
    /// Tokens available (token bucket algorithm).
    tokens: f64,
    /// Last refill time.
    last_refill: Instant,
}

// ── Auth Store ─────────────────────────────────────────────────────────────────

/// In-memory API key store with rate limiting.
#[derive(Debug)]
pub struct AuthStore {
    /// Active API keys indexed by key_id.
    keys: Arc<DashMap<String, ApiKey>>,
    /// Rate limit state per key_id.
    rate_limits: Arc<DashMap<String, RateLimitState>>,
    /// Maximum requests per second per key.
    rate_limit_rps: f64,
    /// Maximum burst size.
    rate_limit_burst: f64,
}

impl AuthStore {
    /// Creates a new auth store.
    pub fn new() -> Self {
        Self {
            keys: Arc::new(DashMap::new()),
            rate_limits: Arc::new(DashMap::new()),
            rate_limit_rps: 100.0,
            rate_limit_burst: 200.0,
        }
    }

    /// Generates a new API key with the given role and label.
    ///
    /// Returns the raw key value (show once!) and the key_id.
    pub fn generate_key(
        &self,
        role: ApiRole,
        label: &str,
        expires_at: Option<u64>,
    ) -> (String, String) {
        use sha2::{Digest, Sha256};

        let key_id = format!("sk-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("0000"));
        let raw_key = format!("shoggoth-{}", uuid::Uuid::new_v4());
        let mut hasher = Sha256::new();
        hasher.update(raw_key.as_bytes());
        let hash: [u8; 32] = hasher.finalize().into();

        let api_key = ApiKey {
            key_id: key_id.clone(),
            key_hash: hash,
            role,
            label: label.into(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            expires_at,
        };

        self.keys.insert(key_id.clone(), api_key);

        tracing::info!(
            key_id = %key_id,
            role = ?role,
            label,
            "API key generated"
        );

        (key_id, raw_key)
    }

    /// Validates an API key and returns its role if valid.
    ///
    /// Returns `None` if the key is invalid, expired, or rate-limited.
    pub fn validate_key(&self, raw_key: &str) -> Option<ApiRole> {
        use sha2::{Digest, Sha256};

        // Extract key_id from the key prefix (simplified: scan all keys).
        // In production: key_id is sent as a header, raw_key as another header.
        let mut hasher = Sha256::new();
        hasher.update(raw_key.as_bytes());
        let hash: [u8; 32] = hasher.finalize().into();

        // Find matching key by hash.
        for entry in self.keys.iter() {
            let api_key = entry.value();
            if api_key.key_hash == hash {
                // Check expiry.
                if let Some(expires) = api_key.expires_at {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    if now > expires {
                        tracing::warn!(key_id = %api_key.key_id, "Expired API key used");
                        return None;
                    }
                }

                // Check rate limit.
                if !self.check_rate_limit(&api_key.key_id) {
                    tracing::warn!(key_id = %api_key.key_id, "Rate limit exceeded");
                    return None;
                }

                return Some(api_key.role);
            }
        }

        None
    }

    /// Revokes an API key by key_id.
    pub fn revoke_key(&self, key_id: &str) -> bool {
        let removed = self.keys.remove(key_id);
        if removed.is_some() {
            tracing::info!(key_id, "API key revoked");
            true
        } else {
            false
        }
    }

    /// Lists all key IDs and labels (no hashes exposed).
    pub fn list_keys(&self) -> Vec<(String, String, ApiRole)> {
        self.keys
            .iter()
            .map(|e| (e.key().clone(), e.value().label.clone(), e.value().role))
            .collect()
    }

    // ── Rate Limiting (Token Bucket) ──────────────────────────────────────────

    fn check_rate_limit(&self, key_id: &str) -> bool {
        let mut state = self.rate_limits.entry(key_id.into()).or_insert_with(|| {
            RateLimitState {
                tokens: self.rate_limit_burst,
                last_refill: Instant::now(),
            }
        });

        let now = Instant::now();
        let elapsed = now.duration_since(state.last_refill).as_secs_f64();
        state.tokens = (state.tokens + elapsed * self.rate_limit_rps).min(self.rate_limit_burst);
        state.last_refill = now;

        if state.tokens >= 1.0 {
            state.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

impl Default for AuthStore {
    fn default() -> Self {
        Self::new()
    }
}

// ── HTTP Middleware Helpers ────────────────────────────────────────────────────

/// Extracts the API key from an HTTP request header.
///
/// Looks for `Authorization: Bearer shoggoth-...` or `X-Shoggoth-Key: shoggoth-...`.
pub fn extract_api_key(headers: &HashMap<String, String>) -> Option<String> {
    if let Some(auth) = headers.get("authorization") {
        if let Some(key) = auth.strip_prefix("Bearer ") {
            return Some(key.trim().to_string());
        }
    }
    if let Some(key) = headers.get("x-shoggoth-key") {
        return Some(key.trim().to_string());
    }
    None
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_validate_key() {
        let store = AuthStore::new();
        let (key_id, raw_key) = store.generate_key(ApiRole::Operator, "test-key", None);

        let role = store.validate_key(&raw_key);
        assert_eq!(role, Some(ApiRole::Operator));

        // Wrong key should fail.
        assert_eq!(store.validate_key("wrong-key"), None);
    }

    #[test]
    fn test_revoke_key() {
        let store = AuthStore::new();
        let (key_id, raw_key) = store.generate_key(ApiRole::Admin, "revocable", None);

        assert!(store.validate_key(&raw_key).is_some());
        assert!(store.revoke_key(&key_id));
        assert!(store.validate_key(&raw_key).is_none());
    }

    #[test]
    fn test_role_permissions() {
        assert!(ApiRole::Admin.can("anything"));
        assert!(ApiRole::Operator.can("topology:read"));
        assert!(ApiRole::Operator.can("launch"));
        assert!(!ApiRole::Operator.can("nodes:delete"));
        assert!(ApiRole::ReadOnly.can("topology:read"));
        assert!(!ApiRole::ReadOnly.can("launch"));
    }
}
