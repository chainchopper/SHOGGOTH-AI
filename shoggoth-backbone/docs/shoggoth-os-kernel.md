# ShoggothOS — Custom Linux Kernel Configuration Guide
#
# Target: Ubuntu Server 24.04 LTS with custom kernel 6.12+
# Hardware: Dual Xeon 6240 + Intel QAT + NVIDIA RTX + AMD MI50 + BC250 APUs
#
# This guide documents every kernel configuration option required
# for optimal Shoggoth fabric performance.

## Essential Kernel Features

### DMA-BUF Subsystem (Zero-Copy GPU Memory Sharing)
```
CONFIG_DMA_SHARED_BUFFER=y          # DMA-BUF framework
CONFIG_DMA_BUF=y                    # DMA-BUF file descriptor support
CONFIG_UDMABUF=y                    # Userspace DMA-BUF allocation
CONFIG_DMABUF_HEAPS=y               # DMA-BUF heap allocator
CONFIG_DMABUF_HEAPS_SYSTEM=y        # System heap for fallback
```
**Why**: DMA-BUF is the backbone of cross-vendor GPU memory sharing. Without it, Shoggoth
cannot export NVIDIA GPU buffers as file descriptors for AMD/Intel GPUs to import.

### HugeTLB Pages (Eliminate TLB Misses for Large GPU Transfers)
```
CONFIG_HUGETLBFS=y                  # HugeTLB filesystem
CONFIG_HUGETLB_PAGE=y               # Huge page support
CONFIG_HUGETLB_PAGE_SIZE_VARIABLE=y # Multiple page sizes
CONFIG_TRANSPARENT_HUGEPAGE=y       # THP for automatic huge pages
CONFIG_TRANSPARENT_HUGEPAGE_MADVISE=y
```
**Runtime**: `echo 262144 > /proc/sys/vm/nr_hugepages`  # 512 GB in 2 MB pages.
**Why**: The 512 GB Xeon RAM pool uses 1 GB huge pages to eliminate TLB thrashing when
GPUs DMA large genomic datasets or model weights from system memory.

### Intel QAT (Hardware Compression & Encryption Offload)
```
CONFIG_CRYPTO_DEV_QAT=y             # Intel QuickAssist driver
CONFIG_CRYPTO_DEV_QAT_DH895XCC=y    # DH895x chipset
CONFIG_CRYPTO_DEV_QAT_C3XXX=y       # C3xxx chipset
CONFIG_CRYPTO_DEV_QAT_C62X=y        # C62x chipset (Xeon Scalable)
CONFIG_CRYPTO_DEV_QAT_4XXX=y        # QAT 4xxx (latest gen)
CONFIG_CRYPTO_DEV_QAT_ERROR_INJECTION=n  # Disable for production.
```
**Why**: Offloads DEFLATE/LZ4/ZSTD compression and AES-256-GCM encryption from CPU to
a dedicated PCIe accelerator card, freeing Xeon cores for orchestrator scheduling.

### AF_VSOCK / AF_HYPERV (WSL2 GPU Passthrough)
```
CONFIG_VSOCKETS=y                   # Virtual socket family
CONFIG_VIRTIO_VSOCKETS=y            # VM ↔ host communication
CONFIG_VIRTIO_VSOCKETS_COMMON=y
CONFIG_HYPERV_VSOCKETS=y            # Hyper-V vsock (WSL2)
CONFIG_HYPERV=y                     # Hyper-V guest support
```
**Why**: Bridges Windows RTX 5090/4090 GPU pipelines into the Linux Xeon orchestrator
via AF_HYPERV vsock channels with DMA-BUF fd passing.

### NVIDIA GPU Support
```
CONFIG_DRM=y                        # Direct Rendering Manager
CONFIG_DRM_NVIDIA=y                 # NVIDIA open-source kernel modules
CONFIG_NVIDIA_DRM=y
```
**Runtime**: Proprietary NVIDIA driver 570+ must also be installed.
**Why**: Provides `/dev/dri/renderD*` nodes and `/dev/nvidia*` for CUDA, NVENC, and DMA-BUF.

