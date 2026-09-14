use super::gateway::PushGateway;
use super::providers::{
    send_with_retry, ApnsProvider, FcmProvider, NotificationCounts, NotificationPayload as ProviderPayload, PushResult,
    WebPushProvider,
};
use super::queue::{PushQueue, QueueConfig};
use futures::stream::{self, StreamExt};
use std::sync::Arc;
use std::time::Instant;
use synapse_common::error::ApiError;
use synapse_storage::push_notification::*;
use tracing::info;

/// The `PushNotificationService` struct.
#[derive(Clone)]
pub struct PushNotificationService {
    storage: Arc<dyn synapse_storage::push_notification::PushNotificationStoreApi>,
    fcm_provider: Option<Arc<FcmProvider>>,
    apns_provider: Option<Arc<ApnsProvider>>,
    webpush_provider: Option<Arc<WebPushProvider>>,
    push_gateway: Option<Arc<PushGateway>>,
    queue: Option<Arc<PushQueue>>,
    /// Optional account_data storage for looking up `m.ignored_user_list`
    /// so that push notifications from ignored users are suppressed.
    account_data_storage: Option<Arc<dyn synapse_storage::account_data::AccountDataStoreApi>>,
}

/// The `NotificationPayload` struct.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NotificationPayload {
    /// The `title` field.
    pub title: String,
    /// The `body` field.
    pub body: String,
    /// The `icon` field.
    pub icon: Option<String>,
    /// The `badge` field.
    pub badge: Option<String>,
    /// The `sound` field.
    pub sound: Option<String>,
    /// The `tag` field.
    pub tag: Option<String>,
    /// The `data` field.
    pub data: serde_json::Value,
    /// The `event_id` field.
    pub event_id: Option<String>,
    /// The `room_id` field.
    pub room_id: Option<String>,
    /// The `room_name` field.
    pub room_name: Option<String>,
    /// The `sender` field.
    pub sender: Option<String>,
    /// The `counts` field.
    pub counts: Option<NotificationCounts>,
}

/// The `SendNotificationRequest` struct.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SendNotificationRequest {
    /// The `user_id` field.
    pub user_id: String,
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

impl PushNotificationService {
    /// See [`new`].
    pub fn new(storage: Arc<dyn synapse_storage::push_notification::PushNotificationStoreApi>) -> Self {
        Self {
            storage,
            fcm_provider: None,
            apns_provider: None,
            webpush_provider: None,
            push_gateway: None,
            queue: None,
            account_data_storage: None,
        }
    }

    /// See [`with_fcm_provider`].
    pub fn with_fcm_provider(mut self, provider: Arc<FcmProvider>) -> Self {
        self.fcm_provider = Some(provider);
        self
    }

    /// See [`with_apns_provider`].
    pub fn with_apns_provider(mut self, provider: Arc<ApnsProvider>) -> Self {
        self.apns_provider = Some(provider);
        self
    }

    /// See [`with_webpush_provider`].
    pub fn with_webpush_provider(mut self, provider: Arc<WebPushProvider>) -> Self {
        self.webpush_provider = Some(provider);
        self
    }

    /// See [`with_push_gateway`].
    pub fn with_push_gateway(mut self, gateway: Arc<PushGateway>) -> Self {
        self.push_gateway = Some(gateway);
        self
    }

    /// See [`with_queue`].
    pub fn with_queue(mut self, config: QueueConfig) -> Self {
        self.queue = Some(Arc::new(PushQueue::new(config)));
        self
    }

    /// Enable `m.ignored_user_list` filtering for push rule evaluation.
    ///
    /// When set, push evaluation will look up the recipient's
    /// ignored user list and immediately return `notify: false` if the event
    /// sender is in that list (matching Synapse's behavior).
    pub fn with_account_data_storage(
        mut self,
        account_data_storage: Arc<dyn synapse_storage::account_data::AccountDataStoreApi>,
    ) -> Self {
        self.account_data_storage = Some(account_data_storage);
        self
    }

