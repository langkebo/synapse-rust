#[cfg(feature = "server-notifications")]
use crate::common::ApiError;
#[cfg(feature = "server-notifications")]
use crate::storage::{decode_server_notification_cursor, CreateNotificationRequest};
#[cfg(feature = "server-notifications")]
use crate::web::routes::AdminUser;
use crate::web::routes::AppState;
#[cfg(feature = "server-notifications")]
use axum::extract::Query;
#[cfg(feature = "server-notifications")]
use axum::routing::post;
use axum::Router;
#[cfg(feature = "server-notifications")]
use axum::{
    extract::{Path, State},
    routing::{delete, get, put},
    Json,
};
use serde::Deserialize;
#[cfg(feature = "server-notifications")]
use serde_json::{json, Value};

#[cfg(feature = "server-notifications")]
fn decode_notice_cursor(cursor: Option<&str>) -> Option<(i64, i64)> {
    let cursor = cursor?;
    let (sent_ts, id) = cursor.split_once('|')?;
    let sent_ts = sent_ts.parse::<i64>().ok()?;
    let id = id.parse::<i64>().ok()?;
    Some((sent_ts, id))
}

#[cfg(all(test, feature = "server-notifications"))]
fn encode_notice_cursor(sent_ts: i64, id: i64) -> String {
    format!("{sent_ts}|{id}")
}

#[cfg(all(test, feature = "server-notifications"))]
mod cursor_tests {
    use super::{decode_notice_cursor, encode_notice_cursor};
    use crate::storage::{
        decode_server_notification_cursor, encode_server_notification_cursor, ServerNotificationCursor,
    };

    #[test]
    fn test_notice_cursor_round_trip() {
        let cursor = encode_notice_cursor(1_700_000_000_000, 42);
        assert_eq!(decode_notice_cursor(Some(&cursor)), Some((1_700_000_000_000, 42)));
    }

    #[test]
    fn test_notice_cursor_rejects_invalid_value() {
        assert_eq!(decode_notice_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_notice_cursor(Some("123|")), None);
    }

    #[test]
    fn test_notification_cursor_round_trip() {
        let cursor =
            encode_server_notification_cursor(&ServerNotificationCursor { created_ts: 1_700_000_000_000, id: 7 });
        assert_eq!(
            decode_server_notification_cursor(Some(&cursor)),
            Some(ServerNotificationCursor { created_ts: 1_700_000_000_000, id: 7 })
        );
    }

    #[test]
    fn test_notification_cursor_rejects_invalid_value() {
        assert_eq!(decode_server_notification_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_server_notification_cursor(Some("123|")), None);
    }
}

pub fn create_notification_router(_state: AppState) -> Router<AppState> {
    #[allow(unused_mut)]
    let mut router = Router::new();

    #[cfg(feature = "server-notifications")]
    {
        router = router
            .route("/_synapse/admin/v1/users/{user_id}/notification", get(get_user_notification))
            .route("/_synapse/admin/v1/users/{user_id}/notification", put(update_user_notification))
            .route("/_synapse/admin/v1/users/{user_id}/pushers", get(get_user_pushers))
            .route("/_synapse/admin/v1/users/{user_id}/pushers/{pushkey}", delete(delete_user_pusher));
    }

    #[cfg(feature = "server-notifications")]
    {
        router = router
            .route("/_synapse/admin/v1/notifications", post(create_notification))
            .route("/_synapse/admin/v1/notifications", get(list_notifications))
            .route("/_synapse/admin/v1/notifications/{notification_id}", get(get_notification))
            .route("/_synapse/admin/v1/notifications/{notification_id}", put(update_notification))
            .route("/_synapse/admin/v1/notifications/{notification_id}", delete(delete_notification))
            .route("/_synapse/admin/v1/notifications/{notification_id}/deactivate", put(deactivate_notification))
            .route("/_synapse/admin/v1/notifications/active", get(list_active_notifications))
            .route("/_synapse/admin/v1/send_server_notice", post(send_server_notice))
            .route("/_synapse/admin/v1/server_notices", get(get_server_notices))
            .route("/_synapse/admin/v1/server_notices/{notice_id}", get(get_server_notice))
            .route("/_synapse/admin/v1/server_notices/{notice_id}", delete(delete_server_notice));
    }

    router
}

