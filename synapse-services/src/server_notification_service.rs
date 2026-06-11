use synapse_common::ApiError;
use synapse_storage::server_notification::*;
use std::sync::Arc;
use tracing::{info, instrument};

pub struct ServerNotificationService {
    storage: Arc<ServerNotificationStorage>,
}

impl ServerNotificationService {
    pub fn new(storage: Arc<ServerNotificationStorage>) -> Self {
        Self { storage }
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
}
