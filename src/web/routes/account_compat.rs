use super::auth_compat::{request_email_verification_with_submit_path, session_client_secret};
use crate::common::ApiError;
use crate::services::uia_service::UiaService;
use crate::web::extractors::{AuthenticatedUser, MatrixJson, OptionalAuthenticatedUser};
use crate::web::routes::{extract_token_from_headers, validate_user_id, AppState};
use axum::{
    extract::{Json, Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
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
    let results = can_view_profile_for_requester_batch(state, requester_id, &[user_id.to_string()]).await?;
    Ok(results.get(user_id).copied().unwrap_or(true))
}

pub(crate) async fn can_view_profile_for_requester_batch(
    state: &AppState,
    requester_id: Option<&str>,
    user_ids: &[String],
) -> Result<std::collections::HashMap<String, bool>, ApiError> {
    let mut result = std::collections::HashMap::with_capacity(user_ids.len());
    if user_ids.is_empty() {
        return Ok(result);
    }

    for uid in user_ids {
        result.insert(uid.clone(), true);
    }

    let rows = sqlx::query("SELECT user_id, profile_visibility, allow_profile_lookup FROM user_privacy_settings WHERE user_id = ANY($1)")
        .bind(user_ids)
        .fetch_all(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    for row in rows {
        let uid: String = row.try_get("user_id").unwrap_or_default();
        let is_self = requester_id == Some(uid.as_str());

        let visible = if let Ok(visibility) = row.try_get::<String, _>("profile_visibility") {
            match visibility.as_str() {
                "private" | "contacts" => is_self,
                _ => true,
            }
        } else if let Ok(allow_lookup) = row.try_get::<bool, _>("allow_profile_lookup") {
            allow_lookup || is_self
        } else {
            true
        };

        result.insert(uid, visible);
    }

    Ok(result)
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
        .map_err(|e| ApiError::internal(format!("Failed to get profile: {e}")))?;

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
        .map_err(|e| ApiError::internal(format!("Failed to get profile: {e}")))?;

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
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {e}")))?;

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
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {e}")))?;

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
    auth_user: OptionalAuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<axum::response::Response, ApiError> {
    let new_password = body
        .get("new_password")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("New password required".to_string()))?;

    let auth = body.get("auth").cloned().unwrap_or(serde_json::json!({}));
    let auth_type = auth.get("type").and_then(|v| v.as_str()).unwrap_or("");

    if auth_type.is_empty() {
        let user_id = auth_user
            .user_id
            .as_deref()
            .ok_or_else(|| ApiError::unauthorized("Access token required".to_string()))?;
        let session = state
            .services
            .uia_service
            .create_session(user_id, UiaService::get_password_change_flows())
            .await;
        return Ok((
            StatusCode::UNAUTHORIZED,
            Json(state.services.uia_service.build_uia_response(
                &session,
                "M_UIA_REQUIRED",
                "User-Interactive Authentication required",
            )),
        )
            .into_response());
    }

    match auth_type {
        "m.login.password" => {
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

            let authenticated_user_id = auth_user.user_id.as_deref().ok_or_else(|| {
                ApiError::unauthorized("Access token required for m.login.password".to_string())
            })?;

            if let Some(username) = user_identifier {
                let user_id = if username.starts_with('@') {
                    username.to_string()
                } else {
                    format!("@{}:{}", username, state.services.server_name)
                };

                if user_id != authenticated_user_id {
                    return Err(ApiError::forbidden("User mismatch".to_string()));
                }

                state
                    .services
                    .registration_service
                    .change_password(authenticated_user_id, Some(password), new_password, auth_user.device_id.as_deref())
                    .await?;

                // Note: change_password should NOT revoke the current device's token.
                // The current session should remain valid per Matrix spec.
                // If the underlying implementation revokes all tokens, we need to
                // re-cache the current token to keep the session alive.

                Ok(Json(json!({})).into_response())
            } else {
                Err(ApiError::bad_request(
                    "User identifier required".to_string(),
                ))
            }
        }
        "m.login.email.identity" => {
            let threepid_creds = auth.get("threepid_creds").unwrap_or(&auth);
            let sid = threepid_creds
                .get("sid")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ApiError::bad_request("Session ID (sid) is required".to_string()))?;
            let client_secret = threepid_creds
                .get("client_secret")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ApiError::bad_request("Client secret is required".to_string()))?;

            let sid_int: i64 = sid
                .parse()
                .map_err(|_| ApiError::bad_request("Invalid session ID format".to_string()))?;

            let verification_token = state
                .services
                .email_verification_storage
                .claim_used_token(sid_int)
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to claim verification token: {e}"))
                })?
                .ok_or_else(|| {
                    ApiError::bad_request(
                        "Verification session is invalid, expired, or has not been submitted"
                            .to_string(),
                    )
                })?;

            if session_client_secret(verification_token.session_data.as_ref())
                != Some(client_secret)
            {
                ::tracing::warn!(
                    target: "security_audit",
                    event = "password_reset_client_secret_mismatch",
                    sid = sid_int,
                    "client_secret mismatch on consumed verification token"
                );
                return Err(ApiError::bad_request("Client secret mismatch".to_string()));
            }

            let user_id = verification_token.user_id.ok_or_else(|| {
                ApiError::bad_request(
                    "Verification session is not valid for password reset".to_string(),
                )
            })?;

            state
                .services
                .registration_service
                .change_password(&user_id, None, new_password, None)
                .await?;

            Ok(Json(json!({})).into_response())
        }
        _ => {
            let user_id = auth_user
                .user_id
                .as_deref()
                .ok_or_else(|| ApiError::unauthorized("Access token required".to_string()))?;
            let session = state
                .services
                .uia_service
                .create_session(user_id, UiaService::get_password_change_flows())
                .await;
            Ok((
                StatusCode::UNAUTHORIZED,
                Json(state.services.uia_service.build_uia_response(
                    &session,
                    "M_UIA_REQUIRED",
                    "m.login.password or m.login.email.identity authentication required",
                )),
            )
                .into_response())
        }
    }
}

