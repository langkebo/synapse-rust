use crate::UserService;
use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::server_notification::*;
use tracing::{info, instrument};

/// The `ServerNotificationService` struct.
pub struct ServerNotificationService {
    storage: Arc<dyn ServerNotificationStoreApi>,
    user_service: Arc<UserService>,
}

impl ServerNotificationService {
    /// See [`new`].
    pub fn new(storage: Arc<dyn ServerNotificationStoreApi>, user_service: Arc<UserService>) -> Self {
        Self { storage, user_service }
    }

    /// See [`ensure_target_users_exist`].
    #[instrument(skip(self))]
    pub async fn ensure_target_users_exist(&self, user_ids: &[String]) -> Result<(), ApiError> {
        for user_id in user_ids {
            self.user_service.ensure_user_exists(user_id).await?;
        }

        Ok(())
    }

    /// See [`create_notification`].
    #[instrument(skip(self))]
    pub async fn create_notification(
        &self,
        request: CreateNotificationRequest,
    ) -> Result<ServerNotification, ApiError> {
        info!(
            title_present = !request.title.is_empty(),
            title_len = request.title.len(),
            notification_type = ?request.notification_type,
            target_audience = ?request.target_audience,
            target_user_count = request.target_user_ids.as_ref().map(std::vec::Vec::len),
            created_by = ?request.created_by,
            "Creating notification"
        );
        self.storage.create_notification(request).await
    }

    /// See [`get_notification`].
    #[instrument(skip(self))]
    pub async fn get_notification(&self, notification_id: i64) -> Result<Option<ServerNotification>, ApiError> {
        self.storage.get_notification(notification_id).await
    }

    /// See [`list_active_notifications`].
    #[instrument(skip(self))]
    pub async fn list_active_notifications(&self) -> Result<Vec<ServerNotification>, ApiError> {
        self.storage.list_active_notifications().await
    }

    /// See [`get_user_notification_setting`].
    #[instrument(skip(self))]
    pub async fn get_user_notification_setting(&self, user_id: &str) -> Result<Option<bool>, ApiError> {
        self.storage.get_user_notification_setting(user_id).await
    }

    /// See [`upsert_user_notification_setting`].
    #[instrument(skip(self))]
    pub async fn upsert_user_notification_setting(&self, user_id: &str, enabled: bool) -> Result<(), ApiError> {
        self.storage.upsert_user_notification_setting(user_id, enabled).await
    }

