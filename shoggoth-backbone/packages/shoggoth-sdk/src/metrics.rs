// SPDX-License-Identifier: Apache-2.0
// shoggoth-sdk/src/metrics.rs — Prometheus metrics exporter.
//
// Exposes Shoggoth fabric metrics in Prometheus format on port 9102.
// Consumed by Grafana dashboards and alerting. Metrics include:
//   • Node count, VRAM, temperature, utilization per node.
//   • Task throughput and latency histograms.
//   • Network RTT and packet loss gauges.
//   • Cloud provisioning cost counter.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use axum::{extract::State, routing::get, Router};
use prometheus_client::{encoding::text::encode, metrics::{counter::Counter, family::Family, gauge::Gauge, histogram::Histogram}, registry::Registry};

// ── Metrics ────────────────────────────────────────────────────────────────────

pub struct ShoggothMetrics {
    /// Registered Prometheus metrics.
    pub registry: Registry,

    // Node metrics.
    pub nodes_total: Gauge,
    pub nodes_online: Gauge,
    pub full_shoggoths_total: Gauge,
    pub total_vram_bytes: Gauge,

    // Per-node labels.
    pub node_vram_bytes: Family<Vec<(String, String)>, Gauge>,
    pub node_temperature_c: Family<Vec<(String, String)>, Gauge>,
    pub node_utilization_pct: Family<Vec<(String, String)>, Gauge>,
    pub node_ping_ms: Family<Vec<(String, String)>, Gauge>,

    // Work metrics.
    pub tasks_dispatched_total: Counter,
    pub tasks_completed_total: Counter,
    pub tasks_failed_total: Counter,
    pub task_latency_us: Histogram,

    // Network metrics.
    pub network_rtt_ms: Gauge,
    pub network_packet_loss_pct: Gauge,

    // Cloud metrics.
    pub cloud_nodes_provisioned: Gauge,
    pub cloud_cost_total_cents: Counter,

    // Uptime.
    pub uptime_seconds: Gauge,
}

impl ShoggothMetrics {
    /// Creates and registers all Shoggoth metrics.
    pub fn new() -> Self {
        let mut registry = Registry::default();

        let nodes_total = Gauge::default();
        registry.register("shoggoth_nodes_total", "Total nodes in the fabric", nodes_total.clone());

        let nodes_online = Gauge::default();
        registry.register("shoggoth_nodes_online", "Nodes currently accepting work", nodes_online.clone());

        let full_shoggoths_total = Gauge::default();
        registry.register("shoggoth_full_shoggoths_total", "Nodes with Full Shoggoth certification", full_shoggoths_total.clone());

        let total_vram_bytes = Gauge::default();
        registry.register("shoggoth_total_vram_bytes", "Total VRAM across all nodes", total_vram_bytes.clone());

        let node_vram_bytes = Family::default();
        registry.register("shoggoth_node_vram_bytes", "VRAM per node", node_vram_bytes.clone());

        let node_temperature_c = Family::default();
        registry.register("shoggoth_node_temperature_c", "GPU temperature per node", node_temperature_c.clone());

        let node_utilization_pct = Family::default();
        registry.register("shoggoth_node_utilization_pct", "GPU utilization per node", node_utilization_pct.clone());

        let node_ping_ms = Family::default();
        registry.register("shoggoth_node_ping_ms", "Network latency per node", node_ping_ms.clone());

        let tasks_dispatched_total = Counter::default();
        registry.register("shoggoth_tasks_dispatched_total", "Total work units dispatched", tasks_dispatched_total.clone());

        let tasks_completed_total = Counter::default();
        registry.register("shoggoth_tasks_completed_total", "Total work units completed", tasks_completed_total.clone());

        let tasks_failed_total = Counter::default();
        registry.register("shoggoth_tasks_failed_total", "Total work units failed", tasks_failed_total.clone());

        let task_latency_us = Histogram::new(vec![
            100.0, 500.0, 1_000.0, 5_000.0, 10_000.0, 50_000.0, 100_000.0, 500_000.0,
        ].into_iter());
        registry.register("shoggoth_task_latency_us", "Work unit execution latency histogram", task_latency_us.clone());

        let network_rtt_ms = Gauge::default();
        registry.register("shoggoth_network_rtt_ms", "Average network round-trip time", network_rtt_ms.clone());

        let network_packet_loss_pct = Gauge::default();
        registry.register("shoggoth_network_packet_loss_pct", "Packet loss percentage", network_packet_loss_pct.clone());

        let cloud_nodes_provisioned = Gauge::default();
        registry.register("shoggoth_cloud_nodes_provisioned", "Cloud nodes currently provisioned", cloud_nodes_provisioned.clone());

        let cloud_cost_total_cents = Counter::default();
        registry.register("shoggoth_cloud_cost_total_cents", "Total cloud provisioning cost in cents", cloud_cost_total_cents.clone());

        let uptime_seconds = Gauge::default();
        registry.register("shoggoth_uptime_seconds", "Orchestrator uptime in seconds", uptime_seconds.clone());

        Self {
            registry,
            nodes_total, nodes_online, full_shoggoths_total, total_vram_bytes,
            node_vram_bytes, node_temperature_c, node_utilization_pct, node_ping_ms,
            tasks_dispatched_total, tasks_completed_total, tasks_failed_total, task_latency_us,
            network_rtt_ms, network_packet_loss_pct, cloud_nodes_provisioned, cloud_cost_total_cents,
            uptime_seconds,
        }
    }
}

// ── Metrics Server ─────────────────────────────────────────────────────────────

/// Starts a Prometheus metrics HTTP endpoint on the given address.
pub async fn serve_metrics(metrics: Arc<ShoggothMetrics>, bind_addr: &str) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(metrics);

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!(addr = %bind_addr, "Prometheus metrics server started");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn metrics_handler(
    State(metrics): State<Arc<ShoggothMetrics>>,
) -> String {
    let mut buf = String::new();
    encode(&mut buf, &metrics.registry).unwrap_or_default();
    buf
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let m = ShoggothMetrics::new();
        let mut buf = String::new();
        encode(&mut buf, &m.registry).unwrap();
        assert!(buf.contains("shoggoth_nodes_total"));
    }
}
