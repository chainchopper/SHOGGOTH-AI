// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/lib.rs — Public SDK for the Shoggoth Mesh Machine.
//
// The SDK provides the public-facing API surface that third-party applications
// (GENEx, NPU-STACK, Unity, Unreal, PyTorch) link against. It abstracts:
//   • Topology discovery and hardware pool management.
//   • QUIC-multiplexed node-agent communication.
//   • WSL2 Vsock proxy bridging (AF_HYPERV).
//   • Asynchronous runtime for split edge/cloud workloads.
//   • Cluster frame synchronization chain for tear-free multi-GPU rendering.
//   • Python bindings via PyO3 (optional feature flag).

pub mod cloud_provision;
pub mod auth;
pub mod discovery;
pub mod dx12_interop;
pub mod error_catalog;
pub mod metal_interop;
pub mod metrics;
pub mod oidc_auth;
pub mod p2p_gpu_direct;
pub mod quic_transport;
pub mod runtime;
pub mod sync_chain;
pub mod telemetry;
pub mod topology;
pub mod vulkan_layer;
pub mod vsock_bridge;
pub mod webhooks;

#[cfg(feature = "wasm-bindings")]
pub mod wasm_bridge;

// Re-export key types for convenience.
pub use runtime::ShoggothRuntimeEngine;
pub use sync_chain::{ShoggothSyncChain, TilePayload};
pub use topology::{
    InfrastructureTier, PhysicalResourceNode, ShoggothFabricPool, SpecializedCapability,
};

/// SDK semantic version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Protocol version for wire compatibility between orchestrator and node agents.
/// Must match across all Shoggoth components.
pub const PROTOCOL_VERSION: u16 = 1;

/// Default port for the orchestrator QUIC control plane.
pub const DEFAULT_ORCHESTRATOR_PORT: u16 = 9100;

/// Default port for the WebSocket telemetry feed consumed by the dashboard.
pub const DEFAULT_TELEMETRY_PORT: u16 = 9101;