### AMD GPU Support (ROCm, BC250 APUs, MI50 Instincts)
```
CONFIG_DRM_AMDGPU=y                 # AMDGPU kernel driver
CONFIG_DRM_AMDGPU_SI=y              # Southern Islands (legacy)
CONFIG_DRM_AMDGPU_CIK=y             # Sea Islands (MI50 is CIK-class)
CONFIG_DRM_AMDGPU_USERPTR=y         # Userptr support for ROCm
CONFIG_HSA_AMD=y                    # AMD HSA (ROCm runtime)
CONFIG_HSA_AMD_SVM=y                # Shared Virtual Memory
```
**Why**: Provides `/dev/dri/renderD*` nodes for the 12 BC250 APUs and 2 MI50 Instincts.
ROCm 7.13+ runtime requires `amdgpu` kernel module.

### Preemption (Real-Time Scheduling)
```
CONFIG_PREEMPT=y                    # Low-latency desktop
# OR for hard real-time:
CONFIG_PREEMPT_RT=y                 # Fully preemptible kernel (RT patch)
CONFIG_PREEMPT_RT_FULL=y
```
**Why**: RT preemption ensures the orchestrator's 72 tokio worker threads are never
starved by kernel locks, keeping compositor latency under 8ms for 4K60 streaming.

### Network Performance
```
CONFIG_NET_SCH_FQ=y                 # Fair Queueing (QUIC-friendly)
CONFIG_NET_SCH_FQ_CODEL=y           # Bufferbloat mitigation
CONFIG_TCP_CONG_BBR=y               # BBR congestion control (QUIC)
CONFIG_DEFAULT_TCP_CONG="bbr"
CONFIG_NETPOLL=y                    # Kernel netpoll for low-latency
CONFIG_NET_RX_BUSY_POLL=y           # Busy-polling for sub-ms receive
```
**Why**: BBR congestion control is optimal for QUIC (UDP-based). FQ-CoDel prevents
bufferbloat on the 1 Gbps LAN switch serving 12 BC250 nodes.

### IOMMU / VFIO (PCIe Passthrough & P2P)
```
CONFIG_IOMMU_SUPPORT=y
CONFIG_IOMMU_DEFAULT_DMA_STRICT=n   # Relaxed DMA for P2P
CONFIG_IOMMU_DEFAULT_PASSTHROUGH=y  # Bypass IOMMU for performance
CONFIG_VFIO=y
CONFIG_VFIO_PCI=y
CONFIG_PCI_P2PDMA=y                 # PCIe peer-to-peer DMA
```
**Why**: Enables direct P2P DMA between NVIDIA and AMD GPUs on the same PCIe host without
IOMMU translation overhead. Required for cross-vendor DMA-BUF sharing.

### CXL 3.0 (Future Fabric)
```
CONFIG_CXL_BUS=y
CONFIG_CXL_MEM=y
CONFIG_CXL_PCI=y
```
**Why**: Future-proofing for CXL 3.0 cache-coherent multi-GPU interconnects.

## Verification Commands

After booting the custom kernel, verify all features:

```bash
# DMA-BUF
ls /dev/dri/renderD*                    # Should list all GPU render nodes.
cat /sys/kernel/debug/dma_buf/bufinfo   # Active DMA-BUF allocations.

# HugeTLB
grep Huge /proc/meminfo                 # HugePages_Total should be 262144.

# QAT
ls /dev/qat_*                           # QAT device nodes.
qat_service status                      # QAT service running.

# VSOCK
ls /dev/vsock                           # Vsock device node.

# GPU
nvidia-smi                              # NVIDIA GPUs.
rocm-smi                                # AMD GPUs.

# P2P
lspci -vvv | grep -i "p2p"              # PCIe P2P capability.
```

## Boot Parameters

Add to `/etc/default/grub`:
```
GRUB_CMDLINE_LINUX="iommu=pt pci=pcie_bus_perf intel_iommu=on hugepagesz=1G default_hugepagesz=1G hugepages=256 transparent_hugepage=always pci=p2p_dma=on"
```
Then: `sudo update-grub && sudo reboot`.

## References

- [Linux DMA-BUF Documentation](https://docs.kernel.org/driver-api/dma-buf.html)
- [Intel QAT Driver Guide](https://www.intel.com/content/www/us/en/developer/topic-technology/open/quick-assist-technology/overview.html)
- [NVIDIA DRM Kernel Module](https://github.com/NVIDIA/open-gpu-kernel-modules)
- [AMD ROCm Installation Guide](https://rocm.docs.amd.com/en/latest/)
