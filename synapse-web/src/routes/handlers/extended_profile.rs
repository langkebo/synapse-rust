//! MSC4133 — Extended Profile handlers
//!
//! Extended profile properties allow clients to store and retrieve per-field
//! profile data beyond the standard Matrix profile fields (displayname, avatar).
//! Data is persisted in account_data under the `uk.tcpip.msc4133.profile` type.

use crate::routes::account_compat;
use crate::routes::context::RoomContext;
use crate::routes::extractors::UserId;
use crate::routes::validators;
use crate::routes::ApiError;
use crate::routes::AuthenticatedUser;
use synapse_services::account_data_service::EXTENDED_PROFILE_DATA_TYPE;

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
/// Spec (`M_KEY_TOO_LARGE`: "maximum allowed length of 255 characters") and
/// upstream Synapse agree on 255 (`synapse/handlers/profile.py:66
/// MAX_CUSTOM_FIELD_LEN = 255`); this repo previously capped keys at 128, which
/// rejected keys the spec allows.
const EXTENDED_PROFILE_MAX_FIELD_NAME_LEN: usize = 255;
/// Total stored profile limit (spec: "the total profile MUST be under 64 KiB").
const EXTENDED_PROFILE_MAX_JSON_LEN: usize = 65536;

/// Validate a profile field write before it touches storage.
///
/// Both refusals carry a **specific** errcode — the request is well-formed JSON,
/// so `M_BAD_JSON` would send clients looking for a syntax error that is not
/// there:
/// - oversize key ⇒ `M_KEY_TOO_LARGE` (400);
/// - value that would exceed the profile size limit ⇒ `M_PROFILE_TOO_LARGE` (400).
///
/// Kept a pure function so the boundary cases are unit-testable without a
/// request context.
fn validate_extended_profile_field(key_name: &str, body_len: usize) -> Result<(), ApiError> {
    if key_name.is_empty() {
        return Err(ApiError::missing_param("Profile field name must not be empty".to_string()));
    }
    if key_name.len() > EXTENDED_PROFILE_MAX_FIELD_NAME_LEN {
        return Err(ApiError::key_too_large(format!(
            "Profile field name exceeds maximum allowed length of {EXTENDED_PROFILE_MAX_FIELD_NAME_LEN} bytes"
        )));
    }
    if body_len > EXTENDED_PROFILE_MAX_JSON_LEN {
        return Err(ApiError::profile_too_large(format!(
            "Extended profile field too large (max {EXTENDED_PROFILE_MAX_JSON_LEN} bytes)"
        )));
    }
    Ok(())
}

async fn ensure_extended_profile_user_exists(ctx: &RoomContext, user_id: &str) -> Result<(), ApiError> {
    // U-2 / upstream #20172: the profile-field family intentionally succeeds for
    // *existing but deactivated* users, so this stays on the row-existence
    // predicate. Do not switch it to `active_user_exists`.
    let exists = ctx.account_identity_service.user_exists(user_id).await?;

    if exists {
        Ok(())
    } else {
        Err(ApiError::not_found("User not found".to_string()))
    }
}

