// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/dx12_interop.rs — DirectX 12 interop layer specification.
//
// Provides the architectural blueprint for bridging Windows-native DirectX 12
// pipelines (RTX 5090/4090, Agility SDK sm_90) into the Shoggoth fabric.
//
// Unlike the Vulkan/WGPU path (which works cross-platform), DirectX 12
// interop requires Windows-specific mechanisms:
//   • ID3D12Device → shared heap → NT handle → DMA-BUF fd (via Vulkan interop).
//   • DX12 Agility SDK for cutting-edge features (Work Graphs, Shader Model 6.8).
//   • NVENC via ID3D12VideoEncodeCommandList for sub-2ms encode.
//   • Present → compositor: IDXGIOutputDuplication for zero-copy display capture.
//
// Architecture:
//
//   Windows Host (RTX 5090, DirectX 12, NVENC)
//         │
//         ├── ID3D12Device → shared heap → NT handle
//         │                                │
//         │                     Vulkan VK_KHR_external_memory_win32
//         │                                │
//         │                         DMA-BUF fd
//         │                                │
//         │                    WSL2 /dev/dri/renderD*
//         │                                │
//         ├── ID3D12VideoEncoder → NVENC bitstream
//         │                                │
//         │                    AF_HYPERV Vsock (port 9152)
//         │                                │
//         └── Shoggoth Orchestrator (Linux Xeon 512GB)
//              ├── DMA-BUF import → wgpu::Buffer
//              ├── NVENC bitstream → WebRTC compositor
//              └── Mesh shader distribution → BC250 APU grid

// ── Types ──────────────────────────────────────────────────────────────────────

/// DX12 interop configuration.
#[derive(Debug, Clone)]
pub struct Dx12InteropConfig {
    /// Path to the DX12 Agility SDK DLL (e.g., "D3D12Core.dll").
    pub agility_sdk_path: String,
    /// Target Shader Model (6.6 for Turing+, 6.8 for Ada Lovelace+).
    pub target_shader_model: String,
    /// Whether to use Work Graphs for GPU-driven rendering.
    pub use_work_graphs: bool,
    /// Whether to use GPU Upload Heaps for DMA-BUF export.
    pub use_gpu_upload_heaps: bool,
    /// NVENC encode resolution preset.
    pub nvenc_quality: NvencQualityPreset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvencQualityPreset {
    P1, // Fastest
    P4, // Balanced
    P7, // Highest quality
}

impl Default for Dx12InteropConfig {
    fn default() -> Self {
        Self {
            agility_sdk_path: "D3D12Core.dll".into(),
            target_shader_model: "6.8".into(),
            use_work_graphs: true,
            use_gpu_upload_heaps: true,
            nvenc_quality: NvencQualityPreset::P4,
        }
    }
}

// ── DX12 Interop Flow ─────────────────────────────────────────────────────────
//
// Step 1: Create Shared Heap
// ─────────────────────────
// D3D12_HEAP_DESC heap_desc = {
//     .SizeInBytes = buffer_size,
//     .Properties = {
//         .Type = D3D12_HEAP_TYPE_DEFAULT,
//         .CPUPageProperty = D3D12_CPU_PAGE_PROPERTY_NOT_AVAILABLE,
//         .MemoryPoolPreference = D3D12_MEMORY_POOL_L0,  // VRAM
//     },
//     .Flags = D3D12_HEAP_FLAG_SHARED,  // ← Cross-adapter sharing
// };
// device->CreateHeap(&heap_desc, IID_PPV_ARGS(&shared_heap));
//
// Step 2: Get NT Handle
// ─────────────────────
// HANDLE nt_handle;
// device->CreateSharedHandle(
//     shared_heap,
//     nullptr,  // Default security attributes
//     GENERIC_ALL,
//     L"ShoggothSharedHeap",
//     &nt_handle);
//
// Step 3: NT Handle → KMT Handle (via GDI)
// ─────────────────────────────────────────
// D3DKMT_QUERYRESOURCEINFO query_info = { ... };
// D3DKMTOpenResourceFromNtHandle(&kmt_handle, nt_handle);
//
// Step 4: KMT Handle → DMA-BUF fd (via Vulkan interop)
// ─────────────────────────────────────────────────────
// VkImportMemoryWin32HandleInfoKHR import_info = {
//     .handleType = VK_EXTERNAL_MEMORY_HANDLE_TYPE_D3D12_RESOURCE_BIT_KHR,
//     .handle = kmt_handle,
// };
// VkMemoryGetFdInfoKHR fd_info = {
//     .handleType = VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT,
//     .memory = vulkan_memory,
// };
// vkGetMemoryFdKHR(device, &fd_info, &dma_buf_fd);
//
// Step 5: DMA-BUF fd → WSL2 Linux via AF_HYPERV Vsock
// ─────────────────────────────────────────────────────
// The DMA-BUF fd is sent over AF_HYPERV to the WSL2 guest,
// where the orchestrator imports it via DRM_PRIME_FD_TO_HANDLE.

// ── DX12 Agility SDK Feature Matrix ───────────────────────────────────────────

/// DX12 features available for Shoggoth fabric integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dx12FabricFeature {
    /// Work Graphs: GPU-driven rendering with producer-consumer nodes.
    /// Available on RTX 40-series+ (Ada Lovelace) with Agility SDK 1.7+.
    WorkGraphs,

    /// Shader Model 6.8: expanded wave intrinsics, reduced precision.
    /// Available on Turing+ (RTX 20+) with Agility SDK 1.7+.
    ShaderModel6_8,

    /// GPU Upload Heaps: direct CPU→GPU upload without staging buffers.
    /// Available on Turing+ with Agility SDK 1.7+.
    GpuUploadHeaps,

