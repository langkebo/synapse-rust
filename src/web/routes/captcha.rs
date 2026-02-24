use crate::common::error::ApiError;
use crate::services::captcha_service::{SendCaptchaRequest, VerifyCaptchaRequest};
use crate::web::routes::AppState;
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct SendCaptchaQuery {
    pub captcha_type: String,
    pub target: String,
    pub template_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SendCaptchaBody {
    pub captcha_type: String,
    pub target: String,
    pub template_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyCaptchaBody {
    pub captcha_id: String,
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct CaptchaResponse {
    pub captcha_id: String,
    pub expires_in: i64,
    pub captcha_type: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub verified: bool,
}

pub async fn send_captcha(
    State(state): State<AppState>,
    Json(body): Json<SendCaptchaBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = SendCaptchaRequest {
        captcha_type: body.captcha_type,
        target: body.target,
        template_name: body.template_name,
    };

    let response = state
        .services
        .captcha_service
        .send_captcha(request, None, None)
        .await?;

    Ok(Json(CaptchaResponse {
        captcha_id: response.captcha_id,
        expires_in: response.expires_in,
        captcha_type: response.captcha_type,
    }))
}

pub async fn verify_captcha(
    State(state): State<AppState>,
    Json(body): Json<VerifyCaptchaBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = VerifyCaptchaRequest {
        captcha_id: body.captcha_id,
        code: body.code,
    };

    let verified = state
        .services
        .captcha_service
        .verify_captcha(request)
        .await?;

    Ok(Json(VerifyResponse { verified }))
}

pub async fn get_captcha_status(
    State(state): State<AppState>,
    Query(query): Query<CaptchaIdQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let captcha = state
        .services
        .captcha_service
        .get_captcha(&query.captcha_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Captcha not found"))?;

    Ok(Json(serde_json::json!({
        "captcha_id": captcha.captcha_id,
        "captcha_type": captcha.captcha_type,
        "target": captcha.target,
        "status": captcha.status,
        "attempt_count": captcha.attempt_count,
        "max_attempts": captcha.max_attempts,
        "expires_at": captcha.expires_at,
        "created_at": captcha.created_at,
    })))
}

#[derive(Debug, Deserialize)]
pub struct CaptchaIdQuery {
    pub captcha_id: String,
}

pub async fn cleanup_expired(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let count = state.services.captcha_service.cleanup_expired().await?;

    Ok(Json(serde_json::json!({
        "cleaned_count": count,
        "message": format!("Cleaned up {} expired captchas", count)
    })))
}

pub fn create_captcha_router() -> axum::Router<AppState> {
    use axum::routing::*;

    axum::Router::new()
        .route(
            "/_matrix/client/r0/register/captcha/send",
            post(send_captcha),
        )
        .route(
            "/_matrix/client/r0/register/captcha/verify",
            post(verify_captcha),
        )
        .route(
            "/_matrix/client/r0/register/captcha/status",
            get(get_captcha_status),
        )
        .route("/_synapse/admin/v1/captcha/cleanup", post(cleanup_expired))
}
