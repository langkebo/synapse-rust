use crate::routes::context::AdminContext;
use crate::routes::{AdminUser, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use synapse_common::error::ApiError;
use synapse_common::types::DeviceId;
use synapse_services::push::{PushDevice, RegisterDeviceRequest};
use synapse_services::push_notification_service::SendNotificationRequest;

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

/// The `SetPushConfigBody` struct.
///
/// `null` for a key deletes it, which is the only way to drop a credential that is
/// no longer wanted (an empty string would be rejected as "not configured" and
/// would leave the row behind).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetPushConfigBody {
    /// The `config` field: `config_key` -> value (or `null` to delete).
    pub config: std::collections::BTreeMap<String, Option<String>>,
}

/// Masks a credential for read-back: all but the last 4 characters.
fn mask_secret(value: &str) -> String {
    let mut chars: Vec<char> = value.chars().collect();
    if chars.len() > 4 {
        let tail: String = chars.split_off(chars.len() - 4).into_iter().collect();
        return format!("{}{tail}", "*".repeat(chars.len()));
    }
    "*".repeat(chars.len())
}

/// Renders the stored config as JSON with secrets masked.
fn config_view(entries: Vec<synapse_services::push::PushConfigEntry>) -> serde_json::Value {
    let mut config = serde_json::Map::new();
    for entry in entries {
        let value = if synapse_services::push_notification_service::SECRET_PUSH_CONFIG_KEYS
            .contains(&entry.config_key.as_str())
        {
            mask_secret(&entry.config_value)
        } else {
            entry.config_value
        };
        config.insert(entry.config_key, serde_json::Value::String(value));
    }
    serde_json::Value::Object(config)
}

/// Validates a push-config patch without touching the database, so a rejected key
/// cannot leave `push_config` half-updated.
///
/// The allowlist is the exact set of keys `initialize_providers` consumes; accepting
/// anything else would let the table accumulate settings that look applied but are not.
pub fn validate_push_config_patch(config: &std::collections::BTreeMap<String, Option<String>>) -> Result<(), ApiError> {
    use synapse_services::push_notification_service::SUPPORTED_PUSH_CONFIG_KEYS;

    if config.is_empty() {
        return Err(ApiError::bad_request("`config` must contain at least one key".to_string()));
    }

    for (key, value) in config {
        if !SUPPORTED_PUSH_CONFIG_KEYS.contains(&key.as_str()) {
            return Err(ApiError::bad_request(format!(
                "unsupported push config key `{key}`; supported keys: {}",
                SUPPORTED_PUSH_CONFIG_KEYS.join(", ")
            )));
        }
        if key.ends_with(".enabled") {
            let valid = matches!(
                value.as_deref(),
                Some(v) if v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("false")
            );
            if !valid {
                return Err(ApiError::bad_request(format!("`{key}` must be \"true\" or \"false\"")));
            }
        }
    }

    Ok(())
}

/// See [`get_push_config`].
pub async fn get_push_config(
    State(ctx): State<AdminContext>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let entries = ctx.push_notification_service.list_config().await?;

    Ok(Json(serde_json::json!({
        "config": config_view(entries),
        "initialized_providers": ctx.push_notification_service.initialized_providers(),
        "supported_keys": synapse_services::push_notification_service::SUPPORTED_PUSH_CONFIG_KEYS,
    })))
}

/// See [`put_push_config`].
///
/// Applies the change immediately: the providers live behind a lock, so an operator
/// does not have to restart the homeserver.
pub async fn put_push_config(
    State(ctx): State<AdminContext>,
    _admin: AdminUser,
    Json(body): Json<SetPushConfigBody>,
) -> Result<impl IntoResponse, ApiError> {
    validate_push_config_patch(&body.config)?;

    for (key, value) in &body.config {
        match value {
            Some(value) => {
                ctx.push_notification_service.set_config(key, value).await?;
            }
            None => {
                ctx.push_notification_service.delete_config(key).await?;
            }
        }
    }

    ctx.push_notification_service.initialize_providers().await?;

    let entries = ctx.push_notification_service.list_config().await?;
    Ok(Json(serde_json::json!({
        "config": config_view(entries),
        "initialized_providers": ctx.push_notification_service.initialized_providers(),
    })))
}

/// See [`create_push_notification_router`].
pub fn create_push_notification_router(state: AppState) -> axum::Router<AppState> {
    use axum::routing::*;

    let public_routes = axum::Router::new()
        .route("/_matrix/client/v3/push/devices", get(get_devices))
        .route("/_matrix/client/v3/push/devices", post(register_device))
        .route("/_matrix/client/v3/push/devices/{device_id}", delete(unregister_device))
        .route("/_matrix/client/v3/push/send", post(send_notification));

    let admin_routes = axum::Router::new()
        .route("/_synapse/admin/v1/push/process", post(process_queue))
        .route("/_synapse/admin/v1/push/cleanup", post(cleanup_logs))
        .route("/_synapse/admin/v1/push/config", get(get_push_config))
        .route("/_synapse/admin/v1/push/config", put(put_push_config))
        .route_layer(axum::middleware::from_fn_with_state(
            <crate::routes::context::AdminContext as axum::extract::FromRef<crate::routes::AppState>>::from_ref(&state),
            crate::middleware::admin_auth_middleware,
        ));

    public_routes.merge(admin_routes).with_state(state)
}

