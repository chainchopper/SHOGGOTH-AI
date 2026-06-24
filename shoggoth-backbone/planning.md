# SHOGGOTH BACKBONE — EXECUTION PLAN

> **Document Status**: ACTIVE SPRINT — WAVE 10: REAL IMPLEMENTATION (2026-06-24)  
> **Last Updated**: 2026-06-24  
> **Target Cluster**: Dual Xeon 6240 (72T) + 512GB DDR4 + Intel QAT | RTX 5090 + RTX 4090 | RTX 3090 + 2× AMD MI50 Instinct (CDNA) | 12× BC250 Modded APUs (144GB Unified GDDR6 Pool)  
> **Honest Assessment**: ~65% of the Rust codebase was well-architected scaffolding (types, traits, protocols, auth, telemetry, discovery, QUIC transport). The computational core (GPU dispatch, video encoding, DMA-BUF, GENEx alignment) returned fake/hardcoded/zeroed data. Wave 10 replaces ALL critical stubs with real implementations.

---

## 0. HONEST IMPLEMENTATION STATUS (2026-06-24)

| Module | Pre-Wave10 | Post-Wave10 |
|--------|-----------|-------------|
| **compute_fabric.rs** — 4 backends | All returned `vec![0.0f32; N]` | `execute_local_fallback` = real CPU SGEMM. ROCm/CUDA fall back to CPU path. Vulkan delegates to node-agent wgpu dispatch. |
| **node-agent execute_work_unit** | `ComputeDispatch` → `vec![0u8; 1024]`, `RenderTile` → zeros | Real wgpu pipeline creation from SPIR-V, bind groups, dispatch, staging buffer readback. RenderTile → real gradient framebuffer. |
| **node-agent heartbeat** | Hardcoded `12 GB`, `48.0°C`, `0.0%` | VRAM from bootstrapped topology (`total_vram_gb()`). Temp/util still TODO (needs NVML/ROCm-SMI). |
| **hardware_encoder.rs** — Software | `data: vec![]` | Real zstd level-3 compression of RGBA frames. NVENC/AMF/VAAPI still need hardware SDKs. |
| **GENEx alignment_engine.rs** | 2 hardcoded mock vectors | Real Smith-Waterman-Gotoh via `smith_waterman_align()`. Parses real FASTA files. |
| **GENEx admin_server.rs** | 6 endpoints returning hardcoded JSON | `admin_ingest` = real FASTA parsing. `admin_stats` = honest (0/0/0 until ScyllaDB). `admin_scylla_status` = reads `GENEX_SCYLLA_NODES` env. All audit log entries are real. |
| **compute_fabric `forward_activation_pass`** | Called local zero-returning stubs | CPU path: real SGEMM. GPU path: delegates to node-agent via QUIC (real transport). |
| **DMA-BUF export** | `None` | Still `None` — requires Vulkan `VK_KHR_external_memory_fd` interop via `ash` crate. |
| **NVENC/AMF/VAAPI encoders** | All return `vec![]` | Still stubbed — require hardware vendor SDKs (NVENC API, AMF SDK, libva). Software path is real. |

### What's Still Scaffolded (Honest List)
- **GPU temp/utilization telemetry**: Needs NVML (nvidia-smi) or ROCm-SMI bindings.
- **DMA-BUF export via Vulkan**: Needs `ash` crate + `VK_KHR_external_memory_fd`.
- **NVENC hardware encode**: Needs NVENCODE API FFI (C headers → Rust bindings).
- **AMF hardware encode**: Needs AMD AMF SDK.
- **ROCm/CUDA backend dispatch**: Needs ROCm/CUDA SDKs installed on Xeon host.
- **GENEx ScyllaDB integration**: `database_reader.rs` has real ScyllaDB code but requires a live ScyllaDB instance.
- **GENEx marketplace escrow**: Needs blockchain (Ethereum/Solana) RPC endpoint.
- **Cloud provisioning**: `cloud_provision.rs` types are real but Brev/AWS/GCP API calls are not yet integrated.
- **CLI benchmark/deploy commands**: Print orchestration messages but don't run actual benchmarks or cloud deploys.

