// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/vsock_bridge.rs — WSL2 AF_HYPERV Vsock bridge.
//
// Bridges Windows-native GPU pipelines (RTX 5090/4090 via DirectX 12 / Vulkan)
// to the Linux Xeon orchestrator over the WSL2 hypervisor socket (AF_HYPERV).
//
// Architecture:
//
//   Windows Host (RTX 5090, DX12, NVENC)
//         │
//         │ AF_HYPERV Vsock (VMADDR_CID_HOST)
//         │
//   WSL2 Linux Guest (/dev/vsock)
//         │
//         │ DMA-BUF fd passing via virtio-gpu / dma_buf
//         │
//   Shoggoth Orchestrator (Xeon 512GB, DMA-BUF import)
//
// The vsock bridge enables:
//   • GPU framebuffer export from Windows → Linux without userspace copies.
//   • NVENC-encoded bitstream delivery from DX12 surface → Linux compositor.
//   • Control-plane RPC: orchestrator commands reach the Windows node agent.
//   • Zero-copy DMA-BUF sharing across the hypervisor boundary.

use std::io;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;

// ── Vsock Constants ────────────────────────────────────────────────────────────

/// AF_HYPERV socket family (Microsoft hypervisor socket).
/// Value: 42 on WSL2 Linux kernels.
#[cfg(target_os = "linux")]
const AF_HYPERV: i32 = 43;

/// Default CID for the Windows host (VMADDR_CID_HOST).
pub const VMADDR_CID_HOST: u32 = 2;

/// Default CID for the local WSL2 VM (VMADDR_CID_LOCAL).
pub const VMADDR_CID_LOCAL: u32 = 1;

/// Default vsock port for the Shoggoth node agent on Windows.
pub const SHOGGOTH_VSOCK_PORT: u32 = 9150;

/// Default vsock port for DMA-BUF fd passing (control channel).
pub const SHOGGOTH_DMABUF_PORT: u32 = 9151;

/// Default vsock port for NVENC bitstream delivery.
pub const SHOGGOTH_NVENC_PORT: u32 = 9152;

// ── Vsock Address ──────────────────────────────────────────────────────────────

/// A hypervisor socket address (VMADDR_CID + port).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VsockAddr {
    /// Context ID (2 = host, 1 = local, VM-specific for others).
    pub cid: u32,
    /// Port number.
    pub port: u32,
}

impl VsockAddr {
    /// Creates a new vsock address targeting the Windows host on the given port.
    pub const fn host(port: u32) -> Self {
        Self { cid: VMADDR_CID_HOST, port }
    }

    /// Creates a new vsock address targeting the local WSL2 VM.
    pub const fn local(port: u32) -> Self {
        Self { cid: VMADDR_CID_LOCAL, port }
    }
}

impl std::fmt::Display for VsockAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "vsock:{}:{}", self.cid, self.port)
    }
}

// ── Vsock Stream ───────────────────────────────────────────────────────────────

/// A connection-oriented vsock stream (similar to TCP, over AF_HYPERV).
///
/// On WSL2 Linux, vsock streams appear as regular file descriptors and
/// support standard read/write operations. DMA-BUF file descriptors
/// can be passed over vsock using SCM_RIGHTS ancillary messages.
#[derive(Debug)]
pub struct VsockStream {
    /// The underlying Unix stream connected to the vsock peer.
    stream: UnixStream,
    /// The peer address.
    peer: VsockAddr,
    /// The local address.
    local: VsockAddr,
}

