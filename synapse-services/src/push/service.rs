use super::gateway::PushGateway;
use super::providers::{
    send_with_retry, ApnsProvider, FcmProvider, NotificationCounts, NotificationPayload as ProviderPayload, PushResult,
    WebPushProvider,
};
use futures::stream::{self, StreamExt};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use synapse_common::error::ApiError;
use synapse_storage::push_notification::*;
use tracing::{info, warn};

/// The providers built from `push_config` by [`PushNotificationService::initialize_providers`].
///
/// Held behind a lock so an admin config update can hot-reload them without a
/// restart; [`PushNotificationService`] is shared as an `Arc` and cannot be
/// replaced in place.
#[derive(Clone, Default)]
struct PushProviders {
    fcm: Option<Arc<FcmProvider>>,
    apns: Option<Arc<ApnsProvider>>,
    webpush: Option<Arc<WebPushProvider>>,
}

/// The `PushNotificationService` struct.
#[derive(Clone)]
pub struct PushNotificationService {
    storage: Arc<dyn synapse_storage::push_notification::PushNotificationStoreApi>,
    providers: Arc<RwLock<PushProviders>>,
    push_gateway: Option<Arc<PushGateway>>,
    /// Optional account_data storage for looking up `m.ignored_user_list`
    /// so that push notifications from ignored users are suppressed.
    account_data_storage: Option<Arc<dyn synapse_storage::account_data::AccountDataStoreApi>>,
}