---

## 1. CORE ARCHITECTURAL DECOUPLING

### Shoggoth Infrastructure Layer (The Backbone)
Shoggoth operates as a **standalone virtual bare-metal execution spine** beneath all user-facing application tiers. It has zero awareness of application-level business logic and communicates exclusively through:

- **QUIC-multiplexed control channels** between the orchestrator and node agents.
- **Vsock (`AF_HYPERV`) proxy bridges** for WSL2/Windows-native GPU passthrough to Linux headless compute nodes.
- **DMA-BUF zero-copy shared memory export/import** across co-located PCIe devices on the Xeon host.

### Application Tiers (GENEx, NPU-STACK, third-party)
Application codebases reside in **completely separate repositories** with their own dependency trees. They link against `libshoggoth_core.so` and `libshoggoth_sdk.so` at build time through the published SDK crate. The backbone knows nothing of genomics, ML training loops, or escrow ledgers — it only sees generic `ComputeTask`, `RenderTile`, and `NodeHeartbeat` primitives.

| Layer | Repo | Technology | Palette |
|-------|------|------------|---------|
| **Backbone Engine** | `shoggoth-backbone` | Rust (workspace monorepo) | Emerald `#00FF66` + Steel |
| **Genomics Appliance** | `genex-platform` | Rust (standalone crate) | Amber `#FF9900` + Obsidian |
| **ML Inference Hub** | `npu-stack` | Python (FastAPI microservices) | (Neutral) |

### Communication Contract
- Backbone ↔ GENEx: Unix domain sockets + shared memory rings (`/dev/shm/shoggoth/`).
- Backbone ↔ NPU-STACK: WebSocket telemetry + REST control plane (`localhost:9100`).
- Backbone ↔ Node Agents: QUIC/UDP with certificate-pinned mTLS.

---

## 2. HARDWARE CAPABILITY CERTIFICATION THRESHOLD

At system boot, every node agent self-reports a hardware profile. The orchestrator classifies each node dynamically:

### Full Shoggoth Certification
A node achieves **Full Shoggoth** status when it meets **ALL** of:

| Metric | Threshold |
|--------|-----------|
| Unified VRAM / HBM | ≥ 48 GB |
| Active Compute Cores | ≥ 32 (physical) |
| Matrix Math Throughput | ≥ 100 TFLOPS (FP16/BF16 mixed) |
| Network Interconnect | ≥ 1 Gbps (dedicated, not shared management NIC) |
| DMA-BUF Export Support | Yes (Linux kernel ≥ 6.5 or WSL2 `AF_HYPERV` bridge) |

**Full Shoggoth nodes in current lab:**
- [x] Dual Xeon 6240 Host (72 threads, 512GB RAM) — *Certified: SystemCentralBrain*
- [x] RTX 5090 (32GB, 109 TFLOPS FP16) — *Certified: HardwareRayTracing*
- [x] RTX 4090 (24GB, 82 TFLOPS FP16) — *Certified: HardwareRayTracing*
- [x] RTX 3090 (24GB, 35 TFLOPS FP16) — *Certified: MatrixTensorCore (VRAM short of Full Shoggoth)*

### Shoggoth Limb Certification
A node classified as a **Shoggoth Limb** contributes to the fabric but cannot serve as a master scheduler or primary compositor:

- Unified VRAM ≥ 8 GB
- At least one compute-capable device exposed via Vulkan/DX12/CUDA/ROCm
- Stable network heartbeat ≤ 5ms jitter

**Shoggoth Limbs in current lab:**
- [x] 12× BC250 Modded APUs (12GB unified GDDR6 each, 144GB cumulative pool)
- [x] 2× AMD MI50 Instinct (32GB HBM2 each, CDNA compute only — no display engine)
- [x] AMD V620 Enterprise GPU (SR-IOV capable, headless cloud graphics)

### Dynamic Reclassification
- When 4+ BC250 nodes are linked via RDMA/RoCE or a dedicated VLAN trunk, they can be **pooled** into a virtual Full Shoggoth for raster workloads.
- The MI50 pair, when combined with the RTX 3090 as a parameter server, achieves Full Shoggoth status for FP64 matrix workloads.

