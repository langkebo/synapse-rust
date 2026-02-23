use crate::common::error::ApiError;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FederationBlacklist {
    pub id: i32,
    pub server_name: String,
    pub block_type: String,
    pub reason: Option<String>,
    pub blocked_by: String,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub expires_at: Option<i64>,
    pub is_enabled: bool,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FederationBlacklistLog {
    pub id: i32,
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
    pub id: i32,
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
    pub id: i32,
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
    pub success: bool,
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

#[derive(Debug, Clone)]
pub struct FederationBlacklistStorage {
    pool: Arc<PgPool>,
}

impl FederationBlacklistStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn add_to_blacklist(
        &self,
        request: AddBlacklistRequest,
    ) -> Result<FederationBlacklist, ApiError> {
        let now = Utc::now().timestamp_millis();
        let metadata = request.metadata.unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, FederationBlacklist>(
            r#"
            INSERT INTO federation_blacklist (
                server_name, block_type, reason, blocked_by, created_ts, updated_ts, expires_at, is_enabled, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $5, $6, true, $7)
            ON CONFLICT (server_name) DO UPDATE SET
                block_type = $2,
                reason = $3,
                blocked_by = $4,
                updated_ts = $5,
                expires_at = $6,
                is_enabled = true,
                metadata = $7
            RETURNING *
            "#,
        )
        .bind(&request.server_name)
        .bind(&request.block_type)
        .bind(&request.reason)
        .bind(&request.blocked_by)
        .bind(now)
        .bind(request.expires_at)
        .bind(&metadata)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to add to blacklist: {}", e)))?;

        info!("Added server {} to blacklist", request.server_name);
        Ok(row)
    }

    pub async fn remove_from_blacklist(&self, server_name: &str, performed_by: &str) -> Result<(), ApiError> {
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"
            UPDATE federation_blacklist 
            SET is_enabled = false, updated_ts = $1
            WHERE server_name = $2
            "#,
        )
        .bind(now)
        .bind(server_name)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to remove from blacklist: {}", e)))?;

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
        }).await?;

        info!("Removed server {} from blacklist", server_name);
        Ok(())
    }

    pub async fn get_blacklist_entry(&self, server_name: &str) -> Result<Option<FederationBlacklist>, ApiError> {
        let row = sqlx::query_as::<_, FederationBlacklist>(
            "SELECT * FROM federation_blacklist WHERE server_name = $1 AND is_enabled = true"
        )
        .bind(server_name)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get blacklist entry: {}", e)))?;

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
            SELECT * FROM federation_blacklist 
            WHERE server_name = $1 AND block_type = 'whitelist' AND is_enabled = true
            "#,
        )
        .bind(server_name)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check whitelist: {}", e)))?;

        Ok(row.is_some())
    }

    pub async fn get_all_blacklist(&self, limit: i32, offset: i32) -> Result<Vec<FederationBlacklist>, ApiError> {
        let rows = sqlx::query_as::<_, FederationBlacklist>(
            r#"
            SELECT * FROM federation_blacklist 
            WHERE is_enabled = true
            ORDER BY created_ts DESC
            LIMIT $1 OFFSET $2
            "#
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get blacklist: {}", e)))?;

        Ok(rows)
    }

    pub async fn create_log(&self, request: CreateLogRequest) -> Result<FederationBlacklistLog, ApiError> {
        let metadata = request.metadata.unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, FederationBlacklistLog>(
            r#"
            INSERT INTO federation_blacklist_log (
                server_name, action, old_status, new_status, reason, performed_by, ip_address, user_agent, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            "#,
        )
        .bind(&request.server_name)
        .bind(&request.action)
        .bind(&request.old_status)
        .bind(&request.new_status)
        .bind(&request.reason)
        .bind(&request.performed_by)
        .bind(&request.ip_address)
        .bind(&request.user_agent)
        .bind(&metadata)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create log: {}", e)))?;

        Ok(row)
    }

    pub async fn update_access_stats(&self, request: UpdateStatsRequest) -> Result<FederationAccessStats, ApiError> {
        let now = Utc::now().timestamp_millis();
        
        let row = sqlx::query_as::<_, FederationAccessStats>(
            r#"
            INSERT INTO federation_access_stats (server_name, total_requests, successful_requests, failed_requests, 
                last_request_ts, last_success_ts, last_failure_ts, average_response_time_ms, error_rate, created_ts, updated_ts)
            VALUES ($1, 1, $2, $3, $4, $5, $6, $7, $8, $4, $4)
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
            RETURNING *
            "#,
        )
        .bind(&request.server_name)
        .bind(if request.success { 1 } else { 0 })
        .bind(if request.success { 0 } else { 1 })
        .bind(now)
        .bind(if request.success { Some(now) } else { None })
        .bind(if request.success { None } else { Some(now) })
        .bind(request.response_time_ms)
        .bind(if request.success { 0.0 } else { 1.0 })
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update access stats: {}", e)))?;

        Ok(row)
    }

    pub async fn get_access_stats(&self, server_name: &str) -> Result<Option<FederationAccessStats>, ApiError> {
        let row = sqlx::query_as::<_, FederationAccessStats>(
            "SELECT * FROM federation_access_stats WHERE server_name = $1"
        )
        .bind(server_name)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get access stats: {}", e)))?;

        Ok(row)
    }

    pub async fn create_rule(&self, request: CreateRuleRequest) -> Result<FederationBlacklistRule, ApiError> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, FederationBlacklistRule>(
            r#"
            INSERT INTO federation_blacklist_rule (
                rule_name, rule_type, pattern, action, priority, description, created_ts, updated_ts, created_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $8)
            RETURNING *
            "#,
        )
        .bind(&request.rule_name)
        .bind(&request.rule_type)
        .bind(&request.pattern)
        .bind(&request.action)
        .bind(request.priority)
        .bind(&request.description)
        .bind(now)
        .bind(&request.created_by)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create rule: {}", e)))?;

        info!("Created federation blacklist rule: {}", request.rule_name);
        Ok(row)
    }

    pub async fn get_all_rules(&self) -> Result<Vec<FederationBlacklistRule>, ApiError> {
        let rows = sqlx::query_as::<_, FederationBlacklistRule>(
            "SELECT * FROM federation_blacklist_rule WHERE is_enabled = true ORDER BY priority DESC"
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get rules: {}", e)))?;

        Ok(rows)
    }

    pub async fn cleanup_expired_entries(&self) -> Result<u64, ApiError> {
        let now = Utc::now().timestamp_millis();
        let result = sqlx::query(
            "UPDATE federation_blacklist SET is_enabled = false WHERE expires_at < $1 AND is_enabled = true"
        )
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to cleanup expired entries: {}", e)))?;

        info!("Cleaned up {} expired blacklist entries", result.rows_affected());
        Ok(result.rows_affected())
    }

    pub async fn get_config(&self, config_key: &str) -> Result<Option<String>, ApiError> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT config_value FROM federation_blacklist_config WHERE config_key = $1"
        )
        .bind(config_key)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get config: {}", e)))?;

        Ok(row.map(|r| r.0))
    }

    pub async fn get_config_as_bool(&self, config_key: &str, default: bool) -> Result<bool, ApiError> {
        let value = self.get_config(config_key).await?;
        
        Ok(match value {
            Some(v) => v.to_lowercase() == "true",
            None => default,
        })
    }

    pub async fn get_config_as_int(&self, config_key: &str, default: i32) -> Result<i32, ApiError> {
        let value = self.get_config(config_key).await?;
        
        Ok(match value {
            Some(v) => v.parse().unwrap_or(default),
            None => default,
        })
    }
}
