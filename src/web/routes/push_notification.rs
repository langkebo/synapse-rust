use crate::common::error::ApiError;
use crate::web::routes::context::AdminContext;
use crate::web::routes::{AdminUser, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use synapse_common::types::DeviceId;
use synapse_services::push_notification_service::SendNotificationRequest;
use synapse_storage::push_notification::{PushDevice, RegisterDeviceRequest};

/// The `RegisterDeviceBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterDeviceBody {
    /// The `device_id` field.
    pub device_id: String,
    /// The `push_token` field.
    pub push_token: String,
    /// The `push_type` field.
    pub push_type: String,
    /// The `app_id` field.
    pub app_id: Option<String>,
    /// The `platform` field.
    pub platform: Option<String>,
    /// The `platform_version` field.
    pub platform_version: Option<String>,
    /// The `app_version` field.
    pub app_version: Option<String>,
    /// The `locale` field.
    pub locale: Option<String>,
    /// The `timezone` field.
    pub timezone: Option<String>,
}

/// The `SendNotificationBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendNotificationBody {
    /// The `device_id` field.
    pub device_id: Option<String>,
    /// The `event_id` field.
    pub event_id: Option<String>,
    /// The `room_id` field.
    pub room_id: Option<String>,
    /// The `notification_type` field.
    pub notification_type: Option<String>,
    /// The `title` field.
    pub title: String,
    /// The `body` field.
    pub body: String,
    /// The `data` field.
    pub data: Option<serde_json::Value>,
    /// The `priority` field.
    pub priority: Option<i32>,
}

/// The `ProcessQueueQuery` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessQueueQuery {
    /// The `batch_size` field.
    pub batch_size: Option<i32>,
}

/// The `CleanupQuery` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupQuery {
    /// The `days` field.
    pub days: Option<i32>,
}

/// The `DeviceResponse` struct.
#[derive(Debug, Serialize)]
pub struct DeviceResponse {
    /// The `device_id` field.
    pub device_id: String,
    /// The `push_type` field.
    pub push_type: String,
    /// The `platform` field.
    pub platform: Option<String>,
    /// The `enabled` field.
    pub enabled: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `last_used_ts` field.
    pub last_used_ts: Option<i64>,
}

impl From<PushDevice> for DeviceResponse {
    fn from(device: PushDevice) -> Self {
        Self {
            device_id: device.device_id,
            push_type: device.push_type,
            platform: device.platform,
            enabled: device.is_enabled,
            created_ts: device.created_ts,
            last_used_ts: device.last_used_ts,
        }
    }
}

/// See [`register_device`].
pub async fn register_device(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<RegisterDeviceBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = RegisterDeviceRequest {
        user_id: auth_user.user_id.clone(),
        device_id: body.device_id,
        push_token: body.push_token,
        push_type: body.push_type,
        app_id: body.app_id,
        platform: body.platform,
        platform_version: body.platform_version,
        app_version: body.app_version,
        locale: body.locale,
        timezone: body.timezone,
        metadata: None,
    };

    let device: PushDevice = ctx.push_notification_service.register_device(request).await?;

    Ok(Json(DeviceResponse::from(device)))
}

/// See [`unregister_device`].
pub async fn unregister_device(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
    Path(device_id): Path<DeviceId>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.push_notification_service.unregister_device(&auth_user.user_id, device_id.as_str()).await?;

    Ok(Json(serde_json::json!({
        "message": "Device unregistered"
    })))
}

/// See [`get_devices`].
pub async fn get_devices(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let devices: Vec<PushDevice> = ctx.push_notification_service.get_user_devices(&auth_user.user_id).await?;

    let response: Vec<DeviceResponse> = devices.into_iter().map(DeviceResponse::from).collect();

    Ok(Json(response))
}

/// See [`send_notification`].
pub async fn send_notification(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<SendNotificationBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = SendNotificationRequest {
        user_id: auth_user.user_id.clone(),
        device_id: body.device_id,
        event_id: body.event_id,
        room_id: body.room_id,
        notification_type: body.notification_type,
        title: body.title,
        body: body.body,
        data: body.data,
        priority: body.priority,
    };

    ctx.push_notification_service.send_notification(request).await?;

    Ok(Json(serde_json::json!({
        "message": "Notification queued"
    })))
}

