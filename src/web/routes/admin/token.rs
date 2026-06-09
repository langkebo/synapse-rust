use crate::common::constants::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use crate::common::ApiError;
use crate::services::registration_token_service::decode_registration_token_cursor;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn create_token_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/registration_tokens", get(get_registration_tokens))
        .route("/_synapse/admin/v1/registration_tokens", post(create_registration_token))
        .route("/_synapse/admin/v1/registration_tokens/{token}", get(get_registration_token))
        .route("/_synapse/admin/v1/registration_tokens/{token}", delete(delete_registration_token))
        .route("/_synapse/admin/v1/registration_tokens/{token}", post(update_registration_token))
        .route("/_synapse/admin/v1/users/{user_id}/tokens", get(get_user_tokens))
        .route("/_synapse/admin/v1/users/{user_id}/tokens/{token_id}", delete(delete_user_token))
        .route("/_synapse/admin/v1/users/{user_id}/refresh_tokens", get(get_user_refresh_tokens))
        .route("/_synapse/admin/v1/users/{user_id}/refresh_tokens/{token_id}", delete(delete_refresh_token))
}

pub fn admin_token_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::GET, "/_synapse/admin/v1/registration_tokens"),
        (Method::POST, "/_synapse/admin/v1/registration_tokens"),
        (Method::GET, "/_synapse/admin/v1/registration_tokens/{token}"),
        (Method::DELETE, "/_synapse/admin/v1/registration_tokens/{token}"),
        (Method::POST, "/_synapse/admin/v1/registration_tokens/{token}"),
        (Method::GET, "/_synapse/admin/v1/users/{user_id}/tokens"),
        (Method::DELETE, "/_synapse/admin/v1/users/{user_id}/tokens/{token_id}"),
        (Method::GET, "/_synapse/admin/v1/users/{user_id}/refresh_tokens"),
        (Method::DELETE, "/_synapse/admin/v1/users/{user_id}/refresh_tokens/{token_id}"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "admin::token"))
    .collect()
}

async fn ensure_user_exists(state: &AppState, user_id: &str) -> Result<(), ApiError> {
    let user = state
        .services
        .user_storage
        .get_user_by_identifier(user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    if user.is_none() {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    pub token: Option<String>,
    pub uses_allowed: Option<i32>,
    pub expiry_time: Option<i64>,
    pub length: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTokenRequest {
    pub uses_allowed: Option<i32>,
    pub expiry_time: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct RegistrationTokenListQuery {
    pub limit: Option<i64>,
    pub from: Option<String>,
}

#[axum::debug_handler]
pub async fn get_registration_tokens(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<RegistrationTokenListQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let from = decode_registration_token_cursor(query.from.as_deref());

    if query.from.is_some() && from.is_none() {
        return Err(ApiError::bad_request("Invalid from cursor".to_string()));
    }

    let (tokens, next_batch) = state.services.admin.registration_token_service.get_all_tokens(limit, from).await?;

    let token_list: Vec<Value> = tokens
        .iter()
        .map(|row| {
            let max_uses = row.max_uses;
            let uses_allowed = if max_uses == 0 { None } else { Some(max_uses) };
            json!({
                "token": row.token,
                "uses_allowed": uses_allowed,
                "pending": 0,
                "completed": row.uses_count,
                "expiry_time": row.expires_at,
                "created_ts": row.created_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "registration_tokens": token_list,
        "next_batch": next_batch
    })))
}

#[axum::debug_handler]
pub async fn create_registration_token(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<Json<Value>, ApiError> {
    let token = body.token.unwrap_or_else(|| crate::common::random_string(body.length.unwrap_or(16)));
    let max_uses = body.uses_allowed.unwrap_or(0);
    let registration_token = state
        .services
        .admin
        .admin_token_service
        .create_registration_token(Some(token), max_uses, body.expiry_time, &admin.user_id)
        .await?;

    Ok(Json(json!({
        "token": registration_token.token,
        "uses_allowed": if registration_token.max_uses == 0 { None } else { Some(registration_token.max_uses) },
        "pending": 0,
        "completed": registration_token.uses_count,
        "expiry_time": registration_token.expires_at,
        "created_ts": registration_token.created_ts
    })))
}

#[axum::debug_handler]
pub async fn get_registration_token(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = state
        .services
        .admin
        .admin_token_service
        .get_registration_token(&token)
        .await?;

    match result {
        Some(row) => {
            Ok(Json(json!({
                "token": &row.token,
                "uses_allowed": if row.max_uses == 0 { None } else { Some(row.max_uses) },
                "pending": 0,
                "completed": row.uses_count,
                "expiry_time": row.expires_at,
                "created_ts": row.created_ts
            })))
        }
        None => Err(ApiError::not_found("Token not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn delete_registration_token(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .admin
        .admin_token_service
        .delete_registration_token(&token)
        .await?;

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn update_registration_token(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(body): Json<UpdateTokenRequest>,
) -> Result<Json<Value>, ApiError> {
    let row = state
        .services
        .admin
        .admin_token_service
        .update_registration_token(&token, body.uses_allowed, body.expiry_time)
        .await?;

    Ok(Json(json!({
        "token": &row.token,
        "uses_allowed": if row.max_uses == 0 { None } else { Some(row.max_uses) },
        "pending": 0,
        "completed": row.uses_count,
        "expiry_time": row.expires_at,
        "created_ts": row.created_ts
    })))
}

#[axum::debug_handler]
pub async fn get_user_tokens(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_user_exists(&state, &user_id).await?;

    let tokens = state
        .services
        .admin
        .admin_token_service
        .get_user_access_tokens(&user_id)
        .await?;

    let token_list: Vec<Value> = tokens
        .iter()
        .map(|row| {
            json!({
                "id": row.id,
                "device_id": row.device_id,
                "created_ts": row.created_ts,
                "expires_at": row.expires_at,
                "is_revoked": row.is_revoked
            })
        })
        .collect();

    Ok(Json(json!({ "tokens": token_list, "total": token_list.len() })))
}

#[axum::debug_handler]
pub async fn delete_user_token(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((user_id, token_id)): Path<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    ensure_user_exists(&state, &user_id).await?;

    state
        .services
        .admin
        .admin_token_service
        .delete_user_access_token(&user_id, token_id)
        .await?;

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_user_refresh_tokens(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_user_exists(&state, &user_id).await?;

    let tokens = state
        .services
        .admin
        .admin_token_service
        .get_user_refresh_tokens(&user_id)
        .await?;

    let token_list: Vec<Value> = tokens
        .iter()
        .map(|row| {
            json!({
                "id": row.id,
                "device_id": row.device_id,
                "created_ts": row.created_ts,
                "expires_at": row.expires_at,
                "is_revoked": row.is_revoked
            })
        })
        .collect();

    Ok(Json(json!({ "refresh_tokens": token_list, "total": token_list.len() })))
}

#[axum::debug_handler]
pub async fn delete_refresh_token(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((user_id, token_id)): Path<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    ensure_user_exists(&state, &user_id).await?;

    state
        .services
        .admin
        .admin_token_service
        .delete_refresh_token(&user_id, token_id)
        .await?;

    Ok(Json(json!({})))
}
