// Secure Backup Models
// E2EE Phase 3: Secure key backup with passphrase

use serde::{Deserialize, Serialize};
use synapse_common::current_timestamp_millis;

/// Secure backup info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureBackupInfo {
    /// The `backup_id` field.
    /// The `user_id` field.
    /// The `algorithm` field.
    /// The `auth_data` field.
    /// The `key_count` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub backup_id: String,
    /// The `user_id` field.
    /// The `algorithm` field.
    /// The `auth_data` field.
    /// The `key_count` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub user_id: String,
    /// The `algorithm` field.
    /// The `auth_data` field.
    /// The `key_count` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub algorithm: String,
    /// The `auth_data` field.
    /// The `key_count` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub auth_data: SecureBackupAuthData,
    /// The `key_count` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub key_count: i64,
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// Auth data for secure backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureBackupAuthData {
    /// The `salt` field.
    /// The `iterations` field.
    /// The `backup_id` field.
    /// The `public_key` field.
    pub salt: String,
    /// The `iterations` field.
    /// The `backup_id` field.
    /// The `public_key` field.
    pub iterations: i64,
    /// The `backup_id` field.
    /// The `public_key` field.
    pub backup_id: String,
    /// The `public_key` field.
    pub public_key: Option<String>,
}

/// Session key data for backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionKeyData {
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `session_key` field.
    pub room_id: String,
    /// The `session_id` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `session_key` field.
    pub session_id: String,
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `session_key` field.
    pub first_message_index: i64,
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `session_key` field.
    pub forwarded_count: i64,
    /// The `is_verified` field.
    /// The `session_key` field.
    pub is_verified: bool,
    /// The `session_key` field.
    pub session_key: String, // Encrypted session key
}

/// Request to restore from secure backup.
///
/// ISSUE-6.3: `passphrase` is no longer used — the server returns ciphertext and
/// the client decrypts locally. Kept as optional (defaulted) for backward compat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreSecureBackupRequest {
    /// The `passphrase` field.
    #[serde(default)]
    pub passphrase: Option<String>,
    /// The `rooms` field.
    pub rooms: Option<Vec<String>>,
}

/// Response for secure backup operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureBackupResponse {
    /// The `backup_id` field.
    /// The `version` field.
    /// The `algorithm` field.
    /// The `auth_data` field.
    /// The `key_count` field.
    pub backup_id: String,
    /// The `version` field.
    /// The `algorithm` field.
    /// The `auth_data` field.
    /// The `key_count` field.
    pub version: String,
    /// The `algorithm` field.
    /// The `auth_data` field.
    /// The `key_count` field.
    pub algorithm: String,
    /// The `auth_data` field.
    /// The `key_count` field.
    pub auth_data: SecureBackupAuthData,
    /// The `key_count` field.
    pub key_count: i64,
}

/// Response for restore operation.
///
/// ISSUE-6.3: the server no longer decrypts session keys. `sessions` carries the
/// client-side ciphertext so the client can decrypt locally with its recovery key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResponse {
    /// The `total_keys` field.
    /// The `sessions` field.
    pub total_keys: i64,
    /// The `sessions` field.
    pub sessions: Vec<EncryptedSessionKey>,
}

/// A single encrypted session key returned to the client for local decryption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedSessionKey {
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `session_key` field.
    pub room_id: String,
    /// The `session_id` field.
    /// The `session_key` field.
    pub session_id: String,
    /// The `session_key` field.
    pub session_key: String,
}

/// Backup version info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupVersion {
    /// The `backup_id` field.
    /// The `version` field.
    /// The `algorithm` field.
    /// The `auth_data` field.
    /// The `etag` field.
    /// The `key_count` field.
    /// The `created_ts` field.
    pub backup_id: String,
    /// The `version` field.
    /// The `algorithm` field.
    /// The `auth_data` field.
    /// The `etag` field.
    /// The `key_count` field.
    /// The `created_ts` field.
    pub version: String,
    /// The `algorithm` field.
    /// The `auth_data` field.
    /// The `etag` field.
    /// The `key_count` field.
    /// The `created_ts` field.
    pub algorithm: String,
    /// The `auth_data` field.
    /// The `etag` field.
    /// The `key_count` field.
    /// The `created_ts` field.
    pub auth_data: serde_json::Value,
    /// The `etag` field.
    /// The `key_count` field.
    /// The `created_ts` field.
    pub etag: Option<String>,
    /// The `key_count` field.
    /// The `created_ts` field.
    pub key_count: i64,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// (see code)
impl SecureBackupInfo {
    /// See [`new`].
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
