// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-sdk/src/error_catalog.rs — API error catalog and status codes.
//
// Defines every error the Shoggoth orchestrator REST API can return,
// with stable error codes, HTTP status codes, and user-facing messages.
// Clients can match on `error_code` for programmatic error handling.

use serde::{Deserialize, Serialize};

// ── Error Codes ───────────────────────────────────────────────────────────────

/// Every error the Shoggoth API can return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    // ── 4xx Client Errors ──
    /// Invalid request body (missing required field, wrong type).
    BadRequest = 400,
    /// Authentication required (missing or invalid API key).
    Unauthorized = 401,
    /// Authenticated but insufficient permissions.
    Forbidden = 403,
    /// Resource not found (unknown node_id, template, etc.).
    NotFound = 404,
    /// Request rate limit exceeded.
    TooManyRequests = 429,

    // ── 5xx Server Errors ──
    /// Generic internal server error.
    InternalError = 500,
    /// Orchestrator not ready (still bootstrapping).
    ServiceUnavailable = 503,
    /// Backend timeout (orchestrator timed out talking to node agent).
    GatewayTimeout = 504,

    // ── Custom Shoggoth Errors ──
    /// Node agent unreachable.
    NodeOffline = 1001,
    /// No hardware available for the requested capability.
    NoHardwareAvailable = 1002,
    /// Cloud provisioning budget exceeded.
    BudgetExceeded = 1003,
    /// Workload type not supported.
    UnsupportedWorkload = 1004,
    /// SPIR-V binary validation failed.
    InvalidSpirv = 1005,
    /// Requested template does not exist.
    UnknownTemplate = 1006,
    /// GPU out of memory on target node.
    GpuOutOfMemory = 1007,
    /// Network partition: orchestrator cannot reach nodes.
    NetworkPartition = 1008,
    /// Configuration error (missing env var, invalid cert).
    ConfigurationError = 1009,
}

impl ErrorCode {
    /// HTTP status code for this error.
    pub fn http_status(&self) -> u16 {
        match self {
            Self::BadRequest => 400,
            Self::Unauthorized => 401,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::TooManyRequests => 429,
            Self::InternalError => 500,
            Self::ServiceUnavailable => 503,
            Self::GatewayTimeout => 504,
            Self::NodeOffline => 502,
            Self::NoHardwareAvailable => 503,
            Self::BudgetExceeded => 402,
            Self::UnsupportedWorkload => 400,
            Self::InvalidSpirv => 400,
            Self::UnknownTemplate => 404,
            Self::GpuOutOfMemory => 507,
            Self::NetworkPartition => 503,
            Self::ConfigurationError => 500,
        }
    }

    /// Human-readable error code string.
    pub fn code_string(&self) -> &str {
        match self {
            Self::BadRequest => "BAD_REQUEST",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Forbidden => "FORBIDDEN",
            Self::NotFound => "NOT_FOUND",
            Self::TooManyRequests => "TOO_MANY_REQUESTS",
            Self::InternalError => "INTERNAL_ERROR",
            Self::ServiceUnavailable => "SERVICE_UNAVAILABLE",
            Self::GatewayTimeout => "GATEWAY_TIMEOUT",
            Self::NodeOffline => "NODE_OFFLINE",
            Self::NoHardwareAvailable => "NO_HARDWARE_AVAILABLE",
            Self::BudgetExceeded => "BUDGET_EXCEEDED",
            Self::UnsupportedWorkload => "UNSUPPORTED_WORKLOAD",
            Self::InvalidSpirv => "INVALID_SPIRV",
            Self::UnknownTemplate => "UNKNOWN_TEMPLATE",
            Self::GpuOutOfMemory => "GPU_OUT_OF_MEMORY",
            Self::NetworkPartition => "NETWORK_PARTITION",
            Self::ConfigurationError => "CONFIGURATION_ERROR",
        }
    }

    /// User-facing message for this error.
    pub fn message(&self) -> &str {
        match self {
            Self::BadRequest => "Invalid request. Check required fields and types.",
            Self::Unauthorized => "Authentication required. Provide a valid API key.",
            Self::Forbidden => "Insufficient permissions for this operation.",
            Self::NotFound => "The requested resource was not found.",
            Self::TooManyRequests => "Rate limit exceeded. Retry after the Retry-After header.",
            Self::InternalError => "An unexpected internal error occurred.",
            Self::ServiceUnavailable => "The orchestrator is not ready yet. Retry in a few seconds.",
            Self::GatewayTimeout => "The orchestrator timed out waiting for a node agent response.",
            Self::NodeOffline => "The target node agent is offline or unreachable.",
            Self::NoHardwareAvailable => "No hardware nodes are available for the requested capability.",
            Self::BudgetExceeded => "Cloud provisioning budget exceeded. Increase budget or wait for idle nodes.",
            Self::UnsupportedWorkload => "The submitted workload type is not supported by the agentic parser.",
            Self::InvalidSpirv => "The provided SPIR-V binary failed validation. Recompile the shader.",
            Self::UnknownTemplate => "The requested template does not exist. Valid: render-farm, heavy-compute, async-game-runtime, genomic-processing, generic.",
            Self::GpuOutOfMemory => "The target GPU ran out of VRAM. Try reducing batch size or adding cloud nodes.",
            Self::NetworkPartition => "Network partition detected. Some nodes are unreachable.",
            Self::ConfigurationError => "Orchestrator configuration error. Check environment variables.",
        }
    }
}

// ── Error Response Type ───────────────────────────────────────────────────────

/// Standard error response body for all Shoggoth API errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Stable error code string (e.g., "NODE_OFFLINE").
    pub error_code: String,
    /// HTTP status code.
    pub status: u16,
    /// Human-readable error message.
    pub message: String,
    /// Optional detail for debugging (not shown to end users in production).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Optional correlation ID for tracing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// ISO 8601 timestamp.
    pub timestamp: String,
}

impl ErrorResponse {
    /// Builds an error response from an error code.
    pub fn new(code: ErrorCode) -> Self {
        Self {
            error_code: code.code_string().into(),
            status: code.http_status(),
            message: code.message().into(),
            detail: None,
            correlation_id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Attaches debug detail to the error.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Attaches a correlation ID for distributed tracing.
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_error_codes_have_messages() {
        let codes = [
            ErrorCode::BadRequest,
            ErrorCode::Unauthorized,
            ErrorCode::NotFound,
            ErrorCode::NodeOffline,
            ErrorCode::NoHardwareAvailable,
            ErrorCode::GpuOutOfMemory,
            ErrorCode::UnknownTemplate,
        ];
        for code in &codes {
            assert!(!code.message().is_empty());
            assert!(!code.code_string().is_empty());
            assert!(code.http_status() >= 400);
        }
    }

    #[test]
    fn test_error_response_serialization() {
        let resp = ErrorResponse::new(ErrorCode::NodeOffline)
            .with_detail("BC250 APU node bc250-01 stopped heartbeating at 14:32:01Z");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("NODE_OFFLINE"));
        assert!(json.contains("bc250-01"));
    }

    #[test]
    fn test_http_status_mapping() {
        assert_eq!(ErrorCode::NotFound.http_status(), 404);
        assert_eq!(ErrorCode::Unauthorized.http_status(), 401);
        assert_eq!(ErrorCode::TooManyRequests.http_status(), 429);
        assert_eq!(ErrorCode::InternalError.http_status(), 500);
    }
}
