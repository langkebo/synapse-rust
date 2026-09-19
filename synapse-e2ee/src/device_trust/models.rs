// Device Trust Models
// E2EE Phase 1: Device trust and verification

use serde::{Deserialize, Serialize};
use synapse_common::current_timestamp_millis;
use synapse_common::current_timestamp_utc;

/// Device trust level enum
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum DeviceTrustLevel {
    /// The `Verified` variant.
    Verified, // Fully trusted - can decrypt messages and access history
    #[default]
    /// The `Blocked` variant.
    Unverified, // New device - requires verification
    /// The `Blocked` variant.
    Blocked, // Blocked - cannot decrypt any messages
}

/// Implementation of [`std`] methods.
impl std::fmt::Display for DeviceTrustLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Verified => write!(f, "verified"),
            Self::Unverified => write!(f, "unverified"),
            Self::Blocked => write!(f, "blocked"),
        }
    }
}

/// Implementation of [`std`] methods.
impl std::str::FromStr for DeviceTrustLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "verified" => Ok(Self::Verified),
            "unverified" => Ok(Self::Unverified),
            "blocked" => Ok(Self::Blocked),
            _ => Err(format!("Unknown trust level: {s}")),
        }
    }
}

/// Device trust status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTrustStatus {
    /// The `id` field.
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `trust_level` field.
    /// The `verified_by_device_id` field.
    /// The `verified_at` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub id: i64,
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `trust_level` field.
    /// The `verified_by_device_id` field.
    /// The `verified_at` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `trust_level` field.
    /// The `verified_by_device_id` field.
    /// The `verified_at` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub device_id: String,
    /// The `trust_level` field.
    /// The `verified_by_device_id` field.
    /// The `verified_at` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub trust_level: DeviceTrustLevel,
    /// The `verified_by_device_id` field.
    /// The `verified_at` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub verified_by_device_id: Option<String>,
    /// The `verified_at` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub verified_at: Option<i64>,
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// Implementation of [`DeviceTrustStatus`] methods.
impl DeviceTrustStatus {
    /// See [`new`].
    pub fn new(user_id: &str, device_id: &str) -> Self {
        let now = current_timestamp_millis();
        Self {
            id: 0,
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            trust_level: DeviceTrustLevel::Unverified,
            verified_by_device_id: None,
            verified_at: None,
            created_ts: now,
            updated_ts: now,
        }
    }

    /// See [`verify`].
    pub fn verify(&mut self, verified_by: &str) {
        self.trust_level = DeviceTrustLevel::Verified;
        self.verified_by_device_id = Some(verified_by.to_string());
        self.verified_at = Some(current_timestamp_millis());
        self.updated_ts = current_timestamp_millis();
    }

    /// See [`block`].
    pub fn block(&mut self) {
        self.trust_level = DeviceTrustLevel::Blocked;
        self.verified_by_device_id = None;
        self.verified_at = None;
        self.updated_ts = current_timestamp_millis();
    }
}

/// Verification methods
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum VerificationMethod {
    /// Short Authentication String verification method (default).
    #[default]
    Sas, // Short Authentication String
    /// The `Qr` variant.
    /// The `Emoji` variant.
    Qr, // QR Code
    /// The `Emoji` variant.
    Emoji, // Emoji verification (alias for SAS)
}

/// Implementation of [`std`] methods.
impl std::fmt::Display for VerificationMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sas => write!(f, "sas"),
            Self::Qr => write!(f, "qr"),
            Self::Emoji => write!(f, "emoji"),
        }
    }
}

/// Implementation of [`std`] methods.
impl std::str::FromStr for VerificationMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sas" => Ok(Self::Sas),
            "qr" => Ok(Self::Qr),
            "emoji" => Ok(Self::Emoji),
            _ => Err(format!("Unknown verification method: {s}")),
        }
    }
}

