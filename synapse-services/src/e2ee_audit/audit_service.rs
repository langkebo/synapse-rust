use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::ApiError;
pub use synapse_storage::{KeyAuditEntry, KeyEvent};
use synapse_e2ee::{CrossSigningStorage, DeviceTrustLevel, DeviceTrustStorage};
use synapse_storage::{DeviceStorage, E2eeAuditStorage};
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

pub struct E2eeAuditService {
    storage: E2eeAuditStorage,
}

impl E2eeAuditService {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { storage: E2eeAuditStorage::new(&pool) }
    }

    pub async fn log_key_operation(&self, event: KeyEvent) -> Result<(), ApiError> {
        self.storage.log_key_operation(&event).await?;

        debug!("Logged E2EE operation: {} for user: {}", event.operation, event.user_id);
        Ok(())
    }

    pub async fn get_key_history(&self, user_id: &str) -> Result<Vec<KeyAuditEntry>, ApiError> {
        self.storage.get_key_history(user_id).await
    }

    pub async fn get_operations_by_type(&self, operation: &str, limit: i64) -> Result<Vec<KeyAuditEntry>, ApiError> {
        self.storage.get_operations_by_type(operation, limit).await
    }

    pub async fn get_user_device_history(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<KeyAuditEntry>, ApiError> {
        self.storage.get_user_device_history(user_id, device_id).await
    }

    pub async fn cleanup_old_logs(&self, days_to_keep: i64) -> Result<u64, ApiError> {
        let deleted = self.storage.cleanup_old_logs(days_to_keep).await?;
        if deleted > 0 {
            info!(deleted_count = deleted, "Cleaned up old E2EE audit log entries");
        }
        Ok(deleted)
    }
}

pub struct CrossSigningVerificationService {
    device_storage: DeviceStorage,
    device_trust_storage: DeviceTrustStorage,
    cross_signing_storage: CrossSigningStorage,
    audit: Arc<E2eeAuditService>,
}

impl CrossSigningVerificationService {
    pub fn new(pool: Arc<PgPool>, audit: Arc<E2eeAuditService>) -> Self {
        let device_storage = DeviceStorage::new(&pool);
        let device_trust_storage = DeviceTrustStorage::new(&pool);
        let cross_signing_storage = CrossSigningStorage::new(&pool);

        Self { device_storage, device_trust_storage, cross_signing_storage, audit }
    }

    pub async fn verify_user_devices(&self, user_id: &str) -> Result<DeviceVerificationReport, ApiError> {
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

    pub async fn verify_device(&self, device: &DeviceInfo) -> Result<DeviceVerificationStatus, ApiError> {
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

    pub async fn mark_device_verified(&self, user_id: &str, device_id: &str, method: &str) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        self.device_trust_storage
            .set_device_trust(user_id, device_id, DeviceTrustLevel::Verified, Some(method))
            .await?;

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

        info!(device_id = %device_id, user_id = %user_id, method = %method, "Marked device as verified");
        Ok(())
    }

    pub async fn mark_device_unverified(&self, user_id: &str, device_id: &str, reason: &str) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        self.device_trust_storage
            .set_device_trust(user_id, device_id, DeviceTrustLevel::Unverified, None)
            .await?;

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

        warn!(device_id = %device_id, user_id = %user_id, reason = %reason, "Marked device as unverified");
        Ok(())
    }

    async fn get_user_devices(&self, user_id: &str) -> Result<Vec<DeviceInfo>, ApiError> {
        let devices = self
            .device_storage
            .get_user_devices(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get devices", &e))?;

        let mut results = Vec::with_capacity(devices.len());
        for device in devices {
            let trust = self.device_trust_storage.get_device_trust(&device.user_id, &device.device_id).await?;
            results.push(DeviceInfo {
                device_id: device.device_id,
                user_id: device.user_id,
                display_name: device.display_name,
                last_verified_ts: trust.as_ref().and_then(|status| status.verified_at),
                verification_method: trust.and_then(|status| status.verified_by_device_id),
            });
        }

        Ok(results)
    }

    async fn verify_device_signature(&self, device: &DeviceInfo) -> Result<bool, ApiError> {
        let signatures = self
            .cross_signing_storage
            .get_device_signatures(&device.user_id, &device.device_id)
            .await?;

        Ok(!signatures.is_empty())
    }

    async fn check_cross_signing(&self, device: &DeviceInfo) -> Result<bool, ApiError> {
        Ok(self
            .cross_signing_storage
            .get_cross_signing_key(&device.user_id, "self_signing")
            .await?
            .is_some())
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
