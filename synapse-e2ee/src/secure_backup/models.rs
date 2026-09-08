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

    #[test]
    fn as_str_all_variants() {
        // Auth data serialization test
        let auth = SecureBackupAuthData {
            salt: "test_salt".to_string(),
            iterations: 100000,
            backup_id: "backup123".to_string(),
            public_key: Some("test_key".to_string()),
        };

        let json = serde_json::to_string(&auth).unwrap();
        let rt: SecureBackupAuthData = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.salt, "test_salt");
        assert_eq!(rt.iterations, 100000);
        assert_eq!(rt.backup_id, "backup123");
        assert_eq!(rt.public_key, Some("test_key".to_string()));
    }

    #[test]
    fn session_key_data_serialization() {
        let data = SessionKeyData {
            room_id: "!room:test.org".to_string(),
            session_id: "session1".to_string(),
            first_message_index: 1,
            forwarded_count: 0,
            is_verified: true,
            session_key: "encrypted_session_key_data".to_string(),
        };

        let json = serde_json::to_string(&data).unwrap();
        let rt: SessionKeyData = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.room_id, "!room:test.org");
        assert_eq!(rt.session_id, "session1");
        assert_eq!(rt.first_message_index, 1);
        assert_eq!(rt.forwarded_count, 0);
        assert_eq!(rt.is_verified, true);
        assert_eq!(rt.session_key, "encrypted_session_key_data");
    }

    #[test]
    fn restore_request_serialization() {
        let req = RestoreSecureBackupRequest {
            passphrase: Some("my_passphrase".to_string()),
            rooms: Some(vec!["!room:test.org".to_string()]),
        };

        let json = serde_json::to_string(&req).unwrap();
        let rt: RestoreSecureBackupRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.passphrase, Some("my_passphrase".to_string()));
        assert_eq!(rt.rooms, Some(vec!["!room:test.org".to_string()]));

        // Test default (no passphrase, no rooms)
        let req_no_passphrase = RestoreSecureBackupRequest { passphrase: None, rooms: None };
        let json_no_passphrase = serde_json::to_string(&req_no_passphrase).unwrap();
        let rt_no_passphrase: RestoreSecureBackupRequest = serde_json::from_str(&json_no_passphrase).unwrap();
        assert_eq!(rt_no_passphrase.passphrase, None);
        assert_eq!(rt_no_passphrase.rooms, None);
    }

    // ── SecureBackupAuthData edge cases ─────────────────────────

    #[test]
    fn auth_data_empty_optional_fields() {
        let auth = SecureBackupAuthData {
            salt: String::new(),
            iterations: 0,
            backup_id: String::new(),
            public_key: None,
        };

        // Should serialize/deserialize without panic
        let json = serde_json::to_string(&auth).unwrap();
        let rt: SecureBackupAuthData = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.salt, "");
        assert_eq!(rt.iterations, 0);
        assert_eq!(rt.public_key, None);
    }

    #[test]
    fn auth_data_with_public_key() {
        let auth = SecureBackupAuthData {
            salt: "realsalt".to_string(),
            iterations: 600000,
            backup_id: "b-uuid".to_string(),
            public_key: Some("curve25519:pubkey123".to_string()),
        };

        let json = serde_json::to_string(&auth).unwrap();
        let rt: SecureBackupAuthData = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.public_key, Some("curve25519:pubkey123".to_string()));
    }

    // ── SessionKeyData edge cases ───────────────────────────────

    #[test]
    fn session_key_empty_keys_and_rooms() {
        let data = SessionKeyData {
            room_id: String::new(),
            session_id: String::new(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: false,
            session_key: String::new(),
        };
        let json = serde_json::to_string(&data).unwrap();
        let rt: SessionKeyData = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.is_verified, false);
    }

    #[test]
    fn session_key_data_verified_false() {
        let data = SessionKeyData {
            room_id: "!a:test.org".to_string(),
            session_id: "sid-1".to_string(),
            first_message_index: 5,
            forwarded_count: 3,
            is_verified: false,
            session_key: "encrypted_key_data".to_string(),
        };
        assert!(!data.is_verified);
        assert_eq!(data.forwarded_count, 3);
        assert_eq!(data.first_message_index, 5);
    }

    // ── RestoreResponse ─────────────────────────────────────────

    #[test]
    fn restore_response_empty_sessions() {
        let resp = RestoreResponse { total_keys: 0, sessions: vec![] };
        assert_eq!(resp.total_keys, 0);
        assert!(resp.sessions.is_empty());
    }

    #[test]
    fn restore_response_with_sessions() {
        let resp = RestoreResponse {
            total_keys: 2,
            sessions: vec![
                EncryptedSessionKey {
                    room_id: "!a:test.org".to_string(),
                    session_id: "s1".to_string(),
                    session_key: "enc1".to_string(),
                },
                EncryptedSessionKey {
                    room_id: "!b:test.org".to_string(),
                    session_id: "s2".to_string(),
                    session_key: "enc2".to_string(),
                },
            ],
        };
        assert_eq!(resp.sessions.len(), 2);
        assert_eq!(resp.sessions[0].room_id, "!a:test.org");
        assert_eq!(resp.sessions[1].session_id, "s2");
    }

    // ── SecureBackupResponse ────────────────────────────────────

    #[test]
    fn secure_backup_response_roundtrip() {
        let auth_data = SecureBackupAuthData {
            salt: "salt".to_string(),
            iterations: 100000,
            backup_id: "b-uuid".to_string(),
            public_key: Some("key".to_string()),
        };
        let resp = SecureBackupResponse {
            backup_id: "b-uuid".to_string(),
            version: "v1".to_string(),
            algorithm: "m.megolm_backup.v1.curve25519-aes-sha2".to_string(),
            auth_data,
            key_count: 5,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let rt: SecureBackupResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.backup_id, "b-uuid");
        assert_eq!(rt.version, "v1");
        assert_eq!(rt.key_count, 5);
    }

    // ── BackupVersion ───────────────────────────────────────────

    #[test]
    fn backup_version_etag_none() {
        let version = BackupVersion {
            backup_id: "b1".to_string(),
            version: "v1".to_string(),
            algorithm: "algo".to_string(),
            auth_data: serde_json::json!({}),
            etag: None,
            key_count: 10,
            created_ts: 1_000_000,
        };
        assert!(version.etag.is_none());
    }

    #[test]
    fn backup_version_etag_some() {
        let version = BackupVersion {
            backup_id: "b1".to_string(),
            version: "v1".to_string(),
            algorithm: "algo".to_string(),
            auth_data: serde_json::json!({}),
            etag: Some("abc123".to_string()),
            key_count: 10,
            created_ts: 1_000_000,
        };
        assert_eq!(version.etag, Some("abc123".to_string()));
    }
}