// -------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that `mask_secret` masks all but the last 4 characters.
    #[test]
    fn test_mask_secret_long_value() {
        let masked = mask_secret("abcdefghij");
        assert_eq!(masked, "******ghij");
    }

    /// Test that `mask_secret` masks short values completely.
    #[test]
    fn test_mask_secret_short_value() {
        let masked = mask_secret("ab");
        assert_eq!(masked, "**");
    }

    /// Test that `mask_secret` handles exactly 4 characters.
    #[test]
    fn test_mask_secret_exactly_four() {
        let masked = mask_secret("abcd");
        assert_eq!(masked, "****");
    }

    /// Test that `validate_push_config_patch` rejects empty config.
    #[test]
    fn test_validate_push_config_patch_empty_rejected() {
        let config = std::collections::BTreeMap::new();
        let result = validate_push_config_patch(&config);
        assert!(result.is_err());
    }

    /// Test that `validate_push_config_patch` rejects unsupported keys.
    #[test]
    fn test_validate_push_config_patch_unsupported_key_rejected() {
        let mut config = std::collections::BTreeMap::new();
        config.insert("unsupported_key".to_string(), Some("value".to_string()));
        let result = validate_push_config_patch(&config);
        assert!(result.is_err());
    }

    /// Test that `validate_push_config_patch` rejects invalid boolean values.
    #[test]
    fn test_validate_push_config_patch_invalid_boolean_rejected() {
        let mut config = std::collections::BTreeMap::new();
        config.insert("apns.enabled".to_string(), Some("not_a_bool".to_string()));
        let result = validate_push_config_patch(&config);
        assert!(result.is_err());
    }

    /// Test that `validate_push_config_patch` accepts valid boolean values.
    #[test]
    fn test_validate_push_config_patch_valid_boolean_accepted() {
        let mut config = std::collections::BTreeMap::new();
        config.insert("apns.enabled".to_string(), Some("true".to_string()));
        let result = validate_push_config_patch(&config);
        assert!(result.is_ok());
    }

    /// Test that `validate_push_config_patch` accepts valid config.
    #[test]
    fn test_validate_push_config_patch_valid_config_accepted() {
        let mut config = std::collections::BTreeMap::new();
        config.insert("apns.enabled".to_string(), Some("true".to_string()));
        config.insert("apns.token".to_string(), Some("test-token".to_string()));
        let result = validate_push_config_patch(&config);
        assert!(result.is_ok());
    }

    /// Test that `DeviceResponse::from` correctly converts PushDevice.
    #[test]
    fn test_device_response_from_push_device() {
        let device = PushDevice {
            id: 1,
            user_id: "@user:example.com".to_string(),
            device_id: "test-device".to_string(),
            push_token: "token-1".to_string(),
            push_type: "apns".to_string(),
            app_id: Some("app-1".to_string()),
            platform: Some("ios".to_string()),
            platform_version: Some("17.0".to_string()),
            app_version: Some("1.0.0".to_string()),
            locale: Some("en".to_string()),
            timezone: Some("UTC".to_string()),
            is_enabled: true,
            created_ts: 1000,
            updated_ts: Some(2000),
            last_used_ts: Some(2000),
            last_error: None,
            error_count: 0,
            metadata: serde_json::Value::Null,
        };
        let response = DeviceResponse::from(device);
        assert_eq!(response.device_id, "test-device");
        assert_eq!(response.push_type, "apns");
        assert!(response.enabled);
    }

    /// Test that `RegisterDeviceBody` deserialization works with all fields.
    #[test]
    fn test_register_device_body_deserialization() {
        let json = serde_json::json!({
            "device_id": "dev-1",
            "push_token": "token-1",
            "push_type": "apns",
            "app_id": "app-1",
            "platform": "ios",
            "platform_version": "17.0",
            "app_version": "1.0.0",
            "locale": "en",
            "timezone": "UTC"
        });
        let body: RegisterDeviceBody = serde_json::from_value(json).unwrap();
        assert_eq!(body.device_id, "dev-1");
        assert_eq!(body.push_type, "apns");
        assert_eq!(body.app_id, Some("app-1".to_string()));
    }

    /// Test that `RegisterDeviceBody` deserialization rejects unknown fields.
    #[test]
    fn test_register_device_body_rejects_unknown_fields() {
        let json = serde_json::json!({
            "device_id": "dev-1",
            "push_token": "token-1",
            "push_type": "apns",
            "unknown_field": "should_fail"
        });
        let result: Result<RegisterDeviceBody, _> = serde_json::from_value(json);
        assert!(result.is_err());
    }

    /// Test that `SendNotificationBody` deserialization works.
    #[test]
    fn test_send_notification_body_deserialization() {
        let json = serde_json::json!({
            "title": "Test",
            "body": "Message body",
            "priority": 5
        });
        let body: SendNotificationBody = serde_json::from_value(json).unwrap();
        assert_eq!(body.title, "Test");
        assert_eq!(body.body, "Message body");
        assert_eq!(body.priority, Some(5));
    }

    /// Test that `ProcessQueueQuery` deserialization works.
    #[test]
    fn test_process_queue_query_deserialization() {
        let json = serde_json::json!({
            "batch_size": 50
        });
        let query: ProcessQueueQuery = serde_json::from_value(json).unwrap();
        assert_eq!(query.batch_size, Some(50));
    }

    /// Test that `CleanupQuery` deserialization works.
    #[test]
    fn test_cleanup_query_deserialization() {
        let json = serde_json::json!({
            "days": 7
        });
        let query: CleanupQuery = serde_json::from_value(json).unwrap();
        assert_eq!(query.days, Some(7));
    }
}
