// Device Trust Models
// E2EE Phase 1: Device trust and verification

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Device trust level enum
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum DeviceTrustLevel {
    Verified, // Fully trusted - can decrypt messages and access history
    #[default]
    Unverified, // New device - requires verification
    Blocked,  // Blocked - cannot decrypt any messages
}

impl std::fmt::Display for DeviceTrustLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceTrustLevel::Verified => write!(f, "verified"),
            DeviceTrustLevel::Unverified => write!(f, "unverified"),
            DeviceTrustLevel::Blocked => write!(f, "blocked"),
        }
    }
}

impl std::str::FromStr for DeviceTrustLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "verified" => Ok(DeviceTrustLevel::Verified),
            "unverified" => Ok(DeviceTrustLevel::Unverified),
            "blocked" => Ok(DeviceTrustLevel::Blocked),
            _ => Err(format!("Unknown trust level: {}", s)),
        }
    }
}

/// Device trust status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTrustStatus {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub trust_level: DeviceTrustLevel,
    pub verified_by_device_id: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl DeviceTrustStatus {
    pub fn new(user_id: &str, device_id: &str) -> Self {
        let now = Utc::now();
        Self {
            id: 0,
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            trust_level: DeviceTrustLevel::Unverified,
            verified_by_device_id: None,
            verified_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn verify(&mut self, verified_by: &str) {
        self.trust_level = DeviceTrustLevel::Verified;
        self.verified_by_device_id = Some(verified_by.to_string());
        self.verified_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn block(&mut self) {
        self.trust_level = DeviceTrustLevel::Blocked;
        self.verified_by_device_id = None;
        self.verified_at = None;
        self.updated_at = Utc::now();
    }
}

/// Verification methods
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum VerificationMethod {
    #[default]
    Sas, // Short Authentication String
    Qr,    // QR Code
    Emoji, // Emoji verification (alias for SAS)
}

impl std::fmt::Display for VerificationMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationMethod::Sas => write!(f, "sas"),
            VerificationMethod::Qr => write!(f, "qr"),
            VerificationMethod::Emoji => write!(f, "emoji"),
        }
    }
}

impl std::str::FromStr for VerificationMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sas" => Ok(VerificationMethod::Sas),
            "qr" => Ok(VerificationMethod::Qr),
            "emoji" => Ok(VerificationMethod::Emoji),
            _ => Err(format!("Unknown verification method: {}", s)),
        }
    }
}

/// Verification request status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum VerificationRequestStatus {
    #[default]
    Pending, // Waiting for verification
    Approved, // Verified successfully
    Rejected, // Verification rejected
    Expired,  // Verification timeout
}

impl std::fmt::Display for VerificationRequestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationRequestStatus::Pending => write!(f, "pending"),
            VerificationRequestStatus::Approved => write!(f, "approved"),
            VerificationRequestStatus::Rejected => write!(f, "rejected"),
            VerificationRequestStatus::Expired => write!(f, "expired"),
        }
    }
}

impl std::str::FromStr for VerificationRequestStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(VerificationRequestStatus::Pending),
            "approved" => Ok(VerificationRequestStatus::Approved),
            "rejected" => Ok(VerificationRequestStatus::Rejected),
            "expired" => Ok(VerificationRequestStatus::Expired),
            _ => Err(format!("Unknown verification status: {}", s)),
        }
    }
}

