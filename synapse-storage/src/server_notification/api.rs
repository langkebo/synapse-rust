use std::collections::HashMap;

use async_trait::async_trait;
use synapse_common::ApiError;

use super::models::*;
use super::repository::ServerNotificationStorage;

#[async_trait]
pub trait ServerNotificationStoreApi: Send + Sync {
    async fn create_notification(&self, request: CreateNotificationRequest) -> Result<ServerNotification, ApiError>;

    async fn get_notification(&self, notification_id: i64) -> Result<Option<ServerNotification>, ApiError>;

    async fn list_active_notifications(&self) -> Result<Vec<ServerNotification>, ApiError>;

    async fn list_all_notifications(
        &self,
        audience: Option<&str>,
        limit: i64,
        from: Option<ServerNotificationCursor>,
    ) -> Result<(Vec<ServerNotification>, Option<String>), ApiError>;

    async fn update_notification(
        &self,
        notification_id: i64,
        request: CreateNotificationRequest,
    ) -> Result<ServerNotification, ApiError>;

    async fn delete_notification(&self, notification_id: i64) -> Result<bool, ApiError>;

    async fn deactivate_notification(&self, notification_id: i64) -> Result<bool, ApiError>;

    async fn get_user_notifications(&self, user_id: &str) -> Result<Vec<NotificationWithStatus>, ApiError>;

    async fn get_or_create_status(
        &self,
        user_id: &str,
        notification_id: i64,
    ) -> Result<UserNotificationStatus, ApiError>;

    async fn get_or_create_statuses_batch(
        &self,
        user_id: &str,
        notification_ids: &[i64],
    ) -> Result<HashMap<i64, UserNotificationStatus>, ApiError>;

    async fn mark_as_read(&self, user_id: &str, notification_id: i64) -> Result<bool, ApiError>;

    async fn mark_as_dismissed(&self, user_id: &str, notification_id: i64) -> Result<bool, ApiError>;

    async fn mark_all_as_read(&self, user_id: &str) -> Result<i64, ApiError>;

    async fn create_template(&self, request: CreateTemplateRequest) -> Result<NotificationTemplate, ApiError>;

    async fn get_template(&self, name: &str) -> Result<Option<NotificationTemplate>, ApiError>;

    async fn list_templates(&self) -> Result<Vec<NotificationTemplate>, ApiError>;

    async fn delete_template(&self, name: &str) -> Result<bool, ApiError>;

    async fn log_delivery(
        &self,
        notification_id: i64,
        user_id: Option<&str>,
        delivery_method: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), ApiError>;

    async fn schedule_notification(
        &self,
        notification_id: i64,
        scheduled_for: i64,
    ) -> Result<ScheduledNotification, ApiError>;

    async fn get_pending_scheduled_notifications(&self) -> Result<Vec<ScheduledNotification>, ApiError>;

    async fn mark_scheduled_sent(&self, scheduled_id: i64) -> Result<bool, ApiError>;

    async fn get_user_notification_setting(&self, user_id: &str) -> Result<Option<bool>, ApiError>;

    async fn upsert_user_notification_setting(&self, user_id: &str, enabled: bool) -> Result<(), ApiError>;

    async fn get_user_pushers(&self, user_id: &str) -> Result<Vec<serde_json::Value>, ApiError>;

    async fn delete_user_pusher(&self, user_id: &str, pushkey: &str) -> Result<bool, ApiError>;

    async fn get_server_notices_count(&self) -> Result<i64, ApiError>;

    async fn get_server_notices_paginated(
        &self,
        cursor: Option<(i64, i64)>,
        limit: i64,
    ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), ApiError>;

    async fn get_server_notice_by_id(&self, notice_id: i64) -> Result<Option<serde_json::Value>, ApiError>;

    async fn get_server_notice_with_room(
        &self,
        notice_id: i64,
    ) -> Result<Option<(Option<String>, Option<String>)>, ApiError>;

    async fn delete_server_notice_by_id(&self, notice_id: i64) -> Result<bool, ApiError>;

    async fn delete_room_cascade(&self, room_id: &str) -> Result<(), ApiError>;

    async fn delete_event_by_id(&self, event_id: &str) -> Result<(), ApiError>;

    #[allow(clippy::too_many_arguments)]
    async fn send_server_notice(
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
    ) -> Result<i64, ApiError>;
}

#[async_trait]
impl ServerNotificationStoreApi for ServerNotificationStorage {
    async fn create_notification(&self, request: CreateNotificationRequest) -> Result<ServerNotification, ApiError> {
        self.create_notification(request).await
    }

    async fn get_notification(&self, notification_id: i64) -> Result<Option<ServerNotification>, ApiError> {
        self.get_notification(notification_id).await
    }

    async fn list_active_notifications(&self) -> Result<Vec<ServerNotification>, ApiError> {
        self.list_active_notifications().await
    }

    async fn list_all_notifications(
        &self,
        audience: Option<&str>,
        limit: i64,
        from: Option<ServerNotificationCursor>,
    ) -> Result<(Vec<ServerNotification>, Option<String>), ApiError> {
        self.list_all_notifications(audience, limit, from).await
    }

