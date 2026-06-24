// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/vulkan_layer.rs — Vulkan layer interceptor specification.
//
// The Vulkan Layer Interceptor allows legacy applications (Unreal Engine,
// Unity, Blender) to run on the Shoggoth fabric without code changes.
//
// How it works:
//   1. A Vulkan implicit layer (`libVkLayer_shoggoth.so`) is installed
//      system-wide via /etc/vulkan/implicit_layer.d/shoggoth.json.
//   2. When any Vulkan application starts, the layer intercepts:
//      - vkQueueSubmit: captures command buffers for redistribution.
//      - vkAllocateMemory: tracks allocations for DMA-BUF export.
//      - vkCreateImage / vkCreateBuffer: tags resources as shoggoth-managed.
//   3. The layer communicates with the Shoggoth orchestrator via
//      Unix domain socket to coordinate multi-GPU task distribution.
//
// ## Layer Manifest (install to /etc/vulkan/implicit_layer.d/)
//
// ```json
// {
//   "file_format_version": "1.2.0",
//   "layer": {
//     "name": "VK_LAYER_SHOGGOTH_mesh_machine",
//     "type": "GLOBAL",
//     "library_path": "/opt/shoggoth/lib/libVkLayer_shoggoth.so",
//     "api_version": "1.3.280",
//     "implementation_version": "1",
//     "description": "Shoggoth Mesh Machine — Distributed GPU Fabric Layer",
//     "enable_environment": {
//       "SHOGGOTH_ENABLE": "1"
//     },
//     "disable_environment": {
//       "SHOGGOTH_DISABLE": "1"
//     }
//   }
// }
// ```
//
// ## Intercepted Vulkan Functions
//
// The layer hooks these Vulkan entry points:

/// Functions intercepted by the Shoggoth Vulkan layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkInterceptedFunction {
    // ── Device Creation ──
    /// Enumerate physical devices → inject virtual Shoggoth devices.
    VkEnumeratePhysicalDevices,
    /// Create logical device → add Shoggoth-specific extensions.
    VkCreateDevice,

    // ── Memory Management ──
    /// Allocate GPU memory → tag for DMA-BUF export.
    VkAllocateMemory,
    /// Free GPU memory → notify orchestrator of deallocation.
    VkFreeMemory,
    /// Map memory → redirect to shared fabric allocation.
    VkMapMemory,

    // ── Resource Creation ──
    /// Create image → tag dimensions for tile sharding.
    VkCreateImage,
    /// Create buffer → tag for compute fabric distribution.
    VkCreateBuffer,

    // ── Command Submission ──
    /// Submit command buffers → potentially split across GPUs.
    VkQueueSubmit,
    /// Present → synchronize multi-GPU output before display.
    VkQueuePresentKHR,

    // ── Synchronization ──
    /// Create semaphore → inject cross-GPU timeline semaphores.
    VkCreateSemaphore,
    /// Create fence → inject fabric-wide fence for frame sync.
    VkCreateFence,

    // ── Query ──
    /// Get memory requirements → report fabric-pooled VRAM.
    VkGetBufferMemoryRequirements,
    /// Get image memory requirements → report fabric-pooled VRAM.
    VkGetImageMemoryRequirements,
}

// ── Layer Configuration ────────────────────────────────────────────────────────

/// Shoggoth Vulkan layer configuration (read from SHOGGOTH_LAYER_CONFIG env).
#[derive(Debug, Clone)]
pub struct LayerConfig {
    /// Path to the orchestrator Unix socket.
    pub orchestrator_socket: String,
    /// Whether to intercept and distribute compute workloads.
    pub distribute_compute: bool,
    /// Whether to intercept and distribute graphics workloads.
    pub distribute_graphics: bool,
    /// Maximum number of GPUs to use for distribution (0 = auto).
    pub max_gpu_count: u32,
    /// Whether to inject a virtual display device.
    pub inject_virtual_display: bool,
}

