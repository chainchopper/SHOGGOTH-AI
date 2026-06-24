// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-display/src/client_stream.rs — Adaptive bitrate WebRTC streaming controller.
//
// Handles delivery of the composited viewport to the client (VSCode, browser,
// Tauri desktop app) over WebRTC with sub-16ms latency targets.
//
// Strategy:
//   • Monitors real-time packet loss and round-trip time.
//   • Dynamically adjusts encoding parameters:
//       - Drops from YUV 4:4:4 → YUV 4:2:0 chroma subsampling under congestion.
//       - Shifts GOP structure toward more P-frames (predicted) vs I-frames (intra).
//       - Reduces target framerate from 60 → 30 → 15 as a last resort.
//   • Uses NVENC (RTX 5090) or AMF (V620/BC250) hardware encoders for zero CPU overhead.

// ── Types ──────────────────────────────────────────────────────────────────────

/// Streaming quality preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamQualityPreset {
    /// 4:4:4 chroma, 60fps, highest bitrate. Used when network is healthy.
    Ultra,
    /// 4:2:0 chroma, 60fps, medium bitrate. Default for 1 Gbps LAN.
    High,
    /// 4:2:0 chroma, 30fps, reduced bitrate. Triggers at > 2% packet loss.
    Balanced,
    /// 4:2:0 chroma, 15fps, minimum bitrate. Last resort for congested links.
    LowLatency,
}

impl StreamQualityPreset {
    /// Target framerate for this quality preset.
    #[must_use]
    pub fn target_framerate(&self) -> u8 {
        match self {
            Self::Ultra | Self::High => 60,
            Self::Balanced => 30,
            Self::LowLatency => 15,
        }
    }

    /// Whether to use 4:4:4 chroma subsampling (true) or 4:2:0 (false).
    #[must_use]
    pub fn full_chroma(&self) -> bool {
        matches!(self, Self::Ultra)
    }
}

// ── Stream Controller ──────────────────────────────────────────────────────────

/// Monitors network conditions and adjusts the WebRTC encoding pipeline in real time.
#[derive(Debug)]
pub struct ClientStreamController {
    /// Current network round-trip time to the client in milliseconds.
    pub current_network_rtt_ms: u32,
    /// Current quality preset based on network conditions.
    pub quality_preset: StreamQualityPreset,
    /// Smoothed packet loss percentage (exponential moving average).
    smoothed_packet_loss: f32,
    /// EMA alpha factor for packet loss smoothing (0.0–1.0). Lower = smoother.
    ema_alpha: f32,
    /// Number of times the quality has been downgraded (for hysteresis).
    downgrade_count: u32,
    /// Number of consecutive healthy reports before upgrading quality.
    healthy_streak: u32,
}

impl ClientStreamController {
    /// Creates a new stream controller targeting ultra quality.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_network_rtt_ms: 0,
            quality_preset: StreamQualityPreset::Ultra,
            smoothed_packet_loss: 0.0,
            ema_alpha: 0.3,
            downgrade_count: 0,
            healthy_streak: 0,
        }
    }

    /// Updates the controller with the latest network telemetry and adjusts
    /// encoding parameters if necessary.
    ///
    /// Call this once per frame (or per RTCP receiver report).
    pub fn on_network_telemetry(&mut self, rtt_ms: u32, lost_packets_percentage: f32) {
        self.current_network_rtt_ms = rtt_ms;

        // Exponential moving average smoothing of packet loss.
        self.smoothed_packet_loss = self.ema_alpha * lost_packets_percentage
            + (1.0 - self.ema_alpha) * self.smoothed_packet_loss;

        // ── Hysteresis: only change quality if condition persists ──
        if self.smoothed_packet_loss > 2.0 {
            // Network is choking — downgrade.
            self.downgrade_count += 1;
            self.healthy_streak = 0;

            if self.downgrade_count >= 5 {
                self.downgrade_quality();
                self.downgrade_count = 0;
            }
        } else if self.smoothed_packet_loss < 0.5 {
            // Network is healthy — consider upgrading.
            self.downgrade_count = 0;
            self.healthy_streak += 1;

            if self.healthy_streak >= 30 {
                self.upgrade_quality();
                self.healthy_streak = 0;
            }
        }
    }

    /// Returns the current target framerate.
    #[must_use]
    pub fn target_framerate(&self) -> u8 {
        self.quality_preset.target_framerate()
    }

    /// Downgrades encoding quality one tier.
    fn downgrade_quality(&mut self) {
        let previous = self.quality_preset;
        self.quality_preset = match self.quality_preset {
            StreamQualityPreset::Ultra => StreamQualityPreset::High,
            StreamQualityPreset::High => StreamQualityPreset::Balanced,
            StreamQualityPreset::Balanced | StreamQualityPreset::LowLatency => {
                StreamQualityPreset::LowLatency
            }
        };

        if previous != self.quality_preset {
            tracing::warn!(
                from = ?previous,
                to = ?self.quality_preset,
                rtt_ms = self.current_network_rtt_ms,
                packet_loss_pct = self.smoothed_packet_loss,
                "Stream quality downgraded due to network congestion"
            );
        }
    }

    /// Upgrades encoding quality one tier.
    fn upgrade_quality(&mut self) {
        let previous = self.quality_preset;
        self.quality_preset = match self.quality_preset {
            StreamQualityPreset::LowLatency => StreamQualityPreset::Balanced,
            StreamQualityPreset::Balanced => StreamQualityPreset::High,
            StreamQualityPreset::High | StreamQualityPreset::Ultra => StreamQualityPreset::Ultra,
        };

        if previous != self.quality_preset {
            tracing::info!(
                from = ?previous,
                to = ?self.quality_preset,
                rtt_ms = self.current_network_rtt_ms,
                "Stream quality upgraded — network healthy"
            );
        }
    }
}

impl Default for ClientStreamController {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_quality_is_ultra() {
        let controller = ClientStreamController::new();
        assert_eq!(controller.quality_preset, StreamQualityPreset::Ultra);
        assert_eq!(controller.target_framerate(), 60);
    }

    #[test]
    fn test_downgrade_on_persistent_loss() {
        let mut controller = ClientStreamController::new();

        // Feed persistent 5% packet loss for 5+ frames.
        for _ in 0..10 {
            controller.on_network_telemetry(50, 5.0);
        }

        assert_eq!(controller.quality_preset, StreamQualityPreset::High);
    }

    #[test]
    fn test_no_downgrade_on_transient_loss() {
        let mut controller = ClientStreamController::new();

        // Single spike — shouldn't trigger downgrade.
        controller.on_network_telemetry(50, 10.0);
        assert_eq!(controller.quality_preset, StreamQualityPreset::Ultra);
    }

    #[test]
    fn test_upgrade_on_sustained_health() {
        let mut controller = ClientStreamController::new();

        // First, force a downgrade.
        for _ in 0..10 {
            controller.on_network_telemetry(100, 5.0);
        }
        assert_ne!(controller.quality_preset, StreamQualityPreset::Ultra);

        // Then feed healthy reports.
        for _ in 0..60 {
            controller.on_network_telemetry(10, 0.0);
        }

        assert_eq!(controller.quality_preset, StreamQualityPreset::Ultra);
    }

    #[test]
    fn test_full_chroma_only_for_ultra() {
        assert!(StreamQualityPreset::Ultra.full_chroma());
        assert!(!StreamQualityPreset::High.full_chroma());
        assert!(!StreamQualityPreset::Balanced.full_chroma());
        assert!(!StreamQualityPreset::LowLatency.full_chroma());
    }
}