async fn load_extended_profile_document(
    ctx: &RoomContext,
    user_id: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, ApiError> {
    let Some(content) = ctx.account_data_service.get_account_data(user_id, EXTENDED_PROFILE_DATA_TYPE).await? else {
        return Ok(serde_json::Map::new());
    };

    match content {
        serde_json::Value::Object(map) => Ok(map),
        // A non-object here means the account-data row was written out of band
        // (the write-time guard rejects this shape).  It is a bad request for
        // *this* read, not a server fault: MSC4133 expects 400, and a 500 would
        // falsely signal an internal error for client-supplied data.
        _ => Err(ApiError::bad_request("Stored extended profile content is not a JSON object".to_string())),
    }
}

async fn save_extended_profile_document(
    ctx: &RoomContext,
    user_id: &str,
    document: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), ApiError> {
    let content = serde_json::Value::Object(document.clone());
    ctx.account_data_service.set_account_data(user_id, EXTENDED_PROFILE_DATA_TYPE, &content).await
}

/// See [`get_extended_profile`].
pub async fn get_extended_profile(
    State(ctx): State<RoomContext>,
    headers: HeaderMap,
    Path(_user_id): Path<UserId>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = _user_id;
    validators::validate_user_id(&user_id)?;
    account_compat::enforce_profile_visibility(
        ctx.token_auth.as_ref(),
        &ctx.account_identity_service,
        &headers,
        &user_id,
    )
    .await?;
    ensure_extended_profile_user_exists(&ctx, &user_id).await?;
    Ok(Json(serde_json::Value::Object(load_extended_profile_document(&ctx, &user_id).await?)))
}

/// See [`get_extended_profile_field`].
pub async fn get_extended_profile_field(
    State(ctx): State<RoomContext>,
    headers: HeaderMap,
    Path((user_id, key_name)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validators::validate_user_id(&user_id)?;
    account_compat::enforce_profile_visibility(
        ctx.token_auth.as_ref(),
        &ctx.account_identity_service,
        &headers,
        &user_id,
    )
    .await?;
    ensure_extended_profile_user_exists(&ctx, &user_id).await?;

    validate_extended_profile_field(&key_name, 0)?;

    let document = load_extended_profile_document(&ctx, &user_id).await?;
    let value = document
        .get(&key_name)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Extended profile field not found".to_string()))?;

    Ok(Json(value))
}

/// See [`put_extended_profile_field`].
pub async fn put_extended_profile_field(
    State(ctx): State<RoomContext>,
    _auth_user: AuthenticatedUser,
    Path((user_id, key_name)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth_user = _auth_user;
    validators::validate_user_id(&user_id)?;
    ensure_extended_profile_user_exists(&ctx, &user_id).await?;

    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }
    let body_str = serde_json::to_string(&body).map_err(|e| ApiError::bad_request(format!("Invalid JSON: {e}")))?;
    validate_extended_profile_field(&key_name, body_str.len())?;

    let mut document = load_extended_profile_document(&ctx, &user_id).await?;
    document.insert(key_name.clone(), body);
    save_extended_profile_document(&ctx, &user_id, &document).await?;

    Ok(Json(json!({
        "key_name": key_name,
        "updated": true
    })))
}

/// See [`delete_extended_profile_field`].
pub async fn delete_extended_profile_field(
    State(ctx): State<RoomContext>,
    _auth_user: AuthenticatedUser,
    Path((user_id, key_name)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth_user = _auth_user;
    validators::validate_user_id(&user_id)?;
    ensure_extended_profile_user_exists(&ctx, &user_id).await?;

    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }
    validate_extended_profile_field(&key_name, 0)?;

    let mut document = load_extended_profile_document(&ctx, &user_id).await?;
    let removed = document.remove(&key_name).is_some();
    if removed {
        save_extended_profile_document(&ctx, &user_id, &document).await?;
    }

    Ok(Json(json!({
        "key_name": key_name,
        "deleted": true
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_common::error::MatrixErrorCode;

    /// 255 bytes is allowed (spec / upstream Synapse `MAX_CUSTOM_FIELD_LEN`);
    /// 256 is not, and the refusal names the right errcode.
    #[test]
    fn field_name_boundary_is_the_spec_limit() {
        let max_ok = "a".repeat(EXTENDED_PROFILE_MAX_FIELD_NAME_LEN);
        assert!(validate_extended_profile_field(&max_ok, 2).is_ok());
        assert_eq!(EXTENDED_PROFILE_MAX_FIELD_NAME_LEN, 255, "spec: max 255 characters");

        let too_long = "a".repeat(EXTENDED_PROFILE_MAX_FIELD_NAME_LEN + 1);
        let error = validate_extended_profile_field(&too_long, 2).expect_err("256 bytes must be refused");
        assert_eq!(error.http_status(), axum::http::StatusCode::BAD_REQUEST);
        assert!(error.code_is(MatrixErrorCode::KeyTooLarge), "got {}", error.code_str());
        assert_eq!(error.code_str(), "M_KEY_TOO_LARGE");
    }

    #[test]
    fn oversize_value_is_profile_too_large_not_bad_json() {
        let error = validate_extended_profile_field("org.example.job", EXTENDED_PROFILE_MAX_JSON_LEN + 1)
            .expect_err("over the profile limit must be refused");
        assert_eq!(error.http_status(), axum::http::StatusCode::BAD_REQUEST);
        assert!(error.code_is(MatrixErrorCode::ProfileTooLarge), "got {}", error.code_str());
        assert_eq!(error.code_str(), "M_PROFILE_TOO_LARGE");
    }

    #[test]
    fn empty_key_is_missing_param() {
        let error = validate_extended_profile_field("", 2).expect_err("empty key must be refused");
        assert!(error.code_is(MatrixErrorCode::MissingParam), "got {}", error.code_str());
    }
}
