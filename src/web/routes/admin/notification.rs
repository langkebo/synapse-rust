use crate::common::ApiError;
use crate::storage::server_notification::{CreateNotificationRequest, ServerNotification};
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

pub fn create_notification_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_synapse/admin/v1/notifications",
            post(create_notification),
        )
        .route("/_synapse/admin/v1/notifications", get(list_notifications))
        .route(
            "/_synapse/admin/v1/notifications/{notification_id}",
            get(get_notification),
        )
        .route(
            "/_synapse/admin/v1/notifications/{notification_id}",
            put(update_notification),
        )
        .route(
            "/_synapse/admin/v1/notifications/{notification_id}",
            delete(delete_notification),
        )
        .route(
            "/_synapse/admin/v1/notifications/{notification_id}/deactivate",
            put(deactivate_notification),
        )
        .route(
            "/_synapse/admin/v1/notifications/active",
            get(list_active_notifications),
        )
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

#[derive(Debug, Deserialize)]
pub struct UpdateNotificationRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub notification_type: Option<String>,
    pub priority: Option<i32>,
    pub target_audience: Option<String>,
    pub target_user_ids: Option<Vec<String>>,
    pub starts_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub is_dismissable: Option<bool>,
    pub action_url: Option<String>,
    pub action_text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NotificationQuery {
    pub audience: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[axum::debug_handler]
pub async fn create_notification(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<CreateNotificationRequest>,
) -> Result<Json<Value>, ApiError> {
    let target_user_ids = serde_json::to_value(body.target_user_ids.unwrap_or_default())
        .unwrap_or(serde_json::json!([]));

    let notification = sqlx::query_as::<_, ServerNotification>(
        r#"
        INSERT INTO server_notifications (
            title, content, notification_type, priority, target_audience,
            target_user_ids, starts_at, expires_at, is_dismissable,
            action_url, action_text, created_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        RETURNING id, title, content, notification_type, priority, target_audience, target_user_ids, starts_at, expires_at, is_enabled, is_dismissable, action_url, action_text, created_by, created_ts, updated_ts
        "#,
    )
    .bind(&body.title)
    .bind(&body.content)
    .bind(body.notification_type.unwrap_or_else(|| "info".to_string()))
    .bind(body.priority.unwrap_or(0))
    .bind(body.target_audience.unwrap_or_else(|| "all".to_string()))
    .bind(&target_user_ids)
    .bind(body.starts_at)
    .bind(body.expires_at)
    .bind(body.is_dismissable.unwrap_or(true))
    .bind(&body.action_url)
    .bind(&body.action_text)
    .bind(&body.created_by)
    .fetch_one(&state.services.server_notification_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to create notification: {}", e)))?;

    Ok(Json(json!(notification)))
}

#[axum::debug_handler]
pub async fn list_notifications(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<NotificationQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = (query.limit.unwrap_or(50).min(100)) as i64;
    let offset = (query.offset.unwrap_or(0)) as i64;

    let notifications = sqlx::query_as::<_, ServerNotification>(
        r#"
        SELECT id, title, content, notification_type, priority, target_audience, target_user_ids, starts_at, expires_at, is_enabled, is_dismissable, action_url, action_text, created_by, created_ts, updated_ts
        FROM server_notifications
        WHERE ($1::text IS NULL OR target_audience = $1)
        ORDER BY created_ts DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(&query.audience)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.services.server_notification_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to list notifications: {}", e)))?;

    Ok(Json(json!(notifications)))
}

#[axum::debug_handler]
pub async fn get_notification(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(notification_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let notification = sqlx::query_as::<_, ServerNotification>(
        r#"
        SELECT id, title, content, notification_type, priority, target_audience, target_user_ids, starts_at, expires_at, is_enabled, is_dismissable, action_url, action_text, created_by, created_ts, updated_ts
        FROM server_notifications
        WHERE id = $1
        "#,
    )
    .bind(notification_id)
    .fetch_optional(&state.services.server_notification_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get notification: {}", e)))?;

    match notification {
        Some(n) => Ok(Json(json!(n))),
        None => Err(ApiError::not_found("Notification not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn update_notification(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(notification_id): Path<i64>,
    Json(body): Json<UpdateNotificationRequest>,
) -> Result<Json<Value>, ApiError> {
    let existing = sqlx::query_as::<_, (String, String)>(
        "SELECT title, content FROM server_notifications WHERE id = $1",
    )
    .bind(notification_id)
    .fetch_optional(&state.services.server_notification_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let (current_title, current_content) = match existing {
        Some(row) => (row.0, row.1),
        None => return Err(ApiError::not_found("Notification not found".to_string())),
    };

    let title = body.title.unwrap_or(current_title);
    let content = body.content.unwrap_or(current_content);
    let notification_type = body.notification_type.unwrap_or_else(|| "info".to_string());
    let priority = body.priority.unwrap_or(0);
    let target_audience = body.target_audience.unwrap_or_else(|| "all".to_string());
    let target_user_ids = serde_json::to_value(body.target_user_ids.unwrap_or_default())
        .unwrap_or(serde_json::json!([]));
    let now = chrono::Utc::now().timestamp_millis();

    let notification = sqlx::query_as::<_, ServerNotification>(
        r#"
        UPDATE server_notifications
        SET title = $2, content = $3, notification_type = $4, priority = $5,
            target_audience = $6, target_user_ids = $7, starts_at = $8, expires_at = $9,
            is_dismissable = $10, action_url = $11, action_text = $12, updated_ts = $13
        WHERE id = $1
        RETURNING id, title, content, notification_type, priority, target_audience, target_user_ids, starts_at, expires_at, is_enabled, is_dismissable, action_url, action_text, created_by, created_ts, updated_ts
        "#,
    )
    .bind(notification_id)
    .bind(&title)
    .bind(&content)
    .bind(&notification_type)
    .bind(priority)
    .bind(&target_audience)
    .bind(&target_user_ids)
    .bind(body.starts_at)
    .bind(body.expires_at)
    .bind(body.is_dismissable.unwrap_or(true))
    .bind(&body.action_url)
    .bind(&body.action_text)
    .bind(now)
    .fetch_one(&state.services.server_notification_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to update notification: {}", e)))?;

    Ok(Json(json!(notification)))
}

#[axum::debug_handler]
pub async fn delete_notification(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(notification_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM server_notifications WHERE id = $1")
        .bind(notification_id)
        .execute(&state.services.server_notification_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Notification not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn deactivate_notification(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(notification_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();

    let result = sqlx::query(
        "UPDATE server_notifications SET is_enabled = FALSE, updated_ts = $2 WHERE id = $1",
    )
    .bind(notification_id)
    .bind(now)
    .execute(&state.services.server_notification_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to disable notification: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Notification not found".to_string()));
    }

    Ok(Json(json!({ "is_enabled": false })))
}

#[axum::debug_handler]
pub async fn list_active_notifications(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();

    let notifications = sqlx::query_as::<_, ServerNotification>(
        r#"
        SELECT id, title, content, notification_type, priority, target_audience, target_user_ids, starts_at, expires_at, is_enabled, is_dismissable, action_url, action_text, created_by, created_ts, updated_ts
        FROM server_notifications
        WHERE is_enabled = TRUE
        AND (starts_at IS NULL OR starts_at <= $1)
        AND (expires_at IS NULL OR expires_at >= $1)
        ORDER BY priority DESC, created_ts DESC
        "#,
    )
    .bind(now)
    .fetch_all(&state.services.server_notification_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to list active notifications: {}", e)))?;

    Ok(Json(json!(notifications)))
}

#[axum::debug_handler]
pub async fn send_server_notice(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<ServerNoticeRequest>,
) -> Result<Json<Value>, ApiError> {
    let target_user = state
        .services
        .user_storage
        .get_user_by_identifier(&body.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    if target_user.is_none() {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let room_id = format!(
        "!server_notice_{}:{}",
        uuid::Uuid::new_v4(),
        state.services.config.server.name
    );
    let now = chrono::Utc::now().timestamp_millis();
    let server_user = format!("@server:{}", state.services.config.server.name);

    let room_result = sqlx::query(
        r#"
        INSERT INTO rooms (room_id, name, topic, creator, is_public, join_rules, created_ts)
        VALUES ($1, $2, $3, $4, false, 'private', $5)
        ON CONFLICT (room_id) DO NOTHING
        "#,
    )
    .bind(&room_id)
    .bind("Server Notice")
    .bind("System notifications")
    .bind(&server_user)
    .bind(now)
    .execute(&*state.services.event_storage.pool)
    .await;

    if let Err(e) = room_result {
        ::tracing::warn!("Failed to create server notice room: {}", e);
    }

    let message_event_id = format!(
        "${}:{}",
        uuid::Uuid::new_v4(),
        state.services.config.server.name
    );

    let create_event_id = format!(
        "${}:{}",
        uuid::Uuid::new_v4(),
        state.services.config.server.name
    );
    let create_result = sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, user_id, event_type, content, origin_server_ts, sender, state_key)
        VALUES ($1, $2, $3, 'm.room.create', $4, $5, $6, '')
        ON CONFLICT (event_id) DO NOTHING
        "#
    )
    .bind(&create_event_id)
    .bind(&room_id)
    .bind(&server_user)
    .bind(json!({"creator": server_user}))
    .bind(now)
    .bind(&server_user)
    .execute(&*state.services.event_storage.pool)
    .await;

    if let Err(e) = create_result {
        ::tracing::warn!("Failed to create room event: {}", e);
    }

    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, user_id, event_type, content, origin_server_ts, sender)
        VALUES ($1, $2, $3, 'm.room.message', $4, $5, $6)
        "#
    )
    .bind(&message_event_id)
    .bind(&room_id)
    .bind(&body.user_id)
    .bind(json!({
        "msgtype": body.content.msgtype,
        "body": body.content.body
    }))
    .bind(now)
    .bind(&server_user)
    .execute(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let notice_content = json!({
        "msgtype": body.content.msgtype,
        "body": body.content.body
    });
    let notice_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO server_notices (user_id, event_id, content, sent_ts)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(&body.user_id)
    .bind(&message_event_id)
    .bind(notice_content.to_string())
    .bind(now)
    .fetch_one(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(
        json!({ "event_id": message_event_id, "room_id": room_id, "notice_id": notice_id }),
    ))
}

#[axum::debug_handler]
pub async fn get_server_notices(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<ServerNoticesQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(10).min(50) as i64;
    let offset = query.from.unwrap_or(0) as i64;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM server_notices")
        .fetch_one(&*state.services.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let notices = sqlx::query(
        "SELECT id, user_id, event_id, content, sent_ts FROM server_notices ORDER BY sent_ts DESC LIMIT $1 OFFSET $2"
    )
    .bind(limit)
    .bind(offset)
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

    Ok(Json(json!({ "notices": notice_list, "total": total })))
}

#[derive(Debug, Deserialize, Default)]
pub struct ServerNoticesQuery {
    pub limit: Option<u32>,
    pub from: Option<u32>,
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
    let user = state
        .services
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if user.is_none() {
        return Err(ApiError::not_found("User not found".to_string()));
    }

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
                "data": row.get::<Option<Value>, _>("data").unwrap_or(json!({}))
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
