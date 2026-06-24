// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-display/src/webrtc_signaling.rs — WebRTC signaling server stub.
//
// The WebRTC signaling server exchanges SDP offers/answers between the
// Shoggoth compositor (offerer) and client devices (answerers).
//
// Protocol:
//   1. Client connects via WebSocket to /ws/signaling.
//   2. Compositor sends SDP offer → server relays to client.
//   3. Client sends SDP answer → server relays to compositor.
//   4. ICE candidates are exchanged bidirectionally.
//   5. Once connected, media flows over UDP (WebRTC data channel).
//
// In production, this integrates with the telemetry WebSocket server
// on port 9101, multiplexing signaling frames alongside telemetry frames.

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ── Types ──────────────────────────────────────────────────────────────────────

/// A WebRTC signaling message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalingMessage {
    /// SDP offer from the compositor to a client.
    Offer {
        sdp: String,
        client_id: String,
    },
    /// SDP answer from a client to the compositor.
    Answer {
        sdp: String,
    },
    /// ICE candidate from either side.
    IceCandidate {
        candidate: String,
        sdp_mid: Option<String>,
        sdp_m_line_index: Option<u16>,
    },
    /// Client registers to receive streams.
    Register {
        client_id: String,
        /// Requested resolution (e.g., "3840x2160").
        resolution: Option<String>,
    },
    /// Server acknowledgement.
    Ack {
        message: String,
    },
}

/// A connected signaling client.
#[derive(Debug)]
struct SignalingClient {
    client_id: String,
    /// Where to forward messages (in production: WebSocket sender handle).
    _sender: (),
    /// When this client connected.
    connected_at: std::time::Instant,
}

// ── Signaling Server ───────────────────────────────────────────────────────────

/// Manages WebRTC signaling between compositor and client devices.
#[derive(Debug, Default)]
pub struct SignalingServer {
    /// Connected clients indexed by client_id.
    clients: Arc<DashMap<String, SignalingClient>>,
}

impl SignalingServer {
    /// Creates a new signaling server.
    pub fn new() -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
        }
    }

    /// Registers a new client.
    pub fn register_client(&self, client_id: &str) {
        let now = std::time::Instant::now();
        self.clients.insert(
            client_id.into(),
            SignalingClient {
                client_id: client_id.into(),
                _sender: (),
                connected_at: now,
            },
        );
        tracing::info!(client_id, "Signaling client registered ({} total)", self.clients.len());
    }

    /// Removes a client.
    pub fn unregister_client(&self, client_id: &str) {
        self.clients.remove(client_id);
        tracing::info!(client_id, "Signaling client unregistered ({} remaining)", self.clients.len());
    }

    /// Number of connected clients.
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Returns all registered client IDs (for offer broadcast).
    pub fn client_ids(&self) -> Vec<String> {
        self.clients.iter().map(|e| e.key().clone()).collect()
    }

    /// Checks if a client is registered.
    pub fn is_registered(&self, client_id: &str) -> bool {
        self.clients.contains_key(client_id)
    }
}

// ── SDP Negotiation Flow ──────────────────────────────────────────────────────
//
//  Compositor                          Signaling Server                     Client
//     │                                      │                                │
//     │  [Register]                          │                                │
//     │─────────────────────────────────────>│                                │
//     │                                      │  [Register]                    │
//     │                                      │<───────────────────────────────│
//     │                                      │                                │
//     │  [Offer: SDP + client_id]            │                                │
//     │─────────────────────────────────────>│                                │
//     │                                      │  [Offer: SDP]                  │
//     │                                      │───────────────────────────────>│
//     │                                      │                                │
//     │                                      │  [Answer: SDP]                 │
//     │                                      │<───────────────────────────────│
//     │  [Answer: SDP]                       │                                │
//     │<─────────────────────────────────────│                                │
//     │                                      │                                │
//     │  ←─── ICE candidates exchanged bidirectionally ───→                  │
//     │                                      │                                │
//     │  ═══ WebRTC media (UDP, direct) ═══ (bypasses signaling server)     │
//

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signaling_client_registration() {
        let server = SignalingServer::new();
        assert_eq!(server.client_count(), 0);

        server.register_client("dashboard-01");
        assert_eq!(server.client_count(), 1);
        assert!(server.is_registered("dashboard-01"));

        server.unregister_client("dashboard-01");
        assert_eq!(server.client_count(), 0);
        assert!(!server.is_registered("dashboard-01"));
    }

    #[test]
    fn test_signaling_message_serialization() {
        let msg = SignalingMessage::Offer {
            sdp: "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\n...".into(),
            client_id: "client-01".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("offer"));
        assert!(json.contains("client-01"));

        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            SignalingMessage::Offer { client_id, .. } => {
                assert_eq!(client_id, "client-01");
            }
            _ => panic!("Wrong variant"),
        }
    }
}
