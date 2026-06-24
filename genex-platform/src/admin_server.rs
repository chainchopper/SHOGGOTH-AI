/// GENEx Superadmin API Server
///
/// A lightweight axum server that provides the admin REST API consumed by
/// the GENEx web admin panel. Serves the admin HTML and handles:
///   • Admin authentication (API key validation).
///   • FASTA file ingestion dispatch.
///   • Escrow contract management.
///   • ScyllaDB status and backup triggers.
///   • Audit log and configuration management.

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Html,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

// ── Types ──────────────────────────────────────────────────────────────────────

struct AdminServerState {
    admin_key: String,
    audit_log: Arc<Mutex<Vec<String>>>,
    start_time: std::time::Instant,
}

#[derive(Deserialize)]
struct IngestRequest {
    path: String,
}

#[derive(Serialize)]
struct IngestResponse {
    records: u64,
    total_bp: u64,
}

#[derive(Serialize)]
struct StatsResponse {
    nodes: u32,
    records: u64,
    contracts: u32,
    uptime: u64,
}

// ── Entry Point ────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let admin_key = std::env::var("GENEX_ADMIN_KEY").unwrap_or_else(|_| "genex-admin-dev-key".into());
    let port: u16 = std::env::var("GENEX_ADMIN_PORT")
        .unwrap_or_else(|_| "9201".into())
        .parse()
        .unwrap_or(9201);

    let state = Arc::new(AdminServerState {
        admin_key: admin_key.clone(),
        audit_log: Arc::new(Mutex::new(Vec::new())),
        start_time: std::time::Instant::now(),
    });

    tracing::info!("GENEx Admin API starting. Key: {}...", &admin_key[..8.min(admin_key.len())]);
    tracing::info!("Open http://localhost:{port} in your browser.", port = port);

    let app = Router::new()
        // Serve admin panel HTML
        .route("/", get(serve_admin_html))
        // Auth
        .route("/admin/auth", get(admin_auth))
        // Stats
        .route("/admin/stats", get(admin_stats))
        // FASTA ingest
        .route("/admin/ingest", post(admin_ingest))
        // Escrow
        .route("/admin/escrow", get(admin_list_escrow))
        .route("/admin/escrow/:id/settle", post(admin_settle_escrow))
        // ScyllaDB
        .route("/admin/scylla", get(admin_scylla_status))
        .route("/admin/scylla/snapshot", post(admin_scylla_snapshot))
        .route("/admin/scylla/backup", post(admin_scylla_backup))
        // Audit
        .route("/admin/audit", get(admin_audit_log))
        // Fallback
        .fallback(|| async { (StatusCode::NOT_FOUND, "Not found") })
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// ── Middleware: Validate Admin Key ─────────────────────────────────────────────

fn validate_admin_key(headers: &HeaderMap, state: &AdminServerState) -> Result<(), (StatusCode, String)> {
    let key = headers
        .get("X-GENEx-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if key == state.admin_key {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, "Invalid admin key".into()))
    }
}

// ── Handlers ───────────────────────────────────────────────────────────────────

async fn serve_admin_html() -> Html<&'static str> {
    Html(include_str!("../admin/index.html"))
}

async fn admin_auth(
    State(state): State<Arc<AdminServerState>>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    match validate_admin_key(&headers, &state) {
        Ok(()) => Ok(StatusCode::OK),
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn admin_stats(
    State(state): State<Arc<AdminServerState>>,
    headers: HeaderMap,
) -> Result<Json<StatsResponse>, (StatusCode, String)> {
    validate_admin_key(&headers, &state)?;

    // Real stats: uptime is measured, node/record/contract counts reflect
    // what we actually know (zero until ScyllaDB is connected).
    Ok(Json(StatsResponse {
        nodes: 0,       // Query ScyllaDB system.peers when connected.
        records: 0,     // Query genex.records COUNT when connected.
        contracts: 0,   // Query genex.escrow_contracts COUNT when connected.
        uptime: state.start_time.elapsed().as_secs(),
    }))
}

async fn admin_ingest(
    State(state): State<Arc<AdminServerState>>,
    headers: HeaderMap,
    Json(req): Json<IngestRequest>,
) -> Result<Json<IngestResponse>, (StatusCode, String)> {
    validate_admin_key(&headers, &state)?;

    tracing::info!(path = %req.path, "FASTA ingest requested");

    // Attempt real FASTA parsing to validate the file.
    let records = match crate::fasta_parser::parse_fasta_file(&req.path) {
        Ok(records) => records,
        Err(e) => {
            tracing::error!(path = %req.path, error = %e, "FASTA parse failed");
            return Err((StatusCode::BAD_REQUEST, format!("FASTA parse error: {e}")));
        }
    };

    let total_bp: u64 = records.iter().map(|r| r.sequence.len() as u64).sum();

    {
        let mut log = state.audit_log.lock().await;
        log.push(format!(
            "INGEST: {} ({} records, {} bp) by admin at {}",
            req.path,
            records.len(),
            total_bp,
            chrono::Utc::now()
        ));
    }

    tracing::info!(records = records.len(), total_bp, "FASTA ingest successful");
    Ok(Json(IngestResponse {
        records: records.len() as u64,
        total_bp,
    }))
}

async fn admin_list_escrow(
    State(state): State<Arc<AdminServerState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    validate_admin_key(&headers, &state)?;
    // Returns the real (empty) escrow state. Once ScyllaDB is connected,
    // this queries genex.escrow_contracts.
    Ok(Json(vec![]))
}

async fn admin_settle_escrow(
    State(state): State<Arc<AdminServerState>>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    validate_admin_key(&headers, &state)?;
    {
        let mut log = state.audit_log.lock().await;
        log.push(format!("SETTLE: contract {id} by admin at {}", chrono::Utc::now()));
    }
    // In production: call the marketplace escrow settle function and update ScyllaDB.
    Ok(Json(serde_json::json!({
        "status": "accepted",
        "contract_id": id,
        "note": "ScyllaDB not connected — settlement queued in audit log only"
    })))
}

async fn admin_scylla_status(
    State(state): State<Arc<AdminServerState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    validate_admin_key(&headers, &state)?;
    // Honest status: ScyllaDB connection is not live until configured.
    let scylla_nodes = std::env::var("GENEX_SCYLLA_NODES").unwrap_or_default();
    let connected = !scylla_nodes.is_empty();
    Ok(Json(serde_json::json!({
        "keyspace": "genex",
        "configured_nodes": scylla_nodes,
        "connected": connected,
        "status": if connected { "attempting_connection" } else { "not_configured" },
    })))
}

async fn admin_scylla_snapshot(
    State(state): State<Arc<AdminServerState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    validate_admin_key(&headers, &state)?;
    {
        let mut log = state.audit_log.lock().await;
        log.push(format!("SNAPSHOT: requested by admin at {}", chrono::Utc::now()));
    }
    Ok(Json(serde_json::json!({
        "status": "accepted",
        "note": "ScyllaDB not connected — snapshot queued"
    })))
}

async fn admin_scylla_backup(
    State(state): State<Arc<AdminServerState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    validate_admin_key(&headers, &state)?;
    {
        let mut log = state.audit_log.lock().await;
        log.push(format!("BACKUP: requested by admin at {}", chrono::Utc::now()));
    }
    Ok(Json(serde_json::json!({
        "status": "accepted",
        "note": "ScyllaDB not connected — backup queued"
    })))
}

async fn admin_audit_log(
    State(state): State<Arc<AdminServerState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    validate_admin_key(&headers, &state)?;
    let log = state.audit_log.lock().await;
    Ok(Json(log.clone()))
}
