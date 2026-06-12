use super::audit::{record_audit_event, resolve_request_id};
use crate::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use synapse_common::constants::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use synapse_common::crypto::hash_password;
use synapse_common::ApiError;
use synapse_storage::User;
use validator::Validate;

fn decode_user_cursor(cursor: Option<&str>) -> Option<(i64, &str)> {
    let cursor = cursor?;
    let (ts, user_id) = cursor.split_once('|')?;
    let ts = ts.parse::<i64>().ok()?;
    if user_id.is_empty() {
        return None;
    }
    Some((ts, user_id))
}

fn encode_user_cursor(created_ts: i64, user_id: &str) -> String {
    format!("{created_ts}|{user_id}")
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_user_cursor, encode_user_cursor};

    #[test]
    fn test_user_cursor_round_trip() {
        let cursor = encode_user_cursor(1_700_000_000_000, "@alice:example.com");
        assert_eq!(decode_user_cursor(Some(&cursor)), Some((1_700_000_000_000, "@alice:example.com")));
    }

    #[test]
    fn test_user_cursor_rejects_invalid_value() {
        assert_eq!(decode_user_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_user_cursor(Some("123|")), None);
    }
}

pub fn create_user_router(_state: AppState) -> Router<AppState> {
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

pub fn admin_user_route_manifest() -> Vec<crate::routes::route_ledger::RouteEntry> {
    use crate::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::GET, "/_synapse/admin/v1/users"),
        (Method::GET, "/_synapse/admin/v1/users/{user_id}"),
        (Method::DELETE, "/_synapse/admin/v1/users/{user_id}"),
        (Method::PUT, "/_synapse/admin/v1/users/{user_id}/admin"),
        (Method::POST, "/_synapse/admin/v1/users/{user_id}/evict"),
        (Method::POST, "/_synapse/admin/v1/users/{user_id}/deactivate"),
        (Method::POST, "/_synapse/admin/v1/users/{user_id}/password"),
        (Method::GET, "/_synapse/admin/v1/users/{user_id}/rooms"),
        (Method::POST, "/_synapse/admin/v1/users/{user_id}/login"),
        (Method::POST, "/_synapse/admin/v1/users/{user_id}/logout"),
        (Method::GET, "/_synapse/admin/v1/users/{user_id}/devices"),
        (Method::POST, "/_synapse/admin/v1/users/{user_id}/devices/delete"),
        (Method::DELETE, "/_synapse/admin/v1/users/{user_id}/devices/{device_id}"),
        (Method::POST, "/_synapse/admin/v1/users/{user_id}/devices/{device_id}/delete"),
        (Method::GET, "/_synapse/admin/v2/users"),
        (Method::GET, "/_synapse/admin/v2/users/{user_id}"),
        (Method::PUT, "/_synapse/admin/v2/users/{user_id}"),
        (Method::DELETE, "/_synapse/admin/v2/users/{user_id}"),
        (Method::GET, "/_synapse/admin/v1/user_stats"),
        (Method::GET, "/_synapse/admin/v1/users/{user_id}/stats"),
        (Method::POST, "/_synapse/admin/v1/users/batch"),
        (Method::POST, "/_synapse/admin/v1/users/batch_deactivate"),
        (Method::GET, "/_synapse/admin/v1/user_sessions/{user_id}"),
        (Method::POST, "/_synapse/admin/v1/user_sessions/{user_id}/invalidate"),
        (Method::GET, "/_synapse/admin/v1/account/{user_id}"),
        (Method::POST, "/_synapse/admin/v1/account/{user_id}"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "admin::user"))
    .collect()
}

