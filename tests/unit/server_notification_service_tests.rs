// Server notification service unit tests — exercises
// `synapse_services::server_notification_service::ServerNotificationService`.
//
// `ServerNotificationService` is a thin wrapper over
// `ServerNotificationStoreApi` (storage) and `UserService` (user existence
// checks). Most methods delegate directly to storage; a few contain logic:
//
//   * `delete_server_notice` — fetches notice→room/event mapping, deletes
//     the notice, then cascades to room or event.
//   * `create_from_template` — loads a template, substitutes `{{var}}`
//     placeholders, and creates a notification.
//   * `process_scheduled_notifications` — iterates pending scheduled
//     notifications and marks each as sent.
//   * `broadcast_notification` — logs a delivery record.
//   * `ensure_target_users_exist` — delegates to `UserService::ensure_user_exists`
//     for each user_id.
//
// These tests verify:
//   * Delegation methods forward arguments and return values correctly.
//   * Error path: storage errors propagate as `ApiError`.
//   * `delete_server_notice` returns `not_found` when the notice doesn't
//     exist, cascades to `delete_room_cascade` when a room_id is present,
//     and cascades to `delete_event_by_id` when only an event_id is present.
//   * `create_from_template` substitutes variables and creates a notification;
//     returns `not_found` when the template is missing.
//   * `process_scheduled_notifications` processes all pending entries and
//     returns the count.
//   * `broadcast_notification` logs the delivery with the correct method.
//   * `ensure_target_users_exist` succeeds when all users exist, fails when
//     any user is missing.

#![cfg(feature = "server-notifications")]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use synapse_common::ApiError;
use synapse_services::server_notification_service::ServerNotificationService;
use synapse_services::user_service::UserService;
use synapse_storage::server_notification::*;
use synapse_storage::test_mocks::FakeUserStore;

// ─────────────────────────────────────────────────────────────────────────────
// Mock ServerNotificationStoreApi
// ─────────────────────────────────────────────────────────────────────────────

/// Delivery log record: `(notice_id, room_id, event_id, user_id, method)`.
type DeliveryLog = (i64, Option<String>, String, String, Option<String>);
/// notice_id → `(room_id, event_id)` mapping.
type NoticeRoomMapping = (Option<String>, Option<String>);

/// In-memory `ServerNotificationStoreApi` fake. Stores notifications,
/// templates, scheduled notifications, and notice→room/event mappings.
/// Tracks side-effect calls (deletes, marks, delivery logs) for verification.
#[derive(Default)]
struct MockServerNotificationStore {
    notifications: Mutex<HashMap<i64, ServerNotification>>,
    templates: Mutex<HashMap<String, NotificationTemplate>>,
    user_settings: Mutex<HashMap<String, bool>>,
    user_pushers: Mutex<HashMap<String, Vec<serde_json::Value>>>,
    scheduled: Mutex<Vec<ScheduledNotification>>,
    notice_rooms: Mutex<HashMap<i64, NoticeRoomMapping>>,
    server_notices: Mutex<HashMap<i64, serde_json::Value>>,

    // Call tracking
    delivery_logs: Mutex<Vec<DeliveryLog>>,
    deleted_rooms: Mutex<Vec<String>>,
    deleted_events: Mutex<Vec<String>>,
    deleted_notices: Mutex<Vec<i64>>,
    marked_sent: Mutex<Vec<i64>>,

    // Error injection
    fail_all: Mutex<bool>,

    // ID counter for new notifications
    next_id: Mutex<i64>,
}

impl MockServerNotificationStore {
    fn new() -> Self {
        let store = Self::default();
        *store.next_id.lock().unwrap() = 1;
        store
    }

    fn set_fail_all(&self, fail: bool) {
        *self.fail_all.lock().unwrap() = fail;
    }

    fn fail_check(&self) -> Result<(), ApiError> {
        if *self.fail_all.lock().unwrap() {
            Err(ApiError::internal("mock storage failure"))
        } else {
            Ok(())
        }
    }

    fn seed_notification(&self, notification: ServerNotification) {
        self.notifications.lock().unwrap().insert(notification.id, notification);
    }

    fn seed_template(&self, template: NotificationTemplate) {
        self.templates.lock().unwrap().insert(template.name.clone(), template);
    }

    fn seed_scheduled(&self, scheduled: ScheduledNotification) {
        self.scheduled.lock().unwrap().push(scheduled);
    }

    fn seed_notice_room(&self, notice_id: i64, event_id: Option<String>, room_id: Option<String>) {
        self.notice_rooms.lock().unwrap().insert(notice_id, (event_id, room_id));
    }

    fn delivery_log_count(&self) -> usize {
        self.delivery_logs.lock().unwrap().len()
    }

    fn last_delivery_log(&self) -> Option<DeliveryLog> {
        self.delivery_logs.lock().unwrap().last().cloned()
    }