pub(crate) async fn request_password_email_verification(
    State(state): State<AppState>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let email = body
        .get("email")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Email is required".to_string()))?;

    // 不向客户端暴露"邮箱是否注册"。即便查不到对应账户也照样创建一个
    // user_id=None 的占位验证会话；下游 change_password_uia 在
    // m.login.email.identity 分支显式拒绝 user_id 为空的 session，
    // 所以占位会话无法被用来重置任何账户的密码，但响应却与命中账户
    // 的情况完全一致 —— 切断 OWASP A07 类账户枚举通道。
    let verified = state
        .services
        .threepid_storage
        .get_verified_threepid_by_address("email", email)
        .await;
    let resolved_user_id = match verified {
        Ok(Some(threepid)) => Some(threepid.user_id),
        Ok(None) => match state.services.user_storage.get_user_by_email(email).await {
            Ok(user) => user.map(|u| u.user_id),
            Err(e) => {
                ::tracing::warn!(
                    target: "security_audit",
                    event = "password_reset_email_lookup_failed",
                    "Failed to resolve email owner during password reset request: {}",
                    e
                );
                None
            }
        },
        Err(e) => {
            ::tracing::warn!(
                target: "security_audit",
                event = "password_reset_threepid_lookup_failed",
                "Failed to resolve verified threepid during password reset request: {}",
                e
            );
            None
        }
    };

    if resolved_user_id.is_none() {
        ::tracing::info!(
            target: "security_audit",
            event = "password_reset_email_not_registered",
            "Password reset requested for an email with no associated account"
        );
    }

    request_email_verification_with_submit_path(
        &state,
        &body,
        "/_matrix/client/v3/account/password/email/submitToken",
        resolved_user_id.as_deref(),
        "password_reset",
    )
    .await
}