/// Verification request status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum VerificationRequestStatus {
    /// Verification request pending (default).
    #[default]
    Pending, // Waiting for verification
    /// The `Approved` variant.
    /// The `Rejected` variant.
    /// The `Expired` variant.
    Approved, // Verified successfully
    /// The `Rejected` variant.
    /// The `Expired` variant.
    Rejected, // Verification rejected
    /// The `Expired` variant.
    Expired, // Verification timeout
}

/// Implementation of [`std`] methods.
impl std::fmt::Display for VerificationRequestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Approved => write!(f, "approved"),
            Self::Rejected => write!(f, "rejected"),
            Self::Expired => write!(f, "expired"),
        }
    }
}

/// Implementation of [`std`] methods.
impl std::str::FromStr for VerificationRequestStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "expired" => Ok(Self::Expired),
            _ => Err(format!("Unknown verification status: {s}")),
        }
    }
}

/// Device verification request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceVerificationRequest {
    /// The `id` field.
    /// The `user_id` field.
    /// The `new_device_id` field.
    /// The `requesting_device_id` field.
    /// The `verification_method` field.
    /// The `status` field.
    /// The `request_token` field.
    /// The `commitment` field.
    /// The `pubkey` field.
    /// The `created_ts` field.
    /// The `expires_at` field.
    /// The `completed_at` field.
    pub id: i64,
    /// The `user_id` field.
    /// The `new_device_id` field.
    /// The `requesting_device_id` field.
    /// The `verification_method` field.
    /// The `status` field.
    /// The `request_token` field.
    /// The `commitment` field.
    /// The `pubkey` field.
    /// The `created_ts` field.
    /// The `expires_at` field.
    /// The `completed_at` field.
    pub user_id: String,
    /// The `new_device_id` field.
    /// The `requesting_device_id` field.
    /// The `verification_method` field.
    /// The `status` field.
    /// The `request_token` field.
    /// The `commitment` field.
    /// The `pubkey` field.
    /// The `created_ts` field.
    /// The `expires_at` field.
    /// The `completed_at` field.
    pub new_device_id: String,
    /// The `requesting_device_id` field.
    /// The `verification_method` field.
    /// The `status` field.
    /// The `request_token` field.
    /// The `commitment` field.
    /// The `pubkey` field.
    /// The `created_ts` field.
    /// The `expires_at` field.
    /// The `completed_at` field.
    pub requesting_device_id: Option<String>,
    /// The `verification_method` field.
    /// The `status` field.
    /// The `request_token` field.
    /// The `commitment` field.
    /// The `pubkey` field.
    /// The `created_ts` field.
    /// The `expires_at` field.
    /// The `completed_at` field.
    pub verification_method: VerificationMethod,
    /// The `status` field.
    /// The `request_token` field.
    /// The `commitment` field.
    /// The `pubkey` field.
    /// The `created_ts` field.
    /// The `expires_at` field.
    /// The `completed_at` field.
    pub status: VerificationRequestStatus,
    /// The `request_token` field.
    /// The `commitment` field.
    /// The `pubkey` field.
    /// The `created_ts` field.
    /// The `expires_at` field.
    /// The `completed_at` field.
    pub request_token: String,
    /// The `commitment` field.
    /// The `pubkey` field.
    /// The `created_ts` field.
    /// The `expires_at` field.
    /// The `completed_at` field.
    pub commitment: Option<String>,
    /// The `pubkey` field.
    /// The `created_ts` field.
    /// The `expires_at` field.
    /// The `completed_at` field.
    pub pubkey: Option<String>,
    /// The `created_ts` field.
    /// The `expires_at` field.
    /// The `completed_at` field.
    pub created_ts: i64,
    /// The `expires_at` field.
    /// The `completed_at` field.
    pub expires_at: i64,
    /// The `completed_at` field.
    pub completed_at: Option<i64>,
}