    /// See [`get_user_pushers`].
    #[instrument(skip(self))]
    pub async fn get_user_pushers(&self, user_id: &str) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage.get_user_pushers(user_id).await
    }

    /// See [`delete_user_pusher`].
    #[instrument(skip(self))]
    pub async fn delete_user_pusher(&self, user_id: &str, pushkey: &str) -> Result<bool, ApiError> {
        self.storage.delete_user_pusher(user_id, pushkey).await
    }

    /// See [`list_all_notifications`].
    #[instrument(skip(self))]
    pub async fn list_all_notifications(
        &self,
        audience: Option<&str>,
        limit: i64,
        from: Option<ServerNotificationCursor>,
    ) -> Result<(Vec<ServerNotification>, Option<String>), ApiError> {
        self.storage.list_all_notifications(audience, limit, from).await
    }

    /// See [`update_notification`].
    #[instrument(skip(self))]
    pub async fn update_notification(
        &self,
        notification_id: i64,
        request: CreateNotificationRequest,
    ) -> Result<ServerNotification, ApiError> {
        info!(
            notification_id,
            title_present = !request.title.is_empty(),
            title_len = request.title.len(),
            notification_type = ?request.notification_type,
            target_audience = ?request.target_audience,
            target_user_count = request.target_user_ids.as_ref().map(std::vec::Vec::len),
            created_by = ?request.created_by,
            "Updating notification"
        );
        self.storage.update_notification(notification_id, request).await
    }

    /// See [`delete_notification`].
    #[instrument(skip(self))]
    pub async fn delete_notification(&self, notification_id: i64) -> Result<bool, ApiError> {
        info!(notification_id, "Deleting notification");
        self.storage.delete_notification(notification_id).await
    }

    /// See [`deactivate_notification`].
    #[instrument(skip(self))]
    pub async fn deactivate_notification(&self, notification_id: i64) -> Result<bool, ApiError> {
        info!(notification_id, "Deactivating notification");
        self.storage.deactivate_notification(notification_id).await
    }

    /// See [`get_server_notices_paginated`].
    #[instrument(skip(self))]
    #[allow(clippy::type_complexity)]
    pub async fn get_server_notices_paginated(
        &self,
        cursor: Option<(i64, i64)>,
        limit: i64,
    ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), ApiError> {
        self.storage.get_server_notices_paginated(cursor, limit).await
    }

    /// See [`get_server_notice_by_id`].
    #[instrument(skip(self))]
    pub async fn get_server_notice_by_id(&self, notice_id: i64) -> Result<Option<serde_json::Value>, ApiError> {
        self.storage.get_server_notice_by_id(notice_id).await
    }

    /// See [`delete_server_notice`].
    #[instrument(skip(self))]
    pub async fn delete_server_notice(&self, notice_id: i64) -> Result<(), ApiError> {
        let notice_info = self.storage.get_server_notice_with_room(notice_id).await?;

        let Some((event_id, room_id)) = notice_info else {
            return Err(ApiError::not_found("Server notice not found".to_string()));
        };

        self.storage.delete_server_notice_by_id(notice_id).await?;

        if let Some(room_id) = room_id {
            self.storage.delete_room_cascade(&room_id).await?;
        } else if let Some(event_id) = event_id {
            self.storage.delete_event_by_id(&event_id).await?;
        }

        Ok(())
    }

    /// See [`get_user_notifications`].
    #[instrument(skip(self))]
    pub async fn get_user_notifications(&self, user_id: &str) -> Result<Vec<NotificationWithStatus>, ApiError> {
        self.storage.get_user_notifications(user_id).await
    }

    /// See [`mark_as_read`].
    #[instrument(skip(self))]
    pub async fn mark_as_read(&self, user_id: &str, notification_id: i64) -> Result<bool, ApiError> {
        info!(notification_id, user_id = %user_id, "Marking notification as read");
        self.storage.mark_as_read(user_id, notification_id).await
    }

    /// See [`mark_as_dismissed`].
    #[instrument(skip(self))]
    pub async fn mark_as_dismissed(&self, user_id: &str, notification_id: i64) -> Result<bool, ApiError> {
        info!(notification_id, user_id = %user_id, "Dismissing notification");
        self.storage.mark_as_dismissed(user_id, notification_id).await
    }

    /// See [`mark_all_as_read`].
    #[instrument(skip(self))]
    pub async fn mark_all_as_read(&self, user_id: &str) -> Result<i64, ApiError> {
        info!(user_id = %user_id, "Marking all notifications as read");
        self.storage.mark_all_as_read(user_id).await
    }

    /// See [`create_template`].
    #[instrument(skip(self))]
    pub async fn create_template(&self, request: CreateTemplateRequest) -> Result<NotificationTemplate, ApiError> {
        info!(
            template_name = %request.name,
            notification_type = ?request.notification_type,
            variable_count = request.variables.as_ref().map(std::vec::Vec::len),
            "Creating notification template"
        );
        self.storage.create_template(request).await
    }

    /// See [`get_template`].
    #[instrument(skip(self))]
    pub async fn get_template(&self, name: &str) -> Result<Option<NotificationTemplate>, ApiError> {
        self.storage.get_template(name).await
    }

    /// See [`list_templates`].
    #[instrument(skip(self))]
    pub async fn list_templates(&self) -> Result<Vec<NotificationTemplate>, ApiError> {
        self.storage.list_templates().await
    }

    /// See [`delete_template`].
    #[instrument(skip(self))]
    pub async fn delete_template(&self, name: &str) -> Result<bool, ApiError> {
        info!(template_name = %name, "Deleting notification template");
        self.storage.delete_template(name).await
    }

    /// See [`create_from_template`].
    #[instrument(skip(self))]
    pub async fn create_from_template(
        &self,
        template_name: &str,
        variables: std::collections::HashMap<String, String>,
        target_audience: Option<String>,
        target_user_ids: Option<Vec<String>>,
    ) -> Result<ServerNotification, ApiError> {
        let template =
            self.storage.get_template(template_name).await?.ok_or_else(|| ApiError::not_found("Template not found"))?;

        let mut title = template.title_template.clone();
        let mut content = template.content_template.clone();

        for (key, value) in variables {
            let placeholder = format!("{{{{{}}}}}", key);
            title = title.replace(&placeholder, &value);
            content = content.replace(&placeholder, &value);
        }

        let request = CreateNotificationRequest {
            title,
            content,
            notification_type: Some(template.notification_type),
            priority: None,
            target_audience,
            target_user_ids,
            starts_at: None,
            expires_at: None,
            is_dismissable: None,
            action_url: None,
            action_text: None,
            created_by: None,
        };

        self.storage.create_notification(request).await
    }

    /// See [`schedule_notification`].
    #[instrument(skip(self))]
    pub async fn schedule_notification(
        &self,
        notification_id: i64,
        scheduled_for: i64,
    ) -> Result<ScheduledNotification, ApiError> {
        info!(notification_id, scheduled_for, "Scheduling notification");
        self.storage.schedule_notification(notification_id, scheduled_for).await
    }

    /// See [`process_scheduled_notifications`].
    #[instrument(skip(self))]
    pub async fn process_scheduled_notifications(&self) -> Result<i64, ApiError> {
        let pending = self.storage.get_pending_scheduled_notifications().await?;
        let mut processed = 0i64;

        for scheduled in pending {
            if let Some(_notification) = self.storage.get_notification(scheduled.notification_id).await? {
                self.storage.mark_scheduled_sent(scheduled.id).await?;
                processed += 1;
            }
        }

        Ok(processed)
    }

    /// See [`broadcast_notification`].
    #[instrument(skip(self))]
    pub async fn broadcast_notification(&self, notification_id: i64, delivery_method: &str) -> Result<(), ApiError> {
        info!(notification_id, delivery_method = %delivery_method, "Broadcasting notification");

        self.storage.log_delivery(notification_id, None, delivery_method, "broadcast", None).await?;

        Ok(())
    }

    /// See [`send_server_notice`].
    #[allow(clippy::too_many_arguments)]
    #[instrument(skip(self, target_displayname, target_avatar_url, body))]
    pub async fn send_server_notice(
        &self,
        room_id: &str,
        server_user: &str,
        target_user_id: &str,
        target_displayname: &Option<String>,
        target_avatar_url: &Option<String>,
        message_event_id: &str,
        create_event_id: &str,
        membership_event_id: &str,
        msgtype: &str,
        body: &str,
        now: i64,
    ) -> Result<i64, ApiError> {
        self.storage
            .send_server_notice(
                room_id,
                server_user,
                target_user_id,
                target_displayname,
                target_avatar_url,
                message_event_id,
                create_event_id,
                membership_event_id,
                msgtype,
                body,
                now,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::shared_fake_user_store;
    use crate::UserService;
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::Arc;
    use synapse_storage::user::UserStore;
    use tokio::sync::Mutex;

    /// Minimal in-memory `ServerNotificationStoreApi` covering only the
    /// surface the service tests exercise. Unused methods stay as
    /// `unimplemented!()` so the test build links cleanly.
    #[derive(Default)]
    struct MockServerNotificationStore {
        notifications: Mutex<std::collections::HashMap<i64, ServerNotification>>,
        templates: Mutex<std::collections::HashMap<String, NotificationTemplate>>,
        pending_scheduled: Mutex<Vec<ScheduledNotification>>,
        #[allow(clippy::type_complexity)]
        delivery_logs: Mutex<Vec<(i64, Option<String>, String, String, Option<String>)>>,
    }

    impl MockServerNotificationStore {
        fn new() -> Self {
            Self::default()
        }
    }

    fn sample_notification(id: i64, title: &str) -> ServerNotification {
        ServerNotification {
            id,
            title: title.to_string(),
            content: "body".to_string(),
            notification_type: "info".to_string(),
            priority: 0,
            target_audience: "all".to_string(),
            target_user_ids: json!([]),
            starts_at: None,
            expires_at: None,
            is_enabled: true,
            is_dismissable: true,
            action_url: None,
            action_text: None,
            created_by: None,
            created_ts: 1_700_000_000_000 + id,
            updated_ts: 1_700_000_000_000 + id,
        }
    }

    fn build_service() -> (Arc<MockServerNotificationStore>, ServerNotificationService) {
        let user_service = Arc::new(UserService::new(shared_fake_user_store() as Arc<dyn UserStore>));
        let store = Arc::new(MockServerNotificationStore::new());
        let service =
            ServerNotificationService::new(store.clone() as Arc<dyn ServerNotificationStoreApi>, user_service);
        (store, service)
    }

    #[async_trait]
    impl ServerNotificationStoreApi for MockServerNotificationStore {
        async fn create_notification(
            &self,
            request: CreateNotificationRequest,
        ) -> Result<ServerNotification, ApiError> {
            let mut map = self.notifications.lock().await;
            let id = (map.len() as i64) + 1;
            let n = ServerNotification {
                id,
                title: request.title,
                content: request.content,
                notification_type: request.notification_type.unwrap_or_else(|| "info".to_string()),
                priority: request.priority.unwrap_or(0),
                target_audience: request.target_audience.unwrap_or_else(|| "all".to_string()),
                target_user_ids: json!(request.target_user_ids.unwrap_or_default()),
                starts_at: request.starts_at,
                expires_at: request.expires_at,
                is_enabled: true,
                is_dismissable: request.is_dismissable.unwrap_or(true),
                action_url: request.action_url,
                action_text: request.action_text,
                created_by: request.created_by,
                created_ts: 1_700_000_000_000 + id,
                updated_ts: 1_700_000_000_000 + id,
            };
            map.insert(id, n.clone());
            Ok(n)
        }

        async fn get_notification(&self, notification_id: i64) -> Result<Option<ServerNotification>, ApiError> {
            Ok(self.notifications.lock().await.get(&notification_id).cloned())
        }

        async fn list_active_notifications(&self) -> Result<Vec<ServerNotification>, ApiError> {
            Ok(self.notifications.lock().await.values().filter(|n| n.is_enabled).cloned().collect())
        }

        async fn list_all_notifications(
            &self,
            audience: Option<&str>,
            limit: i64,
            _from: Option<ServerNotificationCursor>,
        ) -> Result<(Vec<ServerNotification>, Option<String>), ApiError> {
            let map = self.notifications.lock().await;
            let mut out: Vec<ServerNotification> =
                map.values().filter(|n| audience.is_none_or(|a| n.target_audience == a)).cloned().collect();
            out.sort_by_key(|n| (n.created_ts, n.id));
            out.truncate(limit as usize);
            Ok((out, None))
        }

        async fn update_notification(
            &self,
            notification_id: i64,
            request: CreateNotificationRequest,
        ) -> Result<ServerNotification, ApiError> {
            let mut map = self.notifications.lock().await;
            let n = map.get_mut(&notification_id).ok_or_else(|| ApiError::not_found("Notification not found"))?;
            n.title = request.title;
            n.content = request.content;
            n.updated_ts += 1;
            Ok(n.clone())
        }

        async fn delete_notification(&self, notification_id: i64) -> Result<bool, ApiError> {
            Ok(self.notifications.lock().await.remove(&notification_id).is_some())
        }

        async fn deactivate_notification(&self, notification_id: i64) -> Result<bool, ApiError> {
            let mut map = self.notifications.lock().await;
            match map.get_mut(&notification_id) {
                Some(n) if n.is_enabled => {
                    n.is_enabled = false;
                    Ok(true)
                }
                Some(_) => Ok(false), // already deactivated
                None => Ok(false),
            }
        }

        async fn get_user_notifications(&self, _user_id: &str) -> Result<Vec<NotificationWithStatus>, ApiError> {
            Ok(Vec::new())
        }

        async fn get_or_create_status(
            &self,
            _user_id: &str,
            _notification_id: i64,
        ) -> Result<UserNotificationStatus, ApiError> {
            unimplemented!()
        }

        async fn get_or_create_statuses_batch(
            &self,
            _user_id: &str,
            _notification_ids: &[i64],
        ) -> Result<std::collections::HashMap<i64, UserNotificationStatus>, ApiError> {
            unimplemented!()
        }

        async fn mark_as_read(&self, _user_id: &str, _notification_id: i64) -> Result<bool, ApiError> {
            Ok(true)
        }

        async fn mark_as_dismissed(&self, _user_id: &str, _notification_id: i64) -> Result<bool, ApiError> {
            Ok(true)
        }

        async fn mark_all_as_read(&self, _user_id: &str) -> Result<i64, ApiError> {
            Ok(0)
        }

        async fn create_template(&self, request: CreateTemplateRequest) -> Result<NotificationTemplate, ApiError> {
            let mut map = self.templates.lock().await;
            let id = (map.len() as i64) + 1;
            let t = NotificationTemplate {
                id,
                name: request.name.clone(),
                title_template: request.title_template,
                content_template: request.content_template,
                notification_type: request.notification_type.unwrap_or_else(|| "info".to_string()),
                variables: json!(request.variables.unwrap_or_default()),
                is_enabled: true,
                created_ts: 1_700_000_000_000,
                updated_ts: 1_700_000_000_000,
            };
            map.insert(request.name, t.clone());
            Ok(t)
        }

        async fn get_template(&self, name: &str) -> Result<Option<NotificationTemplate>, ApiError> {
            Ok(self.templates.lock().await.get(name).cloned())
        }

        async fn list_templates(&self) -> Result<Vec<NotificationTemplate>, ApiError> {
            Ok(self.templates.lock().await.values().cloned().collect())
        }

        async fn delete_template(&self, name: &str) -> Result<bool, ApiError> {
            Ok(self.templates.lock().await.remove(name).is_some())
        }

        async fn log_delivery(
            &self,
            notification_id: i64,
            user_id: Option<&str>,
            delivery_method: &str,
            status: &str,
            error_message: Option<&str>,
        ) -> Result<(), ApiError> {
            self.delivery_logs.lock().await.push((
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
            Ok(ScheduledNotification {
                id: notification_id,
                notification_id,
                scheduled_for,
                is_sent: false,
                sent_ts: None,
                created_ts: 1_700_000_000_000,
            })
        }

        async fn get_pending_scheduled_notifications(&self) -> Result<Vec<ScheduledNotification>, ApiError> {
            Ok(self.pending_scheduled.lock().await.clone())
        }

        async fn mark_scheduled_sent(&self, scheduled_id: i64) -> Result<bool, ApiError> {
            let mut q = self.pending_scheduled.lock().await;
            let initial = q.len();
            q.retain(|s| s.id != scheduled_id);
            Ok(q.len() != initial)
        }

        async fn get_user_notification_setting(&self, _user_id: &str) -> Result<Option<bool>, ApiError> {
            Ok(None)
        }

        async fn upsert_user_notification_setting(&self, _user_id: &str, _enabled: bool) -> Result<(), ApiError> {
            Ok(())
        }

        async fn get_user_pushers(&self, _user_id: &str) -> Result<Vec<serde_json::Value>, ApiError> {
            Ok(Vec::new())
        }

        async fn delete_user_pusher(&self, _user_id: &str, _pushkey: &str) -> Result<bool, ApiError> {
            Ok(false)
        }

        async fn get_server_notices_count(&self) -> Result<i64, ApiError> {
            Ok(0)
        }

        async fn get_server_notices_paginated(
            &self,
            _cursor: Option<(i64, i64)>,
            _limit: i64,
        ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), ApiError> {
            Ok((Vec::new(), 0, None))
        }

        async fn get_server_notice_by_id(&self, _notice_id: i64) -> Result<Option<serde_json::Value>, ApiError> {
            Ok(None)
        }

        async fn get_server_notice_with_room(
            &self,
            _notice_id: i64,
        ) -> Result<Option<(Option<String>, Option<String>)>, ApiError> {
            Ok(None)
        }

        async fn delete_server_notice_by_id(&self, _notice_id: i64) -> Result<bool, ApiError> {
            Ok(true)
        }

        async fn delete_room_cascade(&self, _room_id: &str) -> Result<(), ApiError> {
            Ok(())
        }

        async fn delete_event_by_id(&self, _event_id: &str) -> Result<(), ApiError> {
            Ok(())
        }

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
            Ok(1)
        }
    }

    // ── ensure_target_users_exist ──────────────────────────────────────

    #[tokio::test]
    async fn ensure_target_users_exist_succeeds_for_seeded_user() {
        let (_store, service) = build_service();
        service.ensure_target_users_exist(&["@alice:example.com".to_string()]).await.unwrap();
    }

    #[tokio::test]
    async fn ensure_target_users_exist_fails_for_missing_user() {
        let (_store, service) = build_service();
        let err = service.ensure_target_users_exist(&["@ghost:example.com".to_string()]).await.unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not found"), "expected not_found, got: {err}");
    }

    #[tokio::test]
    async fn ensure_target_users_exist_propagates_failure_on_first_missing() {
        let (_store, service) = build_service();
        // First user exists, second is missing — should fail fast on second.
        let err = service
            .ensure_target_users_exist(&["@alice:example.com".to_string(), "@ghost:example.com".to_string()])
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not found"));
    }

    #[tokio::test]
    async fn ensure_target_users_exist_empty_slice_is_ok() {
        let (_store, service) = build_service();
        service.ensure_target_users_exist(&[]).await.unwrap();
    }

    // ── create / get / list ────────────────────────────────────────────

    #[tokio::test]
    async fn create_then_get_roundtrip() {
        let (_store, service) = build_service();
        let req = CreateNotificationRequest {
            title: "Welcome".to_string(),
            content: "Hello world".to_string(),
            notification_type: Some("info".to_string()),
            priority: Some(5),
            target_audience: Some("all".to_string()),
            target_user_ids: None,
            starts_at: None,
            expires_at: None,
            is_dismissable: Some(true),
            action_url: None,
            action_text: None,
            created_by: Some("@admin:example.com".to_string()),
        };
        let created = service.create_notification(req).await.unwrap();
        assert_eq!(created.title, "Welcome");
        assert_eq!(created.created_by.as_deref(), Some("@admin:example.com"));

        let fetched = service.get_notification(created.id).await.unwrap();
        assert_eq!(fetched.unwrap().id, created.id);
    }

    #[tokio::test]
    async fn get_notification_returns_none_for_missing() {
        let (_store, service) = build_service();
        let result = service.get_notification(99999).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn list_active_filters_disabled() {
        let (store, service) = build_service();
        let n1 = service
            .create_notification(CreateNotificationRequest {
                title: "on".into(),
                content: "".into(),
                notification_type: None,
                priority: None,
                target_audience: None,
                target_user_ids: None,
                starts_at: None,
                expires_at: None,
                is_dismissable: None,
                action_url: None,
                action_text: None,
                created_by: None,
            })
            .await
            .unwrap();
        let n2 = service
            .create_notification(CreateNotificationRequest {
                title: "off".into(),
                content: "".into(),
                notification_type: None,
                priority: None,
                target_audience: None,
                target_user_ids: None,
                starts_at: None,
                expires_at: None,
                is_dismissable: None,
                action_url: None,
                action_text: None,
                created_by: None,
            })
            .await
            .unwrap();

        service.deactivate_notification(n2.id).await.unwrap();
        let active = service.list_active_notifications().await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, n1.id);

        // sanity: storage retains both
        assert_eq!(store.notifications.lock().await.len(), 2);
    }

    // ── update / delete / deactivate ───────────────────────────────────

    #[tokio::test]
    async fn update_notification_changes_title() {
        let (_store, service) = build_service();
        let n = service
            .create_notification(CreateNotificationRequest {
                title: "old".into(),
                content: "".into(),
                notification_type: None,
                priority: None,
                target_audience: None,
                target_user_ids: None,
                starts_at: None,
                expires_at: None,
                is_dismissable: None,
                action_url: None,
                action_text: None,
                created_by: None,
            })
            .await
            .unwrap();
        let updated = service
            .update_notification(
                n.id,
                CreateNotificationRequest {
                    title: "new".into(),
                    content: "body".into(),
                    notification_type: None,
                    priority: None,
                    target_audience: None,
                    target_user_ids: None,
                    starts_at: None,
                    expires_at: None,
                    is_dismissable: None,
                    action_url: None,
                    action_text: None,
                    created_by: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.title, "new");
        assert!(updated.updated_ts > n.updated_ts);
    }

    #[tokio::test]
    async fn update_notification_missing_returns_not_found() {
        let (_store, service) = build_service();
        let err = service
            .update_notification(
                99999,
                CreateNotificationRequest {
                    title: "x".into(),
                    content: "".into(),
                    notification_type: None,
                    priority: None,
                    target_audience: None,
                    target_user_ids: None,
                    starts_at: None,
                    expires_at: None,
                    is_dismissable: None,
                    action_url: None,
                    action_text: None,
                    created_by: None,
                },
            )
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not found"));
    }

    #[tokio::test]
    async fn delete_notification_true_for_existing_false_for_missing() {
        let (_store, service) = build_service();
        let n = service
            .create_notification(CreateNotificationRequest {
                title: "x".into(),
                content: "".into(),
                notification_type: None,
                priority: None,
                target_audience: None,
                target_user_ids: None,
                starts_at: None,
                expires_at: None,
                is_dismissable: None,
                action_url: None,
                action_text: None,
                created_by: None,
            })
            .await
            .unwrap();
        assert!(service.delete_notification(n.id).await.unwrap());
        assert!(!service.delete_notification(n.id).await.unwrap());
        assert!(!service.delete_notification(424242).await.unwrap());
    }

    #[tokio::test]
    async fn deactivate_toggles_is_enabled() {
        let (_store, service) = build_service();
        let n = service
            .create_notification(CreateNotificationRequest {
                title: "x".into(),
                content: "".into(),
                notification_type: None,
                priority: None,
                target_audience: None,
                target_user_ids: None,
                starts_at: None,
                expires_at: None,
                is_dismissable: None,
                action_url: None,
                action_text: None,
                created_by: None,
            })
            .await
            .unwrap();
        assert!(service.deactivate_notification(n.id).await.unwrap());
        assert!(!service.deactivate_notification(n.id).await.unwrap());
        assert!(!service.deactivate_notification(7777).await.unwrap());
    }

    // ── delete_server_notice branch logic ──────────────────────────────

    #[tokio::test]
    async fn delete_server_notice_returns_not_found_when_get_returns_none() {
        let (_store, service) = build_service();
        let err = service.delete_server_notice(42).await.unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not found"));
    }

    // ── create_from_template variable substitution ─────────────────────

    #[tokio::test]
    async fn create_from_template_replaces_placeholders() {
        let (_store, service) = build_service();
        let tpl = service
            .create_template(CreateTemplateRequest {
                name: "welcome".to_string(),
                title_template: "Hello {{name}}".to_string(),
                content_template: "Welcome {{name}}, your code is {{code}}".to_string(),
                notification_type: Some("info".to_string()),
                variables: Some(vec!["name".to_string(), "code".to_string()]),
            })
            .await
            .unwrap();
        assert_eq!(tpl.name, "welcome");

        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "Alice".to_string());
        vars.insert("code".to_string(), "12345".to_string());
        let n = service.create_from_template("welcome", vars, Some("all".to_string()), None).await.unwrap();
        assert_eq!(n.title, "Hello Alice");
        assert_eq!(n.content, "Welcome Alice, your code is 12345");
    }

    #[tokio::test]
    async fn create_from_template_returns_not_found_for_missing_template() {
        let (_store, service) = build_service();
        let err = service
            .create_from_template("does_not_exist", std::collections::HashMap::new(), None, None)
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not found"));
    }

    #[tokio::test]
    async fn create_from_template_with_no_variables_leaves_placeholders_intact() {
        let (_store, service) = build_service();
        service
            .create_template(CreateTemplateRequest {
                name: "t".to_string(),
                title_template: "Hello {{name}}".to_string(),
                content_template: "Static body".to_string(),
                notification_type: Some("info".to_string()),
                variables: None,
            })
            .await
            .unwrap();
        let n = service.create_from_template("t", std::collections::HashMap::new(), None, None).await.unwrap();
        assert_eq!(n.title, "Hello {{name}}");
        assert_eq!(n.content, "Static body");
    }

    // ── template CRUD ──────────────────────────────────────────────────

    #[tokio::test]
    async fn template_get_list_delete_roundtrip() {
        let (_store, service) = build_service();
        assert!(service.get_template("nope").await.unwrap().is_none());
        service
            .create_template(CreateTemplateRequest {
                name: "t1".to_string(),
                title_template: "T1".to_string(),
                content_template: "C1".to_string(),
                notification_type: None,
                variables: None,
            })
            .await
            .unwrap();
        let listed = service.list_templates().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "t1");
        assert!(service.delete_template("t1").await.unwrap());
        assert!(!service.delete_template("t1").await.unwrap());
    }

    // ── process_scheduled_notifications ────────────────────────────────

    #[tokio::test]
    async fn process_scheduled_notifications_returns_zero_when_queue_empty() {
        let (_store, service) = build_service();
        let n = service.process_scheduled_notifications().await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn process_scheduled_notifications_marks_only_existing() {
        let (store, service) = build_service();
        // Pre-populate the store with a real notification for id=10 and
        // a scheduled entry that references it. Also add a scheduled entry
        // pointing at a missing notification id=999 — that one must be skipped.
        store.notifications.lock().await.insert(10, sample_notification(10, "exists"));
        store.pending_scheduled.lock().await.push(ScheduledNotification {
            id: 100,
            notification_id: 10,
            scheduled_for: 1,
            is_sent: false,
            sent_ts: None,
            created_ts: 0,
        });
        store.pending_scheduled.lock().await.push(ScheduledNotification {
            id: 101,
            notification_id: 999,
            scheduled_for: 1,
            is_sent: false,
            sent_ts: None,
            created_ts: 0,
        });

        let processed = service.process_scheduled_notifications().await.unwrap();
        assert_eq!(processed, 1);
        // Surviving entry is the one whose notification did not exist.
        let remaining = store.pending_scheduled.lock().await;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, 101);
    }

    // ── broadcast_notification ─────────────────────────────────────────

    #[tokio::test]
    async fn broadcast_notification_records_delivery_log() {
        let (store, service) = build_service();
        service.broadcast_notification(7, "email").await.unwrap();
        let logs = store.delivery_logs.lock().await;
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].0, 7);
        assert_eq!(logs[0].2, "email");
        assert_eq!(logs[0].3, "broadcast");
    }

    // ── user setting passthrough ───────────────────────────────────────

    #[tokio::test]
    async fn user_notification_setting_roundtrip() {
        let (_store, service) = build_service();
        assert!(service.get_user_notification_setting("@alice:example.com").await.unwrap().is_none());
        service.upsert_user_notification_setting("@alice:example.com", false).await.unwrap();
        service.upsert_user_notification_setting("@alice:example.com", true).await.unwrap();
        // Mock returns None for get regardless; the no-error path is the contract.
    }

    // ── pusher passthrough (passthrough smoke) ──────────────────────────

    #[tokio::test]
    async fn pusher_passthrough_returns_empty_vec_and_false() {
        let (_store, service) = build_service();
        assert!(service.get_user_pushers("@alice:example.com").await.unwrap().is_empty());
        assert!(!service.delete_user_pusher("@alice:example.com", "key").await.unwrap());
    }

    // ── list_all_notifications pagination/audience filter ──────────────

    #[tokio::test]
    async fn list_all_notifications_respects_audience_filter_and_limit() {
        let (_store, service) = build_service();
        for i in 1..=4 {
            service
                .create_notification(CreateNotificationRequest {
                    title: format!("n{i}"),
                    content: "".into(),
                    notification_type: None,
                    priority: None,
                    target_audience: Some(if i % 2 == 0 { "admins".to_string() } else { "all".to_string() }),
                    target_user_ids: None,
                    starts_at: None,
                    expires_at: None,
                    is_dismissable: None,
                    action_url: None,
                    action_text: None,
                    created_by: None,
                })
                .await
                .unwrap();
        }
        let (admins, _) = service.list_all_notifications(Some("admins"), 10, None).await.unwrap();
        assert_eq!(admins.len(), 2);
        assert!(admins.iter().all(|n| n.target_audience == "admins"));
        let (limited, _) = service.list_all_notifications(Some("all"), 1, None).await.unwrap();
        assert_eq!(limited.len(), 1);
    }
}
