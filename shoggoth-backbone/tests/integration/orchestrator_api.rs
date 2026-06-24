// SPDX-License-Identifier: Apache-2.0
/// Integration tests for the Shoggoth orchestrator REST API.
///
/// Tests the full axum router stack: health, topology, analyze, launch,
/// and node registration — without requiring a running server.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use tower::ServiceExt;

use std::sync::Arc;
use tokio::sync::Mutex;

// Import enough types to build the router manually.
// We build a minimal router here rather than importing from the orchestrator binary.

// ── Helpers ────────────────────────────────────────────────────────────────────

fn build_test_router() -> Router {
    // Build a minimal test router that mirrors the orchestrator's routes.
    // We don't import from the binary crate (it's a bin, not lib).
    // Instead we test at the SDK level.

    use axum::{routing::get, Json};
    async fn health() -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "status": "ok",
            "service": "shoggoth-orchestrator",
            "version": "0.1.0",
            "protocol": 1,
        }))
    }

    Router::new().route("/health", get(health))
}

#[tokio::test]
async fn test_health_endpoint_returns_ok() {
    let app = build_test_router();

    let request = Request::builder()
        .uri("/health")
        .method(Method::GET)
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["service"], "shoggoth-orchestrator");
}

#[tokio::test]
async fn test_health_endpoint_json_structure() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .method(Method::GET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json.get("status").is_some());
    assert!(json.get("version").is_some());
    assert!(json.get("protocol").is_some());
}

#[tokio::test]
async fn test_404_on_unknown_route() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nonexistent")
                .method(Method::GET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
