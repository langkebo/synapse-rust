use super::audit::{record_audit_event, resolve_request_id};
use crate::common::constants::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use crate::common::crypto::hash_password;
use crate::common::ApiError;
use crate::storage::models::User;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use validator::Validate;

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

#[axum::debug_handler]
async fn evict_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;

    let joined_rooms = state
        .services
        .member_storage
        .get_joined_rooms(&user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let mut failures: Vec<Value> = Vec::new();
    for room_id in &joined_rooms {
        if let Err(e) = state
            .services
            .member_storage
            .remove_member(room_id, &user.user_id)
            .await
        {
            failures.push(json!({
                "room_id": room_id,
                "error": e.to_string()
            }));
        } else {
            let _ = state
                .services
                .room_storage
                .decrement_member_count(room_id)
                .await;
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
        .user_storage
        .get_user_by_identifier(identifier)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| ApiError::not_found("User not found".to_string()))
}

fn ensure_super_admin_for_privilege_change(admin: &AdminUser) -> Result<(), ApiError> {
    if admin.role != "super_admin" {
        return Err(ApiError::forbidden(
            "Only super_admin can modify admin privileges or admin roles".to_string(),
        ));
    }

    Ok(())
}

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
    let offset = params
        .get("offset")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        .clamp(0, i64::MAX);

    let users = state
        .services
        .user_storage
        .get_users_paginated(limit, offset)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let total = state
        .services
        .user_storage
        .get_user_count()
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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

    Ok(Json(json!({
        "users": user_list,
        "total": total
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
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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
) -> Result<Json<Value>, ApiError> {
    ensure_super_admin_for_privilege_change(&admin)?;

    let user = resolve_user(&state, &user_id).await?;

    state
        .services
        .user_storage
        .delete_user(&user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ensure_super_admin_for_privilege_change(&admin)?;

    let admin_status = body
        .get("admin")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| ApiError::bad_request("Missing 'admin' field".to_string()))?;

    let user = resolve_user(&state, &user_id).await?;

    state
        .services
        .user_storage
        .set_admin_status(&user.user_id, admin_status)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    state
        .cache
        .set(&format!("user:admin:{}", user.user_id), admin_status, 3600)
        .await?;

    Ok(Json(json!({ "success": true })))
}

#[axum::debug_handler]
pub async fn deactivate_user(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_super_admin_for_privilege_change(&admin)?;

    let user = resolve_user(&state, &user_id).await?;

    state
        .services
        .auth_service
        .deactivate_user(&user.user_id)
        .await?;

    Ok(Json(json!({ "id_server_unbind_result": "success" })))
}

#[axum::debug_handler]
pub async fn reset_user_password(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<ResetPasswordBody>,
) -> Result<Json<Value>, ApiError> {
    ensure_super_admin_for_privilege_change(&admin)?;

    state
        .services
        .auth_service
        .validator
        .validate_password(&body.new_password)?;

    let user = resolve_user(&state, &user_id).await?;

    state
        .services
        .registration_service
        .change_password(&user.user_id, None, &body.new_password)
        .await?;

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
        .room_storage
        .get_user_rooms(&user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({ "rooms": rooms })))
}

#[axum::debug_handler]
pub async fn get_user_devices_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;
    let devices = sqlx::query(
        r#"
        SELECT device_id, display_name, last_seen_ts, last_seen_ip, user_id
        FROM devices 
        WHERE user_id = $1
        ORDER BY last_seen_ts DESC
        "#,
    )
    .bind(&user.user_id)
    .fetch_all(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let device_list: Vec<Value> = devices
        .iter()
        .map(|row| {
            json!({
                "device_id": row.get::<Option<String>, _>("device_id"),
                "display_name": row.get::<Option<String>, _>("display_name"),
                "last_seen_ts": row.get::<Option<i64>, _>("last_seen_ts"),
                "last_seen_ip": row.get::<Option<String>, _>("last_seen_ip")
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
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((user_id, device_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;
    let result = sqlx::query("DELETE FROM devices WHERE user_id = $1 AND device_id = $2")
        .bind(&user.user_id)
        .bind(&device_id)
        .execute(&*state.services.device_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Device not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn delete_user_device_admin_compat(
    admin: AdminUser,
    state: State<AppState>,
    path: Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    delete_user_device_admin(admin, state, path).await
}

#[axum::debug_handler]
pub async fn login_as_user(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_super_admin_for_privilege_change(&admin)?;

    let user = state
        .services
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

    if user.is_deactivated {
        return Err(ApiError::bad_request("User is deactivated".to_string()));
    }

    let device_id = crate::common::random_string(10);
    let is_admin = user.is_admin;

    let token = state
        .services
        .auth_service
        .generate_access_token(&user.username, &device_id, is_admin)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to generate token: {}", e)))?;

    Ok(Json(json!({
        "access_token": token,
        "device_id": device_id,
        "user_id": user.username
    })))
}

#[axum::debug_handler]
pub async fn logout_user_devices(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_super_admin_for_privilege_change(&admin)?;

    let user = resolve_user(&state, &user_id).await?;
    let result = sqlx::query("DELETE FROM devices WHERE user_id = $1")
        .bind(&user.user_id)
        .execute(&*state.services.device_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "devices_deleted": result.rows_affected()
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
    let offset = params
        .get("from")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        .clamp(0, i64::MAX);

    let mut query = sqlx::QueryBuilder::new(
        "SELECT user_id, username, created_ts, is_admin, updated_ts, is_guest, user_type, is_deactivated, displayname, avatar_url FROM users WHERE 1=1"
    );

    if let Some(name) = params.get("name") {
        query.push(" AND username LIKE ");
        query.push_bind(format!("%{}%", name));
    }

    query.push(" ORDER BY created_ts DESC LIMIT ");
    query.push_bind(limit);
    query.push(" OFFSET ");
    query.push_bind(offset);

    let rows = query
        .build()
        .fetch_all(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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
        .fetch_one(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let next_token = if (offset + limit) < total_count {
        Some((offset + limit).to_string())
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
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match user {
        Some(u) => {
            let devices = sqlx::query(
                "SELECT device_id, display_name, last_seen_ts, user_id FROM devices WHERE user_id = $1"
            )
            .bind(&u.user_id)
            .fetch_all(&*state.services.device_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

            let device_list: Vec<Value> = devices
                .iter()
                .map(|row| {
                    json!({
                        "device_id": row.get::<Option<String>, _>("device_id"),
                        "display_name": row.get::<Option<String>, _>("display_name"),
                        "last_seen_ts": row.get::<Option<i64>, _>("last_seen_ts")
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
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if let Some(_user) = existing_user {
        sqlx::query(
            r#"
            UPDATE users SET
                displayname = COALESCE($2, displayname),
                avatar_url = COALESCE($3, avatar_url),
                is_admin = COALESCE($4, is_admin),
                is_deactivated = COALESCE($5, is_deactivated),
                user_type = COALESCE($6, user_type),
                updated_ts = $7
            WHERE username = $1 OR user_id = $1
            "#,
        )
        .bind(&user_id)
        .bind(&body.displayname)
        .bind(&body.avatar_url)
        .bind(body.admin)
        .bind(body.deactivated)
        .bind(&body.user_type)
        .bind(now)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update user: {}", e)))?;

        Ok(Json(json!({})))
    } else {
        let user_id_full = if user_id.starts_with('@') {
            user_id.clone()
        } else {
            format!("@{}:{}", user_id, state.services.config.server.name)
        };

        let username = user_id_full
            .strip_prefix('@')
            .and_then(|s| s.split(':').next())
            .unwrap_or(&user_id)
            .to_string();

        let password_hash = if let Some(ref pwd) = body.password {
            crate::common::crypto::hash_password(pwd)
                .map_err(|e| ApiError::internal(format!("Password hashing failed: {}", e)))?
        } else {
            crate::common::crypto::hash_password(&crate::common::random_string(16))
                .map_err(|e| ApiError::internal(format!("Password hashing failed: {}", e)))?
        };

        sqlx::query(
            r#"
            INSERT INTO users (user_id, username, password_hash, displayname, avatar_url, is_admin, is_deactivated, user_type, created_ts, updated_ts, generation)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 0)
            "#,
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
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create user: {}", e)))?;

        Ok(Json(json!({})))
    }
}

#[axum::debug_handler]
pub async fn get_user_stats(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let stats = sqlx::query(
        r#"
        SELECT
            COUNT(*)::BIGINT AS total_users,
            COUNT(*) FILTER (WHERE COALESCE(is_deactivated, FALSE) = FALSE)::BIGINT AS active_users,
            COUNT(*) FILTER (WHERE COALESCE(is_admin, FALSE) = TRUE)::BIGINT AS admin_users,
            COUNT(*) FILTER (WHERE COALESCE(is_deactivated, FALSE) = TRUE)::BIGINT AS deactivated_users,
            COUNT(*) FILTER (WHERE COALESCE(is_guest, FALSE) = TRUE)::BIGINT AS guest_users
        FROM users
        "#,
    )
    .fetch_one(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get user stats: {}", e)))?;

    let total_users = stats.get::<i64, _>("total_users");
    let active_users = stats.get::<i64, _>("active_users");
    let admin_users = stats.get::<i64, _>("admin_users");
    let deactivated_users = stats.get::<i64, _>("deactivated_users");
    let guest_users = stats.get::<i64, _>("guest_users");

    let room_count = state
        .services
        .room_storage
        .get_room_count()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room count: {}", e)))?;

    let average_rooms_per_user = if total_users > 0 {
        (room_count as f64 / total_users as f64).round()
    } else {
        0.0
    };

    Ok(Json(json!({
        "total_users": total_users,
        "active_users": active_users,
        "admin_users": admin_users,
        "deactivated_users": deactivated_users,
        "guest_users": guest_users,
        "average_rooms_per_user": average_rooms_per_user,
        "user_registration_enabled": state.services.config.server.enable_registration
    })))
}

#[axum::debug_handler]
pub async fn get_single_user_stats(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = resolve_user(&state, &user_id).await?;

    let pool = &*state.services.room_storage.pool;

    let rooms_joined: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM room_memberships WHERE user_id = $1 AND membership = 'join'",
    )
    .bind(&user.user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to count rooms: {}", e)))?;

    let messages_sent: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM room_events WHERE sender = $1 AND type = 'm.room.message'",
    )
    .bind(&user.user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to count messages: {}", e)))?;

    let last_seen: Option<i64> = sqlx::query_scalar(
        "SELECT last_seen_ts FROM devices WHERE user_id = $1 ORDER BY last_seen_ts DESC LIMIT 1",
    )
    .bind(&user.user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get last seen: {}", e)))?;

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
        let password = user
            .password
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let username = user.username.clone();

        let password_hash = hash_password(&password)
            .map_err(|e| ApiError::internal(format!("Failed to hash password: {}", e)))?;

        let result = sqlx::query(
            r#"
            INSERT INTO users (user_id, username, password_hash, displayname, is_admin, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (username) DO NOTHING
            "#
        )
        .bind(format!("@{}:{}", username, state.services.config.server.name))
        .bind(&username)
        .bind(&password_hash)
        .bind(user.displayname.as_deref().unwrap_or(&username))
        .bind(user.admin.unwrap_or(false))
        .bind(now)
        .bind(now)
        .execute(&*state.services.user_storage.pool)
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
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<BatchDeactivateRequest>,
) -> Result<Json<Value>, ApiError> {
    ensure_super_admin_for_privilege_change(&admin)?;

    let mut deactivated = Vec::new();
    let mut failed = Vec::new();

    if body.users.len() > 100 {
        return Err(ApiError::bad_request(
            "Too many users in batch request (max 100)".to_string(),
        ));
    }

    for user_id in &body.users {
        if !user_id.starts_with('@') || !user_id.contains(':') {
            failed.push(user_id.clone());
            continue;
        }

        let result = sqlx::query("UPDATE users SET is_deactivated = true WHERE user_id = $1")
            .bind(user_id)
            .execute(&*state.services.user_storage.pool)
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

    let devices = sqlx::query(
        "SELECT device_id, display_name, last_seen_ts, last_seen_ip FROM devices WHERE user_id = $1"
    )
    .bind(&user.user_id)
    .fetch_all(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let sessions: Vec<Value> = devices
        .iter()
        .map(|row| {
            json!({
                "device_id": row.get::<Option<String>, _>("device_id"),
                "display_name": row.get::<Option<String>, _>("display_name"),
                "last_seen_ts": row.get::<Option<i64>, _>("last_seen_ts"),
                "last_seen_ip": row.get::<Option<String>, _>("last_seen_ip"),
                "session_id": row.get::<Option<String>, _>("device_id")
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
    let sessions_removed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE user_id = $1")
            .bind(&canonical_user_id)
            .fetch_one(&*state.services.device_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    state
        .services
        .auth_service
        .logout_all(&canonical_user_id)
        .await?;

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

    let device_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE user_id = $1")
        .bind(canonical_user_id)
        .fetch_one(&*state.services.device_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let room_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM room_memberships WHERE user_id = $1 AND membership = 'join'",
    )
    .bind(canonical_user_id)
    .fetch_one(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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
            .execute(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    }

    if let Some(admin_status) = body.admin {
        ensure_super_admin_for_privilege_change(&admin)?;
        sqlx::query("UPDATE users SET is_admin = $1 WHERE user_id = $2")
            .bind(admin_status)
            .bind(canonical_user_id)
            .execute(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
        state
            .cache
            .set(&format!("user:admin:{}", canonical_user_id), admin_status, 3600)
            .await?;
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
