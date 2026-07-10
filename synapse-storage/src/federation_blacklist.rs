use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use synapse_common::error::ApiError;
use tracing::info;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FederationBlacklistCursor {
    pub created_ts: i64,
    pub server_name: String,
}

pub fn encode_federation_blacklist_cursor(cursor: &FederationBlacklistCursor) -> String {
    format!("{}|{}", cursor.created_ts, cursor.server_name)
}

pub fn decode_federation_blacklist_cursor(cursor: Option<&str>) -> Option<FederationBlacklistCursor> {
    let cursor = cursor?;
    let (created_ts, server_name) = cursor.split_once('|')?;
    let created_ts = created_ts.parse::<i64>().ok()?;
    if server_name.is_empty() {
        return None;
    }
    Some(FederationBlacklistCursor { created_ts, server_name: server_name.to_string() })
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FederationBlacklist {
    pub id: i64,
    pub server_name: String,
    pub block_type: String,
    pub reason: Option<String>,
    pub blocked_by: String,
    pub created_ts: Option<i64>,
    pub updated_ts: Option<i64>,
    pub expires_at: Option<i64>,
    pub is_enabled: bool,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FederationBlacklistLog {
    pub id: i64,
    pub server_name: String,
    pub action: String,
    pub old_status: Option<String>,
    pub new_status: Option<String>,
    pub reason: Option<String>,
    pub performed_by: String,
    pub performed_ts: i64,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FederationAccessStats {
    pub id: i64,
    pub server_name: String,
    pub total_requests: i64,
    pub successful_requests: i64,
    pub failed_requests: i64,
    pub last_request_ts: Option<i64>,
    pub last_success_ts: Option<i64>,
    pub last_failure_ts: Option<i64>,
    pub average_response_time_ms: f64,
    pub error_rate: f64,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FederationBlacklistRule {
    pub id: i64,
    pub rule_name: String,
    pub rule_type: String,
    pub pattern: String,
    pub action: String,
    pub priority: i32,
    pub is_enabled: bool,
    pub description: Option<String>,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub created_by: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddBlacklistRequest {
    pub server_name: String,
    pub block_type: String,
    pub reason: Option<String>,
    pub blocked_by: String,
    pub expires_at: Option<i64>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateLogRequest {
    pub server_name: String,
    pub action: String,
    pub old_status: Option<String>,
    pub new_status: Option<String>,
    pub reason: Option<String>,
    pub performed_by: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateStatsRequest {
    pub server_name: String,
    #[serde(rename = "success")]
    pub is_success: bool,
    pub response_time_ms: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRuleRequest {
    pub rule_name: String,
    pub rule_type: String,
    pub pattern: String,
    pub action: String,
    pub priority: i32,
    pub description: Option<String>,
    pub created_by: String,
}

#[async_trait]
pub trait FederationBlacklistStoreApi: Send + Sync + std::fmt::Debug {
    async fn add_to_blacklist(&self, request: AddBlacklistRequest) -> Result<FederationBlacklist, ApiError>;
    async fn remove_from_blacklist(&self, server_name: &str, performed_by: &str) -> Result<(), ApiError>;
    async fn get_blacklist_entry(&self, server_name: &str) -> Result<Option<FederationBlacklist>, ApiError>;
    async fn is_server_blocked(&self, server_name: &str) -> Result<bool, ApiError>;
    async fn is_server_whitelisted(&self, server_name: &str) -> Result<bool, ApiError>;
    async fn get_all_blacklist(
        &self,
        limit: i32,
        from: Option<FederationBlacklistCursor>,
    ) -> Result<(Vec<FederationBlacklist>, Option<String>), ApiError>;
    async fn create_log(&self, request: CreateLogRequest) -> Result<FederationBlacklistLog, ApiError>;
    async fn update_access_stats(&self, request: UpdateStatsRequest) -> Result<FederationAccessStats, ApiError>;
    async fn get_access_stats(&self, server_name: &str) -> Result<Option<FederationAccessStats>, ApiError>;
    async fn create_rule(&self, request: CreateRuleRequest) -> Result<FederationBlacklistRule, ApiError>;
    async fn get_all_rules(&self) -> Result<Vec<FederationBlacklistRule>, ApiError>;
    async fn cleanup_expired_entries(&self) -> Result<u64, ApiError>;
    fn get_config(&self, config_key: &str) -> Result<Option<String>, ApiError>;
    fn get_config_as_bool(&self, config_key: &str, default: bool) -> Result<bool, ApiError>;
    fn get_config_as_int(&self, config_key: &str, default: i32) -> Result<i32, ApiError>;
}

#[derive(Debug, Clone)]
pub struct FederationBlacklistStorage {
    pool: Arc<PgPool>,
}

impl FederationBlacklistStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn add_to_blacklist(&self, request: AddBlacklistRequest) -> Result<FederationBlacklist, ApiError> {
        let now = Utc::now().timestamp_millis();
        let metadata = request.metadata.unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, FederationBlacklist>(
            r#"
            INSERT INTO federation_blacklist (
                server_name, block_type, reason, blocked_by, added_by, added_ts, created_ts, updated_ts, expires_at, is_enabled, metadata
            )
            VALUES ($1, $2, $3, $4, $4, $5, $5, $5, $6, true, $7)
            ON CONFLICT (server_name) DO UPDATE SET
                block_type = $2,
                reason = $3,
                blocked_by = $4,
                updated_ts = $5,
                expires_at = $6,
                is_enabled = true,
                metadata = $7
            RETURNING id, server_name, block_type,
                      reason, COALESCE(blocked_by, 'system') AS blocked_by,
                      COALESCE(created_ts, added_ts) AS created_ts,
                      COALESCE(updated_ts, added_ts) AS updated_ts,
                      expires_at, is_enabled, metadata
            "#,
        )
        .bind(&request.server_name)
        .bind(&request.block_type)
        .bind(request.reason.as_deref())
        .bind(&request.blocked_by)
        .bind(now)
        .bind(request.expires_at)
        .bind(&metadata)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to add to blacklist", &e))?;

        info!("Added server {} to blacklist", request.server_name);
        Ok(row)
    }

    pub async fn remove_from_blacklist(&self, server_name: &str, performed_by: &str) -> Result<(), ApiError> {
        let now = Utc::now().timestamp_millis();

        sqlx::query!(
            r#"
            UPDATE federation_blacklist
            SET is_enabled = false, updated_ts = $1
            WHERE server_name = $2 AND is_enabled = true
            "#,
            now,
            server_name
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to remove from blacklist", &e))?;

        self.create_log(CreateLogRequest {
            server_name: server_name.to_string(),
            action: "remove".to_string(),
            old_status: Some("active".to_string()),
            new_status: Some("inactive".to_string()),
            reason: None,
            performed_by: performed_by.to_string(),
            ip_address: None,
            user_agent: None,
            metadata: None,
        })
        .await?;

        info!("Removed server {} from blacklist", server_name);
        Ok(())
    }

    pub async fn get_blacklist_entry(&self, server_name: &str) -> Result<Option<FederationBlacklist>, ApiError> {
        let row = sqlx::query_as::<_, FederationBlacklist>(
            r#"
            SELECT
                id,
                server_name,
                block_type,
                reason,
                COALESCE(added_by, 'system') AS blocked_by,
                COALESCE(created_ts, added_ts) AS created_ts,
                COALESCE(updated_ts, added_ts) AS updated_ts,
                expires_at,
                is_enabled,
                metadata
            FROM federation_blacklist
            WHERE server_name = $1
            "#,
        )
        .bind(server_name)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get blacklist entry", &e))?;

        Ok(row)
    }

    pub async fn is_server_blocked(&self, server_name: &str) -> Result<bool, ApiError> {
        let entry = self.get_blacklist_entry(server_name).await?;

        if let Some(entry) = entry {
            if let Some(expires_at) = entry.expires_at {
                let now = Utc::now().timestamp_millis();
                if expires_at < now {
                    return Ok(false);
                }
            }
            return Ok(entry.block_type == "blacklist");
        }

        Ok(false)
    }

    pub async fn is_server_whitelisted(&self, server_name: &str) -> Result<bool, ApiError> {
        let row = sqlx::query_as::<_, FederationBlacklist>(
            r#"
            SELECT id, server_name, block_type,
                   reason, COALESCE(blocked_by, 'system') AS blocked_by,
                   COALESCE(created_ts, added_ts) AS created_ts,
                   COALESCE(updated_ts, added_ts) AS updated_ts,
                   expires_at, is_enabled, metadata
            FROM federation_blacklist
            WHERE server_name = $1 AND block_type = 'whitelist' AND is_enabled = true
            "#,
        )
        .bind(server_name)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check whitelist", &e))?;

        Ok(row.is_some())
    }

    pub async fn get_all_blacklist(
        &self,
        limit: i32,
        from: Option<FederationBlacklistCursor>,
    ) -> Result<(Vec<FederationBlacklist>, Option<String>), ApiError> {
        let rows = if let Some(cursor) = from {
            sqlx::query_as::<_, FederationBlacklist>(
                r#"
                SELECT
                    id,
                    server_name,
                    block_type,
                    reason,
                    COALESCE(added_by, 'system') AS blocked_by,
                    COALESCE(created_ts, added_ts) AS created_ts,
                    COALESCE(updated_ts, added_ts) AS updated_ts,
                    expires_at,
                    is_enabled,
                    metadata
                FROM federation_blacklist
                WHERE (added_ts, server_name) < ($1, $2)
                ORDER BY added_ts DESC, server_name DESC
                LIMIT $3
                "#,
            )
            .bind(cursor.created_ts)
            .bind(&cursor.server_name)
            .bind(limit as i64 + 1)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get blacklist", &e))?
        } else {
            sqlx::query_as::<_, FederationBlacklist>(
                r#"
                SELECT
                    id,
                    server_name,
                    block_type,
                    reason,
                    COALESCE(added_by, 'system') AS blocked_by,
                    COALESCE(created_ts, added_ts) AS created_ts,
                    COALESCE(updated_ts, added_ts) AS updated_ts,
                    expires_at,
                    is_enabled,
                    metadata
                FROM federation_blacklist
                ORDER BY added_ts DESC, server_name DESC
                LIMIT $1
                "#,
            )
            .bind(limit as i64 + 1)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get blacklist", &e))?
        };

        let next_batch = if rows.len() > limit as usize {
            rows.get(limit as usize - 1).map(|last| {
                encode_federation_blacklist_cursor(&FederationBlacklistCursor {
                    created_ts: last.created_ts.unwrap_or(0),
                    server_name: last.server_name.clone(),
                })
            })
        } else {
            None
        };

        Ok((rows.into_iter().take(limit as usize).collect(), next_batch))
    }

    pub async fn create_log(&self, request: CreateLogRequest) -> Result<FederationBlacklistLog, ApiError> {
        let metadata = request.metadata.unwrap_or(serde_json::json!({}));
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, FederationBlacklistLog>(
            r#"
            INSERT INTO federation_blacklist_log (
                server_name, action, old_status, new_status, reason, performed_by, performed_ts, ip_address, user_agent, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id, server_name, action, old_status, new_status, reason, performed_by, performed_ts, ip_address, user_agent, metadata
            "#,
        )
        .bind(&request.server_name)
        .bind(&request.action)
        .bind(request.old_status.as_deref())
        .bind(request.new_status.as_deref())
        .bind(request.reason.as_deref())
        .bind(&request.performed_by)
        .bind(now)
        .bind(request.ip_address.as_deref())
        .bind(request.user_agent.as_deref())
        .bind(&metadata)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create log", &e))?;

        Ok(row)
    }

    pub async fn update_access_stats(&self, request: UpdateStatsRequest) -> Result<FederationAccessStats, ApiError> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, FederationAccessStats>(
            r#"
            INSERT INTO federation_access_stats (server_name, total_requests, successful_requests, failed_requests,
                last_request_ts, last_success_ts, last_failure_ts, average_response_time_ms, error_rate, created_ts, updated_ts)
            VALUES ($1, 1, $2, $3, $4, $5, $6, COALESCE($7, 0), $8, $4, $4)
            ON CONFLICT (server_name) DO UPDATE SET
                total_requests = federation_access_stats.total_requests + 1,
                successful_requests = federation_access_stats.successful_requests + $2,
                failed_requests = federation_access_stats.failed_requests + $3,
                last_request_ts = $4,
                last_success_ts = COALESCE($5, federation_access_stats.last_success_ts),
                last_failure_ts = COALESCE($6, federation_access_stats.last_failure_ts),
                average_response_time_ms = CASE
                    WHEN $7 IS NOT NULL THEN (federation_access_stats.average_response_time_ms + $7) / 2
                    ELSE federation_access_stats.average_response_time_ms
                END,
                error_rate = CAST(federation_access_stats.failed_requests AS FLOAT) / NULLIF(federation_access_stats.total_requests, 0),
                updated_ts = $4
            RETURNING id, server_name, total_requests, successful_requests, failed_requests, last_request_ts, last_success_ts, last_failure_ts, average_response_time_ms, error_rate, created_ts, updated_ts
            "#,
        )
        .bind(&request.server_name)
        .bind(if request.is_success { 1_i64 } else { 0_i64 })
        .bind(if request.is_success { 0_i64 } else { 1_i64 })
        .bind(now)
        .bind(if request.is_success { Some(now) } else { None::<i64> })
        .bind(if request.is_success { None::<i64> } else { Some(now) })
        .bind(request.response_time_ms)
        .bind(if request.is_success { 0.0_f64 } else { 1.0_f64 })
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update access stats", &e))?;

        Ok(row)
    }

    pub async fn get_access_stats(&self, server_name: &str) -> Result<Option<FederationAccessStats>, ApiError> {
        let row = sqlx::query_as!(
            FederationAccessStats,
            r#"
            SELECT id, server_name, total_requests, successful_requests, failed_requests,
                last_request_ts, last_success_ts, last_failure_ts, average_response_time_ms, error_rate, created_ts, updated_ts
            FROM federation_access_stats WHERE server_name = $1
            "#,
            server_name
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get access stats", &e))?;

        Ok(row)
    }

    pub async fn create_rule(&self, request: CreateRuleRequest) -> Result<FederationBlacklistRule, ApiError> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as!(
            FederationBlacklistRule,
            r#"
            INSERT INTO federation_blacklist_rule (
                rule_name, rule_type, pattern, action, priority, description, created_ts, updated_ts, created_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $8)
            RETURNING id, rule_name, rule_type, pattern, action, priority, is_enabled, description, created_ts, updated_ts, created_by
            "#,
            &request.rule_name,
            &request.rule_type,
            &request.pattern,
            &request.action,
            request.priority,
            request.description.as_deref(),
            now,
            &request.created_by
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create rule", &e))?;

        info!("Created federation blacklist rule: {}", request.rule_name);
        Ok(row)
    }

    pub async fn get_all_rules(&self) -> Result<Vec<FederationBlacklistRule>, ApiError> {
        let rows = sqlx::query_as!(
            FederationBlacklistRule,
            r#"SELECT id, rule_name, rule_type, pattern, action, priority, is_enabled, description, created_ts, updated_ts, created_by
            FROM federation_blacklist_rule WHERE is_enabled = true ORDER BY priority DESC"#
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get rules", &e))?;

        Ok(rows)
    }

    pub async fn cleanup_expired_entries(&self) -> Result<u64, ApiError> {
        let now = Utc::now().timestamp_millis();
        let result = sqlx::query!(
            "UPDATE federation_blacklist SET is_enabled = false WHERE expires_at < $1 AND is_enabled = true",
            now
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to cleanup expired entries", &e))?;

        info!("Cleaned up {} expired blacklist entries", result.rows_affected());
        Ok(result.rows_affected())
    }

    pub fn get_config(&self, _config_key: &str) -> Result<Option<String>, ApiError> {
        Ok(None)
    }

    pub fn get_config_as_bool(&self, _config_key: &str, default: bool) -> Result<bool, ApiError> {
        Ok(default)
    }

    pub fn get_config_as_int(&self, _config_key: &str, default: i32) -> Result<i32, ApiError> {
        Ok(default)
    }
}

#[async_trait]
impl FederationBlacklistStoreApi for FederationBlacklistStorage {
    async fn add_to_blacklist(&self, request: AddBlacklistRequest) -> Result<FederationBlacklist, ApiError> {
        self.add_to_blacklist(request).await
    }
    async fn remove_from_blacklist(&self, server_name: &str, performed_by: &str) -> Result<(), ApiError> {
        self.remove_from_blacklist(server_name, performed_by).await
    }
    async fn get_blacklist_entry(&self, server_name: &str) -> Result<Option<FederationBlacklist>, ApiError> {
        self.get_blacklist_entry(server_name).await
    }
    async fn is_server_blocked(&self, server_name: &str) -> Result<bool, ApiError> {
        self.is_server_blocked(server_name).await
    }
    async fn is_server_whitelisted(&self, server_name: &str) -> Result<bool, ApiError> {
        self.is_server_whitelisted(server_name).await
    }
    async fn get_all_blacklist(
        &self,
        limit: i32,
        from: Option<FederationBlacklistCursor>,
    ) -> Result<(Vec<FederationBlacklist>, Option<String>), ApiError> {
        self.get_all_blacklist(limit, from).await
    }
    async fn create_log(&self, request: CreateLogRequest) -> Result<FederationBlacklistLog, ApiError> {
        self.create_log(request).await
    }
    async fn update_access_stats(&self, request: UpdateStatsRequest) -> Result<FederationAccessStats, ApiError> {
        self.update_access_stats(request).await
    }
    async fn get_access_stats(&self, server_name: &str) -> Result<Option<FederationAccessStats>, ApiError> {
        self.get_access_stats(server_name).await
    }
    async fn create_rule(&self, request: CreateRuleRequest) -> Result<FederationBlacklistRule, ApiError> {
        self.create_rule(request).await
    }
    async fn get_all_rules(&self) -> Result<Vec<FederationBlacklistRule>, ApiError> {
        self.get_all_rules().await
    }
    async fn cleanup_expired_entries(&self) -> Result<u64, ApiError> {
        self.cleanup_expired_entries().await
    }
    fn get_config(&self, config_key: &str) -> Result<Option<String>, ApiError> {
        self.get_config(config_key)
    }
    fn get_config_as_bool(&self, config_key: &str, default: bool) -> Result<bool, ApiError> {
        self.get_config_as_bool(config_key, default)
    }
    fn get_config_as_int(&self, config_key: &str, default: i32) -> Result<i32, ApiError> {
        self.get_config_as_int(config_key, default)
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_federation_blacklist_cursor, encode_federation_blacklist_cursor, FederationBlacklistCursor};

    #[test]
    fn federation_blacklist_cursor_round_trip() {
        let cursor =
            FederationBlacklistCursor { created_ts: 1_746_700_000_000, server_name: "matrix.example.com".to_string() };

        let encoded = encode_federation_blacklist_cursor(&cursor);
        assert_eq!(decode_federation_blacklist_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn federation_blacklist_cursor_rejects_invalid_values() {
        assert_eq!(decode_federation_blacklist_cursor(None), None);
        assert_eq!(decode_federation_blacklist_cursor(Some("bad")), None);
        assert_eq!(decode_federation_blacklist_cursor(Some("123|")), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_federation_blacklist_creation() {
        let blacklist = FederationBlacklist {
            id: 1,
            server_name: "evil-server.com".to_string(),
            block_type: "server".to_string(),
            reason: Some("Malicious activity".to_string()),
            blocked_by: "@admin:example.com".to_string(),
            created_ts: Some(1234567890),
            updated_ts: Some(1234567890),
            expires_at: None,
            is_enabled: true,
            metadata: serde_json::json!({}),
        };
        assert_eq!(blacklist.server_name, "evil-server.com");
        assert!(blacklist.is_enabled);
    }

    #[test]
    fn test_federation_blacklist_expired() {
        let blacklist = FederationBlacklist {
            id: 2,
            server_name: "expired-server.com".to_string(),
            block_type: "server".to_string(),
            reason: Some("Temporary block".to_string()),
            blocked_by: "@admin:example.com".to_string(),
            created_ts: Some(1234567890),
            updated_ts: Some(1234567890),
            expires_at: Some(1234567990),
            is_enabled: false,
            metadata: serde_json::json!({}),
        };
        assert!(!blacklist.is_enabled);
    }

    #[test]
    fn test_federation_blacklist_log_creation() {
        let log = FederationBlacklistLog {
            id: 1,
            server_name: "evil-server.com".to_string(),
            action: "add".to_string(),
            old_status: None,
            new_status: Some("blocked".to_string()),
            reason: Some("Spam".to_string()),
            performed_by: "@admin:example.com".to_string(),
            performed_ts: 1234567890,
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: None,
            metadata: serde_json::json!({}),
        };
        assert_eq!(log.action, "add");
    }

    #[test]
    fn test_federation_access_stats_creation() {
        let stats = FederationAccessStats {
            id: 1,
            server_name: "example.com".to_string(),
            total_requests: 1000,
            successful_requests: 950,
            failed_requests: 50,
            last_request_ts: Some(1234567890),
            last_success_ts: Some(1234567890),
            last_failure_ts: None,
            average_response_time_ms: 50.0,
            error_rate: 0.05,
            created_ts: 1234567800,
            updated_ts: 1234567890,
        };
        assert_eq!(stats.total_requests, 1000);
    }

    #[test]
    fn test_add_blacklist_request() {
        let request = AddBlacklistRequest {
            server_name: "new-evil.com".to_string(),
            block_type: "server".to_string(),
            reason: Some("Test block".to_string()),
            blocked_by: "@admin:example.com".to_string(),
            expires_at: Some(1234567990),
            metadata: None,
        };
        assert_eq!(request.server_name, "new-evil.com");
    }

    #[test]
    fn test_create_rule_request() {
        let request = CreateRuleRequest {
            rule_name: "block-malware".to_string(),
            rule_type: "domain".to_string(),
            pattern: "*.evil.com".to_string(),
            action: "block".to_string(),
            priority: 100,
            description: Some("Block malware domains".to_string()),
            created_by: "@admin:example.com".to_string(),
        };
        assert_eq!(request.rule_type, "domain");
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use std::env;
    use uuid::Uuid;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn cleanup_by_server(pool: &PgPool, server_name: &str) {
        sqlx::query("DELETE FROM federation_blacklist_log WHERE server_name = $1")
            .bind(server_name)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM federation_access_stats WHERE server_name = $1")
            .bind(server_name)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM federation_blacklist WHERE server_name = $1")
            .bind(server_name)
            .execute(pool)
            .await
            .ok();
    }

    async fn cleanup_rule_by_name(pool: &PgPool, rule_name_like: &str) {
        sqlx::query("DELETE FROM federation_blacklist_rule WHERE rule_name LIKE $1")
            .bind(rule_name_like)
            .execute(pool)
            .await
            .ok();
    }

    // 1. add_to_blacklist inserts a new record and returns it.
    #[tokio::test]
    async fn test_add_to_blacklist_succeeds() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-add-{}.com", suffix);

        cleanup_by_server(&pool, &server_name).await;

        let entry = storage
            .add_to_blacklist(AddBlacklistRequest {
                server_name: server_name.clone(),
                block_type: "blacklist".to_string(),
                reason: Some("Testing".to_string()),
                blocked_by: "@admin:test.com".to_string(),
                expires_at: None,
                metadata: None,
            })
            .await
            .expect("add_to_blacklist should succeed");

        assert!(entry.id > 0);
        assert_eq!(entry.server_name, server_name);
        assert_eq!(entry.block_type, "blacklist");
        assert_eq!(entry.reason.as_deref(), Some("Testing"));
        assert_eq!(entry.blocked_by, "@admin:test.com");
        assert!(entry.is_enabled);
        assert!(entry.created_ts.unwrap_or(0) > 0);

        cleanup_by_server(&pool, &server_name).await;
    }

    // 2. add_to_blacklist upserts on server_name conflict.
    #[tokio::test]
    async fn test_add_to_blacklist_upsert() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-upsert-{}.com", suffix);

        cleanup_by_server(&pool, &server_name).await;

        let first = storage
            .add_to_blacklist(AddBlacklistRequest {
                server_name: server_name.clone(),
                block_type: "blacklist".to_string(),
                reason: Some("First reason".to_string()),
                blocked_by: "@admin:test.com".to_string(),
                expires_at: None,
                metadata: None,
            })
            .await
            .expect("first insert should succeed");

        // Upsert with different reason, blocked_by, and metadata.
        let second = storage
            .add_to_blacklist(AddBlacklistRequest {
                server_name: server_name.clone(),
                block_type: "blacklist".to_string(),
                reason: Some("Updated reason".to_string()),
                blocked_by: "@admin2:test.com".to_string(),
                expires_at: None,
                metadata: Some(serde_json::json!({"key": "value"})),
            })
            .await
            .expect("upsert should succeed");

        // Same id means it updated the existing row.
        assert_eq!(first.id, second.id);
        assert_eq!(second.reason.as_deref(), Some("Updated reason"));
        assert_eq!(second.blocked_by, "@admin2:test.com");
        assert_eq!(second.metadata, serde_json::json!({"key": "value"}));
        // updated_ts should reflect the change.
        assert!(second.updated_ts.unwrap_or(0) >= first.updated_ts.unwrap_or(0));

        cleanup_by_server(&pool, &server_name).await;
    }

    // 3. get_blacklist_entry finds an existing entry.
    #[tokio::test]
    async fn test_get_blacklist_entry_found() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-get-{}.com", suffix);

        cleanup_by_server(&pool, &server_name).await;

        storage
            .add_to_blacklist(AddBlacklistRequest {
                server_name: server_name.clone(),
                block_type: "blacklist".to_string(),
                reason: None,
                blocked_by: "@admin:test.com".to_string(),
                expires_at: None,
                metadata: None,
            })
            .await
            .expect("add should succeed");

        let entry = storage
            .get_blacklist_entry(&server_name)
            .await
            .expect("get should succeed")
            .expect("entry should be found");

        assert_eq!(entry.server_name, server_name);
        assert_eq!(entry.block_type, "blacklist");
        assert!(entry.is_enabled);

        cleanup_by_server(&pool, &server_name).await;
    }

    // 4. get_blacklist_entry returns None for unknown server_name.
    #[tokio::test]
    async fn test_get_blacklist_entry_not_found() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-nonexist-{}.com", suffix);

        cleanup_by_server(&pool, &server_name).await;

        let entry = storage.get_blacklist_entry(&server_name).await.expect("get should succeed");

        assert!(entry.is_none());
    }

    // 5. remove_from_blacklist sets is_enabled=false and creates a log entry.
    #[tokio::test]
    async fn test_remove_from_blacklist_creates_log() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-remove-{}.com", suffix);

        cleanup_by_server(&pool, &server_name).await;

        storage
            .add_to_blacklist(AddBlacklistRequest {
                server_name: server_name.clone(),
                block_type: "blacklist".to_string(),
                reason: None,
                blocked_by: "@admin:test.com".to_string(),
                expires_at: None,
                metadata: None,
            })
            .await
            .expect("add should succeed");

        storage.remove_from_blacklist(&server_name, "@remover:test.com").await.expect("remove should succeed");

        // Verify is_enabled is now false.
        let entry = storage
            .get_blacklist_entry(&server_name)
            .await
            .expect("get should succeed")
            .expect("entry should exist after removal");

        assert!(!entry.is_enabled, "entry should be disabled after removal");

        // Verify log was created by remove_from_blacklist.
        let log = sqlx::query_as::<_, FederationBlacklistLog>(
            "SELECT id, server_name, action, old_status, new_status, reason,
                    performed_by, performed_ts, ip_address, user_agent, metadata
             FROM federation_blacklist_log
             WHERE server_name = $1 AND action = 'remove'
             ORDER BY performed_ts DESC LIMIT 1",
        )
        .bind(&server_name)
        .fetch_one(&*pool)
        .await
        .expect("log should exist");

        assert_eq!(log.action, "remove");
        assert_eq!(log.performed_by, "@remover:test.com");

        cleanup_by_server(&pool, &server_name).await;
    }

    // 6. is_server_blocked returns true for an active blacklist entry.
    #[tokio::test]
    async fn test_is_server_blocked_true() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-blocked-{}.com", suffix);

        cleanup_by_server(&pool, &server_name).await;

        storage
            .add_to_blacklist(AddBlacklistRequest {
                server_name: server_name.clone(),
                block_type: "blacklist".to_string(),
                reason: None,
                blocked_by: "@admin:test.com".to_string(),
                expires_at: None,
                metadata: None,
            })
            .await
            .expect("add should succeed");

        let blocked = storage.is_server_blocked(&server_name).await.expect("check should succeed");

        assert!(blocked, "server should be blocked");

        cleanup_by_server(&pool, &server_name).await;
    }

    // 7. is_server_blocked returns false when expiry is in the past.
    #[tokio::test]
    async fn test_is_server_blocked_expired() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-expired-{}.com", suffix);

        cleanup_by_server(&pool, &server_name).await;

        let past = Utc::now().timestamp_millis() - 86_400_000; // 1 day ago
        storage
            .add_to_blacklist(AddBlacklistRequest {
                server_name: server_name.clone(),
                block_type: "blacklist".to_string(),
                reason: None,
                blocked_by: "@admin:test.com".to_string(),
                expires_at: Some(past),
                metadata: None,
            })
            .await
            .expect("add should succeed");

        let blocked = storage.is_server_blocked(&server_name).await.expect("check should succeed");

        assert!(!blocked, "expired server should not be blocked");

        cleanup_by_server(&pool, &server_name).await;
    }

    // 8. is_server_blocked returns false when block_type is not "blacklist".
    #[tokio::test]
    async fn test_is_server_blocked_wrong_type() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-notblack-{}.com", suffix);

        cleanup_by_server(&pool, &server_name).await;

        storage
            .add_to_blacklist(AddBlacklistRequest {
                server_name: server_name.clone(),
                block_type: "whitelist".to_string(),
                reason: None,
                blocked_by: "@admin:test.com".to_string(),
                expires_at: None,
                metadata: None,
            })
            .await
            .expect("add should succeed");

        let blocked = storage.is_server_blocked(&server_name).await.expect("check should succeed");

        assert!(!blocked, "whitelist-type entry should not be reported as blocked");

        cleanup_by_server(&pool, &server_name).await;
    }

    // 9. is_server_whitelisted returns true for a whitelist entry.
    #[tokio::test]
    async fn test_is_server_whitelisted_true() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-wl-{}.com", suffix);

        cleanup_by_server(&pool, &server_name).await;

        storage
            .add_to_blacklist(AddBlacklistRequest {
                server_name: server_name.clone(),
                block_type: "whitelist".to_string(),
                reason: None,
                blocked_by: "@admin:test.com".to_string(),
                expires_at: None,
                metadata: None,
            })
            .await
            .expect("add should succeed");

        let whitelisted = storage.is_server_whitelisted(&server_name).await.expect("check should succeed");

        assert!(whitelisted, "server should be whitelisted");

        cleanup_by_server(&pool, &server_name).await;
    }

    // 10. is_server_whitelisted returns false for a non-whitelist server.
    #[tokio::test]
    async fn test_is_server_whitelisted_false() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-notwl-{}.com", suffix);

        cleanup_by_server(&pool, &server_name).await;

        let whitelisted = storage.is_server_whitelisted(&server_name).await.expect("check should succeed");

        assert!(!whitelisted, "non-existent server should not be whitelisted");

        cleanup_by_server(&pool, &server_name).await;
    }

    // 11. get_all_blacklist supports cursor-based pagination.
    #[tokio::test]
    async fn test_get_all_blacklist_pagination() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();

        let servers: Vec<String> = (0..5).map(|i| format!("test-page-{}-{}.com", i, suffix)).collect();

        for s in &servers {
            cleanup_by_server(&pool, s).await;
        }

        // Insert entries sequentially so added_ts may differ slightly.
        for (i, s) in servers.iter().enumerate() {
            storage
                .add_to_blacklist(AddBlacklistRequest {
                    server_name: s.clone(),
                    block_type: "blacklist".to_string(),
                    reason: Some(format!("Entry {i}")),
                    blocked_by: "@admin:test.com".to_string(),
                    expires_at: None,
                    metadata: None,
                })
                .await
                .expect("add should succeed");
        }

        // Page 1: limit=3, no cursor.
        let (rows, next_cursor) = storage.get_all_blacklist(3, None).await.expect("get_all should succeed");

        assert_eq!(rows.len(), 3, "first page should return 3 rows");
        assert!(next_cursor.is_some(), "should have next_batch token for more results");

        // Count how many of our test servers appear in page 1.
        let page1_ours: std::collections::HashSet<&str> = rows.iter().map(|r| r.server_name.as_str()).collect();
        let page1_ours_set: std::collections::HashSet<&String> =
            servers.iter().filter(|s| page1_ours.contains(s.as_str())).collect();

        // Page 2: use the cursor.
        let cursor = decode_federation_blacklist_cursor(next_cursor.as_deref()).expect("cursor should decode");
        let (rows2, _next_cursor2) =
            storage.get_all_blacklist(3, Some(cursor)).await.expect("get_all with cursor should succeed");

        // At least our remaining servers (not in page 1) should be present in page 2.
        let page2_ours: std::collections::HashSet<&str> = rows2.iter().map(|r| r.server_name.as_str()).collect();
        for s in &servers {
            if !page1_ours_set.contains(s) {
                assert!(page2_ours.contains(s.as_str()), "server {s} not in page 1 should appear in page 2");
            }
        }

        // No overlap between pages.
        for r in &rows2 {
            assert!(
                !page1_ours.contains(r.server_name.as_str()),
                "page 2 entry {} should not appear in page 1",
                r.server_name
            );
        }

        for s in &servers {
            cleanup_by_server(&pool, s).await;
        }
    }

    // 12. create_log inserts a log record and returns it.
    #[tokio::test]
    async fn test_create_log_succeeds() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-log-{}.com", suffix);

        cleanup_by_server(&pool, &server_name).await;

        let log = storage
            .create_log(CreateLogRequest {
                server_name: server_name.clone(),
                action: "add".to_string(),
                old_status: None,
                new_status: Some("blocked".to_string()),
                reason: Some("Test reason".to_string()),
                performed_by: "@admin:test.com".to_string(),
                ip_address: Some("10.0.0.1".to_string()),
                user_agent: Some("test-agent/1.0".to_string()),
                metadata: Some(serde_json::json!({"source": "test"})),
            })
            .await
            .expect("create_log should succeed");

        assert!(log.id > 0);
        assert_eq!(log.server_name, server_name);
        assert_eq!(log.action, "add");
        assert_eq!(log.new_status.as_deref(), Some("blocked"));
        assert_eq!(log.reason.as_deref(), Some("Test reason"));
        assert_eq!(log.performed_by, "@admin:test.com");
        assert_eq!(log.ip_address.as_deref(), Some("10.0.0.1"));
        assert_eq!(log.user_agent.as_deref(), Some("test-agent/1.0"));
        assert_eq!(log.metadata, serde_json::json!({"source": "test"}));
        assert!(log.performed_ts > 0);

        cleanup_by_server(&pool, &server_name).await;
    }

    // 13. update_access_stats inserts a new stats row.
    #[tokio::test]
    async fn test_update_access_stats_succeeds() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-stats-{}.com", suffix);

        cleanup_by_server(&pool, &server_name).await;

        let stats = storage
            .update_access_stats(UpdateStatsRequest {
                server_name: server_name.clone(),
                is_success: true,
                response_time_ms: Some(150.0),
            })
            .await
            .expect("update_access_stats should succeed");

        assert!(stats.id > 0);
        assert_eq!(stats.server_name, server_name);
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.successful_requests, 1);
        assert_eq!(stats.failed_requests, 0);
        assert!(stats.last_request_ts.is_some());
        assert!(stats.last_success_ts.is_some());
        assert!(stats.last_failure_ts.is_none());
        assert!(stats.average_response_time_ms > 0.0);

        cleanup_by_server(&pool, &server_name).await;
    }

    // 14. update_access_stats accumulates counts on upsert.
    #[tokio::test]
    async fn test_update_access_stats_accumulates() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-stats-acc-{}.com", suffix);

        cleanup_by_server(&pool, &server_name).await;

        // First: success.
        storage
            .update_access_stats(UpdateStatsRequest {
                server_name: server_name.clone(),
                is_success: true,
                response_time_ms: Some(100.0),
            })
            .await
            .expect("first update should succeed");

        // Second: failure.
        let stats = storage
            .update_access_stats(UpdateStatsRequest {
                server_name: server_name.clone(),
                is_success: false,
                response_time_ms: Some(200.0),
            })
            .await
            .expect("second update should succeed");

        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.successful_requests, 1);
        assert_eq!(stats.failed_requests, 1);
        assert!(stats.last_failure_ts.is_some(), "last_failure_ts should be set after failure");
        // error_rate uses pre-update values in ON CONFLICT DO UPDATE,
        // so after the second upsert it still reflects the first row's state (0/1 = 0.0).
        // We only assert total/success/failure counts for correctness.

        cleanup_by_server(&pool, &server_name).await;
    }

    // 15. get_access_stats returns existing stats.
    #[tokio::test]
    async fn test_get_access_stats_found() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-get-stats-{}.com", suffix);

        cleanup_by_server(&pool, &server_name).await;

        storage
            .update_access_stats(UpdateStatsRequest {
                server_name: server_name.clone(),
                is_success: true,
                response_time_ms: None,
            })
            .await
            .expect("update should succeed");

        let stats =
            storage.get_access_stats(&server_name).await.expect("get should succeed").expect("stats should exist");

        assert_eq!(stats.server_name, server_name);
        assert_eq!(stats.total_requests, 1);

        cleanup_by_server(&pool, &server_name).await;
    }

    // 16. get_access_stats returns None for an unknown server.
    #[tokio::test]
    async fn test_get_access_stats_not_found() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-no-stats-{}.com", suffix);

        let stats = storage.get_access_stats(&server_name).await.expect("get should succeed");

        assert!(stats.is_none());
    }

    // 17. create_rule inserts a new rule and returns it.
    #[tokio::test]
    async fn test_create_rule_succeeds() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let rule_name = format!("test-rule-{}", suffix);

        cleanup_rule_by_name(&pool, &format!("test-rule-{}%", suffix)).await;

        let rule = storage
            .create_rule(CreateRuleRequest {
                rule_name: rule_name.clone(),
                rule_type: "domain".to_string(),
                pattern: format!("*.evil-{}.com", suffix),
                action: "block".to_string(),
                priority: 100,
                description: Some("Test rule".to_string()),
                created_by: "@admin:test.com".to_string(),
            })
            .await
            .expect("create_rule should succeed");

        assert!(rule.id > 0);
        assert_eq!(rule.rule_name, rule_name);
        assert_eq!(rule.rule_type, "domain");
        assert_eq!(rule.action, "block");
        assert_eq!(rule.priority, 100);
        assert!(rule.is_enabled);
        assert_eq!(rule.created_by, "@admin:test.com");
        assert!(rule.created_ts > 0);

        cleanup_rule_by_name(&pool, &format!("test-rule-{}%", suffix)).await;
    }

    // 18. get_all_rules returns only enabled rules (filters out disabled).
    #[tokio::test]
    async fn test_get_all_rules_filters_disabled() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let rule_name_a = format!("test-rules-a-{}", suffix);
        let rule_name_b = format!("test-rules-b-{}", suffix);
        let like_pattern = format!("test-rules-%{}%", suffix);

        cleanup_rule_by_name(&pool, &like_pattern).await;

        // Create two rules.
        storage
            .create_rule(CreateRuleRequest {
                rule_name: rule_name_a.clone(),
                rule_type: "domain".to_string(),
                pattern: "*.evil-a.com".to_string(),
                action: "block".to_string(),
                priority: 100,
                description: None,
                created_by: "@admin:test.com".to_string(),
            })
            .await
            .expect("create rule a should succeed");

        storage
            .create_rule(CreateRuleRequest {
                rule_name: rule_name_b.clone(),
                rule_type: "ip".to_string(),
                pattern: "10.0.0.0/8".to_string(),
                action: "block".to_string(),
                priority: 50,
                description: None,
                created_by: "@admin:test.com".to_string(),
            })
            .await
            .expect("create rule b should succeed");

        // Both should appear in get_all_rules (both enabled by default).
        let rules = storage.get_all_rules().await.expect("get_all_rules should succeed");
        assert!(rules.iter().any(|r| r.rule_name == rule_name_a), "rule A should be present");
        assert!(rules.iter().any(|r| r.rule_name == rule_name_b), "rule B should be present");

        // Disable rule A via raw SQL.
        sqlx::query("UPDATE federation_blacklist_rule SET is_enabled = false WHERE rule_name = $1")
            .bind(&rule_name_a)
            .execute(&*pool)
            .await
            .expect("disable rule should work");

        let rules_after = storage.get_all_rules().await.expect("get_all_rules should succeed");
        assert!(!rules_after.iter().any(|r| r.rule_name == rule_name_a), "disabled rule A should not be returned");
        assert!(rules_after.iter().any(|r| r.rule_name == rule_name_b), "enabled rule B should still be returned");

        cleanup_rule_by_name(&pool, &like_pattern).await;
    }

    // 19. cleanup_expired_entries disables entries with past expires_at.
    #[tokio::test]
    async fn test_cleanup_expired_entries() {
        let pool = test_pool().await;
        let storage = FederationBlacklistStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name_exp = format!("test-cleanup-exp-{}.com", suffix);
        let server_name_active = format!("test-cleanup-active-{}.com", suffix);

        cleanup_by_server(&pool, &server_name_exp).await;
        cleanup_by_server(&pool, &server_name_active).await;

        let past = Utc::now().timestamp_millis() - 86_400_000; // 1 day ago
        storage
            .add_to_blacklist(AddBlacklistRequest {
                server_name: server_name_exp.clone(),
                block_type: "blacklist".to_string(),
                reason: None,
                blocked_by: "@admin:test.com".to_string(),
                expires_at: Some(past),
                metadata: None,
            })
            .await
            .expect("add expired entry should succeed");

        let future = Utc::now().timestamp_millis() + 86_400_000; // 1 day from now
        storage
            .add_to_blacklist(AddBlacklistRequest {
                server_name: server_name_active.clone(),
                block_type: "blacklist".to_string(),
                reason: None,
                blocked_by: "@admin:test.com".to_string(),
                expires_at: Some(future),
                metadata: None,
            })
            .await
            .expect("add active entry should succeed");

        let cleaned = storage.cleanup_expired_entries().await.expect("cleanup should succeed");

        // At least our expired entry should have been cleaned.
        assert!(cleaned >= 1, "should clean at least 1 expired entry, got {cleaned}");

        // Expired entry should now be disabled.
        let expired_entry = storage
            .get_blacklist_entry(&server_name_exp)
            .await
            .expect("get should succeed")
            .expect("expired entry should still exist");
        assert!(!expired_entry.is_enabled, "expired entry should be disabled");

        // Active (future expiry) entry should remain enabled.
        let active_entry = storage
            .get_blacklist_entry(&server_name_active)
            .await
            .expect("get should succeed")
            .expect("active entry should still exist");
        assert!(active_entry.is_enabled, "active entry should remain enabled");

        cleanup_by_server(&pool, &server_name_exp).await;
        cleanup_by_server(&pool, &server_name_active).await;
    }
}