#[axum::debug_handler]
async fn evict_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;

    let joined_rooms = state
        .services
        .rooms
        .member_storage
        .get_joined_rooms(&user.user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    let mut failures: Vec<Value> = Vec::new();
    for room_id in &joined_rooms {
        if let Err(e) = state.services.rooms.member_storage.remove_member(room_id, &user.user_id).await {
            failures.push(json!({
                "room_id": room_id,
                "error": e.to_string()
            }));
        } else {
            let _ = state.services.rooms.room_storage.decrement_member_count(room_id).await;
        }
    }

    Ok(Json(json!({
        "user_id": user.user_id,
        "rooms_evicted": joined_rooms.len(),
        "rooms": joined_rooms,
        "failures": failures
    })))
}

#[derive(Debug, Deserialize, Validate)]
pub struct ResetPasswordBody {
    #[validate(length(min = 8, max = 512))]
    #[serde(alias = "newPassword", alias = "new_password")]
    pub new_password: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateUpdateUserRequest {
    #[validate(length(max = 255))]
    pub displayname: Option<String>,
    #[validate(length(max = 2048))]
    pub avatar_url: Option<String>,
    pub admin: Option<bool>,
    pub deactivated: Option<bool>,
    #[validate(length(max = 255))]
    pub user_type: Option<String>,
    #[validate(length(min = 8, max = 512))]
    pub password: Option<String>,
}

async fn resolve_user(state: &AppState, identifier: &str) -> Result<User, ApiError> {
    state
        .services
        .account
        .user_storage
        .get_user_by_identifier(identifier)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?
        .ok_or_else(|| ApiError::not_found("User not found".to_string()))
}

// Moved to admin/mod.rs
use super::ensure_super_admin_for_privilege_change;

#[axum::debug_handler]
pub async fn get_users(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let cursor = decode_user_cursor(params.get("since").map(String::as_str));

    let users = state
        .services
        .account
        .user_storage
        .get_users_paginated(limit, cursor.map(|(ts, _)| ts), cursor.map(|(_, user_id)| user_id))
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    let total = state
        .services
        .account
        .user_storage
        .get_user_count()
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

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
        users.last().map(|u| encode_user_cursor(u.created_ts, &u.user_id))
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
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = state
        .services
        .account
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

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
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;

    tracing::info!(
        admin_user = %admin.user_id,
        target_user = %user.user_id,
        "Admin deleting user"
    );

    state.services.account.user_storage.delete_user(&user.user_id).await.map_err(|e| {
        tracing::error!(
            admin_user = %admin.user_id,
            target_user = %user.user_id,
            error = %e,
            "Failed to delete user"
        );
        ApiError::internal_with_log("Database error", &e)
    })?;

    // 记录审计日志
    let request_id = resolve_request_id(&headers);
    if let Err(e) = record_audit_event(
        &state,
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

#[axum::debug_handler]
pub async fn set_admin(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ensure_super_admin_for_privilege_change(&admin)?;

    let admin_status = body
        .get("admin")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| ApiError::bad_request("Missing 'admin' field".to_string()))?;

    let user = resolve_user(&state, &user_id).await?;

    tracing::info!(
        admin_user = %admin.user_id,
        target_user = %user.user_id,
        admin_status = admin_status,
        "Admin changing user admin status"
    );

    state.services.account.user_storage.set_admin_status(&user.user_id, admin_status).await.map_err(|e| {
        tracing::error!(
            admin_user = %admin.user_id,
            target_user = %user.user_id,
            error = %e,
            "Failed to set admin status"
        );
        ApiError::internal_with_log("Database error", &e)
    })?;
    state.cache.set(&format!("user:admin:{}", user.user_id), admin_status, 3600).await?;

    // 记录审计日志
    let request_id = resolve_request_id(&headers);
    if let Err(e) = record_audit_event(
        &state,
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

#[axum::debug_handler]
pub async fn deactivate_user(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;

    tracing::info!(
        admin_user = %admin.user_id,
        target_user = %user.user_id,
        "Admin deactivating user"
    );

    state.services.core.auth_service.deactivate_user(&user.user_id).await.map_err(|e| {
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
        &state,
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

#[axum::debug_handler]
pub async fn reset_user_password(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<ResetPasswordBody>,
) -> Result<Json<Value>, ApiError> {
    state.services.core.auth_service.validator.validate_password(&body.new_password)?;

    let user = resolve_user(&state, &user_id).await?;

    state.services.core.registration_service.change_password(&user.user_id, None, &body.new_password, None).await?;

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_user_rooms_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;

    let rooms = state
        .services
        .rooms
        .room_storage
        .get_user_rooms(&user.user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    Ok(Json(json!({ "rooms": rooms })))
}

#[axum::debug_handler]
pub async fn get_user_devices_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;
    let devices = state
        .services
        .account
        .device_storage
        .get_user_devices(&user.user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

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

#[axum::debug_handler]
pub async fn delete_user_device_admin(
    admin: AdminUser,
    State(state): State<AppState>,
    Path((user_id, device_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;
    let rows = state.services.core.auth_service.revoke_device(&user.user_id, &device_id).await?;

    if rows == 0 {
        return Err(ApiError::not_found("Device not found".to_string()));
    }

    let request_id = resolve_request_id(&headers);
    if let Err(e) = record_audit_event(
        &state,
        &admin.user_id,
        "admin.user.delete_device",
        "device",
        &device_id,
        request_id,
        json!({
            "admin_role": admin.role,
            "target_user": user.user_id,
            "device_id": device_id,
        }),
    )
    .await
    {
        tracing::warn!("Failed to record audit event: {}", e);
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn delete_user_device_admin_compat(
    admin: AdminUser,
    state: State<AppState>,
    path: Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    delete_user_device_admin(admin, state, path, headers).await
}

#[axum::debug_handler]
pub async fn login_as_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = state
        .services
        .account
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?
        .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

    if user.is_deactivated {
        return Err(ApiError::bad_request("User is deactivated".to_string()));
    }

    let device_id = synapse_common::random_string(10);
    let is_admin = user.is_admin;

    let token = state
        .services
        .core
        .auth_service
        .generate_access_token(&user.user_id, &device_id, is_admin)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to generate token", &e))?;

    Ok(Json(json!({
        "access_token": token,
        "device_id": device_id,
        "user_id": user.user_id
    })))
}

#[axum::debug_handler]
pub async fn logout_user_devices(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;

    let device_count: i64 = state
        .services
        .account
        .device_storage
        .get_device_count(&user.user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    // 通过 auth_service 走完整的会话撤销链：access token 黑名单、
    // refresh token 全量吊销、设备清理、logout_all 标记 —
    // 直接 DELETE FROM devices 会留下可继续换发新 access token 的 refresh token。
    state.services.core.auth_service.logout_all(&user.user_id).await?;

    Ok(Json(json!({
        "devices_deleted": device_count
    })))
}

#[axum::debug_handler]
pub async fn get_users_v2(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let cursor = decode_user_cursor(params.get("from").map(String::as_str));

    let mut query = sqlx::QueryBuilder::new(
        "SELECT user_id, username, created_ts, is_admin, updated_ts, is_guest, user_type, is_deactivated, displayname, avatar_url FROM users WHERE 1=1"
    );

    if let Some(name) = params.get("name") {
        query.push(" AND username LIKE ");
        query.push_bind(format!("%{name}%"));
    }

    if let Some((ts, user_id)) = cursor {
        query.push(" AND (created_ts < ");
        query.push_bind(ts);
        query.push(" OR (created_ts = ");
        query.push_bind(ts);
        query.push(" AND user_id < ");
        query.push_bind(user_id);
        query.push("))");
    }

    query.push(" ORDER BY created_ts DESC, user_id DESC LIMIT ");
    query.push_bind(limit);

    let rows = query
        .build()
        .fetch_all(&*state.services.account.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    let users: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name": row.get::<Option<String>, _>("user_id"),
                "user_id": row.get::<Option<String>, _>("user_id"),
                "creation_ts": row.get::<Option<i64>, _>("created_ts"),
                "admin": row.get::<Option<bool>, _>("is_admin").unwrap_or(false),
                "is_guest": row.get::<Option<bool>, _>("is_guest").unwrap_or(false),
                "user_type": row.get::<Option<String>, _>("user_type"),
                "deactivated": row.get::<Option<bool>, _>("is_deactivated").unwrap_or(false),
                "displayname": row.get::<Option<String>, _>("displayname"),
                "avatar_url": row.get::<Option<String>, _>("avatar_url")
            })
        })
        .collect();

    let total_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&*state.services.account.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    let next_token = if rows.len() as i64 == limit {
        rows.last().map(|row| {
            encode_user_cursor(
                row.get::<Option<i64>, _>("created_ts").unwrap_or_default(),
                &row.get::<Option<String>, _>("user_id").unwrap_or_default(),
            )
        })
    } else {
        None
    };

    Ok(Json(json!({
        "users": users,
        "total": total_count,
        "next_token": next_token
    })))
}

#[axum::debug_handler]
pub async fn get_user_v2(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = state
        .services
        .account
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    match user {
        Some(u) => {
            let devices = state
                .services
                .account
                .device_storage
                .get_user_devices(&u.user_id)
                .await
                .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

            let device_list: Vec<Value> = devices
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
                "name": u.user_id,
                "user_id": u.user_id,
                "is_guest": u.is_guest,
                "admin": u.is_admin,
                "deactivated": u.is_deactivated,
                "displayname": u.displayname,
                "avatar_url": u.avatar_url,
                "created_ts": u.created_ts,
                "user_type": u.user_type,
                "devices": device_list,
                "threepids": [],
                "external_ids": []
            })))
        }
        None => Err(ApiError::not_found("User not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn create_or_update_user_v2(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<CreateUpdateUserRequest>,
) -> Result<Json<Value>, ApiError> {
    if body.admin.is_some() || body.user_type.is_some() {
        ensure_super_admin_for_privilege_change(&admin)?;
    }

    let now = chrono::Utc::now().timestamp_millis();

    let existing_user = state
        .services
        .account
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    if let Some(_user) = existing_user {
        sqlx::query(
            r"
            UPDATE users SET
                displayname = COALESCE($2, displayname),
                avatar_url = COALESCE($3, avatar_url),
                is_admin = COALESCE($4, is_admin),
                is_deactivated = COALESCE($5, is_deactivated),
                user_type = COALESCE($6, user_type),
                updated_ts = $7
            WHERE username = $1 OR user_id = $1
            ",
        )
        .bind(&user_id)
        .bind(&body.displayname)
        .bind(&body.avatar_url)
        .bind(body.admin)
        .bind(body.deactivated)
        .bind(&body.user_type)
        .bind(now)
        .execute(&*state.services.account.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update user", &e))?;

        Ok(Json(json!({})))
    } else {
        let user_id_full = if user_id.starts_with('@') {
            user_id.clone()
        } else {
            format!("@{}:{}", user_id, state.services.core.config.server.name)
        };

        let username = user_id_full.strip_prefix('@').and_then(|s| s.split(':').next()).unwrap_or(&user_id).to_string();

        let password_hash = if let Some(ref pwd) = body.password {
            synapse_common::crypto::hash_password(pwd)
                .map_err(|e| ApiError::internal_with_log("Password hashing failed", &e))?
        } else {
            synapse_common::crypto::hash_password(&synapse_common::random_string(16))
                .map_err(|e| ApiError::internal_with_log("Password hashing failed", &e))?
        };

        sqlx::query(
            r"
            INSERT INTO users (user_id, username, password_hash, displayname, avatar_url, is_admin, is_deactivated, user_type, created_ts, updated_ts, generation)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 0)
            ",
        )
        .bind(&user_id_full)
        .bind(&username)
        .bind(&password_hash)
        .bind(&body.displayname)
        .bind(&body.avatar_url)
        .bind(body.admin.unwrap_or(false))
        .bind(body.deactivated.unwrap_or(false))
        .bind(&body.user_type)
        .bind(now)
        .bind(now)
        .execute(&*state.services.account.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create user", &e))?;

        Ok(Json(json!({})))
    }
}

#[axum::debug_handler]
pub async fn get_user_stats(_admin: AdminUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let stats = sqlx::query(
        r"
        SELECT
            COUNT(*)::BIGINT AS total_users,
            COUNT(*) FILTER (WHERE COALESCE(is_deactivated, FALSE) = FALSE)::BIGINT AS active_users,
            COUNT(*) FILTER (WHERE COALESCE(is_admin, FALSE) = TRUE)::BIGINT AS admin_users,
            COUNT(*) FILTER (WHERE COALESCE(is_deactivated, FALSE) = TRUE)::BIGINT AS deactivated_users,
            COUNT(*) FILTER (WHERE COALESCE(is_guest, FALSE) = TRUE)::BIGINT AS guest_users
        FROM users
        ",
    )
    .fetch_one(&*state.services.account.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal_with_log("Failed to get user stats", &e))?;

    let total_users = stats.get::<i64, _>("total_users");
    let active_users = stats.get::<i64, _>("active_users");
    let admin_users = stats.get::<i64, _>("admin_users");
    let deactivated_users = stats.get::<i64, _>("deactivated_users");
    let guest_users = stats.get::<i64, _>("guest_users");

    let room_count = state
        .services
        .rooms
        .room_storage
        .get_room_count()
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get room count", &e))?;

    let average_rooms_per_user = if total_users > 0 { (room_count as f64 / total_users as f64).round() } else { 0.0 };

    Ok(Json(json!({
        "total_users": total_users,
        "active_users": active_users,
        "admin_users": admin_users,
        "deactivated_users": deactivated_users,
        "guest_users": guest_users,
        "average_rooms_per_user": average_rooms_per_user,
        "user_registration_enabled": state.services.core.config.server.enable_registration
    })))
}

#[axum::debug_handler]
pub async fn get_single_user_stats(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;

    let rooms_joined: i64 = state
        .services
        .rooms
        .member_storage
        .get_joined_room_count(&user.user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to count rooms", &e))?;

    let pool = &*state.services.rooms.room_storage.pool;

    let messages_sent: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE sender = $1 AND event_type = 'm.room.message' AND is_redacted = false",
    )
    .bind(&user.user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError::internal_with_log("Failed to count messages", &e))?;

    let last_seen: Option<i64> =
        sqlx::query_scalar("SELECT last_seen_ts FROM devices WHERE user_id = $1 ORDER BY last_seen_ts DESC LIMIT 1")
            .bind(&user.user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get last seen", &e))?;

    Ok(Json(json!({
        "user_id": user.user_id,
        "rooms_joined": rooms_joined,
        "messages_sent": messages_sent,
        "last_seen_ts": last_seen,
        "creation_ts": user.created_ts,
        "is_admin": user.is_admin,
        "dashboard": {
            "total_rooms": rooms_joined,
            "total_messages": messages_sent,
            "last_seen": last_seen
        }
    })))
}

/// Batch create users
#[derive(Debug, Deserialize)]
pub struct BatchCreateUsersRequest {
    pub users: Vec<BatchCreateUser>,
}

#[derive(Debug, Deserialize)]
pub struct BatchCreateUser {
    pub username: String,
    pub password: Option<String>,
    pub displayname: Option<String>,
    pub admin: Option<bool>,
}

#[axum::debug_handler]
pub async fn batch_create_users(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<BatchCreateUsersRequest>,
) -> Result<Json<Value>, ApiError> {
    let has_admin_request = body.users.iter().any(|u| u.admin.unwrap_or(false));
    if has_admin_request {
        ensure_super_admin_for_privilege_change(&admin)?;
    }

    let mut created = Vec::new();
    let mut failed = Vec::new();
    let now = chrono::Utc::now().timestamp_millis();

    for user in &body.users {
        let password = user.password.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let username = user.username.clone();

        let password_hash =
            hash_password(&password).map_err(|e| ApiError::internal_with_log("Failed to hash password", &e))?;

        let result = sqlx::query(
            r"
            INSERT INTO users (user_id, username, password_hash, displayname, is_admin, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (username) DO NOTHING
            ",
        )
        .bind(format!("@{}:{}", username, state.services.core.config.server.name))
        .bind(&username)
        .bind(&password_hash)
        .bind(user.displayname.as_deref().unwrap_or(&username))
        .bind(user.admin.unwrap_or(false))
        .bind(now)
        .bind(now)
        .execute(&*state.services.account.user_storage.pool)
        .await;

        match result {
            Ok(r) if r.rows_affected() > 0 => created.push(username.clone()),
            Ok(_) => failed.push(username),
            Err(_) => failed.push(username),
        }
    }

    Ok(Json(json!({
        "created": created,
        "failed": failed,
        "total": body.users.len()
    })))
}

/// Batch deactivate users
#[derive(Debug, Deserialize)]
pub struct BatchDeactivateRequest {
    pub users: Vec<String>,
    pub erase: Option<bool>,
}

#[axum::debug_handler]
pub async fn batch_deactivate_users(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<BatchDeactivateRequest>,
) -> Result<Json<Value>, ApiError> {
    let mut deactivated = Vec::new();
    let mut failed = Vec::new();

    if body.users.len() > 100 {
        return Err(ApiError::bad_request("Too many users in batch request (max 100)".to_string()));
    }

    for user_id in &body.users {
        if !user_id.starts_with('@') || !user_id.contains(':') {
            failed.push(user_id.clone());
            continue;
        }

        let result = sqlx::query("UPDATE users SET is_deactivated = true WHERE user_id = $1")
            .bind(user_id)
            .execute(&*state.services.account.user_storage.pool)
            .await;

        match result {
            Ok(r) if r.rows_affected() > 0 => deactivated.push(user_id.clone()),
            _ => failed.push(user_id.clone()),
        }
    }

    Ok(Json(json!({
        "deactivated": deactivated,
        "failed": failed,
        "total": body.users.len()
    })))
}

/// Get user sessions (devices and connections)
#[axum::debug_handler]
pub async fn get_user_sessions(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;

    let devices = state
        .services
        .account
        .device_storage
        .get_user_devices(&user.user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

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
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;
    let canonical_user_id = user.user_id;
    let sessions_removed: i64 = state
        .services
        .account
        .device_storage
        .get_device_count(&canonical_user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    state.services.core.auth_service.logout_all(&canonical_user_id).await?;

    Ok(Json(json!({
        "invalidated": true,
        "sessions_removed": sessions_removed
    })))
}

/// Get account details
#[axum::debug_handler]
pub async fn get_account_details(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;
    let canonical_user_id = &user.user_id;

    let device_count: i64 = state
        .services
        .account
        .device_storage
        .get_device_count(canonical_user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    let room_count: i64 = state
        .services
        .rooms
        .member_storage
        .get_joined_room_count(canonical_user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

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
pub struct UpdateAccountRequest {
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub admin: Option<bool>,
}

#[axum::debug_handler]
pub async fn update_account(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UpdateAccountRequest>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;
    let canonical_user_id = &user.user_id;

    if let Some(displayname) = &body.displayname {
        sqlx::query("UPDATE users SET displayname = $1 WHERE user_id = $2")
            .bind(displayname)
            .bind(canonical_user_id)
            .execute(&*state.services.account.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
    }

    if let Some(admin_status) = body.admin {
        ensure_super_admin_for_privilege_change(&admin)?;
        sqlx::query("UPDATE users SET is_admin = $1 WHERE user_id = $2")
            .bind(admin_status)
            .bind(canonical_user_id)
            .execute(&*state.services.account.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
        state.cache.set(&format!("user:admin:{canonical_user_id}"), admin_status, 3600).await?;
    }

    let request_id = resolve_request_id(&headers);
    record_audit_event(
        &state,
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
