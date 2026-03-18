use crate::common::ApiError;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DeviceVerificationStatus {
    pub device_id: String,
    pub user_id: String,
    pub display_name: Option<String>,
    pub is_verified: bool,
    pub is_cross_signed: bool,
    pub signature_valid: bool,
    pub last_verified_ts: Option<i64>,
    pub verification_method: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceVerificationReport {
    pub user_id: String,
    pub devices: Vec<DeviceVerificationStatus>,
    pub all_verified: bool,
    pub cross_signing_setup: bool,
    pub verified_count: usize,
    pub unverified_count: usize,
}

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

pub struct E2eeAuditService {
    pool: Arc<PgPool>,
}

impl E2eeAuditService {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    pub async fn log_key_operation(&self, event: KeyEvent) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO e2ee_audit_log 
            (user_id, device_id, operation, key_id, room_id, details, ip_address, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
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
        .map_err(|e| ApiError::internal(format!("Failed to log key operation: {}", e)))?;

        debug!(
            "Logged E2EE operation: {} for user: {}",
            event.operation, event.user_id
        );
        Ok(())
    }

    pub async fn get_key_history(&self, user_id: &str) -> Result<Vec<KeyAuditEntry>, ApiError> {
        sqlx::query_as::<_, KeyAuditEntry>(
            r#"
            SELECT * FROM e2ee_audit_log 
            WHERE user_id = $1 
            ORDER BY created_ts DESC
            LIMIT 100
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get key history: {}", e)))
    }

    pub async fn get_operations_by_type(
        &self,
        operation: &str,
        limit: i64,
    ) -> Result<Vec<KeyAuditEntry>, ApiError> {
        sqlx::query_as::<_, KeyAuditEntry>(
            r#"
            SELECT * FROM e2ee_audit_log 
            WHERE operation = $1 
            ORDER BY created_ts DESC
            LIMIT $2
            "#,
        )
        .bind(operation)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get operations: {}", e)))
    }

    pub async fn get_user_device_history(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<KeyAuditEntry>, ApiError> {
        sqlx::query_as::<_, KeyAuditEntry>(
            r#"
            SELECT * FROM e2ee_audit_log 
            WHERE user_id = $1 AND device_id = $2
            ORDER BY created_ts DESC
            LIMIT 50
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get device history: {}", e)))
    }

    pub async fn cleanup_old_logs(&self, days_to_keep: i64) -> Result<u64, ApiError> {
        let cutoff_ts = chrono::Utc::now().timestamp_millis() - (days_to_keep * 24 * 60 * 60 * 1000);

        let result = sqlx::query("DELETE FROM e2ee_audit_log WHERE created_ts < $1")
            .bind(cutoff_ts)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to cleanup logs: {}", e)))?;

        let deleted = result.rows_affected();
        if deleted > 0 {
            info!("Cleaned up {} old E2EE audit log entries", deleted);
        }
        Ok(deleted)
    }
}

pub struct CrossSigningVerificationService {
    pool: Arc<PgPool>,
    audit: Arc<E2eeAuditService>,
}

impl CrossSigningVerificationService {
    pub fn new(pool: Arc<PgPool>, audit: Arc<E2eeAuditService>) -> Self {
        Self { pool, audit }
    }

    pub async fn verify_user_devices(
        &self,
        user_id: &str,
    ) -> Result<DeviceVerificationReport, ApiError> {
        let devices = self.get_user_devices(user_id).await?;
        let mut report = DeviceVerificationReport {
            user_id: user_id.to_string(),
            devices: Vec::new(),
            all_verified: true,
            cross_signing_setup: false,
            verified_count: 0,
            unverified_count: 0,
        };

        for device in &devices {
            let status = self.verify_device(device).await?;
            
            if status.is_verified {
                report.verified_count += 1;
            } else {
                report.unverified_count += 1;
                report.all_verified = false;
            }

            if status.is_cross_signed {
                report.cross_signing_setup = true;
            }

            report.devices.push(status);
        }

        self.audit
            .log_key_operation(KeyEvent {
                user_id: user_id.to_string(),
                device_id: None,
                operation: "verify_all_devices".to_string(),
                key_id: None,
                room_id: None,
                details: Some(serde_json::json!({
                    "total_devices": devices.len(),
                    "verified_count": report.verified_count,
                    "unverified_count": report.unverified_count,
                })),
                ip_address: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
            })
            .await?;

        Ok(report)
    }

    pub async fn verify_device(
        &self,
        device: &DeviceInfo,
    ) -> Result<DeviceVerificationStatus, ApiError> {
        let signature_valid = self.verify_device_signature(device).await?;
        let cross_signed = self.check_cross_signing(device).await?;
        let is_verified = signature_valid && cross_signed;

        let status = DeviceVerificationStatus {
            device_id: device.device_id.clone(),
            user_id: device.user_id.clone(),
            display_name: device.display_name.clone(),
            is_verified,
            is_cross_signed: cross_signed,
            signature_valid,
            last_verified_ts: device.last_verified_ts,
            verification_method: device.verification_method.clone(),
        };

        self.audit
            .log_key_operation(KeyEvent {
                user_id: device.user_id.clone(),
                device_id: Some(device.device_id.clone()),
                operation: "verify_device".to_string(),
                key_id: None,
                room_id: None,
                details: Some(serde_json::json!({
                    "is_verified": is_verified,
                    "signature_valid": signature_valid,
                    "cross_signed": cross_signed,
                })),
                ip_address: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
            })
            .await?;

        Ok(status)
    }

    pub async fn mark_device_verified(
        &self,
        user_id: &str,
        device_id: &str,
        method: &str,
    ) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE devices 
            SET verified = TRUE,
                verified_ts = $3,
                verification_method = $4
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .bind(method)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to mark device verified: {}", e)))?;

        self.audit
            .log_key_operation(KeyEvent {
                user_id: user_id.to_string(),
                device_id: Some(device_id.to_string()),
                operation: "mark_verified".to_string(),
                key_id: None,
                room_id: None,
                details: Some(serde_json::json!({
                    "method": method,
                })),
                ip_address: None,
                timestamp: now,
            })
            .await?;

        info!(
            "Marked device {} as verified for user {} via {}",
            device_id, user_id, method
        );
        Ok(())
    }

    pub async fn mark_device_unverified(
        &self,
        user_id: &str,
        device_id: &str,
        reason: &str,
    ) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE devices 
            SET verified = FALSE,
                verified_ts = NULL,
                verification_method = NULL
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to mark device unverified: {}", e)))?;

        self.audit
            .log_key_operation(KeyEvent {
                user_id: user_id.to_string(),
                device_id: Some(device_id.to_string()),
                operation: "mark_unverified".to_string(),
                key_id: None,
                room_id: None,
                details: Some(serde_json::json!({
                    "reason": reason,
                })),
                ip_address: None,
                timestamp: now,
            })
            .await?;

        warn!(
            "Marked device {} as unverified for user {}: {}",
            device_id, user_id, reason
        );
        Ok(())
    }

    async fn get_user_devices(&self, user_id: &str) -> Result<Vec<DeviceInfo>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT device_id, user_id, display_name, verified, verified_ts, verification_method
            FROM devices 
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get devices: {}", e)))?;

        Ok(rows
            .iter()
            .map(|row| DeviceInfo {
                device_id: row.get("device_id"),
                user_id: row.get("user_id"),
                display_name: row.get("display_name"),
                last_verified_ts: row.get("verified_ts"),
                verification_method: row.get("verification_method"),
            })
            .collect())
    }

    async fn verify_device_signature(&self, device: &DeviceInfo) -> Result<bool, ApiError> {
        let result: Option<Option<bool>> = sqlx::query_scalar(
            r#"
            SELECT signature_valid FROM device_signatures 
            WHERE device_id = $1
            ORDER BY created_ts DESC
            LIMIT 1
            "#,
        )
        .bind(&device.device_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check signature: {}", e)))?;

        Ok(result.flatten().unwrap_or(false))
    }

    async fn check_cross_signing(&self, device: &DeviceInfo) -> Result<bool, ApiError> {
        let result: Option<Option<bool>> = sqlx::query_scalar(
            r#"
            SELECT cross_signed FROM cross_signing_keys 
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(&device.user_id)
        .bind(&device.device_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check cross signing: {}", e)))?;

        Ok(result.flatten().unwrap_or(false))
    }
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    device_id: String,
    user_id: String,
    display_name: Option<String>,
    last_verified_ts: Option<i64>,
    verification_method: Option<String>,
}