    fn deleted_room_count(&self) -> usize {
        self.deleted_rooms.lock().unwrap().len()
    }

    fn deleted_event_count(&self) -> usize {
        self.deleted_events.lock().unwrap().len()
    }

    fn marked_sent_count(&self) -> usize {
        self.marked_sent.lock().unwrap().len()
    }

    fn alloc_id(&self) -> i64 {
        let mut id = self.next_id.lock().unwrap();
        let val = *id;
        *id += 1;
        val
    }
}

#[async_trait]
impl ServerNotificationStoreApi for MockServerNotificationStore {
    async fn create_notification(&self, request: CreateNotificationRequest) -> Result<ServerNotification, ApiError> {
        self.fail_check()?;
        let id = self.alloc_id();
        let now = 1_700_000_000_000;
        let notification = ServerNotification {
            id,
            title: request.title,
            content: request.content,
            notification_type: request.notification_type.unwrap_or_else(|| "info".to_string()),
            priority: request.priority.unwrap_or(0),
            target_audience: request.target_audience.unwrap_or_else(|| "all".to_string()),
            target_user_ids: request
                .target_user_ids
                .map_or(serde_json::Value::Null, |v| serde_json::to_value(&v).unwrap_or(serde_json::Value::Null)),
            starts_at: request.starts_at,
            expires_at: request.expires_at,
            is_enabled: true,
            is_dismissable: request.is_dismissable.unwrap_or(true),
            action_url: request.action_url,
            action_text: request.action_text,
            created_by: request.created_by,
            created_ts: now,
            updated_ts: now,
        };
        self.notifications.lock().unwrap().insert(id, notification.clone());
        Ok(notification)
    }

    async fn get_notification(&self, notification_id: i64) -> Result<Option<ServerNotification>, ApiError> {
        self.fail_check()?;
        Ok(self.notifications.lock().unwrap().get(&notification_id).cloned())
    }

    async fn list_active_notifications(&self) -> Result<Vec<ServerNotification>, ApiError> {
        self.fail_check()?;
        Ok(self.notifications.lock().unwrap().values().filter(|n| n.is_enabled).cloned().collect())
    }

    async fn list_all_notifications(
        &self,
        _audience: Option<&str>,
        _limit: i64,
        _from: Option<ServerNotificationCursor>,
    ) -> Result<(Vec<ServerNotification>, Option<String>), ApiError> {
        self.fail_check()?;
        Ok((self.notifications.lock().unwrap().values().cloned().collect(), None))
    }

    async fn update_notification(
        &self,
        notification_id: i64,
        request: CreateNotificationRequest,
    ) -> Result<ServerNotification, ApiError> {
        self.fail_check()?;
        let mut notifications = self.notifications.lock().unwrap();
        let notification =
            notifications.get_mut(&notification_id).ok_or_else(|| ApiError::not_found("Notification not found"))?;
        notification.title = request.title;
        notification.content = request.content;
        if let Some(t) = request.notification_type {
            notification.notification_type = t;
        }
        if let Some(p) = request.priority {
            notification.priority = p;
        }
        if let Some(a) = request.target_audience {
            notification.target_audience = a;
        }
        Ok(notification.clone())
    }

    async fn delete_notification(&self, notification_id: i64) -> Result<bool, ApiError> {
        self.fail_check()?;
        Ok(self.notifications.lock().unwrap().remove(&notification_id).is_some())
    }