/// Treats an empty/whitespace-only config value as "not set".
///
/// `push_config` has no per-key nullability, and every provider constructor has a
/// "disabled" state for an empty credential — building a provider from a blank
/// value would silently succeed on every send instead of failing closed.
fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
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
            providers: Arc::new(RwLock::new(PushProviders::default())),
            push_gateway: None,
            account_data_storage: None,
        }
    }

    fn write_providers(&self) -> std::sync::RwLockWriteGuard<'_, PushProviders> {
        // A poisoned lock only means a previous writer panicked; the value itself is
        // still a valid `PushProviders`, so recover it rather than propagating.
        self.providers.write().unwrap_or_else(|error| error.into_inner())
    }

    fn read_providers(&self) -> std::sync::RwLockReadGuard<'_, PushProviders> {
        self.providers.read().unwrap_or_else(|error| error.into_inner())
    }

    /// See [`with_fcm_provider`].
    pub fn with_fcm_provider(self, provider: Arc<FcmProvider>) -> Self {
        self.write_providers().fcm = Some(provider);
        self
    }

    /// See [`with_apns_provider`].
    pub fn with_apns_provider(self, provider: Arc<ApnsProvider>) -> Self {
        self.write_providers().apns = Some(provider);
        self
    }

    /// See [`with_webpush_provider`].
    pub fn with_webpush_provider(self, provider: Arc<WebPushProvider>) -> Self {
        self.write_providers().webpush = Some(provider);
        self
    }

    /// See [`with_push_gateway`].
    pub fn with_push_gateway(mut self, gateway: Arc<PushGateway>) -> Self {
        self.push_gateway = Some(gateway);
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

    /// Rebuilds every provider from the current `push_config` rows.
    ///
    /// Takes `&self` on purpose: the service is shared as an `Arc`, and the admin
    /// config endpoint relies on this to apply changes without a restart.
    pub async fn initialize_providers(&self) -> Result<(), ApiError> {
        let mut providers = PushProviders::default();

        let fcm_enabled = self.storage.get_config_as_bool("fcm.enabled", false).await?;
        if fcm_enabled {
            // An empty value counts as "not configured": `FcmProvider` treats an empty
            // API key as disabled, and a `Some(disabled provider)` would silently
            // succeed on every send.
            if let Some(api_key) = non_empty(self.storage.get_config("fcm.api_key").await?) {
                providers.fcm = Some(Arc::new(FcmProvider::with_api_key(api_key)));
                info!(provider = %"fcm", provider_enabled = true, "Push provider initialized");
            }
        }

        let apns_enabled = self.storage.get_config_as_bool("apns.enabled", false).await?;
        if apns_enabled {
            if let Some(topic) = non_empty(self.storage.get_config("apns.topic").await?) {
                providers.apns = Some(Arc::new(ApnsProvider::with_topic(topic)));
                info!(provider = %"apns", provider_enabled = true, "Push provider initialized");
            }
        }

        let webpush_enabled = self.storage.get_config_as_bool("webpush.enabled", false).await?;
        if webpush_enabled {
            let public_key = non_empty(self.storage.get_config("webpush.vapid_public_key").await?);
            let private_key = non_empty(self.storage.get_config("webpush.vapid_private_key").await?);

            if let (Some(pk), Some(sk)) = (public_key, private_key) {
                providers.webpush = Some(Arc::new(WebPushProvider::with_vapid_keys(pk, sk)));
                info!(provider = %"webpush", provider_enabled = true, "Push provider initialized");
            }
        }

        *self.write_providers() = providers;
        Ok(())
    }

    /// Names of the push providers that [`initialize_providers`](Self::initialize_providers)
    /// successfully built, in a stable order.
    ///
    /// Surfaced by the admin push-config endpoints so an operator can see whether a
    /// configuration change actually produced a working provider; an empty result
    /// means every delivery is handled as "provider unavailable".
    pub fn initialized_providers(&self) -> Vec<&'static str> {
        let providers = self.read_providers();
        let mut names = Vec::new();
        if providers.fcm.is_some() {
            names.push("fcm");
        }
        if providers.apns.is_some() {
            names.push("apns");
        }
        if providers.webpush.is_some() {
            names.push("webpush");
        }
        names
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

        // Clone the provider handle out of the lock and release it before awaiting:
        // holding a `std` guard across an await would block the admin config endpoint.
        let (fcm, apns, webpush) = {
            let providers = self.read_providers();
            (providers.fcm.clone(), providers.apns.clone(), providers.webpush.clone())
        };

        let result = match push_type {
            "fcm" => match fcm {
                Some(provider) => send_with_retry(provider.as_ref(), &push_token, &provider_payload).await,
                None => self.provider_unavailable("fcm").await?,
            },
            "apns" => match apns {
                Some(provider) => send_with_retry(provider.as_ref(), &push_token, &provider_payload).await,
                None => self.provider_unavailable("apns").await?,
            },
            "webpush" => match webpush {
                Some(provider) => send_with_retry(provider.as_ref(), &push_token, &provider_payload).await,
                None => self.provider_unavailable("webpush").await?,
            },
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

        // Bookkeeping after the provider call is best effort. It must NOT decide the
        // delivery outcome: `process_pending_notifications` turns an `Err` here into
        // `mark_notification_failed`, so a failing log/device write after a *successful*
        // send would retry an already-delivered push (duplicate notifications).
        if let Err(error) = self.storage.create_notification_log(&log_request).await {
            warn!(
                %error,
                user_id = %notification.user_id,
                device_id = %notification.device_id,
                push_type,
                "Failed to persist the push notification log; the delivery outcome is unaffected"
            );
        }

        if success {
            if let Err(error) =
                self.storage.update_device_last_used(&notification.user_id, &notification.device_id).await
            {
                warn!(%error, device_id = %notification.device_id, "Failed to update push device last_used_at");
            }
            Ok(())
        } else {
            if let Some(error) = &error_message {
                if let Err(record_error) =
                    self.storage.record_device_error(&notification.user_id, &notification.device_id, error).await
                {
                    warn!(%record_error, device_id = %notification.device_id, "Failed to record the push device error");
                }
            }
            Err(ApiError::internal(error_message.unwrap_or_else(|| "Push failed".to_string())))
        }
    }

    /// Handles a notification whose provider was never initialized.
    ///
    /// `initialize_providers` leaves a provider `None` in two situations:
    ///   1. the push type is disabled in `push_config` — there is nothing to send
    ///      and retrying forever would be pointless, so the notification is skipped;
    ///   2. the push type is enabled but its credentials/config are missing, or the
    ///      startup config read failed — this is a misconfiguration, and reporting it
    ///      as a successful delivery (the previous "fallback" behaviour) silently
    ///      dropped every push. Fail closed instead, so the failure lands in the
    ///      notification log and the queue's retry/backoff path.
    async fn provider_unavailable(&self, push_type: &str) -> Result<PushResult, ApiError> {
        let enabled = self.storage.get_config_as_bool(&format!("{push_type}.enabled"), false).await?;

        if !enabled {
            info!(provider = %push_type, provider_enabled = false, "Push provider disabled, skipping notification");
            return Ok(PushResult::success());
        }

        Err(ApiError::internal(format!(
            "{push_type} push provider is not initialized although {push_type}.enabled is true; \
             check the push_config credentials and the startup logs"
        )))
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

    /// See [`list_config`].
    pub async fn list_config(&self) -> Result<Vec<PushConfigEntry>, ApiError> {
        self.storage.list_config().await
    }

    /// See [`set_config`].
    pub async fn set_config(&self, config_key: &str, config_value: &str) -> Result<PushConfigEntry, ApiError> {
        self.storage.set_config(config_key, config_value).await
    }

    /// See [`delete_config`].
    pub async fn delete_config(&self, config_key: &str) -> Result<bool, ApiError> {
        self.storage.delete_config(config_key).await
    }
}

/// The `push_config` keys that [`PushNotificationService::initialize_providers`] reads.
///
/// This is the single source of truth for the admin config endpoint's allowlist:
/// accepting a key nothing consumes would turn `push_config` into a graveyard of
/// settings that appear applied but are not.
pub const SUPPORTED_PUSH_CONFIG_KEYS: &[&str] = &[
    "fcm.enabled",
    "fcm.api_key",
    "apns.enabled",
    "apns.topic",
    "webpush.enabled",
    "webpush.vapid_public_key",
    "webpush.vapid_private_key",
];

/// The subset of [`SUPPORTED_PUSH_CONFIG_KEYS`] whose value is a credential and
/// must be masked when read back through the admin API.
pub const SECRET_PUSH_CONFIG_KEYS: &[&str] = &["fcm.api_key", "webpush.vapid_private_key"];

#[cfg(test)]
mod db_tests {
    use super::*;
    use crate::push::providers::FcmProviderConfig;
    use crate::test_utils;
    use synapse_storage::push_notification::{PushNotificationStorage, RegisterDeviceRequest};
    use wiremock::matchers::{header, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn test_pool() -> Arc<sqlx::PgPool> {
        test_utils::prepare_isolated_test_pool().await.expect(
            "isolated test pool must be available - a swallowed error here surfaces later as an unrelated failure",
        )
    }

    fn storage_for(pool: &Arc<sqlx::PgPool>) -> Arc<dyn PushNotificationStoreApi> {
        Arc::new(PushNotificationStorage::new(pool))
    }

    /// Inserts a minimal `users` row. `push_device` has an FK to `users`, and
    /// `push_config.user_id` must satisfy the generated `ck_*_user_id_format`
    /// check, so both fixtures need a real-looking user.
    async fn ensure_user(pool: &sqlx::PgPool, user_id: &str) {
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(user_id.trim_start_matches('@').split(':').next().unwrap_or("push"))
        .bind(synapse_common::current_timestamp_millis())
        .execute(pool)
        .await
        .expect("seeding the users row must succeed");
    }

    /// Seeds one global `push_config` row through the storage API — the same path the
    /// admin config endpoint uses, so the fixtures cannot drift from the real schema.
    async fn set_push_config(storage: &Arc<dyn PushNotificationStoreApi>, key: &str, value: &str) {
        storage.set_config(key, value).await.expect("seeding push_config must succeed");
    }

    /// Reads back the persisted delivery log for a user. This is the evidence that
    /// `push_notification_log` accepts the row at all (it has a NOT NULL `created_ts`
    /// without a default).
    async fn log_provider_response(pool: &sqlx::PgPool, user_id: &str) -> Option<String> {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT provider_response FROM push_notification_log WHERE user_id = $1 ORDER BY id DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("the notification log row must exist")
    }

    async fn queue_row(pool: &sqlx::PgPool, user_id: &str) -> (String, Option<String>) {
        sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT status, error_message FROM push_notification_queue WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("the queued notification row must exist")
    }

    fn send_request(user_id: &str, device_id: &str) -> SendNotificationRequest {
        SendNotificationRequest {
            user_id: user_id.to_string(),
            device_id: Some(device_id.to_string()),
            event_id: Some("$push_event".to_string()),
            room_id: Some("!push_room:test".to_string()),
            notification_type: Some("m.room.message".to_string()),
            title: "hello".to_string(),
            body: "world".to_string(),
            data: None,
            priority: None,
        }
    }

    /// `initialize_providers` must actually build a provider for every push type
    /// that `push_config` enables — this is what the container wiring relies on.
    #[tokio::test]
    async fn initialize_providers_builds_every_enabled_provider() {
        let pool = test_pool().await;
        let storage = storage_for(&pool);

        set_push_config(&storage, "fcm.enabled", "true").await;
        set_push_config(&storage, "fcm.api_key", "test-fcm-key").await;
        set_push_config(&storage, "apns.enabled", "true").await;
        set_push_config(&storage, "apns.topic", "com.example.app").await;
        set_push_config(&storage, "webpush.enabled", "true").await;
        set_push_config(&storage, "webpush.vapid_public_key", "public").await;
        set_push_config(&storage, "webpush.vapid_private_key", "private").await;

        let service = PushNotificationService::new(storage);
        assert!(service.initialized_providers().is_empty(), "providers must start unset");

        service.initialize_providers().await.expect("initialize_providers must succeed");

        assert_eq!(service.initialized_providers(), vec!["fcm", "apns", "webpush"]);
    }

    /// Disabled push types must stay unset (and therefore be skipped at delivery
    /// time) instead of being constructed from a stale credential row.
    #[tokio::test]
    async fn initialize_providers_leaves_disabled_providers_unset() {
        let pool = test_pool().await;
        let storage = storage_for(&pool);

        set_push_config(&storage, "fcm.enabled", "false").await;
        set_push_config(&storage, "fcm.api_key", "stale-key").await;

        let service = PushNotificationService::new(storage);
        service.initialize_providers().await.expect("initialize_providers must succeed");

        assert!(service.initialized_providers().is_empty(), "a disabled provider must not be built");
    }

    /// The real delivery path: `process_pending_notifications` must reach the
    /// provider's HTTP call and record the notification as sent.
    #[tokio::test]
    async fn process_pending_notifications_delivers_through_the_provider() {
        let pool = test_pool().await;
        let storage = storage_for(&pool);

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("authorization", "key=test-fcm-key"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let user_id = format!("@push_{}:test.com", uuid::Uuid::new_v4());
        ensure_user(&pool, &user_id).await;
        let device_id = "PUSHDEVICE";
        storage
            .register_device(RegisterDeviceRequest {
                user_id: user_id.clone(),
                device_id: device_id.to_string(),
                push_token: "device-token".to_string(),
                push_type: "fcm".to_string(),
                app_id: Some("com.example.app".to_string()),
                platform: None,
                platform_version: None,
                app_version: None,
                locale: None,
                timezone: None,
                metadata: None,
            })
            .await
            .expect("register_device must succeed");

        let fcm = Arc::new(FcmProvider::new(FcmProviderConfig {
            api_key: "test-fcm-key".to_string(),
            endpoint: format!("{}/fcm/send", mock_server.uri()),
            ..Default::default()
        }));
        let service = PushNotificationService::new(storage).with_fcm_provider(fcm);

        service.send_notification(send_request(&user_id, device_id)).await.expect("queueing must succeed");

        let processed = service.process_pending_notifications(10).await.expect("processing must succeed");
        assert_eq!(processed, 1, "the notification must be delivered, not skipped");

        let requests = mock_server.received_requests().await.expect("wiremock must record requests");
        assert_eq!(requests.len(), 1, "the provider must be called exactly once");
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).expect("body must be JSON");
        assert_eq!(body["to"], "device-token");

        let (status, error) = queue_row(&pool, &user_id).await;
        assert_eq!(status, "sent", "the delivered notification must be marked sent: {error:?}");
        assert!(
            log_provider_response(&pool, &user_id).await.unwrap_or_default().contains("multicast_id"),
            "the provider response must be persisted in the delivery log"
        );
    }

    /// Fail closed: an enabled push type whose provider was never initialized must
    /// surface a failure. The previous "fallback" reported a fake success and
    /// marked notifications sent without sending anything.
    #[tokio::test]
    async fn enabled_but_uninitialized_provider_fails_instead_of_faking_success() {
        let pool = test_pool().await;
        let storage = storage_for(&pool);

        set_push_config(&storage, "fcm.enabled", "true").await;
        set_push_config(&storage, "fcm.api_key", "test-fcm-key").await;

        let user_id = format!("@push_{}:test.com", uuid::Uuid::new_v4());
        ensure_user(&pool, &user_id).await;
        storage
            .register_device(RegisterDeviceRequest {
                user_id: user_id.clone(),
                device_id: "PUSHDEVICE".to_string(),
                push_token: "device-token".to_string(),
                push_type: "fcm".to_string(),
                app_id: Some("com.example.app".to_string()),
                platform: None,
                platform_version: None,
                app_version: None,
                locale: None,
                timezone: None,
                metadata: None,
            })
            .await
            .expect("register_device must succeed");

        // Deliberately no `.with_fcm_provider(...)` and no `initialize_providers()`.
        let service = PushNotificationService::new(storage);
        service.send_notification(send_request(&user_id, "PUSHDEVICE")).await.expect("queueing must succeed");

        let processed = service.process_pending_notifications(10).await.expect("processing must not error out");
        assert_eq!(processed, 0, "an uninitialized provider must not report a delivery");

        let (status, error) = queue_row(&pool, &user_id).await;
        assert_ne!(status, "sent", "the notification must not be marked sent");
        assert!(
            error.unwrap_or_default().contains("not initialized"),
            "the misconfiguration must be recorded on the queue row"
        );
    }

    /// A disabled push type with a stale device registration is skipped rather
    /// than retried forever.
    #[tokio::test]
    async fn disabled_provider_skips_without_retrying() {
        let pool = test_pool().await;
        let storage = storage_for(&pool);

        set_push_config(&storage, "fcm.enabled", "false").await;

        let user_id = format!("@push_{}:test.com", uuid::Uuid::new_v4());
        ensure_user(&pool, &user_id).await;
        storage
            .register_device(RegisterDeviceRequest {
                user_id: user_id.clone(),
                device_id: "PUSHDEVICE".to_string(),
                push_token: "device-token".to_string(),
                push_type: "fcm".to_string(),
                app_id: Some("com.example.app".to_string()),
                platform: None,
                platform_version: None,
                app_version: None,
                locale: None,
                timezone: None,
                metadata: None,
            })
            .await
            .expect("register_device must succeed");

        let service = PushNotificationService::new(storage);
        service.send_notification(send_request(&user_id, "PUSHDEVICE")).await.expect("queueing must succeed");

        let processed = service.process_pending_notifications(10).await.expect("processing must succeed");
        assert_eq!(processed, 1, "a disabled provider is a skip, not a failure");
        assert_eq!(queue_row(&pool, &user_id).await.0, "sent");
    }

    /// Operators must be able to configure providers without hand-written SQL: the
    /// storage API is the only writer and round-trips keys, values and deletions.
    #[tokio::test]
    async fn push_config_round_trips_through_the_storage_api() {
        let pool = test_pool().await;
        let storage = storage_for(&pool);

        assert!(storage.list_config().await.expect("list must succeed").is_empty(), "config must start empty");

        storage.set_config("fcm.enabled", "true").await.expect("set must succeed");
        let entry = storage.set_config("fcm.api_key", "key-1").await.expect("set must succeed");
        assert_eq!(entry.config_key, "fcm.api_key");
        assert_eq!(entry.config_value, "key-1");
        assert!(entry.updated_ts.is_some(), "updated_ts must be stamped");

        // Second write updates in place rather than duplicating the key.
        storage.set_config("fcm.api_key", "key-2").await.expect("update must succeed");
        let listed = storage.list_config().await.expect("list must succeed");
        assert_eq!(listed.len(), 2, "the key must be upserted, not duplicated: {listed:?}");
        assert_eq!(storage.get_config("fcm.api_key").await.expect("get must succeed").as_deref(), Some("key-2"));

        assert!(storage.delete_config("fcm.api_key").await.expect("delete must succeed"));
        assert!(!storage.delete_config("fcm.api_key").await.expect("second delete must succeed"));
        assert_eq!(storage.get_config("fcm.api_key").await.expect("get must succeed"), None);
    }

    /// A config change must take effect without restarting the homeserver: the
    /// service is shared as an `Arc`, so providers are rebuilt in place.
    #[tokio::test]
    async fn config_changes_reload_providers_without_a_restart() {
        let pool = test_pool().await;
        let storage = storage_for(&pool);
        let service = PushNotificationService::new(storage.clone());

        service.initialize_providers().await.expect("initialization must succeed");
        assert!(service.initialized_providers().is_empty(), "nothing is configured yet");

        set_push_config(&storage, "fcm.enabled", "true").await;
        set_push_config(&storage, "fcm.api_key", "key-1").await;
        service.initialize_providers().await.expect("reload must succeed");
        assert_eq!(service.initialized_providers(), vec!["fcm"], "the change must apply in place");

        set_push_config(&storage, "fcm.enabled", "false").await;
        service.initialize_providers().await.expect("reload must succeed");
        assert!(service.initialized_providers().is_empty(), "disabling must drop the provider again");
    }

    /// A blank credential must not build a provider: `FcmProvider` treats an empty
    /// API key as disabled and would then report every send as successful.
    #[tokio::test]
    async fn blank_credentials_do_not_build_a_provider() {
        let pool = test_pool().await;
        let storage = storage_for(&pool);

        set_push_config(&storage, "fcm.enabled", "true").await;
        set_push_config(&storage, "fcm.api_key", "   ").await;

        let service = PushNotificationService::new(storage);
        service.initialize_providers().await.expect("initialization must succeed");
        assert!(
            service.initialized_providers().is_empty(),
            "a whitespace-only credential counts as unset, not as a working provider"
        );
    }
}