impl VsockStream {
    /// Connects to a vsock peer.
    ///
    /// # Platform
    ///
    /// Linux/WSL2 only. Returns an error on other platforms.
    pub async fn connect(peer: VsockAddr) -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            let fd = unsafe { libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM, 0) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }

            let mut sockaddr = libc::sockaddr_vm {
                svm_family: libc::AF_VSOCK as u16,
                svm_reserved1: 0,
                svm_port: peer.port,
                svm_cid: peer.cid,
                svm_flags: 0, // Default flags
                svm_zero: [0u8; 4],
            };

            let ret = unsafe {
                libc::connect(
                    fd,
                    &sockaddr as *const _ as *const libc::sockaddr,
                    std::mem::size_of::<libc::sockaddr_vm>() as u32,
                )
            };
            if ret < 0 {
                let err = io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(err);
            }

            // Get local address.
            let mut local_sockaddr = unsafe { std::mem::zeroed::<libc::sockaddr_vm>() };
            let mut addrlen = std::mem::size_of::<libc::sockaddr_vm>() as u32;
            let ret = unsafe {
                libc::getsockname(
                    fd,
                    &mut local_sockaddr as *mut _ as *mut libc::sockaddr,
                    &mut addrlen,
                )
            };
            let local_addr = if ret == 0 {
                VsockAddr {
                    cid: local_sockaddr.svm_cid,
                    port: local_sockaddr.svm_port,
                }
            } else {
                VsockAddr::local(0)
            };

            let owned_fd = unsafe { OwnedFd::from_raw_fd(fd) };
            let unix = unsafe { std::os::unix::net::UnixStream::from_raw_fd(owned_fd.as_raw_fd()) };
            // Prevent OwnedFd from closing the fd.
            std::mem::forget(owned_fd);

            tracing::info!(
                peer = %peer,
                local = %local_addr,
                "Vsock stream connected"
            );

            Ok(Self {
                stream: unix,
                peer,
                local: local_addr,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = peer;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Vsock is only supported on Linux/WSL2",
            ))
        }
    }

    /// Sends data over the vsock stream.
    pub fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        use std::io::Write;
        (&self.stream).write_all(buf)
    }

    /// Reads data from the vsock stream.
    pub fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        use std::io::Read;
        (&self.stream).read_exact(buf)
    }

    /// Sends a file descriptor over the vsock stream via SCM_RIGHTS.
    ///
    /// Used to pass DMA-BUF file descriptors from Windows GPU exports
    /// to the Linux orchestrator for zero-copy import.
    pub fn send_fd(&self, fd_to_send: RawFd) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            // Use sendmsg with SCM_RIGHTS ancillary data.
            let iov = libc::iovec {
                iov_base: &0u8 as *const _ as *mut _,
                iov_len: 1,
            };

            let mut cmsg_buf = [0u8; unsafe {
                libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) as usize
            }];

            let mut msghdr = libc::msghdr {
                msg_name: std::ptr::null_mut(),
                msg_namelen: 0,
                msg_iov: &iov as *const _ as *mut _,
                msg_iovlen: 1,
                msg_control: cmsg_buf.as_mut_ptr() as *mut _,
                msg_controllen: cmsg_buf.len(),
                msg_flags: 0,
            };

            let cmsg = unsafe { libc::CMSG_FIRSTHDR(&msghdr) };
            unsafe {
                (*cmsg).cmsg_level = libc::SOL_SOCKET;
                (*cmsg).cmsg_type = libc::SCM_RIGHTS;
                (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as _;
                let data_ptr = libc::CMSG_DATA(cmsg) as *mut RawFd;
                *data_ptr = fd_to_send;
            }
            msghdr.msg_controllen = unsafe { (*cmsg).cmsg_len as _ };

            let ret = unsafe {
                libc::sendmsg(self.stream.as_raw_fd(), &msghdr, 0)
            };

            if ret < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = fd_to_send;
            Err(io::Error::new(io::ErrorKind::Unsupported, "SCM_RIGHTS only on Linux"))
        }
    }

    /// Receives a file descriptor over the vsock stream via SCM_RIGHTS.
    pub fn recv_fd(&self) -> io::Result<OwnedFd> {
        #[cfg(target_os = "linux")]
        {
            let mut buf = [0u8; 1];
            let iov = libc::iovec {
                iov_base: buf.as_mut_ptr() as *mut _,
                iov_len: buf.len(),
            };

            let mut cmsg_buf = [0u8; unsafe {
                libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) as usize
            }];

            let mut msghdr = libc::msghdr {
                msg_name: std::ptr::null_mut(),
                msg_namelen: 0,
                msg_iov: &iov as *const _ as *mut _,
                msg_iovlen: 1,
                msg_control: cmsg_buf.as_mut_ptr() as *mut _,
                msg_controllen: cmsg_buf.len(),
                msg_flags: 0,
            };

            let ret = unsafe {
                libc::recvmsg(self.stream.as_raw_fd(), &mut msghdr, 0)
            };

            if ret < 0 {
                return Err(io::Error::last_os_error());
            }

            let cmsg = unsafe { libc::CMSG_FIRSTHDR(&msghdr) };
            if cmsg.is_null() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "No ancillary data received"));
            }

            let cmsg = unsafe { &*cmsg };
            if cmsg.cmsg_level != libc::SOL_SOCKET || cmsg.cmsg_type != libc::SCM_RIGHTS {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Expected SCM_RIGHTS"));
            }

            let fd = unsafe { *(libc::CMSG_DATA(cmsg) as *const RawFd) };
            Ok(unsafe { OwnedFd::from_raw_fd(fd) })
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(io::Error::new(io::ErrorKind::Unsupported, "SCM_RIGHTS only on Linux"))
        }
    }

    /// Returns the peer address.
    pub fn peer_addr(&self) -> VsockAddr {
        self.peer
    }

    /// Returns the local address.
    pub fn local_addr(&self) -> VsockAddr {
        self.local
    }
}