/// Implementation of [`DeviceVerificationRequest`] methods.
impl DeviceVerificationRequest {
    /// See [`new`].
    pub fn new(
        user_id: &str,
        new_device_id: &str,
        method: VerificationMethod,
        token: &str,
        expires_minutes: i64,
    ) -> Self {
        let now = current_timestamp_utc();
        Self {
            id: 0,
            user_id: user_id.to_string(),
            new_device_id: new_device_id.to_string(),
            requesting_device_id: None,
            verification_method: method,
            status: VerificationRequestStatus::Pending,
            request_token: token.to_string(),
            commitment: None,
            pubkey: None,
            created_ts: now.timestamp_millis(),
            expires_at: (now + chrono::Duration::minutes(expires_minutes)).timestamp_millis(),
            completed_at: None,
        }
    }

    /// See [`is_expired`].
    pub fn is_expired(&self) -> bool {
        current_timestamp_millis() > self.expires_at
    }

    /// See [`approve`].
    pub fn approve(&mut self) {
        self.status = VerificationRequestStatus::Approved;
        self.completed_at = Some(current_timestamp_millis());
    }

    /// See [`reject`].
    pub fn reject(&mut self) {
        self.status = VerificationRequestStatus::Rejected;
        self.completed_at = Some(current_timestamp_millis());
    }

    /// See [`expire`].
    pub fn expire(&mut self) {
        self.status = VerificationRequestStatus::Expired;
        self.completed_at = Some(current_timestamp_millis());
    }
}

/// Key rotation log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationLog {
    /// The `id` field.
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `room_id` field.
    /// The `rotation_type` field.
    /// The `old_key_id` field.
    /// The `new_key_id` field.
    /// The `reason` field.
    /// The `rotated_at` field.
    pub id: i64,
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `room_id` field.
    /// The `rotation_type` field.
    /// The `old_key_id` field.
    /// The `new_key_id` field.
    /// The `reason` field.
    /// The `rotated_at` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `room_id` field.
    /// The `rotation_type` field.
    /// The `old_key_id` field.
    /// The `new_key_id` field.
    /// The `reason` field.
    /// The `rotated_at` field.
    pub device_id: String,
    /// The `room_id` field.
    /// The `rotation_type` field.
    /// The `old_key_id` field.
    /// The `new_key_id` field.
    /// The `reason` field.
    /// The `rotated_at` field.
    pub room_id: Option<String>,
    /// The `rotation_type` field.
    /// The `old_key_id` field.
    /// The `new_key_id` field.
    /// The `reason` field.
    /// The `rotated_at` field.
    pub rotation_type: String,
    /// The `old_key_id` field.
    /// The `new_key_id` field.
    /// The `reason` field.
    /// The `rotated_at` field.
    pub old_key_id: Option<String>,
    /// The `new_key_id` field.
    /// The `reason` field.
    /// The `rotated_at` field.
    pub new_key_id: Option<String>,
    /// The `reason` field.
    /// The `rotated_at` field.
    pub reason: Option<String>,
    /// The `rotated_at` field.
    pub rotated_at: i64,
}

/// Implementation of [`KeyRotationLog`] methods.
impl KeyRotationLog {
    /// See [`new`].
    pub fn new(user_id: &str, device_id: &str, rotation_type: &str) -> Self {
        Self {
            id: 0,
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            room_id: None,
            rotation_type: rotation_type.to_string(),
            old_key_id: None,
            new_key_id: None,
            reason: None,
            rotated_at: current_timestamp_millis(),
        }
    }

    /// See [`with_room`].
    pub fn with_room(mut self, room_id: &str) -> Self {
        self.room_id = Some(room_id.to_string());
        self
    }

    /// See [`with_keys`].
    pub fn with_keys(mut self, old_key: &str, new_key: &str) -> Self {
        self.old_key_id = Some(old_key.to_string());
        self.new_key_id = Some(new_key.to_string());
        self
    }

    /// See [`with_reason`].
    pub fn with_reason(mut self, reason: &str) -> Self {
        self.reason = Some(reason.to_string());
        self
    }
}

