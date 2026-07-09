use async_trait::async_trait;
use synapse_common::ApiError;

use super::models::*;
use super::repository::MediaQuotaStorage;

#[async_trait]
pub trait MediaQuotaStoreApi: Send + Sync {
    async fn get_default_config(&self) -> Result<Option<MediaQuotaConfig>, ApiError>;
    async fn get_config(&self, config_id: i64) -> Result<Option<MediaQuotaConfig>, ApiError>;
    async fn create_config(&self, request: CreateQuotaConfigRequest) -> Result<MediaQuotaConfig, ApiError>;
    async fn list_configs(&self) -> Result<Vec<MediaQuotaConfig>, ApiError>;
    async fn delete_config(&self, config_id: i64) -> Result<bool, ApiError>;
    async fn get_user_quota(&self, user_id: &str) -> Result<Option<UserMediaQuota>, ApiError>;
    async fn get_or_create_user_quota(&self, user_id: &str) -> Result<UserMediaQuota, ApiError>;
    async fn set_user_quota(&self, request: SetUserQuotaRequest) -> Result<UserMediaQuota, ApiError>;
    async fn update_usage(&self, request: UpdateUsageRequest) -> Result<(), ApiError>;
    async fn check_quota(&self, user_id: &str, file_size: i64) -> Result<QuotaCheckResult, ApiError>;
    async fn get_server_quota(&self) -> Result<ServerMediaQuota, ApiError>;
    async fn update_server_quota(
        &self,
        max_storage_bytes: Option<i64>,
        max_file_size_bytes: Option<i64>,
        max_files_count: Option<i32>,
        alert_threshold_percent: Option<i32>,
    ) -> Result<ServerMediaQuota, ApiError>;
    async fn create_alert(
        &self,
        user_id: &str,
        alert_type: &str,
        threshold_percent: i32,
        current_usage: i64,
        quota_limit: i64,
        message: Option<&str>,
    ) -> Result<MediaQuotaAlert, ApiError>;
    async fn get_user_alerts(&self, user_id: &str, unread_only: bool) -> Result<Vec<MediaQuotaAlert>, ApiError>;
    async fn mark_alert_read(&self, alert_id: i64) -> Result<bool, ApiError>;
    async fn get_usage_stats(&self, user_id: &str) -> Result<serde_json::Value, ApiError>;
}

#[async_trait]
impl MediaQuotaStoreApi for MediaQuotaStorage {
    async fn get_default_config(&self) -> Result<Option<MediaQuotaConfig>, ApiError> {
        self.get_default_config().await
    }
    async fn get_config(&self, config_id: i64) -> Result<Option<MediaQuotaConfig>, ApiError> {
        self.get_config(config_id).await
    }
    async fn create_config(&self, request: CreateQuotaConfigRequest) -> Result<MediaQuotaConfig, ApiError> {
        self.create_config(request).await
    }
    async fn list_configs(&self) -> Result<Vec<MediaQuotaConfig>, ApiError> {
        self.list_configs().await
    }
    async fn delete_config(&self, config_id: i64) -> Result<bool, ApiError> {
        self.delete_config(config_id).await
    }
    async fn get_user_quota(&self, user_id: &str) -> Result<Option<UserMediaQuota>, ApiError> {
        self.get_user_quota(user_id).await
    }
    async fn get_or_create_user_quota(&self, user_id: &str) -> Result<UserMediaQuota, ApiError> {
        self.get_or_create_user_quota(user_id).await
    }
    async fn set_user_quota(&self, request: SetUserQuotaRequest) -> Result<UserMediaQuota, ApiError> {
        self.set_user_quota(request).await
    }
    async fn update_usage(&self, request: UpdateUsageRequest) -> Result<(), ApiError> {
        self.update_usage(request).await
    }
    async fn check_quota(&self, user_id: &str, file_size: i64) -> Result<QuotaCheckResult, ApiError> {
        self.check_quota(user_id, file_size).await
    }
    async fn get_server_quota(&self) -> Result<ServerMediaQuota, ApiError> {
        self.get_server_quota().await
    }
    async fn update_server_quota(
        &self,
        max_storage_bytes: Option<i64>,
        max_file_size_bytes: Option<i64>,
        max_files_count: Option<i32>,
        alert_threshold_percent: Option<i32>,
    ) -> Result<ServerMediaQuota, ApiError> {
        self.update_server_quota(max_storage_bytes, max_file_size_bytes, max_files_count, alert_threshold_percent).await
    }
    async fn create_alert(
        &self,
        user_id: &str,
        alert_type: &str,
        threshold_percent: i32,
        current_usage: i64,
        quota_limit: i64,
        message: Option<&str>,
    ) -> Result<MediaQuotaAlert, ApiError> {
        self.create_alert(user_id, alert_type, threshold_percent, current_usage, quota_limit, message).await
    }
    async fn get_user_alerts(&self, user_id: &str, unread_only: bool) -> Result<Vec<MediaQuotaAlert>, ApiError> {
        self.get_user_alerts(user_id, unread_only).await
    }
    async fn mark_alert_read(&self, alert_id: i64) -> Result<bool, ApiError> {
        self.mark_alert_read(alert_id).await
    }
    async fn get_usage_stats(&self, user_id: &str) -> Result<serde_json::Value, ApiError> {
        self.get_usage_stats(user_id).await
    }
}
