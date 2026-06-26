use super::audit::{record_audit_event, resolve_request_id};
use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn create_security_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/users/{user_id}/shadow_ban", post(shadow_ban_user))
        .route("/_synapse/admin/v1/users/{user_id}/shadow_ban", delete(unshadow_ban_user))
        .route("/_synapse/admin/v1/users/{user_id}/rate_limit", get(get_user_rate_limit))
        .route("/_synapse/admin/v1/users/{user_id}/rate_limit", put(set_user_rate_limit))
        .route("/_synapse/admin/v1/users/{user_id}/rate_limit", delete(delete_user_rate_limit))
        .route("/_synapse/admin/v1/users/{user_id}/override_ratelimit", get(get_user_override_rate_limit))
        .route("/_synapse/admin/v1/users/{user_id}/override_ratelimit", post(set_user_override_rate_limit))
        .route("/_synapse/admin/v1/users/{user_id}/override_ratelimit", delete(delete_user_override_rate_limit))
}

pub fn admin_security_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::POST, "/_synapse/admin/v1/users/{user_id}/shadow_ban"),
        (Method::DELETE, "/_synapse/admin/v1/users/{user_id}/shadow_ban"),
        (Method::GET, "/_synapse/admin/v1/users/{user_id}/rate_limit"),
        (Method::PUT, "/_synapse/admin/v1/users/{user_id}/rate_limit"),
        (Method::DELETE, "/_synapse/admin/v1/users/{user_id}/rate_limit"),
        (Method::GET, "/_synapse/admin/v1/users/{user_id}/override_ratelimit"),
        (Method::POST, "/_synapse/admin/v1/users/{user_id}/override_ratelimit"),
        (Method::DELETE, "/_synapse/admin/v1/users/{user_id}/override_ratelimit"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "admin::security"))
    .collect()
}

#[derive(Debug, Deserialize)]
pub struct RateLimitRequest {
    pub messages_per_second: Option<f64>,
    pub burst_count: Option<i32>,
}

async fn ensure_user_exists(state: &AppState, user_id: &str) -> Result<(), ApiError> {
    let user = state.services.account.account_identity_service.get_user_by_identifier(user_id).await?;

    if user.is_none() {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    Ok(())
}

#[axum::debug_handler]
pub async fn shadow_ban_user(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    state.services.admin.security.admin_security_service.set_shadow_ban(&user_id, true).await?;

    record_audit_event(
        &state,
        &admin.user_id,
        "admin.user.shadow_ban",
        "user",
        &user_id,
        resolve_request_id(&headers),
        json!({ "is_shadow_banned": true }),
    )
    .await?;

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn unshadow_ban_user(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    state.services.admin.security.admin_security_service.set_shadow_ban(&user_id, false).await?;

    record_audit_event(
        &state,
        &admin.user_id,
        "admin.user.unshadow_ban",
        "user",
        &user_id,
        resolve_request_id(&headers),
        json!({ "is_shadow_banned": false }),
    )
    .await?;

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_user_rate_limit(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_user_exists(&state, &user_id).await?;

    let limit = state.services.admin.security.admin_security_service.get_user_rate_limit(&user_id).await?;

    Ok(Json(json!({
        "messages_per_second": limit.messages_per_second,
        "burst_count": limit.burst_count
    })))
}

#[axum::debug_handler]
pub async fn set_user_rate_limit(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RateLimitRequest>,
) -> Result<Json<Value>, ApiError> {
    ensure_user_exists(&state, &user_id).await?;

    let messages_per_second = body.messages_per_second.unwrap_or(5.0);
    let burst_count = body.burst_count.unwrap_or(10);

    let limit = state
        .services
        .admin
        .security.admin_security_service
        .set_user_rate_limit(&user_id, messages_per_second, burst_count)
        .await?;

    record_audit_event(
        &state,
        &admin.user_id,
        "admin.user.rate_limit.set",
        "user",
        &user_id,
        resolve_request_id(&headers),
        json!({
            "messages_per_second": messages_per_second,
            "burst_count": burst_count
        }),
    )
    .await?;

    Ok(Json(json!({
        "messages_per_second": limit.messages_per_second,
        "burst_count": limit.burst_count
    })))
}

#[axum::debug_handler]
pub async fn delete_user_rate_limit(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    ensure_user_exists(&state, &user_id).await?;

    state.services.admin.security.admin_security_service.delete_user_rate_limit(&user_id).await?;

    record_audit_event(
        &state,
        &admin.user_id,
        "admin.user.rate_limit.delete",
        "user",
        &user_id,
        resolve_request_id(&headers),
        json!({}),
    )
    .await?;

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_user_override_rate_limit(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    get_user_rate_limit(admin, State(state), Path(user_id)).await
}

#[axum::debug_handler]
pub async fn set_user_override_rate_limit(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    body: Json<RateLimitRequest>,
) -> Result<Json<Value>, ApiError> {
    set_user_rate_limit(admin, State(state), Path(user_id), headers, body).await
}

#[axum::debug_handler]
pub async fn delete_user_override_rate_limit(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    delete_user_rate_limit(admin, State(state), Path(user_id), headers).await
}
