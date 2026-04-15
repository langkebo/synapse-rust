// Device Trust Service
// E2EE Phase 1: Core business logic for device trust and verification

use crate::e2ee::cross_signing::CrossSigningService;
use crate::e2ee::device_keys::DeviceKeyService;
use crate::e2ee::device_trust::models::*;
use crate::e2ee::device_trust::storage::DeviceTrustStorage;
use crate::e2ee::verification::VerificationService;
use crate::error::ApiError;
use std::sync::Arc;

pub struct DeviceTrustService {
    storage: Arc<DeviceTrustStorage>,
    verification: Arc<VerificationService>,
    cross_signing: Arc<CrossSigningService>,
    device_keys: Arc<DeviceKeyService>,
    config: DeviceTrustConfig,
}

#[derive(Clone)]
pub struct DeviceTrustConfig {
    pub verification_timeout_minutes: i64,
    pub max_unverified_devices: i64,
    pub require_verification_for_history: bool,
}

impl Default for DeviceTrustConfig {
    fn default() -> Self {
        Self {
            verification_timeout_minutes: 5,
            max_unverified_devices: 3,
            require_verification_for_history: true,
        }
    }
}

impl Clone for DeviceTrustService {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            verification: self.verification.clone(),
            cross_signing: self.cross_signing.clone(),
            device_keys: self.device_keys.clone(),
            config: self.config.clone(),
        }
    }
}

impl DeviceTrustService {
    pub fn new(
        storage: Arc<DeviceTrustStorage>,
        verification: Arc<VerificationService>,
        cross_signing: Arc<CrossSigningService>,
        device_keys: Arc<DeviceKeyService>,
    ) -> Self {
        Self {
            storage,
            verification,
            cross_signing,
            device_keys,
            config: DeviceTrustConfig::default(),
        }
    }

    pub fn with_config(
        storage: Arc<DeviceTrustStorage>,
        verification: Arc<VerificationService>,
        cross_signing: Arc<CrossSigningService>,
        device_keys: Arc<DeviceKeyService>,
        config: DeviceTrustConfig,
    ) -> Self {
        Self {
            storage,
            verification,
            cross_signing,
            device_keys,
            config,
        }
    }

    // =====================================================
    // Device Trust Operations
    // =====================================================

