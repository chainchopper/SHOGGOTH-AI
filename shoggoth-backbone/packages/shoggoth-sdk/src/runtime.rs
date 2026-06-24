// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/runtime.rs — Asynchronous multi-tier runtime engine.
//
// When an application uses the Shoggoth SDK natively, its frame/tick loop is
// split into independent asynchronous tasks dispatched across edge (local)
// and cloud (remote) infrastructure based on latency tolerance.
//
//   • Edge Component: Zero-latency tasks — player input, UI layout, local
//     network prediction, frame buffer delivery.
//   • Cloud Component: Latency-tolerant tasks — global illumination,
//     secondary ray bounces, background AI, large world state.

use std::time::Duration;

// ── Types ──────────────────────────────────────────────────────────────────────

/// Represents the complete state of a single application frame/tick.
#[derive(Debug, Clone)]
pub struct ApplicationFrameState {
    /// Monotonically increasing frame index.
    pub frame_index: u64,
    /// Player or camera position in world space.
    pub player_position: (f32, f32, f32),
    /// Computed lighting / tensor data for this frame.
    pub computed_lighting_data: Vec<u8>,
    /// Whether this frame was rendered with full quality (all cloud nodes responded)
    /// or degraded quality (timeout on cloud responses).
    pub quality_tier: FrameQualityTier,
}

/// Quality level for a rendered frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameQualityTier {
    /// Full quality: all edge and cloud nodes contributed.
    Full,
    /// Degraded: cloud timeout; only edge nodes contributed.
    EdgeOnly,
    /// Fallback: local CPU rendering only.
    Fallback,
}

// ── Runtime Engine ─────────────────────────────────────────────────────────────

/// The Shoggoth Asynchronous Runtime Engine.
///
/// Drives an application's frame loop by splitting each tick into:
///   1. Critical local tasks (edge, < 2ms latency budget).
///   2. Heavy compute tasks (cloud/BC250 grid, < 16ms latency budget).
///
/// The two task streams run concurrently. The engine synchronizes them at
/// each frame boundary before handing the combined state to the compositor.
#[derive(Debug)]
pub struct ShoggothRuntimeEngine {
    /// Current frame counter.
    pub current_frame: u64,
    /// Maximum allowed cloud task latency before falling back to edge-only quality.
    pub cloud_timeout_ms: u64,
}

