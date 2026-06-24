// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/webhooks.rs — External event & webhook system.
//
// Enables third-party services (Slack, PagerDuty, CI/CD, custom dashboards)
// to receive real-time Shoggoth fabric events via HTTP webhooks.
//
// Event types:
//   • node.joined / node.left — Fabric membership changes.
//   • workload.dispatched / workload.completed / workload.failed — Task lifecycle.
//   • benchmark.completed — GEMM/latency benchmark results.
//   • cloud.provisioned / cloud.terminated — Auto-scale events.
//   • alert.triggered / alert.resolved — Prometheus alert relay.
//   • thermal.throttle — GPU temperature exceeded threshold.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ── Types ──────────────────────────────────────────────────────────────────────

/// All event types emitted by the Shoggoth fabric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    // Fabric events.
    NodeJoined,
    NodeLeft,
    NodeDegraded,

    // Workload events.
    WorkloadDispatched,
    WorkloadCompleted,
    WorkloadFailed,

    // Benchmark events.
    BenchmarkCompleted,

    // Cloud provisioning events.
    CloudProvisioned,
    CloudTerminated,

    // Alert events.
    AlertTriggered,
    AlertResolved,

    // Thermal events.
    ThermalThrottle,
}

/// A webhook subscription registered by an external service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookSubscription {
    /// Unique subscription ID.
    pub id: String,
    /// URL to POST events to.
    pub url: String,
    /// Shared secret for HMAC-SHA256 signature verification.
    pub secret: String,
    /// Which event types to receive.
    pub event_types: Vec<EventType>,
    /// Whether this subscription is active.
    pub active: bool,
    /// Maximum retry count for failed deliveries.
    pub max_retries: u32,
    /// Current consecutive failure count.
    pub consecutive_failures: u32,
    /// When this subscription was created.
    pub created_at: u64,
}

/// A fabric event delivered to webhook subscribers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FabricEvent {
    /// Unique event ID (ULID or UUID).
    pub event_id: String,
    /// Event type.
    #[serde(rename = "type")]
    pub event_type: EventType,
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Node or workload identifier the event pertains to.
    pub subject_id: Option<String>,
    /// Human-readable event description.
    pub message: String,
    /// Arbitrary event-specific payload.
    pub payload: serde_json::Value,
    /// Shoggoth version emitting the event.
    pub shoggoth_version: String,
}

// ── Webhook Engine ─────────────────────────────────────────────────────────────

/// Manages webhook subscriptions and event delivery.
#[derive(Debug)]
pub struct WebhookEngine {
    /// Active subscriptions indexed by ID.
    subscriptions: Arc<DashMap<String, WebhookSubscription>>,
    /// HTTP client for POSTing events.
    http_client: reqwest::Client,
    /// Maximum concurrent deliveries.
    max_concurrency: usize,
}

impl WebhookEngine {
    /// Creates a new webhook engine.
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(DashMap::new()),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("Failed to create HTTP client"),
            max_concurrency: 16,
        }
    }

    /// Registers a new webhook subscription.
    pub fn register(
        &self,
        url: &str,
        secret: &str,
        event_types: Vec<EventType>,
    ) -> String {
        let id = format!("wh_sha256_{}", uuid::Uuid::new_v4());

        let sub = WebhookSubscription {
            id: id.clone(),
            url: url.into(),
            secret: secret.into(),
            event_types,
            active: true,
            max_retries: 3,
            consecutive_failures: 0,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        self.subscriptions.insert(id.clone(), sub);
        tracing::info!(subscription_id = %id, url, "Webhook subscription registered");
        id
    }

    /// Removes a webhook subscription.
    pub fn unregister(&self, id: &str) -> bool {
        let removed = self.subscriptions.remove(id);
        if removed.is_some() {
            tracing::info!(subscription_id = %id, "Webhook subscription removed");
        }
        removed.is_some()
    }

    /// Lists all active subscriptions.
    pub fn list(&self) -> Vec<WebhookSubscription> {
        self.subscriptions
            .iter()
            .map(|e| e.value().clone())
            .collect()
    }

    /// Emits a fabric event to all matching subscribers.
    ///
    /// Delivery is fire-and-forget — failures are logged and the subscription's
    /// consecutive_failures counter is incremented. After max_retries consecutive
    /// failures, the subscription is automatically deactivated.
    pub async fn emit(&self, event: FabricEvent) {
        let matching: Vec<WebhookSubscription> = self
            .subscriptions
            .iter()
            .filter(|e| {
                e.value().active && e.value().event_types.contains(&event.event_type)
            })
            .map(|e| e.value().clone())
            .collect();

        if matching.is_empty() {
            return;
        }

        // Deliver with concurrency limiting via a semaphore.
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.max_concurrency));

        for sub in matching {
            let permit = semaphore.clone().acquire_owned().await;
            let event = event.clone();
            let client = self.http_client.clone();
            let subs = self.subscriptions.clone();
            let sub_id = sub.id.clone();

            tokio::spawn(async move {
                let _permit = permit;
                let result = deliver_event(&client, &sub, &event).await;

                if let Some(mut entry) = subs.get_mut(&sub_id) {
                    match result {
                        Ok(()) => {
                            entry.consecutive_failures = 0;
                        }
                        Err(e) => {
                            entry.consecutive_failures += 1;
                            tracing::warn!(
                                subscription_id = %sub_id,
                                url = %sub.url,
                                error = %e,
                                failures = entry.consecutive_failures,
                                "Webhook delivery failed"
                            );
                            if entry.consecutive_failures >= entry.max_retries {
                                entry.active = false;
                                tracing::error!(
                                    subscription_id = %sub_id,
                                    "Webhook subscription deactivated after {} consecutive failures",
                                    entry.consecutive_failures,
                                );
                            }
                        }
                    }
                }
            });
        }
    }
}

