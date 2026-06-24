// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/metal_interop.rs — Apple Metal interop layer specification.
//
// Bridges Apple Silicon (M-series) GPUs into the Shoggoth fabric via Metal 3.
// Enables MacBook Pro, Mac Studio, and future Apple Silicon devices to serve
// as edge viewport clients and lightweight compute contributors.
//
// Unlike NVIDIA/AMD GPUs, Apple Silicon uses Unified Memory Architecture (UMA):
//   • CPU and GPU share the same physical memory pool.
//   • No PCIe bus — all transfers are zero-copy within the SoC.
//   • Metal 3 features: Mesh Shaders, Ray Tracing, GPU-driven pipelines.
//
// Shoggoth treats Apple Silicon nodes as "Unified Memory Limbs":
//   • Can composit and display the WebRTC viewport with sub-4ms decode.
//   • Can contribute lightweight compute (inference, post-processing).
//   • Cannot serve as primary ray-tracing or heavy compute node.
//
// Architecture:
//
//   macOS Host (M3 Ultra / M4 Max, Metal 3)
//         │
//         ├── MTLDevice → MTLBuffer → sysctl shared memory
//         │                                    │
//         │                          POSIX shared memory (shm_open)
//         │                                    │
//         │                          Shoggoth orchestrator (local socket)
//         │
//         ├── MTLCommandQueue → compute dispatch
//         │         │
//         │         └── SPIR-V → MSL (via spirv-cross or manual translation)
//         │
//         ├── VideoToolbox → H.265/AV1 hardware decode
//         │         │
//         │         └── WebRTC viewport display (< 4ms decode on M3+)
//         │
//         └── Network.framework → QUIC connection to orchestrator

use serde::{Deserialize, Serialize};

// ── Types ──────────────────────────────────────────────────────────────────────

/// Metal device classification for fabric routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetalGpuFamily {
    /// M1: Apple Family 7 (A14-derived).
    Apple7,
    /// M2: Apple Family 8.
    Apple8,
    /// M3: Apple Family 9 (hardware ray tracing, mesh shaders, AV1 decode).
    Apple9,
    /// M4: Apple Family 10.
    Apple10,
}

impl MetalGpuFamily {
    /// Returns the unified memory bandwidth in GB/s.
    pub fn memory_bandwidth_gbps(&self) -> f64 {
        match self {
            Self::Apple7 => 68.0,   // M1 Max
            Self::Apple8 => 100.0,  // M2 Max
            Self::Apple9 => 150.0,  // M3 Max
            Self::Apple10 => 200.0, // M4 Max
        }
    }

    /// Whether hardware ray tracing is available.
    pub fn has_hardware_rt(&self) -> bool {
        matches!(self, Self::Apple9 | Self::Apple10)
    }

    /// Whether mesh shaders are available.
    pub fn has_mesh_shaders(&self) -> bool {
        matches!(self, Self::Apple9 | Self::Apple10)
    }

    /// Whether AV1 hardware decode is available.
    pub fn has_av1_decode(&self) -> bool {
        matches!(self, Self::Apple9 | Self::Apple10)
    }
}

/// Metal interop configuration.
#[derive(Debug, Clone)]
pub struct MetalInteropConfig {
    /// GPU family for capability detection.
    pub gpu_family: MetalGpuFamily,
    /// Total unified memory in GB.
    pub unified_memory_gb: u32,
    /// Whether to use Metal Performance Shaders (MPS) for ML inference.
    pub use_mps: bool,
    /// Target Metal Shading Language version.
    pub msl_version: String,
    /// Shared memory region name for fabric communication.
    pub shm_region_name: String,
}

impl Default for MetalInteropConfig {
    fn default() -> Self {
        Self {
            gpu_family: MetalGpuFamily::Apple9,
            unified_memory_gb: 36, // M3 Max base
            use_mps: true,
            msl_version: "3.1".into(),
            shm_region_name: "/shoggoth_metal_shm".into(),
        }
    }
}

// ── Metal Compute Dispatch ────────────────────────────────────────────────────
//
// Shoggoth sends compute workloads to Apple Silicon nodes via:
//
//   1. Orchestrator compiles SPIR-V → MSL (Metal Shading Language).
//      Uses spirv-cross CLI or Naga's MSL backend.
//   2. MSL source is sent to the Metal node agent via QUIC.
//   3. Node agent:
//      a. Creates a MTLComputePipelineState from the MSL source.
//      b. Creates MTLBuffers from the shared memory region.
//      c. Dispatches the compute kernel via MTLCommandQueue.
//      d. Signals completion via a MTLSharedEvent (timeline semaphore).
//   4. Results are read back from shared memory.