impl AsRawFd for VsockStream {
    fn as_raw_fd(&self) -> RawFd {
        self.stream.as_raw_fd()
    }
}

// ── Vsock Listener ─────────────────────────────────────────────────────────────

/// A vsock listener that accepts incoming connections from the Windows host.
///
/// Used by the Shoggoth orchestrator to accept NVENC bitstreams and
/// DMA-BUF file descriptors from Windows GPU nodes.
pub struct VsockListener {
    #[cfg(target_os = "linux")]
    fd: OwnedFd,
    addr: VsockAddr,
}

impl VsockListener {
    /// Binds to a vsock port and listens for connections.
    pub async fn bind(addr: VsockAddr) -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            let fd = unsafe { libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM, 0) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }

            let sockaddr = libc::sockaddr_vm {
                svm_family: libc::AF_VSOCK as u16,
                svm_reserved1: 0,
                svm_port: addr.port,
                svm_cid: libc::VMADDR_CID_ANY,
                svm_flags: 0,
                svm_zero: [0u8; 4],
            };

            let ret = unsafe {
                libc::bind(
                    fd,
                    &sockaddr as *const _ as *const libc::sockaddr,
                    std::mem::size_of::<libc::sockaddr_vm>() as u32,
                )
            };
            if ret < 0 {
                let err = io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(err);
            }

            let ret = unsafe { libc::listen(fd, 8) };
            if ret < 0 {
                let err = io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(err);
            }

            tracing::info!(addr = %addr, "Vsock listener bound");
            Ok(Self {
                fd: unsafe { OwnedFd::from_raw_fd(fd) },
                addr,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = addr;
            Err(io::Error::new(io::ErrorKind::Unsupported, "Vsock only on Linux/WSL2"))
        }
    }

    /// Accepts an incoming vsock connection.
    pub async fn accept(&self) -> io::Result<VsockStream> {
        #[cfg(target_os = "linux")]
        {
            let mut peer_sockaddr: libc::sockaddr_vm = unsafe { std::mem::zeroed() };
            let mut addrlen = std::mem::size_of::<libc::sockaddr_vm>() as u32;

            let client_fd = unsafe {
                libc::accept(
                    self.fd.as_raw_fd(),
                    &mut peer_sockaddr as *mut _ as *mut libc::sockaddr,
                    &mut addrlen,
                )
            };
            if client_fd < 0 {
                return Err(io::Error::last_os_error());
            }

            let peer = VsockAddr {
                cid: peer_sockaddr.svm_cid,
                port: peer_sockaddr.svm_port,
            };

            let owned = unsafe { OwnedFd::from_raw_fd(client_fd) };
            let unix = unsafe {
                std::os::unix::net::UnixStream::from_raw_fd(owned.as_raw_fd())
            };
            std::mem::forget(owned);

            tracing::debug!(peer = %peer, "Vsock connection accepted");
            Ok(VsockStream {
                stream: unix,
                peer,
                local: self.addr,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(io::Error::new(io::ErrorKind::Unsupported, "Vsock only on Linux/WSL2"))
        }
    }
}

// ── DMA-BUF Bridge ─────────────────────────────────────────────────────────────

/// Bridges a DMA-BUF file descriptor from Windows (via AF_HYPERV) into the
/// Linux orchestrator's address space.
///
/// Flow:
///   1. Windows GPU renders into a DX12/Vulkan surface.
///   2. Surface is exported as DMA-BUF fd via Vulkan VK_KHR_external_memory_win32.
///   3. fd is sent over vsock to WSL2 Linux guest.
///   4. Linux orchestrator imports the fd via DRM_PRIME_FD_TO_HANDLE.
///   5. The imported GEM buffer is mapped as a wgpu::Buffer.
pub struct DmaBufBridge {
    /// Vsock stream to the Windows host.
    stream: VsockStream,
    /// DRM render node on the Linux side for fd import.
    #[cfg(target_os = "linux")]
    drm_fd: OwnedFd,
}

impl DmaBufBridge {
    /// Connects to the DMA-BUF bridge on the Windows host.
    pub async fn connect() -> io::Result<Self> {
        let stream = VsockStream::connect(VsockAddr::host(SHOGGOTH_DMABUF_PORT)).await?;

        #[cfg(target_os = "linux")]
        let drm_fd = {
            let nodes = crate::dma_buf_ffi::list_drm_render_nodes()?;
            let first = nodes.first().ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "No DRM render nodes found")
            })?;
            let (fd, path) = crate::dma_buf_ffi::open_drm_render_node(0)?;
            tracing::info!(path = %path, "DMA-BUF bridge: opened DRM node");
            fd
        };

        Ok(Self {
            stream,
            #[cfg(target_os = "linux")]
            drm_fd,
        })
    }

    /// Receives a DMA-BUF fd from Windows and imports it as a GEM handle.
    #[cfg(target_os = "linux")]
    pub async fn recv_and_import_dma_buf(&self) -> io::Result<u32> {
        let fd = self.stream.recv_fd()?;
        let gem_handle = unsafe {
            crate::dma_buf_ffi::drm_prime_fd_to_handle(
                self.drm_fd.as_raw_fd(),
                fd.as_raw_fd(),
            )
        }?;
        // fd is closed by OwnedFd drop; gem_handle is now the canonical reference.
        tracing::debug!(gem_handle, "DMA-BUF imported from Windows host");
        Ok(gem_handle)
    }
}

// ── Platform Availability ──────────────────────────────────────────────────────

/// Returns `true` if the vsock subsystem is available (WSL2 or bare-metal with
/// AF_VSOCK kernel support).
pub fn is_vsock_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        // Check for /dev/vsock device node.
        std::path::Path::new("/dev/vsock").exists()
            || std::fs::metadata("/proc/sys/net/core/vsock_only").is_ok()
    }
    #[cfg(not(target_os = "linux"))]
    false
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vsock_addr_formatting() {
        let addr = VsockAddr::host(9150);
        assert_eq!(addr.to_string(), "vsock:2:9150");

        let local = VsockAddr::local(9100);
        assert_eq!(local.to_string(), "vsock:1:9100");
    }

    #[test]
    fn test_vsock_availability_check() {
        // Should not panic.
        let _ = is_vsock_available();
    }
}
