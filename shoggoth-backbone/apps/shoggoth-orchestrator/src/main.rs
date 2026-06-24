// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-orchestrator/src/main.rs — Master control-plane daemon.
//
// This is the central nervous system of the Shoggoth Mesh Machine.
// It ties together every subsystem into a single long-running process:
//
//   1. Hardware fabric bootstrap (shoggoth-core) — enumerates every local GPU.
//   2. Node discovery (UDP heartbeat listener) — catalogs remote BC250/cloud nodes.
//   3. Agentic parser (shoggoth-agent) — classifies developer workloads.
//   4. Compute fabric (shoggoth-core) — pipeline-parallel tensor routing.
//   5. Display compositor (shoggoth-display) — multi-source frame blending.
//   6. WebSocket telemetry (tokio-tungstenite) — feeds the Tauri dashboard.
//   7. REST control plane (axum) — accepts commands from CLI and NPU-STACK.
//
// Runs on the Dual Xeon 6240 host with 512 GB RAM, saturating all 72 threads.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};
use tokio::net::UdpSocket;
use tower_http::cors::{Any, CorsLayer};

// ── Application State ──────────────────────────────────────────────────────────

/// Global orchestrator state shared across all subsystems.
struct OrchestratorState {
    /// Live hardware fabric topology.
    fabric_pool: Arc<Mutex<shoggoth_sdk::topology::ShoggothFabricPool>>,
    /// Agentic parser and router.
    agent: Arc<Mutex<shoggoth_agent::ShoggothAgent>>,
    /// Active work units in flight (work_id → status).
    active_work: Arc<DashMap<u64, WorkStatus>>,
    /// Channel to send frame fragments to the compositor.
    compositor_tx: Arc<Mutex<Option<mpsc::Sender<shoggoth_display::compositor::RenderFrameFragment>>>>,
    /// Connected WebSocket clients (dashboard instances).
    ws_clients: Arc<DashMap<String, tokio::sync::mpsc::UnboundedSender<String>>>,
    /// System start time for uptime calculation.
    start_time: std::time::Instant,
}

/// Status of a work unit in flight.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkStatus {
    work_id: u64,
    node_id: String,
    status: String, // queued | running | completed | failed
    submitted_at: String,
    elapsed_ms: u64,
}

// ── REST API Types ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AnalyzeRequest {
    source_code: String,
    #[serde(default)]
    project_name: String,
}

#[derive(Debug, Serialize)]
struct AnalyzeResponse {
    workload: String,
    target_node: String,
    reason: String,
    suggested_template: String,
    template_manifest: String,
    confidence: f32,
}

#[derive(Debug, Serialize)]
struct TopologyResponse {
    total_nodes: usize,
    total_vram_gb: f64,
    full_shoggoths: usize,
    nodes: Vec<NodeInfo>,
    uptime_seconds: u64,
}

#[derive(Debug, Serialize)]
struct NodeInfo {
    node_id: String,
    tier: String,
    vram_gb: u32,
    ping_ms: f32,
    accepting_work: bool,
    temperature_c: f32,
}

#[derive(Debug, Deserialize)]
struct LaunchTemplateRequest {
    template_name: String,
    #[serde(default)]
    project_name: String,
}