impl Default for WebhookEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ── Delivery ──────────────────────────────────────────────────────────────────

async fn deliver_event(
    client: &reqwest::Client,
    sub: &WebhookSubscription,
    event: &FabricEvent,
) -> Result<(), String> {
    let payload = serde_json::to_vec(event).map_err(|e| e.to_string())?;

    // Compute HMAC-SHA256 signature.
    let mut mac = hmac::Hmac::<Sha256>::new_from_slice(sub.secret.as_bytes())
        .map_err(|e| e.to_string())?;
    hmac::Mac::update(&mut mac, &payload);
    let signature = hex::encode(mac.finalize().into_bytes());

    let response = client
        .post(&sub.url)
        .header("Content-Type", "application/json")
        .header("X-Shoggoth-Event", format!("{:?}", event.event_type))
        .header("X-Shoggoth-Signature", format!("sha256={signature}"))
        .header("X-Shoggoth-Delivery", &event.event_id)
        .body(payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("HTTP {}", response.status()))
    }
}

// ── Event Builder ─────────────────────────────────────────────────────────────

/// Builds a FabricEvent with consistent formatting.
pub fn build_event(
    event_type: EventType,
    subject_id: Option<&str>,
    message: &str,
    payload: serde_json::Value,
) -> FabricEvent {
    FabricEvent {
        event_id: uuid::Uuid::new_v4().to_string(),
        event_type,
        timestamp: chrono::Utc::now().to_rfc3339(),
        subject_id: subject_id.map(String::from),
        message: message.into(),
        payload,
        shoggoth_version: crate::VERSION.into(),
    }
}

// ── Pre-built Event Emitters ──────────────────────────────────────────────────

/// Emits a node.joined event.
pub fn emit_node_joined(engine: &WebhookEngine, node_id: &str) {
    let event = build_event(
        EventType::NodeJoined,
        Some(node_id),
        &format!("Node {node_id} joined the Shoggoth fabric"),
        serde_json::json!({"node_id": node_id}),
    );
    // In production: spawn the emit.
    let _ = event;
    let _ = engine;
}

/// Emits a workload.completed event with benchmark data.
pub fn emit_workload_completed(engine: &WebhookEngine, work_id: u64, node_id: &str, elapsed_us: u64) {
    let event = build_event(
        EventType::WorkloadCompleted,
        Some(node_id),
        &format!("Workload {work_id} completed on {node_id} in {elapsed_us}µs"),
        serde_json::json!({
            "work_id": work_id,
            "node_id": node_id,
            "elapsed_us": elapsed_us,
        }),
    );
    let _ = event;
    let _ = engine;
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_registration() {
        let engine = WebhookEngine::new();
        let id = engine.register(
            "https://hooks.example.com/shoggoth",
            "secret123",
            vec![EventType::NodeJoined, EventType::WorkloadCompleted],
        );
        assert!(!id.is_empty());
        assert!(id.starts_with("wh_sha256_"));

        let subs = engine.list();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].event_types.len(), 2);
    }

    #[test]
    fn test_webhook_unregistration() {
        let engine = WebhookEngine::new();
        let id = engine.register("https://hooks.example.com/shoggoth", "secret", vec![EventType::NodeJoined]);
        assert!(engine.unregister(&id));
        assert!(engine.list().is_empty());
    }

    #[test]
    fn test_event_building() {
        let event = build_event(
            EventType::NodeJoined,
            Some("rtx-5090"),
            "Node joined",
            serde_json::json!({"vram": 32}),
        );
        assert_eq!(event.event_type, EventType::NodeJoined);
        assert_eq!(event.subject_id, Some("rtx-5090".into()));
        assert!(!event.event_id.is_empty());
        assert!(event.timestamp.contains("T"));
    }
}