pub(crate) async fn deactivate_account(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<axum::response::Response, ApiError> {
    let flows = UiaService::get_deactivate_account_flows();
    let auth = body.get("auth");

    match auth {
        None => {
            let session = state
                .services
                .uia_service
                .create_session(&auth_user.user_id, flows)
                .await;
            return Ok((
                StatusCode::UNAUTHORIZED,
                Json(state.services.uia_service.build_uia_response(
                    &session,
                    "M_UIA_REQUIRED",
                    "User-Interactive Authentication required",
                )),
            )
                .into_response());
        }
        Some(auth_val) => {
            let result = state
                .services
                .uia_service
                .validate_auth(auth_val, &auth_user.user_id, flows)
                .await;

            match result {
                Ok(_) => {}
                Err(uia_response) => {
                    return Ok((StatusCode::UNAUTHORIZED, Json(uia_response)).into_response());
                }
            }

            let auth_type = auth_val.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match auth_type {
                "m.login.password" => {
                    if let Err(e) = state
                        .services
                        .uia_service
                        .verify_password_stage(
                            auth_val,
                            &auth_user.user_id,
                            &state.services.auth_service,
                        )
                        .await
                    {
                        let session = state
                            .services
                            .uia_service
                            .create_session(
                                &auth_user.user_id,
                                UiaService::get_deactivate_account_flows(),
                            )
                            .await;
                        return Ok((
                            StatusCode::UNAUTHORIZED,
                            Json(state.services.uia_service.build_uia_response(
                                &session,
                                "M_FORBIDDEN",
                                &e.to_string(),
                            )),
                        )
                            .into_response());
                    }
                }
                _ => {
                    return Err(ApiError::unauthorized(
                        "Unsupported authentication type".to_string(),
                    ));
                }
            }
        }
    }

    let user_id = auth_user.user_id.clone();

    state
        .services
        .registration_service
        .deactivate_account(&user_id)
        .await?;

    state
        .services
        .cache
        .delete(&format!("user:active:{user_id}"))
        .await;

    state
        .services
        .cache
        .delete(&format!("token:{}", auth_user.access_token))
        .await;

    Ok(Json(json!({
        "id_server_unbind_result": "success"
    }))
    .into_response())
}

pub(crate) async fn get_threepids(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    let threepids = sqlx::query(
        r"
        SELECT medium, address, validated_ts, added_ts
        FROM user_threepids
        WHERE user_id = $1
        ",
    )
    .bind(user_id)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get threepids: {e}")))?;

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

    let sid = body
        .get("sid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Session ID (sid) is required".to_string()))?;
    let client_secret = body
        .get("client_secret")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Client secret is required".to_string()))?;

    let sid_int: i64 = sid
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid session ID format".to_string()))?;

    // 原子消费已校验会话：DELETE ... RETURNING 在单条 SQL 中完成"取出 + 删除",
    // 任何后续校验失败时 token 都已物理销毁，不能被重放。
    let verification_token = state
        .services
        .email_verification_storage
        .claim_used_token(sid_int)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to claim verification token: {e}")))?
        .ok_or_else(|| {
            ApiError::bad_request(
                "Verification session is invalid, expired, or has not been submitted".to_string(),
            )
        })?;

    if session_client_secret(verification_token.session_data.as_ref()) != Some(client_secret) {
        ::tracing::warn!(
            target: "security_audit",
            event = "threepid_add_client_secret_mismatch",
            sid = sid_int,
            user_id = user_id.as_str(),
            "client_secret mismatch on consumed verification token"
        );
        return Err(ApiError::bad_request("Client secret mismatch".to_string()));
    }

    let session_purpose = verification_token
        .session_data
        .as_ref()
        .and_then(|d| d.get("purpose"))
        .and_then(|v| v.as_str());
    if session_purpose != Some("3pid_add") {
        ::tracing::warn!(
            target: "security_audit",
            event = "threepid_add_session_purpose_mismatch",
            sid = sid_int,
            user_id = user_id.as_str(),
            purpose = session_purpose,
            "Verification session was not requested for 3PID add"
        );
        return Err(ApiError::bad_request(
            "Verification session is not valid for adding a 3PID".to_string(),
        ));
    }

    let session_user = verification_token.user_id.as_deref().ok_or_else(|| {
        ApiError::bad_request("Verification session is not bound to a user".to_string())
    })?;
    if session_user != user_id {
        ::tracing::warn!(
            target: "security_audit",
            event = "threepid_add_user_mismatch",
            sid = sid_int,
            authenticated_user = user_id.as_str(),
            session_user = session_user,
            "Verification session belongs to a different user"
        );
        return Err(ApiError::forbidden(
            "Verification session belongs to a different user".to_string(),
        ));
    }

    let medium = "email";
    let address = verification_token.email.as_str();

    // ON CONFLICT 上的 WHERE 谓词保证：地址若已被另一个账户占用，UPDATE 不会
    // 命中——rows_affected == 0，路由抛 409。本账户重复绑定则幂等更新时间戳。
    let result = sqlx::query(
        r"
        INSERT INTO user_threepids (user_id, medium, address, validated_ts, added_ts, is_verified)
        VALUES ($1, $2, $3, $4, $5, TRUE)
        ON CONFLICT (medium, address) DO UPDATE
        SET validated_ts = EXCLUDED.validated_ts,
            is_verified = TRUE
        WHERE user_threepids.user_id = EXCLUDED.user_id
        ",
    )
    .bind(user_id)
    .bind(medium)
    .bind(address)
    .bind(now)
    .bind(now)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to add threepid: {e}")))?;

    if result.rows_affected() == 0 {
        ::tracing::warn!(
            target: "security_audit",
            event = "threepid_add_address_already_bound",
            user_id = user_id.as_str(),
            medium = medium,
            "3PID address is already bound to a different account"
        );
        return Err(ApiError::conflict(
            "This 3PID is already bound to a different account".to_string(),
        ));
    }

    // 仅在本地校验通过后，才可选地把 3PID 推到身份服务器。这里使用与本地校验
    // 解耦的独立 IS 会话凭证 (is_sid / is_client_secret)，避免误把 HS 的 sid
    // 当成 IS 的 sid 来用。
    let id_server = body.get("id_server").and_then(|v| v.as_str());
    let is_sid = body.get("is_sid").and_then(|v| v.as_str());
    let id_access_token = body.get("id_access_token").and_then(|v| v.as_str());
    let is_client_secret = body.get("is_client_secret").and_then(|v| v.as_str());
    if let (Some(id_server), Some(is_sid), Some(id_access_token), Some(is_client_secret)) =
        (id_server, is_sid, id_access_token, is_client_secret)
    {
        if let Err(e) = state
            .services
            .identity_service
            .bind_three_pid(
                id_server,
                id_access_token,
                is_sid,
                is_client_secret,
                user_id,
            )
            .await
        {
            ::tracing::warn!("Failed to bind 3PID via Identity Server: {}", e);
        }
    }

    Ok(Json(json!({})))
}

pub(crate) async fn request_3pid_add_email_verification(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    request_email_verification_with_submit_path(
        &state,
        &body,
        "/_matrix/client/v3/account/3pid/email/submitToken",
        Some(auth_user.user_id.as_str()),
        "3pid_add",
    )
    .await
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
        r"
        DELETE FROM user_threepids
        WHERE user_id = $1 AND medium = $2 AND address = $3
        ",
    )
    .bind(user_id)
    .bind(&body.medium)
    .bind(&body.address)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to delete threepid: {e}")))?;

    Ok(Json(json!({})))
}

pub(crate) async fn unbind_threepid(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<DeleteThreepidRequest>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    sqlx::query(
        r"
        DELETE FROM user_threepids
        WHERE user_id = $1 AND medium = $2 AND address = $3
        ",
    )
    .bind(user_id)
    .bind(&body.medium)
    .bind(&body.address)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to unbind threepid: {e}")))?;

    Ok(Json(json!({})))
}