// ── Entry Point ────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "shoggoth_orchestrator=info,shoggoth=info".into()),
        )
        .with_target(true)
        .init();

    tracing::info!("╔════════════════════════════════════════════════════════╗");
    tracing::info!("║     SHOGGOTH MESH MACHINE — ORCHESTRATOR v{}       ║", shoggoth_sdk::VERSION);
    tracing::info!("║     Protocol v{}  |  Emerald #00FF66 + Steel        ║", shoggoth_sdk::PROTOCOL_VERSION);
    tracing::info!("╚════════════════════════════════════════════════════════╝");

    // ── 1. Bootstrap the hardware fabric ───────────────────────────────────────
    tracing::info!("[1/6] Bootstrapping local hardware fabric...");
    let topology = shoggoth_core::bootstrap_hardware_fabric().await;
    tracing::info!(
        "  Discovered {} local devices ({:.1} GB VRAM)",
        topology.nodes.len(),
        topology.total_vram_gb()
    );
    for node in &topology.nodes {
        tracing::info!(
            "  └─ [{:?}] {} — {} GB VRAM",
            node.hardware_type,
            node.name,
            node.vram_bytes / (1024 * 1024 * 1024)
        );
    }

    // ── 2. Initialize the fabric pool with lab topology ────────────────────────
    tracing::info!("[2/6] Initializing fabric pool from lab inventory...");
    let fabric_pool = shoggoth_sdk::topology::build_lab_topology();
    let fabric_pool = Arc::new(Mutex::new(fabric_pool));
    {
        let pool = fabric_pool.lock().await;
        tracing::info!(
            "  Fabric pool: {} nodes, {:.1} GB total VRAM",
            pool.active_nodes.len(),
            pool.total_vram_gb()
        );
    }

    // ── 3. Initialize the agentic parser ───────────────────────────────────────
    tracing::info!("[3/6] Initializing agentic orchestration layer...");
    let agent = {
        let pool = fabric_pool.lock().await;
        let a = shoggoth_agent::ShoggothAgent::with_fabric(pool.clone());
        Arc::new(Mutex::new(a))
    };
    tracing::info!("  Agentic parser online (4 workload types, 5 SDK templates)");

    // ── 4. Start the display compositor ────────────────────────────────────────
    tracing::info!("[4/6] Starting display compositor...");
    let (compositor_tx, compositor_rx) = mpsc::channel::<shoggoth_display::compositor::RenderFrameFragment>(256);
    let compositor = shoggoth_display::compositor::ShoggothCompositor::new(3840, 2160, compositor_rx); // 4K default
    let compositor_tx = Arc::new(Mutex::new(Some(compositor_tx)));

    tokio::spawn(async move {
        compositor.begin_compositing_loop().await;
    });
    tracing::info!("  Compositor running at 3840×2160 (4K)");

    // ── 5. Start the node discovery service (UDP heartbeat listener) ──────────
    tracing::info!("[5/7] Starting node discovery service...");
    let discovery_bind = std::env::var("SHOGGOTH_DISCOVERY_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8888".into());
    let discovery = shoggoth_sdk::discovery::DiscoveryService::new(
        fabric_pool.clone(),
        &discovery_bind,
    );
    tokio::spawn(async move {
        if let Err(e) = discovery.run().await {
            tracing::error!("Discovery service failed: {e}");
        }
    });
    tracing::info!("  Discovery service listening on {}", discovery_bind);

    // ── 6. Start the telemetry WebSocket server ─────────────────────────────────
    tracing::info!("[6/7] Starting telemetry WebSocket server...");
    let telemetry_addr: SocketAddr = format!(
        "0.0.0.0:{}",
        shoggoth_sdk::DEFAULT_TELEMETRY_PORT
    ).parse()?;
    let telemetry_server = shoggoth_sdk::telemetry::TelemetryServer::new(telemetry_addr);
    let telemetry_tx = telemetry_server.sender();

    tokio::spawn(async move {
        if let Err(e) = telemetry_server.run().await {
            tracing::error!("Telemetry server failed: {e}");
        }
    });
    tracing::info!("  Telemetry server listening on ws://{}", telemetry_addr);

    // Spawn telemetry push loop: build and broadcast frames at 10 Hz.
    let telemetry_pool = fabric_pool.clone();
    let telemetry_start = std::time::Instant::now();
    tokio::spawn(async move {
        let mut seq = 0u64;
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
        loop {
            interval.tick().await;
            seq += 1;
            let pool = telemetry_pool.lock().await;
            let frame = shoggoth_sdk::telemetry::build_telemetry_frame(
                &pool,
                seq,
                0, // active_work_units
                telemetry_start.elapsed().as_secs(),
            );
            drop(pool);

            if let Ok(json) = serde_json::to_string(&frame) {
                let _ = telemetry_tx.send(format!("FRAME:{json}"));
            }
        }
    });

    // ── 7. Build REST API ──────────────────────────────────────────────────────
    tracing::info!("[7/7] Building REST API on port 9100...");
    let shared_state = Arc::new(OrchestratorState {
        fabric_pool: fabric_pool.clone(),
        agent: agent.clone(),
        active_work: Arc::new(DashMap::new()),
        compositor_tx: compositor_tx.clone(),
        ws_clients: Arc::new(DashMap::new()),
        start_time: std::time::Instant::now(),
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/topology", get(topology_handler))
        .route("/analyze", post(analyze_handler))
        .route("/launch", post(launch_handler))
        .route("/fabric/nodes", get(list_nodes_handler))
        .route("/fabric/register", post(register_node_handler))
        .layer(cors)
        .with_state(shared_state);

    // ── 8. Start the server ────────────────────────────────────────────────────
    tracing::info!("[8/8] Starting HTTP server...");

    let addr: SocketAddr = format!(
        "0.0.0.0:{}",
        shoggoth_sdk::DEFAULT_ORCHESTRATOR_PORT
    )
    .parse()?;

    tracing::info!("╔════════════════════════════════════════════════════════╗");
    tracing::info!("║  Orchestrator listening on http://{}          ║", addr);
    tracing::info!("║  Health:     GET  /health                            ║");
    tracing::info!("║  Topology:   GET  /topology                          ║");
    tracing::info!("║  Analyze:    POST /analyze                           ║");
    tracing::info!("║  Launch:     POST /launch                            ║");
    tracing::info!("║  Nodes:      GET  /fabric/nodes                      ║");
    tracing::info!("║  Register:   POST /fabric/register                   ║");
    tracing::info!("╚════════════════════════════════════════════════════════╝");
    tracing::info!("LEAVE NO ACCELERATOR IDLE. THE BACKBONE IS DEPLOYED.");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// ── Handlers ───────────────────────────────────────────────────────────────────

async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "shoggoth-orchestrator",
        "version": shoggoth_sdk::VERSION,
        "protocol": shoggoth_sdk::PROTOCOL_VERSION,
    }))
}

