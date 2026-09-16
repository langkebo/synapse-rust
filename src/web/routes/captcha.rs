use crate::common::error::ApiError;
use crate::web::routes::context::AdminContext;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use synapse_services::captcha_service::{SendCaptchaRequest, VerifyCaptchaRequest};

/// The `SendCaptchaQuery` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendCaptchaQuery {
    /// The `captcha_type` field.
    pub captcha_type: String,
    /// The `target` field.
    pub target: String,
    /// The `template_name` field.
    pub template_name: Option<String>,
}

/// The `SendCaptchaBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendCaptchaBody {
    /// The `captcha_type` field.
    pub captcha_type: String,
    /// The `target` field.
    pub target: String,
    /// The `template_name` field.
    pub template_name: Option<String>,
}

/// The `VerifyCaptchaBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyCaptchaBody {
    /// The `captcha_id` field.
    pub captcha_id: String,
    /// The `code` field.
    pub code: String,
}

/// The `CaptchaResponse` struct.
#[derive(Debug, Serialize)]
pub struct CaptchaResponse {
    /// The `captcha_id` field.
    pub captcha_id: String,
    /// The `expires_in` field.
    pub expires_in: i64,
    /// The `captcha_type` field.
    pub captcha_type: String,
}

/// The `VerifyResponse` struct.
#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    #[serde(rename = "verified")]
    /// The `is_verified` field.
    pub is_verified: bool,
}

/// See [`send_captcha`].
pub async fn send_captcha(
    State(ctx): State<AdminContext>,
    Json(body): Json<SendCaptchaBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request =
        SendCaptchaRequest { captcha_type: body.captcha_type, target: body.target, template_name: body.template_name };

    let response = ctx.captcha_service.send_captcha(request, None, None).await?;

    Ok(Json(CaptchaResponse {
        captcha_id: response.captcha_id,
        expires_in: response.expires_in,
        captcha_type: response.captcha_type,
    }))
}

/// See [`verify_captcha`].
pub async fn verify_captcha(
    State(ctx): State<AdminContext>,
    Json(body): Json<VerifyCaptchaBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = VerifyCaptchaRequest { captcha_id: body.captcha_id, code: body.code };

    let verified = ctx.captcha_service.verify_captcha(request).await?;

    Ok(Json(VerifyResponse { is_verified: verified }))
}

/// See [`get_captcha_status`].
pub async fn get_captcha_status(
    State(ctx): State<AdminContext>,
    Query(query): Query<CaptchaIdQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let captcha = ctx
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
        "created_at": captcha.created_ts,
    })))
}

/// The `CaptchaIdQuery` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptchaIdQuery {
    /// The `captcha_id` field.
    pub captcha_id: String,
}

/// See [`cleanup_expired`].
pub async fn cleanup_expired(
    State(ctx): State<AdminContext>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let count = ctx.captcha_service.cleanup_expired().await?;

    Ok(Json(serde_json::json!({
        "cleaned_count": count,
        "message": format!("Cleaned up {} expired captchas", count)
    })))
}

/// See [`create_captcha_router`].
pub fn create_captcha_router(state: &AppState) -> axum::Router<AppState> {
    use axum::routing::*;

    let public_routes = axum::Router::new()
        .route("/_matrix/client/v3/register/captcha/send", post(send_captcha))
        .route("/_matrix/client/v3/register/captcha/verify", post(verify_captcha))
        .route("/_matrix/client/v3/register/captcha/status", get(get_captcha_status))
        .route("/_matrix/client/v3/register/captcha/clean", delete(cleanup_expired));

    let admin_routes = axum::Router::new()
        .route("/_synapse/admin/v1/captcha/cleanup", post(cleanup_expired))
        .route_layer(axum::middleware::from_fn_with_state(
        <crate::web::routes::context::AdminContext as axum::extract::FromRef<crate::web::routes::AppState>>::from_ref(
            state,
        ),
        crate::web::middleware::admin_auth_middleware,
    ));

    public_routes.merge(admin_routes)
}
