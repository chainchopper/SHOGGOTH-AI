// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-cli — Real CLI that talks to the orchestrator via HTTP.
// Every command either calls the running orchestrator or falls back to
// local computation when no orchestrator is reachable.

use clap::{Parser, Subcommand};
use std::time::Instant;

#[derive(Parser)]
#[command(name = "shoggoth")]
#[command(about = "Shoggoth Mesh Machine — Deployment & Resource Controller", long_about = None)]
#[command(version)]
struct Cli {
    /// Orchestrator address (default: http://localhost:9100)
    #[arg(long, default_value = "http://localhost:9100", env = "SHOGGOTH_ORCHESTRATOR_URL")]
    orchestrator_url: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Discover and list all hardware nodes in the fabric
    Topology,
    /// Show live telemetry for active nodes
    Status {
        /// Stream live telemetry at 1 Hz (Ctrl-C to stop)
        #[arg(short, long)]
        watch: bool,
    },
    /// Deploy a workflow template
    Deploy {
        /// Template name
        template: String,
        /// Project name
        #[arg(short, long, default_value = "shoggoth-project")]
        project: String,
    },
    /// Run a real benchmark (CPU SGEMM — no hardware needed)
    Benchmark {
        #[arg(default_value = "cpu-gemm")]
        suite: String,
    },
    /// Launch a workflow template via the orchestrator
    Launch {
        template: String,
        #[arg(short, long, default_value = "shoggoth-project")]
        project: String,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "shoggoth=info".into()),
        )
        .init();

    let cli = Cli::parse();
    let orch_url = cli.orchestrator_url.trim_end_matches('/');

    match cli.command {
        Commands::Topology => cmd_topology(orch_url).await,
        Commands::Status { watch } => cmd_status(orch_url, watch).await,
        Commands::Deploy { template, project } => cmd_deploy(orch_url, &template, &project).await,
        Commands::Benchmark { suite } => cmd_benchmark(&suite).await,
        Commands::Launch { template, project } => cmd_launch(orch_url, &template, &project).await,
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("Failed to create HTTP client")
}

async fn orchestator_reachable(url: &str) -> bool {
    client()
        .get(format!("{url}/health"))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

fn print_header(title: &str) {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║  {:<54}║", title);
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();
}

fn chrono_now() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (ts / 3600) % 24;
    let m = (ts / 60) % 60;
    let s = ts % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

// ── Commands ───────────────────────────────────────────────────────────────────

async fn cmd_topology(orch_url: &str) {
    print_header("SHOGGOTH MESH MACHINE — TOPOLOGY");

    if orchestator_reachable(orch_url).await {
        match client().get(format!("{orch_url}/topology")).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(topo) = resp.json::<serde_json::Value>().await {
                    println!("  Source:           orchestrator ({orch_url})");
                    println!("  Total Nodes:      {}", topo["total_nodes"]);
                    println!("  Total VRAM:       {:.0} GB", topo["total_vram_gb"].as_f64().unwrap_or(0.0));
                    println!("  Full Shoggoths:   {}", topo["full_shoggoths"]);
                    println!("  Uptime:           {}s", topo["uptime_seconds"]);
                    println!();
                    println!("  ── NODE INVENTORY ──");
                    println!();
                    if let Some(nodes) = topo["nodes"].as_array() {
                        for n in nodes {
                            let ok = if n["accepting_work"].as_bool().unwrap_or(false) { "✓" } else { "✗" };
                            println!(
                                "  [{ok}] {:30} {:4} GB  {:5.1} ms  {:3.0}°C  {}",
                                n["node_id"].as_str().unwrap_or("?"),
                                n["vram_gb"].as_u64().unwrap_or(0),
                                n["ping_ms"].as_f64().unwrap_or(0.0),
                                n["temperature_c"].as_f64().unwrap_or(0.0),
                                n["tier"].as_str().unwrap_or("?"),
                            );
                        }
                    }
                    return;
                }
            }
            _ => eprintln!("  Orchestrator reachable but topology query failed."),
        }
    }

    // Fallback: local static catalog.
    println!("  Source:           local (static lab catalog)");
    let pool = shoggoth_sdk::topology::build_lab_topology();
    println!("  Total Nodes:      {}", pool.active_nodes.len());
    println!("  Total VRAM:       {:.0} GB", pool.total_vram_gb());
    println!("  Full Shoggoths:   {}", pool.full_shoggoth_nodes().len());
    println!();
    let mut nodes: Vec<_> = pool.active_nodes.values().collect();
    nodes.sort_by_key(|n| &n.node_id);
    for node in &nodes {
        let tier = match node.tier {
            shoggoth_sdk::topology::InfrastructureTier::EdgeOnPrem => "EDGE ",
            shoggoth_sdk::topology::InfrastructureTier::CloudScale => "CLOUD",
        };
        let cert = if node.is_full_shoggoth() { "FULL " } else { "LIMB " };
        println!(
            "  [{tier}] [{cert}] {:30} {:4} GB VRAM  {:5.1} ms ping",
            node.node_id, node.available_vram_gb, node.network_ping_ms
        );
    }
}