/// Device verification request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceVerificationRequest {
    pub id: i64,
    pub user_id: String,
    pub new_device_id: String,
    pub requesting_device_id: Option<String>,
    pub verification_method: VerificationMethod,
    pub status: VerificationRequestStatus,
    pub request_token: String,
    pub commitment: Option<String>,
    pub pubkey: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl DeviceVerificationRequest {
    pub fn new(
        user_id: &str,
        new_device_id: &str,
        method: VerificationMethod,
        token: &str,
        expires_minutes: i64,
    ) -> Self {
        let now = Utc::now();
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
            created_at: now,
            expires_at: now + chrono::Duration::minutes(expires_minutes),
            completed_at: None,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    pub fn approve(&mut self) {
        self.status = VerificationRequestStatus::Approved;
        self.completed_at = Some(Utc::now());
    }

    pub fn reject(&mut self) {
        self.status = VerificationRequestStatus::Rejected;
        self.completed_at = Some(Utc::now());
    }

    pub fn expire(&mut self) {
        self.status = VerificationRequestStatus::Expired;
        self.completed_at = Some(Utc::now());
    }
}

/// Key rotation log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationLog {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub room_id: Option<String>,
    pub rotation_type: String,
    pub old_key_id: Option<String>,
    pub new_key_id: Option<String>,
    pub reason: Option<String>,
    pub rotated_at: DateTime<Utc>,
}

impl KeyRotationLog {
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
            rotated_at: Utc::now(),
        }
    }

    pub fn with_room(mut self, room_id: &str) -> Self {
        self.room_id = Some(room_id.to_string());
        self
    }

    pub fn with_keys(mut self, old_key: &str, new_key: &str) -> Self {
        self.old_key_id = Some(old_key.to_string());
        self.new_key_id = Some(new_key.to_string());
        self
    }

    pub fn with_reason(mut self, reason: &str) -> Self {
        self.reason = Some(reason.to_string());
        self
    }
}

/// E2EE security event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2eeSecurityEvent {
    pub id: i64,
    pub user_id: String,
    pub device_id: Option<String>,
    pub event_type: String,
    pub event_data: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl E2eeSecurityEvent {
    pub fn new(user_id: &str, event_type: &str) -> Self {
        Self {
            id: 0,
            user_id: user_id.to_string(),
            device_id: None,
            event_type: event_type.to_string(),
            event_data: None,
            ip_address: None,
            user_agent: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_device(mut self, device_id: &str) -> Self {
        self.device_id = Some(device_id.to_string());
        self
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.event_data = Some(data);
        self
    }

    pub fn with_ip(mut self, ip: &str) -> Self {
        self.ip_address = Some(ip.to_string());
        self
    }

    pub fn with_user_agent(mut self, ua: &str) -> Self {
        self.user_agent = Some(ua.to_string());
        self
    }
}

/// Cross-signing trust relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSigningTrust {
    pub id: i64,
    pub user_id: String,
    pub target_user_id: String,
    pub master_key_id: Option<String>,
    pub is_trusted: bool,
    pub trusted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Security summary for a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecuritySummary {
    pub verified_devices: i64,
    pub unverified_devices: i64,
    pub blocked_devices: i64,
    pub has_cross_signing_master: bool,
    pub security_score: f64,
    pub recommendations: Vec<String>,
}

impl SecuritySummary {
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
            recommendations.push(
                "Consider verifying your new devices from an existing trusted device".to_string(),
            );
        }
        if blocked > 0 {
            recommendations
                .push("Review and unblock any devices that were mistakenly blocked".to_string());
        }
        if !has_master_key {
            recommendations
                .push("Set up cross-signing to automatically trust your devices".to_string());
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
    pub new_device_id: String,
    pub method: String, // "sas", "qr", "emoji"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRequestResponse {
    pub request_token: String,
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub methods_available: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRespondRequest {
    pub request_token: String,
    pub approved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRespondResponse {
    pub success: bool,
    pub trust_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTrustStatusResponse {
    pub device_id: String,
    pub trust_level: String,
    pub verified_at: Option<DateTime<Utc>>,
    pub verified_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTrustListResponse {
    pub devices: Vec<DeviceTrustStatusResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecuritySummaryResponse {
    pub verified_devices: i64,
    pub unverified_devices: i64,
    pub blocked_devices: i64,
    pub has_cross_signing_master: bool,
    pub security_score: f64,
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
        let request = DeviceVerificationRequest::new(
            "@user:example.com",
            "DEVICE_NEW",
            VerificationMethod::Sas,
            "token123",
            5,
        );

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
}
