use crate::common::error::ApiError;
use synapse_services::captcha_service::{SendCaptchaRequest, VerifyCaptchaRequest};
use crate::web::routes::{AdminUser, AppState};
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
    #[serde(rename = "verified")]
    pub is_verified: bool,
}

pub async fn send_captcha(
    State(state): State<AppState>,
    Json(body): Json<SendCaptchaBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request =
        SendCaptchaRequest { captcha_type: body.captcha_type, target: body.target, template_name: body.template_name };

    let response = state.services.admin.security.captcha_service.send_captcha(request, None, None).await?;

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
    let request = VerifyCaptchaRequest { captcha_id: body.captcha_id, code: body.code };

    let verified = state.services.admin.security.captcha_service.verify_captcha(request).await?;

    Ok(Json(VerifyResponse { is_verified: verified }))
}

pub async fn get_captcha_status(
    State(state): State<AppState>,
    Query(query): Query<CaptchaIdQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let captcha = state
        .services
        .admin
        .security
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

#[derive(Debug, Deserialize)]
pub struct CaptchaIdQuery {
    pub captcha_id: String,
}

pub async fn cleanup_expired(State(state): State<AppState>, _admin: AdminUser) -> Result<impl IntoResponse, ApiError> {
    let count = state.services.admin.security.captcha_service.cleanup_expired().await?;

    Ok(Json(serde_json::json!({
        "cleaned_count": count,
        "message": format!("Cleaned up {} expired captchas", count)
    })))
}

pub fn create_captcha_router(state: AppState) -> axum::Router<AppState> {
    use axum::routing::*;

    let public_routes = axum::Router::new()
        .route("/_matrix/client/r0/register/captcha/send", post(send_captcha))
        .route("/_matrix/client/r0/register/captcha/verify", post(verify_captcha))
        .route("/_matrix/client/r0/register/captcha/status", get(get_captcha_status))
        .route("/_matrix/client/v3/register/captcha/send", post(send_captcha))
        .route("/_matrix/client/v3/register/captcha/verify", post(verify_captcha))
        .route("/_matrix/client/v3/register/captcha/status", get(get_captcha_status))
        .route("/_matrix/client/v3/register/captcha/clean", delete(cleanup_expired));

    let admin_routes = axum::Router::new()
        .route("/_synapse/admin/v1/captcha/cleanup", post(cleanup_expired))
        .route_layer(axum::middleware::from_fn_with_state(state, crate::web::middleware::admin_auth_middleware));

    public_routes.merge(admin_routes)
}

pub fn captcha_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::POST, "/_matrix/client/r0/register/captcha/send"),
        (Method::POST, "/_matrix/client/r0/register/captcha/verify"),
        (Method::GET, "/_matrix/client/r0/register/captcha/status"),
        (Method::POST, "/_matrix/client/v3/register/captcha/send"),
        (Method::POST, "/_matrix/client/v3/register/captcha/verify"),
        (Method::GET, "/_matrix/client/v3/register/captcha/status"),
        (Method::POST, "/_synapse/admin/v1/captcha/cleanup"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "captcha"))
    .collect()
}
