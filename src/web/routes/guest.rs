use crate::storage::device::DeviceStorage;
use crate::storage::user::UserStorage;
use crate::storage::token::AccessTokenStorage;
use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde_json::{json, Value};

pub async fn register_guest(
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    if !state.services.config.server.enable_registration {
        return Err(ApiError::forbidden("Registration is disabled".to_string()));
    }

    let guest_num = rand::random::<u64>();
    let username = format!("guest_{}", guest_num);
    let user_id = format!("@{}:{}", username, state.services.server_name);
    let device_id = format!("guest_device_{}", guest_num);

    let user = UserStorage::create_user(&state.services.user_storage, &user_id, &username, None, false)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create guest user: {}", e)))?;

    let now = Utc::now().timestamp_millis();
    DeviceStorage::create_device(&state.services.device_storage, &device_id, &user.user_id, Some("Guest Device"))
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create device: {}", e)))?;

    let access_token = format!("guest_token_{}", uuid::Uuid::new_v4());
    let expires_at = now + 86400000;

    AccessTokenStorage::create_token(&state.services.token_storage, &access_token, &user.user_id, Some(&device_id), Some(expires_at))
        .await
        .map_err(|e| ApiError::internal(format!("Failed to store token: {}", e)))?;

    Ok(Json(json!({
        "access_token": access_token,
        "device_id": device_id,
        "user_id": user.user_id,
        "expires_in": 86400,
    })))
}

pub async fn get_guest_info(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user = UserStorage::get_user_by_id(&state.services.user_storage, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get user: {}", e)))?;

    match user {
        Some(u) if u.is_guest => Ok(Json(json!({
            "user_id": auth_user.user_id,
            "is_guest": true,
        }))),
        _ => Err(ApiError::forbidden("User is not a guest".to_string())),
    }
}

pub async fn upgrade_guest(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let username = body
        .get("username")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing username".to_string()))?;

    let _password = body
        .get("password")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing password".to_string()))?;

    let user = UserStorage::get_user_by_id(&state.services.user_storage, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get user: {}", e)))?;

    match user {
        Some(u) if u.is_guest => {
            let existing = UserStorage::get_user_by_username(&state.services.user_storage, username)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to check username: {}", e)))?;

            if existing.is_some() {
                return Err(ApiError::conflict("Username already exists".to_string()));
            }

            let new_user_id = format!("@{}:{}", username, state.services.server_name);

            sqlx::query(
                r#"
                UPDATE users SET username = $1, is_guest = FALSE WHERE user_id = $2
                "#,
            )
            .bind(username)
            .bind(&auth_user.user_id)
            .execute(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to upgrade account: {}", e)))?;

            Ok(Json(json!({
                "success": true,
                "user_id": new_user_id,
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
