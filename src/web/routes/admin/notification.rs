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

pub fn create_notification_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_synapse/admin/v1/send_server_notice",
            post(send_server_notice),
        )
        .route("/_synapse/admin/v1/server_notices", get(get_server_notices))
        .route(
            "/_synapse/admin/v1/server_notices/{notice_id}",
            get(get_server_notice),
        )
        .route(
            "/_synapse/admin/v1/server_notices/{notice_id}",
            delete(delete_server_notice),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/notification",
            get(get_user_notification),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/notification",
            put(update_user_notification),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/pushers",
            get(get_user_pushers),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/pushers/{pushkey}",
            delete(delete_user_pusher),
        )
}

#[derive(Debug, Deserialize)]
pub struct ServerNoticeRequest {
    pub user_id: String,
    pub content: NoticeContent,
}

#[derive(Debug, Deserialize)]
pub struct NoticeContent {
    pub msgtype: String,
    pub body: String,
}

#[derive(Debug, Deserialize)]
pub struct UserNotificationRequest {
    pub enabled: bool,
}

#[axum::debug_handler]
pub async fn send_server_notice(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<ServerNoticeRequest>,
) -> Result<Json<Value>, ApiError> {
    let event_id = crate::common::crypto::generate_event_id(&state.services.config.server.name);
    let room_id = format!(
        "!server_notice_{}:{}",
        uuid::Uuid::new_v4(),
        state.services.config.server.name
    );
    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, user_id, event_type, content, origin_server_ts, sender)
        VALUES ($1, $2, $3, 'm.room.message', $4, $5, $6)
        "#
    )
    .bind(&event_id)
    .bind(&room_id)
    .bind(&body.user_id)
    .bind(json!({
        "msgtype": body.content.msgtype,
        "body": body.content.body
    }))
    .bind(now)
    .bind(format!("@server:{}", state.services.config.server.name))
    .execute(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({ "event_id": event_id })))
}

#[axum::debug_handler]
pub async fn get_server_notices(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let notices = sqlx::query(
        "SELECT id, user_id, event_id, content, sent_ts FROM server_notices ORDER BY sent_ts DESC LIMIT 100"
    )
    .fetch_all(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let notice_list: Vec<Value> = notices
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<Option<i64>, _>("id"),
                "user_id": row.get::<Option<String>, _>("user_id"),
                "event_id": row.get::<Option<String>, _>("event_id"),
                "content": row.get::<Option<String>, _>("content"),
                "sent_ts": row.get::<Option<i64>, _>("sent_ts")
            })
        })
        .collect();

    Ok(Json(
        json!({ "notices": notice_list, "total": notice_list.len() }),
    ))
}

#[axum::debug_handler]
pub async fn get_server_notice(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(notice_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let notice = sqlx::query(
        "SELECT id, user_id, event_id, content, sent_ts FROM server_notices WHERE id = $1",
    )
    .bind(notice_id)
    .fetch_optional(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match notice {
        Some(row) => Ok(Json(json!({
            "id": row.get::<Option<i64>, _>("id"),
            "user_id": row.get::<Option<String>, _>("user_id"),
            "event_id": row.get::<Option<String>, _>("event_id"),
            "content": row.get::<Option<String>, _>("content"),
            "sent_ts": row.get::<Option<i64>, _>("sent_ts")
        }))),
        None => Err(ApiError::not_found("Server notice not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn delete_server_notice(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(notice_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM server_notices WHERE id = $1")
        .bind(notice_id)
        .execute(&*state.services.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Server notice not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_user_notification(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let setting = sqlx::query("SELECT enabled FROM user_notification_settings WHERE user_id = $1")
        .bind(&user_id)
        .fetch_optional(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match setting {
        Some(row) => Ok(Json(json!({
            "enabled": row.get::<Option<bool>, _>("enabled").unwrap_or(true)
        }))),
        None => Ok(Json(json!({ "enabled": true }))),
    }
}

#[axum::debug_handler]
pub async fn update_user_notification(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<UserNotificationRequest>,
) -> Result<Json<Value>, ApiError> {
    sqlx::query(
        "INSERT INTO user_notification_settings (user_id, enabled) VALUES ($1, $2) ON CONFLICT (user_id) DO UPDATE SET enabled = $2"
    )
    .bind(&user_id)
    .bind(body.enabled)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({ "enabled": body.enabled })))
}

#[axum::debug_handler]
pub async fn get_user_pushers(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let pushers = sqlx::query(
        "SELECT pushkey, kind, app_id, app_display_name, device_display_name, profile_tag, lang, data FROM pushers WHERE user_id = $1"
    )
    .bind(&user_id)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let pusher_list: Vec<Value> = pushers
        .iter()
        .map(|row| {
            json!({
                "pushkey": row.get::<Option<String>, _>("pushkey"),
                "kind": row.get::<Option<String>, _>("kind"),
                "app_id": row.get::<Option<String>, _>("app_id"),
                "app_display_name": row.get::<Option<String>, _>("app_display_name"),
                "device_display_name": row.get::<Option<String>, _>("device_display_name"),
                "profile_tag": row.get::<Option<String>, _>("profile_tag"),
                "lang": row.get::<Option<String>, _>("lang"),
                "data": row.get::<Option<String>, _>("data")
            })
        })
        .collect();

    Ok(Json(
        json!({ "pushers": pusher_list, "total": pusher_list.len() }),
    ))
}

#[axum::debug_handler]
pub async fn delete_user_pusher(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((user_id, pushkey)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM pushers WHERE user_id = $1 AND pushkey = $2")
        .bind(&user_id)
        .bind(&pushkey)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Pusher not found".to_string()));
    }

    Ok(Json(json!({})))
}