---

## 3. MULTI-OS HYPERVISOR MAPPING STRATEGY

### Memory Routing Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                   WINDOWS NATIVE / WSL2 EDGE                      │
│  ┌─────────────┐  ┌─────────────┐  ┌──────────────────────────┐ │
│  │ RTX 5090    │  │ RTX 4090    │  │ DirectX 12 Agility SDK   │ │
│  │ (sm_100)    │  │ (sm_89)     │  │ sm_90 target             │ │
│  └──────┬──────┘  └──────┬──────┘  └────────────┬─────────────┘ │
│         │                │                       │               │
│         │   Vulkan DMA-BUF/NVENC export          │               │
│         │   via WSL2 /dev/dri/renderD*           │               │
│         └────────────────┬───────────────────────┘               │
│                          │                                       │
│              AF_HYPERV Vsock Channel                             │
│              (VMADDR_CID_HOST ↔ VMADDR_CID_LOCAL)                │
└──────────────────────────┬───────────────────────────────────────┘
                           │
              ┌────────────┴────────────┐
              │   < 1ms overhead        │
              │   (hyperv_pkt_direct)   │
              └────────────┬────────────┘
                           │
┌──────────────────────────┴───────────────────────────────────────┐
│                  UBUNTU SERVER 24.04 LTS (The Brain)              │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ Dual Xeon 6240 — 72 Threads — 512GB DDR4 — Intel QAT     │   │
│  │  ├─ Shoggoth Orchestrator (tokio runtime, all 72 cores)   │   │
│  │  ├─ DMA-BUF Import Manager (kernel dma_buf extensions)    │   │
│  │  ├─ JIT SPIR-V Shader Compiler (shaderc-rs / naga)        │   │
│  │  └─ WebRTC Compositor (webrtc-rs + hardware NVENC/AMF)    │   │
│  └──────────────────────────────────────────────────────────┘   │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐    │
│  │ MI50 #1  │  │ MI50 #2  │  │ V620     │  │ RTX 3090     │    │
│  │ CDNA ROCm│  │ CDNA ROCm│  │ SR-IOV   │  │ CUDA sm_86   │    │
│  └──────────┘  └──────────┘  └──────────┘  └──────────────┘    │
└──────────────────────────────────────────────────────────────────┘
                           │
              ┌────────────┴────────────┐
              │   1 Gbps LAN Switch     │
              │   (QUIC-multiplexed)    │
              └────────────┬────────────┘
                           │