/// Metal compute dispatch descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetalDispatchDescriptor {
    /// MSL source code (compiled from SPIR-V).
    pub msl_source: String,
    /// Entry point function name (e.g., "gemm_main").
    pub entry_point: String,
    /// Threadgroup size (width × height × depth).
    pub threadgroup_size: (u32, u32, u32),
    /// Grid size in threadgroups.
    pub grid_size: (u32, u32, u32),
    /// Input buffer sizes in bytes.
    pub input_sizes: Vec<u64>,
    /// Output buffer size in bytes.
    pub output_size: u64,
}

// ── VideoToolbox Decode Pipeline ──────────────────────────────────────────────
//
// Apple Silicon decodes the composited WebRTC stream with:
//
//   1. VTDecompressionSession created for H.265 or AV1.
//   2. Encoded bitstream fed from the WebRTC data channel.
//   3. Decoded CVPixelBuffer mapped directly to Metal texture.
//   4. Metal compute shader applies final color grading.
//   5. CAMetalLayer presents to the display at native refresh rate.
//
// Total pipeline latency on M3 Max with AV1 4K60: ~3ms decode + ~1ms present.

// ── POSIX Shared Memory Bridge ────────────────────────────────────────────────
//
// On Apple Silicon (UMA), there's no DMA-BUF. Instead, Shoggoth uses:
//
//   1. shm_open("/shoggoth_metal_shm", O_CREAT | O_RDWR, 0600).
//   2. ftruncate(fd, buffer_size).
//   3. mmap(NULL, buffer_size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0).
//   4. The same shm region is mapped by the Rust orchestrator (via nix crate).
//   5. MTLBuffer is created with .storageMode = MTLStorageModeShared,
//      backed by the same physical pages (via IOKit IOSurface).
//
// This achieves true zero-copy between Metal GPU and CPU on UMA.

/// Configuration for the POSIX shared memory bridge.
#[derive(Debug, Clone)]
pub struct PosixShmBridge {
    /// Shared memory region name.
    pub name: String,
    /// Region size in bytes.
    pub size_bytes: u64,
}

impl PosixShmBridge {
    /// Opens or creates a shared memory region.
    #[cfg(target_os = "macos")]
    pub fn open(&self) -> std::io::Result<(std::os::fd::OwnedFd, *mut u8)> {
        use std::os::fd::FromRawFd;

        let name = std::ffi::CString::new(self.name.as_str())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

        // SAFETY: shm_open with correct flags and mode.
        let fd = unsafe {
            libc::shm_open(
                name.as_ptr(),
                libc::O_CREAT | libc::O_RDWR,
                0o600,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }

        let owned_fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) };

        // Set size.
        let ret = unsafe { libc::ftruncate(owned_fd.as_raw_fd(), self.size_bytes as i64) };
        if ret != 0 {
            return Err(std::io::Error::last_os_error());
        }

        // Map into process address space.
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                self.size_bytes as usize,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                owned_fd.as_raw_fd(),
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error());
        }

        Ok((owned_fd, ptr as *mut u8))
    }

    /// Closes and unmaps the shared memory region.
    #[cfg(target_os = "macos")]
    pub fn close(&self, fd: std::os::fd::OwnedFd, ptr: *mut u8) {
        unsafe {
            libc::munmap(ptr as *mut _, self.size_bytes as usize);
            // fd is closed by OwnedFd drop.
            let _ = fd;
        }
    }
}

// ── Shoggoth on Apple Silicon Certification ───────────────────────────────────
//
// Apple Silicon nodes are classified as "Unified Memory Limbs":
//   • No DMA-BUF export (UMA — no PCIe peer-to-peer).
//   • No hardware encoder (NVENC/AMF) — uses VideoToolbox software assist.
//   • Excellent viewport client: sub-4ms AV1 decode + native ProMotion display.
//   • Lightweight compute: MPS Graph for CoreML model inference.
//   • Not suitable as primary fabric node for heavy workloads.

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metal_gpu_family_bandwidth() {
        assert!((MetalGpuFamily::Apple9.memory_bandwidth_gbps() - 150.0).abs() < f64::EPSILON);
        assert!(MetalGpuFamily::Apple7.memory_bandwidth_gbps() < MetalGpuFamily::Apple10.memory_bandwidth_gbps());
    }

    #[test]
    fn test_m3_has_rt_and_mesh_shaders() {
        let m3 = MetalGpuFamily::Apple9;
        assert!(m3.has_hardware_rt());
        assert!(m3.has_mesh_shaders());
        assert!(m3.has_av1_decode());
    }

    #[test]
    fn test_m1_lacks_rt() {
        assert!(!MetalGpuFamily::Apple7.has_hardware_rt());
    }

    #[test]
    fn test_metal_dispatch_serialization() {
        let desc = MetalDispatchDescriptor {
            msl_source: "kernel void main() {}".into(),
            entry_point: "main".into(),
            threadgroup_size: (16, 16, 1),
            grid_size: (64, 32, 1),
            input_sizes: vec![1024, 2048],
            output_size: 4096,
        };
        let json = serde_json::to_string(&desc).unwrap();
        assert!(json.contains("main"));
        assert!(json.contains("1024"));
    }
}
