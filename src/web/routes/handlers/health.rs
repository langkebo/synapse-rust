//! 健康检查和根路由处理器

use axum::{response::IntoResponse, Json};
use serde_json::json;

pub async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

pub async fn detailed_health_check(
    axum::extract::State(state): axum::extract::State<crate::web::routes::AppState>,
) -> impl IntoResponse {
    let mut checks = serde_json::Map::new();
    let mut overall_status = "healthy";

    let db_start = std::time::Instant::now();
    let db_ok = state.services.admin.admin_server_service.is_database_healthy().await;
    if db_ok {
        checks.insert(
            "database".to_string(),
            json!({
                "status": "healthy",
                "message": "Connection successful",
                "duration_ms": db_start.elapsed().as_millis()
            }),
        );
    } else {
        overall_status = "unhealthy";
        checks.insert(
            "database".to_string(),
            json!({
                "status": "unhealthy",
                "message": "Connection failed",
                "duration_ms": db_start.elapsed().as_millis()
            }),
        );
    }

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
    let missing_tables = state
        .services
        .admin
        .admin_server_service
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
