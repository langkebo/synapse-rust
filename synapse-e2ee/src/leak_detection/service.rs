// Session Leak Detection Service
// E2EE Phase 2: Detect potential session key leaks

use crate::megolm::MegolmSession;
use synapse_common::ApiError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct LeakDetectionService {
    storage: Arc<LeakDetectionStorage>,
    config: LeakDetectionConfig,
}

#[derive(Clone, Debug)]
pub struct LeakDetectionConfig {
    pub max_message_index_gap: u32,
    pub max_time_gap_hours: i64,
    pub enable_detection: bool,
}

impl Default for LeakDetectionConfig {
    fn default() -> Self {
        Self { max_message_index_gap: 10, max_time_gap_hours: 24, enable_detection: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeakAlert {
    pub id: i64,
    pub key_id: String,
    pub details: Option<serde_json::Value>,
    pub created_ts: i64,
    pub is_acknowledged: bool,
    pub acknowledged_by: Option<String>,
    pub acknowledged_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeakDetectionResult {
    pub has_leak: bool,
    pub alerts: Vec<LeakAlert>,
    pub risk_level: String,
}

impl LeakDetectionService {
    pub fn new(storage: Arc<LeakDetectionStorage>, config: LeakDetectionConfig) -> Self {
        Self { storage, config }
    }

    pub async fn detect_session_leak(
        &self,
        session: &MegolmSession,
        current_message_index: u32,
    ) -> Result<LeakDetectionResult, ApiError> {
        let mut alerts = Vec::new();
        let mut has_leak = false;

        // Check for message index gaps
        if current_message_index > session.message_index + self.config.max_message_index_gap {
            has_leak = true;
            alerts.push(LeakAlert {
                id: 0,
                key_id: session.session_id.clone(),
                details: Some(serde_json::json!({
                    "alert_type": "message_index_gap",
                    "severity": "high",
                    "message": format!("Message index gap detected: expected {}, got {}", session.message_index, current_message_index),
                    "room_id": session.room_id,
                    "user_id": session.sender_key,
                })),
                created_ts: chrono::Utc::now().timestamp_millis(),
                is_acknowledged: false,
                acknowledged_by: None,
                acknowledged_at: None,
            });
        }

        // Check for unusual time gaps
        let time_since_last_use = (Utc::now() - session.last_used_ts).num_hours();
        if time_since_last_use > self.config.max_time_gap_hours {
            has_leak = true;
            alerts.push(LeakAlert {
                id: 0,
                key_id: session.session_id.clone(),
                details: Some(serde_json::json!({
                    "alert_type": "time_gap",
                    "severity": "medium",
                    "message": format!("Unusual time gap detected: {} hours since last use", time_since_last_use),
                    "room_id": session.room_id,
                    "user_id": session.sender_key,
                })),
                created_ts: chrono::Utc::now().timestamp_millis(),
                is_acknowledged: false,
                acknowledged_by: None,
                acknowledged_at: None,
            });
        }

        // Check for multiple device usage
        let device_count = self.storage.get_session_device_count(&session.session_id).await?;
        if device_count > 1 {
            has_leak = true;
            alerts.push(LeakAlert {
                id: 0,
                key_id: session.session_id.clone(),
                details: Some(serde_json::json!({
                    "alert_type": "multiple_devices",
                    "severity": "high",
                    "message": format!("Session used by {} devices", device_count),
                    "room_id": session.room_id,
                    "user_id": session.sender_key,
                })),
                created_ts: chrono::Utc::now().timestamp_millis(),
                is_acknowledged: false,
                acknowledged_by: None,
                acknowledged_at: None,
            });
        }

        // Determine risk level
        let risk_level = if alerts.iter().any(|a| a.details.as_ref().and_then(|d| d.get("severity")).and_then(|v| v.as_str()) == Some("high")) {
            "high"
        } else if alerts.iter().any(|a| a.details.as_ref().and_then(|d| d.get("severity")).and_then(|v| v.as_str()) == Some("medium")) {
            "medium"
        } else {
            "low"
        };

        // Save alerts
        for alert in &alerts {
            self.storage.save_alert(alert).await?;
        }

        Ok(LeakDetectionResult { has_leak, alerts, risk_level: risk_level.to_string() })
    }

    pub async fn get_user_alerts(&self, user_id: &str) -> Result<Vec<LeakAlert>, ApiError> {
        self.storage.get_user_alerts(user_id).await
    }

    pub async fn acknowledge_alert(&self, alert_id: i64, acknowledged_by: &str) -> Result<(), ApiError> {
        self.storage.acknowledge_alert(alert_id, acknowledged_by).await
    }

    pub async fn get_leak_statistics(&self) -> Result<LeakStatistics, ApiError> {
        self.storage.get_leak_statistics().await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeakStatistics {
    pub total_alerts: i64,
    pub unresolved_alerts: i64,
    pub high_severity_count: i64,
    pub medium_severity_count: i64,
    pub low_severity_count: i64,
}

pub struct LeakDetectionStorage {
    pool: Arc<sqlx::PgPool>,
}

impl LeakDetectionStorage {
    pub fn new(pool: Arc<sqlx::PgPool>) -> Self {
        Self { pool }
    }

    pub async fn save_alert(&self, alert: &LeakAlert) -> Result<(), ApiError> {
        sqlx::query(
            "INSERT INTO leak_alerts
             (key_id, details, created_ts, is_acknowledged, acknowledged_by, acknowledged_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&alert.key_id)
        .bind(&alert.details)
        .bind(alert.created_ts)
        .bind(alert.is_acknowledged)
        .bind(&alert.acknowledged_by)
        .bind(alert.acknowledged_at)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_session_device_count(&self, _session_id: &str) -> Result<i64, ApiError> {
        Ok(1)
    }

    pub async fn get_user_alerts(&self, user_id: &str) -> Result<Vec<LeakAlert>, ApiError> {
        let rows = sqlx::query(
            "SELECT id, key_id, details, created_ts, is_acknowledged, acknowledged_by, acknowledged_at
             FROM leak_alerts
             WHERE key_id LIKE $1
             ORDER BY created_ts DESC",
        )
        .bind(format!("%{}%", user_id))
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        use sqlx::Row;
        let alerts = rows
            .into_iter()
            .map(|row| LeakAlert {
                id: row.get("id"),
                key_id: row.get("key_id"),
                details: row.get("details"),
                created_ts: row.get("created_ts"),
                is_acknowledged: row.get("is_acknowledged"),
                acknowledged_by: row.get("acknowledged_by"),
                acknowledged_at: row.get("acknowledged_at"),
            })
            .collect();

        Ok(alerts)
    }

    pub async fn acknowledge_alert(&self, alert_id: i64, acknowledged_by: &str) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE leak_alerts
             SET is_acknowledged = true, acknowledged_by = $2, acknowledged_at = $3
             WHERE id = $1",
        )
        .bind(alert_id)
        .bind(acknowledged_by)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_leak_statistics(&self) -> Result<LeakStatistics, ApiError> {
        let row = sqlx::query(
            "SELECT
             COUNT(*) as total_alerts,
             COUNT(CASE WHEN NOT is_acknowledged THEN 1 END) as unresolved_alerts,
             COUNT(CASE WHEN severity = 'high' THEN 1 END) as high_severity_count,
             COUNT(CASE WHEN severity = 'medium' THEN 1 END) as medium_severity_count,
             COUNT(CASE WHEN severity = 'low' THEN 1 END) as low_severity_count
             FROM leak_alerts",
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        use sqlx::Row;
        Ok(LeakStatistics {
            total_alerts: row.get("total_alerts"),
            unresolved_alerts: row.get("unresolved_alerts"),
            high_severity_count: row.get("high_severity_count"),
            medium_severity_count: row.get("medium_severity_count"),
            low_severity_count: row.get("low_severity_count"),
        })
    }
}
