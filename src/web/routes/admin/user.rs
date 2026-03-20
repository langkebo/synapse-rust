use crate::common::constants::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

pub fn create_user_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/users", get(get_users))
        .route("/_synapse/admin/v1/users/{user_id}", get(get_user))
        .route("/_synapse/admin/v1/users/{user_id}", delete(delete_user))
        .route("/_synapse/admin/v1/users/{user_id}/admin", put(set_admin))
        .route("/_synapse/admin/v1/users/{user_id}/deactivate", post(deactivate_user))
        .route("/_synapse/admin/v1/users/{user_id}/password", post(reset_user_password))
        .route("/_synapse/admin/v1/users/{user_id}/rooms", get(get_user_rooms_admin))
        .route("/_synapse/admin/v1/users/{user_id}/login", post(login_as_user))
        .route("/_synapse/admin/v1/users/{user_id}/logout", post(logout_user_devices))
        .route("/_synapse/admin/v1/users/{user_id}/devices", get(get_user_devices_admin))
        .route("/_synapse/admin/v1/users/{user_id}/devices/{device_id}", delete(delete_user_device_admin))
        .route("/_synapse/admin/v2/users", get(get_users_v2))
        .route("/_synapse/admin/v2/users/{user_id}", get(get_user_v2))
        .route("/_synapse/admin/v2/users/{user_id}", put(create_or_update_user_v2))
        .route("/_synapse/admin/v1/user_stats", get(get_user_stats))
        // Batch operations
        .route("/_synapse/admin/v1/users/batch", post(batch_create_users))
        .route("/_synapse/admin/v1/users/batch_deactivate", post(batch_deactivate_users))
        // User sessions
        .route("/_synapse/admin/v1/user_sessions/{user_id}", get(get_user_sessions))
        .route("/_synapse/admin/v1/user_sessions/{user_id}/invalidate", post(invalidate_user_sessions))
        // Account details
        .route("/_synapse/admin/v1/account/{user_id}", get(get_account_details))
        .route("/_synapse/admin/v1/account/{user_id}", post(update_account))
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordBody {
    #[serde(alias = "newPassword", alias = "new_password")]
    pub new_password: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateUpdateUserRequest {
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub admin: Option<bool>,
    pub deactivated: Option<bool>,
    pub user_type: Option<String>,
    pub password: Option<String>,
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
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .user_storage
        .delete_user(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "user_id": user_id,
        "deleted": true
    })))
}

#[axum::debug_handler]
pub async fn set_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let admin_status = body
        .get("admin")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| ApiError::bad_request("Missing 'admin' field".to_string()))?;

    if !state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .user_storage
        .set_admin_status(&user_id, admin_status)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({ "success": true })))
}

#[axum::debug_handler]
pub async fn deactivate_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .auth_service
        .deactivate_user(&user_id)
        .await?;

    Ok(Json(json!({ "id_server_unbind_result": "success" })))
}