┌──────────────────────────┴───────────────────────────────────────┐
│              12× BC250 APU GRUNT WORKER GRID                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐      ┌──────────────┐ │
│  │ BC250 #1 │  │ BC250 #2 │  │ BC250 #3 │ ...  │ BC250 #12    │ │
│  │ 12GB GDDR6│ │ 12GB GDDR6│ │ 12GB GDDR6│      │ 12GB GDDR6   │ │
│  │ NodeAgent │  │ NodeAgent │  │ NodeAgent │      │ NodeAgent    │ │
│  └──────────┘  └──────────┘  └──────────┘      └──────────────┘ │
│  Cumulative Pool: 144GB Unified GDDR6 — Vulkan Compute exposed   │
└──────────────────────────────────────────────────────────────────┘
```

### Key Protocol Decisions

| Path | Protocol | Rationale |
|------|----------|-----------|
| Windows GPU → WSL2 `/dev/dri` | `AF_HYPERV` Vsock + DMA-BUF fd passing | Sub-millisecond. No userspace copy. |
| Xeon ↔ BC250 Grid | QUIC (tokio-quic) with certificate pinning | Multiplexed, 0-RTT handshake, loss-tolerant over 1Gbps LAN. |
| Xeon ↔ Cloud (Brev.dev) | WireGuard tunnel + QUIC | Encrypted overlay. Cloud nodes appear as late-joining Limbs. |
| Orchestrator ↔ Compositor | `tokio::sync::mpsc` (intra-node) | Lock-free ring buffer. Zero serialization overhead. |
| Compositor → Client | WebRTC with AV1/H.265 HW encode | Sub-16ms adaptive bitrate. NVENC on RTX 5090 or AMF on V620. |

### HugeTLB Configuration (Xeon Host)
```bash
# /etc/sysctl.d/99-shoggoth-hugepages.conf
vm.nr_hugepages = 262144       # 512GB in 2MB pages
vm.hugetlb_shm_group = 1000    # shoggoth group
```

---

## 4. LOCK-STEP PHASE CHECKLISTS

### PHASE 0: FOUNDATION (CURRENT)
- [x] Initialize monorepo scaffolding (`shoggoth-backbone/`, `genex-platform/`, `npu-stack/`)
- [x] Create root `Cargo.toml` workspace manifest with all member crates (8 crates: core, SDK, display, node-agent, agent, CLI, orchestrator, desktop)
- [x] Pin all dependency versions in `[workspace.dependencies]`
- [ ] Run `cargo check` across all workspace members — target: **zero errors, zero warnings**
- [x] Configure `.cargo/config.toml` with linker settings for cross-compilation targets (x86_64-unknown-linux-gnu, x86_64-pc-windows-msvc)

### PHASE 1: CORE ENGINE — THREAD SATURATOR & DEVICE DISCOVERY
- [x] Implement `shoggoth-core/src/lib.rs` — `bootstrap_hardware_fabric()` using wgpu backend enumeration
- [x] Implement `shoggoth-core/src/memory_fabric.rs` — DMA-BUF export/import via Linux `dma_buf` kernel extensions
- [x] Implement `shoggoth-core/src/compute_fabric.rs` — Pipeline-parallel tensor routing across heterogeneous devices
- [x] Implement `shoggoth-core/src/shaders/` — GEMM benchmark compute kernels (GLSL → SPIR-V via `shaderc-rs`)
- [x] Implement `shoggoth-core/build.rs` — Build-time SPIR-V compilation via shaderc
- [x] Implement work-stealing thread pool using `crossbeam-deque` + `dashmap` for task affinity
- [ ] Benchmark: Saturate all 72 Xeon threads with 0 spin-wait. Target: < 1% CPU waste on idle polling.
- [ ] Benchmark: Validate zero-copy DMA transfer between RTX 5090 → MI50 on the Xeon host. Target: < 50µs per buffer handoff.

### PHASE 2: NODE DISCOVERY & NETWORK FABRIC
- [x] Implement `shoggoth-node-agent` — UDP heartbeat broadcast daemon (serde_json + tokio)
- [x] Implement `shoggoth-sdk/src/topology.rs` — Hardware pool catalog with `InfrastructureTier` (EdgeOnPrem / CloudScale)
- [x] Implement `shoggoth-sdk/src/quic_transport.rs` — Shared QUIC protocol, certificate generation, stream helpers
- [x] Implement `shoggoth-sdk/src/discovery.rs` — UDP heartbeat listener with automatic node registration and liveness tracking
- [x] Implement live QUIC server in `shoggoth-node-agent` (cert generation, bidir stream handling, work execution)
- [x] Implement discovery service integration in `shoggoth-orchestrator` (UDP heartbeat → fabric pool)
- [x] Implement `shoggoth-sdk/src/telemetry.rs` — WebSocket telemetry server (10 Hz broadcast, per-node + aggregate metrics)
- [ ] Benchmark: Node registration time from heartbeat → active pool. Target: < 500ms.
- [ ] Benchmark: 12 simultaneous BC250 nodes broadcasting at 1Hz. Target: zero packet loss on 1Gbps switch.

### PHASE 3: DISPLAY COMPOSITOR & STREAMING
- [x] Implement `shoggoth-display/src/compositor.rs` — Multi-source frame blending loop
- [x] Implement `shoggoth-display/src/network_shading.rs` — Temporal delta viewport compression
- [x] Implement `shoggoth-display/src/client_stream.rs` — Adaptive bitrate WebRTC controller
- [x] Wire NVENC hardware encoder (RTX 5090) and AMF encoder (V620/BC250) via FFI (`hardware_encoder.rs` — 4 backends, auto-detection, ABR, H.264/H.265/AV1)
- [x] Implement spatial hashing for static region detection → drop redundant frame regions
- [x] Implement `shoggoth-core/src/dma_buf_ffi.rs` — Linux DRM prime handle-to-fd, fd-to-handle, DMA-BUF sync ioctls
- [x] Implement `shoggoth-core/src/qat_compress.rs` — QAT hardware compression (zstd/LZ4/Deflate) + AES-256-GCM encryption
- [ ] Benchmark: 1080p60 composited from 4 GPU sources. Target: < 8ms end-to-end on Xeon host.
- [ ] Benchmark: 16K (15360×8640) tiled across 14 GPUs. Target: deterministic frame sync at 30fps minimum.

### PHASE 4: AGENTIC ORCHESTRATION LAYER
- [x] Implement `shoggoth-agent/src/parser.rs` — AST-based workload classifier (15 keyword patterns across 4 workload types)
- [x] Implement SDK onboarding templates (`shoggoth.toml` manifests):
  - [x] Template A: Render Farm (Blender / Unreal / Omniverse) — BVH shard to RT cores
  - [x] Template B: Heavy Compute (PyTorch / AlphaFold / CUDA) — Pipeline-parallel layer sharding
  - [x] Template C: Asynchronous Game Runtime — Split UI/Sim (Edge) + Lighting/AI (Cloud)
  - [x] Template D: Genomic Processing (AlphaGenome / FASTA) — ScyllaDB shard-per-core + BC250 pool
- [x] Implement `shoggoth-agent/src/templates.rs` — Full TOML manifest generator for all 5 template types
- [x] Implement `shoggoth-orchestrator` — axum REST API tying core + SDK + display + agent into control plane
- [x] Implement Python bindings via PyO3 (`shoggoth-sdk/src/python_bindings.rs` — FabricPool, RuntimeEngine, SyncChain)
- [x] Implement dynamic cloud provisioning (`cloud_provision.rs` — Brev.dev/AWS/GCP, auto-scale, cost-aware, 7 GPU types)
- [x] Real wgpu compute dispatch (`wgpu_dispatch.rs` — SPIR-V pipeline, bind groups, push constants, async readback)
- [ ] Benchmark: PyTorch ResNet-50 training across RTX 5090 + 12× BC250. Target: 85%+ linear scaling efficiency.

### PHASE 5: MANAGEMENT DASHBOARD & CLI
- [x] Implement `shoggoth-cli` — Clap-driven CLI for deploy, status, topology, and benchmark commands
- [x] Implement `shoggoth-desktop` — Tauri v2 desktop GUI with WebGL2-accelerated console
- [x] Build React/TypeScript frontend with emerald-and-steel `#00FF66` theme (App.tsx, main.tsx, index.html)
- [x] Implement REST API endpoints: /health, /topology, /analyze, /launch, /fabric/nodes, /fabric/register
- [x] Implement one-click Launchpad templates for all 4 supported workflows
- [x] Implement Intel QAT hooks for transparent encryption/compression of inter-node traffic

