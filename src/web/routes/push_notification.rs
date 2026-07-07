use crate::common::error::ApiError;
use crate::web::routes::context::AdminContext;
use crate::web::routes::{AdminUser, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use synapse_services::push_notification_service::SendNotificationRequest;
use synapse_storage::push_notification::{CreatePushRuleRequest, PushDevice, PushRule, RegisterDeviceRequest};

#[derive(Debug, Deserialize)]
pub struct RegisterDeviceBody {
    pub device_id: String,
    pub push_token: String,
    pub push_type: String,
    pub app_id: Option<String>,
    pub platform: Option<String>,
    pub platform_version: Option<String>,
    pub app_version: Option<String>,
    pub locale: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SendNotificationBody {
    pub device_id: Option<String>,
    pub event_id: Option<String>,
    pub room_id: Option<String>,
    pub notification_type: Option<String>,
    pub title: String,
    pub body: String,
    pub data: Option<serde_json::Value>,
    pub priority: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRuleBody {
    pub rule_id: String,
    pub scope: String,
    pub kind: String,
    pub priority: i32,
    pub conditions: serde_json::Value,
    pub actions: serde_json::Value,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct RulePath {
    pub scope: String,
    pub kind: String,
    pub rule_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ProcessQueueQuery {
    pub batch_size: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct CleanupQuery {
    pub days: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct DeviceResponse {
    pub device_id: String,
    pub push_type: String,
    pub platform: Option<String>,
    pub enabled: bool,
    pub created_ts: i64,
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

#[derive(Debug, Serialize)]
pub struct RuleResponse {
    pub rule_id: String,
    pub scope: String,
    pub kind: String,
    pub priority: i32,
    pub conditions: serde_json::Value,
    pub actions: serde_json::Value,
    pub enabled: bool,
}

impl From<PushRule> for RuleResponse {
    fn from(rule: PushRule) -> Self {
        Self {
            rule_id: rule.rule_id,
            scope: rule.scope,
            kind: rule.kind,
            priority: rule.priority,
            conditions: rule.conditions,
            actions: rule.actions,
            enabled: rule.is_enabled,
        }
    }
}

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

pub async fn unregister_device(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.push_notification_service.unregister_device(&auth_user.user_id, &device_id).await?;

    Ok(Json(serde_json::json!({
        "message": "Device unregistered"
    })))
}

pub async fn get_devices(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let devices: Vec<PushDevice> = ctx.push_notification_service.get_user_devices(&auth_user.user_id).await?;

    let response: Vec<DeviceResponse> = devices.into_iter().map(DeviceResponse::from).collect();

    Ok(Json(response))
}

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

pub async fn create_rule(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateRuleBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreatePushRuleRequest {
        user_id: auth_user.user_id.clone(),
        rule_id: body.rule_id,
        scope: body.scope,
        kind: body.kind,
        priority: body.priority,
        conditions: body.conditions,
        actions: body.actions,
        enabled: body.enabled,
    };

    let rule: PushRule = ctx.push_notification_service.create_push_rule(request).await?;

    Ok(Json(RuleResponse::from(rule)))
}

pub async fn get_rules(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let rules: Vec<PushRule> = ctx.push_notification_service.get_push_rules(&auth_user.user_id).await?;

    let response: Vec<RuleResponse> = rules.into_iter().map(RuleResponse::from).collect();

    Ok(Json(response))
}

pub async fn delete_rule(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
    Path(path): Path<RulePath>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.push_notification_service.delete_push_rule(&auth_user.user_id, &path.scope, &path.kind, &path.rule_id).await?;

    Ok(Json(serde_json::json!({
        "message": "Rule deleted"
    })))
}

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

pub fn create_push_notification_router(state: AppState) -> axum::Router<AppState> {
    use axum::routing::*;

    let public_routes = axum::Router::new()
        .route("/_matrix/client/r0/push/devices", get(get_devices))
        .route("/_matrix/client/r0/push/devices", post(register_device))
        .route("/_matrix/client/r0/push/devices/{device_id}", delete(unregister_device))
        .route("/_matrix/client/r0/push/send", post(send_notification))
        .route("/_matrix/client/r0/push/rules", get(get_rules))
        .route("/_matrix/client/r0/push/rules", post(create_rule))
        .route("/_matrix/client/r0/push/rules/{scope}/{kind}/{rule_id}", delete(delete_rule));

    let admin_routes = axum::Router::new()
        .route("/_synapse/admin/v1/push/process", post(process_queue))
        .route("/_synapse/admin/v1/push/cleanup", post(cleanup_logs))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::web::middleware::admin_auth_middleware,
        ));

    public_routes.merge(admin_routes).with_state(state)
}

pub fn push_notification_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::GET, "/_matrix/client/r0/push/devices"),
        (Method::POST, "/_matrix/client/r0/push/devices"),
        (Method::DELETE, "/_matrix/client/r0/push/devices/{device_id}"),
        (Method::POST, "/_matrix/client/r0/push/send"),
        (Method::GET, "/_matrix/client/r0/push/rules"),
        (Method::POST, "/_matrix/client/r0/push/rules"),
        (Method::DELETE, "/_matrix/client/r0/push/rules/{scope}/{kind}/{rule_id}"),
        (Method::POST, "/_synapse/admin/v1/push/process"),
        (Method::POST, "/_synapse/admin/v1/push/cleanup"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "push_notification"))
    .collect()
}
