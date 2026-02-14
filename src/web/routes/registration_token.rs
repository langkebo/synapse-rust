use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
    Router,
    routing::{get, post, put, delete},
};
use serde::{Deserialize, Serialize};

use crate::common::ApiError;
use crate::storage::registration_token::{
    CreateRegistrationTokenRequest, UpdateRegistrationTokenRequest,
    RegistrationToken, RegistrationTokenUsage, RoomInvite,
};
use crate::web::routes::AuthenticatedUser;
use crate::web::routes::AppState;

#[derive(Debug, Deserialize)]
pub struct QueryParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTokenBody {
    pub token: Option<String>,
    pub token_type: Option<String>,
    pub description: Option<String>,
    pub max_uses: Option<i32>,
    pub expires_at: Option<i64>,
    pub allowed_email_domains: Option<Vec<String>>,
    pub allowed_user_ids: Option<Vec<String>>,
    pub auto_join_rooms: Option<Vec<String>>,
    pub display_name: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTokenBody {
    pub description: Option<String>,
    pub max_uses: Option<i32>,
    pub is_active: Option<bool>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateBatchBody {
    pub count: i32,
    pub description: Option<String>,
    pub expires_at: Option<i64>,
    pub allowed_email_domains: Option<Vec<String>>,
    pub auto_join_rooms: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRoomInviteBody {
    pub room_id: String,
    pub invitee_email: Option<String>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UseRoomInviteBody {
    pub invitee_user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct RevokeInviteBody {
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub id: i64,
    pub token: String,
    pub token_type: String,
    pub description: Option<String>,
    pub max_uses: i32,
    pub current_uses: i32,
    pub remaining_uses: i32,
    pub is_used: bool,
    pub is_active: bool,
    pub expires_at: Option<i64>,
    pub created_ts: i64,
    pub last_used_ts: Option<i64>,
}

impl From<RegistrationToken> for TokenResponse {
    fn from(t: RegistrationToken) -> Self {
        let remaining = if t.max_uses > 0 {
            (t.max_uses - t.current_uses).max(0)
        } else {
            i32::MAX
        };

        Self {
            id: t.id,
            token: t.token,
            token_type: t.token_type,
            description: t.description,
            max_uses: t.max_uses,
            current_uses: t.current_uses,
            remaining_uses: remaining,
            is_used: t.is_used,
            is_active: t.is_active,
            expires_at: t.expires_at,
            created_ts: t.created_ts,
            last_used_ts: t.last_used_ts,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TokenUsageResponse {
    pub id: i64,
    pub token_id: i64,
    pub user_id: String,
    pub username: Option<String>,
    pub email: Option<String>,
    pub used_ts: i64,
    pub success: bool,
}

impl From<RegistrationTokenUsage> for TokenUsageResponse {
    fn from(u: RegistrationTokenUsage) -> Self {
        Self {
            id: u.id,
            token_id: u.token_id,
            user_id: u.user_id,
            username: u.username,
            email: u.email,
            used_ts: u.used_ts,
            success: u.success,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RoomInviteResponse {
    pub id: i64,
    pub invite_code: String,
    pub room_id: String,
    pub inviter_user_id: String,
    pub invitee_email: Option<String>,
    pub is_used: bool,
    pub is_revoked: bool,
    pub expires_at: Option<i64>,
    pub created_ts: i64,
}

impl From<RoomInvite> for RoomInviteResponse {
    fn from(i: RoomInvite) -> Self {
        Self {
            id: i.id,
            invite_code: i.invite_code,
            room_id: i.room_id,
            inviter_user_id: i.inviter_user_id,
            invitee_email: i.invitee_email,
            is_used: i.is_used,
            is_revoked: i.is_revoked,
            expires_at: i.expires_at,
            created_ts: i.created_ts,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BatchResponse {
    pub batch_id: String,
    pub token_count: i32,
    pub tokens: Vec<String>,
}

pub async fn create_token(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CreateTokenBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateRegistrationTokenRequest {
        token: body.token,
        token_type: body.token_type,
        description: body.description,
        max_uses: body.max_uses,
        expires_at: body.expires_at,
        created_by: Some(_auth_user.user_id.clone()),
        allowed_email_domains: body.allowed_email_domains,
        allowed_user_ids: body.allowed_user_ids,
        auto_join_rooms: body.auto_join_rooms,
        display_name: body.display_name,
        email: body.email,
    };

    let token = state.services.registration_token_service.create_token(request).await?;

    Ok((StatusCode::CREATED, Json(TokenResponse::from(token))))
}

pub async fn get_token(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let token = state.services.registration_token_service.get_token(&token).await?
        .ok_or_else(|| ApiError::not_found("Token not found"))?;

    Ok(Json(TokenResponse::from(token)))
}

pub async fn get_token_by_id(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    let token = state.services.registration_token_service.get_token_by_id(id).await?
        .ok_or_else(|| ApiError::not_found("Token not found"))?;

    Ok(Json(TokenResponse::from(token)))
}

pub async fn update_token(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<UpdateTokenBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = UpdateRegistrationTokenRequest {
        description: body.description,
        max_uses: body.max_uses,
        is_active: body.is_active,
        expires_at: body.expires_at,
    };

    let token = state.services.registration_token_service.update_token(id, request).await?;

    Ok(Json(TokenResponse::from(token)))
}

pub async fn delete_token(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    state.services.registration_token_service.delete_token(id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn deactivate_token(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    state.services.registration_token_service.deactivate_token(id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_all_tokens(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let tokens = state.services.registration_token_service.get_all_tokens(limit, offset).await?;

    let response: Vec<TokenResponse> = tokens.into_iter().map(TokenResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_active_tokens(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let tokens = state.services.registration_token_service.get_active_tokens().await?;

    let response: Vec<TokenResponse> = tokens.into_iter().map(TokenResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_token_usage(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let usage = state.services.registration_token_service.get_token_usage(id).await?;

    let response: Vec<TokenUsageResponse> = usage.into_iter().map(TokenUsageResponse::from).collect();

    Ok(Json(response))
}

pub async fn validate_token(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let result = state.services.registration_token_service.validate_token(&token).await?;

    Ok(Json(serde_json::json!({
        "valid": result.is_valid,
        "error": result.error_message,
    })))
}

pub async fn create_batch(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CreateBatchBody>,
) -> Result<impl IntoResponse, ApiError> {
    let (batch_id, tokens) = state.services.registration_token_service.create_batch(
        body.count,
        body.description,
        body.expires_at,
        Some(_auth_user.user_id.clone()),
        body.allowed_email_domains,
        body.auto_join_rooms,
    ).await?;

    let response = BatchResponse {
        batch_id,
        token_count: body.count,
        tokens,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn cleanup_expired(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let count = state.services.registration_token_service.cleanup_expired_tokens().await?;

    Ok(Json(serde_json::json!({
        "tokens_cleaned": count,
    })))
}

pub async fn create_room_invite(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CreateRoomInviteBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = crate::storage::registration_token::CreateRoomInviteRequest {
        room_id: body.room_id,
        inviter_user_id: _auth_user.user_id.clone(),
        invitee_email: body.invitee_email,
        expires_at: body.expires_at,
    };

    let invite = state.services.registration_token_service.create_room_invite(request).await?;

    Ok((StatusCode::CREATED, Json(RoomInviteResponse::from(invite))))
}

pub async fn get_room_invite(
    State(state): State<AppState>,
    Path(invite_code): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let invite = state.services.registration_token_service.get_room_invite(&invite_code).await?
        .ok_or_else(|| ApiError::not_found("Room invite not found"))?;

    Ok(Json(RoomInviteResponse::from(invite)))
}

pub async fn use_room_invite(
    State(state): State<AppState>,
    Path(invite_code): Path<String>,
    Json(body): Json<UseRoomInviteBody>,
) -> Result<impl IntoResponse, ApiError> {
    state.services.registration_token_service.use_room_invite(&invite_code, &body.invitee_user_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn revoke_room_invite(
    State(state): State<AppState>,
    Path(invite_code): Path<String>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<RevokeInviteBody>,
) -> Result<impl IntoResponse, ApiError> {
    state.services.registration_token_service.revoke_room_invite(&invite_code, &body.reason).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub fn create_registration_token_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/registration_tokens", post(create_token))
        .route("/_synapse/admin/v1/registration_tokens", get(get_all_tokens))
        .route("/_synapse/admin/v1/registration_tokens/active", get(get_active_tokens))
        .route("/_synapse/admin/v1/registration_tokens/cleanup", post(cleanup_expired))
        .route("/_synapse/admin/v1/registration_tokens/batch", post(create_batch))
        .route("/_synapse/admin/v1/registration_tokens/{token}", get(get_token))
        .route("/_synapse/admin/v1/registration_tokens/{token}/validate", get(validate_token))
        .route("/_synapse/admin/v1/registration_tokens/id/{id}", get(get_token_by_id))
        .route("/_synapse/admin/v1/registration_tokens/id/{id}", put(update_token))
        .route("/_synapse/admin/v1/registration_tokens/id/{id}", delete(delete_token))
        .route("/_synapse/admin/v1/registration_tokens/id/{id}/deactivate", post(deactivate_token))
        .route("/_synapse/admin/v1/registration_tokens/id/{id}/usage", get(get_token_usage))
        .route("/_synapse/admin/v1/room_invites", post(create_room_invite))
        .route("/_synapse/admin/v1/room_invites/{invite_code}", get(get_room_invite))
        .route("/_synapse/admin/v1/room_invites/{invite_code}/use", post(use_room_invite))
        .route("/_synapse/admin/v1/room_invites/{invite_code}/revoke", post(revoke_room_invite))
        .with_state(state)
}
