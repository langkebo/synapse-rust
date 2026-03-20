// Session Leak Detection Service
// E2EE Phase 2: Detect potential session key leaks

use crate::e2ee::megolm::MegolmSession;
use crate::error::ApiError;
use chrono::{DateTime, Utc};
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
        Self {
            max_message_index_gap: 10,
            max_time_gap_hours: 24,
            enable_detection: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeakAlert {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub room_id: String,
    pub session_id: String,
    pub alert_type: String,
    pub severity: String,
    pub details: Option<String>,
    pub detected_at: DateTime<Utc>,
    pub resolved: bool,
    pub resolved_at: Option<DateTime<Utc>>,
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
                user_id: session.sender_key.clone(),
                device_id: String::new(),
                room_id: session.room_id.clone(),
                session_id: session.session_id.clone(),
                alert_type: "message_index_gap".to_string(),
                severity: "high".to_string(),
                details: Some(format!(
                    "Message index gap detected: expected {}, got {}",
                    session.message_index, current_message_index
                )),
                detected_at: Utc::now(),
                resolved: false,
                resolved_at: None,
            });
        }

        // Check for unusual time gaps
        let time_since_last_use = (Utc::now() - session.last_used_ts).num_hours();
        if time_since_last_use > self.config.max_time_gap_hours {
            has_leak = true;
            alerts.push(LeakAlert {
                id: 0,
                user_id: session.sender_key.clone(),
                device_id: String::new(),
                room_id: session.room_id.clone(),
                session_id: session.session_id.clone(),
                alert_type: "time_gap".to_string(),
                severity: "medium".to_string(),
                details: Some(format!(
                    "Unusual time gap detected: {} hours since last use",
                    time_since_last_use
                )),
                detected_at: Utc::now(),
                resolved: false,
                resolved_at: None,
            });
        }

        // Check for multiple device usage
        let device_count = self.storage.get_session_device_count(&session.session_id).await?;
        if device_count > 1 {
            has_leak = true;
            alerts.push(LeakAlert {
                id: 0,
                user_id: session.sender_key.clone(),
                device_id: String::new(),
                room_id: session.room_id.clone(),
                session_id: session.session_id.clone(),
                alert_type: "multiple_devices".to_string(),
                severity: "high".to_string(),
                details: Some(format!(
                    "Session used by {} devices",
                    device_count
                )),
                detected_at: Utc::now(),
                resolved: false,
                resolved_at: None,
            });
        }

        // Determine risk level
        let risk_level = if alerts.iter().any(|a| a.severity == "high") {
            "high"
        } else if alerts.iter().any(|a| a.severity == "medium") {
            "medium"
        } else {
            "low"
        };

        // Save alerts
        for alert in &alerts {
            self.storage.save_alert(alert).await?;
        }

        Ok(LeakDetectionResult {
            has_leak,
            alerts,
            risk_level: risk_level.to_string(),
        })
    }

    pub async fn get_user_alerts(&self, user_id: &str) -> Result<Vec<LeakAlert>, ApiError> {
        self.storage.get_user_alerts(user_id).await
    }

    pub async fn resolve_alert(&self, alert_id: i64) -> Result<(), ApiError> {
        self.storage.resolve_alert(alert_id).await
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
             (user_id, device_id, room_id, session_id, alert_type, severity, details, detected_at, resolved)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        )
        .bind(&alert.user_id)
        .bind(&alert.device_id)
        .bind(&alert.room_id)
        .bind(&alert.session_id)
        .bind(&alert.alert_type)
        .bind(&alert.severity)
        .bind(&alert.details)
        .bind(alert.detected_at)
        .bind(alert.resolved)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    pub async fn get_session_device_count(&self, _session_id: &str) -> Result<i64, ApiError> {
        Ok(1)
    }

    pub async fn get_user_alerts(&self, user_id: &str) -> Result<Vec<LeakAlert>, ApiError> {
        let rows = sqlx::query(
            "SELECT id, user_id, device_id, room_id, session_id, alert_type, severity, 
             details, detected_at, resolved, resolved_at
             FROM leak_alerts 
             WHERE user_id = $1 
             ORDER BY detected_at DESC"
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        use sqlx::Row;
        let alerts = rows
            .into_iter()
            .map(|row| LeakAlert {
                id: row.get("id"),
                user_id: row.get("user_id"),
                device_id: row.get("device_id"),
                room_id: row.get("room_id"),
                session_id: row.get("session_id"),
                alert_type: row.get("alert_type"),
                severity: row.get("severity"),
                details: row.get("details"),
                detected_at: row.get("detected_at"),
                resolved: row.get("resolved"),
                resolved_at: row.get("resolved_at"),
            })
            .collect();

        Ok(alerts)
    }

    pub async fn resolve_alert(&self, alert_id: i64) -> Result<(), ApiError> {
        sqlx::query(
            "UPDATE leak_alerts 
             SET resolved = true, resolved_at = NOW() 
             WHERE id = $1"
        )
        .bind(alert_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    pub async fn get_leak_statistics(&self) -> Result<LeakStatistics, ApiError> {
        let row = sqlx::query(
            "SELECT 
             COUNT(*) as total_alerts,
             COUNT(CASE WHEN NOT resolved THEN 1 END) as unresolved_alerts,
             COUNT(CASE WHEN severity = 'high' THEN 1 END) as high_severity_count,
             COUNT(CASE WHEN severity = 'medium' THEN 1 END) as medium_severity_count,
             COUNT(CASE WHEN severity = 'low' THEN 1 END) as low_severity_count
             FROM leak_alerts"
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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
