use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
/// The `KeyBackup` type.
pub struct KeyBackup {
    /// The `user_id` field.
    /// The `backup_id` field.
    /// The `version` field.
    /// The `algorithm` field.
    /// The `auth_key` field.
    /// The `mgmt_key` field.
    /// The `backup_data` field.
    /// The `etag` field.
    pub user_id: String,
    /// The `backup_id` field.
    /// The `version` field.
    /// The `algorithm` field.
    /// The `auth_key` field.
    /// The `mgmt_key` field.
    /// The `backup_data` field.
    /// The `etag` field.
    pub backup_id: String,
    /// The `version` field.
    /// The `algorithm` field.
    /// The `auth_key` field.
    /// The `mgmt_key` field.
    /// The `backup_data` field.
    /// The `etag` field.
    pub version: i64,
    /// The `algorithm` field.
    /// The `auth_key` field.
    /// The `mgmt_key` field.
    /// The `backup_data` field.
    /// The `etag` field.
    pub algorithm: String,
    /// The `auth_key` field.
    /// The `mgmt_key` field.
    /// The `backup_data` field.
    /// The `etag` field.
    pub auth_key: String,
    /// The `mgmt_key` field.
    /// The `backup_data` field.
    /// The `etag` field.
    pub mgmt_key: String,
    /// The `backup_data` field.
    /// The `etag` field.
    pub backup_data: serde_json::Value,
    /// The `etag` field.
    pub etag: Option<String>,
}

/// SQLx row type for KeyBackup — absorbs nullable columns from the DB schema.
#[derive(sqlx::FromRow)]
pub struct KeyBackupRow {
    /// The `user_id` field.
    /// The `backup_id` field.
    /// The `version` field.
    /// The `algorithm` field.
    /// The `auth_key` field.
    /// The `mgmt_key` field.
    /// The `backup_data` field.
    /// The `etag` field.
    pub user_id: String,
    /// The `backup_id` field.
    /// The `version` field.
    /// The `algorithm` field.
    /// The `auth_key` field.
    /// The `mgmt_key` field.
    /// The `backup_data` field.
    /// The `etag` field.
    pub backup_id: String,
    /// The `version` field.
    /// The `algorithm` field.
    /// The `auth_key` field.
    /// The `mgmt_key` field.
    /// The `backup_data` field.
    /// The `etag` field.
    pub version: i64,
    /// The `algorithm` field.
    /// The `auth_key` field.
    /// The `mgmt_key` field.
    /// The `backup_data` field.
    /// The `etag` field.
    pub algorithm: String,
    /// The `auth_key` field.
    /// The `mgmt_key` field.
    /// The `backup_data` field.
    /// The `etag` field.
    pub auth_key: Option<String>,
    /// The `mgmt_key` field.
    /// The `backup_data` field.
    /// The `etag` field.
    pub mgmt_key: Option<String>,
    /// The `backup_data` field.
    /// The `etag` field.
    pub backup_data: Option<serde_json::Value>,
    /// The `etag` field.
    pub etag: Option<String>,
}