/// See [`process_queue`].
pub async fn process_queue(
    State(ctx): State<AdminContext>,
    _admin: AdminUser,
    Query(query): Query<ProcessQueueQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let batch_size: i32 = query.batch_size.unwrap_or(100).clamp(1, 500);

    let processed_u64: u64 = ctx.push_notification_service.process_pending_notifications(batch_size).await?;
    let processed = processed_u64 as i32;

    Ok(Json(serde_json::json!({
        "processed": processed,
        "message": format!("Processed {} notifications", processed)
    })))
}

/// See [`cleanup_logs`].
pub async fn cleanup_logs(
    State(ctx): State<AdminContext>,
    _admin: AdminUser,
    Query(query): Query<CleanupQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let days: i32 = query.days.unwrap_or(30).clamp(1, 200);

    let cleaned_u64: u64 = ctx.push_notification_service.cleanup_old_logs(days).await?;
    let cleaned = cleaned_u64 as i32;

    Ok(Json(serde_json::json!({
        "cleaned": cleaned,
        "message": format!("Cleaned up {} old logs", cleaned)
    })))
}

/// See [`create_push_notification_router`].
pub fn create_push_notification_router(state: AppState) -> axum::Router<AppState> {
    use axum::routing::*;

    let public_routes = axum::Router::new()
        .route("/_matrix/client/r0/push/devices", get(get_devices))
        .route("/_matrix/client/r0/push/devices", post(register_device))
        .route("/_matrix/client/r0/push/devices/{device_id}", delete(unregister_device))
        .route("/_matrix/client/r0/push/send", post(send_notification));

    let admin_routes =
        axum::Router::new()
            .route("/_synapse/admin/v1/push/process", post(process_queue))
            .route("/_synapse/admin/v1/push/cleanup", post(cleanup_logs))
            .route_layer(
                axum::middleware::from_fn_with_state(
                    <crate::web::routes::context::AdminContext as axum::extract::FromRef<
                        crate::web::routes::AppState,
                    >>::from_ref(&state),
                    crate::web::middleware::admin_auth_middleware,
                ),
            );

    public_routes.merge(admin_routes).with_state(state)
}

/// See [`push_notification_route_manifest`].
///
/// B-2: the 4 remaining legacy `/_matrix/client/r0/push/*` entries overlap
/// with the spec-compliant `pushers`/`pushrules` routes registered by
/// [`crate::web::routes::push`] and have **zero call sites** in the SDK
/// (`src/push` uses `/pushers` + `/pushrules`; `src/notifications` uses
/// `/notifications` only). The 3 `push/rules*` entries were removed (they
/// duplicated the spec surface and could inject rule ordering — see the audit
/// report). The 2 `/_synapse/admin/*` routes are internal management endpoints.
pub fn push_notification_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;

    // Legacy push 路由 (r0/push/*)。其 spec 替代是 /_matrix/client/v3/pushers
    // 与 /_matrix/client/v3/pushrules，但此处不再做结构化 Deprecated 标注——
    // ledger 的 status 字段全仓无消费方（SDK 不读），机制已删除
    // （B-7 连带，见 docs/audit/LEDGER_CONTRACT_ISSUES_2026-09-13.md）。
    let legacy = [
        (Method::GET, "/_matrix/client/r0/push/devices"),
        (Method::POST, "/_matrix/client/r0/push/devices"),
        (Method::DELETE, "/_matrix/client/r0/push/devices/{device_id}"),
        (Method::POST, "/_matrix/client/r0/push/send"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "push_notification"))
    .collect::<Vec<_>>();

    // Admin 路由：稳定，供内部管理使用，无 spec 替代。
    let admin = [(Method::POST, "/_synapse/admin/v1/push/process"), (Method::POST, "/_synapse/admin/v1/push/cleanup")]
        .into_iter()
        .map(|(m, p)| RouteEntry::new(m, p, "push_notification"))
        .collect::<Vec<_>>();

    [legacy, admin].into_iter().flatten().collect()
}
