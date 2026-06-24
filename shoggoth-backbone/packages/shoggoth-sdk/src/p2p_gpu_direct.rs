// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/p2p_gpu_direct.rs — P2P GPU Direct strategy document.
//
// Defines the cross-vendor peer-to-peer GPU communication strategy for the
// Shoggoth fabric. When GPUs share a PCIe host (Xeon 512GB), direct P2P
// transfers avoid system RAM entirely — data moves GPU VRAM → GPU VRAM
// over PCIe or NVLink without CPU involvement.
//
// Supported P2P technologies:
//   • NVIDIA NVLink / NVSwitch: RTX 5090 ↔ RTX 4090 (if NVLink bridge present).
//   • AMD Infinity Fabric: MI50 #1 ↔ MI50 #2 (CDNA native).
//   • PCIe P2P (BAR1): Cross-vendor (NVIDIA ↔ AMD) via PCIe atomics.
//   • CXL 3.0: Future fabric for cache-coherent multi-GPU sharing.
//   • RDMA / GPUDirect RDMA: GPU → NIC → GPU for inter-node P2P.
//
// Strategy by hardware pair:
//
//   RTX 5090 ↔ RTX 4090 (same host):
//     NVLink Bridge (if present) → 100+ GB/s bidirectional.
//     Fallback: PCIe Gen5 P2P → 64 GB/s.
//
//   RTX 5090 ↔ AMD MI50 (same host):
//     PCIe P2P via BAR1 aperture → 32 GB/s.
//     Requires both cards in same IOMMU group (ACS disabled).
//
//   MI50 #1 ↔ MI50 #2 (same host):
//     AMD Infinity Fabric (xGMI) → 200+ GB/s.
//     Native CDNA P2P — no configuration needed.
//
//   RTX 3090 ↔ BC250 APU (over LAN):
//     GPUDirect RDMA → Mellanox/NVIDIA ConnectX NIC → 1 Gbps LAN.
//     NUMA-aware buffer placement on Xeon host.
//
//   Cloud GPU ↔ Local GPU:
//     GPUDirect RDMA over WireGuard → limited by WAN bandwidth.
//     Only tensor activations (KB), never model weights (GB).

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// ── Types ──────────────────────────────────────────────────────────────────────

/// P2P link type between two GPU nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum P2PLinkType {
    /// NVIDIA NVLink / NVSwitch bridge.
    NvLink,
    /// AMD Infinity Fabric (xGMI).
    InfinityFabric,
    /// PCIe peer-to-peer via BAR1 aperture.
    PcieBar1,
    /// GPUDirect RDMA (GPU → NIC → GPU).
    GpuDirectRdma,
    /// CXL 3.0 cache-coherent interconnect.
    Cxl3,
    /// No P2P — data must go through system RAM (slow path).
    None,
}

impl P2PLinkType {
    /// Approximate bidirectional bandwidth in GB/s.
    pub fn bandwidth_gbps(&self) -> f64 {
        match self {
            Self::NvLink => 100.0,        // NVLink 4.0, 2-lane bridge
            Self::InfinityFabric => 200.0, // xGMI v3, MI300-class
            Self::PcieBar1 => 32.0,       // PCIe Gen5 x16
            Self::GpuDirectRdma => 12.5,  // 100 Gbps ConnectX-7
            Self::Cxl3 => 64.0,           // CXL 3.0 x16
            Self::None => 0.0,
        }
    }

    /// Whether this link enables direct P2P without CPU bounce buffers.
    pub fn is_direct(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Whether this link is intra-node (same PCIe host).
    pub fn is_intra_node(&self) -> bool {
        matches!(self, Self::NvLink | Self::InfinityFabric | Self::PcieBar1 | Self::Cxl3)
    }
}

/// A P2P connection between two nodes in the fabric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PConnection {
    /// Source node ID.
    pub source: String,
    /// Destination node ID.
    pub destination: String,
    /// Link type.
    pub link_type: P2PLinkType,
    /// Measured bidirectional bandwidth in GB/s.
    pub measured_bandwidth_gbps: f64,
    /// Whether this link is currently active.
    pub active: bool,
}

// ── P2P Topology ──────────────────────────────────────────────────────────────

/// Maps the P2P connectivity topology for the Shoggoth fabric.
#[derive(Debug, Default)]
pub struct P2PTopology {
    /// All known P2P connections.
    pub connections: Vec<P2PConnection>,
    /// Adjacency matrix: (source, dest) → link type.
    adjacency: HashMap<(String, String), P2PLinkType>,
}

impl P2PTopology {
    /// Creates a topology from a list of connections.
    pub fn new(connections: Vec<P2PConnection>) -> Self {
        let mut adjacency = HashMap::new();
        for conn in &connections {
            adjacency.insert(
                (conn.source.clone(), conn.destination.clone()),
                conn.link_type,
            );
            // P2P is bidirectional.
            adjacency.insert(
                (conn.destination.clone(), conn.source.clone()),
                conn.link_type,
            );
        }
        Self { connections, adjacency }
    }

    /// Returns the P2P link type between two nodes, if any.
    pub fn link_between(&self, source: &str, dest: &str) -> P2PLinkType {
        self.adjacency
            .get(&(source.into(), dest.into()))
            .copied()
            .unwrap_or(P2PLinkType::None)
    }

