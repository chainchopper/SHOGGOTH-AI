// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/telemetry.rs — WebSocket telemetry server for the dashboard.
//
// Provides a lightweight WebSocket server (tokio-tungstenite) that streams
// live fabric metrics to all connected dashboard clients at ~10 Hz.
//
// Metrics streamed:
//   • Per-node: VRAM usage, temperature, utilization, queue depth, ping.
//   • Aggregate: total nodes, total VRAM, full shoggoth count.
//   • Events: node join, node leave, workload dispatch, benchmark results.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::topology::ShoggothFabricPool;

// ── Types ──────────────────────────────────────────────────────────────────────

/// A single telemetry frame broadcast to all dashboard clients.
#[derive(Debug, Clone, Serialize)]
pub struct TelemetryFrame {
    /// Frame sequence number.
    pub seq: u64,
    /// Unix timestamp (seconds since epoch).
    pub timestamp_secs: f64,
    /// Per-node metrics.
    pub nodes: Vec<NodeTelemetry>,
    /// Aggregate metrics.
    pub aggregate: AggregateMetrics,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeTelemetry {
    pub node_id: String,
    pub vram_gb: u32,
    pub temperature_c: f32,
    pub utilization_pct: f32,
    pub queue_depth: u32,
    pub ping_ms: f32,
    pub accepting_work: bool,
    pub vendor: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AggregateMetrics {
    pub total_nodes: usize,
    pub online_nodes: usize,
    pub total_vram_gb: f64,
    pub full_shoggoths: usize,
    pub active_work_units: u64,
    pub uptime_seconds: u64,
}

/// An event pushed to dashboard clients (node join, workload dispatch, etc.).
#[derive(Debug, Clone, Serialize)]
pub struct TelemetryEvent {
    pub event_type: String, // "node_joined", "node_left", "work_dispatched", "benchmark_complete"
    pub node_id: Option<String>,
    pub message: String,
    pub timestamp_secs: f64,
}

// ── Telemetry Server ───────────────────────────────────────────────────────────

/// A lightweight WebSocket server for streaming fabric telemetry to dashboards.
#[derive(Debug)]
pub struct TelemetryServer {
    /// Broadcast channel: every connected client receives frames from this.
    frame_tx: broadcast::Sender<String>,
    /// Track connected clients for metrics.
    client_count: Arc<DashMap<SocketAddr, ()>>,
    /// Bind address.
    bind_addr: SocketAddr,
}

impl TelemetryServer {
    /// Creates a new telemetry server.
    pub fn new(bind_addr: SocketAddr) -> Self {
        let (frame_tx, _) = broadcast::channel::<String>(64);
        Self {
            frame_tx,
            client_count: Arc::new(DashMap::new()),
            bind_addr,
        }
    }

    /// Returns a clone of the broadcast sender for pushing frames.
    pub fn sender(&self) -> broadcast::Sender<String> {
        self.frame_tx.clone()
    }

    /// Runs the telemetry server: accepts WebSocket connections and relays frames.
    ///
    /// Spawn this in a tokio task. It runs forever.
    pub async fn run(self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(&self.bind_addr).await?;
        tracing::info!(
            addr = %self.bind_addr,
            "Telemetry WebSocket server listening"
        );

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            let frame_rx = self.frame_tx.subscribe();
            let client_count = self.client_count.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_telemetry_client(stream, peer_addr, frame_rx, client_count).await {
                    tracing::warn!(peer = %peer_addr, error = %e, "Telemetry client error");
                }
            });
        }
    }

    /// Returns the number of connected dashboard clients.
    pub fn connected_clients(&self) -> usize {
        self.client_count.len()
    }
}

/// Handles a single WebSocket dashboard client connection.
async fn handle_telemetry_client(
    stream: TcpStream,
    peer_addr: SocketAddr,
    mut frame_rx: broadcast::Receiver<String>,
    client_count: Arc<DashMap<SocketAddr, ()>>,
) -> anyhow::Result<()> {
    let ws_stream = accept_async(stream).await?;
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    client_count.insert(peer_addr, ());
    tracing::info!(peer = %peer_addr, "Dashboard client connected ({} total)", client_count.len());

    // Relay frames from the broadcast channel to the WebSocket.
    loop {
        tokio::select! {
            frame = frame_rx.recv() => {
                match frame {
                    Ok(json) => {
                        if ws_sender.send(Message::Text(json.into())).await.is_err() {
                            break; // Client disconnected.
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(peer = %peer_addr, skipped = n, "Telemetry client lagging");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {} // Ignore other messages from client.
                }
            }
        }
    }

    client_count.remove(&peer_addr);
    tracing::info!(peer = %peer_addr, "Dashboard client disconnected ({} remaining)", client_count.len());
    Ok(())
}

// ── Frame Builder ──────────────────────────────────────────────────────────────

/// Builds a telemetry frame from the current fabric pool state.
pub fn build_telemetry_frame(
    pool: &ShoggothFabricPool,
    seq: u64,
    active_work_units: u64,
    uptime_seconds: u64,
) -> TelemetryFrame {
    let nodes: Vec<NodeTelemetry> = pool
        .active_nodes
        .values()
        .map(|n| NodeTelemetry {
            node_id: n.node_id.clone(),
            vram_gb: n.available_vram_gb,
            temperature_c: n.temperature_c,
            utilization_pct: 0.0, // Updated per-node in production
            queue_depth: 0,
            ping_ms: n.network_ping_ms,
            accepting_work: n.accepting_work,
            vendor: "unknown".into(), // Populated from heartbeat
        })
        .collect();

    let online_count = nodes.iter().filter(|n| n.accepting_work).count();

    TelemetryFrame {
        seq,
        timestamp_secs: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64(),
        nodes,
        aggregate: AggregateMetrics {
            total_nodes: pool.active_nodes.len(),
            online_nodes: online_count,
            total_vram_gb: pool.total_vram_gb(),
            full_shoggoths: pool.full_shoggoth_nodes().len(),
            active_work_units,
            uptime_seconds,
        },
    }
}

/// Builds and broadcasts a telemetry event to all dashboard clients.
pub fn push_telemetry_event(
    sender: &broadcast::Sender<String>,
    event_type: &str,
    node_id: Option<&str>,
    message: &str,
) {
    let event = TelemetryEvent {
        event_type: event_type.into(),
        node_id: node_id.map(String::from),
        message: message.into(),
        timestamp_secs: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64(),
    };

    if let Ok(json) = serde_json::to_string(&event) {
        let _ = sender.send(format!("EVENT:{json}"));
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology;

    #[test]
    fn test_build_telemetry_frame() {
        let pool = topology::build_lab_topology();
        let frame = build_telemetry_frame(&pool, 0, 0, 3600);
        assert_eq!(frame.seq, 0);
        assert_eq!(frame.nodes.len(), 19);
        assert_eq!(frame.aggregate.total_nodes, 19);
        assert_eq!(frame.aggregate.uptime_seconds, 3600);
    }

    #[test]
    fn test_telemetry_event_serialization() {
        let event = TelemetryEvent {
            event_type: "node_joined".into(),
            node_id: Some("bc250-01".into()),
            message: "BC250 APU node joined the fabric".into(),
            timestamp_secs: 1234567890.0,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("node_joined"));
        assert!(json.contains("bc250-01"));
    }
}
