//! 健康检查和根路由处理器

use crate::web::routes::context::AdminContext;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

/// Basic liveness + readiness probe used by Docker healthcheck.
///
/// Performs a lightweight database `SELECT 1` so that the container is
/// marked unhealthy when Postgres is unreachable, even if the HTTP server
/// itself is still accepting connections.
pub async fn health_check(State(ctx): State<AdminContext>) -> impl IntoResponse {
    let db_ok = ctx.admin_server_service.is_database_healthy().await;
    let status = if db_ok { "ok" } else { "unhealthy" };
    let http_status = if db_ok { axum::http::StatusCode::OK } else { axum::http::StatusCode::SERVICE_UNAVAILABLE };

    (
        http_status,
        Json(json!({
            "status": status,
            "timestamp": chrono::Utc::now().to_rfc3339()
        })),
    )
}

pub async fn detailed_health_check(State(ctx): State<AdminContext>) -> impl IntoResponse {
    let mut checks = serde_json::Map::new();
    let mut overall_status = "healthy";

    let db_start = std::time::Instant::now();
    let db_ok = ctx.admin_server_service.is_database_healthy().await;
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
    let missing_tables = ctx
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

    // Redis connectivity probe — only checked when Redis is enabled in config.
    // A degraded Redis does NOT make the server unhealthy (in-memory fallback
    // exists), but it does affect rate-limit consistency in multi-worker setups.
    if ctx.cache.is_redis_enabled() {
        let redis_start = std::time::Instant::now();
        let redis_ok = ctx.cache.get::<String>("__health_probe__").await.is_ok();
        let redis_status =
            if redis_ok && redis_start.elapsed() < std::time::Duration::from_secs(5) { "healthy" } else { "degraded" };
        if redis_status == "degraded" && overall_status == "healthy" {
            overall_status = "degraded";
        }
        checks.insert(
            "redis".to_string(),
            json!({
                "status": redis_status,
                "message": if redis_status == "healthy" {
                    "Round-trip completed".to_string()
                } else {
                    "Round-trip failed or exceeded 5s threshold".to_string()
                },
                "duration_ms": redis_start.elapsed().as_millis()
            }),
        );
    } else {
        checks.insert(
            "redis".to_string(),
            json!({
                "status": "disabled",
                "message": "Redis not enabled — using in-memory cache"
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
