//! 健康检查和根路由处理器

use axum::{response::IntoResponse, routing::get, Json, Router};
use serde_json::json;

pub async fn root_handler() -> impl IntoResponse {
    Json(json!({
        "msg": "Synapse Rust Matrix Server",
        "version": "0.1.0"
    }))
}

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

    let pool = state.services.user_storage.pool.clone();

    let db_start = std::time::Instant::now();
    match sqlx::query("SELECT 1").fetch_one(&*pool).await {
        Ok(_) => {
            checks.insert("database".to_string(), json!({
                "status": "healthy",
                "message": "Connection successful",
                "duration_ms": db_start.elapsed().as_millis()
            }));
        }
        Err(e) => {
            overall_status = "unhealthy";
            checks.insert("database".to_string(), json!({
                "status": "unhealthy",
                "message": format!("Connection failed: {}", e),
                "duration_ms": db_start.elapsed().as_millis()
            }));
        }
    }

    let schema_start = std::time::Instant::now();
    let required_tables = [
        "users", "rooms", "events", "devices", "access_tokens",
        "refresh_tokens", "federation_signing_keys", "room_memberships",
        "widgets", "secure_key_backups", "media_metadata",
    ];
    let mut missing_tables = Vec::new();
    for table in &required_tables {
        let exists: Result<(bool,), _> = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name=$1)"
        )
        .bind(table)
        .fetch_one(&*pool)
        .await;
        match exists {
            Ok((true,)) => {}
            _ => {
                missing_tables.push(table.to_string());
            }
        }
    }
    if missing_tables.is_empty() {
        checks.insert("schema".to_string(), json!({
            "status": "healthy",
            "message": format!("All {} required tables exist", required_tables.len()),
            "duration_ms": schema_start.elapsed().as_millis()
        }));
    } else {
        overall_status = "degraded";
        checks.insert("schema".to_string(), json!({
            "status": "degraded",
            "message": format!("Missing tables: {}", missing_tables.join(", ")),
            "duration_ms": schema_start.elapsed().as_millis()
        }));
    }

    Json(json!({
        "status": overall_status,
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "checks": checks
    }))
}

pub fn create_health_router() -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_check))
}
