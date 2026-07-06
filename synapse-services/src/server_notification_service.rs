use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::server_notification::*;
use synapse_storage::user::{User, UserStore};
use tracing::{info, instrument};

pub struct ServerNotificationService {
    storage: Arc<dyn ServerNotificationStoreApi>,
    user_storage: Arc<dyn UserStore>,
}

impl ServerNotificationService {
    pub fn new(storage: Arc<dyn ServerNotificationStoreApi>, user_storage: Arc<dyn UserStore>) -> Self {
        Self { storage, user_storage }
    }

    #[instrument(skip(self))]
    pub async fn get_user_by_identifier(&self, user_id: &str) -> Result<Option<User>, ApiError> {
        self.user_storage.get_user_by_identifier(user_id).await.map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })
    }

    #[instrument(skip(self))]
    pub async fn ensure_user_exists(&self, user_id: &str) -> Result<(), ApiError> {
        if self.get_user_by_identifier(user_id).await?.is_none() {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn ensure_target_users_exist(&self, user_ids: &[String]) -> Result<(), ApiError> {
        for user_id in user_ids {
            self.ensure_user_exists(user_id).await?;
        }

        Ok(())
    }

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

    #[instrument(skip(self))]
    pub async fn get_notification(&self, notification_id: i64) -> Result<Option<ServerNotification>, ApiError> {
        self.storage.get_notification(notification_id).await
    }

    #[instrument(skip(self))]
    pub async fn list_active_notifications(&self) -> Result<Vec<ServerNotification>, ApiError> {
        self.storage.list_active_notifications().await
    }

    #[instrument(skip(self))]
    pub async fn get_user_notification_setting(&self, user_id: &str) -> Result<Option<bool>, ApiError> {
        self.storage.get_user_notification_setting(user_id).await
    }

    #[instrument(skip(self))]
    pub async fn upsert_user_notification_setting(&self, user_id: &str, enabled: bool) -> Result<(), ApiError> {
        self.storage.upsert_user_notification_setting(user_id, enabled).await
    }

    #[instrument(skip(self))]
    pub async fn get_user_pushers(&self, user_id: &str) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage.get_user_pushers(user_id).await
    }

    #[instrument(skip(self))]
    pub async fn delete_user_pusher(&self, user_id: &str, pushkey: &str) -> Result<bool, ApiError> {
        self.storage.delete_user_pusher(user_id, pushkey).await
    }

    #[instrument(skip(self))]
    pub async fn list_all_notifications(
        &self,
        audience: Option<&str>,
        limit: i64,
        from: Option<ServerNotificationCursor>,
    ) -> Result<(Vec<ServerNotification>, Option<String>), ApiError> {
        self.storage.list_all_notifications(audience, limit, from).await
    }

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

    #[instrument(skip(self))]
    pub async fn delete_notification(&self, notification_id: i64) -> Result<bool, ApiError> {
        info!(notification_id, "Deleting notification");
        self.storage.delete_notification(notification_id).await
    }

    #[instrument(skip(self))]
    pub async fn deactivate_notification(&self, notification_id: i64) -> Result<bool, ApiError> {
        info!(notification_id, "Deactivating notification");
        self.storage.deactivate_notification(notification_id).await
    }

    #[instrument(skip(self))]
    #[allow(clippy::type_complexity)]
    pub async fn get_server_notices_paginated(
        &self,
        cursor: Option<(i64, i64)>,
        limit: i64,
    ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), ApiError> {
        self.storage.get_server_notices_paginated(cursor, limit).await
    }

    #[instrument(skip(self))]
    pub async fn get_server_notice_by_id(&self, notice_id: i64) -> Result<Option<serde_json::Value>, ApiError> {
        self.storage.get_server_notice_by_id(notice_id).await
    }

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

    #[instrument(skip(self))]
    pub async fn get_user_notifications(&self, user_id: &str) -> Result<Vec<NotificationWithStatus>, ApiError> {
        self.storage.get_user_notifications(user_id).await
    }

    #[instrument(skip(self))]
    pub async fn mark_as_read(&self, user_id: &str, notification_id: i64) -> Result<bool, ApiError> {
        info!(notification_id, user_id = %user_id, "Marking notification as read");
        self.storage.mark_as_read(user_id, notification_id).await
    }

    #[instrument(skip(self))]
    pub async fn mark_as_dismissed(&self, user_id: &str, notification_id: i64) -> Result<bool, ApiError> {
        info!(notification_id, user_id = %user_id, "Dismissing notification");
        self.storage.mark_as_dismissed(user_id, notification_id).await
    }

    #[instrument(skip(self))]
    pub async fn mark_all_as_read(&self, user_id: &str) -> Result<i64, ApiError> {
        info!(user_id = %user_id, "Marking all notifications as read");
        self.storage.mark_all_as_read(user_id).await
    }

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

    #[instrument(skip(self))]
    pub async fn get_template(&self, name: &str) -> Result<Option<NotificationTemplate>, ApiError> {
        self.storage.get_template(name).await
    }

    #[instrument(skip(self))]
    pub async fn list_templates(&self) -> Result<Vec<NotificationTemplate>, ApiError> {
        self.storage.list_templates().await
    }

    #[instrument(skip(self))]
    pub async fn delete_template(&self, name: &str) -> Result<bool, ApiError> {
        info!(template_name = %name, "Deleting notification template");
        self.storage.delete_template(name).await
    }

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

    #[instrument(skip(self))]
    pub async fn schedule_notification(
        &self,
        notification_id: i64,
        scheduled_for: i64,
    ) -> Result<ScheduledNotification, ApiError> {
        info!(notification_id, scheduled_for, "Scheduling notification");
        self.storage.schedule_notification(notification_id, scheduled_for).await
    }

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

    #[instrument(skip(self))]
    pub async fn broadcast_notification(&self, notification_id: i64, delivery_method: &str) -> Result<(), ApiError> {
        info!(notification_id, delivery_method = %delivery_method, "Broadcasting notification");

        self.storage.log_delivery(notification_id, None, delivery_method, "broadcast", None).await?;

        Ok(())
    }

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
