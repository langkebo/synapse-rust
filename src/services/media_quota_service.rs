use crate::common::ApiError;
use crate::storage::media_quota::*;
use std::sync::Arc;
use tracing::{info, instrument};

pub struct MediaQuotaService {
    storage: Arc<MediaQuotaStorage>,
}

impl MediaQuotaService {
    pub fn new(storage: Arc<MediaQuotaStorage>) -> Self {
        Self { storage }
    }

    #[instrument(skip(self))]
    pub async fn check_upload_quota(
        &self,
        user_id: &str,
        file_size: i64,
    ) -> Result<QuotaCheckResult, ApiError> {
        info!(
            "Checking upload quota for user: {}, size: {}",
            user_id, file_size
        );

        let server_quota = self.storage.get_server_quota().await?;
        if server_quota.max_file_size_bytes > 0 && file_size > server_quota.max_file_size_bytes {
            return Ok(QuotaCheckResult {
                allowed: false,
                reason: Some(format!(
                    "File size {} exceeds maximum allowed size {}",
                    file_size, server_quota.max_file_size_bytes
                )),
                current_usage: 0,
                quota_limit: server_quota.max_file_size_bytes,
                usage_percent: 0.0,
            });
        }

        if server_quota.max_storage_bytes > 0 {
            let new_total = server_quota.current_storage_bytes + file_size;
            if new_total > server_quota.max_storage_bytes {
                return Ok(QuotaCheckResult {
                    allowed: false,
                    reason: Some("Server storage quota exceeded".to_string()),
                    current_usage: server_quota.current_storage_bytes,
                    quota_limit: server_quota.max_storage_bytes,
                    usage_percent: (server_quota.current_storage_bytes as f64
                        / server_quota.max_storage_bytes as f64)
                        * 100.0,
                });
            }
        }

        let result = self.storage.check_quota(user_id, file_size).await?;

        if result.usage_percent >= 80.0 && result.usage_percent < 100.0 {
            let _ = self
                .storage
                .create_alert(
                    user_id,
                    "warning",
                    80,
                    result.current_usage,
                    result.quota_limit,
                    Some("You are approaching your storage quota limit"),
                )
                .await;
        }

        Ok(result)
    }

    #[instrument(skip(self))]
    pub async fn record_upload(
        &self,
        user_id: &str,
        media_id: &str,
        file_size: i64,
        mime_type: Option<&str>,
    ) -> Result<(), ApiError> {
        info!(
            "Recording upload for user: {}, media: {}",
            user_id, media_id
        );

        self.storage
            .update_usage(UpdateUsageRequest {
                user_id: user_id.to_string(),
                media_id: media_id.to_string(),
                file_size_bytes: file_size,
                mime_type: mime_type.map(|s| s.to_string()),
                operation: "upload".to_string(),
            })
            .await
    }

    #[instrument(skip(self))]
    pub async fn record_delete(
        &self,
        user_id: &str,
        media_id: &str,
        file_size: i64,
    ) -> Result<(), ApiError> {
        info!(
            "Recording delete for user: {}, media: {}",
            user_id, media_id
        );

        self.storage
            .update_usage(UpdateUsageRequest {
                user_id: user_id.to_string(),
                media_id: media_id.to_string(),
                file_size_bytes: file_size,
                mime_type: None,
                operation: "delete".to_string(),
            })
            .await
    }

    #[instrument(skip(self))]
    pub async fn get_user_quota(&self, user_id: &str) -> Result<UserQuotaInfo, ApiError> {
        let user_quota = self.storage.get_or_create_user_quota(user_id).await?;
        let config = if let Some(config_id) = user_quota.quota_config_id {
            self.storage.get_config(config_id).await?
        } else {
            self.storage.get_default_config().await?
        };

        let max_storage = user_quota
            .custom_max_storage_bytes
            .or(config.as_ref().map(|c| c.max_storage_bytes))
            .unwrap_or(0);

        let max_file_size = user_quota
            .custom_max_file_size_bytes
            .or(config.as_ref().map(|c| c.max_file_size_bytes))
            .unwrap_or(0);

        let max_files = user_quota
            .custom_max_files_count
            .or(config.as_ref().map(|c| c.max_files_count))
            .unwrap_or(0);

        let usage_percent = if max_storage > 0 {
            (user_quota.current_storage_bytes as f64 / max_storage as f64) * 100.0
        } else {
            0.0
        };

        Ok(UserQuotaInfo {
            current_storage_bytes: user_quota.current_storage_bytes,
            current_files_count: user_quota.current_files_count,
            max_storage_bytes: max_storage,
            max_file_size_bytes: max_file_size,
            max_files_count: max_files,
            usage_percent,
        })
    }

    #[instrument(skip(self))]
    pub async fn set_user_quota(
        &self,
        request: SetUserQuotaRequest,
    ) -> Result<UserMediaQuota, ApiError> {
        info!("Setting user quota for: {}", request.user_id);
        self.storage.set_user_quota(request).await
    }

    #[instrument(skip(self))]
    pub async fn create_quota_config(
        &self,
        request: CreateQuotaConfigRequest,
    ) -> Result<MediaQuotaConfig, ApiError> {
        info!("Creating quota config: {}", request.name);
        self.storage.create_config(request).await
    }

    #[instrument(skip(self))]
    pub async fn list_quota_configs(&self) -> Result<Vec<MediaQuotaConfig>, ApiError> {
        self.storage.list_configs().await
    }

    #[instrument(skip(self))]
    pub async fn delete_quota_config(&self, config_id: i32) -> Result<bool, ApiError> {
        info!("Deleting quota config: {}", config_id);
        self.storage.delete_config(config_id).await
    }

    #[instrument(skip(self))]
    pub async fn get_server_quota(&self) -> Result<ServerMediaQuota, ApiError> {
        self.storage.get_server_quota().await
    }

    #[instrument(skip(self))]
    pub async fn update_server_quota(
        &self,
        max_storage_bytes: Option<i64>,
        max_file_size_bytes: Option<i64>,
        max_files_count: Option<i32>,
        alert_threshold_percent: Option<i32>,
    ) -> Result<ServerMediaQuota, ApiError> {
        info!("Updating server quota");
        self.storage
            .update_server_quota(
                max_storage_bytes,
                max_file_size_bytes,
                max_files_count,
                alert_threshold_percent,
            )
            .await
    }

    #[instrument(skip(self))]
    pub async fn get_user_alerts(
        &self,
        user_id: &str,
        unread_only: bool,
    ) -> Result<Vec<MediaQuotaAlert>, ApiError> {
        self.storage.get_user_alerts(user_id, unread_only).await
    }

    #[instrument(skip(self))]
    pub async fn mark_alert_read(&self, alert_id: i32) -> Result<bool, ApiError> {
        self.storage.mark_alert_read(alert_id).await
    }

    #[instrument(skip(self))]
    pub async fn get_usage_stats(&self, user_id: &str) -> Result<serde_json::Value, ApiError> {
        self.storage.get_usage_stats(user_id).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserQuotaInfo {
    pub current_storage_bytes: i64,
    pub current_files_count: i32,
    pub max_storage_bytes: i64,
    pub max_file_size_bytes: i64,
    pub max_files_count: i32,
    pub usage_percent: f64,
}

use serde::{Deserialize, Serialize};
