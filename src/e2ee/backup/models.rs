use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct KeyBackup {
    pub id: Uuid,
    pub user_id: String,
    pub version: String,
    pub algorithm: String,
    pub auth_data: serde_json::Value,
    pub encrypted_data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupKeyInfo {
    pub id: i64,
    pub backup_id: i64,
    pub room_id: String,
    pub session_id: String,
    pub first_message_index: i64,
    pub forwarded_count: i64,
    pub is_verified: bool,
    pub session_data: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct BackupKeyUpload {
    pub session_id: String,
    pub session_data: String,
    pub first_message_index: i64,
    pub forwarded_count: i64,
    pub is_verified: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_backup_creation() {
        let backup = KeyBackup {
            id: uuid::Uuid::new_v4(),
            user_id: "@test:example.com".to_string(),
            version: "1".to_string(),
            algorithm: "m.megolm_backup.v1".to_string(),
            auth_data: serde_json::json!({
                "signatures": {}
            }),
            encrypted_data: serde_json::json!({
                "rooms": {}
            }),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        assert_eq!(backup.user_id, "@test:example.com");
        assert_eq!(backup.version, "1");
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
            id: uuid::Uuid::new_v4(),
            user_id: "@test:example.com".to_string(),
            version: "1".to_string(),
            algorithm: "m.megolm_backup.v1".to_string(),
            auth_data: serde_json::json!({
                "signatures": {},
                "rotation_distance_ms": 1000
            }),
            encrypted_data: serde_json::json!({
                "rooms": {
                    "!room:example.com": {
                        "sessions": {}
                    }
                }
            }),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        assert!(backup.encrypted_data.is_object());
        assert!(backup.encrypted_data["rooms"].is_object());
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
                id: uuid::Uuid::new_v4(),
                user_id: "@test:example.com".to_string(),
                version: "1".to_string(),
                algorithm: algo.to_string(),
                auth_data: serde_json::json!({}),
                encrypted_data: serde_json::json!({}),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };

            assert_eq!(backup.algorithm, algo);
        }
    }
}
