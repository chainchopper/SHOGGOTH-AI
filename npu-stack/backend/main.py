"""NPU-STACK — Shoggoth Machine Learning Inference & Training Hub.

Provides a FastAPI microservice backbone that bridges the Shoggoth Mesh Machine
fabric to ML workloads:

    • GGUF edge model quantization workflows.
    • Unsloth fine-tuning loops with parameter-efficient adapters (LoRA/QLoRA).
    • NVIDIA NIM inference microservice integration.
    • OpenAI Triton custom cross-vendor matrix compute shards.
    • Live WebSocket telemetry for the Shoggoth dashboard.

Architecture:
    backend/main.py          — FastAPI application entry point.
    backend/routers/
        shoggoth_fabric.py   — WebSocket telemetry, node heartbeat, pool registration.
        genomic_training.py  — Unsloth training routers for AlphaFold/AlphaGenome workloads.
    kernels/
        shoggoth_sharder.py  — Triton custom shards for cross-vendor (AMD+NVIDIA) GEMM.
    infrastructure/
        backup_scylla.sh     — Non-blocking ScyllaDB snapshot script.
"""

import logging
from contextlib import asynccontextmanager

import uvicorn
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from routers import genomic_training, shoggoth_fabric

# ── Logging ─────────────────────────────────────────────────────────────────────

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
)
logger = logging.getLogger("npu-stack")

# ── Lifespan ────────────────────────────────────────────────────────────────────


@asynccontextmanager
async def lifespan(_app: FastAPI):
    """Startup/shutdown lifecycle for the NPU-STACK service."""
    logger.info("NPU-STACK starting — connecting to Shoggoth fabric on localhost:9100")
    # In production: establish QUIC connection to shoggoth-orchestrator.
    yield
    logger.info("NPU-STACK shutting down")


# ── Application ─────────────────────────────────────────────────────────────────

app = FastAPI(
    title="NPU-STACK",
    description="Shoggoth Mesh Machine — ML Inference & Training Hub",
    version="0.1.0",
    lifespan=lifespan,
)

# Allow the Shoggoth Dashboard (Tauri on localhost) to connect.
app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:1420", "http://localhost:9101"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# ── Routers ─────────────────────────────────────────────────────────────────────

app.include_router(shoggoth_fabric.router, prefix="/fabric", tags=["Fabric"])
app.include_router(genomic_training.router, prefix="/training", tags=["Training"])


@app.get("/health")
async def health_check():
    """Liveness probe for orchestrator health monitoring."""
    return {"status": "ok", "service": "npu-stack", "version": "0.1.0"}


# ── Entry Point ─────────────────────────────────────────────────────────────────

if __name__ == "__main__":
    uvicorn.run(
        "main:app",
        host="0.0.0.0",
        port=8100,
        reload=True,
        log_level="info",
    )
