use super::audit::{record_audit_event, resolve_request_id};
use crate::common::ApiError;
use crate::common::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use crate::web::routes::context::AdminContext;
use crate::web::routes::AdminUser;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use synapse_common::types::DeviceId;
use synapse_common::types::UserId;
use synapse_services::admin_user_service::{decode_user_cursor, encode_user_cursor, AdminUserCursor};
use synapse_services::user_service::User as AdminUserRecord;
use validator::Validate;

/// See [`create_user_router`].
pub fn create_user_router() -> Router<crate::web::routes::AppState> {
    Router::new()
        .route("/_synapse/admin/v1/users", get(get_users))
        .route("/_synapse/admin/v1/users/{user_id}", get(get_user))
        .route("/_synapse/admin/v1/users/{user_id}", delete(delete_user))
        .route("/_synapse/admin/v1/users/{user_id}/admin", put(set_admin))
        .route("/_synapse/admin/v1/users/{user_id}/evict", post(evict_user))
        .route(
            "/_synapse/admin/v1/users/{user_id}/deactivate",
            post(deactivate_user),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/password",
            post(reset_user_password),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/rooms",
            get(get_user_rooms_admin),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/login",
            post(login_as_user),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/logout",
            post(logout_user_devices),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/devices",
            get(get_user_devices_admin),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/devices/delete",
            post(logout_user_devices),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/devices/{device_id}",
            delete(delete_user_device_admin),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/devices/{device_id}/delete",
            post(delete_user_device_admin_compat),
        )
        .route("/_synapse/admin/v2/users", get(get_users_v2))
        .route("/_synapse/admin/v2/users/{user_id}", get(get_user_v2))
        .route(
            "/_synapse/admin/v2/users/{user_id}",
            put(create_or_update_user_v2),
        )
        .route("/_synapse/admin/v2/users/{user_id}", delete(delete_user))
        .route("/_synapse/admin/v1/user_stats", get(get_user_stats))
        .route(
            "/_synapse/admin/v1/users/{user_id}/stats",
            get(get_single_user_stats),
        )
        // Batch operations
        .route("/_synapse/admin/v1/users/batch", post(batch_create_users))
        .route(
            "/_synapse/admin/v1/users/batch_deactivate",
            post(batch_deactivate_users),
        )
        // User sessions
        .route(
            "/_synapse/admin/v1/user_sessions/{user_id}",
            get(get_user_sessions),
        )
        .route(
            "/_synapse/admin/v1/user_sessions/{user_id}/invalidate",
            post(invalidate_user_sessions),
        )
        // Account details
        .route(
            "/_synapse/admin/v1/account/{user_id}",
            get(get_account_details),
        )
        .route("/_synapse/admin/v1/account/{user_id}", post(update_account))
}

#[axum::debug_handler]
async fn evict_user(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    let user_id = user_id.as_str();
    let user = resolve_user(&ctx, user_id).await?;
    let eviction = ctx.admin_user_service.evict_user_from_joined_rooms(&user.user_id).await?;
    let failures: Vec<Value> = eviction
        .failures
        .into_iter()
        .map(|failure| {
            json!({
                "room_id": failure.room_id,
                "error": failure.error
            })
        })
        .collect();

    Ok(Json(json!({
        "user_id": user.user_id,
        "rooms_evicted": eviction.joined_rooms.len(),
        "rooms": eviction.joined_rooms,
        "failures": failures
    })))
}

/// The `ResetPasswordBody` struct.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ResetPasswordBody {
    #[validate(length(min = 8, max = 512))]
    #[serde(alias = "newPassword", alias = "new_password")]
    /// The `new_password` field.
    pub new_password: String,
}

/// The `CreateUpdateUserRequest` struct.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateUpdateUserRequest {
    #[validate(length(max = 255))]
    /// The `displayname` field.
    pub displayname: Option<String>,
    #[validate(length(max = 2048))]
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `admin` field.
    pub admin: Option<bool>,
    /// The `deactivated` field.
    pub deactivated: Option<bool>,
    #[validate(length(max = 255))]
    /// The `user_type` field.
    pub user_type: Option<String>,
    #[validate(length(min = 8, max = 512))]
    /// The `password` field.
    pub password: Option<String>,
}

async fn resolve_user(ctx: &AdminContext, identifier: &str) -> Result<AdminUserRecord, ApiError> {
    ctx.user_service.get_user_or_not_found(identifier).await
}