pub fn admin_notification_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    #[allow(unused_mut)]
    let mut entries = vec![
        (Method::GET, "/_synapse/admin/v1/users/{user_id}/notification"),
        (Method::PUT, "/_synapse/admin/v1/users/{user_id}/notification"),
        (Method::GET, "/_synapse/admin/v1/users/{user_id}/pushers"),
        (Method::DELETE, "/_synapse/admin/v1/users/{user_id}/pushers/{pushkey}"),
    ];

    #[cfg(feature = "server-notifications")]
    {
        entries.extend_from_slice(&[
            (Method::POST, "/_synapse/admin/v1/notifications"),
            (Method::GET, "/_synapse/admin/v1/notifications"),
            (Method::GET, "/_synapse/admin/v1/notifications/{notification_id}"),
            (Method::PUT, "/_synapse/admin/v1/notifications/{notification_id}"),
            (Method::DELETE, "/_synapse/admin/v1/notifications/{notification_id}"),
            (Method::PUT, "/_synapse/admin/v1/notifications/{notification_id}/deactivate"),
            (Method::GET, "/_synapse/admin/v1/notifications/active"),
            (Method::POST, "/_synapse/admin/v1/send_server_notice"),
            (Method::GET, "/_synapse/admin/v1/server_notices"),
            (Method::GET, "/_synapse/admin/v1/server_notices/{notice_id}"),
            (Method::DELETE, "/_synapse/admin/v1/server_notices/{notice_id}"),
        ]);
    }

    entries.into_iter().map(|(m, p)| RouteEntry::new(m, p, "admin::notification")).collect()
}

