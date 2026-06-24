# Shoggoth Mesh Machine — SDK Documentation

Welcome to the **Shoggoth Mesh Machine** developer documentation.

Shoggoth treats diverse, asymmetric multi-vendor hardware clusters (NVIDIA, AMD, Intel, custom APUs) as a **single parallel computing fabric** — a virtual bare-metal execution spine beneath your host operating system.

## Quick Start

### 1. Install the Shoggoth SDK

```toml
# Cargo.toml
[dependencies]
shoggoth-sdk = "0.1"
```

```python
# Python (requires shoggoth-sdk built with python-bindings feature)
import shoggoth
```

### 2. Discover Your Hardware

```rust
use shoggoth_sdk::topology::build_lab_topology;

let pool = build_lab_topology();
println!("{} nodes, {:.1} GB VRAM", pool.active_nodes.len(), pool.total_vram_gb());
```

### 3. Launch a Workload

```rust
use shoggoth_agent::ShoggothAgent;

let agent = ShoggothAgent::new();
let code = "import torch.nn as nn; model = nn.Linear(20, 20).cuda()";
let decision = agent.analyze_and_route(code);
println!("Workload: {:?} → {}", decision.workload, decision.target.node_friendly_name);
```

## Architecture

```
┌─────────────────────────────────────────────┐
│              USER APPLICATIONS               │
│  PyTorch │ Unreal │ Blender │ AlphaFold      │
├─────────────────────────────────────────────┤
│            SHOGGOTH SDK (shoggoth-sdk)       │
│  Topology │ Runtime │ Sync Chain │ Python    │
├─────────────────────────────────────────────┤
│         SHOGGOTH CORE (shoggoth-core)        │
│  Thread Pool │ DMA-BUF │ Compute Fabric      │
├─────────────────────────────────────────────┤
│              HARDWARE FABRIC                 │
│  RTX 5090 │ RTX 4090 │ MI50 │ BC250 │ Cloud  │
└─────────────────────────────────────────────┘
```

## Crate Overview

| Crate | Purpose |
|-------|---------|
| `shoggoth-core` | Hardware fabric bootstrap, thread saturator, DMA-BUF memory fabric, JIT SPIR-V compiler, QAT compression |
| `shoggoth-sdk` | Public API: topology discovery, async runtime, sync chain, QUIC transport, Python bindings, WebSocket telemetry |
| `shoggoth-display` | Multi-source frame compositor, delta viewport compression, adaptive bitrate WebRTC streaming |
| `shoggoth-agent` | Agentic workload classifier, SDK onboarding templates |
| `shoggoth-node-agent` | Per-node daemon (BC250, cloud, edge): heartbeat, QUIC server, work execution |
| `shoggoth-orchestrator` | Master control plane: axum REST API, node discovery, telemetry broadcast |

## Hardware Certification

### Full Shoggoth
A node is a **Full Shoggoth** when:
- VRAM ≥ 48 GB
- Compute cores ≥ 32 (physical)
- Matrix throughput ≥ 100 TFLOPS (FP16)
- Network ≥ 1 Gbps
- DMA-BUF export support

### Shoggoth Limb
A **Shoggoth Limb** contributes to the fabric with VRAM ≥ 8 GB and Vulkan/DX12/CUDA/ROCm compute capability.

## SDK Templates

| Template | Use Case |
|----------|----------|
| Render Farm | Blender / Unreal / Omniverse → BVH shard to RT cores |
| Heavy Compute | PyTorch / AlphaFold / CUDA → pipeline-parallel layer sharding |
| Async Game Runtime | Unity / Unreal → split UI/Sim (Edge) + Lighting/AI (Cloud) |
| Genomic Processing | FASTA → ScyllaDB shard-per-core + BC250 alignment |

## Network Protocol

- **Node ↔ Orchestrator**: QUIC/UDP with certificate-pinned mTLS (port 9100)
- **Orchestrator → Dashboard**: WebSocket telemetry at 10 Hz (port 9101)
- **Orchestrator ↔ NPU-STACK**: REST + WebSocket (port 8100)
- **Orchestrator ↔ GENEx**: Unix domain sockets + shared memory (`/dev/shm/shoggoth/`)

## License

Apache-2.0 (Backbone + SDK). UNLICENSED (GENEx Platform).