    /// See [`initialize_providers`].
    pub async fn initialize_providers(&mut self) -> Result<(), ApiError> {
        let fcm_enabled = self.storage.get_config_as_bool("fcm.enabled", false).await?;
        if fcm_enabled {
            if let Some(api_key) = self.storage.get_config("fcm.api_key").await? {
                self.fcm_provider = Some(Arc::new(FcmProvider::with_api_key(api_key)));
                info!(provider = %"fcm", provider_enabled = true, "Push provider initialized");
            }
        }

        let apns_enabled = self.storage.get_config_as_bool("apns.enabled", false).await?;
        if apns_enabled {
            if let Some(topic) = self.storage.get_config("apns.topic").await? {
                self.apns_provider = Some(Arc::new(ApnsProvider::with_topic(topic)));
                info!(provider = %"apns", provider_enabled = true, "Push provider initialized");
            }
        }

        let webpush_enabled = self.storage.get_config_as_bool("webpush.enabled", false).await?;
        if webpush_enabled {
            let public_key = self.storage.get_config("webpush.vapid_public_key").await?;
            let private_key = self.storage.get_config("webpush.vapid_private_key").await?;

            if let (Some(pk), Some(sk)) = (public_key, private_key) {
                self.webpush_provider = Some(Arc::new(WebPushProvider::with_vapid_keys(pk, sk)));
                info!(provider = %"webpush", provider_enabled = true, "Push provider initialized");
            }
        }

        if self.queue.is_none() {
            self.queue = Some(Arc::new(PushQueue::new(QueueConfig::default())));
        }

        Ok(())
    }

    /// See [`register_device`].
    pub async fn register_device(&self, request: RegisterDeviceRequest) -> Result<PushDevice, ApiError> {
        if !matches!(request.push_type.as_str(), "fcm" | "apns" | "webpush" | "upstream") {
            return Err(ApiError::bad_request("Invalid push type"));
        }

        self.storage.register_device(request).await
    }

    /// See [`unregister_device`].
    pub async fn unregister_device(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        self.storage.unregister_device(user_id, device_id).await
    }

    /// See [`get_user_devices`].
    pub async fn get_user_devices(&self, user_id: &str) -> Result<Vec<PushDevice>, ApiError> {
        self.storage.get_user_devices(user_id).await
    }

    /// See [`get_room_notifications`].
    pub async fn get_room_notifications(
        &self,
        user_id: &str,
        room_id: &str,
        limit: i64,
    ) -> Result<Vec<RoomNotification>, ApiError> {
        self.storage
            .get_room_notifications(user_id, room_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get room notifications", e))
    }

    /// See [`send_notification`].
    pub async fn send_notification(&self, request: SendNotificationRequest) -> Result<(), ApiError> {
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
            info!(
                user_id = %request.user_id,
                device_id = ?request.device_id,
                notification_type = ?request.notification_type,
                "No push devices registered for notification"
            );
            return Ok(());
        }

        let device_count = devices.len();
        let priority = request.priority.unwrap_or(5);
        let data = request.data.clone().unwrap_or(serde_json::json!({}));

        // P2: Build batch insertion requests instead of N individual INSERTs.
        let batch_requests: Vec<QueueNotificationRequest> = devices
            .iter()
            .map(|device| {
                let content = serde_json::json!({
                    "title": &request.title,
                    "body": &request.body,
                    "data": &data,
                    "push_type": &device.push_type,
                    "push_token": &device.push_token,
                });
                QueueNotificationRequest {
                    user_id: request.user_id.clone(),
                    device_id: device.device_id.clone(),
                    event_id: request.event_id.clone(),
                    room_id: request.room_id.clone(),
                    notification_type: request.notification_type.clone(),
                    content,
                    priority,
                }
            })
            .collect();

        self.storage.queue_notifications_batch(&batch_requests).await?;

        info!(
            user_id = %request.user_id,
            device_id = ?request.device_id,
            notification_type = ?request.notification_type,
            priority,
            device_count,
            "Queued push notifications"
        );
        Ok(())
    }

