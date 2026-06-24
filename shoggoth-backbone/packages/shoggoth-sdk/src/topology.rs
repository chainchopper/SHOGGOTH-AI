// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/topology.rs — Hardware pool catalog & topology discovery.
//
// Models the physical hardware topology of a Shoggoth cluster, classifying
// nodes by infrastructure tier (Edge/Cloud), specialized capabilities
// (RayTracing, Tensor, APU grid, CPU brain), and available VRAM.

use std::collections::HashMap;

// ── Enums ──────────────────────────────────────────────────────────────────────

/// Whether a node is local (ultra-low latency) or remote (high throughput).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum InfrastructureTier {
    /// Local PCIe or LAN-connected hardware (RTX 5090, BC250 grid, Xeon host).
    EdgeOnPrem,
    /// Remote cloud instances provisioned via Brev.dev or similar.
    CloudScale,
}

/// Specialized hardware capabilities used for workload routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SpecializedCapability {
    /// Dedicated ray-tracing hardware (NVIDIA RT cores, OptiX BVH acceleration).
    HardwareRayTracing,
    /// Deep-learning optimized matrix cores (NVIDIA Tensor Cores, AMD Matrix Engines).
    MatrixTensorCore,
    /// Unified VRAM pool across multiple co-located APUs for raster/parallel compute.
    MassiveUnifiedAPU,
    /// Large system RAM, high core count, compilation and IO management.
    SystemCentralBrain,
    /// SR-IOV capable, multi-tenant headless cloud graphics rendering.
    VirtualizedGraphics,
}

// ── Domain Types ───────────────────────────────────────────────────────────────

/// A physical resource node registered in the Shoggoth fabric.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PhysicalResourceNode {
    /// Unique node identifier (MAC-based UUID or user-assigned hostname).
    pub node_id: String,
    /// Edge or Cloud classification.
    pub tier: InfrastructureTier,
    /// Hardware capabilities for workload routing.
    pub capabilities: Vec<SpecializedCapability>,
    /// Available VRAM / HBM in gigabytes.
    pub available_vram_gb: u32,
    /// Round-trip network latency in milliseconds to the orchestrator.
    pub network_ping_ms: f32,
    /// Whether this node is currently accepting new work.
    pub accepting_work: bool,
    /// Current GPU temperature in Celsius (for thermal throttling awareness).
    pub temperature_c: f32,
}

impl PhysicalResourceNode {
    /// Returns `true` if this node has the given capability.
    #[must_use]
    pub fn has_capability(&self, capability: SpecializedCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Returns `true` if this node meets the Full Shoggoth certification threshold.
    #[must_use]
    pub fn is_full_shoggoth(&self) -> bool {
        self.available_vram_gb >= 48
            && self.network_ping_ms < 5.0
            && self.accepting_work
    }

    /// Returns `true` if this node qualifies as a Shoggoth Limb (can contribute
    /// to the fabric but cannot serve as master scheduler).
    #[must_use]
    pub fn is_shoggoth_limb(&self) -> bool {
        self.available_vram_gb >= 8 && self.accepting_work
    }
}

// ── Fabric Pool ────────────────────────────────────────────────────────────────

/// The live hardware topology registry for the Shoggoth cluster.
///
/// Maintained by the orchestrator; updated via node heartbeat broadcasts.
/// Used by the agentic parser to route workloads to optimal hardware.
#[derive(Debug, Default)]
pub struct ShoggothFabricPool {
    /// All active nodes keyed by their node ID.
    pub active_nodes: HashMap<String, PhysicalResourceNode>,
}

impl ShoggothFabricPool {
    /// Creates an empty fabric pool.
    #[must_use]
    pub fn new() -> Self {
        Self {
            active_nodes: HashMap::new(),
        }
    }

    /// Registers or updates a node in the fabric pool.
    pub fn discover_and_register_node(&mut self, node: PhysicalResourceNode) {
        tracing::info!(
            node_id = %node.node_id,
            tier = ?node.tier,
            vram_gb = node.available_vram_gb,
            ping_ms = node.network_ping_ms,
            "Fabric discovery: registering node"
        );
        self.active_nodes.insert(node.node_id.clone(), node);
    }