// Moved to admin/mod.rs
use super::ensure_super_admin_for_privilege_change;

/// See [`get_users`].
#[axum::debug_handler]
pub async fn get_users(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let cursor = decode_user_cursor(params.get("since").map(String::as_str));

    if params.contains_key("offset") && params.get("offset").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0) > 0 {
        return Err(ApiError::bad_request(
            "Legacy offset pagination is no longer supported; use since cursor".to_string(),
        ));
    }

    let page = ctx
        .admin_user_service
        .list_users_legacy(
            limit,
            cursor.as_ref().map(|cursor| cursor.created_ts),
            cursor.as_ref().map(|cursor| cursor.user_id.as_str()),
        )
        .await?;
    let users = page.users;
    let total = page.total;

    let user_list: Vec<Value> = users
        .iter()
        .map(|u| {
            json!({
                "name": u.username,
                "is_guest": u.is_guest,
                "admin": u.is_admin,
                "deactivated": u.is_deactivated,
                "displayname": u.displayname,
                "avatar_url": u.avatar_url,
                "created_ts": u.created_ts,
                "user_type": u.user_type
            })
        })
        .collect();

    let next_batch = if users.len() as i64 == limit {
        users
            .last()
            .map(|u| encode_user_cursor(&AdminUserCursor { created_ts: u.created_ts, user_id: u.user_id.clone() }))
    } else {
        None
    };

    Ok(Json(json!({
        "users": user_list,
        "total": total,
        "next_batch": next_batch
    })))
}

#[axum::debug_handler]
async fn get_user(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    let user = ctx.user_service.get_user_by_identifier(&user_id).await?;

    match user {
        Some(u) => Ok(Json(json!({
            "name": u.username,
            "is_guest": u.is_guest,
            "admin": u.is_admin,
            "deactivated": u.is_deactivated,
            "displayname": u.displayname,
            "avatar_url": u.avatar_url,
            "created_ts": u.created_ts,
            "user_type": u.user_type
        }))),
        None => Err(ApiError::not_found("User not found".to_string())),
    }
}

#[axum::debug_handler]
async fn delete_user(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&ctx, &user_id).await?;

    tracing::info!(
        admin_user = %admin.user_id,
        target_user = %user.user_id,
        "Admin deleting user"
    );

    ctx.admin_user_service.delete_user(&user.user_id).await.map_err(|e| {
        tracing::error!(
            admin_user = %admin.user_id,
            target_user = %user.user_id,
            error = %e,
            "Failed to delete user"
        );
        e
    })?;

    // 记录审计日志
    let request_id = resolve_request_id(&headers);
    if let Err(e) = record_audit_event(
        &ctx,
        &admin.user_id,
        "delete_user",
        "user",
        &user.user_id,
        request_id,
        json!({
            "admin_role": admin.role,
            "target_user": user.user_id,
        }),
    )
    .await
    {
        tracing::warn!("Failed to record audit event: {}", e);
    }

    tracing::info!(
        admin_user = %admin.user_id,
        target_user = %user.user_id,
        "User deleted successfully"
    );

    Ok(Json(json!({
        "user_id": user.user_id,
        "deleted": true
    })))
}

/// See [`set_admin`].
#[axum::debug_handler]
pub async fn set_admin(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ensure_super_admin_for_privilege_change(&admin)?;

    let admin_status = body
        .get("admin")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| ApiError::bad_request("Missing 'admin' field".to_string()))?;

    let user = resolve_user(&ctx, &user_id).await?;

    tracing::info!(
        admin_user = %admin.user_id,
        target_user = %user.user_id,
        admin_status = admin_status,
        "Admin changing user admin status"
    );

    ctx.admin_user_service.set_admin_status(&user.user_id, admin_status).await.map_err(|e| {
        tracing::error!(
            admin_user = %admin.user_id,
            target_user = %user.user_id,
            error = %e,
            "Failed to set admin status"
        );
        e
    })?;
    ctx.cache.set(&format!("user:admin:{}", user.user_id), admin_status, 3600).await?;

    // 记录审计日志
    let request_id = resolve_request_id(&headers);
    if let Err(e) = record_audit_event(
        &ctx,
        &admin.user_id,
        "set_admin_status",
        "user",
        &user.user_id,
        request_id,
        json!({
            "admin_role": admin.role,
            "target_user": user.user_id,
            "admin_status": admin_status,
        }),
    )
    .await
    {
        tracing::warn!("Failed to record audit event: {}", e);
    }

    tracing::info!(
        admin_user = %admin.user_id,
        target_user = %user.user_id,
        admin_status = admin_status,
        "Admin status changed successfully"
    );

    Ok(Json(json!({ "success": true })))
}