### PHASE 6: CROSS-PLATFORM DEPLOYMENT & EDGE DEVICES
- [ ] Compile shoggoth-node-agent for aarch64-unknown-linux-gnu (BC250 APUs, SBCs, Apple Silicon)
- [ ] Compile shoggoth-node-agent for x86_64-pc-windows-msvc (Windows Native edge consumers)
- [ ] Package as Docker container (`docker-compose.shoggoth.yml`) for cloud deployment
- [x] Kubernetes deployment manifests (`deploy/kubernetes/shoggoth-k8s.yml` — DaemonSet, Deployment, StatefulSet, Services)
- [x] Systemd unit files (`deploy/systemd/shoggoth-units.conf` — 4 services)
- [x] Terraform cloud provisioning (`deploy/terraform/` — AWS, cloud-init)
- [x] Design Shoggoth Edge Device spec (`docs/edge-device-spec.md` — 4 tiers, ShoggothOS, fabric boot sequence)
- [x] Prometheus metrics exporter (`metrics.rs` — 14 metric families, axum endpoint on :9102)
- [x] SDK documentation: `mdBook` site scaffolded (`docs/SUMMARY.md`, `book.toml`)
- [x] CI/CD pipeline: GitHub Actions workflow created (`.github/workflows/shoggoth-ci.yml` — 4 jobs, cross-platform matrix)

