use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{extract::State, routing::post, Json, Router};
use serde_json::{json, Value};

pub fn create_cleanup_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/cleanup/all", post(cleanup_all))
        .route("/_synapse/admin/v1/cleanup/rooms", post(cleanup_rooms))
        .route("/_synapse/admin/v1/cleanup/tokens", post(cleanup_tokens))
        .with_state(state)
}

pub fn admin_cleanup_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::POST, "/_synapse/admin/v1/cleanup/all"),
        (Method::POST, "/_synapse/admin/v1/cleanup/rooms"),
        (Method::POST, "/_synapse/admin/v1/cleanup/tokens"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "admin::cleanup"))
    .collect()
}

#[axum::debug_handler]
pub async fn cleanup_all(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let min_age_ms = body.get("min_age_ms").and_then(|v| v.as_i64());

    let mut results = serde_json::Map::new();

    // 1. Cleanup rooms and orphans
    let room_results = state
        .services
        .rooms
        .room_storage
        .cleanup_abnormal_data(min_age_ms)
        .await
        .map_err(|e| ApiError::internal_with_log("Room cleanup failed", &e))?;
    results.insert("rooms".to_string(), room_results);

    // 2. Cleanup tokens
    let mut token_results = serde_json::Map::new();

    let access_tokens = state
        .services
        .token_storage
        .cleanup_expired_tokens()
        .await
        .map_err(|e| ApiError::internal_with_log("Access token cleanup failed", &e))?;
    token_results.insert("access_tokens_deleted".to_string(), json!(access_tokens));

    let refresh_tokens = state.services.admin.refresh_token_service.cleanup_expired_tokens().await?;
    token_results.insert("refresh_tokens_deleted".to_string(), json!(refresh_tokens));

    let reg_tokens = state.services.admin.registration_token_service.cleanup_expired_tokens().await?;
    token_results.insert("registration_tokens_deleted".to_string(), json!(reg_tokens));

    let qr_txns = state
        .services
        .qr_login_storage
        .cleanup_expired()
        .await
        .map_err(|e| ApiError::internal_with_log("QR login cleanup failed", &e))?;
    token_results.insert("qr_transactions_deleted".to_string(), json!(qr_txns));

    let email_tokens = state
        .services
        .admin
        .email_verification_storage
        .cleanup_expired_tokens()
        .await
        .map_err(|e| ApiError::internal_with_log("Email token cleanup failed", &e))?;
    token_results.insert("email_tokens_deleted".to_string(), json!(email_tokens));

    results.insert("tokens".to_string(), Value::Object(token_results));

    Ok(Json(Value::Object(results)))
}

#[axum::debug_handler]
pub async fn cleanup_rooms(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let min_age_ms = body.get("min_age_ms").and_then(|v| v.as_i64());
    let results = state
        .services
        .rooms
        .room_storage
        .cleanup_abnormal_data(min_age_ms)
        .await
        .map_err(|e| ApiError::internal_with_log("Room cleanup failed", &e))?;
    Ok(Json(results))
}

#[axum::debug_handler]
pub async fn cleanup_tokens(_admin: AdminUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let mut token_results = serde_json::Map::new();

    let access_tokens = state
        .services
        .token_storage
        .cleanup_expired_tokens()
        .await
        .map_err(|e| ApiError::internal_with_log("Access token cleanup failed", &e))?;
    token_results.insert("access_tokens_deleted".to_string(), json!(access_tokens));

    let refresh_tokens = state.services.admin.refresh_token_service.cleanup_expired_tokens().await?;
    token_results.insert("refresh_tokens_deleted".to_string(), json!(refresh_tokens));

    Ok(Json(Value::Object(token_results)))
}