/// See [`deactivate_user`].
#[axum::debug_handler]
pub async fn deactivate_user(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    // A4 (API 路由审计 2026-09-04): 拒绝 admin 停用自己。
    // 单 admin 部署中，admin 停用自己会失去所有管理能力且无回退；
    // 多 admin 部署中应要求另一个 admin 来执行。返回 400 而不是 403，
    // 因为这是业务约束而非权限不足。
    if admin.user_id == user_id.as_str() {
        return Err(ApiError::bad_request(
            "Cannot deactivate your own admin account; ask another admin to perform this action".to_string(),
        ));
    }

    let user = resolve_user(&ctx, &user_id).await?;

    // P2 #33: 审计日志 - deactivate_user 操作
    tracing::warn!(
        action = "admin.deactivate_user",
        admin_user_id = %admin.user_id,
        target_user_id = %user.user_id,
        timestamp_ms = current_timestamp_millis(),
        "Admin deactivate user operation"
    );

    tracing::info!(
        admin_user = %admin.user_id,
        target_user = %user.user_id,
        "Admin deactivating user"
    );

    ctx.credential_auth.deactivate_user(&user.user_id).await.map_err(|e| {
        tracing::error!(
            admin_user = %admin.user_id,
            target_user = %user.user_id,
            error = %e,
            "Failed to deactivate user"
        );
        e
    })?;

    // 记录审计日志
    let request_id = resolve_request_id(&headers);
    if let Err(e) = record_audit_event(
        &ctx,
        &admin.user_id,
        "deactivate_user",
        "user",
        &user.user_id,
        request_id,
        json!({
            "admin_role": admin.role,
            "target_user": user.user_id,
        }),
    )
    .await
    {
        tracing::warn!("Failed to record audit event: {}", e);
    }

    tracing::info!(
        admin_user = %admin.user_id,
        target_user = %user.user_id,
        "User deactivated successfully"
    );

    Ok(Json(json!({ "id_server_unbind_result": "success" })))
}

/// See [`reset_user_password`].
#[axum::debug_handler]
pub async fn reset_user_password(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
    Json(body): Json<ResetPasswordBody>,
) -> Result<Json<Value>, ApiError> {
    ctx.validator.validate_password(&body.new_password)?;

    let user = resolve_user(&ctx, &user_id).await?;

    // P2 #33: 审计日志 - reset_password 操作
    tracing::warn!(
        action = "admin.reset_password",
        admin_user_id = %admin.user_id,
        target_user_id = %user.user_id,
        timestamp_ms = current_timestamp_millis(),
        "Admin reset user password operation"
    );

    // Admin-initiated reset: always revoke all sessions (logout_devices=true).
    // This is intentional — an admin resetting a user's password must force
    // re-auth on all their devices.
    ctx.registration_service.change_password(&user.user_id, None, &body.new_password, None, true).await?;

    Ok(Json(json!({})))
}

/// See [`get_user_rooms_admin`].
#[axum::debug_handler]
pub async fn get_user_rooms_admin(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&ctx, &user_id).await?;

    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100).clamp(1, 500);
    let from = params.get("from").map(|s| s.as_str());

    let room_ids = ctx.admin_user_service.get_user_rooms_paginated(&user.user_id, limit, from).await?;

    let next_batch = if room_ids.len() as i64 == limit { room_ids.last().cloned() } else { None };

    Ok(Json(json!({
        "joined_rooms": room_ids,
        "total": room_ids.len(),
        "next_batch": next_batch
    })))
}

/// See [`get_user_devices_admin`].
#[axum::debug_handler]
pub async fn get_user_devices_admin(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&ctx, &user_id).await?;
    let devices = ctx.admin_user_service.get_user_devices(&user.user_id).await?;

    let device_list: Vec<Value> = devices
        .iter()
        .map(|d| {
            json!({
                "device_id": d.device_id,
                "display_name": d.display_name,
                "last_seen_ts": d.last_seen_ts,
                "last_seen_ip": d.last_seen_ip
            })
        })
        .collect();

    Ok(Json(json!({
        "devices": device_list,
        "total": device_list.len()
    })))
}

