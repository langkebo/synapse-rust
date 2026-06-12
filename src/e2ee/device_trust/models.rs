// Device Trust Models
// E2EE Phase 1: Device trust and verification

use chrono::Utc;
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
            Self::Verified => write!(f, "verified"),
            Self::Unverified => write!(f, "unverified"),
            Self::Blocked => write!(f, "blocked"),
        }
    }
}

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
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub trust_level: DeviceTrustLevel,
    pub verified_by_device_id: Option<String>,
    pub verified_at: Option<i64>,
    pub created_ts: i64,
    pub updated_ts: i64,
}

impl DeviceTrustStatus {
    pub fn new(user_id: &str, device_id: &str) -> Self {
        let now = Utc::now().timestamp_millis();
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

    pub fn verify(&mut self, verified_by: &str) {
        self.trust_level = DeviceTrustLevel::Verified;
        self.verified_by_device_id = Some(verified_by.to_string());
        self.verified_at = Some(Utc::now().timestamp_millis());
        self.updated_ts = Utc::now().timestamp_millis();
    }

    pub fn block(&mut self) {
        self.trust_level = DeviceTrustLevel::Blocked;
        self.verified_by_device_id = None;
        self.verified_at = None;
        self.updated_ts = Utc::now().timestamp_millis();
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
            Self::Sas => write!(f, "sas"),
            Self::Qr => write!(f, "qr"),
            Self::Emoji => write!(f, "emoji"),
        }
    }
}

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
    #[default]
    Pending, // Waiting for verification
    Approved, // Verified successfully
    Rejected, // Verification rejected
    Expired,  // Verification timeout
}

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
    pub id: i64,
    pub user_id: String,
    pub new_device_id: String,
    pub requesting_device_id: Option<String>,
    pub verification_method: VerificationMethod,
    pub status: VerificationRequestStatus,
    pub request_token: String,
    pub commitment: Option<String>,
    pub pubkey: Option<String>,
    pub created_ts: i64,
    pub expires_at: i64,
    pub completed_at: Option<i64>,
}

impl DeviceVerificationRequest {
    pub fn new(
        user_id: &str,
        new_device_id: &str,
        method: VerificationMethod,
        token: &str,
        expires_minutes: i64,
    ) -> Self {
        let now = Utc::now().timestamp_millis();
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
            created_ts: now,
            expires_at: now + expires_minutes * 60 * 1000,
            completed_at: None,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp_millis() > self.expires_at
    }

    pub fn approve(&mut self) {
        self.status = VerificationRequestStatus::Approved;
        self.completed_at = Some(Utc::now().timestamp_millis());
    }

    pub fn reject(&mut self) {
        self.status = VerificationRequestStatus::Rejected;
        self.completed_at = Some(Utc::now().timestamp_millis());
    }

