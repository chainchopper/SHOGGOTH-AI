// SPDX-License-Identifier: Apache-2.0
/// Integration tests for node discovery, topology, and telemetry.
///
/// Verifies heartbeat parsing, capability classification, fabric pool
/// operations, and telemetry frame serialization.

use shoggoth_sdk::discovery::NodeHeartbeat;
use shoggoth_sdk::telemetry::{build_telemetry_frame, TelemetryFrame};
use shoggoth_sdk::topology::{
    build_lab_topology, PhysicalResourceNode, ShoggothFabricPool,
    SpecializedCapability,
};

// ── Heartbeat Parsing ──────────────────────────────────────────────────────────

#[test]
fn test_heartbeat_json_round_trip() {
    let hb = NodeHeartbeat {
        node_id: "bc250-01".into(),
        protocol_version: 1,
        available_vram_bytes: 12 * 1024 * 1024 * 1024,
        temperature_c: 48.5,
        utilization_pct: 12.3,
        queue_depth: 0,
        accepting_work: true,
        vendor: "AMD".into(),
        kernel_version: "Linux 6.8.0".into(),
    };

    let json = serde_json::to_string(&hb).unwrap();
    let parsed: NodeHeartbeat = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.node_id, "bc250-01");
    assert_eq!(parsed.available_vram_bytes, 12 * 1024 * 1024 * 1024);
    assert!((parsed.temperature_c - 48.5).abs() < f32::EPSILON);
}

// ── Fabric Pool Operations ─────────────────────────────────────────────────────

#[test]
fn test_fabric_pool_register_and_query() {
    let mut pool = ShoggothFabricPool::new();

    pool.discover_and_register_node(PhysicalResourceNode {
        node_id: "test-5090".into(),
        tier: shoggoth_sdk::topology::InfrastructureTier::EdgeOnPrem,
        capabilities: vec![
            SpecializedCapability::HardwareRayTracing,
            SpecializedCapability::MatrixTensorCore,
        ],
        available_vram_gb: 32,
        network_ping_ms: 0.3,
        accepting_work: true,
        temperature_c: 52.0,
    });

    assert_eq!(pool.active_nodes.len(), 1);

    let rt_nodes = pool.request_pooled_resources(SpecializedCapability::HardwareRayTracing);
    assert_eq!(rt_nodes.len(), 1);
    assert_eq!(rt_nodes[0].node_id, "test-5090");
}

#[test]
fn test_fabric_pool_deregister() {
    let mut pool = ShoggothFabricPool::new();
    pool.discover_and_register_node(PhysicalResourceNode {
        node_id: "ephemeral".into(),
        tier: shoggoth_sdk::topology::InfrastructureTier::EdgeOnPrem,
        capabilities: vec![],
        available_vram_gb: 8,
        network_ping_ms: 1.0,
        accepting_work: true,
        temperature_c: 45.0,
    });

    let removed = pool.deregister_node("ephemeral");
    assert!(removed.is_some());
    assert!(pool.active_nodes.is_empty());
}

#[test]
fn test_lab_topology_counts() {
    let pool = build_lab_topology();
    assert_eq!(pool.active_nodes.len(), 19); // 1 Xeon + 5 NVIDIA + 2 MI50 + 1 V620 + 12 BC250

    let rt = pool.request_pooled_resources(SpecializedCapability::HardwareRayTracing);
    assert_eq!(rt.len(), 3); // 5090, 4090, 3090

    let apu = pool.request_pooled_resources(SpecializedCapability::MassiveUnifiedAPU);
    assert_eq!(apu.len(), 12); // All 12 BC250s
}

// ── Full Shoggoth Certification ────────────────────────────────────────────────

#[test]
fn test_full_shoggoth_certification() {
    let full = PhysicalResourceNode {
        node_id: "test-full".into(),
        tier: shoggoth_sdk::topology::InfrastructureTier::EdgeOnPrem,
        capabilities: vec![],
        available_vram_gb: 48,
        network_ping_ms: 2.0,
        accepting_work: true,
        temperature_c: 50.0,
    };
    assert!(full.is_full_shoggoth());
    assert!(full.is_shoggoth_limb());
}

#[test]
fn test_limb_certification() {
    let limb = PhysicalResourceNode {
        node_id: "test-limb".into(),
        tier: shoggoth_sdk::topology::InfrastructureTier::EdgeOnPrem,
        capabilities: vec![],
        available_vram_gb: 12,
        network_ping_ms: 10.0,
        accepting_work: true,
        temperature_c: 50.0,
    };
    assert!(!limb.is_full_shoggoth());
    assert!(limb.is_shoggoth_limb());
}

// ── Telemetry Frame Integrity ──────────────────────────────────────────────────

#[test]
fn test_telemetry_frame_builds_from_pool() {
    let pool = build_lab_topology();
    let frame = build_telemetry_frame(&pool, 42, 5, 3600);

    assert_eq!(frame.seq, 42);
    assert_eq!(frame.nodes.len(), 19);
    assert_eq!(frame.aggregate.total_nodes, 19);
    assert_eq!(frame.aggregate.active_work_units, 5);
    assert_eq!(frame.aggregate.uptime_seconds, 3600);
    assert!(frame.aggregate.total_vram_gb > 0.0);
}

#[test]
fn test_telemetry_frame_json_serialization() {
    let pool = build_lab_topology();
    let frame = build_telemetry_frame(&pool, 0, 0, 0);
    let json = serde_json::to_string(&frame).unwrap();

    assert!(json.contains("\"seq\":0"));
    assert!(json.contains("\"total_nodes\":19"));
    assert!(json.contains("node_id"));
    assert!(json.contains("aggregate"));
}
