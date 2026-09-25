use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;
use synapse_e2ee::CrossSigningStorage;
use synapse_storage::{DeviceStorage, E2eeAuditStorage};
pub use synapse_storage::{KeyAuditEntry, KeyEvent};
use tracing::{debug, info};

/// The `DeviceVerificationStatus` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DeviceVerificationStatus {
    /// The `device_id` field.
    pub device_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `display_name` field.
    pub display_name: Option<String>,
    /// The `is_verified` field.
    pub is_verified: bool,
    /// The `is_cross_signed` field.
    pub is_cross_signed: bool,
    /// The `signature_valid` field.
    pub signature_valid: bool,
}

/// The `DeviceVerificationReport` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceVerificationReport {
    /// The `user_id` field.
    pub user_id: String,
    /// The `devices` field.
    pub devices: Vec<DeviceVerificationStatus>,
    /// The `all_verified` field.
    pub all_verified: bool,
    /// The `cross_signing_setup` field.
    pub cross_signing_setup: bool,
    /// The `verified_count` field.
    pub verified_count: usize,
    /// The `unverified_count` field.
    pub unverified_count: usize,
}

/// The `E2eeAuditService` struct.
pub struct E2eeAuditService {
    storage: E2eeAuditStorage,
}

impl E2eeAuditService {
    /// See [`new`].
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { storage: E2eeAuditStorage::new(&pool) }
    }

    /// See [`log_key_operation`].
    pub async fn log_key_operation(&self, event: KeyEvent) -> Result<(), ApiError> {
        self.storage.log_key_operation(&event).await?;

        debug!("Logged E2EE operation: {} for user: {}", event.operation, event.user_id);
        Ok(())
    }

    /// See [`get_key_history`].
    pub async fn get_key_history(&self, user_id: &str) -> Result<Vec<KeyAuditEntry>, ApiError> {
        self.storage.get_key_history(user_id).await
    }

    /// See [`get_key_history_paginated`].
    pub async fn get_key_history_paginated(
        &self,
        user_id: &str,
        limit: i64,
        from_ts: Option<i64>,
        from_id: Option<i64>,
    ) -> Result<Vec<KeyAuditEntry>, ApiError> {
        self.storage.get_key_history_paginated(user_id, limit, from_ts, from_id).await
    }

    /// See [`get_operations_by_type`].
    pub async fn get_operations_by_type(&self, operation: &str, limit: i64) -> Result<Vec<KeyAuditEntry>, ApiError> {
        self.storage.get_operations_by_type(operation, limit).await
    }

    /// See [`get_user_device_history`].
    pub async fn get_user_device_history(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<KeyAuditEntry>, ApiError> {
        self.storage.get_user_device_history(user_id, device_id).await
    }

    /// See [`cleanup_old_logs`].
    pub async fn cleanup_old_logs(&self, days_to_keep: i64) -> Result<u64, ApiError> {
        let deleted = self.storage.cleanup_old_logs(days_to_keep).await?;
        if deleted > 0 {
            info!(deleted_count = deleted, "Cleaned up old E2EE audit log entries");
        }
        Ok(deleted)
    }
}

/// The `CrossSigningVerificationService` struct.
pub struct CrossSigningVerificationService {
    device_storage: DeviceStorage,
    cross_signing_storage: CrossSigningStorage,
    audit: Arc<E2eeAuditService>,
}

impl CrossSigningVerificationService {
    /// See [`new`].
    pub fn new(pool: Arc<PgPool>, audit: Arc<E2eeAuditService>) -> Self {
        let device_storage = DeviceStorage::new(&pool);
        let cross_signing_storage = CrossSigningStorage::new(&pool);

        Self { device_storage, cross_signing_storage, audit }
    }

    /// See [`verify_user_devices`].
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

        if devices.is_empty() {
            return Ok(report);
        }

        // Batch the read queries: fetch all device signatures for the user in
        // one query, and the self_signing cross-signing key once (it is the
        // same for every device of the same user).
        let signatures_map =
            self.cross_signing_storage.get_device_signatures_batch(std::slice::from_ref(&user_id.to_string())).await?;
        let user_signatures = signatures_map.get(user_id).cloned().unwrap_or_default();
        let cross_signing_setup =
            self.cross_signing_storage.get_cross_signing_key(user_id, "self_signing").await?.is_some();

        for device in &devices {
            let signature_valid = user_signatures.iter().any(|sig| sig.target_device_id == device.device_id);
            let is_verified = signature_valid && cross_signing_setup;

            let status = DeviceVerificationStatus {
                device_id: device.device_id.clone(),
                user_id: device.user_id.clone(),
                display_name: device.display_name.clone(),
                is_verified,
                is_cross_signed: cross_signing_setup,
                signature_valid,
            };

            if status.is_verified {
                report.verified_count += 1;
            } else {
                report.unverified_count += 1;
                report.all_verified = false;
            }

            if status.is_cross_signed {
                report.cross_signing_setup = true;
            }

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
                        "cross_signed": cross_signing_setup,
                    })),
                    ip_address: None,
                    timestamp: current_timestamp_millis(),
                })
                .await?;

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
                timestamp: current_timestamp_millis(),
            })
            .await?;

        Ok(report)
    }

    /// See [`verify_device`].
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
                timestamp: current_timestamp_millis(),
            })
            .await?;

        Ok(status)
    }

    async fn get_user_devices(&self, user_id: &str) -> Result<Vec<DeviceInfo>, ApiError> {
        let devices = self
            .device_storage
            .get_user_devices(user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get devices", e))?;

        Ok(devices
            .into_iter()
            .map(|device| DeviceInfo {
                device_id: device.device_id,
                user_id: device.user_id,
                display_name: device.display_name,
            })
            .collect())
    }

    async fn verify_device_signature(&self, device: &DeviceInfo) -> Result<bool, ApiError> {
        let signatures = self.cross_signing_storage.get_device_signatures(&device.user_id, &device.device_id).await?;

        Ok(!signatures.is_empty())
    }

    async fn check_cross_signing(&self, device: &DeviceInfo) -> Result<bool, ApiError> {
        Ok(self.cross_signing_storage.get_cross_signing_key(&device.user_id, "self_signing").await?.is_some())
    }
}

/// The `DeviceInfo` struct.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    device_id: String,
    user_id: String,
    display_name: Option<String>,
}