    /// Returns true if direct P2P is available between two nodes.
    pub fn has_direct_p2p(&self, source: &str, dest: &str) -> bool {
        self.link_between(source, dest).is_direct()
    }

    /// Returns the total P2P bandwidth available to a node.
    pub fn total_bandwidth_gbps(&self, node_id: &str) -> f64 {
        self.connections
            .iter()
            .filter(|c| (c.source == node_id || c.destination == node_id) && c.active)
            .map(|c| c.measured_bandwidth_gbps)
            .sum()
    }
}

// ── P2P Transfer Strategy ─────────────────────────────────────────────────────
//
// When the compute fabric needs to move data between GPUs:
//
//   1. Check P2PTopology for direct link.
//   2. If direct link exists AND source/dest are on the same IOMMU group:
//      → Use cudaMemcpyPeer (NVIDIA) or hipMemcpyPeer (AMD).
//      → Zero CPU involvement. ~2µs launch overhead + bandwidth-limited transfer.
//   3. If direct link exists but cross-vendor (NVIDIA ↔ AMD):
//      → Export DMA-BUF from source GPU.
//      → Import DMA-BUF on destination GPU via DRM_PRIME_FD_TO_HANDLE.
//      → Use dma_buf_ffi.rs ioctl wrappers.
//   4. If inter-node (different physical machines):
//      → GPUDirect RDMA: GPU writes to NIC buffer directly.
//      → RDMA Read from destination GPU.
//      → Limited by network bandwidth (1 Gbps LAN → 125 MB/s max).
//      → Only tensor activations (KB-MB). Never model weights (GB).
//   5. If no P2P available:
//      → GPU → CPU (cudaMemcpyDeviceToHost).
//      → CPU → network → CPU.
//      → CPU → GPU (cudaMemcpyHostToDevice).
//      → SLOW. Only for control messages.

// ── Lab P2P Topology (Current Hardware) ───────────────────────────────────────

/// Builds the P2P topology for the current lab hardware.
pub fn build_lab_p2p_topology() -> P2PTopology {
    P2PTopology::new(vec![
        // ── Intra-Xeon PCIe P2P (all on the Dual Xeon host) ──
        P2PConnection {
            source: "rtx-5090-edge".into(),
            destination: "rtx-4090-edge".into(),
            link_type: P2PLinkType::PcieBar1,
            measured_bandwidth_gbps: 64.0,
            active: true,
        },
        P2PConnection {
            source: "rtx-5090-edge".into(),
            destination: "rtx-3090-compute".into(),
            link_type: P2PLinkType::PcieBar1,
            measured_bandwidth_gbps: 32.0,
            active: true,
        },
        P2PConnection {
            source: "mi50-instinct-01".into(),
            destination: "mi50-instinct-02".into(),
            link_type: P2PLinkType::InfinityFabric,
            measured_bandwidth_gbps: 200.0,
            active: true,
        },
        P2PConnection {
            source: "rtx-5090-edge".into(),
            destination: "mi50-instinct-01".into(),
            link_type: P2PLinkType::PcieBar1,
            measured_bandwidth_gbps: 32.0, // Cross-vendor, BAR1
            active: true,
        },
        P2PConnection {
            source: "rtx-4090-edge".into(),
            destination: "amd-v620-enterprise".into(),
            link_type: P2PLinkType::PcieBar1,
            measured_bandwidth_gbps: 32.0,
            active: true,
        },
        // ── Inter-Node (1 Gbps LAN) ──
        P2PConnection {
            source: "xeon-brain-01".into(),
            destination: "bc250-apu-01".into(),
            link_type: P2PLinkType::None, // No GPUDirect RDMA NIC available
            measured_bandwidth_gbps: 0.125, // 1 Gbps LAN
            active: true,
        },
    ])
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_p2p_bandwidths() {
        assert!((P2PLinkType::NvLink.bandwidth_gbps() - 100.0).abs() < f64::EPSILON);
        assert!((P2PLinkType::InfinityFabric.bandwidth_gbps() - 200.0).abs() < f64::EPSILON);
        assert!((P2PLinkType::PcieBar1.bandwidth_gbps() - 32.0).abs() < f64::EPSILON);
        assert!((P2PLinkType::None.bandwidth_gbps()).abs() < f64::EPSILON);
    }

    #[test]
    fn test_p2p_direct_links() {
        assert!(P2PLinkType::NvLink.is_direct());
        assert!(P2PLinkType::PcieBar1.is_direct());
        assert!(!P2PLinkType::None.is_direct());
    }

    #[test]
    fn test_lab_topology_has_connections() {
        let topo = build_lab_p2p_topology();
        assert!(!topo.connections.is_empty());
        assert!(topo.has_direct_p2p("rtx-5090-edge", "rtx-4090-edge"));
        assert!(topo.has_direct_p2p("mi50-instinct-01", "mi50-instinct-02"));
        assert!(!topo.has_direct_p2p("xeon-brain-01", "bc250-apu-01"));
    }

    #[test]
    fn test_p2p_bidirectionality() {
        let topo = build_lab_p2p_topology();
        let link_ab = topo.link_between("rtx-5090-edge", "rtx-4090-edge");
        let link_ba = topo.link_between("rtx-4090-edge", "rtx-5090-edge");
        assert_eq!(link_ab, link_ba);
        assert_eq!(link_ab, P2PLinkType::PcieBar1);
    }
}
