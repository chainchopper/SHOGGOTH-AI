// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-display/src/webrtc_signaling.rs — WebRTC signaling server.
//
// Exchanges SDP offers/answers between the Shoggoth compositor and client
// devices. Uses tokio-tungstenite for real WebSocket transport, with a
// broadcast channel per client for SDP/ICE relay.
//
// Protocol:
//   1. Client connects via WebSocket to /ws/signaling.
//   2. Compositor sends SDP offer → server relays to client.
//   3. Client sends SDP answer → server relays to compositor.
//   4. ICE candidates are exchanged bidirectionally.
//   5. Once connected, media flows over UDP (WebRTC data channel).

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

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

/// A connected signaling client with a real broadcast sender.
#[derive(Debug, Clone)]
struct SignalingClient {
    client_id: String,
    /// Broadcast sender — each client gets its own channel.
    sender: broadcast::Sender<String>,
    connected_at: std::time::Instant,
}

// ── Signaling Server ───────────────────────────────────────────────────────────

/// Manages WebRTC signaling between compositor and client devices.
///
/// Each connected client gets a tokio broadcast channel. The compositor
/// sends SDP offers and ICE candidates to all clients; answers and
/// client ICE candidates are relayed back via the same channels.
#[derive(Debug, Clone)]
pub struct SignalingServer {
    /// Connected clients indexed by client_id.
    clients: Arc<DashMap<String, SignalingClient>>,
}

impl SignalingServer {
    /// Creates a new signaling server.
    #[must_use]
    pub fn new() -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
        }
    }

    /// Registers a new client and returns a receiver for its message stream.
    ///
    /// The returned receiver is used by the WebSocket handler to read
    /// SDP answers and ICE candidates from the client.
    pub fn register_client(&self, client_id: &str) -> broadcast::Receiver<String> {
        let (tx, rx) = broadcast::channel::<String>(64);
        let now = std::time::Instant::now();
        self.clients.insert(
            client_id.into(),
            SignalingClient {
                client_id: client_id.into(),
                sender: tx,
                connected_at: now,
            },
        );
        tracing::info!(client_id, "Signaling client registered ({} total)", self.clients.len());
        rx
    }

    /// Sends a signaling message to a specific client.
    ///
    /// Returns `true` if the client was found and the message was queued.
    pub fn send_to_client(&self, client_id: &str, message: &SignalingMessage) -> bool {
        if let Some(entry) = self.clients.get(client_id) {
            let json = serde_json::to_string(message).unwrap_or_default();
            let _ = entry.sender.send(json);
            true
        } else {
            false
        }
    }

    /// Broadcasts a signaling message to all connected clients.
    pub fn broadcast(&self, message: &SignalingMessage) {
        let json = serde_json::to_string(message).unwrap_or_default();
        for entry in self.clients.iter() {
            let _ = entry.sender.send(json.clone());
        }
    }

    /// Removes a client.
    pub fn unregister_client(&self, client_id: &str) {
        self.clients.remove(client_id);
        tracing::info!(client_id, "Signaling client unregistered ({} remaining)", self.clients.len());
    }

    /// Number of connected clients.
    #[must_use]
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Returns all registered client IDs.
    #[must_use]
    pub fn client_ids(&self) -> Vec<String> {
        self.clients.iter().map(|e| e.key().clone()).collect()
    }

    /// Checks if a client is registered.
    #[must_use]
    pub fn is_registered(&self, client_id: &str) -> bool {
        self.clients.contains_key(client_id)
    }
}

impl Default for SignalingServer {
    fn default() -> Self {
        Self::new()
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
            _ => panic!("Expected Offer, got other variant"),
        }
    }

    #[test]
    fn test_send_to_client() {
        let server = SignalingServer::new();
        let _rx = server.register_client("client-02");
        assert!(server.is_registered("client-02"));

        let sent = server.send_to_client("client-02", &SignalingMessage::Ack {
            message: "hello".into(),
        });
        assert!(sent);
    }
}