/// See [`delete_user_device_admin`].
#[axum::debug_handler]
pub async fn delete_user_device_admin(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path((user_id, device_id)): Path<(String, DeviceId)>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&ctx, &user_id).await?;
    let rows = ctx.token_auth.revoke_device(&user.user_id, device_id.as_str()).await?;

    if rows == 0 {
        return Err(ApiError::not_found("Device not found".to_string()));
    }

    let request_id = resolve_request_id(&headers);
    if let Err(e) = record_audit_event(
        &ctx,
        &admin.user_id,
        "admin.user.delete_device",
        "device",
        device_id.as_str(),
        request_id,
        json!({
            "admin_role": admin.role,
            "target_user": user.user_id,
            "device_id": device_id.as_str(),
        }),
    )
    .await
    {
        tracing::warn!("Failed to record audit event: {}", e);
    }

    Ok(Json(json!({})))
}

/// See [`delete_user_device_admin_compat`].
#[axum::debug_handler]
pub async fn delete_user_device_admin_compat(
    admin: AdminUser,
    state: State<AdminContext>,
    path: Path<(String, DeviceId)>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    delete_user_device_admin(admin, state, path, headers).await
}

/// See [`login_as_user`].
#[axum::debug_handler]
pub async fn login_as_user(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let user = ctx.user_service.get_user_or_not_found(&user_id).await?;

    if user.is_deactivated {
        return Err(ApiError::bad_request("User is deactivated".to_string()));
    }

    let device_id = crate::common::random_string(10);
    let is_admin = user.is_admin;

    // 在插入 access_tokens 之前必须先在 devices 表中创建对应设备记录，
    // 否则 access_tokens.device_id 上的 fk_access_tokens_device 外键约束会拒绝插入。
    // 见 Admin User Login 500 Internal Error 修复 (2026-09-01)。
    ctx.account_device_list_service
        .create_device(&device_id, &user.user_id, Some("Admin Login Device"))
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to create device for admin login", e))?;

    let token = ctx
        .token_auth
        .generate_access_token(&user.user_id, &device_id, is_admin)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to generate token", e))?;

    // A1 (API 路由审计 2026-09-04): 显式记录 admin 互登录事件。
    // `admin_auth_middleware` 会在 HTTP 层面记一次 (POST .../login)，
    // 但 actor_id 来自最终生效的 token（即 target user），无法溯源到发起 admin。
    // 这里用发起 admin 的 user_id 记一条独立 action，便于合规审计。
    // `record_audit_event` 内部已 warn-兜底，不会因审计失败阻塞 API 响应。
    let request_id = resolve_request_id(&headers);
    if let Err(e) = record_audit_event(
        &ctx,
        &admin.user_id,
        "admin.login_as_user",
        "user",
        &user.user_id,
        request_id,
        json!({
            "admin_role": admin.role,
            "target_user": user.user_id,
            "target_is_admin": is_admin,
            "device_id": &device_id,
        }),
    )
    .await
    {
        tracing::warn!(
            target: "admin_audit",
            admin_user = %admin.user_id,
            target_user = %user.user_id,
            error = %e,
            "Failed to record login_as_user audit event"
        );
    }

    Ok(Json(json!({
        "access_token": token,
        "device_id": device_id,
        "user_id": user.user_id
    })))
}

/// See [`logout_user_devices`].
#[axum::debug_handler]
pub async fn logout_user_devices(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&ctx, &user_id).await?;

    let device_count = ctx.admin_user_service.get_user_device_count(&user.user_id).await?;

    // 通过 auth_service 走完整的会话撤销链：access token 黑名单、
    // refresh token 全量吊销、设备清理、logout_all 标记 —
    // 直接 DELETE FROM devices 会留下可继续换发新 access token 的 refresh token。
    ctx.token_auth.logout_all(&user.user_id).await?;

    Ok(Json(json!({
        "devices_deleted": device_count
    })))
}

