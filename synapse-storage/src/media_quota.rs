use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use synapse_common::ApiError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MediaQuotaConfig {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub max_storage_bytes: i64,
    pub max_file_size_bytes: i64,
    pub max_files_count: i32,
    pub allowed_mime_types: serde_json::Value,
    pub blocked_mime_types: serde_json::Value,
    pub is_default: bool,
    pub is_enabled: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserMediaQuota {
    pub id: i64,
    pub user_id: String,
    pub quota_config_id: Option<i64>,
    pub custom_max_storage_bytes: Option<i64>,
    pub custom_max_file_size_bytes: Option<i64>,
    pub custom_max_files_count: Option<i32>,
    pub current_storage_bytes: i64,
    pub current_files_count: i32,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MediaUsageLog {
    pub id: i64,
    pub user_id: String,
    pub media_id: String,
    pub file_size_bytes: i64,
    pub mime_type: Option<String>,
    pub operation: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MediaQuotaAlert {
    pub id: i64,
    pub user_id: String,
    pub alert_type: String,
    pub threshold_percent: i32,
    pub current_usage_bytes: i64,
    pub quota_limit_bytes: i64,
    pub message: Option<String>,
    pub is_read: bool,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServerMediaQuota {
    pub id: i64,
    pub max_storage_bytes: Option<i64>,
    pub max_file_size_bytes: Option<i64>,
    pub max_files_count: Option<i32>,
    pub current_storage_bytes: i64,
    pub current_files_count: i32,
    pub alert_threshold_percent: i32,
    pub updated_ts: i64,
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
    pub quota_config_id: Option<i64>,
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
    #[serde(rename = "allowed")]
    pub is_allowed: bool,
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
            r"SELECT id, name, description, max_storage_bytes, max_file_size_bytes, max_files_count, allowed_mime_types, blocked_mime_types, is_default, is_enabled, created_ts, updated_ts FROM media_quota_config WHERE is_default = TRUE AND is_enabled = TRUE LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get default quota config", &e))?;

        Ok(config)
    }

    pub async fn get_config(&self, config_id: i64) -> Result<Option<MediaQuotaConfig>, ApiError> {
        let config = sqlx::query_as::<_, MediaQuotaConfig>(r"SELECT id, name, description, max_storage_bytes, max_file_size_bytes, max_files_count, allowed_mime_types, blocked_mime_types, is_default, is_enabled, created_ts, updated_ts FROM media_quota_config WHERE id = $1")
            .bind(config_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get quota config", &e))?;

        Ok(config)
    }

    pub async fn create_config(&self, request: CreateQuotaConfigRequest) -> Result<MediaQuotaConfig, ApiError> {
        let now = Utc::now().timestamp_millis();
        let allowed_mime_types =
            serde_json::to_value(request.allowed_mime_types.unwrap_or_default()).unwrap_or(serde_json::json!([]));
        let blocked_mime_types =
            serde_json::to_value(request.blocked_mime_types.unwrap_or_default()).unwrap_or(serde_json::json!([]));

        if request.is_default.unwrap_or(false) {
            sqlx::query(r"UPDATE media_quota_config SET is_default = FALSE WHERE is_default = TRUE")
                .execute(&self.pool)
                .await
                .ok();
        }

        let config = sqlx::query_as::<_, MediaQuotaConfig>(
            r"
            INSERT INTO media_quota_config (
                config_name, name, description, max_storage_bytes, max_file_size_bytes,
                max_files_count, allowed_mime_types, blocked_mime_types, is_default, created_ts
            )
            VALUES ($1, $1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            ",
        )
        .bind(&request.name)
        .bind(&request.description)
        .bind(request.max_storage_bytes)
        .bind(request.max_file_size_bytes)
        .bind(request.max_files_count)
        .bind(&allowed_mime_types)
        .bind(&blocked_mime_types)
        .bind(request.is_default.unwrap_or(false))
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create quota config", &e))?;

        Ok(config)
    }

    pub async fn list_configs(&self) -> Result<Vec<MediaQuotaConfig>, ApiError> {
        let configs = sqlx::query_as::<_, MediaQuotaConfig>(
            r"SELECT id, name, description, max_storage_bytes, max_file_size_bytes, max_files_count, allowed_mime_types, blocked_mime_types, is_default, is_enabled, created_ts, updated_ts FROM media_quota_config WHERE is_enabled = TRUE ORDER BY created_ts DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to list quota configs", &e))?;

        Ok(configs)
    }

    pub async fn delete_config(&self, config_id: i64) -> Result<bool, ApiError> {
        let result = sqlx::query(r"UPDATE media_quota_config SET is_enabled = FALSE WHERE id = $1 AND is_enabled = TRUE")
            .bind(config_id)
            .execute(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete quota config", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_user_quota(&self, user_id: &str) -> Result<Option<UserMediaQuota>, ApiError> {
        let quota = sqlx::query_as::<_, UserMediaQuota>(r"SELECT id, user_id, quota_config_id, custom_max_storage_bytes, custom_max_file_size_bytes, custom_max_files_count, current_storage_bytes, current_files_count, created_ts, updated_ts FROM user_media_quota WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get user quota", &e))?;

        Ok(quota)
    }

    pub async fn get_or_create_user_quota(&self, user_id: &str) -> Result<UserMediaQuota, ApiError> {
        if let Some(quota) = self.get_user_quota(user_id).await? {
            return Ok(quota);
        }

        let default_config = self.get_default_config().await?;
        let quota_config_id = default_config.map(|c| c.id);
        let now = chrono::Utc::now().timestamp_millis();

        let quota = sqlx::query_as::<_, UserMediaQuota>(
            r"
            INSERT INTO user_media_quota (user_id, quota_config_id, created_ts, updated_ts)
            VALUES ($1, $2, $3, $3)
            RETURNING *
            ",
        )
        .bind(user_id)
        .bind(quota_config_id)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create user quota", &e))?;

        Ok(quota)
    }

    pub async fn set_user_quota(&self, request: SetUserQuotaRequest) -> Result<UserMediaQuota, ApiError> {
        let now = Utc::now().timestamp_millis();

        let quota = sqlx::query_as::<_, UserMediaQuota>(
            r"
            INSERT INTO user_media_quota (
                user_id, quota_config_id, custom_max_storage_bytes,
                custom_max_file_size_bytes, custom_max_files_count, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $6)
            ON CONFLICT (user_id)
            DO UPDATE SET
                quota_config_id = $2,
                custom_max_storage_bytes = $3,
                custom_max_file_size_bytes = $4,
                custom_max_files_count = $5,
                updated_ts = $6
            RETURNING *
            ",
        )
        .bind(&request.user_id)
        .bind(request.quota_config_id)
        .bind(request.custom_max_storage_bytes)
        .bind(request.custom_max_file_size_bytes)
        .bind(request.custom_max_files_count)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to set user quota", &e))?;

        Ok(quota)
    }

    pub async fn update_usage(&self, request: UpdateUsageRequest) -> Result<(), ApiError> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r"
            INSERT INTO media_usage_log (user_id, media_id, file_size_bytes, mime_type, operation, timestamp)
            VALUES ($1, $2, $3, $4, $5, $6)
            ",
        )
        .bind(&request.user_id)
        .bind(&request.media_id)
        .bind(request.file_size_bytes)
        .bind(&request.mime_type)
        .bind(&request.operation)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to log media usage", &e))?;

        let delta = if request.operation == "upload" {
            request.file_size_bytes
        } else if request.operation == "delete" {
            -request.file_size_bytes
        } else {
            0
        };

        sqlx::query(
            r"
            INSERT INTO user_media_quota (
                user_id, current_storage_bytes, current_files_count, created_ts, updated_ts
            )
            VALUES ($1, $2, 1, $4, $4)
            ON CONFLICT (user_id)
            DO UPDATE SET
                current_storage_bytes = GREATEST(0, user_media_quota.current_storage_bytes + $2),
                current_files_count = CASE
                    WHEN $3 = 'upload' THEN user_media_quota.current_files_count + 1
                    WHEN $3 = 'delete' THEN GREATEST(0, user_media_quota.current_files_count - 1)
                    ELSE user_media_quota.current_files_count
                END,
                updated_ts = $4
            ",
        )
        .bind(&request.user_id)
        .bind(delta)
        .bind(&request.operation)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update user quota usage", &e))?;

        sqlx::query(
            r"
            UPDATE server_media_quota
            SET current_storage_bytes = GREATEST(0, current_storage_bytes + $1),
                current_files_count = CASE
                    WHEN $2 = 'upload' THEN current_files_count + 1
                    WHEN $2 = 'delete' THEN GREATEST(0, current_files_count - 1)
                    ELSE current_files_count
                END,
                updated_ts = $3
            WHERE id = 1
            ",
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

        let max_storage = if let Some(custom) = user_quota.custom_max_storage_bytes {
            custom
        } else if let Some(config_id) = user_quota.quota_config_id {
            self.get_config(config_id).await.ok().flatten().map_or(0, |c| c.max_storage_bytes)
        } else {
            0
        };

        if max_storage == 0 {
            return Ok(QuotaCheckResult {
                is_allowed: true,
                reason: None,
                current_usage: user_quota.current_storage_bytes,
                quota_limit: max_storage,
                usage_percent: 0.0,
            });
        }

        let new_usage = user_quota.current_storage_bytes + file_size;
        let is_allowed = new_usage <= max_storage;
        let usage_percent = (user_quota.current_storage_bytes as f64 / max_storage as f64) * 100.0;

        Ok(QuotaCheckResult {
            is_allowed,
            reason: if !is_allowed { Some("Quota exceeded".to_string()) } else { None },
            current_usage: user_quota.current_storage_bytes,
            quota_limit: max_storage,
            usage_percent,
        })
    }

    pub async fn get_server_quota(&self) -> Result<ServerMediaQuota, ApiError> {
        let quota = sqlx::query_as::<_, ServerMediaQuota>(r"SELECT id, max_storage_bytes, max_file_size_bytes, max_files_count, current_storage_bytes, current_files_count, alert_threshold_percent, updated_ts FROM server_media_quota WHERE id = 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get server quota", &e))?;

        Ok(quota)
    }

    pub async fn update_server_quota(
        &self,
        max_storage_bytes: Option<i64>,
        max_file_size_bytes: Option<i64>,
        max_files_count: Option<i32>,
        alert_threshold_percent: Option<i32>,
    ) -> Result<ServerMediaQuota, ApiError> {
        let now = Utc::now().timestamp_millis();

        let quota = sqlx::query_as::<_, ServerMediaQuota>(
            r"
            UPDATE server_media_quota
            SET
                max_storage_bytes = COALESCE($1, max_storage_bytes),
                max_file_size_bytes = COALESCE($2, max_file_size_bytes),
                max_files_count = COALESCE($3, max_files_count),
                alert_threshold_percent = COALESCE($4, alert_threshold_percent),
                updated_ts = $5
            WHERE id = 1
            RETURNING *
            ",
        )
        .bind(max_storage_bytes)
        .bind(max_file_size_bytes)
        .bind(max_files_count)
        .bind(alert_threshold_percent)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update server quota", &e))?;

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
        let now = Utc::now().timestamp_millis();
        let alert = sqlx::query_as::<_, MediaQuotaAlert>(
            r"
            INSERT INTO media_quota_alerts (
                user_id, alert_type, threshold_percent, current_usage_bytes,
                quota_limit_bytes, message, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            ",
        )
        .bind(user_id)
        .bind(alert_type)
        .bind(threshold_percent)
        .bind(current_usage)
        .bind(quota_limit)
        .bind(message)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create quota alert", &e))?;

        Ok(alert)
    }

    pub async fn get_user_alerts(&self, user_id: &str, unread_only: bool) -> Result<Vec<MediaQuotaAlert>, ApiError> {
        let alerts = if unread_only {
            sqlx::query_as::<_, MediaQuotaAlert>(
                r"SELECT id, user_id, alert_type, threshold_percent, current_usage_bytes, quota_limit_bytes, message, is_read, created_ts FROM media_quota_alerts WHERE user_id = $1 AND is_read = FALSE ORDER BY created_ts DESC",
            )
            .bind(user_id)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, MediaQuotaAlert>(
                r"SELECT id, user_id, alert_type, threshold_percent, current_usage_bytes, quota_limit_bytes, message, is_read, created_ts FROM media_quota_alerts WHERE user_id = $1 ORDER BY created_ts DESC",
            )
            .bind(user_id)
            .fetch_all(&self.pool)
            .await
        };

        alerts.map_err(|e| ApiError::internal_with_log("Failed to get user alerts", &e))
    }

    pub async fn mark_alert_read(&self, alert_id: i64) -> Result<bool, ApiError> {
        let result = sqlx::query(r"UPDATE media_quota_alerts SET is_read = TRUE WHERE id = $1 AND is_read = FALSE")
            .bind(alert_id)
            .execute(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to mark alert read", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_usage_stats(&self, user_id: &str) -> Result<serde_json::Value, ApiError> {
        let quota = self.get_or_create_user_quota(user_id).await?;
        let seven_days_ago = Utc::now().timestamp_millis() - (7 * 24 * 60 * 60 * 1000);

        let recent_uploads: i64 = sqlx::query_scalar(
            r"SELECT COALESCE(SUM(file_size_bytes), 0)::BIGINT FROM media_usage_log
               WHERE user_id = $1 AND operation = 'upload' AND timestamp > $2",
        )
        .bind(user_id)
        .bind(seven_days_ago)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to aggregate usage stats", &e))?;

        Ok(serde_json::json!({
            "current_storage_bytes": quota.current_storage_bytes,
            "current_files_count": quota.current_files_count,
            "recent_uploads_bytes": recent_uploads,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_quota_config() -> MediaQuotaConfig {
        MediaQuotaConfig {
            id: 1,
            name: "default".to_string(),
            description: Some("Default quota".to_string()),
            max_storage_bytes: 1073741824,
            max_file_size_bytes: 104857600,
            max_files_count: 1000,
            allowed_mime_types: serde_json::json!(["image/*", "video/*"]),
            blocked_mime_types: serde_json::json!(["application/exe"]),
            is_default: true,
            is_enabled: true,
            created_ts: Utc::now().timestamp_millis(),
            updated_ts: Some(Utc::now().timestamp_millis()),
        }
    }

    fn create_test_user_quota() -> UserMediaQuota {
        UserMediaQuota {
            id: 1,
            user_id: "@user:example.com".to_string(),
            quota_config_id: Some(1),
            custom_max_storage_bytes: None,
            custom_max_file_size_bytes: None,
            custom_max_files_count: None,
            current_storage_bytes: 524288000,
            current_files_count: 50,
            created_ts: Utc::now().timestamp_millis(),
            updated_ts: Some(Utc::now().timestamp_millis()),
        }
    }

    fn create_test_media_alert() -> MediaQuotaAlert {
        MediaQuotaAlert {
            id: 1,
            user_id: "@user:example.com".to_string(),
            alert_type: "warning".to_string(),
            threshold_percent: 80,
            current_usage_bytes: 858993459,
            quota_limit_bytes: 1073741824,
            message: Some("Storage usage at 80%".to_string()),
            is_read: false,
            created_ts: Utc::now().timestamp_millis(),
        }
    }

    #[test]
    fn test_quota_config_creation() {
        let config = create_test_quota_config();
        assert_eq!(config.name, "default");
        assert_eq!(config.max_storage_bytes, 1073741824);
        assert!(config.is_default);
        assert!(config.is_enabled);
    }

    #[test]
    fn test_user_quota_creation() {
        let quota = create_test_user_quota();
        assert_eq!(quota.user_id, "@user:example.com");
        assert_eq!(quota.current_storage_bytes, 524288000);
        assert_eq!(quota.current_files_count, 50);
    }

    #[test]
    fn test_media_alert_creation() {
        let alert = create_test_media_alert();
        assert_eq!(alert.alert_type, "warning");
        assert_eq!(alert.threshold_percent, 80);
        assert!(!alert.is_read);
    }

    #[test]
    fn test_create_quota_config_request() {
        let request = CreateQuotaConfigRequest {
            name: "premium".to_string(),
            description: Some("Premium quota".to_string()),
            max_storage_bytes: 10737418240,
            max_file_size_bytes: 524288000,
            max_files_count: 5000,
            allowed_mime_types: Some(vec!["*".to_string()]),
            blocked_mime_types: None,
            is_default: Some(false),
        };
        assert_eq!(request.name, "premium");
        assert_eq!(request.max_storage_bytes, 10737418240);
    }

    #[test]
    fn test_set_user_quota_request() {
        let request = SetUserQuotaRequest {
            user_id: "@user:example.com".to_string(),
            quota_config_id: Some(1),
            custom_max_storage_bytes: Some(2147483648),
            custom_max_file_size_bytes: None,
            custom_max_files_count: None,
        };
        assert_eq!(request.user_id, "@user:example.com");
        assert!(request.custom_max_storage_bytes.is_some());
    }

    #[test]
    fn test_update_usage_request() {
        let request = UpdateUsageRequest {
            user_id: "@user:example.com".to_string(),
            media_id: "media123".to_string(),
            file_size_bytes: 1048576,
            mime_type: Some("image/png".to_string()),
            operation: "upload".to_string(),
        };
        assert_eq!(request.operation, "upload");
        assert_eq!(request.file_size_bytes, 1048576);
    }

    #[test]
    fn test_quota_check_result() {
        let result = QuotaCheckResult {
            is_allowed: true,
            reason: None,
            current_usage: 524288000,
            quota_limit: 1073741824,
            usage_percent: 48.8,
        };
        assert!(result.is_allowed);
        assert!(result.reason.is_none());
        assert!(result.usage_percent < 100.0);
    }

    #[test]
    fn test_quota_check_result_exceeded() {
        let result = QuotaCheckResult {
            is_allowed: false,
            reason: Some("Quota exceeded".to_string()),
            current_usage: 1073741824,
            quota_limit: 1073741824,
            usage_percent: 100.0,
        };
        assert!(!result.is_allowed);
        assert!(result.reason.is_some());
    }

    #[test]
    fn test_server_media_quota() {
        let quota = ServerMediaQuota {
            id: 1,
            max_storage_bytes: Some(1099511627776),
            max_file_size_bytes: Some(1073741824),
            max_files_count: Some(100000),
            current_storage_bytes: 549755813888,
            current_files_count: 25000,
            alert_threshold_percent: 90,
            updated_ts: Utc::now().timestamp_millis(),
        };
        assert_eq!(quota.max_storage_bytes, Some(1099511627776));
        assert_eq!(quota.alert_threshold_percent, 90);
    }

    #[test]
    fn test_usage_percent_calculation() {
        let current: i64 = 524288000;
        let limit: i64 = 1073741824;
        let percent = (current as f64 / limit as f64) * 100.0;
        assert!(percent > 48.0 && percent < 49.0);
    }

    #[test]
    fn test_mime_type_validation() {
        let allowed = ["image/*", "video/*", "application/pdf"];
        let blocked = ["application/exe", "application/bat"];

        assert!(allowed.contains(&"image/*"));
        assert!(blocked.contains(&"application/exe"));
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use std::sync::Arc;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn ensure_test_user(pool: &PgPool, user_id: &str) {
        let username = user_id
            .strip_prefix('@')
            .and_then(|u| u.split(':').next())
            .unwrap_or("testuser");
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .execute(pool)
        .await
        .ok();
    }

    async fn ensure_server_quota_row(pool: &PgPool) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO server_media_quota (id, current_storage_bytes, current_files_count, alert_threshold_percent, updated_ts) VALUES (1, 0, 0, 80, $1) ON CONFLICT (id) DO NOTHING",
        )
        .bind(now)
        .execute(pool)
        .await
        .ok();
    }

    async fn cleanup_test_data(pool: &PgPool, suffix: &str) {
        let pattern = format!("%{suffix}%");
        sqlx::query("DELETE FROM media_quota_alerts WHERE user_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM media_usage_log WHERE user_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM user_media_quota WHERE user_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM media_quota_config WHERE config_name LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await
            .ok();
    }

    // —— get_default_config ——

    #[tokio::test]
    async fn test_get_default_config_found() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let config_name = format!("mq_default_config_{suffix}");

        cleanup_test_data(&pool, &suffix).await;
        // Directly insert a default enabled config so get_default_config can find it.
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO media_quota_config (config_name, name, max_storage_bytes, max_file_size_bytes, max_files_count, allowed_mime_types, blocked_mime_types, is_default, is_enabled, created_ts) VALUES ($1, $1, 1073741824, 10485760, 1000, '[]'::jsonb, '[]'::jsonb, TRUE, TRUE, $2)",
        )
        .bind(&config_name)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .expect("should insert default config");

        let result = storage
            .get_default_config()
            .await
            .expect("should succeed");
        assert!(result.is_some(), "default config should be found");
        let config = result.unwrap();
        assert_eq!(config.name, config_name);
        assert!(config.is_default);
        assert!(config.is_enabled);

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_default_config_not_found() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_test_data(&pool, &suffix).await;

        let result = storage
            .get_default_config()
            .await
            .expect("should succeed");
        assert!(result.is_none(), "should be None when no default config exists");

        cleanup_test_data(&pool, &suffix).await;
    }

    // —— CRUD: create_config / get_config / list_configs / delete_config ——

    #[tokio::test]
    async fn test_create_config() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let config_name = format!("mq_crud_{suffix}");

        cleanup_test_data(&pool, &suffix).await;

        let request = CreateQuotaConfigRequest {
            name: config_name.clone(),
            description: Some("CRUD test config".to_string()),
            max_storage_bytes: 5_000_000,
            max_file_size_bytes: 1_000_000,
            max_files_count: 500,
            allowed_mime_types: Some(vec!["image/png".to_string()]),
            blocked_mime_types: Some(vec!["application/exe".to_string()]),
            is_default: Some(false),
        };

        let config = storage
            .create_config(request)
            .await
            .expect("should create config");

        assert_eq!(config.name, config_name);
        assert_eq!(config.max_storage_bytes, 5_000_000);
        assert_eq!(config.max_file_size_bytes, 1_000_000);
        assert_eq!(config.max_files_count, 500);
        assert!(!config.is_default);
        assert!(config.is_enabled);
        assert!(config.id > 0);

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_config() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let config_name = format!("mq_get_{suffix}");

        cleanup_test_data(&pool, &suffix).await;

        let request = CreateQuotaConfigRequest {
            name: config_name.clone(),
            description: None,
            max_storage_bytes: 10_000_000,
            max_file_size_bytes: 2_000_000,
            max_files_count: 200,
            allowed_mime_types: None,
            blocked_mime_types: None,
            is_default: Some(false),
        };
        let created = storage
            .create_config(request)
            .await
            .expect("should create config");

        let fetched = storage
            .get_config(created.id)
            .await
            .expect("should succeed");
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.name, config_name);
        assert_eq!(fetched.max_storage_bytes, 10_000_000);

        // get_config for non-existent id
        let missing = storage
            .get_config(99999999)
            .await
            .expect("should succeed");
        assert!(missing.is_none());

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_list_configs() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let name_a = format!("mq_list_a_{suffix}");
        let name_b = format!("mq_list_b_{suffix}");

        cleanup_test_data(&pool, &suffix).await;

        storage
            .create_config(CreateQuotaConfigRequest {
                name: name_a.clone(),
                description: None,
                max_storage_bytes: 1_000_000,
                max_file_size_bytes: 100_000,
                max_files_count: 10,
                allowed_mime_types: None,
                blocked_mime_types: None,
                is_default: Some(false),
            })
            .await
            .expect("should create config A");

        storage
            .create_config(CreateQuotaConfigRequest {
                name: name_b.clone(),
                description: None,
                max_storage_bytes: 2_000_000,
                max_file_size_bytes: 200_000,
                max_files_count: 20,
                allowed_mime_types: None,
                blocked_mime_types: None,
                is_default: Some(false),
            })
            .await
            .expect("should create config B");

        let configs = storage.list_configs().await.expect("should list configs");
        assert!(
            configs.iter().any(|c| c.name == name_a),
            "should contain config A"
        );
        assert!(
            configs.iter().any(|c| c.name == name_b),
            "should contain config B"
        );

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_delete_config() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let config_name = format!("mq_delete_{suffix}");

        cleanup_test_data(&pool, &suffix).await;

        let created = storage
            .create_config(CreateQuotaConfigRequest {
                name: config_name.clone(),
                description: None,
                max_storage_bytes: 1_000_000,
                max_file_size_bytes: 100_000,
                max_files_count: 10,
                allowed_mime_types: None,
                blocked_mime_types: None,
                is_default: Some(false),
            })
            .await
            .expect("should create config");

        let deleted = storage
            .delete_config(created.id)
            .await
            .expect("should succeed");
        assert!(deleted, "delete should return true for existing config");

        // Double-delete should return false (already disabled).
        let deleted_again = storage
            .delete_config(created.id)
            .await
            .expect("should succeed");
        assert!(!deleted_again, "second delete should return false");

        // get_config still returns the row (it only filters by id, not by is_enabled).
        let fetched = storage
            .get_config(created.id)
            .await
            .expect("should succeed");
        assert!(fetched.is_some(), "row still exists but is_enabled=false");
        assert!(!fetched.unwrap().is_enabled);

        cleanup_test_data(&pool, &suffix).await;
    }

    // —— get_user_quota ——

    #[tokio::test]
    async fn test_get_user_quota_found() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_uq_{suffix}:localhost");

        cleanup_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        // Populate a user_media_quota row via set_user_quota (UPSERT).
        storage
            .set_user_quota(SetUserQuotaRequest {
                user_id: user_id.clone(),
                quota_config_id: None,
                custom_max_storage_bytes: Some(50_000_000),
                custom_max_file_size_bytes: None,
                custom_max_files_count: None,
            })
            .await
            .expect("should set user quota");

        let quota = storage
            .get_user_quota(&user_id)
            .await
            .expect("should succeed");
        assert!(quota.is_some(), "user quota should be found");
        let quota = quota.unwrap();
        assert_eq!(quota.user_id, user_id);
        assert_eq!(quota.custom_max_storage_bytes, Some(50_000_000));

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_user_quota_not_found() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_nf_{suffix}:localhost");

        cleanup_test_data(&pool, &suffix).await;

        let result = storage
            .get_user_quota(&user_id)
            .await
            .expect("should succeed");
        assert!(result.is_none(), "should be None for unknown user");

        cleanup_test_data(&pool, &suffix).await;
    }

    // —— get_or_create_user_quota ——

    #[tokio::test]
    async fn test_get_or_create_user_quota_creates() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_goc_{suffix}:localhost");

        cleanup_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let quota = storage
            .get_or_create_user_quota(&user_id)
            .await
            .expect("should succeed");
        assert_eq!(quota.user_id, user_id);
        assert!(quota.id > 0);
        // No default config exists, so quota_config_id should be None.
        assert_eq!(quota.quota_config_id, None);

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_or_create_user_quota_returns_existing() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_goe_{suffix}:localhost");

        cleanup_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        // First call creates.
        let first = storage
            .get_or_create_user_quota(&user_id)
            .await
            .expect("should succeed");
        let first_id = first.id;

        // Second call returns existing.
        let second = storage
            .get_or_create_user_quota(&user_id)
            .await
            .expect("should succeed");
        assert_eq!(second.id, first_id, "should return the same row");
        assert_eq!(second.user_id, first.user_id);

        cleanup_test_data(&pool, &suffix).await;
    }

    // —— set_user_quota ——

    #[tokio::test]
    async fn test_set_user_quota_sets_custom_limit() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_sql_{suffix}:localhost");

        cleanup_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let quota = storage
            .set_user_quota(SetUserQuotaRequest {
                user_id: user_id.clone(),
                quota_config_id: Some(42),
                custom_max_storage_bytes: Some(100_000_000),
                custom_max_file_size_bytes: Some(10_000_000),
                custom_max_files_count: Some(1000),
            })
            .await
            .expect("should set quota");

        assert_eq!(quota.user_id, user_id);
        assert_eq!(quota.quota_config_id, Some(42));
        assert_eq!(quota.custom_max_storage_bytes, Some(100_000_000));
        assert_eq!(quota.custom_max_file_size_bytes, Some(10_000_000));
        assert_eq!(quota.custom_max_files_count, Some(1000));

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_set_user_quota_updates_defaults() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_sqd_{suffix}:localhost");

        cleanup_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        // Set full custom limits first.
        storage
            .set_user_quota(SetUserQuotaRequest {
                user_id: user_id.clone(),
                quota_config_id: Some(10),
                custom_max_storage_bytes: Some(50_000_000),
                custom_max_file_size_bytes: Some(5_000_000),
                custom_max_files_count: Some(500),
            })
            .await
            .expect("should set initial quota");

        // Update with only partial fields — unset fields become None.
        let updated = storage
            .set_user_quota(SetUserQuotaRequest {
                user_id: user_id.clone(),
                quota_config_id: Some(20),
                custom_max_storage_bytes: None,
                custom_max_file_size_bytes: None,
                custom_max_files_count: Some(200),
            })
            .await
            .expect("should update quota");

        assert_eq!(updated.quota_config_id, Some(20));
        assert_eq!(updated.custom_max_storage_bytes, None);
        assert_eq!(updated.custom_max_file_size_bytes, None);
        assert_eq!(updated.custom_max_files_count, Some(200));

        cleanup_test_data(&pool, &suffix).await;
    }

    // —— update_usage ——

    #[tokio::test]
    async fn test_update_usage_upload_increments() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_up_{suffix}:localhost");
        let media_id = format!("media_up_{suffix}");

        cleanup_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;
        ensure_server_quota_row(&pool).await;

        storage
            .update_usage(UpdateUsageRequest {
                user_id: user_id.clone(),
                media_id: media_id.clone(),
                file_size_bytes: 500_000,
                mime_type: Some("image/png".to_string()),
                operation: "upload".to_string(),
            })
            .await
            .expect("should log upload");

        let quota = storage
            .get_user_quota(&user_id)
            .await
            .expect("should succeed")
            .expect("user quota should exist");
        assert_eq!(quota.current_storage_bytes, 500_000);
        assert_eq!(quota.current_files_count, 1);

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_update_usage_multiple_accumulates() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_ma_{suffix}:localhost");
        let media_a = format!("media_ma_a_{suffix}");
        let media_b = format!("media_ma_b_{suffix}");

        cleanup_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;
        ensure_server_quota_row(&pool).await;

        // First upload creates the user_media_quota row.
        storage
            .update_usage(UpdateUsageRequest {
                user_id: user_id.clone(),
                media_id: media_a,
                file_size_bytes: 200_000,
                mime_type: Some("image/png".to_string()),
                operation: "upload".to_string(),
            })
            .await
            .expect("should log first upload");

        // Second upload accumulates on top.
        storage
            .update_usage(UpdateUsageRequest {
                user_id: user_id.clone(),
                media_id: media_b,
                file_size_bytes: 300_000,
                mime_type: Some("image/jpeg".to_string()),
                operation: "upload".to_string(),
            })
            .await
            .expect("should log second upload");

        let quota = storage
            .get_user_quota(&user_id)
            .await
            .expect("should succeed")
            .expect("user quota should exist");
        assert_eq!(quota.current_storage_bytes, 500_000);
        assert_eq!(quota.current_files_count, 2);

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_update_usage_delete_decrements() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_del_{suffix}:localhost");
        let media_id = format!("media_del_{suffix}");

        cleanup_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;
        ensure_server_quota_row(&pool).await;

        // Upload first.
        storage
            .update_usage(UpdateUsageRequest {
                user_id: user_id.clone(),
                media_id: media_id.clone(),
                file_size_bytes: 1_000_000,
                mime_type: Some("video/mp4".to_string()),
                operation: "upload".to_string(),
            })
            .await
            .expect("should log upload");

        // Delete.
        storage
            .update_usage(UpdateUsageRequest {
                user_id: user_id.clone(),
                media_id: format!("{media_id}_del"),
                file_size_bytes: 600_000,
                mime_type: Some("video/mp4".to_string()),
                operation: "delete".to_string(),
            })
            .await
            .expect("should log delete");

        let quota = storage
            .get_user_quota(&user_id)
            .await
            .expect("should succeed")
            .expect("user quota should exist");
        assert_eq!(quota.current_storage_bytes, 400_000);
        assert_eq!(quota.current_files_count, 0);

        cleanup_test_data(&pool, &suffix).await;
    }

    // —— check_quota ——

    #[tokio::test]
    async fn test_check_quota_allowed() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_ca_{suffix}:localhost");

        cleanup_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        // Set a custom storage limit of 100_000.
        storage
            .set_user_quota(SetUserQuotaRequest {
                user_id: user_id.clone(),
                quota_config_id: None,
                custom_max_storage_bytes: Some(100_000),
                custom_max_file_size_bytes: None,
                custom_max_files_count: None,
            })
            .await
            .expect("should set quota");

        // Current usage is 0 (default), check a 50_000-byte file.
        let result = storage
            .check_quota(&user_id, 50_000)
            .await
            .expect("should check quota");
        assert!(result.is_allowed, "should be allowed under limit");
        assert_eq!(result.quota_limit, 100_000);
        assert_eq!(result.current_usage, 0);

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_check_quota_exceeded() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_ce_{suffix}:localhost");

        cleanup_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;
        ensure_server_quota_row(&pool).await;

        // Set a low custom storage limit.
        storage
            .set_user_quota(SetUserQuotaRequest {
                user_id: user_id.clone(),
                quota_config_id: None,
                custom_max_storage_bytes: Some(1_000),
                custom_max_file_size_bytes: None,
                custom_max_files_count: None,
            })
            .await
            .expect("should set quota");

        // Upload 900 bytes.
        storage
            .update_usage(UpdateUsageRequest {
                user_id: user_id.clone(),
                media_id: format!("media_ce_{suffix}"),
                file_size_bytes: 900,
                mime_type: None,
                operation: "upload".to_string(),
            })
            .await
            .expect("should log upload");

        // Check with 200 more bytes -> 1100 > 1000.
        let result = storage
            .check_quota(&user_id, 200)
            .await
            .expect("should check quota");
        assert!(!result.is_allowed, "should be exceeded");
        assert!(result.reason.is_some());
        assert_eq!(result.quota_limit, 1_000);
        assert_eq!(result.current_usage, 900);

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_check_quota_no_limit_always_allowed() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_cz_{suffix}:localhost");

        cleanup_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        // No quota config, no custom limits — max_storage = 0 — always allowed.
        let result = storage
            .check_quota(&user_id, 9_999_999_999)
            .await
            .expect("should check quota");
        assert!(result.is_allowed, "should always be allowed when limit is 0");
        assert_eq!(result.quota_limit, 0);
        assert!(result.reason.is_none());

        cleanup_test_data(&pool, &suffix).await;
    }

    // —— server_quota ——

    #[tokio::test]
    async fn test_get_server_quota() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);

        ensure_server_quota_row(&pool).await;

        let quota = storage
            .get_server_quota()
            .await
            .expect("should succeed");
        assert_eq!(quota.id, 1);
        // alert_threshold_percent may differ from the insert default if a
        // pre-existing row was already present (ON CONFLICT DO NOTHING).
        assert!(quota.alert_threshold_percent > 0, "should have a threshold");
    }

    #[tokio::test]
    async fn test_update_server_quota() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);

        ensure_server_quota_row(&pool).await;

        let updated = storage
            .update_server_quota(
                Some(500_000_000_000_i64),
                Some(100_000_000_i64),
                Some(50000_i32),
                Some(95_i32),
            )
            .await
            .expect("should update server quota");

        assert_eq!(updated.max_storage_bytes, Some(500_000_000_000_i64));
        assert_eq!(updated.max_file_size_bytes, Some(100_000_000_i64));
        assert_eq!(updated.max_files_count, Some(50000_i32));
        assert_eq!(updated.alert_threshold_percent, 95);

        // Verify persisted.
        let fetched = storage
            .get_server_quota()
            .await
            .expect("should succeed");
        assert_eq!(fetched.max_storage_bytes, Some(500_000_000_000_i64));
        assert_eq!(fetched.alert_threshold_percent, 95);
    }

    // —— create_alert / get_user_alerts ——

    #[tokio::test]
    async fn test_create_alert_and_get_user_alerts() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_alert_{suffix}:localhost");

        cleanup_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let alert = storage
            .create_alert(
                &user_id,
                "warning",
                80,
                800_000,
                1_000_000,
                Some("Storage at 80%"),
            )
            .await
            .expect("should create alert");

        assert_eq!(alert.user_id, user_id);
        assert_eq!(alert.alert_type, "warning");
        assert_eq!(alert.threshold_percent, 80);
        assert!(!alert.is_read);
        assert!(alert.id > 0);

        // Retrieve all alerts for the user.
        let alerts = storage
            .get_user_alerts(&user_id, false)
            .await
            .expect("should get alerts");
        assert!(!alerts.is_empty());
        assert!(alerts.iter().any(|a| a.id == alert.id));

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_user_alerts_unread_only() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_unread_{suffix}:localhost");

        cleanup_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        // Create two alerts.
        let alert1 = storage
            .create_alert(&user_id, "warning", 50, 500_000, 1_000_000, None)
            .await
            .expect("should create alert1");

        let alert2 = storage
            .create_alert(&user_id, "critical", 90, 900_000, 1_000_000, None)
            .await
            .expect("should create alert2");

        // Mark alert2 as read.
        let marked = storage
            .mark_alert_read(alert2.id)
            .await
            .expect("should mark alert read");
        assert!(marked);

        // unread_only = true should only return alert1.
        let unread = storage
            .get_user_alerts(&user_id, true)
            .await
            .expect("should get unread alerts");
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].id, alert1.id);
        assert!(!unread[0].is_read);

        // unread_only = false should return both.
        let all = storage
            .get_user_alerts(&user_id, false)
            .await
            .expect("should get all alerts");
        assert_eq!(all.len(), 2);

        cleanup_test_data(&pool, &suffix).await;
    }

    // —— mark_alert_read ——

    #[tokio::test]
    async fn test_mark_alert_read_already_read() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_mar_{suffix}:localhost");

        cleanup_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let alert = storage
            .create_alert(&user_id, "info", 30, 300_000, 1_000_000, None)
            .await
            .expect("should create alert");

        // First mark works.
        let first = storage
            .mark_alert_read(alert.id)
            .await
            .expect("should succeed");
        assert!(first);

        // Second mark on already-read alert returns false.
        let second = storage
            .mark_alert_read(alert.id)
            .await
            .expect("should succeed");
        assert!(!second);

        cleanup_test_data(&pool, &suffix).await;
    }

    // —— get_usage_stats ——

    #[tokio::test]
    async fn test_get_usage_stats() {
        let pool = test_pool().await;
        let storage = MediaQuotaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mq_stats_{suffix}:localhost");

        cleanup_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;
        ensure_server_quota_row(&pool).await;

        storage
            .update_usage(UpdateUsageRequest {
                user_id: user_id.clone(),
                media_id: format!("media_stats_{suffix}"),
                file_size_bytes: 200_000,
                mime_type: Some("image/png".to_string()),
                operation: "upload".to_string(),
            })
            .await
            .expect("should log upload");

        let stats = storage
            .get_usage_stats(&user_id)
            .await
            .expect("should get usage stats");

        assert_eq!(stats["current_storage_bytes"], 200_000);
        assert_eq!(stats["current_files_count"], 1);
        // recent_uploads_bytes should be >= 200_000 (the upload we just did).
        assert!(stats["recent_uploads_bytes"].as_i64().unwrap() >= 200_000);

        cleanup_test_data(&pool, &suffix).await;
    }
}
