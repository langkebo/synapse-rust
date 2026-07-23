use axum::extract::State;
use axum::routing::{post, Router};
use axum::Json;
use serde::{Deserialize, Serialize};
use synapse_common::current_timestamp_millis;

use crate::web::routes::context::AuthContext;
use crate::web::routes::ApiError;

fn generate_token() -> String {
    let chars: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".chars().collect();
    (0..12).map(|_| chars[rand::Rng::random_range(&mut rand::rng(), 0..chars.len())]).collect()
}

pub fn create_threepid_router() -> Router<AuthContext> {
    Router::new().route("/requestToken", post(request_token)).route("/submitToken", post(submit_token))
}

#[derive(Debug, Deserialize)]
pub struct RequestTokenRequest {
    pub client_secret: String,
    pub email: String,
    pub send_attempt: Option<i32>,
    pub next_link: Option<String>,
    pub id_server: Option<String>,
    pub id_access_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RequestTokenResponse {
    pub sid: String,
    pub submit_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitTokenRequest {
    pub client_secret: String,
    pub token: String,
    pub sid: String,
}

#[derive(Debug, Serialize)]
pub struct SubmitTokenResponse {
    #[serde(rename = "success")]
    pub is_success: bool,
}

pub async fn request_token(
    State(ctx): State<AuthContext>,
    Json(req): Json<RequestTokenRequest>,
) -> Result<Json<RequestTokenResponse>, ApiError> {
    let email = req.email.trim().to_lowercase();

    if email.is_empty() || !email.contains('@') {
        return Err(ApiError::bad_request("Invalid email address"));
    }

    if req.client_secret.is_empty() {
        return Err(ApiError::bad_request("client_secret is required"));
    }

    let session_id = format!(
        "3pid_{}_{}",
        current_timestamp_millis(),
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("")
    );

    let token: String = generate_token();
    let now: i64 = current_timestamp_millis();
    let expires_at: i64 = now + 3_600_000; // 1 hour

    let _id: i64 = ctx
        .threepid_storage
        .create_validation_session(
            &session_id,
            "email",
            &email,
            &req.client_secret,
            &token,
            req.next_link.as_deref(),
            now,
            expires_at,
        )
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create validation session", &e))?;

    // In a full implementation, send email here
    // For now, return the token in the response for testing
    tracing::debug!("3PID validation session created: sid={}", session_id);

    let submit_url = req.next_link;

    Ok(Json(RequestTokenResponse { sid: session_id, submit_url }))
}

pub async fn submit_token(
    State(ctx): State<AuthContext>,
    Json(req): Json<SubmitTokenRequest>,
) -> Result<Json<SubmitTokenResponse>, ApiError> {
    if req.sid.is_empty() || req.client_secret.is_empty() || req.token.is_empty() {
        return Err(ApiError::bad_request("sid, client_secret, and token are required"));
    }

    let session: synapse_storage::threepid::ThreepidValidationSession = ctx
        .threepid_storage
        .get_validation_session(&req.sid, &req.client_secret, &req.token)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?
        .ok_or_else(|| ApiError::bad_request("Invalid or expired validation token"))?;

    ctx.threepid_storage
        .mark_validation_validated(session.id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to mark session validated", &e))?;

    tracing::info!("3PID validation successful: sid={}", session.session_id);

    Ok(Json(SubmitTokenResponse { is_success: true }))
}
