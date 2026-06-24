# npu-stack — AGENTS.md

## Purpose
NPU-STACK is the Shoggoth machine learning inference and training hub. It provides GGUF edge model quantization workflows, Unsloth fine-tuning loops, NVIDIA NIM inference microservice integration, and OpenAI Triton cross-vendor kernel sharding.

## Ownership
- **Owner**: Shoggoth NPU-STACK Team
- **Language**: Python 3.12+ (FastAPI microservices)
- **Deployment**: Runs alongside the Shoggoth orchestrator on the Xeon host (port 8100).
- **Runtime Dependency**: Communicates with Shoggoth Backbone via WebSocket telemetry and REST control plane on `localhost:9100`.

## Local Contracts
- All Python code must pass `ruff check` and `mypy` (strict mode).
- Dependencies pinned in `pyproject.toml`; use `pip install -e ".[dev]"` for development.
- FastAPI routes must include OpenAPI docstrings for all endpoints.
- WebSocket handlers must be cancellation-safe and handle `WebSocketDisconnect` gracefully.
- Training jobs must stream loss metrics to the Shoggoth dashboard WebSocket at `ws://localhost:9101/ws/telemetry`.

## Work Guidance
- FastAPI 0.115+ with async route handlers and background tasks.
- `unsloth` for memory-efficient LoRA/QLoRA fine-tuning.
- `triton` 3.0+ for cross-vendor GPU kernel compilation (CUDA/ROCm/Xe).
- `scylla-driver` 3.28 for ScyllaDB connectivity (Python shard-aware driver).
- `prometheus-client` for metrics export consumed by the Shoggoth dashboard.
- Triton kernels in `kernels/` must be vendor-agnostic (auto-detect NVIDIA/AMD/Intel at runtime).

## Verification
- `ruff check .` — must pass with zero errors.
- `mypy backend/ kernels/` — must pass in strict mode.
- `pytest -v` — all async tests must pass.
- FastAPI server must start and respond to `GET /health` within 2 seconds.
- `backup_scylla.sh` must be executable (`chmod +x`) and tested against a running ScyllaDB instance.

## Child DOX Index
- `backend/main.py` — FastAPI application entry point with CORS and lifespan.
- `backend/routers/shoggoth_fabric.py` — WebSocket telemetry, node registration, topology REST API.
- `backend/routers/genomic_training.py` — Unsloth fine-tuning launch, status, and cancellation endpoints.
- `kernels/shoggoth_sharder.py` — Triton cross-vendor GEMM kernel with automatic hardware detection.
- `infrastructure/backup_scylla.sh` — Non-blocking ScyllaDB snapshot script for cron-based backups.
