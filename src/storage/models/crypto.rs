use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct DeviceKey {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub algorithm: String,
    pub key_id: String,
    pub public_key: String,
    pub key_data: Option<String>,
    pub signatures: Option<serde_json::Value>,
    pub added_ts: i64,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub ts_updated_ms: Option<i64>,
    pub is_verified: bool,
    pub is_blocked: bool,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct CrossSigningKey {
    pub id: i64,
    pub user_id: String,
    pub key_type: String,
    pub key_data: String,
    pub signatures: Option<serde_json::Value>,
    pub added_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct MegolmSession {
    pub id: uuid::Uuid,
    pub session_id: String,
    pub room_id: String,
    pub sender_key: String,
    pub session_key: String,
    pub algorithm: String,
    pub message_index: i64,
    pub created_ts: i64,
    pub last_used_ts: Option<i64>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct EventSignature {
    pub id: uuid::Uuid,
    pub event_id: String,
    pub user_id: String,
    pub device_id: String,
    pub signature: String,
    pub key_id: String,
    pub algorithm: String,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct DeviceSignature {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub target_user_id: String,
    pub target_device_id: String,
    pub algorithm: String,
    pub signature: String,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct KeyBackup {
    pub id: i64,
    pub user_id: String,
    pub algorithm: String,
    pub auth_data: String,
    pub key_count: i64,
    pub auth_key: Option<String>,
    pub mgmt_key: Option<String>,
    pub version: i64,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct BackupKey {
    pub id: i64,
    pub backup_id: i64,
    pub room_id: String,
    pub session_id: String,
    pub session_data: serde_json::Value,
    pub created_ts: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_key() {
        let device_key = DeviceKey {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            algorithm: "ed25519".to_string(),
            key_id: "ed25519:DEVICE123".to_string(),
            public_key: "public_key_data".to_string(),
            key_data: None,
            signatures: Some(serde_json::json!({})),
            added_ts: 1234567890000,
            created_ts: 1234567890000,
            updated_ts: None,
            ts_updated_ms: None,
            is_verified: true,
            is_blocked: false,
            display_name: Some("iPhone 15".to_string()),
        };

        assert_eq!(device_key.algorithm, "ed25519");
        assert!(device_key.is_verified);
        assert!(!device_key.is_blocked);
    }

    #[test]
    fn test_cross_signing_key() {
        let cross_key = CrossSigningKey {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            key_type: "master".to_string(),
            key_data: "master_key_data".to_string(),
            signatures: Some(serde_json::json!({})),
            added_ts: 1234567890000,
        };

        assert_eq!(cross_key.key_type, "master");
    }

    #[test]
    fn test_megolm_session() {
        let session = MegolmSession {
            id: uuid::Uuid::nil(),
            session_id: "session_abc123".to_string(),
            room_id: "!room:example.com".to_string(),
            sender_key: "sender_key_data".to_string(),
            session_key: "session_key_data".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 0,
            created_ts: 1234567890000,
            last_used_ts: Some(1234567900000),
            expires_at: Some(1234653490000),
        };

        assert_eq!(session.algorithm, "m.megolm.v1.aes-sha2");
        assert_eq!(session.message_index, 0);
    }

    #[test]
    fn test_key_backup() {
        let backup = KeyBackup {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            algorithm: "m.megolm.backup.v1".to_string(),
            auth_data: r#"{"public_key": "backup_key"}"#.to_string(),
            key_count: 0,
            auth_key: Some("auth_key_data".to_string()),
            mgmt_key: None,
            version: 1,
            created_ts: 1234567890000,
            updated_ts: None,
        };

        assert_eq!(backup.algorithm, "m.megolm.backup.v1");
        assert_eq!(backup.version, 1);
    }
}
