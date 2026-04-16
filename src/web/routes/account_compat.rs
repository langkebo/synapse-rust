use crate::common::ApiError;
use crate::web::extractors::AuthenticatedUser;
use crate::web::routes::{extract_token_from_headers, validate_user_id, AppState};
use axum::{
    extract::{Json, Path, State},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

pub(crate) async fn whoami(
    State(_state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "user_id": auth_user.user_id,
        "device_id": auth_user.device_id,
        "is_guest": auth_user.is_guest
    })))
}

pub(crate) async fn can_view_profile_for_requester(
    state: &AppState,
    requester_id: Option<&str>,
    user_id: &str,
) -> Result<bool, ApiError> {
    let privacy_settings = sqlx::query("SELECT * FROM user_privacy_settings WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if let Some(settings) = privacy_settings {
        if let Ok(visibility) = settings.try_get::<String, _>("profile_visibility") {
            match visibility.as_str() {
                "private" => {
                    if requester_id != Some(user_id) {
                        return Ok(false);
                    }
                }
                "contacts" => {
                    if requester_id != Some(user_id) {
                        return Ok(false);
                    }
                }
                _ => {}
            }
        } else if let Ok(allow_lookup) = settings.try_get::<bool, _>("allow_profile_lookup") {
            if !allow_lookup && requester_id != Some(user_id) {
                return Ok(false);
            }
        }
    }

    Ok(true)
}

pub(crate) async fn enforce_profile_visibility(
    state: &AppState,
    headers: &HeaderMap,
    user_id: &str,
) -> Result<(), ApiError> {
    let token = extract_token_from_headers(headers).ok();
    let requester_id = if let Some(t) = token {
        state
            .services
            .auth_service
            .validate_token(&t)
            .await
            .ok()
            .map(|(id, _, _, _, _)| id)
    } else {
        None
    };

    if !can_view_profile_for_requester(state, requester_id.as_deref(), user_id).await? {
        return Err(ApiError::forbidden(
            "Profile is private or not visible to you".to_string(),
        ));
    }

    Ok(())
}

pub(crate) async fn get_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    enforce_profile_visibility(&state, &headers, &user_id).await?;

    Ok(Json(
        state
            .services
            .registration_service
            .get_profile(&user_id)
            .await?,
    ))
}

