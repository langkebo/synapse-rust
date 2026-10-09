use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::media_quota::*;
use tracing::{info, instrument};

/// The `MediaQuotaService` struct.
pub struct MediaQuotaService {
    storage: Arc<synapse_storage::media_quota::MediaQuotaStorage>,
}

impl MediaQuotaService {
    /// See [`new`].
    pub fn new(storage: Arc<synapse_storage::media_quota::MediaQuotaStorage>) -> Self {
        Self { storage }
    }

    /// See [`check_upload_quota`].
    #[instrument(skip(self))]
    pub async fn check_upload_quota(&self, user_id: &str, file_size: i64) -> Result<QuotaCheckResult, ApiError> {
        info!(user_id = %user_id, file_size, "Checking upload quota");

        let server_quota = self.storage.get_server_quota().await?;

        if let Some(max_file_size) = server_quota.max_file_size_bytes {
            if file_size > max_file_size {
                return Ok(QuotaCheckResult {
                    is_allowed: false,
                    reason: Some(format!("File size {file_size} exceeds maximum allowed size {max_file_size}")),
                    current_usage: 0,
                    quota_limit: max_file_size,
                    usage_percent: 0.0,
                    rejection: Some(QuotaRejection::FileTooLarge),
                });
            }
        }

        if let Some(max_storage) = server_quota.max_storage_bytes {
            if max_storage > 0 {
                let new_total = server_quota.current_storage_bytes + file_size;
                if new_total > max_storage {
                    return Ok(QuotaCheckResult {
                        is_allowed: false,
                        reason: Some("Server storage quota exceeded".to_string()),
                        current_usage: server_quota.current_storage_bytes,
                        quota_limit: max_storage,
                        usage_percent: (server_quota.current_storage_bytes as f64 / max_storage as f64) * 100.0,
                        rejection: Some(QuotaRejection::StorageExceeded),
                    });
                }
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

    /// See [`record_upload`].
    #[instrument(skip(self))]
    pub async fn record_upload(
        &self,
        user_id: &str,
        media_id: &str,
        file_size: i64,
        mime_type: Option<&str>,
    ) -> Result<(), ApiError> {
        info!(
            user_id = %user_id,
            media_id = %media_id,
            file_size,
            mime_type = ?mime_type,
            "Recording media upload"
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

    /// See [`record_delete`].
    #[instrument(skip(self))]
    pub async fn record_delete(&self, user_id: &str, media_id: &str, file_size: i64) -> Result<(), ApiError> {
        info!(user_id = %user_id, media_id = %media_id, file_size, "Recording media delete");

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

    /// See [`get_user_quota`].
    #[instrument(skip(self))]
    pub async fn get_user_quota(&self, user_id: &str) -> Result<UserQuotaInfo, ApiError> {
        let user_quota = self.storage.get_or_create_user_quota(user_id).await?;
        let config = if let Some(config_id) = user_quota.quota_config_id {
            self.storage.get_config(config_id).await?
        } else {
            self.storage.get_default_config().await?
        };

        let max_storage =
            user_quota.custom_max_storage_bytes.or(config.as_ref().map(|c| c.max_storage_bytes)).unwrap_or(0);

        let max_file_size =
            user_quota.custom_max_file_size_bytes.or(config.as_ref().map(|c| c.max_file_size_bytes)).unwrap_or(0);

        let max_files = user_quota.custom_max_files_count.or(config.as_ref().map(|c| c.max_files_count)).unwrap_or(0);

        let usage_percent =
            if max_storage > 0 { (user_quota.current_storage_bytes as f64 / max_storage as f64) * 100.0 } else { 0.0 };

        Ok(UserQuotaInfo {
            current_storage_bytes: user_quota.current_storage_bytes,
            current_files_count: user_quota.current_files_count,
            max_storage_bytes: max_storage,
            max_file_size_bytes: max_file_size,
            max_files_count: max_files,
            usage_percent,
        })
    }

    /// See [`get_server_quota`].
    #[instrument(skip(self))]
    pub async fn get_server_quota(&self) -> Result<ServerMediaQuota, ApiError> {
        self.storage.get_server_quota().await
    }

    /// See [`get_user_alerts`].
    #[instrument(skip(self))]
    pub async fn get_user_alerts(&self, user_id: &str, unread_only: bool) -> Result<Vec<MediaQuotaAlert>, ApiError> {
        self.storage.get_user_alerts(user_id, unread_only).await
    }

    /// See [`get_usage_stats`].
    #[instrument(skip(self))]
    pub async fn get_usage_stats(&self, user_id: &str) -> Result<serde_json::Value, ApiError> {
        self.storage.get_usage_stats(user_id).await
    }
}

/// The `UserQuotaInfo` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserQuotaInfo {
    /// The `current_storage_bytes` field.
    pub current_storage_bytes: i64,
    /// The `current_files_count` field.
    pub current_files_count: i32,
    /// The `max_storage_bytes` field.
    pub max_storage_bytes: i64,
    /// The `max_file_size_bytes` field.
    pub max_file_size_bytes: i64,
    /// The `max_files_count` field.
    pub max_files_count: i32,
    /// The `usage_percent` field.
    pub usage_percent: f64,
}

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_quota_info() {
        let info = UserQuotaInfo {
            current_storage_bytes: 1000,
            current_files_count: 10,
            max_storage_bytes: 10000,
            max_file_size_bytes: 1000,
            max_files_count: 100,
            usage_percent: 10.0,
        };

        assert_eq!(info.current_storage_bytes, 1000);
        assert_eq!(info.usage_percent, 10.0);
    }

    #[test]
    fn test_user_quota_info_usage_percent() {
        let info = UserQuotaInfo {
            current_storage_bytes: 5000,
            current_files_count: 50,
            max_storage_bytes: 10000,
            max_file_size_bytes: 1000,
            max_files_count: 100,
            usage_percent: 50.0,
        };

        assert_eq!(info.usage_percent, 50.0);
    }

    #[test]
    fn test_user_quota_info_full() {
        let info = UserQuotaInfo {
            current_storage_bytes: 10000,
            current_files_count: 100,
            max_storage_bytes: 10000,
            max_file_size_bytes: 1000,
            max_files_count: 100,
            usage_percent: 100.0,
        };

        assert_eq!(info.usage_percent, 100.0);
    }

    #[test]
    fn test_user_quota_info_zero_limit() {
        let info = UserQuotaInfo {
            current_storage_bytes: 100,
            current_files_count: 5,
            max_storage_bytes: 0,
            max_file_size_bytes: 0,
            max_files_count: 0,
            usage_percent: 0.0,
        };

        assert_eq!(info.usage_percent, 0.0);
    }

    #[test]
    fn test_user_quota_info_serialization() {
        let info = UserQuotaInfo {
            current_storage_bytes: 1000,
            current_files_count: 10,
            max_storage_bytes: 10000,
            max_file_size_bytes: 1000,
            max_files_count: 100,
            usage_percent: 10.0,
        };

        let json = serde_json::to_string(&info).expect("Failed to serialize UserQuotaInfo");
        let parsed: UserQuotaInfo = serde_json::from_str(&json).expect("Failed to deserialize UserQuotaInfo");

        assert_eq!(parsed.current_storage_bytes, info.current_storage_bytes);
        assert_eq!(parsed.usage_percent, info.usage_percent);
    }
}