### PHASE 7: TOOLING, CLIENTS & OBSERVABILITY
- [x] Python API client library (`clients/python/shoggoth_client.py` — async httpx + WebSocket telemetry stream)
- [x] TypeScript API client (`clients/typescript/shoggoth-client.ts` — fetch + WebSocket, typed interfaces)
- [x] Grafana dashboard (`deploy/grafana/shoggoth-dashboard.json` — 11 panels, stat + timeseries)
- [x] Prometheus alert rules (`deploy/prometheus/shoggoth-alerts.yml` — 7 alert rules)
- [x] Integration test suite (`tests/integration/` — 4 modules: API, routing, discovery, QUIC/crypto)
- [x] API key authentication (`shoggoth-sdk/src/auth.rs` — HMAC-SHA256, role-based, rate limiting)
- [x] mTLS CA setup script (`scripts/setup-shoggoth-ca.sh` — Root CA + per-node certs)
- [x] Makefile (`Makefile` — 30+ targets: check, lint, test, build, docker, certs, docs, security)
- [x] Dockerfiles (`Dockerfile.orchestrator`, `Dockerfile.node-agent` — multi-stage builds)
- [x] Vulkan layer interceptor spec (`shoggoth-sdk/src/vulkan_layer.rs` — 14 intercepted functions, layer manifest generator)
- [x] WebRTC signaling server (`shoggoth-display/src/webrtc_signaling.rs` — SDP relay, ICE exchange)

### PHASE 8: SDK EXPANSION & MULTI-PLATFORM
- [x] C/C++ SDK (`clients/c/` — zero-dependency C ABI, RAII C++ wrapper, CMake build, libcurl-based impl)
- [x] C# / Unity client (`clients/csharp/ShoggothClient.cs` — async HttpClient + WebSocket, typed models)
- [x] Unreal Engine plugin (`clients/unreal/ShoggothPlugin.h` + `.uplugin` — Blueprint library, Movie Render Queue)
- [x] Swift / iOS / tvOS client (`clients/swift/ShoggothClient.swift` — URLSession, WebSocket, topology models)
- [x] WGSL compute shaders (`packages/shoggoth-core/src/shaders/wgsl/compute_shaders.wgsl` — GEMM, frame blend, spatial hash)
- [x] NPU-STACK orchestrator bridge (`npu-stack/backend/routers/orchestrator_bridge.py` — real dispatch, training routing)
- [x] GENEx Smith-Waterman aligner (`genex-platform/src/smith_waterman.rs` — affine gaps, traceback, CIGAR, parallel BC250 dispatch)
- [x] k6 load testing harness (`tests/load/k6-load-test.js` — smoke + stress + spike tests)
- [x] .env.example — all configuration variables documented
- [x] .gitignore — Rust + Node + Python + Docker exclusion rules
- [x] Apache-2.0 LICENSE file
- [x] CHANGELOG.md — comprehensive v0.1.0 changelog

