"""Shoggoth Python Client Library.

Thin, async-first Python wrapper around the Shoggoth orchestrator REST API.
Provides type-safe access to topology, workload analysis, template launching,
and telemetry streaming.

Usage:
    from shoggoth_client import ShoggothClient

    async with ShoggothClient("http://localhost:9100") as client:
        health = await client.health()
        topology = await client.get_topology()
        decision = await client.analyze("import torch.nn as nn")
        await client.launch("render-farm", "my-blender-project")

Requirements:
    pip install httpx websockets
"""

from __future__ import annotations

import asyncio
import json
import logging
from dataclasses import dataclass, field
from typing import Optional

import httpx
import websockets

logger = logging.getLogger(__name__)

# ── Data Models ────────────────────────────────────────────────────────────────


@dataclass
class NodeInfo:
    node_id: str
    tier: str  # EdgeOnPrem | CloudScale
    vram_gb: int
    ping_ms: float
    accepting_work: bool
    temperature_c: float


@dataclass
class TopologySnapshot:
    total_nodes: int
    total_vram_gb: float
    full_shoggoths: int
    nodes: list[NodeInfo]
    uptime_seconds: int


@dataclass
class AnalysisResult:
    workload: str
    target_node: str
    reason: str
    suggested_template: str
    template_manifest: str
    confidence: float


@dataclass
class LaunchResult:
    status: str
    template: str
    manifest: str
    message: str


@dataclass
class TelemetryFrame:
    seq: int
    timestamp_secs: float
    nodes: list[dict]
    aggregate: dict

# ── Client ─────────────────────────────────────────────────────────────────────


class ShoggothClient:
    """Async client for the Shoggoth orchestrator REST API."""

    def __init__(self, base_url: str = "http://localhost:9100", timeout: float = 30.0):
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self._client: Optional[httpx.AsyncClient] = None

    async def __aenter__(self) -> "ShoggothClient":
        self._client = httpx.AsyncClient(
            base_url=self.base_url,
            timeout=self.timeout,
            headers={"Content-Type": "application/json"},
        )
        return self

    async def __aexit__(self, *args):
        if self._client:
            await self._client.aclose()

    @property
    def client(self) -> httpx.AsyncClient:
        if self._client is None:
            raise RuntimeError("Client not opened. Use 'async with ShoggothClient()'")
        return self._client

    # ── Health ─────────────────────────────────────────────────────────────────

    async def health(self) -> dict:
        """GET /health — orchestrator liveness check."""
        r = await self.client.get("/health")
        r.raise_for_status()
        return r.json()

    # ── Topology ───────────────────────────────────────────────────────────────

    async def get_topology(self) -> TopologySnapshot:
        """GET /topology — full hardware fabric snapshot."""
        r = await self.client.get("/topology")
        r.raise_for_status()
        data = r.json()
        return TopologySnapshot(
            total_nodes=data["total_nodes"],
            total_vram_gb=data["total_vram_gb"],
            full_shoggoths=data["full_shoggoths"],
            nodes=[NodeInfo(**n) for n in data["nodes"]],
            uptime_seconds=data["uptime_seconds"],
        )

    async def list_nodes(self, capability: Optional[str] = None) -> list[NodeInfo]:
        """GET /fabric/nodes — list nodes, optionally filtered by capability."""
        params = {}
        if capability:
            params["capability"] = capability
        r = await self.client.get("/fabric/nodes", params=params)
        r.raise_for_status()
        return [NodeInfo(**n) for n in r.json()["nodes"]]

    # ── Analysis ───────────────────────────────────────────────────────────────

    async def analyze(self, source_code: str, project_name: str = "") -> AnalysisResult:
        """POST /analyze — classify workload and get hardware routing."""
        r = await self.client.post(
            "/analyze",
            json={"source_code": source_code, "project_name": project_name},
        )
        r.raise_for_status()
        data = r.json()
        return AnalysisResult(
            workload=data["workload"],
            target_node=data["target_node"],
            reason=data["reason"],
            suggested_template=data["suggested_template"],
            template_manifest=data["template_manifest"],
            confidence=data["confidence"],
        )

    # ── Launch ─────────────────────────────────────────────────────────────────

    async def launch(self, template_name: str, project_name: str = "") -> LaunchResult:
        """POST /launch — deploy a pre-configured workflow template.

        Valid templates: render-farm, heavy-compute, async-game-runtime,
                         genomic-processing, generic
        """
        r = await self.client.post(
            "/launch",
            json={"template_name": template_name, "project_name": project_name},
        )
        r.raise_for_status()
        data = r.json()
        return LaunchResult(
            status=data.get("status", "unknown"),
            template=data.get("template", template_name),
            manifest=data.get("manifest", ""),
            message=data.get("message", ""),
        )

    # ── Node Registration ──────────────────────────────────────────────────────

    async def register_node(self, node: dict) -> dict:
        """POST /fabric/register — manually register a node."""
        r = await self.client.post("/fabric/register", json=node)
        r.raise_for_status()
        return r.json()


# ── Telemetry Stream ───────────────────────────────────────────────────────────


class TelemetryStream:
    """Async iterator over live Shoggoth telemetry via WebSocket."""

    def __init__(self, base_url: str = "ws://localhost:9101"):
        self.url = f"{base_url.rstrip('/')}/ws/telemetry"

    async def __aiter__(self):
        async for ws in websockets.connect(self.url):
            try:
                async for message in ws:
                    try:
                        data = json.loads(message)
                        yield TelemetryFrame(
                            seq=data.get("seq", 0),
                            timestamp_secs=data.get("timestamp_secs", 0.0),
                            nodes=data.get("nodes", []),
                            aggregate=data.get("aggregate", {}),
                        )
                    except json.JSONDecodeError:
                        logger.warning("Malformed telemetry frame: %s", message[:100])
            except websockets.ConnectionClosed:
                logger.info("Telemetry WebSocket closed; reconnecting...")
                await asyncio.sleep(1)


# ── Convenience Functions ──────────────────────────────────────────────────────


async def quick_analyze(source_code: str, orchestrator_url: str = "http://localhost:9100") -> AnalysisResult:
    """One-shot analysis without managing a client context."""
    async with ShoggothClient(orchestrator_url) as client:
        return await client.analyze(source_code)


async def quick_topology(orchestrator_url: str = "http://localhost:9100") -> TopologySnapshot:
    """One-shot topology fetch without managing a client context."""
    async with ShoggothClient(orchestrator_url) as client:
        return await client.get_topology()


# ── Tests ──────────────────────────────────────────────────────────────────────

if __name__ == "__main__":
    async def _demo():
        async with ShoggothClient() as client:
            # Health check
            health = await client.health()
            print(f"Orchestrator: {health}")

            # Topology
            topo = await client.get_topology()
            print(f"Nodes: {topo.total_nodes}, VRAM: {topo.total_vram_gb:.0f} GB")

            # Analyze
            result = await client.analyze("import torch.nn as nn; model = nn.Linear(20, 20).cuda()")
            print(f"Workload: {result.workload} → {result.target_node} ({result.confidence:.0%} confidence)")
            print(f"Template manifest:\n{result.template_manifest[:200]}...")

    asyncio.run(_demo())
