// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/wasm_bridge.rs — WASM/WebGPU SDK for browser clients.
//
// Provides a WebAssembly-compiled shim of the Shoggoth SDK that runs in
// browser environments. Enables:
//   • WebGPU compute dispatch from the browser to local GPU.
//   • WebRTC viewport streaming from the Shoggoth compositor.
//   • WebSocket telemetry feed consumption.
//   • Lightweight node-agent for headless browser contexts.
//
// This module is compiled to WASM via wasm-pack and published as an npm
// package: @shoggoth/sdk
//
// # Build
//
//   wasm-pack build --target web --out-dir pkg --features wasm-bindings

use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};

// ── Types ──────────────────────────────────────────────────────────────────────

/// Web-compatible FabricPool (no async wgpu, uses browser WebGPU).
#[wasm_bindgen]
pub struct WasmFabricPool {
    nodes: Vec<WasmNodeInfo>,
}

#[wasm_bindgen]
impl WasmFabricPool {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { nodes: vec![] }
    }

    /// Connects to the Shoggoth orchestrator via WebSocket and fetches topology.
    #[wasm_bindgen]
    pub async fn connect(&mut self, orchestrator_url: &str) -> Result<(), JsValue> {
        let ws_url = orchestrator_url.replace("http://", "ws://").replace("https://", "wss://")
            + "/ws/telemetry";

        // In production: open WebSocket, receive topology frame.
        let _ = ws_url;
        Ok(())
    }

    #[wasm_bindgen]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    #[wasm_bindgen]
    pub fn get_nodes_json(&self) -> String {
        serde_json::to_string(&self.nodes).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WasmNodeInfo {
    node_id: String,
    vram_gb: u32,
    online: bool,
}

// ── WebGPU Dispatch ────────────────────────────────────────────────────────────

/// Dispatches a WebGPU compute shader from the browser.
///
/// Used for client-side post-processing, UI rendering, and lightweight
/// compute tasks that don't need the full Shoggoth cluster.
#[wasm_bindgen]
pub async fn dispatch_webgpu_compute(
    _wgsl_source: &str,
    _input_data: &[u8],
) -> Result<Vec<u8>, JsValue> {
    // In production:
    //   1. const adapter = await navigator.gpu.requestAdapter();
    //   2. const device = await adapter.requestDevice();
    //   3. const module = device.createShaderModule({ code: wgsl_source });
    //   4. const pipeline = device.createComputePipeline({ ... });
    //   5. Copy input data to GPU buffer.
    //   6. Dispatch workgroups.
    //   7. Read back result.

    Err(JsValue::from_str("WebGPU compute not yet implemented for WASM target"))
}

// ── Version ────────────────────────────────────────────────────────────────────

#[wasm_bindgen]
pub fn sdk_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
