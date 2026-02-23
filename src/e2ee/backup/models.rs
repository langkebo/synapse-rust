use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct KeyBackup {
    pub user_id: String,
    pub backup_id: String,
    pub version: i64,
    pub algorithm: String,
    pub auth_key: String,
    pub mgmt_key: String,
    pub backup_data: serde_json::Value,
    pub etag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupVersion {
    pub version: String,
    pub algorithm: String,
    pub auth_data: serde_json::Value,
    pub count: i64,
    pub etag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupUploadRequest {
    pub algorithm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupKeyUploadRequest {
    pub first_message_index: i64,
    pub forwarded_count: i64,
    pub is_verified: bool,
    pub session_data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupUploadResponse {
    pub etag: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BackupKeyInfo {
    pub user_id: String,
    pub backup_id: String,
    pub room_id: String,
    pub session_id: String,
    pub first_message_index: i64,
    pub forwarded_count: i64,
    pub is_verified: bool,
    pub backup_data: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct BackupKeyUpload {
    pub session_id: String,
    pub session_data: String,
    pub first_message_index: i64,
    pub forwarded_count: i64,
    pub is_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryRequest {
    pub version: String,
    pub rooms: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryResponse {
    pub rooms: serde_json::Value,
    pub total_keys: i64,
    pub recovered_keys: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryProgress {
    pub user_id: String,
    pub version: String,
    pub total_keys: i64,
    pub recovered_keys: i64,
    pub status: String,
    pub started_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoverySession {
    pub user_id: String,
    pub version: String,
    pub room_id: String,
    pub session_id: String,
    pub session_data: serde_json::Value,
    pub first_message_index: i64,
    pub forwarded_count: i64,
    pub is_verified: bool,
    pub recovered_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupVerificationRequest {
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupVerificationResponse {
    pub valid: bool,
    pub algorithm: String,
    pub auth_data: serde_json::Value,
    pub key_count: i64,
    pub signatures: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRecoveryRequest {
    pub version: String,
    pub room_ids: Vec<String>,
    pub session_limit: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRecoveryResponse {
    pub rooms: serde_json::Map<String, serde_json::Value>,
    pub total_sessions: i64,
    pub has_more: bool,
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
        let request = BackupUploadRequest {
            algorithm: "m.megolm_backup.v1".to_string(),
        };

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
        let response = BackupUploadResponse {
            etag: "etag123".to_string(),
            count: 50,
        };

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
