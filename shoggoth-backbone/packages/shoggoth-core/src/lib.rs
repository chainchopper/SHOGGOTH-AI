// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-core/src/lib.rs — Core distributed execution engine.
//
// Responsibilities:
//   • Hardware fabric bootstrap: enumerate every Vulkan/DX12 adapter on the host
//     and bind wgpu Device+Queue handles for cross-vendor dispatch.
//   • Work-stealing thread saturator: lock-free MPMC task queues across all 72 Xeon
//     threads with NUMA-aware affinity hints.
//   • JIT SPIR-V shader compiler: GLSL compute kernels → SPIR-V at runtime via shaderc.
//   • Memory fabric: DMA-BUF zero-copy export/import across co-located PCIe GPUs.
//   • Compute fabric: pipeline-parallel tensor routing across heterogeneous devices.

pub mod compute_fabric;
pub mod dma_buf_ffi;
pub mod memory_fabric;
pub mod qat_compress;
pub mod thread_saturator;
pub mod wgpu_dispatch;

use std::sync::Arc;
use wgpu::{Adapter, Backends, Device, Instance, InstanceDescriptor, PowerPreference, Queue};

// ── Public Type Exports ────────────────────────────────────────────────────────

/// Represents a single physical GPU or accelerator device bound into the Shoggoth fabric.
#[derive(Debug)]
pub struct ShoggothNode {
    /// Human-readable device name from the driver (e.g. "NVIDIA GeForce RTX 5090").
    pub name: String,
    /// Classified hardware vendor for routing decisions.
    pub hardware_type: HardwareVendor,
    /// Total video memory in bytes as reported by the adapter.
    pub vram_bytes: u64,
    /// wgpu logical device handle — all submissions go through this.
    pub device: Arc<Device>,
    /// wgpu command queue — the single submission point for this device.
    pub queue: Arc<Queue>,
    /// Raw adapter info for capability introspection (features, limits, driver version).
    pub adapter_info: wgpu::AdapterInfo,
}

/// Vendor classification used by the orchestrator to route workloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HardwareVendor {
    Nvidia,
    Amd,
    Intel,
    Unknown,
}

impl std::fmt::Display for HardwareVendor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nvidia => write!(f, "NVIDIA"),
            Self::Amd => write!(f, "AMD"),
            Self::Intel => write!(f, "Intel"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Result of fabric bootstrap — all discovered nodes and the wgpu instance.
#[derive(Debug)]
pub struct FabricTopology {
    pub instance: Instance,
    pub nodes: Vec<ShoggothNode>,
}

impl FabricTopology {
    /// Returns the total unified VRAM across all discovered nodes.
    #[must_use]
    pub fn total_vram_gb(&self) -> f64 {
        self.nodes.iter().map(|n| n.vram_bytes as f64).sum::<f64>() / (1024.0 * 1024.0 * 1024.0)
    }

    /// Lists nodes filtered by vendor.
    #[must_use]
    pub fn nodes_by_vendor(&self, vendor: HardwareVendor) -> Vec<&ShoggothNode> {
        self.nodes
            .iter()
            .filter(|n| n.hardware_type == vendor)
            .collect()
    }

    /// Count of nodes with at least `min_vram_gb` of video memory.
    #[must_use]
    pub fn node_count_above_vram(&self, min_vram_gb: u64) -> usize {
        let min_bytes = min_vram_gb * 1024 * 1024 * 1024;
        self.nodes
            .iter()
            .filter(|n| n.vram_bytes >= min_bytes)
            .count()
    }
}

// ── Hardware Fabric Bootstrap ──────────────────────────────────────────────────

/// Discovers every available physical GPU/accelerator on the host system.
///
/// Creates a wgpu instance targeting Vulkan and DirectX 12 backends, enumerates
/// all adapters, requests logical devices with default limits, and packages each
/// into a [`ShoggothNode`] with vendor classification.
///
/// # Panics
///
/// Panics if any adapter fails to create a device. In production, this should
/// gracefully skip failed adapters and log the error.
///
/// # Example
///
/// ```no_run
/// use shoggoth_core::bootstrap_hardware_fabric;
///
/// #[tokio::main]
/// async fn main() {
///     let topology = bootstrap_hardware_fabric().await;
///     println!("Discovered {} nodes, {:.2} GB total VRAM",
///              topology.nodes.len(), topology.total_vram_gb());
/// }
/// ```
pub async fn bootstrap_hardware_fabric() -> FabricTopology {
    let instance = Instance::new(InstanceDescriptor {
        backends: Backends::VULKAN | Backends::DX12,
        ..Default::default()
    });

    let adapters: Vec<Adapter> = instance
        .enumerate_adapters(Backends::all())
        .collect();

    let mut nodes = Vec::with_capacity(adapters.len());

    for (i, adapter) in adapters.into_iter().enumerate() {
        let info = adapter.get_info();

        let vendor = match info.vendor {
            0x10DE => HardwareVendor::Nvidia,
            0x1002 => HardwareVendor::Amd, // BC250 APUs register here
            0x8086 => HardwareVendor::Intel,
            _ => HardwareVendor::Unknown,
        };

        // Request a logical device with performance memory hints.
        // SAFETY: adapter.request_device is a safe async call; panics on
        // incompatible features which is acceptable during bootstrap.
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some(&format!("shoggoth_node_{i}")),
                    required_features: wgpu::Features::default(),
                    required_limits: if cfg!(debug_assertions) {
                        wgpu::Limits::default()
                    } else {
                        wgpu::Limits {
                            max_storage_buffer_binding_size: 1 << 30, // 1 GB
                            max_buffer_size: 1 << 32,                 // 4 GB
                            ..wgpu::Limits::default()
                        }
                    },
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "Failed to create wgpu device for adapter {i} ({:?}): {e}",
                    info.name
                )
            });

        tracing::info!(
            vendor = %vendor,
            name = %info.name,
            vram_gb = (info.device_id as u64 * 0), // wgpu doesn't expose VRAM directly; use driver heuristics
            node_index = i,
            "Bound Shoggoth node"
        );

        nodes.push(ShoggothNode {
            name: info.name.clone(),
            hardware_type: vendor,
            // wgpu does not expose VRAM bytes directly through AdapterInfo.
            // Downstream consumers should query via Vulkan/DX12 FFI or driver-specific APIs.
            vram_bytes: estimate_vram_bytes(&info, vendor),
            device: Arc::new(device),
            queue: Arc::new(queue),
            adapter_info: info,
        });
    }

    FabricTopology { instance, nodes }
}

