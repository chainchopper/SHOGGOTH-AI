"""Shoggoth Fabric Router — Live telemetry WebSockets, node splitting & registration.

Provides real-time WebSocket endpoints consumed by the Shoggoth Tauri Dashboard
and the orchestrator's telemetry feed. Handles:

    • /ws/telemetry — streaming push of node metrics (VRAM, temp, task queue depth).
    • /register — node heartbeat registration.
    • /topology — snapshot of the current hardware pool.
"""

import json
import logging
import time
from typing import Optional

from fastapi import APIRouter, WebSocket, WebSocketDisconnect

logger = logging.getLogger(__name__)

router = APIRouter()

# ── In-Memory State ─────────────────────────────────────────────────────────────

# Connected WebSocket clients (dashboard instances).
_connected_clients: list[WebSocket] = []

# Current topology snapshot (populated by the Rust orchestrator via REST).
_topology_snapshot: dict = {
    "nodes": [],
    "total_nodes": 0,
    "total_vram_gb": 0.0,
    "updated_at": 0.0,
}

# ── WebSocket Telemetry ─────────────────────────────────────────────────────────


@router.websocket("/ws/telemetry")
async def telemetry_websocket(websocket: WebSocket):
    """Streams live fabric telemetry to connected dashboard clients.

    The Shoggoth orchestrator pushes node metrics (VRAM usage, temperature,
    task queue depth, network RTT) over this WebSocket at ~10 Hz.
    """
    await websocket.accept()
    _connected_clients.append(websocket)
    logger.info("Dashboard client connected (%d total)", len(_connected_clients))

    try:
        while True:
            # Wait for the orchestrator to push updates.
            # In production, this receives from a tokio::broadcast channel
            # that the Rust orchestrator writes to.
            raw = await websocket.receive_text()
            try:
                message = json.loads(raw)
                logger.debug("Telemetry update: %s", message.get("type", "unknown"))
            except json.JSONDecodeError:
                logger.warning("Malformed telemetry message received")
    except WebSocketDisconnect:
        _connected_clients.remove(websocket)
        logger.info("Dashboard client disconnected (%d remaining)", len(_connected_clients))


@router.websocket("/ws/node/{node_id}")
async def node_websocket(websocket: WebSocket, node_id: str):
    """Per-node WebSocket for direct agent-to-orchestrator communication.

    Each node agent (BC250, MI50, Cloud instance) maintains one of these
    connections for control-plane commands and heartbeat acknowledgement.
    """
    await websocket.accept()
    logger.info("Node agent connected: %s", node_id)

    try:
        while True:
            raw = await websocket.receive_text()
            heartbeat = json.loads(raw)
            logger.debug(
                "Heartbeat from %s: vram=%dGB temp=%.1fC",
                node_id,
                heartbeat.get("available_vram_gb", 0),
                heartbeat.get("temperature_c", 0.0),
            )
            # Echo acknowledgement.
            await websocket.send_text(json.dumps({"ack": True, "node_id": node_id}))
    except WebSocketDisconnect:
        logger.warning("Node agent disconnected: %s", node_id)


# ── REST Endpoints ──────────────────────────────────────────────────────────────


@router.get("/topology")
async def get_topology():
    """Returns the current hardware topology snapshot."""
    return {
        **_topology_snapshot,
        "updated_at_human": time.strftime(
            "%Y-%m-%dT%H:%M:%SZ", time.gmtime(_topology_snapshot.get("updated_at", 0))
        ),
    }


@router.get("/nodes")
async def list_nodes(capability: Optional[str] = None):
    """Lists all registered nodes, optionally filtered by capability."""
    nodes = _topology_snapshot.get("nodes", [])
    if capability:
        nodes = [n for n in nodes if capability in n.get("capabilities", [])]
    return {"nodes": nodes, "count": len(nodes)}


@router.post("/register")
async def register_node(node: dict):
    """Registers or updates a node in the fabric pool.

    Called by the Rust orchestrator when a node heartbeat is first received.
    """
    node_id = node.get("node_id", "unknown")
    logger.info("Node registered: %s (tier=%s, vram=%dGB)", node_id, node.get("tier"), node.get("available_vram_gb", 0))

    # In production: forward to the Rust orchestrator's DashMap.
    # For now, maintain in-memory.
    nodes = _topology_snapshot.get("nodes", [])
    nodes = [n for n in nodes if n.get("node_id") != node_id]
    nodes.append(node)
    _topology_snapshot["nodes"] = nodes
    _topology_snapshot["total_nodes"] = len(nodes)
    _topology_snapshot["total_vram_gb"] = sum(n.get("available_vram_gb", 0) for n in nodes)
    _topology_snapshot["updated_at"] = time.time()

    return {"status": "registered", "node_id": node_id, "total_nodes": len(nodes)}
