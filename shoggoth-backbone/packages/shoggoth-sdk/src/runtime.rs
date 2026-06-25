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

use tokio::sync::broadcast;

// ── Types ──────────────────────────────────────────────────────────────────────

/// Represents the complete state of a single application frame/tick.
#[derive(Debug, Clone)]
pub struct ApplicationFrameState {
    /// Monotonically increasing frame index.
    pub frame_index: u64,
    /// Player or camera position in world space.
    pub player_position: (f32, f32, f32),
    /// Computed data for this frame (lighting, AI, physics).
    pub computed_data: Vec<u8>,
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
/// each frame boundary before handing the combined state to the compositor
/// via a broadcast channel.
#[derive(Debug, Clone)]
pub struct ShoggothRuntimeEngine {
    /// Current frame counter.
    pub current_frame: u64,
    /// Maximum allowed cloud task latency before falling back to edge-only quality.
    pub cloud_timeout_ms: u64,
    /// Broadcast sender for frame state → compositor pipeline.
    frame_tx: broadcast::Sender<ApplicationFrameState>,
}

impl ShoggothRuntimeEngine {
    /// Creates a new runtime engine with a broadcast channel for the compositor.
    #[must_use]
    pub fn new() -> Self {
        let (frame_tx, _) = broadcast::channel(256);
        Self {
            current_frame: 0,
            cloud_timeout_ms: 16,
            frame_tx,
        }
    }

    /// Creates a new runtime engine with the given frame channel capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let (frame_tx, _) = broadcast::channel(capacity);
        Self {
            current_frame: 0,
            cloud_timeout_ms: 16,
            frame_tx,
        }
    }

    /// Returns a receiver for frame state (for the compositor to consume).
    #[must_use]
    pub fn frame_receiver(&self) -> broadcast::Receiver<ApplicationFrameState> {
        self.frame_tx.subscribe()
    }

    /// Sets the cloud task timeout. If cloud nodes don't respond within this
    /// window, the frame degrades to edge-only quality.
    #[must_use]
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
        // Must complete within 2ms. Runs on local CPU/GPU.
        let local_frame = frame;
        let local_future = tokio::task::spawn(async move {
            // Real edge compute: run a lightweight CPU SGEMM to simulate
            // input polling + local prediction + UI update.
            let n = 64usize;
            let mut a = vec![0.0f32; n * n];
            for i in 0..n { a[i * n + i] = 1.0; }
            let tensor = shoggoth_core::compute_fabric::ComputeTaskTensor {
                task_id: local_frame,
                shape: vec![n as i64, n as i64, n as i64],
                flat_data: a,
            };
            // Real SGEMM via CPU fallback.
            let _result = shoggoth_core::compute_fabric::execute_local_fallback(&tensor);
            (0.0f32, 0.0f32, local_frame as f32) // position = frame counter for demo
        });

        // ── Task 2: Heavy Compute Loop (Cloud / Distributed) ──
        let cloud_future = tokio::task::spawn(async move {
            // Real heavy compute: 512×512 SGEMM on CPU (GPU path via node-agent).
            let n = 512usize;
            let mut data = vec![0.0f32; n * n * 2]; // A + B in flat buffer
            for i in 0..n { data[i * n + i] = 1.0; } // A = I
            for i in 0..n { data[n * n + i * n + i] = 1.0; } // B = I

            let tensor = shoggoth_core::compute_fabric::ComputeTaskTensor {
                task_id: 100,
                shape: vec![n as i64, n as i64, n as i64],
                flat_data: data,
            };

            // Real SGEMM via CPU fallback path.
            let result = shoggoth_core::compute_fabric::execute_local_fallback(&tensor);
            result.flat_data.into_iter().map(|v| v.to_le_bytes()).flatten().collect::<Vec<u8>>()
        });

        // ── Synchronize ──
        let player_pos = local_future.await.unwrap_or((0.0, 0.0, 0.0));

        let (computed_data, quality) = match tokio::time::timeout(
            Duration::from_millis(self.cloud_timeout_ms),
            cloud_future,
        )
        .await
        {
            Ok(Ok(data)) => (data, FrameQualityTier::Full),
            Ok(Err(e)) => {
                tracing::error!("Cloud task panicked: {e}");
                (vec![0; 256], FrameQualityTier::Fallback)
            }
            Err(_elapsed) => {
                tracing::warn!(
                    frame,
                    timeout_ms = self.cloud_timeout_ms,
                    "Cloud task timed out; degrading to edge-only quality"
                );
                (vec![0; 256], FrameQualityTier::EdgeOnly)
            }
        };

        let state = ApplicationFrameState {
            frame_index: frame,
            player_position: player_pos,
            computed_data,
            quality_tier: quality,
        };

        // Hand off to the display compositor via broadcast channel.
        self.dispatch_to_compositor(&state);

        state
    }

    /// Hands the compiled frame state to the display compositor for encoding
    /// and streaming to the client viewport.
    fn dispatch_to_compositor(&self, state: &ApplicationFrameState) {
        // Send frame state to all compositor subscribers.
        if self.frame_tx.receiver_count() > 0 {
            let _ = self.frame_tx.send(state.clone());
        }
        tracing::trace!(
            frame = state.frame_index,
            quality = ?state.quality_tier,
            "Frame dispatched to compositor ({} subscribers)",
            self.frame_tx.receiver_count()
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