    async fn update_notification(
        &self,
        notification_id: i64,
        request: CreateNotificationRequest,
    ) -> Result<ServerNotification, ApiError> {
        self.update_notification(notification_id, request).await
    }

    async fn delete_notification(&self, notification_id: i64) -> Result<bool, ApiError> {
        self.delete_notification(notification_id).await
    }

    async fn deactivate_notification(&self, notification_id: i64) -> Result<bool, ApiError> {
        self.deactivate_notification(notification_id).await
    }

    async fn get_user_notifications(&self, user_id: &str) -> Result<Vec<NotificationWithStatus>, ApiError> {
        self.get_user_notifications(user_id).await
    }

    async fn get_or_create_status(
        &self,
        user_id: &str,
        notification_id: i64,
    ) -> Result<UserNotificationStatus, ApiError> {
        self.get_or_create_status(user_id, notification_id).await
    }

    async fn get_or_create_statuses_batch(
        &self,
        user_id: &str,
        notification_ids: &[i64],
    ) -> Result<HashMap<i64, UserNotificationStatus>, ApiError> {
        self.get_or_create_statuses_batch(user_id, notification_ids).await
    }

    async fn mark_as_read(&self, user_id: &str, notification_id: i64) -> Result<bool, ApiError> {
        self.mark_as_read(user_id, notification_id).await
    }

    async fn mark_as_dismissed(&self, user_id: &str, notification_id: i64) -> Result<bool, ApiError> {
        self.mark_as_dismissed(user_id, notification_id).await
    }

    async fn mark_all_as_read(&self, user_id: &str) -> Result<i64, ApiError> {
        self.mark_all_as_read(user_id).await
    }

    async fn create_template(&self, request: CreateTemplateRequest) -> Result<NotificationTemplate, ApiError> {
        self.create_template(request).await
    }

    async fn get_template(&self, name: &str) -> Result<Option<NotificationTemplate>, ApiError> {
        self.get_template(name).await
    }

    async fn list_templates(&self) -> Result<Vec<NotificationTemplate>, ApiError> {
        self.list_templates().await
    }

    async fn delete_template(&self, name: &str) -> Result<bool, ApiError> {
        self.delete_template(name).await
    }

    async fn log_delivery(
        &self,
        notification_id: i64,
        user_id: Option<&str>,
        delivery_method: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), ApiError> {
        self.log_delivery(notification_id, user_id, delivery_method, status, error_message).await
    }

    async fn schedule_notification(
        &self,
        notification_id: i64,
        scheduled_for: i64,
    ) -> Result<ScheduledNotification, ApiError> {
        self.schedule_notification(notification_id, scheduled_for).await
    }

    async fn get_pending_scheduled_notifications(&self) -> Result<Vec<ScheduledNotification>, ApiError> {
        self.get_pending_scheduled_notifications().await
    }

    async fn mark_scheduled_sent(&self, scheduled_id: i64) -> Result<bool, ApiError> {
        self.mark_scheduled_sent(scheduled_id).await
    }

    async fn get_user_notification_setting(&self, user_id: &str) -> Result<Option<bool>, ApiError> {
        self.get_user_notification_setting(user_id).await
    }

    async fn upsert_user_notification_setting(&self, user_id: &str, enabled: bool) -> Result<(), ApiError> {
        self.upsert_user_notification_setting(user_id, enabled).await
    }

    async fn get_user_pushers(&self, user_id: &str) -> Result<Vec<serde_json::Value>, ApiError> {
        self.get_user_pushers(user_id).await
    }

    async fn delete_user_pusher(&self, user_id: &str, pushkey: &str) -> Result<bool, ApiError> {
        self.delete_user_pusher(user_id, pushkey).await
    }

    async fn get_server_notices_count(&self) -> Result<i64, ApiError> {
        self.get_server_notices_count().await
    }

    async fn get_server_notices_paginated(
        &self,
        cursor: Option<(i64, i64)>,
        limit: i64,
    ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), ApiError> {
        self.get_server_notices_paginated(cursor, limit).await
    }

    async fn get_server_notice_by_id(&self, notice_id: i64) -> Result<Option<serde_json::Value>, ApiError> {
        self.get_server_notice_by_id(notice_id).await
    }

    async fn get_server_notice_with_room(
        &self,
        notice_id: i64,
    ) -> Result<Option<(Option<String>, Option<String>)>, ApiError> {
        self.get_server_notice_with_room(notice_id).await
    }

    async fn delete_server_notice_by_id(&self, notice_id: i64) -> Result<bool, ApiError> {
        self.delete_server_notice_by_id(notice_id).await
    }

    async fn delete_room_cascade(&self, room_id: &str) -> Result<(), ApiError> {
        self.delete_room_cascade(room_id).await
    }

    async fn delete_event_by_id(&self, event_id: &str) -> Result<(), ApiError> {
        self.delete_event_by_id(event_id).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn send_server_notice(
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
        self.send_server_notice(
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