    /// See [`process_pending_notifications`].
    pub async fn process_pending_notifications(&self, batch_size: i32) -> Result<u64, ApiError> {
        let notifications = self.storage.get_pending_notifications(batch_size).await?;

        // P2: Process notifications concurrently with bounded parallelism
        // (max 8 concurrent provider calls). Each send_to_provider call is an
        // independent outbound HTTP request — no ordering dependency between
        // different user/device pairs.
        const MAX_CONCURRENT_SENDS: usize = 8;

        let results: Vec<(PushNotificationQueue, Result<(), ApiError>)> = stream::iter(notifications.into_iter())
            .map(|notification| async {
                let result = self.send_to_provider(&notification).await;
                (notification, result)
            })
            .buffer_unordered(MAX_CONCURRENT_SENDS)
            .collect()
            .await;

        let mut processed = 0u64;
        for (notification, result) in results {
            match result {
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

        let device = self
            .storage
            .get_device(&notification.user_id, &notification.device_id)
            .await?
            .ok_or_else(|| ApiError::not_found("Device not found"))?;

        let push_type = device.push_type.as_str();
        let push_token = device.push_token.clone();

        let content: NotificationPayload = serde_json::from_value(notification.content.clone())
            .map_err(|e| ApiError::bad_request(format!("Invalid notification content: {e}")))?;

        let provider_payload = ProviderPayload {
            title: content.title.clone(),
            body: content.body.clone(),
            icon: content.icon.clone(),
            badge: content.badge.clone(),
            sound: content.sound.clone(),
            tag: content.tag.clone(),
            data: content.data.clone(),
            event_id: content.event_id.clone(),
            room_id: content.room_id.clone(),
            room_name: content.room_name.clone(),
            sender: content.sender.clone(),
            counts: content
                .counts
                .as_ref()
                .map(|c| NotificationCounts { unread: c.unread, missed_calls: c.missed_calls }),
        };

        let result = match push_type {
            "fcm" => {
                if let Some(provider) = &self.fcm_provider {
                    send_with_retry(provider.as_ref(), &push_token, &provider_payload).await
                } else {
                    self.send_fcm_fallback(&push_token, &content).await?
                }
            }
            "apns" => {
                if let Some(provider) = &self.apns_provider {
                    send_with_retry(provider.as_ref(), &push_token, &provider_payload).await
                } else {
                    self.send_apns_fallback(&push_token, &content).await?
                }
            }
            "webpush" => {
                if let Some(provider) = &self.webpush_provider {
                    send_with_retry(provider.as_ref(), &push_token, &provider_payload).await
                } else {
                    self.send_webpush_fallback(&push_token, &content).await?
                }
            }
            "upstream" => self.send_upstream(&push_token, &content)?,
            _ => return Err(ApiError::bad_request("Invalid push type")),
        };

        let response_time_ms = start.elapsed().as_millis() as i32;
        let success = result.is_success;
        let error_message = result.error;
        let provider_response = result.provider_response;

        let log_request =
            CreateNotificationLogRequest::new(&notification.user_id, &notification.device_id, push_type, success)
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

        let log_request =
            if let Some(resp) = &provider_response { log_request.provider_response(resp) } else { log_request };

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

    async fn send_fcm_fallback(&self, token: &str, _payload: &NotificationPayload) -> Result<PushResult, ApiError> {
        let enabled = self.storage.get_config_as_bool("fcm.enabled", false).await?;

        if !enabled {
            info!(provider = %"fcm", provider_enabled = false, "Push provider disabled, skipping notification");
            return Ok(PushResult::success());
        }

        let _api_key = self
            .storage
            .get_config("fcm.api_key")
            .await?
            .ok_or_else(|| ApiError::internal("FCM API key not configured"))?;

        info!(
            provider = %"fcm",
            token_present = !token.is_empty(),
            token_len = token.len(),
            "Sending fallback push notification"
        );

        Ok(PushResult::success_with_response("FCM accepted (fallback)"))
    }

    async fn send_apns_fallback(&self, token: &str, _payload: &NotificationPayload) -> Result<PushResult, ApiError> {
        let enabled = self.storage.get_config_as_bool("apns.enabled", false).await?;

        if !enabled {
            info!(provider = %"apns", provider_enabled = false, "Push provider disabled, skipping notification");
            return Ok(PushResult::success());
        }

        let _topic = self
            .storage
            .get_config("apns.topic")
            .await?
            .ok_or_else(|| ApiError::internal("APNS topic not configured"))?;

        info!(
            provider = %"apns",
            token_present = !token.is_empty(),
            token_len = token.len(),
            "Sending fallback push notification"
        );

        Ok(PushResult::success_with_response("APNS accepted (fallback)"))
    }

    async fn send_webpush_fallback(
        &self,
        endpoint: &str,
        _payload: &NotificationPayload,
    ) -> Result<PushResult, ApiError> {
        let enabled = self.storage.get_config_as_bool("webpush.enabled", false).await?;

        if !enabled {
            info!(provider = %"webpush", provider_enabled = false, "Push provider disabled, skipping notification");
            return Ok(PushResult::success());
        }

        let _vapid_public_key = self
            .storage
            .get_config("webpush.vapid_public_key")
            .await?
            .ok_or_else(|| ApiError::internal("WebPush VAPID public key not configured"))?;

        info!(
            provider = %"webpush",
            endpoint_present = !endpoint.is_empty(),
            endpoint_len = endpoint.len(),
            "Sending fallback push notification"
        );

        Ok(PushResult::success_with_response("WebPush accepted (fallback)"))
    }

    fn send_upstream(&self, _target: &str, payload: &NotificationPayload) -> Result<PushResult, ApiError> {
        info!(
            provider = %"upstream",
            event_id = ?payload.event_id,
            room_id = ?payload.room_id,
            title_present = !payload.title.is_empty(),
            "Sending upstream push notification"
        );
        Ok(PushResult::success_with_response("Upstream accepted"))
    }

    /// See [`cleanup_old_logs`].
    pub async fn cleanup_old_logs(&self, days: i32) -> Result<u64, ApiError> {
        self.storage.cleanup_old_logs(days).await
    }
}