    /// Removes a node from the fabric pool (e.g., heartbeat timeout).
    pub fn deregister_node(&mut self, node_id: &str) -> Option<PhysicalResourceNode> {
        let removed = self.active_nodes.remove(node_id);
        if removed.is_some() {
            tracing::warn!(node_id, "Fabric: node deregistered");
        }
        removed
    }

    /// Returns all nodes with a specific capability, sorted by ping (fastest first).
    #[must_use]
    pub fn request_pooled_resources(
        &self,
        capability: SpecializedCapability,
    ) -> Vec<&PhysicalResourceNode> {
        let mut matches: Vec<&PhysicalResourceNode> = self
            .active_nodes
            .values()
            .filter(|node| node.has_capability(capability) && node.accepting_work)
            .collect();
        matches.sort_by(|a, b| {
            a.network_ping_ms
                .partial_cmp(&b.network_ping_ms)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches
    }

    /// Returns all Full Shoggoth nodes.
    #[must_use]
    pub fn full_shoggoth_nodes(&self) -> Vec<&PhysicalResourceNode> {
        self.active_nodes
            .values()
            .filter(|n| n.is_full_shoggoth())
            .collect()
    }

    /// Returns total VRAM across all active nodes, in gigabytes.
    #[must_use]
    pub fn total_vram_gb(&self) -> f64 {
        self.active_nodes
            .values()
            .map(|n| f64::from(n.available_vram_gb))
            .sum()
    }

    /// Node count by infrastructure tier.
    #[must_use]
    pub fn node_count_by_tier(&self) -> HashMap<InfrastructureTier, usize> {
        let mut counts = HashMap::new();
        for node in self.active_nodes.values() {
            *counts.entry(node.tier).or_insert(0) += 1;
        }
        counts
    }
}

// ── Factory: Build the lab topology ────────────────────────────────────────────

/// Constructs a [`ShoggothFabricPool`] pre-populated with the current lab
/// hardware inventory.
///
/// This is the canonical hardware truth for routing decisions.
#[must_use]
pub fn build_lab_topology() -> ShoggothFabricPool {
    let mut pool = ShoggothFabricPool::new();

    // ── The Brain ──
    pool.discover_and_register_node(PhysicalResourceNode {
        node_id: "xeon-brain-01".into(),
        tier: InfrastructureTier::EdgeOnPrem,
        capabilities: vec![SpecializedCapability::SystemCentralBrain],
        available_vram_gb: 0,  // System RAM (512 GB DDR4) tracked separately
        network_ping_ms: 0.1,
        accepting_work: true,
        temperature_c: 45.0,
    });

    // ── Premium Edge Consumers (Windows / WSL2) ──
    pool.discover_and_register_node(PhysicalResourceNode {
        node_id: "rtx-5090-edge".into(),
        tier: InfrastructureTier::EdgeOnPrem,
        capabilities: vec![
            SpecializedCapability::HardwareRayTracing,
            SpecializedCapability::MatrixTensorCore,
        ],
        available_vram_gb: 32,
        network_ping_ms: 0.3,
        accepting_work: true,
        temperature_c: 52.0,
    });

    pool.discover_and_register_node(PhysicalResourceNode {
        node_id: "rtx-4090-edge".into(),
        tier: InfrastructureTier::EdgeOnPrem,
        capabilities: vec![
            SpecializedCapability::HardwareRayTracing,
            SpecializedCapability::MatrixTensorCore,
        ],
        available_vram_gb: 24,
        network_ping_ms: 0.3,
        accepting_work: true,
        temperature_c: 55.0,
    });

    // ── Compute / Legacy ──
    pool.discover_and_register_node(PhysicalResourceNode {
        node_id: "rtx-3090-compute".into(),
        tier: InfrastructureTier::EdgeOnPrem,
        capabilities: vec![
            SpecializedCapability::HardwareRayTracing,
            SpecializedCapability::MatrixTensorCore,
        ],
        available_vram_gb: 24,
        network_ping_ms: 0.4,
        accepting_work: true,
        temperature_c: 60.0,
    });

    for i in 1..=2 {
        pool.discover_and_register_node(PhysicalResourceNode {
            node_id: format!("mi50-instinct-{i:02}"),
            tier: InfrastructureTier::EdgeOnPrem,
            capabilities: vec![SpecializedCapability::MatrixTensorCore],
            available_vram_gb: 32,
            network_ping_ms: 0.4,
            accepting_work: true,
            temperature_c: 58.0,
        });
    }

    pool.discover_and_register_node(PhysicalResourceNode {
        node_id: "amd-v620-enterprise".into(),
        tier: InfrastructureTier::EdgeOnPrem,
        capabilities: vec![
            SpecializedCapability::VirtualizedGraphics,
            SpecializedCapability::MatrixTensorCore,
        ],
        available_vram_gb: 32,
        network_ping_ms: 0.4,
        accepting_work: true,
        temperature_c: 50.0,
    });

    // ── BC250 APU Grunt Worker Grid ──
    for i in 1..=12 {
        pool.discover_and_register_node(PhysicalResourceNode {
            node_id: format!("bc250-apu-{i:02}"),
            tier: InfrastructureTier::EdgeOnPrem,
            capabilities: vec![SpecializedCapability::MassiveUnifiedAPU],
            available_vram_gb: 12,
            network_ping_ms: 1.2 + (i as f32 * 0.05), // Slight variance across the switch
            accepting_work: true,
            temperature_c: 48.0,
        });
    }

    pool
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fabric_pool_registration() {
        let mut pool = ShoggothFabricPool::new();
        pool.discover_and_register_node(PhysicalResourceNode {
            node_id: "test-node".into(),
            tier: InfrastructureTier::EdgeOnPrem,
            capabilities: vec![SpecializedCapability::HardwareRayTracing],
            available_vram_gb: 24,
            network_ping_ms: 1.0,
            accepting_work: true,
            temperature_c: 50.0,
        });
        assert_eq!(pool.active_nodes.len(), 1);
    }

    #[test]
    fn test_fabric_pool_deregistration() {
        let mut pool = ShoggothFabricPool::new();
        pool.discover_and_register_node(PhysicalResourceNode {
            node_id: "ephemeral-node".into(),
            tier: InfrastructureTier::EdgeOnPrem,
            capabilities: vec![],
            available_vram_gb: 8,
            network_ping_ms: 1.0,
            accepting_work: true,
            temperature_c: 50.0,
        });
        let removed = pool.deregister_node("ephemeral-node");
        assert!(removed.is_some());
        assert!(pool.active_nodes.is_empty());
    }

    #[test]
    fn test_resource_filtering_by_capability() {
        let pool = build_lab_topology();
        let rt_nodes = pool.request_pooled_resources(SpecializedCapability::HardwareRayTracing);
        assert_eq!(rt_nodes.len(), 3); // 5090, 4090, 3090

        let apu_nodes = pool.request_pooled_resources(SpecializedCapability::MassiveUnifiedAPU);
        assert_eq!(apu_nodes.len(), 12); // 12 BC250 nodes
    }

    #[test]
    fn test_lab_topology_total_nodes() {
        let pool = build_lab_topology();
        // 1 Xeon + 5090 + 4090 + 3090 + 2 MI50 + 1 V620 + 12 BC250 = 19 nodes
        assert_eq!(pool.active_nodes.len(), 19);
    }

    #[test]
    fn test_full_shoggoth_certification() {
        let full = PhysicalResourceNode {
            node_id: "full".into(),
            tier: InfrastructureTier::EdgeOnPrem,
            capabilities: vec![],
            available_vram_gb: 48,
            network_ping_ms: 1.0,
            accepting_work: true,
            temperature_c: 50.0,
        };
        assert!(full.is_full_shoggoth());

        let limb = PhysicalResourceNode {
            node_id: "limb".into(),
            tier: InfrastructureTier::EdgeOnPrem,
            capabilities: vec![],
            available_vram_gb: 12,
            network_ping_ms: 10.0,
            accepting_work: true,
            temperature_c: 50.0,
        };
        assert!(!limb.is_full_shoggoth());
        assert!(limb.is_shoggoth_limb());
    }
}
