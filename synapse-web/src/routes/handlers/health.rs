//! 健康检查和根路由处理器

use axum::{response::IntoResponse, Json};
use serde_json::json;
use synapse_common::health::CheckResult;
use synapse_storage::schema_validator::SchemaValidator;

pub async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

pub async fn detailed_health_check(
    axum::extract::State(state): axum::extract::State<crate::routes::AppState>,
) -> impl IntoResponse {
    let mut checks = serde_json::Map::new();
    let mut overall_status = "healthy";
    let readiness = state.health_checker.check_readiness().await;
    let database_check = readiness.checks.get("database").cloned().unwrap_or(CheckResult {
        status: "unhealthy".to_string(),
        message: "Database health check not configured".to_string(),
        duration_ms: 0,
    });

    if database_check.status != "healthy" {
        overall_status = "unhealthy";
    }
    checks.insert("database".to_string(), json!(database_check));

    let schema_start = std::time::Instant::now();
    let required_tables = [
        "users",
        "rooms",
        "events",
        "devices",
        "access_tokens",
        "refresh_tokens",
        "federation_signing_keys",
        "room_memberships",
        "widgets",
        "secure_key_backups",
        "media_metadata",
    ];
    let validator = SchemaValidator::new(state.services.account.user_storage.pool.clone());
    let missing_tables = validator
        .validate_required_tables(&required_tables)
        .await
        .unwrap_or_else(|_| required_tables.iter().map(|table| (*table).to_string()).collect());

    if missing_tables.is_empty() {
        checks.insert(
            "schema".to_string(),
            json!({
                "status": "healthy",
                "message": format!("All {} required tables exist", required_tables.len()),
                "duration_ms": schema_start.elapsed().as_millis()
            }),
        );
    } else {
        overall_status = "degraded";
        checks.insert(
            "schema".to_string(),
            json!({
                "status": "degraded",
                "message": format!("Missing tables: {}", missing_tables.join(", ")),
                "duration_ms": schema_start.elapsed().as_millis()
            }),
        );
    }

    Json(json!({
        "status": overall_status,
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "checks": checks
    }))
}
