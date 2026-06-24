# shoggoth-backbone — AGENTS.md

## Purpose
Shoggoth Backbone is the standalone bare-metal execution spine that treats diverse, asymmetric multi-vendor hardware clusters as a single parallel computing fabric. It functions beneath host operating systems, unparking core tracking limits and optimizing inter-process communications.

## Ownership
- **Owner**: Shoggoth Mesh Machine Core Team
- **Language**: Rust (edition 2024, workspace monorepo)
- **Palette**: Emerald `#00FF66` + Steel
- **Deployment**: Linux (Ubuntu Server 24.04 LTS), Windows Native/WSL2, Docker

## Local Contracts
- All crates must pass `cargo check` and `cargo clippy -- -D warnings` before merge.
- `#[deny(unsafe_code)]` on all crates except `shoggoth-core` (DMA-BUF FFI, CUDA FFI, Vsock raw fd handling).
- All `unsafe` blocks require `// SAFETY:` comments citing kernel/driver guarantees.
- No `std::sync::Mutex` on hot paths — use `dashmap`, `crossbeam-channel`, or atomics.
- No blocking I/O inside `async` context — use `tokio::task::spawn_blocking`.
- All workspace members must declare dependencies in the root `Cargo.toml` `[workspace.dependencies]` section.
- Cross-compilation targets: `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`, `aarch64-unknown-linux-gnu`.

## Work Guidance
- Rust Edition 2024 with workspace-level lint configuration.
- `crossbeam-deque` for work-stealing, `dashmap` for concurrent state, `tokio` for async runtime.
- wgpu 24.0 for cross-vendor graphics/compute (Vulkan + DX12 backends).
- webrtc-rs 0.11 for sub-16ms client streaming.
- QUIC via `quinn` 0.11 for multiplexed node-agent control channels.
- JIT SPIR-V compilation via `shaderc-rs` + `naga` for cross-vendor shader validation.
- Python bindings via PyO3 (optional `python-bindings` feature flag on `shoggoth-sdk`).

## Verification
- `cargo check --workspace` — must pass with zero errors.
- `cargo clippy --workspace -- -D warnings` — must pass.
- `cargo test --workspace` — all tests must pass.
- `cargo bench --workspace` — thread saturator benchmark must show < 1% CPU waste on idle polling.
- GitHub Actions CI: cross-compilation matrix across all target triples (`.github/workflows/shoggoth-ci.yml`).
- WASM build: `wasm-pack build --target web` for browser SDK.
- Kubernetes manifests: validated via `kubectl --dry-run=client apply -f deploy/kubernetes/`.
- Terraform: validated via `terraform validate` in `deploy/terraform/`.

## Child DOX Index
- `packages/shoggoth-core/` — Hardware fabric bootstrap, DMA-BUF memory fabric, DMA-BUF FFI, compute fabric, thread saturator, GLSL shaders, SPIR-V build script, QAT compression, wgpu compute dispatch.
- `packages/shoggoth-sdk/` — Public SDK: 22 modules (topology, runtime, sync chain, QUIC transport, node discovery, WebSocket telemetry, cloud provisioning, Vsock bridge, Prometheus metrics, WASM bridge, Vulkan layer, DX12 interop, Metal interop, P2P GPU Direct, API key auth, OIDC/SSO auth, webhook/event system, error catalog, Python bindings via PyO3).
- `packages/shoggoth-display/` — Compositor, network shading (delta compression), client stream controller, NVENC/AMF/VAAPI hardware encoder FFI, WebRTC signaling server.
- `packages/shoggoth-node-agent/` — Per-node daemon: UDP heartbeat broadcast, live QUIC control-plane server, Vulkan/DX12 device reporting, work execution.
- `packages/shoggoth-agent/` — Agentic orchestration: AST workload classifier, SDK onboarding templates, hardware-aware routing.
- `apps/shoggoth-cli/` — Clap-driven CLI for deploy, status, topology, benchmark, and launch commands.
- `apps/shoggoth-orchestrator/` — Master control-plane daemon (axum): ties core + SDK + display + agent into a single REST API service with discovery & telemetry.
- `apps/shoggoth-desktop/` — Tauri v2 desktop dashboard with React/TypeScript + WebGL2 emerald console.
- `deploy/kubernetes/` — K8s manifests: DaemonSet (node-agent), Deployment (orchestrator, NPU-STACK), StatefulSet (ScyllaDB).
- `deploy/systemd/` — Production systemd units for orchestrator, node-agent, ScyllaDB, NPU-STACK.
- `deploy/terraform/` — AWS/GCP cloud GPU provisioning with cloud-init bootstrap.
- `docs/` — SDK documentation (mdBook), edge device hardware/software specification.