/// E2EE security event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2eeSecurityEvent {
    /// The `id` field.
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `event_type` field.
    /// The `event_data` field.
    /// The `ip_address` field.
    /// The `user_agent` field.
    /// The `created_ts` field.
    pub id: i64,
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `event_type` field.
    /// The `event_data` field.
    /// The `ip_address` field.
    /// The `user_agent` field.
    /// The `created_ts` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `event_type` field.
    /// The `event_data` field.
    /// The `ip_address` field.
    /// The `user_agent` field.
    /// The `created_ts` field.
    pub device_id: Option<String>,
    /// The `event_type` field.
    /// The `event_data` field.
    /// The `ip_address` field.
    /// The `user_agent` field.
    /// The `created_ts` field.
    pub event_type: String,
    /// The `event_data` field.
    /// The `ip_address` field.
    /// The `user_agent` field.
    /// The `created_ts` field.
    pub event_data: Option<serde_json::Value>,
    /// The `ip_address` field.
    /// The `user_agent` field.
    /// The `created_ts` field.
    pub ip_address: Option<String>,
    /// The `user_agent` field.
    /// The `created_ts` field.
    pub user_agent: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// Implementation of [`E2eeSecurityEvent`] methods.
impl E2eeSecurityEvent {
    /// See [`new`].
    pub fn new(user_id: &str, event_type: &str) -> Self {
        Self {
            id: 0,
            user_id: user_id.to_string(),
            device_id: None,
            event_type: event_type.to_string(),
            event_data: None,
            ip_address: None,
            user_agent: None,
            created_ts: current_timestamp_millis(),
        }
    }

    /// See [`with_device`].
    pub fn with_device(mut self, device_id: &str) -> Self {
        self.device_id = Some(device_id.to_string());
        self
    }

    /// See [`with_data`].
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.event_data = Some(data);
        self
    }

    /// See [`with_ip`].
    pub fn with_ip(mut self, ip: &str) -> Self {
        self.ip_address = Some(ip.to_string());
        self
    }

    /// See [`with_user_agent`].
    pub fn with_user_agent(mut self, ua: &str) -> Self {
        self.user_agent = Some(ua.to_string());
        self
    }
}

/// Cross-signing trust relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSigningTrust {
    /// The `id` field.
    /// The `user_id` field.
    /// The `target_user_id` field.
    /// The `master_key_id` field.
    /// The `is_trusted` field.
    /// The `trusted_at` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub id: i64,
    /// The `user_id` field.
    /// The `target_user_id` field.
    /// The `master_key_id` field.
    /// The `is_trusted` field.
    /// The `trusted_at` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub user_id: String,
    /// The `target_user_id` field.
    /// The `master_key_id` field.
    /// The `is_trusted` field.
    /// The `trusted_at` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub target_user_id: String,
    /// The `master_key_id` field.
    /// The `is_trusted` field.
    /// The `trusted_at` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub master_key_id: Option<String>,
    /// The `is_trusted` field.
    /// The `trusted_at` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub is_trusted: bool,
    /// The `trusted_at` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub trusted_at: Option<i64>,
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// Security summary for a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecuritySummary {
    /// The `verified_devices` field.
    /// The `unverified_devices` field.
    /// The `blocked_devices` field.
    /// The `has_cross_signing_master` field.
    /// The `security_score` field.
    /// The `recommendations` field.
    pub verified_devices: i64,
    /// The `unverified_devices` field.
    /// The `blocked_devices` field.
    /// The `has_cross_signing_master` field.
    /// The `security_score` field.
    /// The `recommendations` field.
    pub unverified_devices: i64,
    /// The `blocked_devices` field.
    /// The `has_cross_signing_master` field.
    /// The `security_score` field.
    /// The `recommendations` field.
    pub blocked_devices: i64,
    /// The `has_cross_signing_master` field.
    /// The `security_score` field.
    /// The `recommendations` field.
    pub has_cross_signing_master: bool,
    /// The `security_score` field.
    /// The `recommendations` field.
    pub security_score: f64,
    /// The `recommendations` field.
    pub recommendations: Vec<String>,
}