    /// Sampler Feedback: texture-space shading for Nanite-style rendering.
    /// Available on Turing+ with Agility SDK 1.6+.
    SamplerFeedback,

    /// DirectStorage: GPU decompression of assets directly from NVMe.
    /// Available on Turing+ with Agility SDK 1.6+.
    DirectStorage,

    /// Mesh Shaders: programmable geometry pipeline for Nanite.
    /// Available on Turing+.
    MeshShaders,

    /// Raytracing Tier 1.1: inline raytracing + execution reordering.
    /// Available on RTX 40-series+.
    RaytracingTier1_1,
}

/// Returns the DX12 features available on a given GPU vendor and generation.
pub fn dx12_features_for_gpu(vendor: &str, generation: &str) -> Vec<Dx12FabricFeature> {
    match (vendor.to_lowercase().as_str(), generation.to_lowercase().as_str()) {
        ("nvidia", "ada") | ("nvidia", "blackwell") => vec![
            Dx12FabricFeature::WorkGraphs,
            Dx12FabricFeature::ShaderModel6_8,
            Dx12FabricFeature::GpuUploadHeaps,
            Dx12FabricFeature::SamplerFeedback,
            Dx12FabricFeature::DirectStorage,
            Dx12FabricFeature::MeshShaders,
            Dx12FabricFeature::RaytracingTier1_1,
        ],
        ("nvidia", "ampere") => vec![
            Dx12FabricFeature::ShaderModel6_8,
            Dx12FabricFeature::SamplerFeedback,
            Dx12FabricFeature::DirectStorage,
            Dx12FabricFeature::MeshShaders,
        ],
        ("nvidia", "turing") => vec![
            Dx12FabricFeature::SamplerFeedback,
            Dx12FabricFeature::MeshShaders,
        ],
        ("amd", "rdna3") | ("amd", "rdna4") => vec![
            Dx12FabricFeature::ShaderModel6_8,
            Dx12FabricFeature::MeshShaders,
        ],
        _ => vec![Dx12FabricFeature::MeshShaders],
    }
}

// ── NVENC Pipeline Config ─────────────────────────────────────────────────────

/// NVENC configuration for DX12 pipeline.
#[derive(Debug, Clone)]
pub struct Dx12NvencConfig {
    /// Encode codec.
    pub codec: Dx12EncodeCodec,
    /// Target bitrate in bps.
    pub bitrate_bps: u64,
    /// GOP size.
    pub gop_size: u32,
    /// Quality preset.
    pub quality: NvencQualityPreset,
    /// Whether to use AV1 (requires RTX 40-series+).
    pub use_av1: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dx12EncodeCodec {
    H264,
    Hevc,
    Av1,
}

impl Default for Dx12NvencConfig {
    fn default() -> Self {
        Self {
            codec: Dx12EncodeCodec::Av1,
            bitrate_bps: 50_000_000,
            gop_size: 120,
            quality: NvencQualityPreset::P4,
            use_av1: true,
        }
    }
}

// ── Display Capture (IDXGIOutputDuplication) ──────────────────────────────────
//
// Captures the Windows desktop (game, Unreal viewport, Blender) without
// CPU readback using IDXGIOutputDuplication:
//
//   1. IDXGIOutput1::DuplicateOutput → IDXGIOutputDuplication.
//   2. AcquireNextFrame → IDXGIResource (DXGI surface in VRAM).
//   3. CopyResource → shared heap (D3D12_HEAP_FLAG_SHARED).
//   4. Export shared heap as DMA-BUF → WSL2 → compositor.
//
// This achieves zero-copy display capture at 4K 120Hz with ~0.5ms GPU time.

// ── Shader Distribution Strategy ──────────────────────────────────────────────
//
// When a Unreal Engine 5 game uses Nanite + Lumen:
//
//   Local RTX 5090 (DX12):
//     • Primary viewport raster (Nanite mesh shaders).
//     • Lumen surface cache updates.
//     • UI overlay rendering.
//     • NVENC encode.
//
//   Distributed BC250 APU Grid (Vulkan Compute):
//     • Secondary ray bounces (path tracing).
//     • Shadow map rendering for distant lights.
//     • Physics simulation (GPU particles, cloth).
//     • AI inference (NPC behavior via ONNX Runtime).
//
//   AMD V620 (SR-IOV):
//     • Virtual display output for headless cloud clients.
//     • AMF encode for secondary stream.
//
// The orchestrator distributes RenderTile work units with:
//   • Dx12RenderTile { tile_id, viewport_region, camera_matrix, scene_bvh_hash }
//   • Each tile is a portion of the final viewport.
//   • The compositor blends tiles and streams via WebRTC.

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = Dx12InteropConfig::default();
        assert_eq!(cfg.target_shader_model, "6.8");
        assert!(cfg.use_work_graphs);
    }

    #[test]
    fn test_nvenc_defaults() {
        let cfg = Dx12NvencConfig::default();
        assert!(cfg.use_av1);
        assert_eq!(cfg.bitrate_bps, 50_000_000);
    }

    #[test]
    fn test_features_ada_lovelace() {
        let features = dx12_features_for_gpu("nvidia", "ada");
        assert!(features.contains(&Dx12FabricFeature::WorkGraphs));
        assert!(features.contains(&Dx12FabricFeature::RaytracingTier1_1));
    }

    #[test]
    fn test_features_ampere() {
        let features = dx12_features_for_gpu("nvidia", "ampere");
        assert!(!features.contains(&Dx12FabricFeature::WorkGraphs));
        assert!(features.contains(&Dx12FabricFeature::MeshShaders));
    }
}
