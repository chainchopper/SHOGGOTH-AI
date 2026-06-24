# Shoggoth Mesh Machine — Integration Tests

Runs cross-crate integration tests that verify the full Shoggoth stack works
end-to-end without requiring actual GPU hardware.

## Prerequisites

```bash
cargo test --workspace -- --include-ignored
```

## Test Categories

| Suite | What It Verifies |
|-------|-----------------|
| `orchestrator_api` | REST API endpoints respond correctly (health, topology, analyze, launch) |
| `node_discovery` | Heartbeat parsing, capability classification, liveness tracking |
| `quic_transport` | Certificate generation, message serialization round-trip |
| `workload_routing` | Agentic parser classifies 15+ code patterns correctly |
| `template_generation` | All 5 SDK templates produce valid TOML |
| `telemetry_frames` | Telemetry frame building and serialization |
| `encoder_factory` | Encoder auto-detection and config validation |
| `sync_chain` | Multi-node barrier synchronization |
| `qat_compress` | Compression/decompression round-trips for all algorithms |

## Running Specific Suites

```bash
cargo test -p shoggoth-core   -- integration
cargo test -p shoggoth-sdk    -- integration
cargo test -p shoggoth-agent  -- integration
cargo test -p shoggoth-display -- integration
```
