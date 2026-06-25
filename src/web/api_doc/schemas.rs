#![cfg(feature = "openapi-docs")]

//! Shared OpenAPI schema definitions.

use std::collections::HashMap;

/// Result of a single health-check component.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct ApiHealthCheckResult {
    status: String,
    message: String,
    duration_ms: u64,
}

/// Composite health status returned by the detailed health endpoint.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct ApiHealthStatus {
    status: String,
    version: String,
    timestamp: i64,
    checks: HashMap<String, ApiHealthCheckResult>,
}