/// See [`get_users_v2`].
#[axum::debug_handler]
pub async fn get_users_v2(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let cursor = decode_user_cursor(params.get("from").map(String::as_str));

    if params.contains_key("offset") && params.get("offset").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0) > 0 {
        return Err(ApiError::bad_request(
            "Legacy offset pagination is no longer supported; use from cursor".to_string(),
        ));
    }

    let page = ctx.admin_user_service.list_users_v2(limit, cursor, params.get("name").map(String::as_str)).await?;

    let users: Vec<Value> = page
        .users
        .iter()
        .map(|user| {
            json!({
                "name": user.user_id,
                "user_id": user.user_id,
                "creation_ts": user.created_ts,
                "admin": user.is_admin,
                "is_guest": user.is_guest,
                "user_type": user.user_type,
                "deactivated": user.is_deactivated,
                "displayname": user.displayname,
                "avatar_url": user.avatar_url
            })
        })
        .collect();

    Ok(Json(json!({
        "users": users,
        "total": page.total,
        "next_token": page.next_token
    })))
}

/// See [`get_user_v2`].
#[axum::debug_handler]
pub async fn get_user_v2(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    let user = ctx.admin_user_service.get_user_v2(&user_id).await?;

    match user {
        Some(details) => {
            let device_list: Vec<Value> = details
                .devices
                .iter()
                .map(|d| {
                    json!({
                        "device_id": d.device_id,
                        "display_name": d.display_name,
                        "last_seen_ts": d.last_seen_ts
                    })
                })
                .collect();

            Ok(Json(json!({
                "name": details.user.user_id,
                "user_id": details.user.user_id,
                "is_guest": details.user.is_guest,
                "admin": details.user.is_admin,
                "deactivated": details.user.is_deactivated,
                "displayname": details.user.displayname,
                "avatar_url": details.user.avatar_url,
                "created_ts": details.user.created_ts,
                "user_type": details.user.user_type,
                "devices": device_list,
                "threepids": [],
                "external_ids": []
            })))
        }
        None => Err(ApiError::not_found("User not found".to_string())),
    }
}

/// See [`create_or_update_user_v2`].
#[axum::debug_handler]
pub async fn create_or_update_user_v2(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
    Json(body): Json<CreateUpdateUserRequest>,
) -> Result<Json<Value>, ApiError> {
    if body.admin.is_some() || body.user_type.is_some() {
        ensure_super_admin_for_privilege_change(&admin)?;
    }

    ctx.admin_user_service
        .create_or_update_user_v2(
            &user_id,
            body.displayname.as_deref(),
            body.avatar_url.as_deref(),
            body.admin,
            body.deactivated,
            body.user_type.as_deref(),
            body.password.as_deref(),
        )
        .await?;

    Ok(Json(json!({})))
}

/// See [`get_user_stats`].
#[axum::debug_handler]
pub async fn get_user_stats(_admin: AdminUser, State(ctx): State<AdminContext>) -> Result<Json<Value>, ApiError> {
    let stats = ctx.admin_user_service.get_user_stats().await?;

    Ok(Json(json!({
        "total_users": stats.total_users,
        "active_users": stats.active_users,
        "admin_users": stats.admin_users,
        "deactivated_users": stats.deactivated_users,
        "guest_users": stats.guest_users,
        "average_rooms_per_user": stats.average_rooms_per_user,
        "user_registration_enabled": ctx.config.server.enable_registration
    })))
}

/// See [`get_single_user_stats`].
#[axum::debug_handler]
pub async fn get_single_user_stats(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    let stats = ctx.admin_user_service.get_single_user_stats(&user_id).await?;

    Ok(Json(json!({
        "user_id": stats.user.user_id,
        "rooms_joined": stats.rooms_joined,
        "messages_sent": stats.messages_sent,
        "last_seen_ts": stats.last_seen_ts,
        "creation_ts": stats.user.created_ts,
        "is_admin": stats.user.is_admin,
        "dashboard": {
            "total_rooms": stats.rooms_joined,
            "total_messages": stats.messages_sent,
            "last_seen": stats.last_seen_ts
        }
    })))
}

/// Batch create users
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchCreateUsersRequest {
    /// The `users` field.
    pub users: Vec<BatchCreateUser>,
}

/// The `BatchCreateUser` struct.
#[derive(Debug, Deserialize)]
pub struct BatchCreateUser {
    /// The `username` field.
    pub username: String,
    /// The `password` field.
    pub password: Option<String>,
    /// The `displayname` field.
    pub displayname: Option<String>,
    /// The `admin` field.
    pub admin: Option<bool>,
}

