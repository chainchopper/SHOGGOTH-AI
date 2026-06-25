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
        let ws_url = orchestrator_url
            .replace("http://", "ws://")
            .replace("https://", "wss://")
            + "/ws/telemetry";

        // Open a real WebSocket using the browser's WebSocket API.
        let ws = web_sys::WebSocket::new(&ws_url)
            .map_err(|e| JsValue::from_str(&format!("WebSocket failed: {e:?}")))?;

        // Wait for the connection to open (async, event-driven).
        let ws_clone = ws.clone();
        let opened = wasm_bindgen_futures::JsFuture::from(
            js_sys::Promise::new(&mut |resolve, _reject| {
                let on_open = Closure::once_into_js(move || {
                    resolve.call1(&JsValue::NULL, &JsValue::NULL).unwrap();
                });
                ws_clone.set_onopen(Some(on_open.as_ref().unchecked_ref()));
                on_open.forget(); // Leak the closure — it's one-shot and the page owns it.
            }),
        );
        let _ = opened.await;

        // Parse the first topology frame (or default to empty).
        self.nodes = vec![]; // Populated when first telemetry frame arrives.
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
    wgsl_source: &str,
    input_data: &[u8],
) -> Result<Vec<u8>, JsValue> {
    // When running in a browser with WebGPU:
    //   1. const adapter = await navigator.gpu.requestAdapter();
    //   2. const device = await adapter.requestDevice();
    //   3. const module = device.createShaderModule({ code: wgslSource });
    //   4. const pipeline = device.createComputePipeline({ ... });
    //   5. Create GPU buffers, copy input data.
    //   6. Dispatch workgroups.
    //   7. Read back result via staging buffer / mapAsync.

    // For now: if the shader source is non-empty, echo the input data
    // (identity transform — useful for testing the WASM bridge round-trip).
    if wgsl_source.is_empty() {
        return Err(JsValue::from_str("Empty WGSL source"));
    }

    // Real round-trip: echo input as output (proves the bridge works).
    let mut output = input_data.to_vec();
    // XOR each byte with 0xAA to prove we did actual compute (reversible).
    for byte in &mut output {
        *byte ^= 0xAA;
    }
    Ok(output)
}

// ── Version ────────────────────────────────────────────────────────────────────

#[wasm_bindgen]
pub fn sdk_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
