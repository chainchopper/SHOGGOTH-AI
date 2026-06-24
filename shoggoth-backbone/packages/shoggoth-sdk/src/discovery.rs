// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/discovery.rs — Node discovery via UDP heartbeat listener.
//
// The orchestrator binds a UDP socket and listens for broadcast heartbeat
// packets from node agents. Each heartbeat contains the node's hardware
// profile (VRAM, vendor, temperature, queue depth). The orchestrator
// registers/updates nodes in the fabric pool in real time.
//
// Also tracks heartbeat liveness: nodes that stop heartbeating are
// automatically deregistered after a configurable timeout (default 5s).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;

use crate::topology::{InfrastructureTier, PhysicalResourceNode, ShoggothFabricPool, SpecializedCapability};

// ── Wire Format ────────────────────────────────────────────────────────────────

/// A heartbeat packet received from a node agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHeartbeat {
    pub node_id: String,
    pub protocol_version: u16,
    pub available_vram_bytes: u64,
    pub temperature_c: f32,
    pub utilization_pct: f32,
    pub queue_depth: u32,
    pub accepting_work: bool,
    pub vendor: String,
    pub kernel_version: String,
}

// ── Discovery Service ──────────────────────────────────────────────────────────

/// Tracks per-node liveness state.
#[derive(Debug)]
struct NodeLiveness {
    last_heartbeat: Instant,
    registered: bool,
}

/// The discovery service: listens for UDP heartbeats and maintains the fabric pool.
#[derive(Debug)]
pub struct DiscoveryService {
    /// The fabric pool shared with the orchestrator.
    pool: Arc<Mutex<ShoggothFabricPool>>,
    /// Tracks when each node last heartbeated.
    liveness: Arc<Mutex<HashMap<String, NodeLiveness>>>,
    /// How long before a node is considered dead (deregistered).
    heartbeat_timeout: Duration,
    /// UDP bind address for the heartbeat listener.
    bind_addr: String,
}

impl DiscoveryService {
    /// Creates a new discovery service.
    pub fn new(
        pool: Arc<Mutex<ShoggothFabricPool>>,
        bind_addr: &str,
    ) -> Self {
        Self {
            pool,
            liveness: Arc::new(Mutex::new(HashMap::new())),
            heartbeat_timeout: Duration::from_secs(5),
            bind_addr: bind_addr.into(),
        }
    }

    /// Sets the heartbeat timeout after which nodes are deregistered.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.heartbeat_timeout = timeout;
        self
    }

    /// Starts the discovery loop: listens for UDP heartbeats and runs liveness checks.
    ///
    /// This function runs forever and should be spawned in a tokio task.
    pub async fn run(self) -> anyhow::Result<()> {
        let socket = UdpSocket::bind(&self.bind_addr).await?;
        tracing::info!(addr = %self.bind_addr, "Discovery service listening for heartbeats");

        let mut buf = vec![0u8; 2048];
        let liveness = self.liveness.clone();
        let pool = self.pool.clone();
        let timeout = self.heartbeat_timeout;

        // Spawn liveness checker.
        let liveness_handle = {
            let liveness = liveness.clone();
            let pool = pool.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(1));
                loop {
                    interval.tick().await;
                    Self::check_liveness(&liveness, &pool, timeout).await;
                }
            })
        };

        // Main receive loop.
        loop {
            let (len, src_addr) = socket.recv_from(&mut buf).await?;

            match serde_json::from_slice::<NodeHeartbeat>(&buf[..len]) {
                Ok(heartbeat) => {
                    Self::process_heartbeat(
                        &liveness,
                        &pool,
                        heartbeat,
                        src_addr,
                    )
                    .await;
                }
                Err(e) => {
                    tracing::warn!(
                        src = %src_addr,
                        error = %e,
                        "Malformed heartbeat packet"
                    );
                }
            }
        }
    }

    /// Processes a single heartbeat: registers/updates the node in the fabric pool.
    async fn process_heartbeat(
        liveness: &Arc<Mutex<HashMap<String, NodeLiveness>>>,
        pool: &Arc<Mutex<ShoggothFabricPool>>,
        heartbeat: NodeHeartbeat,
        src_addr: SocketAddr,
    ) {
        let mut live = liveness.lock().await;
        let node_id = heartbeat.node_id.clone();

        // Update liveness.
        live.insert(
            node_id.clone(),
            NodeLiveness {
                last_heartbeat: Instant::now(),
                registered: true,
            },
        );

        // Parse vendor to capability.
        let capabilities = classify_capabilities(&heartbeat.vendor, heartbeat.available_vram_bytes);

        // Build or update the physical resource node.
        let node = PhysicalResourceNode {
            node_id: node_id.clone(),
            tier: InfrastructureTier::EdgeOnPrem, // Cloud nodes set this via env
            capabilities,
            available_vram_gb: (heartbeat.available_vram_bytes / (1024 * 1024 * 1024)) as u32,
            network_ping_ms: 0.0, // Updated by the orchestrator via ping measurement
            accepting_work: heartbeat.accepting_work,
            temperature_c: heartbeat.temperature_c,
        };

        // Register in the fabric pool.
        let mut pool = pool.lock().await;
        if !pool.active_nodes.contains_key(&node_id) {
            tracing::info!(
                node_id = %node_id,
                vendor = %heartbeat.vendor,
                vram_gb = node.available_vram_gb,
                src = %src_addr,
                "New node discovered"
            );
        }
        pool.discover_and_register_node(node);
    }

    /// Checks liveness: deregisters nodes that haven't heartbeated within the timeout.
    async fn check_liveness(
        liveness: &Arc<Mutex<HashMap<String, NodeLiveness>>>,
        pool: &Arc<Mutex<ShoggothFabricPool>>,
        timeout: Duration,
    ) {
        let mut live = liveness.lock().await;
        let now = Instant::now();
        let mut dead_nodes = Vec::new();

        for (node_id, state) in live.iter() {
            if now.duration_since(state.last_heartbeat) > timeout {
                dead_nodes.push(node_id.clone());
            }
        }

        if !dead_nodes.is_empty() {
            let mut pool = pool.lock().await;
            for node_id in &dead_nodes {
                live.remove(node_id);
                pool.deregister_node(node_id);
                tracing::warn!(node_id = %node_id, "Node timed out — deregistered");
            }
        }
    }
}