    async fn deactivate_notification(&self, notification_id: i64) -> Result<bool, ApiError> {
        self.fail_check()?;
        let mut notifications = self.notifications.lock().unwrap();
        if let Some(n) = notifications.get_mut(&notification_id) {
            n.is_enabled = false;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn get_user_notifications(&self, _user_id: &str) -> Result<Vec<NotificationWithStatus>, ApiError> {
        self.fail_check()?;
        Ok(Vec::new())
    }

    async fn get_or_create_status(
        &self,
        _user_id: &str,
        _notification_id: i64,
    ) -> Result<UserNotificationStatus, ApiError> {
        self.fail_check()?;
        Ok(UserNotificationStatus {
            id: 0,
            user_id: String::new(),
            notification_id: 0,
            is_read: false,
            is_dismissed: false,
            read_ts: None,
            dismissed_ts: None,
            created_ts: 0,
        })
    }

    async fn get_or_create_statuses_batch(
        &self,
        _user_id: &str,
        _notification_ids: &[i64],
    ) -> Result<HashMap<i64, UserNotificationStatus>, ApiError> {
        self.fail_check()?;
        Ok(HashMap::new())
    }

    async fn mark_as_read(&self, _user_id: &str, _notification_id: i64) -> Result<bool, ApiError> {
        self.fail_check()?;
        Ok(true)
    }

    async fn mark_as_dismissed(&self, _user_id: &str, _notification_id: i64) -> Result<bool, ApiError> {
        self.fail_check()?;
        Ok(true)
    }

    async fn mark_all_as_read(&self, _user_id: &str) -> Result<i64, ApiError> {
        self.fail_check()?;
        Ok(0)
    }

    async fn create_template(&self, request: CreateTemplateRequest) -> Result<NotificationTemplate, ApiError> {
        self.fail_check()?;
        let id = self.alloc_id();
        let now = 1_700_000_000_000;
        let template = NotificationTemplate {
            id,
            name: request.name.clone(),
            title_template: request.title_template,
            content_template: request.content_template,
            notification_type: request.notification_type.unwrap_or_else(|| "info".to_string()),
            variables: request
                .variables
                .map_or(serde_json::Value::Null, |v| serde_json::to_value(&v).unwrap_or(serde_json::Value::Null)),
            is_enabled: true,
            created_ts: now,
            updated_ts: now,
        };
        self.templates.lock().unwrap().insert(request.name, template.clone());
        Ok(template)
    }

    async fn get_template(&self, name: &str) -> Result<Option<NotificationTemplate>, ApiError> {
        self.fail_check()?;
        Ok(self.templates.lock().unwrap().get(name).cloned())
    }

    async fn list_templates(&self) -> Result<Vec<NotificationTemplate>, ApiError> {
        self.fail_check()?;
        Ok(self.templates.lock().unwrap().values().cloned().collect())
    }

    async fn delete_template(&self, name: &str) -> Result<bool, ApiError> {
        self.fail_check()?;
        Ok(self.templates.lock().unwrap().remove(name).is_some())
    }

    async fn log_delivery(
        &self,
        notification_id: i64,
        user_id: Option<&str>,
        delivery_method: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), ApiError> {
        self.fail_check()?;
        self.delivery_logs.lock().unwrap().push((
            notification_id,
            user_id.map(|s| s.to_string()),
            delivery_method.to_string(),
            status.to_string(),
            error_message.map(|s| s.to_string()),
        ));
        Ok(())
    }

    async fn schedule_notification(
        &self,
        notification_id: i64,
        scheduled_for: i64,
    ) -> Result<ScheduledNotification, ApiError> {
        self.fail_check()?;
        let id = self.alloc_id();
        let scheduled = ScheduledNotification {
            id,
            notification_id,
            scheduled_for,
            is_sent: false,
            sent_ts: None,
            created_ts: 1_700_000_000_000,
        };
        self.scheduled.lock().unwrap().push(scheduled.clone());
        Ok(scheduled)
    }

    async fn get_pending_scheduled_notifications(&self) -> Result<Vec<ScheduledNotification>, ApiError> {
        self.fail_check()?;
        Ok(self.scheduled.lock().unwrap().iter().filter(|s| !s.is_sent).cloned().collect())
    }

    async fn mark_scheduled_sent(&self, scheduled_id: i64) -> Result<bool, ApiError> {
        self.fail_check()?;
        let mut scheduled = self.scheduled.lock().unwrap();
        for s in scheduled.iter_mut() {
            if s.id == scheduled_id && !s.is_sent {
                s.is_sent = true;
                s.sent_ts = Some(1_700_000_000_001);
                self.marked_sent.lock().unwrap().push(scheduled_id);
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn get_user_notification_setting(&self, user_id: &str) -> Result<Option<bool>, ApiError> {
        self.fail_check()?;
        Ok(self.user_settings.lock().unwrap().get(user_id).copied())
    }

    async fn upsert_user_notification_setting(&self, user_id: &str, enabled: bool) -> Result<(), ApiError> {
        self.fail_check()?;
        self.user_settings.lock().unwrap().insert(user_id.to_string(), enabled);
        Ok(())
    }

    async fn get_user_pushers(&self, user_id: &str) -> Result<Vec<serde_json::Value>, ApiError> {
        self.fail_check()?;
        Ok(self.user_pushers.lock().unwrap().get(user_id).cloned().unwrap_or_default())
    }

    async fn delete_user_pusher(&self, user_id: &str, pushkey: &str) -> Result<bool, ApiError> {
        self.fail_check()?;
        let mut pushers = self.user_pushers.lock().unwrap();
        if let Some(vec) = pushers.get_mut(user_id) {
            let before = vec.len();
            vec.retain(|p| p.get("pushkey").and_then(|v| v.as_str()) != Some(pushkey));
            Ok(vec.len() < before)
        } else {
            Ok(false)
        }
    }

    async fn get_server_notices_count(&self) -> Result<i64, ApiError> {
        self.fail_check()?;
        Ok(self.server_notices.lock().unwrap().len() as i64)
    }

    async fn get_server_notices_paginated(
        &self,
        _cursor: Option<(i64, i64)>,
        _limit: i64,
    ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), ApiError> {
        self.fail_check()?;
        Ok((Vec::new(), 0, None))
    }

    async fn get_server_notice_by_id(&self, notice_id: i64) -> Result<Option<serde_json::Value>, ApiError> {
        self.fail_check()?;
        Ok(self.server_notices.lock().unwrap().get(&notice_id).cloned())
    }

    async fn get_server_notice_with_room(&self, notice_id: i64) -> Result<Option<NoticeRoomMapping>, ApiError> {
        self.fail_check()?;
        Ok(self.notice_rooms.lock().unwrap().get(&notice_id).cloned())
    }

    async fn delete_server_notice_by_id(&self, notice_id: i64) -> Result<bool, ApiError> {
        self.fail_check()?;
        self.deleted_notices.lock().unwrap().push(notice_id);
        self.server_notices.lock().unwrap().remove(&notice_id);
        self.notice_rooms.lock().unwrap().remove(&notice_id);
        Ok(true)
    }

    async fn delete_room_cascade(&self, room_id: &str) -> Result<(), ApiError> {
        self.fail_check()?;
        self.deleted_rooms.lock().unwrap().push(room_id.to_string());
        Ok(())
    }

    async fn delete_event_by_id(&self, event_id: &str) -> Result<(), ApiError> {
        self.fail_check()?;
        self.deleted_events.lock().unwrap().push(event_id.to_string());
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn send_server_notice(
        &self,
        _room_id: &str,
        _server_user: &str,
        _target_user_id: &str,
        _target_displayname: &Option<String>,
        _target_avatar_url: &Option<String>,
        _message_event_id: &str,
        _create_event_id: &str,
        _membership_event_id: &str,
        _msgtype: &str,
        _body: &str,
        _now: i64,
    ) -> Result<i64, ApiError> {
        self.fail_check()?;
        Ok(self.alloc_id())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn build_service() -> ServerNotificationService {
    let store = Arc::new(MockServerNotificationStore::new());
    let user_store: Arc<FakeUserStore> = Arc::new(FakeUserStore::new());
    let user_service = Arc::new(UserService::new(user_store as Arc<dyn synapse_storage::user::UserStore>));
    ServerNotificationService::new(store, user_service)
}

fn build_service_with_store(
    store: MockServerNotificationStore,
) -> (ServerNotificationService, Arc<MockServerNotificationStore>) {
    let store_arc = Arc::new(store);
    let user_store: Arc<FakeUserStore> = Arc::new(FakeUserStore::new());
    let user_service = Arc::new(UserService::new(user_store as Arc<dyn synapse_storage::user::UserStore>));
    let svc = ServerNotificationService::new(store_arc.clone(), user_service);
    (svc, store_arc)
}

fn make_notification(id: i64, title: &str) -> ServerNotification {
    ServerNotification {
        id,
        title: title.to_string(),
        content: "test content".to_string(),
        notification_type: "info".to_string(),
        priority: 0,
        target_audience: "all".to_string(),
        target_user_ids: serde_json::Value::Null,
        starts_at: None,
        expires_at: None,
        is_enabled: true,
        is_dismissable: true,
        action_url: None,
        action_text: None,
        created_by: None,
        created_ts: 1_700_000_000_000,
        updated_ts: 1_700_000_000_000,
    }
}

fn make_create_request(title: &str, content: &str) -> CreateNotificationRequest {
    CreateNotificationRequest {
        title: title.to_string(),
        content: content.to_string(),
        notification_type: Some("info".to_string()),
        priority: None,
        target_audience: None,
        target_user_ids: None,
        starts_at: None,
        expires_at: None,
        is_dismissable: None,
        action_url: None,
        action_text: None,
        created_by: None,
    }
}

fn make_template(name: &str, title_template: &str, content_template: &str) -> NotificationTemplate {
    NotificationTemplate {
        id: 1,
        name: name.to_string(),
        title_template: title_template.to_string(),
        content_template: content_template.to_string(),
        notification_type: "info".to_string(),
        variables: serde_json::Value::Null,
        is_enabled: true,
        created_ts: 1_700_000_000_000,
        updated_ts: 1_700_000_000_000,
    }
}

fn make_scheduled(id: i64, notification_id: i64) -> ScheduledNotification {
    ScheduledNotification {
        id,
        notification_id,
        scheduled_for: 1_700_000_000_000,
        is_sent: false,
        sent_ts: None,
        created_ts: 1_700_000_000_000,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Construction
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn server_notification_service_constructs() {
    let _svc = build_service();
}

// ─────────────────────────────────────────────────────────────────────────────
// create_notification — delegation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_notification_returns_notification_with_id() {
    let svc = build_service();
    let request = make_create_request("Test Title", "Test Content");
    let result = svc.create_notification(request).await.expect("should succeed");
    assert_eq!(result.title, "Test Title");
    assert_eq!(result.content, "Test Content");
    assert!(result.id > 0);
}

#[tokio::test]
async fn create_notification_propagates_storage_error() {
    let store = MockServerNotificationStore::new();
    store.set_fail_all(true);
    let (svc, _) = build_service_with_store(store);
    let err = svc.create_notification(make_create_request("x", "y")).await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// get_notification — delegation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_notification_returns_some_when_seeded() {
    let store = MockServerNotificationStore::new();
    store.seed_notification(make_notification(42, "Seeded"));
    let (svc, _) = build_service_with_store(store);
    let result = svc.get_notification(42).await.expect("should succeed");
    assert!(result.is_some());
    assert_eq!(result.unwrap().title, "Seeded");
}

#[tokio::test]
async fn get_notification_returns_none_when_missing() {
    let svc = build_service();
    let result = svc.get_notification(999).await.expect("should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn get_notification_propagates_storage_error() {
    let store = MockServerNotificationStore::new();
    store.set_fail_all(true);
    let (svc, _) = build_service_with_store(store);
    let err = svc.get_notification(1).await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// list_active_notifications — delegation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_active_notifications_returns_enabled_notifications() {
    let store = MockServerNotificationStore::new();
    store.seed_notification(make_notification(1, "Active"));
    let (svc, _) = build_service_with_store(store);
    let result = svc.list_active_notifications().await.expect("should succeed");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].title, "Active");
}

#[tokio::test]
async fn list_active_notifications_returns_empty_when_none_seeded() {
    let svc = build_service();
    let result = svc.list_active_notifications().await.expect("should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn list_active_notifications_propagates_storage_error() {
    let store = MockServerNotificationStore::new();
    store.set_fail_all(true);
    let (svc, _) = build_service_with_store(store);
    let err = svc.list_active_notifications().await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// get_user_notification_setting / upsert — delegation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_user_notification_setting_returns_none_by_default() {
    let svc = build_service();
    let result = svc.get_user_notification_setting("@alice:example.com").await.expect("should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn upsert_user_notification_setting_succeeds() {
    let svc = build_service();
    svc.upsert_user_notification_setting("@alice:example.com", true).await.expect("should succeed");
}

#[tokio::test]
async fn upsert_then_get_returns_expected_value() {
    let store = MockServerNotificationStore::new();
    let (svc, _) = build_service_with_store(store);
    svc.upsert_user_notification_setting("@alice:example.com", false).await.expect("should succeed");
    let result = svc.get_user_notification_setting("@alice:example.com").await.expect("should succeed");
    assert_eq!(result, Some(false));
}

#[tokio::test]
async fn upsert_user_notification_setting_propagates_error() {
    let store = MockServerNotificationStore::new();
    store.set_fail_all(true);
    let (svc, _) = build_service_with_store(store);
    let err = svc.upsert_user_notification_setting("@alice:example.com", true).await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// get_user_pushers / delete_user_pusher — delegation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_user_pushers_returns_empty_by_default() {
    let svc = build_service();
    let result = svc.get_user_pushers("@alice:example.com").await.expect("should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn delete_user_pusher_returns_false_when_no_pushers() {
    let svc = build_service();
    let result = svc.delete_user_pusher("@alice:example.com", "pushkey1").await.expect("should succeed");
    assert!(!result);
}

#[tokio::test]
async fn get_user_pushers_propagates_error() {
    let store = MockServerNotificationStore::new();
    store.set_fail_all(true);
    let (svc, _) = build_service_with_store(store);
    let err = svc.get_user_pushers("@alice:example.com").await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// list_all_notifications — delegation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_all_notifications_returns_seeded_notifications() {
    let store = MockServerNotificationStore::new();
    store.seed_notification(make_notification(1, "First"));
    store.seed_notification(make_notification(2, "Second"));
    let (svc, _) = build_service_with_store(store);
    let (notifications, next_cursor) = svc.list_all_notifications(None, 10, None).await.expect("should succeed");
    assert_eq!(notifications.len(), 2);
    assert!(next_cursor.is_none());
}

#[tokio::test]
async fn list_all_notifications_propagates_error() {
    let store = MockServerNotificationStore::new();
    store.set_fail_all(true);
    let (svc, _) = build_service_with_store(store);
    let err = svc.list_all_notifications(None, 10, None).await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// update_notification — delegation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn update_notification_modifies_existing() {
    let store = MockServerNotificationStore::new();
    store.seed_notification(make_notification(1, "Old Title"));
    let (svc, _) = build_service_with_store(store);
    let request = make_create_request("New Title", "New Content");
    let result = svc.update_notification(1, request).await.expect("should succeed");
    assert_eq!(result.title, "New Title");
}

#[tokio::test]
async fn update_notification_returns_error_for_missing() {
    let svc = build_service();
    let err = svc.update_notification(999, make_create_request("x", "y")).await.expect_err("should error");
    assert!(err.is_not_found());
}

// ─────────────────────────────────────────────────────────────────────────────
// delete_notification — delegation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_notification_returns_true_when_exists() {
    let store = MockServerNotificationStore::new();
    store.seed_notification(make_notification(1, "To Delete"));
    let (svc, _) = build_service_with_store(store);
    let result = svc.delete_notification(1).await.expect("should succeed");
    assert!(result);
}

#[tokio::test]
async fn delete_notification_returns_false_when_missing() {
    let svc = build_service();
    let result = svc.delete_notification(999).await.expect("should succeed");
    assert!(!result);
}

// ─────────────────────────────────────────────────────────────────────────────
// deactivate_notification — delegation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn deactivate_notification_returns_true_when_exists() {
    let store = MockServerNotificationStore::new();
    store.seed_notification(make_notification(1, "Active"));
    let (svc, _) = build_service_with_store(store);
    let result = svc.deactivate_notification(1).await.expect("should succeed");
    assert!(result);
}

#[tokio::test]
async fn deactivate_notification_returns_false_when_missing() {
    let svc = build_service();
    let result = svc.deactivate_notification(999).await.expect("should succeed");
    assert!(!result);
}

// ─────────────────────────────────────────────────────────────────────────────
// get_server_notices_paginated / get_server_notice_by_id — delegation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_server_notices_paginated_returns_empty_by_default() {
    let svc = build_service();
    let (notices, count, next) = svc.get_server_notices_paginated(None, 10).await.expect("should succeed");
    assert!(notices.is_empty());
    assert_eq!(count, 0);
    assert!(next.is_none());
}

#[tokio::test]
async fn get_server_notice_by_id_returns_none_when_missing() {
    let svc = build_service();
    let result = svc.get_server_notice_by_id(999).await.expect("should succeed");
    assert!(result.is_none());
}

// ─────────────────────────────────────────────────────────────────────────────
// delete_server_notice — logic
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_server_notice_returns_not_found_when_notice_missing() {
    let svc = build_service();
    let err = svc.delete_server_notice(999).await.expect_err("should be not_found");
    assert!(err.is_not_found(), "missing notice must surface as not_found");
}

#[tokio::test]
async fn delete_server_notice_cascades_to_room_when_room_id_present() {
    let store = MockServerNotificationStore::new();
    store.seed_notice_room(1, Some("$event:example.com".to_string()), Some("!room:example.com".to_string()));
    let (svc, store_arc) = build_service_with_store(store);

    svc.delete_server_notice(1).await.expect("should succeed");

    assert_eq!(store_arc.deleted_room_count(), 1, "delete_room_cascade must be called");
    assert_eq!(store_arc.deleted_event_count(), 0, "delete_event_by_id must NOT be called when room exists");
}

#[tokio::test]
async fn delete_server_notice_cascades_to_event_when_only_event_id_present() {
    let store = MockServerNotificationStore::new();
    store.seed_notice_room(1, Some("$event:example.com".to_string()), None);
    let (svc, store_arc) = build_service_with_store(store);

    svc.delete_server_notice(1).await.expect("should succeed");

    assert_eq!(store_arc.deleted_room_count(), 0, "delete_room_cascade must NOT be called when no room");
    assert_eq!(store_arc.deleted_event_count(), 1, "delete_event_by_id must be called");
}

#[tokio::test]
async fn delete_server_notice_succeeds_when_both_event_and_room_are_none() {
    let store = MockServerNotificationStore::new();
    store.seed_notice_room(1, None, None);
    let (svc, store_arc) = build_service_with_store(store);

    svc.delete_server_notice(1).await.expect("should succeed");

    assert_eq!(store_arc.deleted_room_count(), 0);
    assert_eq!(store_arc.deleted_event_count(), 0);
}

#[tokio::test]
async fn delete_server_notice_propagates_delete_error() {
    let store = MockServerNotificationStore::new();
    store.seed_notice_room(1, Some("$event:example.com".to_string()), Some("!room:example.com".to_string()));
    store.set_fail_all(true);
    let (svc, _) = build_service_with_store(store);
    let err = svc.delete_server_notice(1).await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// get_user_notifications / mark_as_read / mark_as_dismissed / mark_all_as_read
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_user_notifications_returns_empty_by_default() {
    let svc = build_service();
    let result = svc.get_user_notifications("@alice:example.com").await.expect("should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn mark_as_read_returns_true() {
    let svc = build_service();
    let result = svc.mark_as_read("@alice:example.com", 1).await.expect("should succeed");
    assert!(result);
}

#[tokio::test]
async fn mark_as_dismissed_returns_true() {
    let svc = build_service();
    let result = svc.mark_as_dismissed("@alice:example.com", 1).await.expect("should succeed");
    assert!(result);
}

#[tokio::test]
async fn mark_all_as_read_returns_zero_by_default() {
    let svc = build_service();
    let result = svc.mark_all_as_read("@alice:example.com").await.expect("should succeed");
    assert_eq!(result, 0);
}

#[tokio::test]
async fn mark_as_read_propagates_error() {
    let store = MockServerNotificationStore::new();
    store.set_fail_all(true);
    let (svc, _) = build_service_with_store(store);
    let err = svc.mark_as_read("@alice:example.com", 1).await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// create_template / get_template / list_templates / delete_template — delegation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_template_returns_template_with_id() {
    let svc = build_service();
    let request = CreateTemplateRequest {
        name: "welcome".to_string(),
        title_template: "Welcome {{name}}".to_string(),
        content_template: "Hello {{name}}!".to_string(),
        notification_type: Some("info".to_string()),
        variables: Some(vec!["name".to_string()]),
    };
    let result = svc.create_template(request).await.expect("should succeed");
    assert_eq!(result.name, "welcome");
    assert!(result.id > 0);
}

#[tokio::test]
async fn get_template_returns_some_after_create() {
    let store = MockServerNotificationStore::new();
    store.seed_template(make_template("welcome", "Welcome {{name}}", "Hello {{name}}!"));
    let (svc, _) = build_service_with_store(store);
    let result = svc.get_template("welcome").await.expect("should succeed");
    assert!(result.is_some());
    assert_eq!(result.unwrap().name, "welcome");
}

#[tokio::test]
async fn get_template_returns_none_when_missing() {
    let svc = build_service();
    let result = svc.get_template("nonexistent").await.expect("should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn list_templates_returns_seeded_templates() {
    let store = MockServerNotificationStore::new();
    store.seed_template(make_template("t1", "Title 1", "Content 1"));
    store.seed_template(make_template("t2", "Title 2", "Content 2"));
    let (svc, _) = build_service_with_store(store);
    let result = svc.list_templates().await.expect("should succeed");
    assert_eq!(result.len(), 2);
}

#[tokio::test]
async fn delete_template_returns_true_when_exists() {
    let store = MockServerNotificationStore::new();
    store.seed_template(make_template("welcome", "t", "c"));
    let (svc, _) = build_service_with_store(store);
    let result = svc.delete_template("welcome").await.expect("should succeed");
    assert!(result);
}

#[tokio::test]
async fn delete_template_returns_false_when_missing() {
    let svc = build_service();
    let result = svc.delete_template("nonexistent").await.expect("should succeed");
    assert!(!result);
}

// ─────────────────────────────────────────────────────────────────────────────
// create_from_template — logic
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_from_template_substitutes_variables_and_creates_notification() {
    let store = MockServerNotificationStore::new();
    store.seed_template(make_template("welcome", "Welcome {{name}}", "Hello {{name}}, welcome to {{place}}!"));
    let (svc, _) = build_service_with_store(store);

    let mut variables = HashMap::new();
    variables.insert("name".to_string(), "Alice".to_string());
    variables.insert("place".to_string(), "Wonderland".to_string());

    let result =
        svc.create_from_template("welcome", variables, Some("all".to_string()), None).await.expect("should succeed");

    assert_eq!(result.title, "Welcome Alice");
    assert_eq!(result.content, "Hello Alice, welcome to Wonderland!");
}

#[tokio::test]
async fn create_from_template_returns_not_found_when_template_missing() {
    let svc = build_service();
    let err =
        svc.create_from_template("nonexistent", HashMap::new(), None, None).await.expect_err("should be not_found");
    assert!(err.is_not_found());
}

#[tokio::test]
async fn create_from_template_leaves_unmatched_placeholders_untouched() {
    let store = MockServerNotificationStore::new();
    store.seed_template(make_template("welcome", "Hello {{name}}", "Welcome {{name}} to {{place}}"));
    let (svc, _) = build_service_with_store(store);

    let mut variables = HashMap::new();
    variables.insert("name".to_string(), "Bob".to_string());
    // "place" is not provided — the placeholder should remain as-is.

    let result = svc.create_from_template("welcome", variables, None, None).await.expect("should succeed");

    assert_eq!(result.title, "Hello Bob");
    assert_eq!(result.content, "Welcome Bob to {{place}}");
}

// ─────────────────────────────────────────────────────────────────────────────
// schedule_notification — delegation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn schedule_notification_returns_scheduled_notification() {
    let svc = build_service();
    let result = svc.schedule_notification(1, 1_700_000_001_000).await.expect("should succeed");
    assert_eq!(result.notification_id, 1);
    assert_eq!(result.scheduled_for, 1_700_000_001_000);
    assert!(!result.is_sent);
}

#[tokio::test]
async fn schedule_notification_propagates_error() {
    let store = MockServerNotificationStore::new();
    store.set_fail_all(true);
    let (svc, _) = build_service_with_store(store);
    let err = svc.schedule_notification(1, 100).await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// process_scheduled_notifications — logic
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn process_scheduled_notifications_returns_zero_when_none_pending() {
    let svc = build_service();
    let result = svc.process_scheduled_notifications().await.expect("should succeed");
    assert_eq!(result, 0);
}

#[tokio::test]
async fn process_scheduled_notifications_marks_all_pending_as_sent() {
    let store = MockServerNotificationStore::new();
    // The notification must exist for `get_notification` to return Some.
    store.seed_notification(make_notification(10, "Notification 10"));
    store.seed_notification(make_notification(20, "Notification 20"));
    store.seed_scheduled(make_scheduled(1, 10));
    store.seed_scheduled(make_scheduled(2, 20));
    let (svc, store_arc) = build_service_with_store(store);

    let processed = svc.process_scheduled_notifications().await.expect("should succeed");
    assert_eq!(processed, 2, "both pending scheduled notifications should be processed");
    assert_eq!(store_arc.marked_sent_count(), 2, "mark_scheduled_sent should be called twice");
}

#[tokio::test]
async fn process_scheduled_notifications_skips_when_notification_missing() {
    let store = MockServerNotificationStore::new();
    // Don't seed the notification — get_notification returns None.
    store.seed_scheduled(make_scheduled(1, 999));
    let (svc, store_arc) = build_service_with_store(store);

    let processed = svc.process_scheduled_notifications().await.expect("should succeed");
    assert_eq!(processed, 0, "should skip when notification doesn't exist");
    assert_eq!(store_arc.marked_sent_count(), 0, "mark_scheduled_sent should NOT be called");
}

#[tokio::test]
async fn process_scheduled_notifications_propagates_error() {
    let store = MockServerNotificationStore::new();
    store.set_fail_all(true);
    let (svc, _) = build_service_with_store(store);
    let err = svc.process_scheduled_notifications().await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// broadcast_notification — logic
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn broadcast_notification_logs_delivery() {
    let store = MockServerNotificationStore::new();
    let (svc, store_arc) = build_service_with_store(store);

    svc.broadcast_notification(42, "push").await.expect("should succeed");

    assert_eq!(store_arc.delivery_log_count(), 1, "one delivery log should be recorded");
    let log = store_arc.last_delivery_log().expect("log should exist");
    assert_eq!(log.0, 42, "notification_id must match");
    assert_eq!(log.2, "push", "delivery_method must match");
    assert_eq!(log.3, "broadcast", "status must be 'broadcast'");
}

#[tokio::test]
async fn broadcast_notification_propagates_error() {
    let store = MockServerNotificationStore::new();
    store.set_fail_all(true);
    let (svc, _) = build_service_with_store(store);
    let err = svc.broadcast_notification(42, "push").await.expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// send_server_notice — delegation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn send_server_notice_returns_notice_id() {
    let svc = build_service();
    let result = svc
        .send_server_notice(
            "!room:example.com",
            "@server:example.com",
            "@alice:example.com",
            &Some("Alice".to_string()),
            &Some("mxc://example.com/a".to_string()),
            "$msg:example.com",
            "$create:example.com",
            "$member:example.com",
            "m.text",
            "Hello",
            1_700_000_000_000,
        )
        .await
        .expect("should succeed");
    assert!(result > 0);
}

#[tokio::test]
async fn send_server_notice_propagates_error() {
    let store = MockServerNotificationStore::new();
    store.set_fail_all(true);
    let (svc, _) = build_service_with_store(store);
    let err = svc
        .send_server_notice(
            "!room:example.com",
            "@server:example.com",
            "@alice:example.com",
            &None,
            &None,
            "$msg:example.com",
            "$create:example.com",
            "$member:example.com",
            "m.text",
            "Hello",
            1_700_000_000_000,
        )
        .await
        .expect_err("should error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// ensure_target_users_exist — logic (delegates to UserService)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn ensure_target_users_exist_succeeds_when_all_users_exist() {
    // FakeUserStore seeds @alice:example.com by default.
    let svc = build_service();
    svc.ensure_target_users_exist(&["@alice:example.com".to_string()])
        .await
        .expect("should succeed — alice exists in FakeUserStore");
}

#[tokio::test]
async fn ensure_target_users_exist_returns_not_found_when_user_missing() {
    let svc = build_service();
    let err =
        svc.ensure_target_users_exist(&["@nobody:example.com".to_string()]).await.expect_err("should be not_found");
    assert!(err.is_not_found());
}

#[tokio::test]
async fn ensure_target_users_exist_succeeds_with_empty_list() {
    let svc = build_service();
    svc.ensure_target_users_exist(&[]).await.expect("empty list should succeed");
}

#[tokio::test]
async fn ensure_target_users_exist_fails_on_first_missing_user() {
    let svc = build_service();
    // @alice exists, @nobody doesn't. The first missing user should cause
    // an error before the rest are checked.
    let err = svc
        .ensure_target_users_exist(&[
            "@alice:example.com".to_string(),
            "@nobody:example.com".to_string(),
            "@bob:example.com".to_string(),
        ])
        .await
        .expect_err("should be not_found");
    assert!(err.is_not_found());
}