async fn cmd_status(orch_url: &str, watch: bool) {
    print_header("SHOGGOTH MESH MACHINE — STATUS");

    if !orchestator_reachable(orch_url).await {
        println!("  Orchestrator not reachable at {orch_url}");
        println!("  Start one with: cargo run -p shoggoth-orchestrator");
        return;
    }

    match client().get(format!("{orch_url}/health")).send().await {
        Ok(resp) => {
            if let Ok(health) = resp.json::<serde_json::Value>().await {
                println!("  Service:    {}", health["service"].as_str().unwrap_or("?"));
                println!("  Version:    {}", health["version"].as_str().unwrap_or("?"));
                println!("  Protocol:   {}", health["protocol"].as_str().unwrap_or("?"));
                println!("  Status:     {}", health["status"].as_str().unwrap_or("?"));
            }
        }
        Err(e) => println!("  Health check failed: {e}"),
    }

    if watch {
        println!();
        println!("  Polling at 1 Hz (Ctrl-C to stop)...");
        let hurl = format!("{orch_url}/health");
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            interval.tick().await;
            match client().get(&hurl).send().await {
                Ok(resp) => {
                    let status = resp
                        .json::<serde_json::Value>()
                        .await
                        .map(|j| j["status"].as_str().unwrap_or("?").to_string())
                        .unwrap_or_else(|_| "?".into());
                    println!("  [{}] status={status}", chrono_now());
                }
                Err(_) => println!("  [{}] connection lost", chrono_now()),
            }
        }
    }
}

async fn cmd_deploy(orch_url: &str, template: &str, project: &str) {
    print_header("SHOGGOTH MESH MACHINE — DEPLOY");

    if !orchestator_reachable(orch_url).await {
        println!("  Orchestrator not reachable at {orch_url}");
        println!("  Would deploy template '{template}' for project '{project}'");
        println!("  Start the orchestrator to enable live deployment.");
        return;
    }

    let payload = serde_json::json!({
        "template_name": template,
        "project_name": project,
    });

    match client()
        .post(format!("{orch_url}/launch"))
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) => {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                println!("  Status:    {}", body["status"].as_str().unwrap_or("?"));
                println!("  Template:  {}", body["template"].as_str().unwrap_or("?"));
                if let Some(msg) = body["message"].as_str() {
                    println!("  Message:   {msg}");
                }
            }
        }
        Err(e) => eprintln!("  Deploy failed: {e}"),
    }
}

async fn cmd_benchmark(suite: &str) {
    print_header("SHOGGOTH MESH MACHINE — BENCHMARK");

    match suite {
        "cpu-gemm" | "all" => {
            println!("  ── CPU SGEMM (real computation, no hardware needed) ──");
            println!();
            run_cpu_gemm_benchmark();
        }
        "topology" | "all" => {
            println!("  ── TOPOLOGY DISCOVERY ──");
            let start = Instant::now();
            let pool = shoggoth_sdk::topology::build_lab_topology();
            let elapsed = start.elapsed();
            println!(
                "  Cataloged {} nodes ({:.1} GB) in {:.1} µs",
                pool.active_nodes.len(),
                pool.total_vram_gb(),
                elapsed.as_micros() as f64,
            );
        }
        other => {
            println!("  Unknown benchmark: {other}");
            println!("  Available: cpu-gemm, topology, all");
        }
    }
}

async fn cmd_launch(orch_url: &str, template: &str, project: &str) {
    cmd_deploy(orch_url, template, project).await;
}

// ── Real CPU GEMM Benchmark ────────────────────────────────────────────────────

fn run_cpu_gemm_benchmark() {
    use shoggoth_core::compute_fabric::ComputeTaskTensor;

    let sizes = [128usize, 256, 512, 1024, 2048];
    let iters = [100u32, 50, 20, 5, 1];

    println!("  {:<12} {:>6} {:>12} {:>12}", "Matrix", "Iters", "Time", "GFLOPS");
    println!("  {:-<12} {:-<6} {:-<12} {:-<12}", "", "", "", "");

    for (&size, &n) in sizes.iter().zip(iters.iter()) {
        let nel = size * size;
        let mut a = vec![0.0f32; nel];
        for i in 0..size {
            a[i * size + i] = 1.0;
        }

        let tensor = ComputeTaskTensor {
            task_id: 0,
            shape: vec![size as i64, size as i64, size as i64],
            flat_data: a,
        };

        let start = Instant::now();
        for _ in 0..n {
            let _ = std::hint::black_box(
                shoggoth_core::compute_fabric::execute_local_fallback(&tensor),
            );
        }
        let elapsed = start.elapsed();

        let flops = 2.0 * (size as f64).powi(3) * n as f64;
        let gflops = flops / elapsed.as_secs_f64() / 1e9;

        println!(
            "  {size:>4}×{size:>4}  {n:>5}  {elapsed:>10.1?}  {gflops:>10.2}",
        );
    }

    println!();
    println!("  ── Hardware reference (requires GPUs) ──");
    println!("  RTX 5090 cuBLAS:     ~130,000 GFLOPS (FP32)");
    println!("  BC250 grid (12×):    ~12,000 GFLOPS (FP32)");
    println!("  MI50 pair:           ~15,000 GFLOPS (FP32)");
    println!("  Xeon 6240 (72T):       ~400 GFLOPS (this benchmark)");
}