/// See [`batch_create_users`].
#[axum::debug_handler]
pub async fn batch_create_users(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Json(body): Json<BatchCreateUsersRequest>,
) -> Result<Json<Value>, ApiError> {
    let has_admin_request = body.users.iter().any(|u| u.admin.unwrap_or(false));
    if has_admin_request {
        ensure_super_admin_for_privilege_change(&admin)?;
    }

    let users: Vec<(String, String, Option<String>, bool)> = body
        .users
        .iter()
        .map(|user| {
            (
                user.username.clone(),
                user.password.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                user.displayname.clone(),
                user.admin.unwrap_or(false),
            )
        })
        .collect();
    let result = ctx.admin_user_service.batch_create_users(&users).await?;

    Ok(Json(json!({
        "created": result.succeeded,
        "failed": result.failed,
        "total": body.users.len()
    })))
}

/// Batch deactivate users
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchDeactivateRequest {
    /// The `users` field.
    pub users: Vec<String>,
    /// The `erase` field.
    pub erase: Option<bool>,
}

/// See [`batch_deactivate_users`].
#[axum::debug_handler]
pub async fn batch_deactivate_users(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Json(body): Json<BatchDeactivateRequest>,
) -> Result<Json<Value>, ApiError> {
    if body.users.len() > 100 {
        return Err(ApiError::bad_request("Too many users in batch request (max 100)".to_string()));
    }

    let result = ctx.admin_user_service.batch_deactivate_users(&body.users).await?;

    Ok(Json(json!({
        "deactivated": result.succeeded,
        "failed": result.failed,
        "total": body.users.len()
    })))
}

/// Get user sessions (devices and connections)
#[axum::debug_handler]
pub async fn get_user_sessions(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&ctx, &user_id).await?;

    let devices = ctx.admin_user_service.get_user_devices(&user.user_id).await?;

    let sessions: Vec<Value> = devices
        .iter()
        .map(|d| {
            json!({
                "device_id": d.device_id,
                "display_name": d.display_name,
                "last_seen_ts": d.last_seen_ts,
                "last_seen_ip": d.last_seen_ip,
                "session_id": d.device_id
            })
        })
        .collect();

    Ok(Json(json!({
        "user_id": user.user_id,
        "sessions": sessions,
        "total": sessions.len()
    })))
}

/// Invalidate all user sessions
#[axum::debug_handler]
pub async fn invalidate_user_sessions(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&ctx, &user_id).await?;
    let canonical_user_id = user.user_id;
    let sessions_removed = ctx.admin_user_service.get_user_device_count(&canonical_user_id).await?;

    ctx.token_auth.logout_all(&canonical_user_id).await?;

    Ok(Json(json!({
        "invalidated": true,
        "sessions_removed": sessions_removed
    })))
}

/// Get account details
#[axum::debug_handler]
pub async fn get_account_details(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&ctx, &user_id).await?;
    let canonical_user_id = &user.user_id;

    let device_count = ctx.admin_user_service.get_user_device_count(canonical_user_id).await?;

    let room_count = ctx.admin_user_service.get_joined_room_count(canonical_user_id).await?;

    Ok(Json(json!({
        "name": user.username,
        "user_id": user.user_id,
        "displayname": user.displayname,
        "admin": user.is_admin,
        "deactivated": user.is_deactivated,
        "creation_ts": user.created_ts,
        "device_count": device_count,
        "room_count": room_count
    })))
}

/// Update account
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateAccountRequest {
    /// The `displayname` field.
    pub displayname: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `admin` field.
    pub admin: Option<bool>,
}

/// See [`update_account`].
#[axum::debug_handler]
pub async fn update_account(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
    headers: HeaderMap,
    Json(body): Json<UpdateAccountRequest>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&ctx, &user_id).await?;
    let canonical_user_id = &user.user_id;

    if body.admin.is_some() {
        ensure_super_admin_for_privilege_change(&admin)?;
    }
    ctx.admin_user_service
        .update_account(canonical_user_id, body.displayname.as_deref(), body.avatar_url.as_deref(), body.admin)
        .await?;

    if let Some(admin_status) = body.admin {
        ctx.cache.set(&format!("user:admin:{canonical_user_id}"), admin_status, 3600).await?;
    }

    let request_id = resolve_request_id(&headers);
    record_audit_event(
        &ctx,
        &admin.user_id,
        "admin.user.update",
        "user",
        canonical_user_id,
        request_id,
        json!({
            "displayname": body.displayname,
            "avatar_url": body.avatar_url,
            "admin": body.admin
        }),
    )
    .await?;

    Ok(Json(json!({
        "user_id": canonical_user_id,
        "updated": true
    })))
}