impl ShoggothRuntimeEngine {
    /// Creates a new runtime engine starting at frame 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_frame: 0,
            cloud_timeout_ms: 16,
        }
    }

    /// Sets the cloud task timeout. If cloud nodes don't respond within this
    /// window, the frame degrades to edge-only quality.
    pub fn with_cloud_timeout(mut self, timeout_ms: u64) -> Self {
        self.cloud_timeout_ms = timeout_ms;
        self
    }

    /// Executes a single frame tick, splitting work across edge and cloud.
    ///
    /// # How It Works
    ///
    /// ```text
    /// ┌─────────────────────────────────────────────────────┐
    /// │                 SHOGGOTH FRAME TICK                 │
    /// │                                                     │
    /// │  ┌──────────────┐       ┌──────────────────────┐   │
    /// │  │ EDGE (local) │       │ CLOUD (remote/grid)  │   │
    /// │  │ < 2ms budget │       │ < 16ms budget        │   │
    /// │  │              │       │                      │   │
    /// │  │ • Input poll │       │ • Global illumination│   │
    /// │  │ • UI update  │       │ • Path tracing       │   │
    /// │  │ • Prediction │       │ • AI inference       │   │
    /// │  │ • Submit     │       │ • Asset streaming    │   │
    /// │  └──────┬───────┘       └──────────┬───────────┘   │
    /// │         │                          │               │
    /// │         └──────────┬───────────────┘               │
    /// │                    ▼                               │
    /// │           Frame Synchronization                    │
    /// │                    │                               │
    /// │                    ▼                               │
    /// │         Display Compositor                         │
    /// └─────────────────────────────────────────────────────┘
    /// ```
    ///
    /// In production, the edge task runs on the local Xeon + RTX 5090 while
    /// the cloud task is sharded across BC250 nodes and Brev.dev instances.
    pub async fn execute_frame(&mut self) -> ApplicationFrameState {
        self.current_frame += 1;
        let frame = self.current_frame;

        tracing::debug!(frame, "Shoggoth runtime: executing frame tick");

        // ── Task 1: Critical Local Loop (Edge) ──
        // Must complete within 2ms. Runs on Xeon cores + RTX 5090.
        let local_future = tokio::task::spawn(async move {
            // In production: poll input devices, update UI state,
            // run network prediction for client-side rollback.
            tokio::time::sleep(Duration::from_millis(2)).await;
            (10.5f32, 2.0f32, -4.1f32) // Mock updated position
        });

        // ── Task 2: Heavy Compute Loop (Cloud / Distributed) ──
        // Tolerates up to cloud_timeout_ms. Sharded across BC250 grid + cloud.
        let cloud_future = tokio::task::spawn(async move {
            // In production: dispatch global illumination rays to RT cores
            // on cloud nodes, run AI inference on BC250 grid.
            tokio::time::sleep(Duration::from_millis(12)).await;
            vec![0xFFu8; 512] // Mock lighting data
        });

        // ── Synchronize ──
        let player_pos = local_future.await.expect("Edge task panicked");

        let (lighting_data, quality) = match tokio::time::timeout(
            Duration::from_millis(self.cloud_timeout_ms),
            cloud_future,
        )
        .await
        {
            Ok(Ok(data)) => (data, FrameQualityTier::Full),
            Ok(Err(e)) => {
                tracing::error!("Cloud task panicked: {e}");
                (vec![0; 512], FrameQualityTier::Fallback)
            }
            Err(_elapsed) => {
                tracing::warn!(
                    frame,
                    timeout_ms = self.cloud_timeout_ms,
                    "Cloud task timed out; degrading to edge-only quality"
                );
                (vec![0; 512], FrameQualityTier::EdgeOnly)
            }
        };

        let state = ApplicationFrameState {
            frame_index: frame,
            player_position: player_pos,
            computed_lighting_data: lighting_data,
            quality_tier: quality,
        };

        // Hand off to the display compositor.
        self.dispatch_to_compositor(&state);

        state
    }

    /// Hands the compiled frame state to the display compositor for encoding
    /// and streaming to the client viewport.
    fn dispatch_to_compositor(&self, _state: &ApplicationFrameState) {
        // In production: write to a lock-free ring buffer consumed by
        // shoggoth-display/src/compositor.rs → WebRTC encoder → client.
        tracing::trace!(
            frame = _state.frame_index,
            quality = ?_state.quality_tier,
            "Frame dispatched to compositor"
        );
    }
}

impl Default for ShoggothRuntimeEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_single_frame_execution() {
        let mut engine = ShoggothRuntimeEngine::new();
        let state = engine.execute_frame().await;
        assert_eq!(state.frame_index, 1);
        assert_eq!(state.player_position, (10.5, 2.0, -4.1));
    }

    #[tokio::test]
    async fn test_frame_counter_increments() {
        let mut engine = ShoggothRuntimeEngine::new();
        let s1 = engine.execute_frame().await;
        let s2 = engine.execute_frame().await;
        assert_eq!(s1.frame_index, 1);
        assert_eq!(s2.frame_index, 2);
    }

    #[tokio::test]
    async fn test_cloud_timeout_triggers_degradation() {
        let mut engine = ShoggothRuntimeEngine::new().with_cloud_timeout(0); // Immediate timeout
        let state = engine.execute_frame().await;
        assert_eq!(state.quality_tier, FrameQualityTier::EdgeOnly);
    }
}
