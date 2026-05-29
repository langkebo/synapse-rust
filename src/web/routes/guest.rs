use crate::storage::device::DeviceStorage;
use crate::storage::user::UserStorage;
use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use validator::Validate;

pub async fn register_guest(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    if !state.services.config.server.enable_registration {
        return Err(ApiError::forbidden("Registration is disabled".to_string()));
    }

    let guest_num = rand::random::<u64>();
    let username = format!("guest_{guest_num}");
    let user_id = format!("@{}:{}", username, state.services.server_name);
    let device_id = format!("guest_device_{guest_num}");

    let user = UserStorage::create_user(&state.services.user_storage, &user_id, &username, None, false)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create guest user", &e))?;

    sqlx::query(
        r"
        UPDATE users SET is_guest = TRUE WHERE user_id = $1
        ",
    )
    .bind(&user.user_id)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal_with_log("Failed to mark guest user", &e))?;

    DeviceStorage::create_device(&state.services.device_storage, &device_id, &user.user_id, Some("Guest Device"))
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create device", &e))?;

    let access_token = state
        .services
        .auth_service
        .generate_access_token(&user.user_id, &device_id, false)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to generate guest token", &e))?;

    Ok(Json(json!({
        "access_token": access_token,
        "device_id": device_id,
        "user_id": user.user_id,
        "expires_in": state.services.auth_service.token_expiry,
    })))
}

pub async fn get_guest_info(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user = UserStorage::get_user_by_id(&state.services.user_storage, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get user", &e))?;

    match user {
        Some(u) if u.is_guest => Ok(Json(json!({
            "user_id": auth_user.user_id,
            "is_guest": true,
        }))),
        _ => Err(ApiError::forbidden("User is not a guest".to_string())),
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpgradeGuestRequest {
    #[validate(length(min = 1, max = 255))]
    username: String,
    #[validate(length(min = 8, max = 512))]
    password: String,
}

pub async fn upgrade_guest(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<UpgradeGuestRequest>,
) -> Result<Json<Value>, ApiError> {
    body.validate().map_err(|e| ApiError::bad_request(format!("Validation error: {e}")))?;

    let username = &body.username;
    let password = &body.password;

    state.services.auth_service.validator.validate_username(username)?;
    state.services.auth_service.validator.validate_password(password)?;

    let user = UserStorage::get_user_by_id(&state.services.user_storage, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get user", &e))?;

    match user {
        Some(u) if u.is_guest => {
            let existing = UserStorage::get_user_by_username(&state.services.user_storage, username)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to check username", &e))?;

            if existing.is_some() {
                return Err(ApiError::conflict("Username already exists".to_string()));
            }

            let password_hash = state.services.auth_service.hash_password_for_storage(password).await?;

            sqlx::query(
                r"
                UPDATE users SET username = $1, is_guest = FALSE, password_hash = $2 WHERE user_id = $3
                ",
            )
            .bind(username)
            .bind(&password_hash)
            .bind(&auth_user.user_id)
            .execute(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to upgrade account", &e))?;

            let access_token = state
                .services
                .auth_service
                .generate_access_token(&auth_user.user_id, auth_user.device_id.as_deref().unwrap_or(""), false)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to generate token", &e))?;

            Ok(Json(json!({
                "success": true,
                "user_id": auth_user.user_id,
                "access_token": access_token,
            })))
        }
        _ => Err(ApiError::forbidden("User is not a guest".to_string())),
    }
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