impl Default for LayerConfig {
    fn default() -> Self {
        Self {
            orchestrator_socket: "/var/run/shoggoth/orchestrator.sock".into(),
            distribute_compute: true,
            distribute_graphics: true,
            max_gpu_count: 0, // Auto-detect
            inject_virtual_display: false,
        }
    }
}

// ── Layer Communication Protocol ───────────────────────────────────────────────

/// Messages exchanged between the Vulkan layer and orchestrator over Unix socket.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayerMessage {
    /// Layer registers with the orchestrator.
    Register {
        process_name: String,
        pid: u32,
        vendor: String,
    },
    /// Layer reports a GPU memory allocation for fabric tracking.
    MemoryAllocated {
        allocation_id: u64,
        size_bytes: u64,
        memory_type: u32,
        is_exportable: bool,
    },
    /// Layer requests DMA-BUF export for a specific allocation.
    ExportDmaBuf {
        allocation_id: u64,
    },
    /// Layer requests DMA-BUF import from another GPU.
    ImportDmaBuf {
        allocation_id: u64,
        remote_node_id: String,
        remote_allocation_id: u64,
    },
    /// Layer submits a command buffer for potential distribution.
    CommandBufferSubmit {
        queue_family: u32,
        command_count: u32,
        estimated_gpu_time_us: u64,
    },
    /// Orchestrator response: how to distribute the command buffer.
    SubmitDecision {
        /// If true, split across multiple GPUs.
        split: bool,
        /// Which GPUs to use.
        target_nodes: Vec<String>,
        /// Tile dimensions for split rendering.
        tile_width: u32,
        tile_height: u32,
    },
}

// ── Layer Manifest Generator ───────────────────────────────────────────────────

/// Generates the Vulkan implicit layer manifest JSON for installation.
pub fn generate_layer_manifest(library_path: &str) -> String {
    let manifest = serde_json::json!({
        "file_format_version": "1.2.0",
        "layer": {
            "name": "VK_LAYER_SHOGGOTH_mesh_machine",
            "type": "GLOBAL",
            "library_path": library_path,
            "api_version": "1.3.280",
            "implementation_version": "1",
            "description": "Shoggoth Mesh Machine — Distributed GPU Fabric Layer",
            "enable_environment": {
                "SHOGGOTH_ENABLE": "1"
            },
            "disable_environment": {
                "SHOGGOTH_DISABLE": "1"
            }
        }
    });

    serde_json::to_string_pretty(&manifest).unwrap_or_default()
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_manifest_generation() {
        let manifest = generate_layer_manifest("/opt/shoggoth/lib/libVkLayer_shoggoth.so");
        assert!(manifest.contains("VK_LAYER_SHOGGOTH_mesh_machine"));
        assert!(manifest.contains("SHOGGOTH_ENABLE"));
    }

    #[test]
    fn test_layer_message_serialization() {
        let msg = LayerMessage::Register {
            process_name: "UnrealEditor".into(),
            pid: 12345,
            vendor: "NVIDIA".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("UnrealEditor"));
        assert!(json.contains("register"));

        let parsed: LayerMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            LayerMessage::Register { process_name, pid, .. } => {
                assert_eq!(process_name, "UnrealEditor");
                assert_eq!(pid, 12345);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_submit_decision_serialization() {
        let msg = LayerMessage::SubmitDecision {
            split: true,
            target_nodes: vec!["rtx-5090-edge".into(), "rtx-4090-edge".into()],
            tile_width: 1920,
            tile_height: 1080,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("rtx-5090-edge"));
        assert!(json.contains("submit_decision"));
    }

    #[test]
    fn test_default_layer_config() {
        let cfg = LayerConfig::default();
        assert_eq!(cfg.orchestrator_socket, "/var/run/shoggoth/orchestrator.sock");
        assert!(cfg.distribute_compute);
        assert!(cfg.distribute_graphics);
    }
}