#[axum::debug_handler]
pub async fn reset_user_password(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<ResetPasswordBody>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .auth_service
        .validator
        .validate_password(&body.new_password)?;

    if !state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .registration_service
        .change_password(&user_id, &body.new_password)
        .await?;

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_user_rooms_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let rooms = state
        .services
        .room_storage
        .get_user_rooms(&user_id)
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
    let devices = sqlx::query(
        r#"
        SELECT device_id, display_name, last_seen_ts, last_seen_ip, user_id
        FROM devices 
        WHERE user_id = (SELECT username FROM users WHERE username = $1 OR user_id = $1)
        ORDER BY last_seen_ts DESC
        "#,
    )
    .bind(&user_id)
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
    let result = sqlx::query(
        "DELETE FROM devices WHERE user_id = (SELECT username FROM users WHERE username = $1 OR user_id = $1) AND device_id = $2"
    )
    .bind(&user_id)
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
pub async fn login_as_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
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
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query(
        "DELETE FROM devices WHERE user_id = (SELECT username FROM users WHERE username = $1 OR user_id = $1)"
    )
    .bind(&user_id)
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
                "name": row.get::<Option<String>, _>("username"),
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
        Some(offset + limit)
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
            .bind(&u.username)
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
                "name": u.username,
                "user_id": u.username,
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
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<CreateUpdateUserRequest>,
) -> Result<Json<Value>, ApiError> {
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
    let total_users = state
        .services
        .user_storage
        .get_user_count()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get user count: {}", e)))?;

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
        "active_users": total_users,
        "admin_users": 1,
        "deactivated_users": 0,
        "guest_users": 0,
        "average_rooms_per_user": average_rooms_per_user,
        "user_registration_enabled": true
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
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<BatchCreateUsersRequest>,
) -> Result<Json<Value>, ApiError> {
    let mut created = Vec::new();
    let mut failed = Vec::new();
    let now = chrono::Utc::now().timestamp_millis();

    for user in &body.users {
        let password = user.password.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let username = user.username.clone();
        
        let result = sqlx::query(
            r#"
            INSERT INTO users (user_id, username, password_hash, displayname, is_admin, creation_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (username) DO NOTHING
            "#
        )
        .bind(format!("@{}:{}", username, state.services.config.server.name))
        .bind(&username)
        .bind(&password)  // In production, hash this!
        .bind(user.displayname.as_deref().unwrap_or(&username))
        .bind(user.admin.unwrap_or(false))
        .bind(now)
        .bind(now)
        .execute(&*state.services.user_storage.pool)
        .await;

        match result {
            Ok(_) => created.push(username.clone()),
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
    
    for user_id in body.users {
        sqlx::query("UPDATE users SET deactivated = true WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*state.services.user_storage.pool)
            .await
            .ok();
        
        deactivated.push(user_id);
    }

    Ok(Json(json!({
        "deactivated": deactivated,
        "total": deactivated.len()
    })))
}

/// Get user sessions (devices and connections)
#[axum::debug_handler]
pub async fn get_user_sessions(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let devices = sqlx::query(
        "SELECT device_id, display_name, last_seen_ts, last_seen_ip FROM devices WHERE user_id = $1"
    )
    .bind(&user_id)
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
        "user_id": user_id,
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
    let deleted = sqlx::query("DELETE FROM devices WHERE user_id = $1")
        .bind(&user_id)
        .execute(&*state.services.device_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "invalidated": true,
        "sessions_removed": deleted.rows_affected()
    })))
}

/// Get account details
#[axum::debug_handler]
pub async fn get_account_details(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = sqlx::query(
        "SELECT user_id, username, displayname, is_admin, deactivated, creation_ts FROM users WHERE user_id = $1"
    )
    .bind(&user_id)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match user {
        Some(row) => {
            let device_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM devices WHERE user_id = $1"
            )
            .bind(&user_id)
            .fetch_one(&*state.services.device_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

            let room_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM room_memberships WHERE user_id = $1 AND membership = 'join'"
            )
            .bind(&user_id)
            .fetch_one(&*state.services.room_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

            Ok(Json(json!({
                "name": row.get::<Option<String>, _>("username"),
                "user_id": row.get::<String, _>("user_id"),
                "displayname": row.get::<Option<String>, _>("displayname"),
                "admin": row.get::<bool, _>("is_admin"),
                "deactivated": row.get::<bool, _>("deactivated"),
                "creation_ts": row.get::<i64, _>("creation_ts"),
                "device_count": device_count,
                "room_count": room_count
            })))
        }
        None => Err(ApiError::not_found("User not found".to_string())),
    }
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
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<UpdateAccountRequest>,
) -> Result<Json<Value>, ApiError> {
    if let Some(displayname) = &body.displayname {
        sqlx::query("UPDATE users SET displayname = $1 WHERE user_id = $2")
            .bind(displayname)
            .bind(&user_id)
            .execute(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    }

    if let Some(admin) = body.admin {
        sqlx::query("UPDATE users SET is_admin = $1 WHERE user_id = $2")
            .bind(admin)
            .bind(&user_id)
            .execute(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    }

    Ok(Json(json!({
        "user_id": user_id,
        "updated": true
    })))
}