/// (see code)
impl From<KeyBackupRow> for KeyBackup {
    fn from(row: KeyBackupRow) -> Self {
        Self {
            user_id: row.user_id,
            backup_id: row.backup_id,
            version: row.version,
            algorithm: row.algorithm,
            auth_key: row.auth_key.unwrap_or_default(),
            mgmt_key: row.mgmt_key.unwrap_or_default(),
            backup_data: row.backup_data.unwrap_or(serde_json::json!({})),
            etag: row.etag,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `BackupVersion` type.
pub struct BackupVersion {
    /// The `version` field.
    /// The `algorithm` field.
    /// The `auth_data` field.
    /// The `count` field.
    /// The `etag` field.
    pub version: String,
    /// The `algorithm` field.
    /// The `auth_data` field.
    /// The `count` field.
    /// The `etag` field.
    pub algorithm: String,
    /// The `auth_data` field.
    /// The `count` field.
    /// The `etag` field.
    pub auth_data: serde_json::Value,
    /// The `count` field.
    /// The `etag` field.
    pub count: i64,
    /// The `etag` field.
    pub etag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `BackupUploadRequest` type.
pub struct BackupUploadRequest {
    /// The `algorithm` field.
    pub algorithm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `BackupKeyUploadRequest` type.
pub struct BackupKeyUploadRequest {
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `session_data` field.
    pub first_message_index: i64,
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `session_data` field.
    pub forwarded_count: i64,
    /// The `is_verified` field.
    /// The `session_data` field.
    pub is_verified: bool,
    /// The `session_data` field.
    pub session_data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `BackupUploadResponse` type.
pub struct BackupUploadResponse {
    /// The `etag` field.
    /// The `count` field.
    pub etag: String,
    /// The `count` field.
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
/// The `BackupKeyInfo` type.
pub struct BackupKeyInfo {
    /// The `user_id` field.
    /// The `backup_id` field.
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `session_data` field.
    pub user_id: String,
    /// The `backup_id` field.
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `session_data` field.
    pub backup_id: String,
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `session_data` field.
    pub room_id: String,
    /// The `session_id` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `session_data` field.
    pub session_id: String,
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `session_data` field.
    pub first_message_index: i64,
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `session_data` field.
    pub forwarded_count: i64,
    /// The `is_verified` field.
    /// The `session_data` field.
    pub is_verified: bool,
    /// The `session_data` field.
    pub session_data: serde_json::Value,
}

#[derive(Debug, Clone)]
/// The `BackupKeyUpload` type.
pub struct BackupKeyUpload {
    /// The `session_id` field.
    /// The `session_data` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    pub session_id: String,
    /// The `session_data` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    pub session_data: String,
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    pub first_message_index: i64,
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    pub forwarded_count: i64,
    /// The `is_verified` field.
    pub is_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `RecoveryRequest` type.
pub struct RecoveryRequest {
    /// The `version` field.
    /// The `rooms` field.
    pub version: String,
    /// The `rooms` field.
    pub rooms: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `RecoveryResponse` type.
pub struct RecoveryResponse {
    /// The `rooms` field.
    /// The `total_keys` field.
    /// The `recovered_keys` field.
    pub rooms: serde_json::Value,
    /// The `total_keys` field.
    /// The `recovered_keys` field.
    pub total_keys: i64,
    /// The `recovered_keys` field.
    pub recovered_keys: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `RecoveryProgress` type.
pub struct RecoveryProgress {
    /// The `user_id` field.
    /// The `version` field.
    /// The `total_keys` field.
    /// The `recovered_keys` field.
    /// The `status` field.
    /// The `started_ts` field.
    /// The `updated_ts` field.
    pub user_id: String,
    /// The `version` field.
    /// The `total_keys` field.
    /// The `recovered_keys` field.
    /// The `status` field.
    /// The `started_ts` field.
    /// The `updated_ts` field.
    pub version: String,
    /// The `total_keys` field.
    /// The `recovered_keys` field.
    /// The `status` field.
    /// The `started_ts` field.
    /// The `updated_ts` field.
    pub total_keys: i64,
    /// The `recovered_keys` field.
    /// The `status` field.
    /// The `started_ts` field.
    /// The `updated_ts` field.
    pub recovered_keys: i64,
    /// The `status` field.
    /// The `started_ts` field.
    /// The `updated_ts` field.
    pub status: String,
    /// The `started_ts` field.
    /// The `updated_ts` field.
    pub started_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `RecoverySession` type.
pub struct RecoverySession {
    /// The `user_id` field.
    /// The `version` field.
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `session_data` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `recovered_ts` field.
    pub user_id: String,
    /// The `version` field.
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `session_data` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `recovered_ts` field.
    pub version: String,
    /// The `room_id` field.
    /// The `session_id` field.
    /// The `session_data` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `recovered_ts` field.
    pub room_id: String,
    /// The `session_id` field.
    /// The `session_data` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `recovered_ts` field.
    pub session_id: String,
    /// The `session_data` field.
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `recovered_ts` field.
    pub session_data: serde_json::Value,
    /// The `first_message_index` field.
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `recovered_ts` field.
    pub first_message_index: i64,
    /// The `forwarded_count` field.
    /// The `is_verified` field.
    /// The `recovered_ts` field.
    pub forwarded_count: i64,
    /// The `is_verified` field.
    /// The `recovered_ts` field.
    pub is_verified: bool,
    /// The `recovered_ts` field.
    pub recovered_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `BackupVerificationRequest` type.
pub struct BackupVerificationRequest {
    /// The `version` field.
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `BackupVerificationResponse` type.
pub struct BackupVerificationResponse {
    /// The `valid` field.
    /// The `algorithm` field.
    /// The `auth_data` field.
    /// The `key_count` field.
    /// The `signatures` field.
    pub valid: bool,
    /// The `algorithm` field.
    /// The `auth_data` field.
    /// The `key_count` field.
    /// The `signatures` field.
    pub algorithm: String,
    /// The `auth_data` field.
    /// The `key_count` field.
    /// The `signatures` field.
    pub auth_data: serde_json::Value,
    /// The `key_count` field.
    /// The `signatures` field.
    pub key_count: i64,
    /// The `signatures` field.
    pub signatures: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `BatchRecoveryRequest` type.
pub struct BatchRecoveryRequest {
    /// The `version` field.
    /// The `room_ids` field.
    /// The `session_limit` field.
    pub version: String,
    /// The `room_ids` field.
    /// The `session_limit` field.
    pub room_ids: Vec<String>,
    /// The `session_limit` field.
    pub session_limit: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `BatchRecoveryResponse` type.
pub struct BatchRecoveryResponse {
    /// The `rooms` field.
    /// The `total_sessions` field.
    /// The `has_more` field.
    /// The `next_batch` field.
    pub rooms: serde_json::Map<String, serde_json::Value>,
    /// The `total_sessions` field.
    /// The `has_more` field.
    /// The `next_batch` field.
    pub total_sessions: i64,
    /// The `has_more` field.
    /// The `next_batch` field.
    pub has_more: bool,
    /// The `next_batch` field.
    pub next_batch: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_backup_creation() {
        let backup = KeyBackup {
            user_id: "@test:example.com".to_string(),
            backup_id: "1".to_string(),
            version: 1,
            algorithm: "m.megolm_backup.v1".to_string(),
            auth_key: "".to_string(),
            mgmt_key: "".to_string(),
            backup_data: serde_json::json!({
                "signatures": {}
            }),
            etag: None,
        };

        assert_eq!(backup.user_id, "@test:example.com");
        assert_eq!(backup.version, 1);
        assert_eq!(backup.algorithm, "m.megolm_backup.v1");
    }

    #[test]
    fn test_backup_version_creation() {
        let version = BackupVersion {
            version: "2".to_string(),
            algorithm: "m.megolm_backup.v1".to_string(),
            auth_data: serde_json::json!({
                "signatures": {"@test:example.com": {}}
            }),
            count: 100,
            etag: "abc123".to_string(),
        };

        assert_eq!(version.version, "2");
        assert_eq!(version.count, 100);
        assert_eq!(version.etag, "abc123");
    }

    #[test]
    fn test_backup_upload_request() {
        let request = BackupUploadRequest { algorithm: "m.megolm_backup.v1".to_string() };

        assert_eq!(request.algorithm, "m.megolm_backup.v1");
    }

    #[test]
    fn test_backup_key_upload_request() {
        let request = BackupKeyUploadRequest {
            first_message_index: 0,
            forwarded_count: 1,
            is_verified: true,
            session_data: "encrypted_session_data".to_string(),
        };

        assert_eq!(request.first_message_index, 0);
        assert_eq!(request.forwarded_count, 1);
        assert!(request.is_verified);
    }

    #[test]
    fn test_backup_upload_response() {
        let response = BackupUploadResponse { etag: "etag123".to_string(), count: 50 };

        assert_eq!(response.etag, "etag123");
        assert_eq!(response.count, 50);
    }

    #[test]
    fn test_key_backup_with_rooms() {
        let backup = KeyBackup {
            user_id: "@test:example.com".to_string(),
            backup_id: "1".to_string(),
            version: 1,
            algorithm: "m.megolm_backup.v1".to_string(),
            auth_key: "".to_string(),
            mgmt_key: "".to_string(),
            backup_data: serde_json::json!({
                "rooms": {
                    "!room:example.com": {
                        "sessions": {}
                    }
                }
            }),
            etag: None,
        };

        assert!(backup.backup_data.is_object());
        assert!(backup.backup_data["rooms"].is_object());
    }

    #[test]
    fn test_backup_version_etag_format() {
        let version = BackupVersion {
            version: "1".to_string(),
            algorithm: "m.megolm_backup.v1".to_string(),
            auth_data: serde_json::json!({}),
            count: 0,
            etag: format!("{:x}", chrono::Utc::now().timestamp()),
        };

        assert!(!version.etag.is_empty());
    }

    #[test]
    fn test_backup_key_serialization() {
        let key = BackupKeyUploadRequest {
            first_message_index: 10,
            forwarded_count: 2,
            is_verified: false,
            session_data: "session_data_encrypted".to_string(),
        };

        let json = serde_json::to_string(&key).unwrap();
        let deserialized: BackupKeyUploadRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(key.first_message_index, deserialized.first_message_index);
        assert_eq!(key.forwarded_count, deserialized.forwarded_count);
        assert_eq!(key.is_verified, deserialized.is_verified);
        assert_eq!(key.session_data, deserialized.session_data);
    }

    #[test]
    fn test_backup_algorithm_types() {
        let algorithms = vec!["m.megolm_backup.v1", "m.megolm_backup.v2"];

        for algo in algorithms {
            let backup = KeyBackup {
                user_id: "@test:example.com".to_string(),
                backup_id: "1".to_string(),
                version: 1,
                algorithm: algo.to_string(),
                auth_key: "".to_string(),
                mgmt_key: "".to_string(),
                backup_data: serde_json::json!({}),
                etag: None,
            };

            assert_eq!(backup.algorithm, algo);
        }
    }
}
