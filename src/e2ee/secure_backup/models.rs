// Secure Backup Models
// E2EE Phase 3: Secure key backup with passphrase

use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Secure backup info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureBackupInfo {
    pub backup_id: String,
    pub user_id: String,
    pub algorithm: String,
    pub auth_data: SecureBackupAuthData,
    pub key_count: i64,
    pub created_ts: i64,
    pub updated_ts: i64,
}

/// Auth data for secure backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureBackupAuthData {
    pub salt: String,
    pub iterations: i64,
    pub backup_id: String,
    pub public_key: Option<String>,
}

/// Session key data for backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionKeyData {
    pub room_id: String,
    pub session_id: String,
    pub first_message_index: i64,
    pub forwarded_count: i64,
    pub is_verified: bool,
    pub session_key: String, // Encrypted session key
}

/// Request to create a secure backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSecureBackupRequest {
    pub passphrase: String,
    pub key_count: Option<i64>,
}

/// Request to restore from secure backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreSecureBackupRequest {
    pub backup_id: String,
    pub passphrase: String,
}

/// Request to verify passphrase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyPassphraseRequest {
    pub backup_id: String,
    pub passphrase: String,
}

/// Response for secure backup operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureBackupResponse {
    pub backup_id: String,
    pub version: String,
    pub algorithm: String,
    pub auth_data: SecureBackupAuthData,
    pub key_count: i64,
}

/// Response for restore operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResponse {
    pub success: bool,
    pub key_count: i64,
    pub message: String,
}

/// Response for passphrase verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyPassphraseResponse {
    pub valid: bool,
}

/// Key derivation params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyDerivationParams {
    pub salt: Vec<u8>,
    pub iterations: i64,
    pub memory_kb: i64,
    pub parallelism: i64,
}

impl Default for KeyDerivationParams {
    fn default() -> Self {
        Self {
            salt: vec![0u8; 16],
            iterations: 500000,
            memory_kb: 65536,
            parallelism: 4,
        }
    }
}

/// Backup version info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupVersion {
    pub backup_id: String,
    pub version: String,
    pub algorithm: String,
    pub auth_data: serde_json::Value,
    pub etag: Option<String>,
    pub key_count: i64,
    pub created_ts: i64,
}

impl SecureBackupInfo {
    pub fn new(user_id: &str, algorithm: &str, auth_data: SecureBackupAuthData) -> Self {
        let now = Utc::now().timestamp_millis();
        Self {
            backup_id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            algorithm: algorithm.to_string(),
            auth_data,
            key_count: 0,
            created_ts: now,
            updated_ts: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_backup_info_new() {
        let auth_data = SecureBackupAuthData {
            salt: "testsalt".to_string(),
            iterations: 500000,
            backup_id: "backup123".to_string(),
            public_key: None,
        };

        let backup =
            SecureBackupInfo::new("@user:example.com", "m.megolm_backup.v1.secure", auth_data);

        assert_eq!(backup.user_id, "@user:example.com");
        assert_eq!(backup.algorithm, "m.megolm_backup.v1.secure");
        assert_eq!(backup.key_count, 0);
    }

    #[test]
    fn test_key_derivation_params_default() {
        let params = KeyDerivationParams::default();

        assert_eq!(params.iterations, 500000);
        assert_eq!(params.memory_kb, 65536);
        assert_eq!(params.parallelism, 4);
    }
}