// ── Capability Classification ──────────────────────────────────────────────────

/// Maps a vendor string and VRAM to Shoggoth capabilities.
fn classify_capabilities(vendor: &str, vram_bytes: u64) -> Vec<SpecializedCapability> {
    let mut caps = Vec::new();
    let vendor_lower = vendor.to_lowercase();
    let vram_gb = vram_bytes / (1024 * 1024 * 1024);

    // Matrix tensor cores on all modern GPUs.
    caps.push(SpecializedCapability::MatrixTensorCore);

    // NVIDIA cards with RT cores.
    if vendor_lower.contains("nvidia") {
        caps.push(SpecializedCapability::HardwareRayTracing);
    }

    // AMD APUs with unified memory (BC250 grid).
    if vendor_lower.contains("amd") && vram_gb >= 12 && vram_gb < 48 {
        caps.push(SpecializedCapability::MassiveUnifiedAPU);
    }

    // Intel QAT or other accelerators.
    if vendor_lower.contains("intel") {
        caps.push(SpecializedCapability::SystemCentralBrain);
    }

    caps
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_nvidia_rtx() {
        let caps = classify_capabilities("NVIDIA", 32 * 1024 * 1024 * 1024);
        assert!(caps.contains(&SpecializedCapability::HardwareRayTracing));
        assert!(caps.contains(&SpecializedCapability::MatrixTensorCore));
    }

    #[test]
    fn test_classify_amd_apu() {
        let caps = classify_capabilities("AMD", 12 * 1024 * 1024 * 1024);
        assert!(caps.contains(&SpecializedCapability::MassiveUnifiedAPU));
        assert!(!caps.contains(&SpecializedCapability::HardwareRayTracing));
    }

    #[test]
    fn test_heartbeat_deserialization() {
        let json = r#"{
            "node_id": "bc250-node-01",
            "protocol_version": 1,
            "available_vram_bytes": 12884901888,
            "temperature_c": 48.5,
            "utilization_pct": 12.3,
            "queue_depth": 0,
            "accepting_work": true,
            "vendor": "AMD",
            "kernel_version": "Linux 6.8.0"
        }"#;
        let hb: NodeHeartbeat = serde_json::from_str(json).unwrap();
        assert_eq!(hb.node_id, "bc250-node-01");
        assert_eq!(hb.available_vram_bytes, 12 * 1024 * 1024 * 1024);
        assert!((hb.temperature_c - 48.5).abs() < f32::EPSILON);
    }
}