pub(crate) async fn get_displayname(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    enforce_profile_visibility(&state, &headers, &user_id).await?;

    let profile = state
        .services
        .registration_service
        .get_profile(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get profile: {}", e)))?;

    let displayname = profile
        .get("displayname")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    Ok(Json(json!({ "displayname": displayname })))
}

pub(crate) async fn get_avatar_url(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    enforce_profile_visibility(&state, &headers, &user_id).await?;

    let profile = state
        .services
        .registration_service
        .get_profile(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get profile: {}", e)))?;

    let avatar_url = profile
        .get("avatar_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    Ok(Json(json!({ "avatar_url": avatar_url })))
}

pub(crate) async fn update_displayname(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let displayname = body
        .get("displayname")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Displayname required".to_string()))?;

    if displayname.len() > 255 {
        return Err(ApiError::bad_request(
            "Displayname too long (max 255 characters)".to_string(),
        ));
    }

    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let user_exists = state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?;

    if !user_exists {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .registration_service
        .update_user_profile(&user_id, Some(displayname), None)
        .await?;
    Ok(Json(json!({})))
}

pub(crate) async fn update_avatar(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let avatar_url = body
        .get("avatar_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Avatar URL required".to_string()))?;

    if avatar_url.len() > 255 {
        return Err(ApiError::bad_request(
            "Avatar URL too long (max 255 characters)".to_string(),
        ));
    }

    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let user_exists = state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?;

    if !user_exists {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .registration_service
        .update_user_profile(&user_id, None, Some(avatar_url))
        .await?;
    Ok(Json(json!({})))
}

pub(crate) async fn change_password_uia(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let new_password = body
        .get("new_password")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("New password required".to_string()))?;

    let auth = body.get("auth").cloned().unwrap_or(serde_json::json!({}));
    let auth_type = auth.get("type").and_then(|v| v.as_str()).unwrap_or("");

    if auth_type == "m.login.password" {
        let password = auth
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ApiError::bad_request("Password required for m.login.password".to_string())
            })?;

        let user_identifier = auth
            .get("identifier")
            .and_then(|i| i.get("user"))
            .and_then(|u| u.as_str())
            .or_else(|| auth.get("user").and_then(|u| u.as_str()));

        if let Some(username) = user_identifier {
            let user_id = if username.starts_with('@') {
                username.to_string()
            } else {
                format!("@{}:{}", username, state.services.server_name)
            };

            if user_id != auth_user.user_id {
                return Err(ApiError::forbidden("User mismatch".to_string()));
            }

            state
                .services
                .auth_service
                .validator
                .validate_password(new_password)?;

            let user = state
                .services
                .user_storage
                .get_user_by_id(&user_id)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get user: {}", e)))?
                .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

            let password_hash = user
                .password_hash
                .ok_or_else(|| ApiError::forbidden("User has no password set".to_string()))?;

            let valid = crate::common::crypto::verify_password(password, &password_hash, false)
                .map_err(|e| ApiError::internal(format!("Password verification failed: {}", e)))?;

            if !valid {
                return Err(ApiError::forbidden("Invalid password".to_string()));
            }

            state
                .services
                .registration_service
                .change_password(&auth_user.user_id, Some(password), new_password)
                .await?;

            Ok(Json(json!({})))
        } else {
            Err(ApiError::bad_request(
                "User identifier required".to_string(),
            ))
        }
    } else {
        Err(ApiError::unauthorized(
            "m.login.password authentication required".to_string(),
        ))
    }
}

pub(crate) async fn deactivate_account(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = auth_user.user_id.clone();

    state
        .services
        .registration_service
        .deactivate_account(&user_id)
        .await?;

    state
        .services
        .cache
        .delete(&format!("user:active:{}", user_id))
        .await;

    state
        .services
        .cache
        .delete(&format!("token:{}", auth_user.access_token))
        .await;

    Ok(Json(json!({
        "id_server_unbind_result": "success"
    })))
}

pub(crate) async fn get_threepids(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    let threepids = sqlx::query(
        r#"
        SELECT medium, address, validated_ts, added_ts
        FROM user_threepids
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get threepids: {}", e)))?;

    let threepids_list: Vec<Value> = threepids
        .iter()
        .map(|row| {
            json!({
                "medium": row.get::<String, _>("medium"),
                "address": row.get::<String, _>("address"),
                "validated_ts": row.get::<Option<i64>, _>("validated_ts").unwrap_or(0),
                "added_at": row.get::<Option<i64>, _>("added_ts").unwrap_or(0)
            })
        })
        .collect();

    Ok(Json(json!({
        "threepids": threepids_list
    })))
}

pub(crate) async fn add_threepid(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;
    let now = chrono::Utc::now().timestamp_millis();

    let medium = body
        .get("medium")
        .and_then(|v| v.as_str())
        .unwrap_or("email");
    let address = body.get("address").and_then(|v| v.as_str()).unwrap_or("");

    if address.is_empty() {
        return Err(ApiError::bad_request("Address is required".to_string()));
    }

    sqlx::query(
        r#"
        INSERT INTO user_threepids (user_id, medium, address, validated_ts, added_ts)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (medium, address) DO UPDATE
        SET validated_ts = EXCLUDED.validated_ts
        "#,
    )
    .bind(user_id)
    .bind(medium)
    .bind(address)
    .bind(now)
    .bind(now)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to add threepid: {}", e)))?;

    Ok(Json(json!({})))
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeleteThreepidRequest {
    medium: String,
    address: String,
}

pub(crate) async fn delete_threepid(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<DeleteThreepidRequest>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    sqlx::query(
        r#"
        DELETE FROM user_threepids
        WHERE user_id = $1 AND medium = $2 AND address = $3
        "#,
    )
    .bind(user_id)
    .bind(&body.medium)
    .bind(&body.address)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to delete threepid: {}", e)))?;

    Ok(Json(json!({})))
}

pub(crate) async fn unbind_threepid(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<DeleteThreepidRequest>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    sqlx::query(
        r#"
        DELETE FROM user_threepids
        WHERE user_id = $1 AND medium = $2 AND address = $3
        "#,
    )
    .bind(user_id)
    .bind(&body.medium)
    .bind(&body.address)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to unbind threepid: {}", e)))?;

    Ok(Json(json!({})))
}