    pub fn expire(&mut self) {
        self.status = VerificationRequestStatus::Expired;
        self.completed_at = Some(Utc::now().timestamp_millis());
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
    pub rotated_at: i64,
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
            rotated_at: Utc::now().timestamp_millis(),
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
    pub created_ts: i64,
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
            created_ts: Utc::now().timestamp_millis(),
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
    pub trusted_at: Option<i64>,
    pub created_ts: i64,
    pub updated_ts: i64,
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
    pub new_device_id: String,
    pub method: String, // "sas", "qr", "emoji"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRequestResponse {
    pub request_token: String,
    pub status: String,
    pub expires_at: i64,
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
    pub verified_at: Option<i64>,
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

    // --- DeviceTrustLevel ---

    #[test]
    fn test_device_trust_level_display() {
        assert_eq!(DeviceTrustLevel::Verified.to_string(), "verified");
        assert_eq!(DeviceTrustLevel::Unverified.to_string(), "unverified");
        assert_eq!(DeviceTrustLevel::Blocked.to_string(), "blocked");
    }

    #[test]
    fn test_device_trust_level_from_str() {
        assert_eq!("verified".parse::<DeviceTrustLevel>().unwrap(), DeviceTrustLevel::Verified);
        assert_eq!("VERIFIED".parse::<DeviceTrustLevel>().unwrap(), DeviceTrustLevel::Verified);
        assert_eq!("unverified".parse::<DeviceTrustLevel>().unwrap(), DeviceTrustLevel::Unverified);
        assert_eq!("blocked".parse::<DeviceTrustLevel>().unwrap(), DeviceTrustLevel::Blocked);
        assert!("unknown".parse::<DeviceTrustLevel>().is_err());
    }

    #[test]
    fn test_device_trust_level_default() {
        assert_eq!(DeviceTrustLevel::default(), DeviceTrustLevel::Unverified);
    }

    #[test]
    fn test_device_trust_level_serialization() {
        assert_eq!(serde_json::to_string(&DeviceTrustLevel::Verified).unwrap(), r#""verified""#);
        assert_eq!(serde_json::to_string(&DeviceTrustLevel::Unverified).unwrap(), r#""unverified""#);
        assert_eq!(serde_json::to_string(&DeviceTrustLevel::Blocked).unwrap(), r#""blocked""#);
    }

    // --- VerificationMethod ---

    #[test]
    fn test_verification_method_from_str() {
        assert_eq!("sas".parse::<VerificationMethod>().unwrap(), VerificationMethod::Sas);
        assert_eq!("qr".parse::<VerificationMethod>().unwrap(), VerificationMethod::Qr);
        assert_eq!("emoji".parse::<VerificationMethod>().unwrap(), VerificationMethod::Emoji);
        assert!("invalid".parse::<VerificationMethod>().is_err());
    }

    #[test]
    fn test_verification_method_serialization() {
        assert_eq!(serde_json::to_string(&VerificationMethod::Sas).unwrap(), r#""sas""#);
        assert_eq!(serde_json::to_string(&VerificationMethod::Qr).unwrap(), r#""qr""#);
        assert_eq!(serde_json::to_string(&VerificationMethod::Emoji).unwrap(), r#""emoji""#);
    }

    // --- VerificationRequestStatus ---

    #[test]
    fn test_verification_request_status_display() {
        assert_eq!(VerificationRequestStatus::Pending.to_string(), "pending");
        assert_eq!(VerificationRequestStatus::Approved.to_string(), "approved");
        assert_eq!(VerificationRequestStatus::Rejected.to_string(), "rejected");
        assert_eq!(VerificationRequestStatus::Expired.to_string(), "expired");
    }

    #[test]
    fn test_verification_request_status_from_str() {
        assert_eq!("pending".parse::<VerificationRequestStatus>().unwrap(), VerificationRequestStatus::Pending);
        assert_eq!("approved".parse::<VerificationRequestStatus>().unwrap(), VerificationRequestStatus::Approved);
        assert_eq!("rejected".parse::<VerificationRequestStatus>().unwrap(), VerificationRequestStatus::Rejected);
        assert_eq!("expired".parse::<VerificationRequestStatus>().unwrap(), VerificationRequestStatus::Expired);
        assert!("unknown".parse::<VerificationRequestStatus>().is_err());
    }

    #[test]
    fn test_verification_request_status_default() {
        assert_eq!(VerificationRequestStatus::default(), VerificationRequestStatus::Pending);
    }

    // --- DeviceVerificationRequest ---

    #[test]
    fn test_device_verification_request_approve() {
        let mut req = DeviceVerificationRequest::new("@user:example.com", "DEVICE123", VerificationMethod::Sas, "token456", 5);
        req.approve();
        assert_eq!(req.status, VerificationRequestStatus::Approved);
        assert!(req.completed_at.is_some());
    }

    #[test]
    fn test_device_verification_request_reject() {
        let mut req = DeviceVerificationRequest::new("@user:example.com", "DEVICE123", VerificationMethod::Qr, "token456", 5);
        req.reject();
        assert_eq!(req.status, VerificationRequestStatus::Rejected);
        assert!(req.completed_at.is_some());
    }

    #[test]
    fn test_device_verification_request_expire() {
        let mut req = DeviceVerificationRequest::new("@user:example.com", "DEVICE123", VerificationMethod::Emoji, "token456", 5);
        req.expire();
        assert_eq!(req.status, VerificationRequestStatus::Expired);
        assert!(req.completed_at.is_some());
    }

    // --- E2eeSecurityEvent ---

    #[test]
    fn test_e2ee_security_event_new() {
        let event = E2eeSecurityEvent::new("@user:example.com", "device.added");
        assert_eq!(event.user_id, "@user:example.com");
        assert_eq!(event.event_type, "device.added");
        assert!(event.device_id.is_none());
        assert!(event.created_ts > 0);
    }

    #[test]
    fn test_e2ee_security_event_builder() {
        let event = E2eeSecurityEvent::new("@user:example.com", "device.verified")
            .with_device("DEVICE123")
            .with_data(serde_json::json!({"key": "value"}))
            .with_ip("192.168.1.1")
            .with_user_agent("MatrixClient/1.0");

        assert_eq!(event.device_id, Some("DEVICE123".to_string()));
        assert!(event.event_data.is_some());
        assert_eq!(event.ip_address, Some("192.168.1.1".to_string()));
        assert_eq!(event.user_agent, Some("MatrixClient/1.0".to_string()));
    }

    // --- CrossSigningTrust ---

    #[test]
    fn test_cross_signing_trust() {
        let trust = CrossSigningTrust {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            target_user_id: "@bob:example.com".to_string(),
            master_key_id: Some("master_key_1".to_string()),
            is_trusted: true,
            trusted_at: Some(1710000000000),
            created_ts: 1710000000000,
            updated_ts: 1710000000000,
        };
        assert!(trust.is_trusted);
        assert_eq!(trust.master_key_id, Some("master_key_1".to_string()));
    }

    // --- Response types ---

    #[test]
    fn test_verification_respond_response() {
        let resp = VerificationRespondResponse {
            success: true,
            trust_level: Some("verified".to_string()),
        };
        assert!(resp.success);
        assert_eq!(resp.trust_level, Some("verified".to_string()));
    }

    #[test]
    fn test_device_trust_list_response() {
        let device = DeviceTrustStatusResponse {
            device_id: "DEVICE123".to_string(),
            trust_level: "verified".to_string(),
            verified_at: Some(1710000000000),
            verified_by: Some("DEVICE_OLD".to_string()),
        };
        let list = DeviceTrustListResponse { devices: vec![device] };
        assert_eq!(list.devices.len(), 1);
        assert_eq!(list.devices[0].device_id, "DEVICE123");
    }

    #[test]
    fn test_security_summary_response() {
        let summary = SecuritySummaryResponse {
            verified_devices: 3,
            unverified_devices: 1,
            blocked_devices: 0,
            has_cross_signing_master: true,
            security_score: 87.5,
            recommendations: vec!["Verify remaining devices".to_string()],
        };
        assert_eq!(summary.verified_devices, 3);
        assert!(summary.security_score > 80.0);
    }

    // --- Existing tests ---

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
}
