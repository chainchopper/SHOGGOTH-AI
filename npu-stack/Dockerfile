# NPU-STACK Dockerfile
# Shoggoth Machine Learning Inference & Training Hub
#
# Build:
#   docker build -t ghcr.io/chainchopper/npu-stack:latest .
# Run:
#   docker run --network host -v ./checkpoints:/app/checkpoints ghcr.io/chainchopper/npu-stack:latest

FROM python:3.12-slim AS base

LABEL org.opencontainers.image.title="NPU-STACK"
LABEL org.opencontainers.image.description="Shoggoth Mesh Machine — ML Inference & Training Hub"
LABEL org.opencontainers.image.source="https://github.com/chainchopper/npu-stack"
LABEL org.opencontainers.image.version="0.1.0"

# ── System Dependencies ────────────────────────────────────────────────────────
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

# ── Application ────────────────────────────────────────────────────────────────
WORKDIR /app

# Install Python dependencies.
COPY pyproject.toml .
RUN pip install --no-cache-dir -e ".[dev]" && \
    pip install --no-cache-dir uvicorn[standard]

# Copy application code.
COPY backend/ backend/
COPY kernels/ kernels/
COPY infrastructure/ infrastructure/

# Make the backup script executable.
RUN chmod +x infrastructure/backup_scylla.sh

# ── Runtime ────────────────────────────────────────────────────────────────────
ENV PYTHONUNBUFFERED=1
ENV SHOGGOTH_ORCHESTRATOR_ADDR=localhost:9100

EXPOSE 8100

# Use uvicorn with production settings.
CMD ["uvicorn", "backend.main:app", "--host", "0.0.0.0", "--port", "8100", "--workers", "4", "--log-level", "info"]

# ── Development Stage ──────────────────────────────────────────────────────────
FROM base AS dev

RUN pip install --no-cache-dir \
    pytest>=8.0 \
    pytest-asyncio>=0.25 \
    black>=24.0 \
    ruff>=0.8 \
    mypy>=1.14

CMD ["uvicorn", "backend.main:app", "--host", "0.0.0.0", "--port", "8100", "--reload"]
