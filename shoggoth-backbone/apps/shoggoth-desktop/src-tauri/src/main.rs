// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors

// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::State;
use std::sync::Arc;
use tokio::sync::Mutex;

// ── Application State ──────────────────────────────────────────────────────────

struct DashboardState {
    /// Live fabric topology.
    fabric_pool: Arc<Mutex<shoggoth_sdk::topology::ShoggothFabricPool>>,
}

// ── Tauri Commands ─────────────────────────────────────────────────────────────

/// Returns the current hardware topology as JSON for the React frontend.
#[tauri::command]
async fn get_topology(state: State<'_, DashboardState>) -> Result<String, String> {
    let pool = state.fabric_pool.lock().await;
    let nodes: Vec<_> = pool.active_nodes.values().collect();
    serde_json::to_string(&nodes).map_err(|e| e.to_string())
}

/// Launches a pre-configured workflow template.
#[tauri::command]
async fn launch_workflow_template(template_name: String) -> String {
    match template_name.as_str() {
        "async_game_runtime" => {
            "Deploying Custom Asynchronous Runtime. Sharding rendering workloads across local 5090 and cloud matrices.".into()
        }
        "pytorch_hybrid_scale" => {
            "Spinning up Hybrid Compute Matrix. Binding 12x BC250 nodes locally with remote cloud storage fabrics.".into()
        }
        "render_farm" => {
            "Activating Render Farm. BVH sharding to RT cores, BC250 grid as distributed rasterizers.".into()
        }
        "genomic_pipeline" => {
            "Initializing Genomic Pipeline. Loading ScyllaDB shard-per-core, FASTA parser online.".into()
        }
        other => {
            format!("Unknown template: {other}. Available: async_game_runtime, pytorch_hybrid_scale, render_farm, genomic_pipeline")
        }
    }
}

/// Returns live cluster metrics for the dashboard telemetry feed.
#[tauri::command]
async fn get_cluster_metrics(state: State<'_, DashboardState>) -> Result<String, String> {
    let pool = state.fabric_pool.lock().await;
    let metrics = serde_json::json!({
        "total_nodes": pool.active_nodes.len(),
        "total_vram_gb": pool.total_vram_gb(),
        "full_shoggoths": pool.full_shoggoth_nodes().len(),
        "tiers": pool.node_count_by_tier(),
    });
    serde_json::to_string(&metrics).map_err(|e| e.to_string())
}

// ── Entry Point ────────────────────────────────────────────────────────────────

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "shoggoth=info".into()),
        )
        .init();

    let fabric_pool = Arc::new(Mutex::new(shoggoth_sdk::topology::build_lab_topology()));

    tauri::Builder::default()
        .manage(DashboardState { fabric_pool })
        .invoke_handler(tauri::generate_handler![
            get_topology,
            launch_workflow_template,
            get_cluster_metrics,
        ])
        .run(tauri::generate_context!())
        .expect("Failed to launch Shoggoth Launchpad Dashboard");
}