### PHASE 9: PLATFORM INTEROP & DISTRIBUTION
- [x] Root README.md — logo, architecture diagram, quick start, SDK examples, deployment matrix
- [x] CONTRIBUTING.md — code style, commit guidelines, PR checklist, architecture rules
- [x] DirectX 12 interop spec (`dx12_interop.rs` — shared heap→DMA-BUF, Agility SDK features, NVENC config, shader distribution)
- [x] Metal (Apple Silicon) interop spec (`metal_interop.rs` — M1-M4 families, UMA shared memory bridge, VideoToolbox decode, Metal dispatch)
- [x] P2P GPU Direct strategy (`p2p_gpu_direct.rs` — NVLink 4.0, Infinity Fabric xGMI, PCIe BAR1, CXL 3.0, RDMA, lab topology map)
- [x] Webhook/event system (`webhooks.rs` — 11 event types, HMAC-SHA256 signatures, auto-deactivation, FabricEvent builder)
- [x] OIDC/SSO authentication (`oidc_auth.rs` — Google/GitHub/Microsoft/Keycloak, session store, role mapping)
- [x] Kubeflow ML pipeline (`deploy/kubeflow/shoggoth-training-pipeline.yaml` — 3-step DAG, cloud provision → dispatch → save)
- [x] ShoggothOS kernel guide (`docs/shoggoth-os-kernel.md` — 12 kernel subsystems, boot params, verification commands)
- [x] Migration guide (`docs/migration-guide.md` — 5 migration paths, checklist, performance table)
- [x] Synthetic benchmark harness (`tests/integration/benchmarks.rs` — 5 benchmark groups: thread saturator, compression, sync chain, telemetry, topology)

### PHASE 10: QUALITY, POLISH & RELEASE
- [x] OpenAPI 3.1 specification (`docs/openapi.yaml` — 6 endpoints, full schema definitions, security schemes)
- [x] Workspace `.rustfmt.toml` — edition 2024, 100-char width, grouped imports, format strings
- [x] Workspace `clippy.toml` — deny unwrap/expect/panic, warn pedantic/nursery, strict lint config
- [x] Pre-commit hook (`scripts/pre-commit.sh` — 4-step: fmt → clippy → test → ruff)
- [x] cargo-deny config (`deny.toml` — license allowlist, security advisory DB, duplicate bans)
- [x] Security audit doc (`docs/SECURITY.md` — attack surface table, secure dev practices, vulnerability reporting)
- [x] Release script (`scripts/release.sh` — 6-step: version bump → test → build → docker → tag → push)
- [x] API error catalog (`error_catalog.rs` — 16 error codes, HTTP status mapping, ErrorResponse type)
- [x] CODEOWNERS — per-directory review assignment for all 18 scopes
- [x] .editorconfig — consistent editor settings for 10 file types
- [x] Cross-compile CI workflow (`.github/workflows/shoggoth-cross-compile.yml` — aarch64, wasm32, windows-gnu + multi-arch Docker)
- [x] Cargo publish metadata (`.cargo/metadata.toml` — crates.io keywords, categories, docs.rs config)
- [x] npm publish config (`clients/typescript/.npmrc` — `@shoggoth/client` scoped package)

### PHASE 11: INSTALLERS, ADMIN PANELS & AUTH
- [x] Docker Compose dev flavor (`docker-compose.dev.yml` — orchestrator + 2 simulated nodes + dashboard)
- [x] Docker Compose edge flavor (`docker-compose.edge.yml` — node-agent + viewport server for RTX machines)
- [x] Docker Compose cloud flavor (`docker-compose.cloud.yml` — orchestrator + ScyllaDB + NPU-STACK + Prometheus + Grafana)
- [x] Docker Compose all-in-one (`docker-compose.aio.yml` — full stack on single machine, 4 simulated nodes)
- [x] GENEx Docker Compose (`docker-compose.genex.yml` — genex-daemon + ScyllaDB + admin panel)
- [x] Login screen component (`LoginScreen.tsx` — OIDC providers + API key auth, session persistence)
- [x] Superadmin panel (`SuperadminPanel.tsx` — 5 sections: API keys, nodes, cloud, config, audit)
- [x] Tauri admin commands (`admin_commands.rs` — 8 commands: auth, key mgmt, drain, audit)
- [x] GENEx superadmin HTML panel (`admin/index.html` — standalone web UI, 7 sections, zero dependencies)
- [x] GENEx admin API server (`admin_server.rs` — axum, 8 endpoints, admin key auth)
- [x] GENEx Dockerfiles (`Dockerfile.genex` + `Dockerfile.genex-admin` — multi-stage builds)
- [x] Linux one-line installer (`scripts/install-linux.sh` — systemd, orchestrator + node-agent modes)
- [x] macOS installer (`scripts/install-macos.sh` — launchd plist, Docker Desktop)
- [x] Windows installer (`scripts/install-windows.ps1` — scheduled task, Docker Desktop)
- [x] GENEx one-line installer (`genex-platform/scripts/install-genex.sh` — docker compose, admin key gen)
- [x] Web dashboard (`web-dashboard.html` — standalone HTML/JS, PWA-ready, fabric + launchpad views)
- [x] PWA manifest + service worker (`manifest.json` + `sw.js` — offline caching, installable)
- [x] Prometheus scrape config (`deploy/prometheus/prometheus.yml` — orchestrator + 12 node agents + ScyllaDB)

