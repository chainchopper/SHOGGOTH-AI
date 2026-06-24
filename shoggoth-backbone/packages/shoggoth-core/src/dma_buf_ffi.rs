// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-core/src/dma_buf_ffi.rs — Linux DMA-BUF raw FFI bindings.
//
// Provides direct ioctl wrappers for the Linux kernel DMA-BUF subsystem.
// Used by memory_fabric.rs to export/import GPU buffer file descriptors
// across vendor boundaries (AMD ↔ NVIDIA ↔ Intel) without going through
// system RAM.
//
// # Safety
//
// This module uses raw FFI and ioctl calls. All public functions are
// marked `unsafe` because callers must guarantee:
//   • Valid wgpu Device backed by a Vulkan implementation.
//   • VK_KHR_external_memory_fd extension is available.
//   • DMA-BUF-capable Linux kernel (CONFIG_DMA_SHARED_BUFFER=y).
//
// # Platform
//
// Linux-only. Conditionally compiled with #[cfg(target_os = "linux")].

#[cfg(target_os = "linux")]
use std::os::fd::{AsRawFd, OwnedFd, RawFd};

// ── Kernel DMA-BUF ioctl Constants ─────────────────────────────────────────────

/// DMA-BUF ioctl: get information about a DMA-BUF fd.
#[cfg(target_os = "linux")]
const DMA_BUF_IOCTL_SYNC: u64 = 0x40086200;

/// DMA-BUF ioctl: begin CPU access to a buffer.
#[cfg(target_os = "linux")]
const DMA_BUF_IOCTL_SYNC_START: u64 = 0x40086201;

/// DMA-BUF ioctl: end CPU access to a buffer.
#[cfg(target_os = "linux")]
const DMA_BUF_IOCTL_SYNC_END: u64 = 0x40086202;

/// DMA-BUF ioctl: export a DMA-BUF fd from a DRM device.
#[cfg(target_os = "linux")]
const DRM_IOCTL_PRIME_HANDLE_TO_FD: u64 = 0xC020_6422;

/// DMA-BUF ioctl: import a DMA-BUF fd into a DRM device.
#[cfg(target_os = "linux")]
const DRM_IOCTL_PRIME_FD_TO_HANDLE: u64 = 0xC020_6423;

// ── DRM DMA-BUF Export ─────────────────────────────────────────────────────────

/// Exports a GPU buffer from a DRM device as a DMA-BUF file descriptor.
///
/// # Arguments
///
/// * `drm_fd` — File descriptor of the DRM device (e.g., `/dev/dri/renderD128`).
/// * `gem_handle` — GEM buffer handle to export.
/// * `flags` — Export flags (typically 0, or `O_CLOEXEC`).
///
/// # Returns
///
/// A new file descriptor representing the DMA-BUF export.
///
/// # Safety
///
/// * `drm_fd` must be a valid, open DRM device file descriptor.
/// * `gem_handle` must be a valid GEM buffer handle on that device.
/// * The caller must close the returned fd when done.
#[cfg(target_os = "linux")]
pub unsafe fn drm_prime_handle_to_fd(
    drm_fd: RawFd,
    gem_handle: u32,
    flags: i32,
) -> std::io::Result<OwnedFd> {
    #[repr(C)]
    struct DrmPrimeHandle {
        handle: u32,
        flags: i32,
        fd: i32,
    }

    let mut args = DrmPrimeHandle {
        handle: gem_handle,
        flags,
        fd: -1,
    };

    let ret = unsafe {
        libc::ioctl(drm_fd, DRM_IOCTL_PRIME_HANDLE_TO_FD as _, &mut args as *mut _)
    };

    if ret != 0 {
        return Err(std::io::Error::last_os_error());
    }

    debug_assert!(args.fd >= 0, "DRM_PRIME_HANDLE_TO_FD returned negative fd");
    Ok(unsafe { OwnedFd::from_raw_fd(args.fd) })
}

/// Imports a DMA-BUF file descriptor into a DRM device as a GEM handle.
///
/// # Arguments
///
/// * `drm_fd` — File descriptor of the DRM device.
/// * `dma_buf_fd` — DMA-BUF file descriptor to import (ownership transfers).
///
/// # Returns
///
/// A GEM buffer handle valid on the target DRM device.
///
/// # Safety
///
/// * `drm_fd` must be a valid, open DRM device file descriptor.
/// * `dma_buf_fd` must be a valid DMA-BUF fd (ownership transfers to kernel).
/// * The returned GEM handle must be freed via `drm_gem_close`.
#[cfg(target_os = "linux")]
pub unsafe fn drm_prime_fd_to_handle(
    drm_fd: RawFd,
    dma_buf_fd: RawFd,
) -> std::io::Result<u32> {
    #[repr(C)]
    struct DrmPrimeHandle {
        handle: u32,
        flags: i32,
        fd: i32,
    }

    let mut args = DrmPrimeHandle {
        handle: 0,
        flags: 0,
        fd: dma_buf_fd,
    };

    let ret = unsafe {
        libc::ioctl(drm_fd, DRM_IOCTL_PRIME_FD_TO_HANDLE as _, &mut args as *mut _)
    };

    if ret != 0 {
        return Err(std::io::Error::last_os_error());
    }

    Ok(args.handle)
}

