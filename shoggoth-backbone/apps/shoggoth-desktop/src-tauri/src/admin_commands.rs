// Shoggoth Dashboard — Tauri Backend: Auth + Admin Commands
//
// Add these commands to src-tauri/src/main.rs to enable:
//   • OIDC callback handling
//   • API key validation
//   • Admin: generate/revoke keys, drain nodes, view audit log, update config

use tauri::State;
use std::sync::Arc;
use tokio::sync::Mutex;

use shoggoth_sdk::auth::{ApiRole, AuthStore};
use shoggoth_sdk::oidc_auth::{OidcConfig, SessionStore};

// ── State ──────────────────────────────────────────────────────────────────────

struct AdminState {
    auth_store: Arc<AuthStore>,
    session_store: Arc<SessionStore>,
    oidc_configs: Vec<OidcConfig>,
}

// ── Auth Commands ──────────────────────────────────────────────────────────────

/// Returns the current OIDC provider URLs for the login screen.
#[tauri::command]
async fn get_auth_providers(state: State<'_, AdminState>) -> Result<Vec<String>, String> {
    Ok(state.oidc_configs.iter().map(|c| format!("{:?}", c.provider)).collect())
}

/// Validates an API key and returns a session token.
#[tauri::command]
async fn login_with_api_key(state: State<'_, AdminState>, api_key: String) -> Result<String, String> {
    let role = state.auth_store.validate_key(&api_key)
        .ok_or_else(|| "Invalid API key".to_string())?;

    let session_id = state.session_store.create_session(
        "api-key-user@shoggoth.local",
        "API Key User",
        role,
        shoggoth_sdk::oidc_auth::OidcProvider::Custom,
    );

    Ok(serde_json::to_string(&serde_json::json!({
        "session_id": session_id,
        "email": "api-key-user@shoggoth.local",
        "name": "API Key User",
        "role": format!("{:?}", role),
        "provider": "api-key",
        "expires_at": (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 3600),
    })).map_err(|e| e.to_string())?)
}

/// Validates a session token.
#[tauri::command]
async fn validate_session(state: State<'_, AdminState>, session_id: String) -> Result<String, String> {
    let session = state.session_store.validate(&session_id)
        .ok_or_else(|| "Invalid or expired session".to_string())?;
    serde_json::to_string(&session).map_err(|e| e.to_string())
}

/// Logs out (invalidates session).
#[tauri::command]
async fn logout_session(state: State<'_, AdminState>, session_id: String) -> Result<(), String> {
    state.session_store.invalidate(&session_id);
    Ok(())
}

// ── Admin Commands ─────────────────────────────────────────────────────────────

/// Generates a new API key (Admin only).
#[tauri::command]
async fn admin_generate_key(
    state: State<'_, AdminState>,
    label: String,
    role: String,
    session_id: String,
) -> Result<String, String> {
    // Verify admin session.
    let session = state.session_store.validate(&session_id)
        .ok_or_else(|| "Invalid session".to_string())?;
    if session.role != ApiRole::Admin {
        return Err("Admin role required".into());
    }

    let api_role = match role.as_str() {
        "Admin" => ApiRole::Admin,
        "Operator" => ApiRole::Operator,
        _ => ApiRole::ReadOnly,
    };

    let (key_id, raw_key) = state.auth_store.generate_key(api_role, &label, None);

    Ok(serde_json::to_string(&serde_json::json!({
        "key_id": key_id,
        "raw_key": raw_key,
    })).map_err(|e| e.to_string())?)
}

/// Revokes an API key (Admin only).
#[tauri::command]
async fn admin_revoke_key(
    state: State<'_, AdminState>,
    key_id: String,
    session_id: String,
) -> Result<bool, String> {
    let session = state.session_store.validate(&session_id)
        .ok_or_else(|| "Invalid session".to_string())?;
    if session.role != ApiRole::Admin {
        return Err("Admin role required".into());
    }
    Ok(state.auth_store.revoke_key(&key_id))
}

/// Lists all API keys (Admin only).
#[tauri::command]
async fn admin_list_keys(state: State<'_, AdminState>, session_id: String) -> Result<String, String> {
    let session = state.session_store.validate(&session_id)
        .ok_or_else(|| "Invalid session".to_string())?;
    if session.role != ApiRole::Admin {
        return Err("Admin role required".into());
    }
    serde_json::to_string(&state.auth_store.list_keys()).map_err(|e| e.to_string())
}

/// Drains a node (stops accepting new work).
#[tauri::command]
async fn admin_drain_node(node_id: String) -> Result<String, String> {
    // POST to orchestrator /admin/nodes/{node_id}/drain
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://localhost:9100/admin/nodes/{node_id}/drain"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(res.text().await.unwrap_or_default())
}

/// Returns the audit log.
#[tauri::command]
async fn admin_get_audit_log() -> Result<String, String> {
    let client = reqwest::Client::new();
    let res = client
        .get("http://localhost:9100/admin/audit")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(res.text().await.unwrap_or_default())
}