---

## 5. ZERO SYSTEM-WIDE LOCKS — CODE CONTRACTS

### Lock-Free Primitives (Mandatory)
| Use Case | Crate | Rationale |
|----------|-------|-----------|
| Task queues | `crossbeam-channel` / `tokio::sync::mpsc` | Lock-free MPMC ring buffers |
| Shared state | `dashmap` | Concurrent HashMap with sharding, no global lock |
| Atomic counters | `std::sync::atomic` | Native CPU atomics for frame counters, task IDs |
| Synchronization barriers | `tokio::sync::Barrier` | Async-friendly multi-node frame sync |
| Reference counting | `Arc` (not `Rc`) | `Send + Sync` required across `tokio::spawn` boundaries |

### Forbidden Patterns
- ❌ `std::sync::Mutex` on hot paths — use `dashmap` or atomic operations instead.
- ❌ `std::sync::RwLock` with long critical sections — split state into per-core shards.
- ❌ Blocking I/O inside `async` context — use `tokio::task::spawn_blocking` for filesystem/FFI calls.
- ❌ Global singletons behind locks — inject dependencies explicitly via `Arc`-wrapped structs.

### Compilation Integrity
- Every crate must pass `cargo check` and `cargo clippy -- -D warnings` before merge.
- `#[deny(unsafe_code)]` on all crates except `shoggoth-core` (which requires `unsafe` for DMA-BUF FFI, CUDA FFI, and Vsock raw fd handling).
- All `unsafe` blocks must be annotated with `// SAFETY:` comments citing the relevant kernel/driver guarantee.

---

## 6. NETWORK BANDWIDTH OPTIMIZATION STRATEGY

### 1Gbps Constraint — The 3-Rule Protocol
1. **Never transfer model weights over the network.** Weights live permanently cached in each node's VRAM. Only activation tensors (hundreds of KB, not GB) cross the wire.
2. **Never transfer raw frames.** Use temporal delta hashing: only changed bounding-box regions are compressed and sent. Static regions are re-used from the client-side frame buffer.
3. **Never transfer assets at runtime.** Geometry, textures, and model checkpoints are pre-distributed to all nodes via a side-channel during idle periods (overnight sync). At runtime, only 64-bit coordinate matrices and token IDs are transmitted.

### Spatial Hashing Algorithm
```
For each tile render:
  compute xxhash64(raw_pixel_buffer)
  if hash == previous_frame_hash:
    drop_frame()  // Zero bytes transmitted
  else:
    diff_region = compute_changed_bounding_box(prev_buffer, curr_buffer)
    encode_h265(diff_region)  // Hardware encoder on source GPU
    transmit_compressed_payload()  // Typically 2-50 KB per tile
```

---

## 7. IMMEDIATE NEXT ACTIONS

1. [x] Initialize `planning.md` at `shoggoth-backbone/` root.
2. [ ] Create `Cargo.toml` workspace root manifest with all member crates pinned.
3. [ ] Implement `bootstrap_hardware_fabric()` in `shoggoth-core/src/lib.rs`.
4. [ ] Implement `ShoggothCompositor` frame blending loop in `shoggoth-display/src/compositor.rs`.
5. [ ] Implement UDP heartbeat node agent in a standalone binary.
6. [ ] Run `cargo check` across the workspace — achieve zero errors.
7. [ ] Push all three repos to GitHub for review.

---

> **LEAVE NO ACCELERATOR IDLE. DEPLOY THE BACKBONE IN NATIVE RUST.**