// ── DMA-BUF CPU Access Synchronization ─────────────────────────────────────────

/// Synchronization flags for DMA-BUF access.
#[derive(Debug, Clone, Copy)]
pub enum DmaBufSyncFlags {
    /// CPU will read from the buffer.
    Read = 1,
    /// CPU will write to the buffer.
    Write = 2,
    /// CPU read + write.
    ReadWrite = 3,
}

/// Begins CPU access to a DMA-BUF, ensuring GPU writes are visible.
///
/// # Safety
///
/// * `dma_buf_fd` must be a valid DMA-BUF fd.
/// * Must pair with `dma_buf_end_cpu_access` when done.
/// * No GPU access to the buffer while CPU access is active.
#[cfg(target_os = "linux")]
pub unsafe fn dma_buf_begin_cpu_access(
    dma_buf_fd: RawFd,
    flags: DmaBufSyncFlags,
) -> std::io::Result<()> {
    #[repr(C)]
    struct DmaBufSync {
        flags: u64,
    }

    let args = DmaBufSync {
        flags: flags as u64,
    };

    let ret = unsafe {
        libc::ioctl(dma_buf_fd, DMA_BUF_IOCTL_SYNC_START as _, &args as *const _)
    };

    if ret != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Ends CPU access to a DMA-BUF, allowing GPU access again.
///
/// # Safety
///
/// * Must follow a prior `dma_buf_begin_cpu_access` call.
#[cfg(target_os = "linux")]
pub unsafe fn dma_buf_end_cpu_access(
    dma_buf_fd: RawFd,
    flags: DmaBufSyncFlags,
) -> std::io::Result<()> {
    #[repr(C)]
    struct DmaBufSync {
        flags: u64,
    }

    let args = DmaBufSync {
        flags: flags as u64,
    };

    let ret = unsafe {
        libc::ioctl(dma_buf_fd, DMA_BUF_IOCTL_SYNC_END as _, &args as *const _)
    };

    if ret != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

// ── DRM Device Discovery ───────────────────────────────────────────────────────

/// Opens a DRM render node by index (e.g., `/dev/dri/renderD128`).
///
/// Returns the file descriptor and the device path.
#[cfg(target_os = "linux")]
pub fn open_drm_render_node(index: u32) -> std::io::Result<(OwnedFd, String)> {
    let path = format!("/dev/dri/renderD{}", 128 + index);
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)?;

    let fd = OwnedFd::from(file);
    Ok((fd, path))
}

/// Lists all available DRM render nodes on the system.
#[cfg(target_os = "linux")]
pub fn list_drm_render_nodes() -> std::io::Result<Vec<String>> {
    let mut nodes = Vec::new();
    for entry in std::fs::read_dir("/dev/dri")? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("renderD") {
            nodes.push(format!("/dev/dri/{name}"));
        }
    }
    nodes.sort();
    Ok(nodes)
}

// ── Stub Implementations for Non-Linux ─────────────────────────────────────────

#[cfg(not(target_os = "linux"))]
pub fn list_drm_render_nodes() -> std::io::Result<Vec<String>> {
    Ok(vec![])
}

// ── Wgpu ↔ Vulkan External Memory Integration ─────────────────────────────────
//
// In production, the memory_fabric.rs export/import flow bridges wgpu buffers
// to Vulkan external memory like this:
//
//   1. Create a wgpu::Buffer with usage STORAGE | COPY_SRC | COPY_DST.
//   2. Use unsafe wgpu::Buffer::as_hal() to get the Vulkan VkBuffer handle.
//      (Requires wgpu feature "hal" and the vulkan backend.)
//   3. Call vkGetMemoryFdKHR() to export the buffer's memory as a DMA-BUF fd.
//   4. Pass the fd to another process or GPU vendor driver.
//   5. On the receiving end: vkImportMemoryFdKHR() → VkDeviceMemory → wgpu::Buffer.
//
// This requires linking against ash (Vulkan bindings) or erupt.
// For now, the drm_prime_* functions above provide the kernel-level primitives
// when combined with Mesa/KMS for GPU-agnostic DMA-BUF sharing.

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn test_list_drm_nodes() {
        let nodes = list_drm_render_nodes();
        // Even in CI without GPUs, this should not panic.
        assert!(nodes.is_ok());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_open_drm_node_errors_gracefully() {
        // Opening a non-existent render node should return an error, not panic.
        let result = open_drm_render_node(999);
        assert!(result.is_err());
    }
}
