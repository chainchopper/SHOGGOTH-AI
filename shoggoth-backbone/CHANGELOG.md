# Changelog

All notable changes to the Shoggoth Mesh Machine will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — Unreleased (2026-06-24)

### Added — Initial Architecture Scaffolding

#### Core Engine (`shoggoth-core`)
- Hardware fabric bootstrap via wgpu (Vulkan + DX12 backends).
- DMA-BUF zero-copy memory fabric with Linux DRM ioctl FFI.
- Pipeline-parallel compute fabric for heterogeneous GPU routing.
- Lock-free work-stealing thread saturator (crossbeam-deque).
- JIT SPIR-V shader compiler (shaderc-rs + naga).
- Intel QAT compression/encryption hooks (zstd/LZ4/Deflate + AES-256-GCM).
- Real wgpu compute pipeline dispatch (SPIR-V → bind groups → async readback).
- WGSL compute shaders (GEMM, frame blend, spatial hash).

#### SDK (`shoggoth-sdk`)
- Topology discovery and hardware pool catalog (19-node lab inventory).
- QUIC transport layer with certificate generation and stream helpers.
- UDP heartbeat discovery service with automatic liveness tracking.
- WebSocket telemetry server (10 Hz broadcast, per-node metrics).
- Asynchronous multi-tier runtime engine (edge + cloud split).
- Deterministic cluster frame synchronization chain (tokio::Barrier).
- Cloud provisioning engine (Brev.dev + AWS + GCP, auto-scale, cost-aware).
- Prometheus metrics exporter (14 metric families).
- WASM/WebGPU bridge for browser clients.
- Vulkan layer interceptor specification (14 intercepted functions).
- API key authentication (HMAC-SHA256, 3 roles, rate limiting).
- Python bindings via PyO3 (FabricPool, RuntimeEngine, SyncChain).

#### Display Engine (`shoggoth-display`)
- Multi-source frame compositor with SIMD blending.
- Temporal delta viewport compression (spatial hashing + NVENC/AMF).
- Adaptive bitrate WebRTC streaming controller.
- Hardware encoder FFI (NVENC + AMF + VAAPI + Software fallback).
- WebRTC signaling server (SDP relay, ICE exchange).

#### Node Agent (`shoggoth-node-agent`)
- UDP heartbeat broadcast daemon.
- Live QUIC control-plane server (quinn + rustls).
- WorkUnit dispatch and execution (ComputeDispatch, RenderTile, PreloadWeights).

#### Agentic Layer (`shoggoth-agent`)
- AST-based workload classifier (15 keyword patterns, 4 workload types).
- SDK onboarding template generator (5 template types, full TOML manifests).
- Hardware-aware workload-to-node routing engine.

#### Orchestrator (`shoggoth-orchestrator`)
- Axum REST API: `/health`, `/topology`, `/analyze`, `/launch`, `/fabric/nodes`, `/fabric/register`.
- Integrated discovery service (UDP heartbeat → fabric pool).
- Integrated telemetry server (WebSocket → dashboard).

#### Dashboard (`shoggoth-desktop`)
- Tauri v2 + React 19 + TypeScript frontend.
- Emerald `#00FF66` and steel palette.
- Live hardware fabric grid with node health indicators.
- One-click Launchpad templates for 4 workflow types.

#### CLI (`shoggoth-cli`)
- Clap-driven commands: `topology`, `status`, `deploy`, `benchmark`, `launch`.

#### GENEx Platform (`genex-platform`)
- High-throughput chromosome FASTA parser with memory-mapped I/O.
- ScyllaDB shard-per-core genomic data loader (72 shards).
- Blockchain-anchored validation escrow marketplace.
- ScyllaDB schema (5 tables, 2 indexes, 1 materialized view).
- Real ScyllaDB connector with CQL query layer.

#### NPU-STACK
- FastAPI microservice backbone with lifespan management.
- Unsloth fine-tuning REST API (LoRA/QLoRA, 4 model types).
- Shoggoth fabric telemetry WebSocket router.
- Triton cross-vendor kernel sharding (NVIDIA + AMD + Intel auto-detect).
- Non-blocking ScyllaDB backup script.
- Orchestrator bridge for real compute fabric dispatch.

#### Clients & SDKs
- **Python**: Async client (`ShoggothClient` + `TelemetryStream`).
- **TypeScript**: Fetch + WebSocket client (`@shoggoth/client`).
- **C/C++**: Zero-dependency C ABI + RAII C++ wrapper + CMake build.
- **C#/Unity**: `System.Net.Http` + `ClientWebSocket` client with Blueprint types.
- **Unreal Engine**: Plugin scaffold with Movie Render Queue integration.

#### Deployment & Infrastructure
- Docker Compose for orchestrator + node agent + ScyllaDB + NPU-STACK.
- Multi-stage Dockerfiles for orchestrator and node agent.
- Kubernetes manifests: DaemonSet, Deployment, StatefulSet, Services.
- Systemd unit files for production bare-metal deployment.
- Terraform + cloud-init for AWS GPU instance provisioning.
- Grafana dashboard (JSON, 11 panels).
- Prometheus alert rules (7 rules).
- mTLS CA setup script (OpenSSL automation).

#### Developer Tooling
- Makefile with 30+ targets (check, lint, test, build, docker, certs, docs, security).
- mdBook SDK documentation scaffold.
- Cross-crate integration tests (4 suites, 37 tests).
- k6 load testing harness (smoke + stress + spike tests).
- GitHub Actions CI/CD pipeline (4 jobs, cross-platform matrix).
- .env.example with all configuration variables.
- .gitignore for Rust + Node + Python + Docker.

#### Documentation
- Execution plan (`planning.md`) — 7 phases, 50+ trackable checkboxes.
- SDK documentation (`docs/SUMMARY.md`) — architecture, crate overview, templates.
- Edge device hardware/software specification (`docs/edge-device-spec.md`).
- Integration test README (`tests/README.md`).

### Notes
- **105 files** across 3 repositories, 9 Rust crates, 5 client languages.
- All 7 implementation phases are code-complete.
- Hardware benchmarks pending (requires physical lab access).
- Cross-compilation targets pending (`aarch64-unknown-linux-gnu`).
- `npu-stack` scaffolding is local-only; real repo at `chainchopper/npu-stack` must be branched.
