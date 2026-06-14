use serde::{Deserialize, Serialize};
use std::sync::Arc;
use synapse_common::ApiError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyEvent {
    pub user_id: String,
    pub device_id: Option<String>,
    pub operation: String,
    pub key_id: Option<String>,
    pub room_id: Option<String>,
    pub details: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct KeyAuditEntry {
    pub id: i64,
    pub user_id: String,
    pub device_id: Option<String>,
    pub operation: String,
    pub key_id: Option<String>,
    pub room_id: Option<String>,
    pub details: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub created_ts: i64,
}

#[derive(Clone)]
pub struct E2eeAuditStorage {
    pool: Arc<sqlx::PgPool>,
}

impl E2eeAuditStorage {
    pub fn new(pool: &Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn log_key_operation(&self, event: &KeyEvent) -> Result<(), ApiError> {
        sqlx::query(
            r"
            INSERT INTO e2ee_audit_log
            (user_id, device_id, operation, key_id, room_id, details, ip_address, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ",
        )
        .bind(&event.user_id)
        .bind(&event.device_id)
        .bind(&event.operation)
        .bind(&event.key_id)
        .bind(&event.room_id)
        .bind(&event.details)
        .bind(&event.ip_address)
        .bind(event.timestamp)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to log key operation", &e))?;

        Ok(())
    }

    pub async fn get_key_history(&self, user_id: &str) -> Result<Vec<KeyAuditEntry>, ApiError> {
        sqlx::query_as::<_, KeyAuditEntry>(
            r"
            SELECT id, user_id, device_id, operation, key_id, room_id, details, ip_address, created_ts
            FROM e2ee_audit_log
            WHERE user_id = $1
            ORDER BY created_ts DESC, id DESC
            LIMIT 100
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get key history", &e))
    }

    pub async fn get_key_history_paginated(
        &self,
        user_id: &str,
        limit: i64,
        from_ts: Option<i64>,
        from_id: Option<i64>,
    ) -> Result<Vec<KeyAuditEntry>, ApiError> {
        if let (Some(ts), Some(id)) = (from_ts, from_id) {
            sqlx::query_as::<_, KeyAuditEntry>(
                r"
                SELECT id, user_id, device_id, operation, key_id, room_id, details, ip_address, created_ts
                FROM e2ee_audit_log
                WHERE user_id = $1 AND (created_ts < $2 OR (created_ts = $2 AND id < $3))
                ORDER BY created_ts DESC, id DESC
                LIMIT $4
                ",
            )
            .bind(user_id)
            .bind(ts)
            .bind(id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get key history", &e))
        } else {
            sqlx::query_as::<_, KeyAuditEntry>(
                r"
                SELECT id, user_id, device_id, operation, key_id, room_id, details, ip_address, created_ts
                FROM e2ee_audit_log
                WHERE user_id = $1
                ORDER BY created_ts DESC, id DESC
                LIMIT $2
                ",
            )
            .bind(user_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get key history", &e))
        }
    }

    pub async fn get_operations_by_type(&self, operation: &str, limit: i64) -> Result<Vec<KeyAuditEntry>, ApiError> {
        sqlx::query_as::<_, KeyAuditEntry>(
            r"
            SELECT id, user_id, device_id, operation, key_id, room_id, details, ip_address, created_ts
            FROM e2ee_audit_log
            WHERE operation = $1
            ORDER BY created_ts DESC, id DESC
            LIMIT $2
            ",
        )
        .bind(operation)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get operations", &e))
    }

    pub async fn get_user_device_history(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<KeyAuditEntry>, ApiError> {
        sqlx::query_as::<_, KeyAuditEntry>(
            r"
            SELECT id, user_id, device_id, operation, key_id, room_id, details, ip_address, created_ts
            FROM e2ee_audit_log
            WHERE user_id = $1 AND device_id = $2
            ORDER BY created_ts DESC, id DESC
            LIMIT 50
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get device history", &e))
    }

    pub async fn cleanup_old_logs(&self, days_to_keep: i64) -> Result<u64, ApiError> {
        let cutoff_ts = chrono::Utc::now().timestamp_millis() - (days_to_keep * 24 * 60 * 60 * 1000);
        let result = sqlx::query("DELETE FROM e2ee_audit_log WHERE created_ts < $1")
            .bind(cutoff_ts)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to cleanup logs", &e))?;

        Ok(result.rows_affected())
    }
}
