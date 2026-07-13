use crate::web::routes::context::AuthContext;
use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use validator::Validate;

pub async fn register_guest(State(ctx): State<AuthContext>) -> Result<Json<Value>, ApiError> {
    if !ctx.config.server.enable_registration {
        return Err(ApiError::forbidden("Registration is disabled".to_string()));
    }
    let (user, device_id, access_token) = ctx.credential_auth.register_guest_account().await?;

    Ok(Json(json!({
        "access_token": access_token,
        "device_id": device_id,
        "user_id": user.user_id,
        "expires_in": ctx.token_auth.token_expiry(),
    })))
}

pub async fn get_guest_info(
    State(ctx): State<AuthContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    ctx.credential_auth.require_guest_user(&auth_user.user_id).await?;
    Ok(Json(json!({
        "user_id": auth_user.user_id,
        "is_guest": true,
    })))
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpgradeGuestRequest {
    #[validate(length(min = 1, max = 255))]
    username: String,
    #[validate(length(min = 8, max = 512))]
    password: String,
}

pub async fn upgrade_guest(
    State(ctx): State<AuthContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<UpgradeGuestRequest>,
) -> Result<Json<Value>, ApiError> {
    body.validate().map_err(|e| ApiError::bad_request(format!("Validation error: {e}")))?;

    let username = &body.username;
    let password = &body.password;
    let access_token = ctx
        .credential_auth
        .upgrade_guest_account(&auth_user.user_id, auth_user.device_id.as_deref(), username, password)
        .await?;

    Ok(Json(json!({
        "success": true,
        "user_id": auth_user.user_id,
        "access_token": access_token,
    })))
}

pub fn create_guest_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v3/register/guest", post(register_guest))
        .route("/_matrix/client/v3/account/guest", get(get_guest_info))
        .route("/_matrix/client/v3/account/guest/upgrade", post(upgrade_guest))
        .with_state(state)
}

pub fn guest_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::POST, "/_matrix/client/v3/register/guest"),
        (Method::GET, "/_matrix/client/v3/account/guest"),
        (Method::POST, "/_matrix/client/v3/account/guest/upgrade"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "guest"))
    .collect()
}