// ── VRAM Estimation Heuristic ──────────────────────────────────────────────────

/// Estimates VRAM from adapter info using vendor-specific heuristics.
///
/// wgpu does not expose total VRAM through the safe API. This function uses
/// the PCI device ID to look up known VRAM configurations. For unknown devices,
/// it returns a safe default of 4 GB.
#[must_use]
fn estimate_vram_bytes(info: &wgpu::AdapterInfo, _vendor: HardwareVendor) -> u64 {
    // Heuristic based on known device IDs. In production, this should be
    // replaced with Vulkan VK_EXT_memory_budget or CUDA cudaMemGetInfo FFI calls.
    match info.device {
        // NVIDIA RTX 5090 (tentative device ID — update when confirmed)
        0x2B85 | 0x2B86 => 32 * 1024 * 1024 * 1024,
        // NVIDIA RTX 4090
        0x2684 => 24 * 1024 * 1024 * 1024,
        // NVIDIA RTX 3090
        0x2204 | 0x2208 => 24 * 1024 * 1024 * 1024,
        // AMD MI50 Instinct
        0x66A0 | 0x66A1 => 32 * 1024 * 1024 * 1024,
        // AMD BC250 (RDNA2 APU — 12GB unified GDDR6 mod)
        0x73FF => 12 * 1024 * 1024 * 1024,
        // AMD V620
        0x740F => 32 * 1024 * 1024 * 1024,
        // Unknown: assume reasonable minimum
        _ => 4 * 1024 * 1024 * 1024,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vram_estimation_known_devices() {
        let info_4090 = wgpu::AdapterInfo {
            name: "NVIDIA GeForce RTX 4090".into(),
            vendor: 0x10DE,
            device: 0x2684,
            device_type: wgpu::DeviceType::DiscreteGpu,
            driver: "".into(),
            driver_info: "".into(),
            backend: wgpu::Backend::Vulkan,
        };
        assert_eq!(
            estimate_vram_bytes(&info_4090, HardwareVendor::Nvidia),
            24 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn test_vram_estimation_unknown_device() {
        let info_unknown = wgpu::AdapterInfo {
            name: "Mystery GPU".into(),
            vendor: 0x9999,
            device: 0x9999,
            device_type: wgpu::DeviceType::DiscreteGpu,
            driver: "".into(),
            driver_info: "".into(),
            backend: wgpu::Backend::Vulkan,
        };
        assert_eq!(
            estimate_vram_bytes(&info_unknown, HardwareVendor::Unknown),
            4 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn test_hardware_vendor_display() {
        assert_eq!(HardwareVendor::Nvidia.to_string(), "NVIDIA");
        assert_eq!(HardwareVendor::Amd.to_string(), "AMD");
        assert_eq!(HardwareVendor::Intel.to_string(), "Intel");
    }
}
