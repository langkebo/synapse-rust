use crate::common::ApiError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MediaQuotaConfig {
    pub id: i32,
    pub name: String,
    pub description: Option<String>,
    pub max_storage_bytes: i64,
    pub max_file_size_bytes: i64,
    pub max_files_count: i32,
    pub allowed_mime_types: serde_json::Value,
    pub blocked_mime_types: serde_json::Value,
    pub is_default: bool,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserMediaQuota {
    pub id: i32,
    pub user_id: String,
    pub quota_config_id: Option<i32>,
    pub custom_max_storage_bytes: Option<i64>,
    pub custom_max_file_size_bytes: Option<i64>,
    pub custom_max_files_count: Option<i32>,
    pub current_storage_bytes: i64,
    pub current_files_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MediaUsageLog {
    pub id: i32,
    pub user_id: String,
    pub media_id: String,
    pub file_size_bytes: i64,
    pub mime_type: Option<String>,
    pub operation: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MediaQuotaAlert {
    pub id: i32,
    pub user_id: String,
    pub alert_type: String,
    pub threshold_percent: i32,
    pub current_usage_bytes: i64,
    pub quota_limit_bytes: i64,
    pub message: Option<String>,
    pub is_read: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServerMediaQuota {
    pub id: i32,
    pub max_storage_bytes: i64,
    pub max_file_size_bytes: i64,
    pub max_files_count: i32,
    pub current_storage_bytes: i64,
    pub current_files_count: i32,
    pub alert_threshold_percent: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateQuotaConfigRequest {
    pub name: String,
    pub description: Option<String>,
    pub max_storage_bytes: i64,
    pub max_file_size_bytes: i64,
    pub max_files_count: i32,
    pub allowed_mime_types: Option<Vec<String>>,
    pub blocked_mime_types: Option<Vec<String>>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetUserQuotaRequest {
    pub user_id: String,
    pub quota_config_id: Option<i32>,
    pub custom_max_storage_bytes: Option<i64>,
    pub custom_max_file_size_bytes: Option<i64>,
    pub custom_max_files_count: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUsageRequest {
    pub user_id: String,
    pub media_id: String,
    pub file_size_bytes: i64,
    pub mime_type: Option<String>,
    pub operation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaCheckResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub current_usage: i64,
    pub quota_limit: i64,
    pub usage_percent: f64,
}

#[derive(Clone)]
pub struct MediaQuotaStorage {
    pool: PgPool,
}

impl MediaQuotaStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: (**pool).clone() }
    }

    pub async fn get_default_config(&self) -> Result<Option<MediaQuotaConfig>, ApiError> {
        let config = sqlx::query_as::<_, MediaQuotaConfig>(
            r#"SELECT * FROM media_quota_config WHERE is_default = TRUE AND is_active = TRUE LIMIT 1"#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get default quota config: {}", e)))?;

        Ok(config)
    }

    pub async fn get_config(&self, config_id: i32) -> Result<Option<MediaQuotaConfig>, ApiError> {
        let config = sqlx::query_as::<_, MediaQuotaConfig>(
            r#"SELECT * FROM media_quota_config WHERE id = $1"#,
        )
        .bind(config_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get quota config: {}", e)))?;

        Ok(config)
    }

    pub async fn create_config(&self, request: CreateQuotaConfigRequest) -> Result<MediaQuotaConfig, ApiError> {
        let allowed_mime_types = serde_json::to_value(request.allowed_mime_types.unwrap_or_default())
            .unwrap_or(serde_json::json!([]));
        let blocked_mime_types = serde_json::to_value(request.blocked_mime_types.unwrap_or_default())
            .unwrap_or(serde_json::json!([]));

        if request.is_default.unwrap_or(false) {
            sqlx::query(
                r#"UPDATE media_quota_config SET is_default = FALSE WHERE is_default = TRUE"#,
            )
            .execute(&self.pool)
            .await
            .ok();
        }

        let config = sqlx::query_as::<_, MediaQuotaConfig>(
            r#"
            INSERT INTO media_quota_config (
                name, description, max_storage_bytes, max_file_size_bytes,
                max_files_count, allowed_mime_types, blocked_mime_types, is_default
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#,
        )
        .bind(&request.name)
        .bind(&request.description)
        .bind(request.max_storage_bytes)
        .bind(request.max_file_size_bytes)
        .bind(request.max_files_count)
        .bind(&allowed_mime_types)
        .bind(&blocked_mime_types)
        .bind(request.is_default.unwrap_or(false))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create quota config: {}", e)))?;

        Ok(config)
    }

    pub async fn list_configs(&self) -> Result<Vec<MediaQuotaConfig>, ApiError> {
        let configs = sqlx::query_as::<_, MediaQuotaConfig>(
            r#"SELECT * FROM media_quota_config WHERE is_active = TRUE ORDER BY created_at DESC"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list quota configs: {}", e)))?;

        Ok(configs)
    }

    pub async fn delete_config(&self, config_id: i32) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r#"UPDATE media_quota_config SET is_active = FALSE WHERE id = $1"#,
        )
        .bind(config_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete quota config: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_user_quota(&self, user_id: &str) -> Result<Option<UserMediaQuota>, ApiError> {
        let quota = sqlx::query_as::<_, UserMediaQuota>(
            r#"SELECT * FROM user_media_quota WHERE user_id = $1"#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get user quota: {}", e)))?;

        Ok(quota)
    }

    pub async fn get_or_create_user_quota(&self, user_id: &str) -> Result<UserMediaQuota, ApiError> {
        if let Some(quota) = self.get_user_quota(user_id).await? {
            return Ok(quota);
        }

        let default_config = self.get_default_config().await?;
        let quota_config_id = default_config.map(|c| c.id);

        let quota = sqlx::query_as::<_, UserMediaQuota>(
            r#"
            INSERT INTO user_media_quota (user_id, quota_config_id)
            VALUES ($1, $2)
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(quota_config_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create user quota: {}", e)))?;

        Ok(quota)
    }

    pub async fn set_user_quota(&self, request: SetUserQuotaRequest) -> Result<UserMediaQuota, ApiError> {
        let now = Utc::now();

        let quota = sqlx::query_as::<_, UserMediaQuota>(
            r#"
            INSERT INTO user_media_quota (
                user_id, quota_config_id, custom_max_storage_bytes,
                custom_max_file_size_bytes, custom_max_files_count
            )
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id)
            DO UPDATE SET
                quota_config_id = $2,
                custom_max_storage_bytes = $3,
                custom_max_file_size_bytes = $4,
                custom_max_files_count = $5,
                updated_at = $6
            RETURNING *
            "#,
        )
        .bind(&request.user_id)
        .bind(&request.quota_config_id)
        .bind(&request.custom_max_storage_bytes)
        .bind(&request.custom_max_file_size_bytes)
        .bind(&request.custom_max_files_count)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to set user quota: {}", e)))?;

        Ok(quota)
    }

    pub async fn update_usage(&self, request: UpdateUsageRequest) -> Result<(), ApiError> {
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO media_usage_log (user_id, media_id, file_size_bytes, mime_type, operation, timestamp)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(&request.user_id)
        .bind(&request.media_id)
        .bind(request.file_size_bytes)
        .bind(&request.mime_type)
        .bind(&request.operation)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to log media usage: {}", e)))?;

        let delta = if request.operation == "upload" {
            request.file_size_bytes
        } else if request.operation == "delete" {
            -request.file_size_bytes
        } else {
            0
        };

        sqlx::query(
            r#"
            INSERT INTO user_media_quota (user_id, current_storage_bytes, current_files_count)
            VALUES ($1, $2, 1)
            ON CONFLICT (user_id)
            DO UPDATE SET
                current_storage_bytes = GREATEST(0, user_media_quota.current_storage_bytes + $2),
                current_files_count = CASE
                    WHEN $3 = 'upload' THEN user_media_quota.current_files_count + 1
                    WHEN $3 = 'delete' THEN GREATEST(0, user_media_quota.current_files_count - 1)
                    ELSE user_media_quota.current_files_count
                END,
                updated_at = $4
            "#,
        )
        .bind(&request.user_id)
        .bind(delta)
        .bind(&request.operation)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update user quota usage: {}", e)))?;

        sqlx::query(
            r#"
            UPDATE server_media_quota
            SET current_storage_bytes = GREATEST(0, current_storage_bytes + $1),
                current_files_count = CASE
                    WHEN $2 = 'upload' THEN current_files_count + 1
                    WHEN $2 = 'delete' THEN GREATEST(0, current_files_count - 1)
                    ELSE current_files_count
                END,
                updated_at = $3
            WHERE id = 1
            "#,
        )
        .bind(delta)
        .bind(&request.operation)
        .bind(now)
        .execute(&self.pool)
        .await
        .ok();

        Ok(())
    }

    pub async fn check_quota(&self, user_id: &str, file_size: i64) -> Result<QuotaCheckResult, ApiError> {
        let user_quota = self.get_or_create_user_quota(user_id).await?;

        let max_storage = user_quota.custom_max_storage_bytes.unwrap_or_else(|| {
            if let Some(config_id) = user_quota.quota_config_id {
                futures::executor::block_on(self.get_config(config_id))
                    .ok()
                    .flatten()
                    .map(|c| c.max_storage_bytes)
                    .unwrap_or(0)
            } else {
                0
            }
        });

        if max_storage == 0 {
            return Ok(QuotaCheckResult {
                allowed: true,
                reason: None,
                current_usage: user_quota.current_storage_bytes,
                quota_limit: max_storage,
                usage_percent: 0.0,
            });
        }

        let new_usage = user_quota.current_storage_bytes + file_size;
        let allowed = new_usage <= max_storage;
        let usage_percent = (user_quota.current_storage_bytes as f64 / max_storage as f64) * 100.0;

        Ok(QuotaCheckResult {
            allowed,
            reason: if !allowed {
                Some("Quota exceeded".to_string())
            } else {
                None
            },
            current_usage: user_quota.current_storage_bytes,
            quota_limit: max_storage,
            usage_percent,
        })
    }

    pub async fn get_server_quota(&self) -> Result<ServerMediaQuota, ApiError> {
        let quota = sqlx::query_as::<_, ServerMediaQuota>(
            r#"SELECT * FROM server_media_quota WHERE id = 1"#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get server quota: {}", e)))?;

        Ok(quota)
    }

    pub async fn update_server_quota(
        &self,
        max_storage_bytes: Option<i64>,
        max_file_size_bytes: Option<i64>,
        max_files_count: Option<i32>,
        alert_threshold_percent: Option<i32>,
    ) -> Result<ServerMediaQuota, ApiError> {
        let now = Utc::now();

        let quota = sqlx::query_as::<_, ServerMediaQuota>(
            r#"
            UPDATE server_media_quota
            SET
                max_storage_bytes = COALESCE($1, max_storage_bytes),
                max_file_size_bytes = COALESCE($2, max_file_size_bytes),
                max_files_count = COALESCE($3, max_files_count),
                alert_threshold_percent = COALESCE($4, alert_threshold_percent),
                updated_at = $5
            WHERE id = 1
            RETURNING *
            "#,
        )
        .bind(max_storage_bytes)
        .bind(max_file_size_bytes)
        .bind(max_files_count)
        .bind(alert_threshold_percent)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update server quota: {}", e)))?;

        Ok(quota)
    }

    pub async fn create_alert(
        &self,
        user_id: &str,
        alert_type: &str,
        threshold_percent: i32,
        current_usage: i64,
        quota_limit: i64,
        message: Option<&str>,
    ) -> Result<MediaQuotaAlert, ApiError> {
        let alert = sqlx::query_as::<_, MediaQuotaAlert>(
            r#"
            INSERT INTO media_quota_alerts (
                user_id, alert_type, threshold_percent, current_usage_bytes,
                quota_limit_bytes, message
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(alert_type)
        .bind(threshold_percent)
        .bind(current_usage)
        .bind(quota_limit)
        .bind(message)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create quota alert: {}", e)))?;

        Ok(alert)
    }

    pub async fn get_user_alerts(&self, user_id: &str, unread_only: bool) -> Result<Vec<MediaQuotaAlert>, ApiError> {
        let alerts = if unread_only {
            sqlx::query_as::<_, MediaQuotaAlert>(
                r#"SELECT * FROM media_quota_alerts WHERE user_id = $1 AND is_read = FALSE ORDER BY created_at DESC"#,
            )
            .bind(user_id)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, MediaQuotaAlert>(
                r#"SELECT * FROM media_quota_alerts WHERE user_id = $1 ORDER BY created_at DESC"#,
            )
            .bind(user_id)
            .fetch_all(&self.pool)
            .await
        };

        alerts.map_err(|e| ApiError::internal(format!("Failed to get user alerts: {}", e)))
    }

    pub async fn mark_alert_read(&self, alert_id: i32) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r#"UPDATE media_quota_alerts SET is_read = TRUE WHERE id = $1"#,
        )
        .bind(alert_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to mark alert read: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_usage_stats(&self, user_id: &str) -> Result<serde_json::Value, ApiError> {
        let quota = self.get_or_create_user_quota(user_id).await?;

        let recent_uploads: i64 = sqlx::query_scalar(
            r#"SELECT COALESCE(SUM(file_size_bytes), 0) FROM media_usage_log 
               WHERE user_id = $1 AND operation = 'upload' AND timestamp > NOW() - INTERVAL '7 days'"#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        Ok(serde_json::json!({
            "current_storage_bytes": quota.current_storage_bytes,
            "current_files_count": quota.current_files_count,
            "recent_uploads_bytes": recent_uploads,
        }))
    }
}
