use crate::routes::context::AuthContext;
use crate::routes::MatrixJson;
use crate::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_services::uia_service::UiaFlow;
use validator::Validate;

/// Body of `POST /_matrix/client/v3/register/guest`.
///
/// Both fields are optional. Before D-10 the handler had no body extractor at
/// all, so a client that pre-allocated an E2EE `device_id` silently got a
/// server-minted one and lost the device-trust chain it had already set up.
///
/// Unknown fields are ignored (not `deny_unknown_fields`): the spec tells
/// servers to ignore unrecognised request fields, and a hard 400 here would
/// break guest registration for clients that send vendor extensions.
#[derive(Debug, Default, Deserialize)]
pub struct GuestRegisterBody {
    device_id: Option<String>,
    initial_device_display_name: Option<String>,
}

/// See [`register_guest`].
pub async fn register_guest(
    State(ctx): State<AuthContext>,
    body: Option<MatrixJson<GuestRegisterBody>>,
) -> Result<Json<Value>, ApiError> {
    if !ctx.config.server.enable_registration {
        return Err(ApiError::forbidden("Registration is disabled".to_string()));
    }

    let body = body.map(|MatrixJson(body)| body).unwrap_or_default();
    let (user, device_id, access_token) = ctx
        .credential_auth
        .register_guest_account(body.device_id.as_deref(), body.initial_device_display_name.as_deref())
        .await?;

    Ok(Json(json!({
        "access_token": access_token,
        "device_id": device_id,
        "user_id": user.user_id,
        "expires_in": ctx.token_auth.token_expiry(),
    })))
}

/// See [`get_guest_info`].
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

/// The `UpgradeGuestRequest` struct.
///
/// `auth` is the UIA envelope. It used to be absent (and `deny_unknown_fields`
/// turned it into a 400 when present), so a caller that followed the spec and
/// completed an auth stage had it silently discarded.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UpgradeGuestRequest {
    #[validate(length(min = 1, max = 255))]
    username: String,
    #[validate(length(min = 8, max = 512))]
    password: String,
    #[serde(default)]
    auth: Option<Value>,
}

/// See [`upgrade_guest`].
pub async fn upgrade_guest(
    State(ctx): State<AuthContext>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<UpgradeGuestRequest>,
) -> Result<Response, ApiError> {
    body.validate().map_err(|e| ApiError::bad_request(format!("Validation error: {e}")))?;

    // A guest has no password and no 3PIDs yet, so `m.login.password` and
    // `m.login.email.identity` cannot be completed. `m.login.dummy` is the one
    // stage the account can actually satisfy today; the point of the flow is
    // that the client goes through UIA (and gets a 401 + session when it
    // sends no `auth`) instead of the envelope being dropped on the floor.
    let flows = vec![UiaFlow { stages: vec!["m.login.dummy".to_string()] }];
    let auth = body.auth.clone().unwrap_or_else(|| json!({}));
    if let Err(uia_response) = ctx.uia_service.validate_auth(&auth, &auth_user.user_id, flows).await {
        return Ok((axum::http::StatusCode::UNAUTHORIZED, Json(uia_response)).into_response());
    }

    let access_token = ctx
        .credential_auth
        .upgrade_guest_account(&auth_user.user_id, auth_user.device_id.as_deref(), &body.username, &body.password)
        .await?;

    Ok(Json(json!({
        "success": true,
        "user_id": auth_user.user_id,
        "access_token": access_token,
    }))
    .into_response())
}

/// See [`create_guest_router`].
pub fn create_guest_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v3/register/guest", post(register_guest))
        .route("/_matrix/client/v3/account/guest", get(get_guest_info))
        .route("/_matrix/client/v3/account/guest/upgrade", post(upgrade_guest))
        .with_state(state)
}
