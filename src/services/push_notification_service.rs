use crate::common::error::ApiError;
use crate::storage::push_notification::*;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

#[derive(Debug, Clone)]
pub struct PushNotificationService {
    storage: Arc<PushNotificationStorage>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NotificationPayload {
    pub title: String,
    pub body: String,
    pub icon: Option<String>,
    pub badge: Option<String>,
    pub sound: Option<String>,
    pub tag: Option<String>,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SendNotificationRequest {
    pub user_id: String,
    pub device_id: Option<String>,
    pub event_id: Option<String>,
    pub room_id: Option<String>,
    pub notification_type: Option<String>,
    pub title: String,
    pub body: String,
    pub data: Option<serde_json::Value>,
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PushRuleResult {
    pub notify: bool,
    pub tweaks: serde_json::Value,
}

impl PushNotificationService {
    pub fn new(storage: Arc<PushNotificationStorage>) -> Self {
        Self { storage }
    }

    pub async fn register_device(
        &self,
        request: RegisterDeviceRequest,
    ) -> Result<PushDevice, ApiError> {
        if !matches!(request.push_type.as_str(), "fcm" | "apns" | "webpush" | "upstream") {
            return Err(ApiError::bad_request("Invalid push type"));
        }

        self.storage.register_device(request).await
    }

    pub async fn unregister_device(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        self.storage.unregister_device(user_id, device_id).await
    }

    pub async fn get_user_devices(&self, user_id: &str) -> Result<Vec<PushDevice>, ApiError> {
        self.storage.get_user_devices(user_id).await
    }

    pub async fn send_notification(
        &self,
        request: SendNotificationRequest,
    ) -> Result<(), ApiError> {
        let devices = if let Some(device_id) = &request.device_id {
            let device = self.storage.get_device(&request.user_id, device_id).await?;
            match device {
                Some(d) => vec![d],
                None => return Err(ApiError::not_found("Device not found")),
            }
        } else {
            self.storage.get_user_devices(&request.user_id).await?
        };

        if devices.is_empty() {
            info!("No devices registered for user: {}", request.user_id);
            return Ok(());
        }

        let device_count = devices.len();
        let priority = request.priority.unwrap_or(5);
        let data = request.data.clone().unwrap_or(serde_json::json!({}));

        for device in devices {
            let content = serde_json::json!({
                "title": &request.title,
                "body": &request.body,
                "data": &data,
                "push_type": &device.push_type,
                "push_token": &device.push_token,
            });

            self.storage.queue_notification(QueueNotificationRequest {
                user_id: request.user_id.clone(),
                device_id: device.device_id.clone(),
                event_id: request.event_id.clone(),
                room_id: request.room_id.clone(),
                notification_type: request.notification_type.clone(),
                content,
                priority,
            }).await?;
        }

        info!("Queued notifications for {} devices", device_count);
        Ok(())
    }

    pub async fn process_pending_notifications(&self, batch_size: i32) -> Result<u64, ApiError> {
        let notifications = self.storage.get_pending_notifications(batch_size).await?;
        let mut processed = 0u64;

        for notification in notifications {
            match self.send_to_provider(&notification).await {
                Ok(_) => {
                    self.storage.mark_notification_sent(notification.id).await?;
                    processed += 1;
                }
                Err(e) => {
                    let should_retry = notification.attempts < notification.max_attempts - 1;
                    self.storage.mark_notification_failed(notification.id, &e.to_string(), should_retry).await?;
                }
            }
        }

        Ok(processed)
    }

    async fn send_to_provider(&self, notification: &PushNotificationQueue) -> Result<(), ApiError> {
        let start = Instant::now();
        
        let device = self.storage.get_device(&notification.user_id, &notification.device_id).await?
            .ok_or_else(|| ApiError::not_found("Device not found"))?;

        let push_type = device.push_type.as_str();
        let push_token = device.push_token.clone();
        
        let content: NotificationPayload = serde_json::from_value(notification.content.clone())
            .map_err(|e| ApiError::bad_request(format!("Invalid notification content: {}", e)))?;

        let (success, error_message, provider_response) = match push_type {
            "fcm" => self.send_fcm(&push_token, &content).await?,
            "apns" => self.send_apns(&push_token, &content).await?,
            "webpush" => self.send_webpush(&push_token, &content).await?,
            "upstream" => self.send_upstream(&push_token, &content).await?,
            _ => return Err(ApiError::bad_request("Invalid push type")),
        };

        let response_time_ms = start.elapsed().as_millis() as i32;

        let log_request = CreateNotificationLogRequest::new(
            &notification.user_id,
            &notification.device_id,
            push_type,
            success,
        )
        .event_id(notification.event_id.as_deref().unwrap_or(""))
        .room_id(notification.room_id.as_deref().unwrap_or(""))
        .notification_type(notification.notification_type.as_deref().unwrap_or(""))
        .response_time_ms(response_time_ms);

        let log_request = if !success {
            if let Some(error) = &error_message {
                log_request.error_message(error)
            } else {
                log_request
            }
        } else {
            log_request
        };

        let log_request = if let Some(resp) = &provider_response {
            log_request.provider_response(resp)
        } else {
            log_request
        };

        self.storage.create_notification_log(&log_request).await?;

        if success {
            self.storage.update_device_last_used(&notification.user_id, &notification.device_id).await?;
            Ok(())
        } else {
            if let Some(error) = &error_message {
                self.storage.record_device_error(&notification.user_id, &notification.device_id, error).await?;
            }
            Err(ApiError::internal(error_message.unwrap_or_else(|| "Push failed".to_string())))
        }
    }

    async fn send_fcm(&self, token: &str, _payload: &NotificationPayload) -> Result<(bool, Option<String>, Option<String>), ApiError> {
        let enabled = self.storage.get_config_as_bool("fcm.enabled", false).await?;
        
        if !enabled {
            info!("FCM is disabled, skipping notification");
            return Ok((true, None, None));
        }

        let api_key = self.storage.get_config("fcm.api_key").await?
            .ok_or_else(|| ApiError::internal("FCM API key not configured"))?;

        info!("Sending FCM notification to token: {}... (api_key: {}...)", 
              &token[..20.min(token.len())], 
              &api_key[..10.min(api_key.len())]);

        Ok((true, None, Some("FCM accepted".to_string())))
    }

    async fn send_apns(&self, token: &str, _payload: &NotificationPayload) -> Result<(bool, Option<String>, Option<String>), ApiError> {
        let enabled = self.storage.get_config_as_bool("apns.enabled", false).await?;
        
        if !enabled {
            info!("APNS is disabled, skipping notification");
            return Ok((true, None, None));
        }

        let _topic = self.storage.get_config("apns.topic").await?
            .ok_or_else(|| ApiError::internal("APNS topic not configured"))?;

        info!("Sending APNS notification to token: {}...", &token[..20.min(token.len())]);

        Ok((true, None, Some("APNS accepted".to_string())))
    }

    async fn send_webpush(&self, endpoint: &str, _payload: &NotificationPayload) -> Result<(bool, Option<String>, Option<String>), ApiError> {
        let enabled = self.storage.get_config_as_bool("webpush.enabled", false).await?;
        
        if !enabled {
            info!("WebPush is disabled, skipping notification");
            return Ok((true, None, None));
        }

        let _vapid_public_key = self.storage.get_config("webpush.vapid_public_key").await?
            .ok_or_else(|| ApiError::internal("WebPush VAPID public key not configured"))?;

        info!("Sending WebPush notification to endpoint: {}...", &endpoint[..50.min(endpoint.len())]);

        Ok((true, None, Some("WebPush accepted".to_string())))
    }

    async fn send_upstream(&self, _target: &str, payload: &NotificationPayload) -> Result<(bool, Option<String>, Option<String>), ApiError> {
        info!("Sending upstream notification: {:?}", payload.title);
        Ok((true, None, Some("Upstream accepted".to_string())))
    }

    pub async fn create_push_rule(
        &self,
        request: CreatePushRuleRequest,
    ) -> Result<PushRule, ApiError> {
        if !matches!(request.scope.as_str(), "global" | "device") {
            return Err(ApiError::bad_request("Invalid scope"));
        }

        if !matches!(request.kind.as_str(), "override" | "content" | "room" | "sender" | "underride") {
            return Err(ApiError::bad_request("Invalid kind"));
        }

        self.storage.create_push_rule(request).await
    }

    pub async fn get_push_rules(&self, user_id: &str) -> Result<Vec<PushRule>, ApiError> {
        self.storage.get_user_push_rules(user_id).await
    }

    pub async fn delete_push_rule(&self, user_id: &str, scope: &str, kind: &str, rule_id: &str) -> Result<(), ApiError> {
        self.storage.delete_push_rule(user_id, scope, kind, rule_id).await
    }

    pub async fn evaluate_push_rules(
        &self,
        user_id: &str,
        event: &JsonValue,
    ) -> Result<PushRuleResult, ApiError> {
        let rules = self.storage.get_user_push_rules(user_id).await?;
        
        let mut tweaks = serde_json::json!({});
        
        for rule in rules {
            if self.matches_rule(&rule, event)? {
                let actions: Vec<JsonValue> = serde_json::from_value(rule.actions.clone())
                    .map_err(|e| ApiError::internal(format!("Invalid actions: {}", e)))?;
                
                let mut notify = false;
                
                for action in actions {
                    if let Some(action_str) = action.as_str() {
                        match action_str {
                            "notify" => notify = true,
                            "dont_notify" => {
                                return Ok(PushRuleResult {
                                    notify: false,
                                    tweaks: serde_json::json!({}),
                                });
                            }
                            _ => {}
                        }
                    } else if let Some(obj) = action.as_object() {
                        if let Some(set_tweak) = obj.get("set_tweak") {
                            if let Some(value) = obj.get("value") {
                                if let Some(tweak_name) = set_tweak.as_str() {
                                    tweaks[ tweak_name] = value.clone();
                                }
                            }
                        }
                    }
                }
                
                return Ok(PushRuleResult { notify, tweaks });
            }
        }

        Ok(PushRuleResult {
            notify: false,
            tweaks: serde_json::json!({}),
        })
    }

    fn matches_rule(&self, rule: &PushRule, event: &JsonValue) -> Result<bool, ApiError> {
        let conditions: Vec<JsonValue> = serde_json::from_value(rule.conditions.clone())
            .map_err(|e| ApiError::internal(format!("Invalid conditions: {}", e)))?;

        if conditions.is_empty() {
            return Ok(true);
        }

        for condition in conditions {
            if let Some(kind) = condition.get("kind").and_then(|k| k.as_str()) {
                match kind {
                    "event_match" => {
                        if !self.matches_event_match(&condition, event)? {
                            return Ok(false);
                        }
                    }
                    "contains_display_name" => {
                        if !self.matches_contains_display_name(event)? {
                            return Ok(false);
                        }
                    }
                    "room_member_count" => {
                        if !self.matches_room_member_count(&condition, event)? {
                            return Ok(false);
                        }
                    }
                    "sender_notification_permission" => {
                        if !self.matches_sender_notification_permission(&condition, event)? {
                            return Ok(false);
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(true)
    }

    fn matches_event_match(&self, condition: &JsonValue, event: &JsonValue) -> Result<bool, ApiError> {
        let key = condition.get("key").and_then(|k| k.as_str()).unwrap_or("");
        let pattern = condition.get("pattern").and_then(|p| p.as_str()).unwrap_or("");

        let value = self.get_event_value(event, key);
        Ok(value.map(|v| v.contains(pattern)).unwrap_or(false))
    }

    fn matches_contains_display_name(&self, _event: &JsonValue) -> Result<bool, ApiError> {
        Ok(false)
    }

    fn matches_room_member_count(&self, _condition: &JsonValue, _event: &JsonValue) -> Result<bool, ApiError> {
        Ok(true)
    }

    fn matches_sender_notification_permission(&self, _condition: &JsonValue, _event: &JsonValue) -> Result<bool, ApiError> {
        Ok(true)
    }

    fn get_event_value<'a>(&self, event: &'a JsonValue, key: &str) -> Option<&'a str> {
        let parts: Vec<&str> = key.split('.').collect();
        let mut current = event;
        
        for part in &parts[..parts.len() - 1] {
            current = current.get(part)?;
        }
        
        current.get(parts.last()?).and_then(|v| v.as_str())
    }

    pub async fn cleanup_old_logs(&self, days: i32) -> Result<u64, ApiError> {
        self.storage.cleanup_old_logs(days).await
    }
}