/// Implementation of [`SecuritySummary`] methods.
impl SecuritySummary {
    /// See [`calculate`].
    pub fn calculate(verified: i64, unverified: i64, blocked: i64, has_master_key: bool) -> Self {
        let total = verified + unverified + blocked;
        let mut score = 100.0;

        // Deduct for unverified devices
        if total > 0 {
            score -= (unverified as f64 / total as f64) * 50.0;
            score -= (blocked as f64 / total as f64) * 30.0;
        }

        // Bonus for having cross-signing
        if !has_master_key {
            score -= 20.0;
        }

        score = score.clamp(0.0, 100.0);

        // Generate recommendations
        let mut recommendations = Vec::new();
        if unverified > 0 {
            recommendations.push("Consider verifying your new devices from an existing trusted device".to_string());
        }
        if blocked > 0 {
            recommendations.push("Review and unblock any devices that were mistakenly blocked".to_string());
        }
        if !has_master_key {
            recommendations.push("Set up cross-signing to automatically trust your devices".to_string());
        }

        Self {
            verified_devices: verified,
            unverified_devices: unverified,
            blocked_devices: blocked,
            has_cross_signing_master: has_master_key,
            security_score: score,
            recommendations,
        }
    }
}

/// API request/response types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRequestRequest {
    /// The `new_device_id` field.
    /// The `method` field.
    pub new_device_id: String,
    /// The `method` field.
    pub method: String, // "sas", "qr", "emoji"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `VerificationRequestResponse` type.