#[cfg(feature = "server-notifications")]
async fn ensure_user_exists(state: &AppState, user_id: &str) -> Result<(), ApiError> {
    let user = state.services.account.user_storage.get_user_by_identifier(user_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

    if user.is_none() {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    Ok(())
}

#[cfg(feature = "server-notifications")]
async fn ensure_target_users_exist(state: &AppState, user_ids: &[String]) -> Result<(), ApiError> {
    for user_id in user_ids {
        ensure_user_exists(state, user_id).await?;
    }

    Ok(())
}

#[cfg(feature = "server-notifications")]
#[derive(Debug, Deserialize)]
pub struct ServerNoticeRequest {
    pub user_id: String,
    pub content: NoticeContent,
}

#[cfg(feature = "server-notifications")]
#[derive(Debug, Deserialize)]
pub struct NoticeContent {
    pub msgtype: String,
    pub body: String,
}

#[derive(Debug, Deserialize)]
pub struct UserNotificationRequest {
    pub is_enabled: bool,
}

#[cfg(feature = "server-notifications")]
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

#[cfg(feature = "server-notifications")]
#[derive(Debug, Deserialize)]
pub struct NotificationQuery {
    pub audience: Option<String>,
    pub limit: Option<usize>,
    pub from: Option<String>,
}

#[cfg(feature = "server-notifications")]
#[axum::debug_handler]
pub async fn create_notification(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<CreateNotificationRequest>,
) -> Result<Json<Value>, ApiError> {
    let requested_target_user_ids = body.target_user_ids.clone().unwrap_or_default();
    ensure_target_users_exist(&state, &requested_target_user_ids).await?;

    let notification = state.services.extensions.server_notification_storage.create_notification(body).await?;

    Ok(Json(json!(notification)))
}

#[cfg(feature = "server-notifications")]
#[axum::debug_handler]
pub async fn list_notifications(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<NotificationQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = (query.limit.unwrap_or(50).min(100)) as i64;
    let cursor = decode_server_notification_cursor(query.from.as_deref());

    if query.from.is_some() && cursor.is_none() {
        return Err(ApiError::bad_request("Invalid from cursor".to_string()));
    }

    let (notifications, next_batch) = state
        .services
        .extensions
        .server_notification_service
        .list_all_notifications(query.audience.as_deref(), limit, cursor)
        .await?;

    Ok(Json(json!({
        "notifications": notifications,
        "next_batch": next_batch
    })))
}

#[cfg(feature = "server-notifications")]
#[axum::debug_handler]
pub async fn get_notification(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(notification_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let notification = state.services.extensions.server_notification_storage.get_notification(notification_id).await?;

    match notification {
        Some(n) => Ok(Json(json!(n))),
        None => Err(ApiError::not_found("Notification not found".to_string())),
    }
}

#[cfg(feature = "server-notifications")]
#[axum::debug_handler]
pub async fn update_notification(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(notification_id): Path<i64>,
    Json(body): Json<UpdateNotificationRequest>,
) -> Result<Json<Value>, ApiError> {
    let existing = state.services.extensions.server_notification_storage.get_notification(notification_id).await?;

    let existing = match existing {
        Some(n) => n,
        None => return Err(ApiError::not_found("Notification not found".to_string())),
    };

    let requested_target_user_ids = body.target_user_ids.clone().unwrap_or_default();
    ensure_target_users_exist(&state, &requested_target_user_ids).await?;

    let update_request = CreateNotificationRequest {
        title: body.title.unwrap_or(existing.title),
        content: body.content.unwrap_or(existing.content),
        notification_type: body.notification_type.or(Some(existing.notification_type)),
        priority: body.priority.or(Some(existing.priority)),
        target_audience: body.target_audience.or(Some(existing.target_audience)),
        target_user_ids: body.target_user_ids,
        starts_at: body.starts_at.or(existing.starts_at),
        expires_at: body.expires_at.or(existing.expires_at),
        is_dismissable: body.is_dismissable.or(Some(existing.is_dismissable)),
        action_url: body.action_url.or(existing.action_url),
        action_text: body.action_text.or(existing.action_text),
        created_by: existing.created_by,
    };

    let notification = state
        .services
        .extensions
        .server_notification_storage
        .update_notification(notification_id, update_request)
        .await?;

    Ok(Json(json!(notification)))
}

#[cfg(feature = "server-notifications")]
#[axum::debug_handler]
pub async fn delete_notification(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(notification_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let deleted = state.services.extensions.server_notification_storage.delete_notification(notification_id).await?;

    if !deleted {
        return Err(ApiError::not_found("Notification not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[cfg(feature = "server-notifications")]
#[axum::debug_handler]
pub async fn deactivate_notification(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(notification_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let deactivated =
        state.services.extensions.server_notification_storage.deactivate_notification(notification_id).await?;

    if !deactivated {
        return Err(ApiError::not_found("Notification not found".to_string()));
    }

    Ok(Json(json!({ "is_enabled": false })))
}

#[cfg(feature = "server-notifications")]
#[axum::debug_handler]
pub async fn list_active_notifications(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let notifications = state.services.extensions.server_notification_storage.list_active_notifications().await?;

    Ok(Json(json!(notifications)))
}

#[cfg(feature = "server-notifications")]
#[axum::debug_handler]
pub async fn send_server_notice(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<ServerNoticeRequest>,
) -> Result<Json<Value>, ApiError> {
    let target_user = state.services.account.user_storage.get_user_by_identifier(&body.user_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;
    let Some(target_user) = target_user else {
        return Err(ApiError::not_found("User not found".to_string()));
    };

    let room_id = format!("!server_notice_{}:{}", uuid::Uuid::new_v4(), state.services.core.server_name);
    let now = chrono::Utc::now().timestamp_millis();
    let server_user = format!("@server:{}", state.services.core.server_name);
    let message_event_id = format!("${}:{}", uuid::Uuid::new_v4(), state.services.core.server_name);
    let create_event_id = format!("${}:{}", uuid::Uuid::new_v4(), state.services.core.server_name);
    let membership_event_id = format!("${}:{}", uuid::Uuid::new_v4(), state.services.core.server_name);

    let notice_id = state
        .services
        .extensions
        .server_notification_storage
        .send_server_notice(
            &room_id,
            &server_user,
            &target_user.user_id,
            &target_user.displayname,
            &target_user.avatar_url,
            &message_event_id,
            &create_event_id,
            &membership_event_id,
            &body.content.msgtype,
            &body.content.body,
            now,
        )
        .await?;

    Ok(Json(json!({ "event_id": message_event_id, "room_id": room_id, "notice_id": notice_id })))
}

#[cfg(feature = "server-notifications")]
#[axum::debug_handler]
pub async fn get_server_notices(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<ServerNoticesQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(10).min(50) as i64;
    let cursor = decode_notice_cursor(query.from.as_deref());

    let (notice_list, total, next_batch) =
        state.services.extensions.server_notification_storage.get_server_notices_paginated(cursor, limit).await?;

    Ok(Json(json!({
        "notices": notice_list,
        "total": total,
        "next_batch": next_batch
    })))
}

#[derive(Debug, Deserialize, Default)]
pub struct ServerNoticesQuery {
    pub limit: Option<u32>,
    pub from: Option<String>,
}

#[cfg(feature = "server-notifications")]
#[axum::debug_handler]
pub async fn get_server_notice(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(notice_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let notice = state.services.extensions.server_notification_storage.get_server_notice_by_id(notice_id).await?;

    match notice {
        Some(notice) => Ok(Json(notice)),
        None => Err(ApiError::not_found("Server notice not found".to_string())),
    }
}

#[cfg(feature = "server-notifications")]
#[axum::debug_handler]
pub async fn delete_server_notice(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(notice_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let notice_info =
        state.services.extensions.server_notification_storage.get_server_notice_with_room(notice_id).await?;

    let Some((event_id, room_id)) = notice_info else {
        return Err(ApiError::not_found("Server notice not found".to_string()));
    };

    state.services.extensions.server_notification_storage.delete_server_notice_by_id(notice_id).await?;

    if let Some(rid) = room_id {
        state.services.extensions.server_notification_storage.delete_room_cascade(&rid).await?;
    } else if let Some(eid) = event_id {
        state.services.extensions.server_notification_storage.delete_event_by_id(&eid).await?;
    }

    Ok(Json(json!({})))
}

#[cfg(feature = "server-notifications")]
#[axum::debug_handler]
pub async fn get_user_notification(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_user_exists(&state, &user_id).await?;

    let setting = state.services.extensions.server_notification_storage.get_user_notification_setting(&user_id).await?;

    match setting {
        Some(enabled) => Ok(Json(json!({ "enabled": enabled }))),
        None => Ok(Json(json!({ "enabled": true }))),
    }
}

#[cfg(feature = "server-notifications")]
#[axum::debug_handler]
pub async fn update_user_notification(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<UserNotificationRequest>,
) -> Result<Json<Value>, ApiError> {
    ensure_user_exists(&state, &user_id).await?;

    state
        .services
        .extensions
        .server_notification_storage
        .upsert_user_notification_setting(&user_id, body.is_enabled)
        .await?;

    Ok(Json(json!({ "is_enabled": body.is_enabled })))
}

#[cfg(feature = "server-notifications")]
#[axum::debug_handler]
pub async fn get_user_pushers(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_user_exists(&state, &user_id).await?;

    let pusher_list = state.services.extensions.server_notification_storage.get_user_pushers(&user_id).await?;

    Ok(Json(json!({ "pushers": pusher_list, "total": pusher_list.len() })))
}

#[cfg(feature = "server-notifications")]
#[axum::debug_handler]
pub async fn delete_user_pusher(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((user_id, pushkey)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    ensure_user_exists(&state, &user_id).await?;

    let deleted = state.services.extensions.server_notification_storage.delete_user_pusher(&user_id, &pushkey).await?;

    if !deleted {
        return Err(ApiError::not_found("Pusher not found".to_string()));
    }

    Ok(Json(json!({})))
}
