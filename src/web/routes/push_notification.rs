use crate::common::error::ApiError;
use crate::services::push_notification_service::SendNotificationRequest;
use crate::storage::push_notification::{
    CreatePushRuleRequest, PushDevice, PushRule, RegisterDeviceRequest,
};
use crate::web::routes::AppState;
use crate::web::routes::AuthenticatedUser;
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

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
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<PushDevice> for DeviceResponse {
    fn from(device: PushDevice) -> Self {
        Self {
            device_id: device.device_id,
            push_type: device.push_type,
            platform: device.platform,
            enabled: device.enabled,
            created_at: device.created_at,
            last_used_at: device.last_used_at,
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
            enabled: rule.enabled,
        }
    }
}

pub async fn register_device(
    State(state): State<AppState>,
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

    let device = state.services.push_notification_service.register_device(request).await?;

    Ok(Json(DeviceResponse::from(device)))
}

pub async fn unregister_device(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    state.services.push_notification_service
        .unregister_device(&auth_user.user_id, &device_id)
        .await?;

    Ok(Json(serde_json::json!({
        "message": "Device unregistered"
    })))
}

pub async fn get_devices(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let devices = state.services.push_notification_service
        .get_user_devices(&auth_user.user_id)
        .await?;

    let response: Vec<DeviceResponse> = devices.into_iter().map(DeviceResponse::from).collect();

    Ok(Json(response))
}

pub async fn send_notification(
    State(state): State<AppState>,
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

    state.services.push_notification_service.send_notification(request).await?;

    Ok(Json(serde_json::json!({
        "message": "Notification queued"
    })))
}

pub async fn create_rule(
    State(state): State<AppState>,
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

    let rule = state.services.push_notification_service.create_push_rule(request).await?;

    Ok(Json(RuleResponse::from(rule)))
}

pub async fn get_rules(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let rules = state.services.push_notification_service
        .get_push_rules(&auth_user.user_id)
        .await?;

    let response: Vec<RuleResponse> = rules.into_iter().map(RuleResponse::from).collect();

    Ok(Json(response))
}

pub async fn delete_rule(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(path): Path<RulePath>,
) -> Result<impl IntoResponse, ApiError> {
    state.services.push_notification_service
        .delete_push_rule(&auth_user.user_id, &path.scope, &path.kind, &path.rule_id)
        .await?;

    Ok(Json(serde_json::json!({
        "message": "Rule deleted"
    })))
}

pub async fn process_queue(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(query): Query<ProcessQueueQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let batch_size = query.batch_size.unwrap_or(100);

    let processed = state.services.push_notification_service
        .process_pending_notifications(batch_size)
        .await?;

    Ok(Json(serde_json::json!({
        "processed": processed,
        "message": format!("Processed {} notifications", processed)
    })))
}

pub async fn cleanup_logs(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(query): Query<CleanupQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let days = query.days.unwrap_or(30);

    let cleaned = state.services.push_notification_service.cleanup_old_logs(days).await?;

    Ok(Json(serde_json::json!({
        "cleaned": cleaned,
        "message": format!("Cleaned up {} old logs", cleaned)
    })))
}

pub fn create_push_notification_router() -> axum::Router<AppState> {
    use axum::routing::*;

    axum::Router::new()
        .route("/_matrix/client/r0/push/devices", get(get_devices))
        .route("/_matrix/client/r0/push/devices", post(register_device))
        .route("/_matrix/client/r0/push/devices/{device_id}", delete(unregister_device))
        .route("/_matrix/client/r0/push/send", post(send_notification))
        .route("/_matrix/client/r0/push/rules", get(get_rules))
        .route("/_matrix/client/r0/push/rules", post(create_rule))
        .route("/_matrix/client/r0/push/rules/{scope}/{kind}/{rule_id}", delete(delete_rule))
        .route("/_synapse/admin/v1/push/process", post(process_queue))
        .route("/_synapse/admin/v1/push/cleanup", post(cleanup_logs))
}