    /// Check if a device can access message history
    pub async fn can_access_history(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<bool, ApiError> {
        // If verification is not required, allow access
        if !self.config.require_verification_for_history {
            return Ok(true);
        }

        let trust = self.storage.get_device_trust(user_id, device_id).await?;

        match trust {
            Some(status) => {
                // Verified devices can access history
                Ok(status.trust_level == DeviceTrustLevel::Verified)
            }
            None => {
                // New device - check if it's verified now
                let devices = self.storage.get_all_devices_with_trust(user_id).await?;
                let verified_count = devices
                    .iter()
                    .filter(|d| d.trust_level == DeviceTrustLevel::Verified)
                    .count();

                // If user has other verified devices, require verification
                // Otherwise, allow access (first device)
                Ok(verified_count == 0)
            }
        }
    }

    /// Check if a device can decrypt messages
    pub async fn can_decrypt_messages(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<bool, ApiError> {
        let trust = self.storage.get_device_trust(user_id, device_id).await?;

        match trust {
            Some(status) => {
                // Blocked devices cannot decrypt
                Ok(status.trust_level != DeviceTrustLevel::Blocked)
            }
            None => {
                if self.config.require_verification_for_history {
                    Ok(false)
                } else {
                    Ok(true)
                }
            }
        }
    }

    /// Get trust status for a device
    pub async fn get_device_trust_status(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<DeviceTrustStatusResponse>, ApiError> {
        let trust = self.storage.get_device_trust(user_id, device_id).await?;

        Ok(trust.map(|t| DeviceTrustStatusResponse {
            device_id: t.device_id,
            trust_level: t.trust_level.to_string(),
            verified_at: t.verified_at,
            verified_by: t.verified_by_device_id,
        }))
    }

    /// Get all devices with trust status for a user
    pub async fn get_all_devices_with_trust(
        &self,
        user_id: &str,
    ) -> Result<Vec<DeviceTrustStatusResponse>, ApiError> {
        let devices = self.storage.get_all_devices_with_trust(user_id).await?;

        Ok(devices
            .into_iter()
            .map(|t| DeviceTrustStatusResponse {
                device_id: t.device_id,
                trust_level: t.trust_level.to_string(),
                verified_at: t.verified_at,
                verified_by: t.verified_by_device_id,
            })
            .collect())
    }

    /// Set device trust level
    pub async fn set_device_trust(
        &self,
        user_id: &str,
        device_id: &str,
        level: DeviceTrustLevel,
        verified_by: Option<&str>,
    ) -> Result<(), ApiError> {
        self.storage
            .set_device_trust(user_id, device_id, level, verified_by)
            .await?;

        // Log security event
        let event = E2eeSecurityEvent::new(
            user_id,
            match level {
                DeviceTrustLevel::Verified => "device_verified",
                DeviceTrustLevel::Unverified => "device_unverified",
                DeviceTrustLevel::Blocked => "device_blocked",
            },
        )
        .with_device(device_id)
        .with_data(serde_json::json!({
            "verified_by": verified_by,
            "previous_action": "trust_level_changed"
        }));

        let _ = self.storage.log_security_event(&event).await;

        Ok(())
    }

    // =====================================================
    // Verification Request Operations
    // =====================================================

    /// Request verification for a new device
    pub async fn request_device_verification(
        &self,
        user_id: &str,
        new_device_id: &str,
        method: VerificationMethod,
        requesting_device_id: Option<&str>,
    ) -> Result<VerificationRequestResponse, ApiError> {
        // Check if there's already a pending request
        if let Some(existing) = self
            .storage
            .get_pending_request(user_id, new_device_id)
            .await?
        {
            return Ok(VerificationRequestResponse {
                request_token: existing.request_token,
                status: existing.status.to_string(),
                expires_at: existing.expires_at,
                methods_available: vec!["sas".to_string(), "qr".to_string(), "emoji".to_string()],
            });
        }

        // Generate verification token
        let token = generate_verification_token();

        // Create verification request
        let mut request = DeviceVerificationRequest::new(
            user_id,
            new_device_id,
            method,
            &token,
            self.config.verification_timeout_minutes,
        );
        request.requesting_device_id = requesting_device_id.map(|s| s.to_string());

        // Generate SAS keypair for verification
        let (_secret_key, public_key) = self.verification.generate_key_pair();

        // Calculate commitment
        let commitment = self
            .verification
            .compute_mac(
                std::slice::from_ref(&public_key),
                &[0u8; 32],
                "verification",
            )
            .map_err(|e| ApiError::internal(format!("Failed to compute commitment: {}", e)))?;

        request.commitment = Some(commitment);
        request.pubkey = Some(public_key);

        // Store request
        self.storage.create_verification_request(&request).await?;

        // Log security event
        let event = E2eeSecurityEvent::new(user_id, "verification_requested")
            .with_device(new_device_id)
            .with_data(serde_json::json!({
                "method": method.to_string()
            }));

        let _ = self.storage.log_security_event(&event).await;

        Ok(VerificationRequestResponse {
            request_token: request.request_token,
            status: request.status.to_string(),
            expires_at: request.expires_at,
            methods_available: vec!["sas".to_string(), "qr".to_string(), "emoji".to_string()],
        })
    }

    /// Respond to a verification request
    pub async fn respond_to_verification(
        &self,
        user_id: &str,
        token: &str,
        approved: bool,
    ) -> Result<VerificationRespondResponse, ApiError> {
        // Get the request
        let request = self
            .storage
            .get_request_by_token(token)
            .await?
            .ok_or_else(|| ApiError::not_found("Verification request not found"))?;

        // Verify it's for this user
        if request.user_id != user_id {
            return Err(ApiError::forbidden(
                "Verification request does not match user",
            ));
        }

        // Check if expired
        if request.is_expired() {
            return Err(ApiError::bad_request("Verification request has expired"));
        }

        // Check if already processed
        if request.status != VerificationRequestStatus::Pending {
            return Err(ApiError::bad_request(
                "Verification request already processed",
            ));
        }

        // Update request status
        if approved {
            self.storage
                .update_request_status(token, VerificationRequestStatus::Approved)
                .await?;

            // Update device trust status
            self.storage
                .set_device_trust(
                    &request.user_id,
                    &request.new_device_id,
                    DeviceTrustLevel::Verified,
                    request.requesting_device_id.as_deref(),
                )
                .await?;

            // Log security event
            let event = E2eeSecurityEvent::new(user_id, "device_verified")
                .with_device(&request.new_device_id)
                .with_data(serde_json::json!({
                    "method": request.verification_method.to_string(),
                    "verified_by": request.requesting_device_id
                }));

            let _ = self.storage.log_security_event(&event).await;

            Ok(VerificationRespondResponse {
                success: true,
                trust_level: Some("verified".to_string()),
            })
        } else {
            self.storage
                .update_request_status(token, VerificationRequestStatus::Rejected)
                .await?;

            self.storage
                .set_device_trust(
                    &request.user_id,
                    &request.new_device_id,
                    DeviceTrustLevel::Unverified,
                    None,
                )
                .await?;

            // Log security event
            let event = E2eeSecurityEvent::new(user_id, "verification_rejected")
                .with_device(&request.new_device_id)
                .with_data(serde_json::json!({
                    "reason": "rejected_by_user"
                }));

            let _ = self.storage.log_security_event(&event).await;

            Ok(VerificationRespondResponse {
                success: true,
                trust_level: Some("unverified".to_string()),
            })
        }
    }

    /// Get verification request status
    pub async fn get_verification_status(
        &self,
        user_id: &str,
        token: &str,
    ) -> Result<Option<VerificationRequestResponse>, ApiError> {
        let request = self.storage.get_request_by_token(token).await?;

        Ok(request.map(|r| {
            // Don't reveal if it belongs to another user
            if r.user_id != user_id {
                return VerificationRequestResponse {
                    request_token: token.to_string(),
                    status: "not_found".to_string(),
                    expires_at: chrono::Utc::now(),
                    methods_available: vec![],
                };
            }

            VerificationRequestResponse {
                request_token: r.request_token,
                status: r.status.to_string(),
                expires_at: r.expires_at,
                methods_available: vec!["sas".to_string(), "qr".to_string(), "emoji".to_string()],
            }
        }))
    }

    // =====================================================
    // Security Summary
    // =====================================================

    /// Get security summary for a user
    pub async fn get_security_summary(
        &self,
        user_id: &str,
    ) -> Result<SecuritySummaryResponse, ApiError> {
        let (verified, unverified, blocked) = self.storage.count_devices_by_trust(user_id).await?;
        let has_master_key = self.storage.has_cross_signing_master_key(user_id).await?;

        let summary = SecuritySummary::calculate(verified, unverified, blocked, has_master_key);

        Ok(SecuritySummaryResponse {
            verified_devices: summary.verified_devices,
            unverified_devices: summary.unverified_devices,
            blocked_devices: summary.blocked_devices,
            has_cross_signing_master: summary.has_cross_signing_master,
            security_score: summary.security_score,
            recommendations: summary.recommendations,
        })
    }

    // =====================================================
    // Auto-verification via Cross-Signing
    // =====================================================

    /// Try to auto-verify a device via cross-signing
    pub async fn try_auto_verify_via_cross_signing(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<bool, ApiError> {
        // Get user's cross-signing keys
        let cross_signing_keys = self.cross_signing.get_cross_signing_keys(user_id).await;

        if cross_signing_keys.is_err() {
            return Ok(false);
        }

        // Verify device is signed by cross-signing
        // This will check if the device keys are signed by the user's cross-signing key
        let verification_status = self
            .cross_signing
            .verify_device_signature(user_id, device_id)
            .await?;

        if verification_status.is_verified {
            // Auto-verify the device
            self.storage
                .set_device_trust(
                    user_id,
                    device_id,
                    DeviceTrustLevel::Verified,
                    Some("cross_signing"),
                )
                .await?;

            // Log security event
            let event = E2eeSecurityEvent::new(user_id, "device_auto_verified")
                .with_device(device_id)
                .with_data(serde_json::json!({
                    "method": "cross_signing",
                    "automatic": true
                }));

            let _ = self.storage.log_security_event(&event).await;

            return Ok(true);
        }

        Ok(false)
    }

    // =====================================================
    // Cleanup
    // =====================================================

    /// Clean up expired verification requests
    pub async fn cleanup_expired_requests(&self) -> Result<i64, ApiError> {
        let count = self.storage.cleanup_expired_requests().await?;

        if count > 0 {
            tracing::info!("Cleaned up {} expired verification requests", count);
        }

        Ok(count)
    }
}

// =====================================================
// Helper Functions
// =====================================================

/// Generate a secure verification token
fn generate_verification_token() -> String {
    use base64::Engine;
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_verification_token() {
        let token1 = generate_verification_token();
        let token2 = generate_verification_token();

        assert_ne!(token1, token2);
        assert!(token1.len() > 20);
    }

    #[test]
    fn test_verification_timeout() {
        let config = DeviceTrustConfig::default();
        assert_eq!(config.verification_timeout_minutes, 5);
    }
}
