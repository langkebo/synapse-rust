// Secure Backup Models
// E2EE Phase 3: Secure key backup with passphrase

use serde::{Deserialize, Serialize};
use synapse_common::current_timestamp_millis;

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

/// Request to restore from secure backup.
///
/// ISSUE-6.3: `passphrase` is no longer used — the server returns ciphertext and
/// the client decrypts locally. Kept as optional (defaulted) for backward compat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreSecureBackupRequest {
    #[serde(default)]
    pub passphrase: Option<String>,
    pub rooms: Option<Vec<String>>,
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

/// Response for restore operation.
///
/// ISSUE-6.3: the server no longer decrypts session keys. `sessions` carries the
/// client-side ciphertext so the client can decrypt locally with its recovery key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResponse {
    pub total_keys: i64,
    pub sessions: Vec<EncryptedSessionKey>,
}

/// A single encrypted session key returned to the client for local decryption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedSessionKey {
    pub room_id: String,
    pub session_id: String,
    pub session_key: String,
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
        let now = current_timestamp_millis();
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

        let backup = SecureBackupInfo::new("@user:example.com", "m.megolm_backup.v1.secure", auth_data);

        assert_eq!(backup.user_id, "@user:example.com");
        assert_eq!(backup.algorithm, "m.megolm_backup.v1.secure");
        assert_eq!(backup.key_count, 0);
    }
}
