//! MSC4133 — Extended Profile handlers
//!
//! Extended profile properties allow clients to store and retrieve per-field
//! profile data beyond the standard Matrix profile fields (displayname, avatar).
//! Data is persisted in account_data under the `uk.tcpip.msc4133.profile` type.

use crate::web::routes::account_compat;
use crate::web::routes::validators;
use crate::web::routes::ApiError;
use crate::web::routes::AuthenticatedUser;
use crate::web::AppState;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde_json::json;

/// MSC4133 — extended profile properties.
///
/// We persist a user-scoped JSON object in `account_data` and expose per-field
/// accessors on top of it. This keeps the implementation small while providing
/// real interoperability for clients probing the unstable MSC4133 endpoints.
const EXTENDED_PROFILE_DATA_TYPE: &str = "uk.tcpip.msc4133.profile";
const EXTENDED_PROFILE_MAX_FIELD_NAME_LEN: usize = 128;
const EXTENDED_PROFILE_MAX_JSON_LEN: usize = 65536;

async fn ensure_extended_profile_user_exists(state: &AppState, user_id: &str) -> Result<(), ApiError> {
    let exists = state.services.account.account_identity_service.user_exists(user_id).await?;

    if exists {
        Ok(())
    } else {
        Err(ApiError::not_found("User not found".to_string()))
    }
}

async fn load_extended_profile_document(
    state: &AppState,
    user_id: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, ApiError> {
    let Some(content) =
        state.services.core.account_data_service.get_account_data(user_id, EXTENDED_PROFILE_DATA_TYPE).await?
    else {
        return Ok(serde_json::Map::new());
    };

    match content {
        serde_json::Value::Object(map) => Ok(map),
        _ => Err(ApiError::internal("Stored extended profile content is not a JSON object".to_string())),
    }
}

async fn save_extended_profile_document(
    state: &AppState,
    user_id: &str,
    document: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), ApiError> {
    let content = serde_json::Value::Object(document.clone());
    state.services.core.account_data_service.set_account_data(user_id, EXTENDED_PROFILE_DATA_TYPE, &content).await
}

pub async fn get_extended_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(_user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = _user_id;
    validators::validate_user_id(&user_id)?;
    account_compat::enforce_profile_visibility(&state, &headers, &user_id).await?;
    ensure_extended_profile_user_exists(&state, &user_id).await?;
    Ok(Json(serde_json::Value::Object(load_extended_profile_document(&state, &user_id).await?)))
}

pub async fn get_extended_profile_field(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((user_id, key_name)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validators::validate_user_id(&user_id)?;
    account_compat::enforce_profile_visibility(&state, &headers, &user_id).await?;
    ensure_extended_profile_user_exists(&state, &user_id).await?;

    if key_name.is_empty() || key_name.len() > EXTENDED_PROFILE_MAX_FIELD_NAME_LEN {
        return Err(ApiError::bad_request("Invalid extended profile field name".to_string()));
    }

    let document = load_extended_profile_document(&state, &user_id).await?;
    let value = document
        .get(&key_name)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Extended profile field not found".to_string()))?;

    Ok(Json(value))
}

pub async fn put_extended_profile_field(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((user_id, key_name)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth_user = _auth_user;
    validators::validate_user_id(&user_id)?;
    ensure_extended_profile_user_exists(&state, &user_id).await?;

    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }
    if key_name.is_empty() || key_name.len() > EXTENDED_PROFILE_MAX_FIELD_NAME_LEN {
        return Err(ApiError::bad_request("Invalid extended profile field name".to_string()));
    }

    let body_str = serde_json::to_string(&body).map_err(|e| ApiError::bad_request(format!("Invalid JSON: {e}")))?;
    if body_str.len() > EXTENDED_PROFILE_MAX_JSON_LEN {
        return Err(ApiError::bad_request("Extended profile field too large (max 64KB)".to_string()));
    }

    let mut document = load_extended_profile_document(&state, &user_id).await?;
    document.insert(key_name.clone(), body);
    save_extended_profile_document(&state, &user_id, &document).await?;

    Ok(Json(json!({
        "key_name": key_name,
        "updated": true
    })))
}

pub async fn delete_extended_profile_field(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((user_id, key_name)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth_user = _auth_user;
    validators::validate_user_id(&user_id)?;
    ensure_extended_profile_user_exists(&state, &user_id).await?;

    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }
    if key_name.is_empty() || key_name.len() > EXTENDED_PROFILE_MAX_FIELD_NAME_LEN {
        return Err(ApiError::bad_request("Invalid extended profile field name".to_string()));
    }

    let mut document = load_extended_profile_document(&state, &user_id).await?;
    let removed = document.remove(&key_name).is_some();
    if removed {
        save_extended_profile_document(&state, &user_id, &document).await?;
    }

    Ok(Json(json!({
        "key_name": key_name,
        "deleted": true
    })))
}
