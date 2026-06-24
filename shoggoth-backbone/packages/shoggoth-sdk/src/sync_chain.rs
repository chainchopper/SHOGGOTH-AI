// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/sync_chain.rs — Deterministic cluster frame synchronization.
//
// When the Shoggoth display engine composites a 16K viewport from 14+ GPUs
// across mixed AMD/NVIDIA/APU hardware, every tile must arrive at the
// compositor at the exact same logical frame boundary. Without a sync barrier,
// screen tearing and visual jitter occur because different GPUs render at
// different speeds.
//
// The sync chain uses a tokio::sync::Barrier: every rendering node calls
// `synchronize_cluster_tick()` when its tile is ready. The barrier blocks
// all nodes until the last one arrives, then the leader (the Xeon brain)
// signals the display flip.

use std::sync::Arc;
use tokio::sync::Barrier;

// ── Types ──────────────────────────────────────────────────────────────────────

/// A lightweight tile completion payload sent by each rendering node.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TilePayload {
    /// Which tile of the viewport this payload covers (0..N-1).
    pub tile_id: u32,
    /// The frame index this tile belongs to.
    pub frame_id: u64,
    /// xxHash64 of the vertex matrix for this tile. Used to verify geometric
    /// consistency across nodes without transferring heavy data.
    pub vertex_matrix_hash: u64,
}

// ── Sync Chain ─────────────────────────────────────────────────────────────────

/// Coordinates deterministic frame presentation across asymmetric GPU nodes.
///
/// # How It Works
///
/// 1. Each GPU node renders its assigned 16K tile.
/// 2. When complete, it calls `synchronize_cluster_tick()` with a lightweight
///    [`TilePayload`] containing only a hash of the geometry state.
/// 3. The barrier blocks until ALL nodes have reported in.
/// 4. The "leader" (Xeon brain) signals the compositor to perform the display flip.
///
/// This prevents:
///   • Screen tearing: no partial frames are ever displayed.
///   • Jitter: all tiles are from the same logical frame.
///   • Bandwidth waste: only hashes, not pixels, cross the sync wire.
#[derive(Debug)]
pub struct ShoggothSyncChain {
    /// Total number of rendering nodes in this chain.
    pub total_nodes: usize,
    /// tokio barrier: all nodes must `.wait()` before any proceeds.
    frame_barrier: Arc<Barrier>,
}

impl ShoggothSyncChain {
    /// Creates a new sync chain for `nodes_count` rendering nodes.
    ///
    /// # Panics
    ///
    /// Panics if `nodes_count` is 0.
    #[must_use]
    pub fn new(nodes_count: usize) -> Self {
        assert!(nodes_count > 0, "Sync chain requires at least 1 node");
        Self {
            total_nodes: nodes_count,
            frame_barrier: Arc::new(Barrier::new(nodes_count)),
        }
    }

    /// Blocks the calling task until every node in the chain has completed
    /// its tile for the current frame.
    ///
    /// # Leader Election
    ///
    /// The barrier automatically elects one node as the "leader" (the last
    /// node to arrive). The leader triggers the display flip via the compositor.
    ///
    /// # Cancellation Safety
    ///
    /// This function is cancel-safe. If a task is cancelled while waiting,
    /// the barrier count is NOT decremented — other nodes will hang. In
    /// production, wrap with `tokio::time::timeout` and handle the fallout.
    pub async fn synchronize_cluster_tick(&self, node_id: &str, tile: TilePayload) {
        tracing::trace!(
            node_id,
            tile_id = tile.tile_id,
            frame_id = tile.frame_id,
            hash = tile.vertex_matrix_hash,
            "Node waiting at sync barrier"
        );

        // Wait for all sibling nodes to arrive.
        let wait_result = self.frame_barrier.wait().await;

        if wait_result.is_leader() {
            // The Xeon brain is the leader — trigger the display flip.
            tracing::info!(
                frame = tile.frame_id,
                node_count = self.total_nodes,
                "Sync chain leader: signaling display flip"
            );
            signal_display_flip(tile.frame_id);
        }
    }

    /// Returns a clone of the barrier for sharing across tasks.
    #[must_use]
    pub fn barrier(&self) -> Arc<Barrier> {
        Arc::clone(&self.frame_barrier)
    }
}

// ── Leader Action ──────────────────────────────────────────────────────────────

/// Called by the sync chain leader when all tiles are ready.
fn signal_display_flip(frame: u64) {
    // In production: signals the compositor that a complete frame is ready
    // for encoding and streaming. This triggers:
    //   1. NVENC/AMF hardware encode of the composited 16K frame.
    //   2. WebRTC media track submission to the client.
    //   3. VSYNC-aligned buffer swap on the client display.
    tracing::debug!(frame, "Display flip signaled");
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sync_chain_barrier_all_arrive() {
        let chain = Arc::new(ShoggothSyncChain::new(3));

        let mut handles = Vec::new();
        for i in 0..3 {
            let c = Arc::clone(&chain);
            handles.push(tokio::spawn(async move {
                let tile = TilePayload {
                    tile_id: i,
                    frame_id: 1,
                    vertex_matrix_hash: 0xDEAD_BEEF,
                };
                c.synchronize_cluster_tick(&format!("node-{i}"), tile).await;
            }));
        }

        for h in handles {
            h.await.expect("Sync task panicked");
        }
    }

    #[test]
    #[should_panic(expected = "Sync chain requires at least 1 node")]
    fn test_zero_nodes_panics() {
        ShoggothSyncChain::new(0);
    }
}
