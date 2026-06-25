// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// Shoggoth SDK Quickstart — demonstrates the full pipeline end-to-end.
//
// Build:   cargo run --example quickstart
// Requires: No hardware — runs entirely on CPU.
//
// Flow:
//   1. Bootstrap the hardware fabric (discovers local GPUs via wgpu).
//   2. Build the lab topology catalog.
//   3. Run a real CPU SGEMM benchmark.
//   4. Create a runtime engine with broadcast → compositor pipeline.
//   5. Execute 5 frames of real compute dispatch.
//   6. Demonstrate the network shading delta optimizer.
//   7. Demonstrate the WebRTC signaling server.

use shoggoth_core::compute_fabric::{ComputeTaskTensor, execute_local_fallback};
use shoggoth_display::network_shading::DeltaOptimizer;
use shoggoth_display::webrtc_signaling::SignalingServer;
use shoggoth_sdk::runtime::ShoggothRuntimeEngine;
use shoggoth_sdk::topology;

use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║     SHOGGOTH SDK QUICKSTART — Full Pipeline Demo        ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    // ── 1. Hardware Fabric Bootstrap ───────────────────────────────────────
    println!("[1/5] Bootstrapping hardware fabric...");
    let topology_fabric = shoggoth_core::bootstrap_hardware_fabric().await;
    println!(
        "  Found {} GPU device(s), {:.1} GB total VRAM",
        topology_fabric.nodes.len(),
        topology_fabric.total_vram_gb(),
    );
    for node in &topology_fabric.nodes {
        println!("  └─ {:?} — {} ({} GB)", node.hardware_type, node.name, node.vram_bytes / (1024 * 1024 * 1024));
    }

    // ── 2. Lab Topology ────────────────────────────────────────────────────
    println!();
    println!("[2/5] Building lab topology catalog...");
    let pool = topology::build_lab_topology();
    println!("  {} nodes, {:.1} GB total VRAM", pool.active_nodes.len(), pool.total_vram_gb());
    println!("  Full Shoggoths: {}", pool.full_shoggoth_nodes().len());

    // ── 3. CPU SGEMM Benchmark ─────────────────────────────────────────────
    println!();
    println!("[3/5] Real CPU SGEMM benchmark...");
    let size = 256usize;
    let mut a = vec![0.0f32; size * size];
    for i in 0..size { a[i * size + i] = 1.0; }

    let tensor = ComputeTaskTensor {
        task_id: 1,
        shape: vec![size as i64, size as i64, size as i64],
        flat_data: a,
    };

    let start = Instant::now();
    let result = execute_local_fallback(&tensor);
    let elapsed = start.elapsed();

    let flops = 2.0 * (size as f64).powi(3);
    let gflops = flops / elapsed.as_secs_f64() / 1e9;
    println!("  {size}×{size} SGEMM: {gflops:.2} GFLOPS ({elapsed:.2?})");
    println!("  Result shape: {:?}, non-zero values: {}",
        result.shape,
        result.flat_data.iter().filter(|&&v| v.abs() > 0.001).count(),
    );

    // ── 4. Runtime Engine + Compositor Pipeline ────────────────────────────
    println!();
    println!("[4/5] Runtime engine with broadcast compositor pipeline...");
    let mut engine = ShoggothRuntimeEngine::with_capacity(128);
    let mut rx = engine.frame_receiver();

    // Spawn compositor subscriber.
    let compositor_handle = tokio::spawn(async move {
        let mut frames = 0u64;
        while let Ok(state) = rx.recv().await {
            frames += 1;
            println!(
                "  Compositor received frame {}: quality={:?}, pos=({:.1},{:.1},{:.1}), data={} bytes",
                state.frame_index,
                state.quality_tier,
                state.player_position.0,
                state.player_position.1,
                state.player_position.2,
                state.computed_data.len(),
            );
            if frames >= 5 {
                break;
            }
        }
        frames
    });

    // Execute 5 frames.
    for _ in 0..5 {
        engine.execute_frame().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let total_frames = compositor_handle.await?;
    println!("  Pipeline completed: {total_frames} frames delivered to compositor.");

    // ── 5. Network Shading + Signaling ─────────────────────────────────────
    println!();
    println!("[5/5] Network shading + WebRTC signaling...");

    // Delta optimizer: demonstrate frame skipping.
    let mut optimizer = DeltaOptimizer::new();
    let buffer = vec![0u8; 1024];
    let tx = optimizer.optimize_traffic(0, 0xAAAA, &buffer, &buffer);
    assert!(tx.is_some(), "First frame should transmit");
    let skip = optimizer.optimize_traffic(0, 0xAAAA, &buffer, &buffer);
    assert!(skip.is_none(), "Identical frame should be skipped");
    println!("  Delta optimizer: skip ratio = {:.1}%", optimizer.skip_ratio() * 100.0);

    // Signaling server: register clients and relay messages.
    let server = SignalingServer::new();
    let _client_rx = server.register_client("iphone-15");
    let _dashboard_rx = server.register_client("dashboard");
    assert_eq!(server.client_count(), 2);

    let sent = server.send_to_client("iphone-15", &shoggoth_display::webrtc_signaling::SignalingMessage::Ack {
        message: "Ready for SDP offer".into(),
    });
    assert!(sent);
    println!("  Signaling: {} clients, message relay OK", server.client_count());

    // ── Done ───────────────────────────────────────────────────────────────
    println!();
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║  QUICKSTART COMPLETE — All systems operational          ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();
    println!("Next steps:");
    println!("  cargo run -p shoggoth-orchestrator     # Start the control plane");
    println!("  cargo run -p shoggoth-cli -- topology  # Query live topology");
    println!("  cargo test --workspace                 # Run all tests");

    Ok(())
}