pub struct VerificationRequestResponse {
    /// The `request_token` field.
    /// The `status` field.
    /// The `expires_at` field.
    /// The `methods_available` field.
    pub request_token: String,
    /// The `status` field.
    /// The `expires_at` field.
    /// The `methods_available` field.
    pub status: String,
    /// The `expires_at` field.
    /// The `methods_available` field.
    pub expires_at: i64,
    /// The `methods_available` field.
    pub methods_available: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `VerificationRespondRequest` type.
pub struct VerificationRespondRequest {
    /// The `request_token` field.
    /// The `approved` field.
    pub request_token: String,
    /// The `approved` field.
    pub approved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `VerificationRespondResponse` type.
pub struct VerificationRespondResponse {
    /// The `success` field.
    /// The `trust_level` field.
    pub success: bool,
    /// The `trust_level` field.
    pub trust_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `DeviceTrustStatusResponse` type.
pub struct DeviceTrustStatusResponse {
    /// The `device_id` field.
    /// The `trust_level` field.
    /// The `verified_at` field.
    /// The `verified_by` field.
    pub device_id: String,
    /// The `trust_level` field.
    /// The `verified_at` field.
    /// The `verified_by` field.
    pub trust_level: String,
    /// The `verified_at` field.
    /// The `verified_by` field.
    pub verified_at: Option<i64>,
    /// The `verified_by` field.
    pub verified_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `DeviceTrustListResponse` type.
pub struct DeviceTrustListResponse {
    /// The `devices` field.
    pub devices: Vec<DeviceTrustStatusResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SecuritySummaryResponse` type.
pub struct SecuritySummaryResponse {
    /// The `verified_devices` field.
    /// The `unverified_devices` field.
    /// The `blocked_devices` field.
    /// The `has_cross_signing_master` field.
    /// The `security_score` field.
    /// The `recommendations` field.
    pub verified_devices: i64,
    /// The `unverified_devices` field.
    /// The `blocked_devices` field.
    /// The `has_cross_signing_master` field.
    /// The `security_score` field.
    /// The `recommendations` field.
    pub unverified_devices: i64,
    /// The `blocked_devices` field.
    /// The `has_cross_signing_master` field.
    /// The `security_score` field.
    /// The `recommendations` field.
    pub blocked_devices: i64,
    /// The `has_cross_signing_master` field.
    /// The `security_score` field.
    /// The `recommendations` field.
    pub has_cross_signing_master: bool,
    /// The `security_score` field.
    /// The `recommendations` field.
    pub security_score: f64,
    /// The `recommendations` field.
    pub recommendations: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_trust_status_new() {
        let status = DeviceTrustStatus::new("@user:example.com", "DEVICE123");
        assert_eq!(status.trust_level, DeviceTrustLevel::Unverified);
        assert_eq!(status.verified_by_device_id, None);
    }

    #[test]
    fn test_device_trust_status_verify() {
        let mut status = DeviceTrustStatus::new("@user:example.com", "DEVICE123");
        status.verify("DEVICE_OLD");

        assert_eq!(status.trust_level, DeviceTrustLevel::Verified);
        assert_eq!(status.verified_by_device_id, Some("DEVICE_OLD".to_string()));
        assert!(status.verified_at.is_some());
    }

    #[test]
    fn test_device_trust_status_block() {
        let mut status = DeviceTrustStatus::new("@user:example.com", "DEVICE123");
        status.block();

        assert_eq!(status.trust_level, DeviceTrustLevel::Blocked);
    }

    #[test]
    fn test_verification_request_new() {
        let request =
            DeviceVerificationRequest::new("@user:example.com", "DEVICE_NEW", VerificationMethod::Sas, "token123", 5);

        assert_eq!(request.status, VerificationRequestStatus::Pending);
        assert!(!request.is_expired());
    }

    #[test]
    fn test_key_rotation_log() {
        let log = KeyRotationLog::new("@user:example.com", "DEVICE123", "megolm")
            .with_room("!room:example.com")
            .with_keys("old_key", "new_key")
            .with_reason("scheduled");

        assert_eq!(log.rotation_type, "megolm");
        assert_eq!(log.room_id, Some("!room:example.com".to_string()));
    }

    #[test]
    fn test_security_summary() {
        let summary = SecuritySummary::calculate(3, 1, 0, true);

        assert_eq!(summary.verified_devices, 3);
        assert_eq!(summary.unverified_devices, 1);
        assert!(summary.security_score > 50.0);
        assert!(!summary.recommendations.is_empty());
    }

    #[test]
    fn test_security_summary_low_score() {
        let summary = SecuritySummary::calculate(1, 3, 1, false);

        assert_eq!(summary.verified_devices, 1);
        assert_eq!(summary.unverified_devices, 3);
        assert!(summary.security_score < 50.0);
    }

    // =========================================================================
    // B.3 batch 4/6 — supplemental coverage for Display/FromStr impls and
    // DeviceVerificationRequest state transitions.
    // =========================================================================

    #[test]
    fn test_device_trust_level_display() {
        assert_eq!(DeviceTrustLevel::Verified.to_string(), "verified");
        assert_eq!(DeviceTrustLevel::Unverified.to_string(), "unverified");
        assert_eq!(DeviceTrustLevel::Blocked.to_string(), "blocked");
    }

    #[test]
    fn test_device_trust_level_from_str_valid() {
        assert_eq!("verified".parse::<DeviceTrustLevel>().unwrap(), DeviceTrustLevel::Verified);
        assert_eq!("unverified".parse::<DeviceTrustLevel>().unwrap(), DeviceTrustLevel::Unverified);
        assert_eq!("blocked".parse::<DeviceTrustLevel>().unwrap(), DeviceTrustLevel::Blocked);
        // Case-insensitive.
        assert_eq!("VERIFIED".parse::<DeviceTrustLevel>().unwrap(), DeviceTrustLevel::Verified);
    }

    #[test]
    fn test_device_trust_level_from_str_invalid() {
        assert!("unknown".parse::<DeviceTrustLevel>().is_err());
    }

    #[test]
    fn test_verification_method_display() {
        assert_eq!(VerificationMethod::Sas.to_string(), "sas");
        assert_eq!(VerificationMethod::Qr.to_string(), "qr");
        assert_eq!(VerificationMethod::Emoji.to_string(), "emoji");
    }

    #[test]
    fn test_verification_method_from_str_valid() {
        assert_eq!("sas".parse::<VerificationMethod>().unwrap(), VerificationMethod::Sas);
        assert_eq!("qr".parse::<VerificationMethod>().unwrap(), VerificationMethod::Qr);
        assert_eq!("emoji".parse::<VerificationMethod>().unwrap(), VerificationMethod::Emoji);
        assert_eq!("SAS".parse::<VerificationMethod>().unwrap(), VerificationMethod::Sas);
    }

    #[test]
    fn test_verification_method_from_str_invalid() {
        assert!("unknown".parse::<VerificationMethod>().is_err());
    }

    #[test]
    fn test_verification_request_status_display() {
        assert_eq!(VerificationRequestStatus::Pending.to_string(), "pending");
        assert_eq!(VerificationRequestStatus::Approved.to_string(), "approved");
        assert_eq!(VerificationRequestStatus::Rejected.to_string(), "rejected");
        assert_eq!(VerificationRequestStatus::Expired.to_string(), "expired");
    }

    #[test]
    fn test_verification_request_status_from_str_valid() {
        assert_eq!("pending".parse::<VerificationRequestStatus>().unwrap(), VerificationRequestStatus::Pending);
        assert_eq!("approved".parse::<VerificationRequestStatus>().unwrap(), VerificationRequestStatus::Approved);
        assert_eq!("rejected".parse::<VerificationRequestStatus>().unwrap(), VerificationRequestStatus::Rejected);
        assert_eq!("expired".parse::<VerificationRequestStatus>().unwrap(), VerificationRequestStatus::Expired);
        assert_eq!("PENDING".parse::<VerificationRequestStatus>().unwrap(), VerificationRequestStatus::Pending);
    }

    #[test]
    fn test_verification_request_status_from_str_invalid() {
        assert!("unknown".parse::<VerificationRequestStatus>().is_err());
    }

    #[test]
    fn test_verification_request_approve() {
        let mut request =
            DeviceVerificationRequest::new("@user:example.com", "DEVICE_NEW", VerificationMethod::Sas, "token123", 5);
        request.approve();
        assert_eq!(request.status, VerificationRequestStatus::Approved);
        assert!(request.completed_at.is_some());
    }

    #[test]
    fn test_verification_request_reject() {
        let mut request =
            DeviceVerificationRequest::new("@user:example.com", "DEVICE_NEW", VerificationMethod::Sas, "token123", 5);
        request.reject();
        assert_eq!(request.status, VerificationRequestStatus::Rejected);
        assert!(request.completed_at.is_some());
    }

    #[test]
    fn test_verification_request_expire() {
        let mut request =
            DeviceVerificationRequest::new("@user:example.com", "DEVICE_NEW", VerificationMethod::Sas, "token123", 5);
        request.expire();
        assert_eq!(request.status, VerificationRequestStatus::Expired);
        assert!(request.completed_at.is_some());
    }

    #[test]
    fn test_verification_request_is_expired_after_expiry() {
        // expires_minutes = -1 → already expired.
        let request =
            DeviceVerificationRequest::new("@user:example.com", "DEVICE_NEW", VerificationMethod::Sas, "token123", -1);
        assert!(request.is_expired());
    }

    #[test]
    fn test_key_rotation_log_with_keys_and_reason() {
        let log = KeyRotationLog::new("@user:example.com", "DEVICE123", "olm")
            .with_keys("old_key_id", "new_key_id")
            .with_reason("manual");
        assert_eq!(log.old_key_id, Some("old_key_id".to_string()));
        assert_eq!(log.new_key_id, Some("new_key_id".to_string()));
        assert_eq!(log.reason, Some("manual".to_string()));
    }
}