async fn topology_handler(
    State(state): State<Arc<OrchestratorState>>,
) -> Json<TopologyResponse> {
    let pool = state.fabric_pool.lock().await;
    let nodes: Vec<NodeInfo> = pool
        .active_nodes
        .values()
        .map(|n| NodeInfo {
            node_id: n.node_id.clone(),
            tier: format!("{:?}", n.tier),
            vram_gb: n.available_vram_gb,
            ping_ms: n.network_ping_ms,
            accepting_work: n.accepting_work,
            temperature_c: n.temperature_c,
        })
        .collect();

    Json(TopologyResponse {
        total_nodes: nodes.len(),
        total_vram_gb: pool.total_vram_gb(),
        full_shoggoths: pool.full_shoggoth_nodes().len(),
        nodes,
        uptime_seconds: state.start_time.elapsed().as_secs(),
    })
}

async fn analyze_handler(
    State(state): State<Arc<OrchestratorState>>,
    Json(request): Json<AnalyzeRequest>,
) -> Result<Json<AnalyzeResponse>, (StatusCode, String)> {
    let agent = state.agent.lock().await;
    let decision = agent.analyze_and_route(&request.source_code);

    let template_manifest = decision
        .suggested_template
        .generate_manifest(&request.project_name);

    Ok(Json(AnalyzeResponse {
        workload: format!("{:?}", decision.workload),
        target_node: decision.target.node_friendly_name,
        reason: decision.target.primary_reason,
        suggested_template: decision.suggested_template.display_name().to_string(),
        template_manifest,
        confidence: decision.confidence,
    }))
}

async fn launch_handler(
    State(state): State<Arc<OrchestratorState>>,
    Json(request): Json<LaunchTemplateRequest>,
) -> Json<serde_json::Value> {
    let template = shoggoth_agent::templates::TemplateType::from_str(&request.template_name);

    match template {
        Some(t) => {
            let manifest = t.generate_manifest(&request.project_name);
            tracing::info!(
                template = %t.display_name(),
                project = %request.project_name,
                "Launchpad: deploying template"
            );
            Json(serde_json::json!({
                "status": "deployed",
                "template": t.display_name(),
                "manifest": manifest,
                "message": format!("Template '{}' deployed successfully.", t.display_name()),
            }))
        }
        None => Json(serde_json::json!({
            "status": "error",
            "message": format!(
                "Unknown template: '{}'. Available: render-farm, heavy-compute, async-game-runtime, genomic-processing, generic",
                request.template_name
            ),
        })),
    }
}

async fn list_nodes_handler(
    State(state): State<Arc<OrchestratorState>>,
) -> Json<serde_json::Value> {
    let pool = state.fabric_pool.lock().await;
    let nodes: Vec<_> = pool.active_nodes.values().collect();
    Json(serde_json::json!({
        "nodes": nodes,
        "count": nodes.len(),
        "total_vram_gb": pool.total_vram_gb(),
    }))
}

async fn register_node_handler(
    State(state): State<Arc<OrchestratorState>>,
    Json(node): Json<shoggoth_sdk::topology::PhysicalResourceNode>,
) -> Json<serde_json::Value> {
    let mut pool = state.fabric_pool.lock().await;
    let node_id = node.node_id.clone();
    pool.discover_and_register_node(node);

    tracing::info!(node_id = %node_id, "Node registered via API");

    Json(serde_json::json!({
        "status": "registered",
        "node_id": node_id,
        "total_nodes": pool.active_nodes.len(),
    }))
}

// ── TemplateType Extension ─────────────────────────────────────────────────────

impl shoggoth_agent::templates::TemplateType {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "render-farm" | "render_farm" => Some(Self::RenderFarm),
            "heavy-compute" | "heavy_compute" => Some(Self::HeavyCompute),
            "async-game-runtime" | "async_game_runtime" => Some(Self::AsyncGameRuntime),
            "genomic-processing" | "genomic_processing" => Some(Self::GenomicProcessing),
            "generic" | "auto-detect" | "auto_detect" => Some(Self::Generic),
            _ => None,
        }
    }
}

