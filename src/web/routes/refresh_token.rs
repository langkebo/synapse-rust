use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
    Router,
    routing::{get, post, delete},
};
use serde::{Deserialize, Serialize};

use crate::common::ApiError;
use crate::storage::refresh_token::{
    RefreshToken, RefreshTokenUsage, RefreshTokenStats,
};
use crate::web::routes::{AdminUser, AppState};

#[derive(Debug, Deserialize)]
pub struct QueryLimit {
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

#[derive(Debug, Deserialize)]
pub struct RevokeTokenRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RevokeAllTokensRequest {
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub token_type: String,
}

#[derive(Debug, Serialize)]
pub struct RefreshTokenResponse {
    pub id: i64,
    pub user_id: String,
    pub device_id: Option<String>,
    pub scope: Option<String>,
    pub expires_at: Option<i64>,
    pub created_ts: i64,
    pub last_used_ts: Option<i64>,
    pub use_count: i32,
    pub is_revoked: bool,
}

impl From<RefreshToken> for RefreshTokenResponse {
    fn from(t: RefreshToken) -> Self {
        Self {
            id: t.id,
            user_id: t.user_id,
            device_id: t.device_id,
            scope: t.scope,
            expires_at: t.expires_at,
            created_ts: t.created_ts,
            last_used_ts: t.last_used_ts,
            use_count: t.use_count,
            is_revoked: t.is_revoked,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TokenUsageResponse {
    pub id: i64,
    pub refresh_token_id: i64,
    pub used_ts: i64,
    pub ip_address: Option<String>,
    pub success: bool,
}

impl From<RefreshTokenUsage> for TokenUsageResponse {
    fn from(u: RefreshTokenUsage) -> Self {
        Self {
            id: u.id,
            refresh_token_id: u.refresh_token_id,
            used_ts: u.used_ts,
            ip_address: u.ip_address,
            success: u.success,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TokenStatsResponse {
    pub user_id: String,
    pub total_tokens: i64,
    pub active_tokens: i64,
    pub revoked_tokens: i64,
    pub expired_tokens: i64,
    pub total_uses: i64,
}

impl From<RefreshTokenStats> for TokenStatsResponse {
    fn from(s: RefreshTokenStats) -> Self {
        Self {
            user_id: s.user_id,
            total_tokens: s.total_tokens,
            active_tokens: s.active_tokens,
            revoked_tokens: s.revoked_tokens,
            expired_tokens: s.expired_tokens,
            total_uses: s.total_uses,
        }
    }
}

pub async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshTokenRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let new_access_token = uuid::Uuid::new_v4().to_string();

    let (new_refresh_token, _token_record) = state.services.refresh_token_service
        .refresh_access_token(&body.refresh_token, &new_access_token, None, None)
        .await?;

    let response = TokenResponse {
        access_token: new_access_token,
        refresh_token: new_refresh_token,
        expires_in: state.services.refresh_token_service.get_default_expiry_ms() / 1000,
        token_type: "Bearer".to_string(),
    };

    Ok(Json(response))
}

pub async fn get_user_tokens(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let tokens = state.services.refresh_token_service.get_user_tokens(&user_id).await?;

    let response: Vec<RefreshTokenResponse> = tokens.into_iter().map(RefreshTokenResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_active_tokens(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let tokens = state.services.refresh_token_service.get_active_tokens(&user_id).await?;

    let response: Vec<RefreshTokenResponse> = tokens.into_iter().map(RefreshTokenResponse::from).collect();

    Ok(Json(response))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    _auth_user: AdminUser,
    Json(body): Json<RevokeTokenRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _token = state.services.refresh_token_storage.get_token_by_id(id).await
        .map_err(|e| ApiError::internal(format!("Failed to get token: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Token not found"))?;

    let reason = body.reason.unwrap_or_else(|| "Admin requested".to_string());

    state.services.refresh_token_service.revoke_token_by_id(id, &reason).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn revoke_all_tokens(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    _auth_user: AdminUser,
    Json(body): Json<RevokeAllTokensRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let count = state.services.refresh_token_service.revoke_all_user_tokens(&user_id, &body.reason).await?;

    Ok(Json(serde_json::json!({
        "user_id": user_id,
        "tokens_revoked": count,
    })))
}

pub async fn get_token_stats(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let stats = state.services.refresh_token_service.get_user_stats(&user_id).await?
        .ok_or_else(|| ApiError::not_found("Token stats not found"))?;

    Ok(Json(TokenStatsResponse::from(stats)))
}

pub async fn get_usage_history(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    _auth_user: AdminUser,
    Query(query): Query<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(50);
    let history = state.services.refresh_token_service.get_usage_history(&user_id, limit).await?;

    let response: Vec<TokenUsageResponse> = history.into_iter().map(TokenUsageResponse::from).collect();

    Ok(Json(response))
}

pub async fn cleanup_expired_tokens(
    State(state): State<AppState>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let count = state.services.refresh_token_service.cleanup_expired_tokens().await?;

    Ok(Json(serde_json::json!({
        "tokens_cleaned": count,
    })))
}

pub async fn delete_token(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let token = state.services.refresh_token_storage.get_token_by_id(id).await
        .map_err(|e| ApiError::internal(format!("Failed to get token: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Token not found"))?;

    state.services.refresh_token_service.delete_token(&token.token_hash).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub fn create_refresh_token_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v3/refresh", post(refresh))
        .route("/_synapse/admin/v1/users/{user_id}/tokens", get(get_user_tokens))
        .route("/_synapse/admin/v1/users/{user_id}/tokens/active", get(get_active_tokens))
        .route("/_synapse/admin/v1/users/{user_id}/tokens/revoke_all", post(revoke_all_tokens))
        .route("/_synapse/admin/v1/users/{user_id}/tokens/stats", get(get_token_stats))
        .route("/_synapse/admin/v1/users/{user_id}/tokens/usage", get(get_usage_history))
        .route("/_synapse/admin/v1/tokens/{id}", delete(delete_token))
        .route("/_synapse/admin/v1/tokens/{id}/revoke", post(revoke_token))
        .route("/_synapse/admin/v1/tokens/cleanup", post(cleanup_expired_tokens))
        .with_state(state)
}
