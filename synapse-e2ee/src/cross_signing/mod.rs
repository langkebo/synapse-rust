pub mod models;
pub mod service;
pub mod storage;

pub use models::*;
pub use service::*;
pub use storage::*;

#[cfg(test)]
mod tests {
    use super::models::*;

    #[test]
    fn test_device_key_verification_result_creation() {
        let result = DeviceKeyVerificationResult {
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE1".to_string(),
            is_verified: true,
            verified_by_master: false,
            verified_by_self_signing: true,
            verification_method: Some("self_signing_key".to_string()),
            verified_at: Some(chrono::Utc::now()),
        };
        assert!(result.is_verified);
        assert!(result.verified_by_self_signing);
        assert!(!result.verified_by_master);
    }

    #[test]
    fn test_verified_devices_map_creation() {
        let map = VerifiedDevicesMap { user_id: "@alice:example.com".to_string(), verified_devices: vec![] };
        assert!(map.verified_devices.is_empty());
        assert_eq!(map.user_id, "@alice:example.com");
    }
}
