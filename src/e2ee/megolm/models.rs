use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MegolmSession {
    pub id: Uuid,
    pub session_id: String,
    pub room_id: String,
    pub sender_key: String,
    pub session_key: String,
    pub algorithm: String,
    pub message_index: i64,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedEvent {
    pub room_id: String,
    pub event_id: String,
    pub sender: String,
    pub content: serde_json::Value,
    pub algorithm: String,
    pub session_id: String,
    pub ciphertext: String,
    pub device_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_megolm_session_creation() {
        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: "session123".to_string(),
            room_id: "!room:example.com".to_string(),
            sender_key: "sender_key123".to_string(),
            session_key: "session_key456".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 0,
            created_at: chrono::Utc::now(),
            last_used_at: chrono::Utc::now(),
            expires_at: None,
        };

        assert_eq!(session.room_id, "!room:example.com");
        assert_eq!(session.algorithm, "m.megolm.v1.aes-sha2");
        assert_eq!(session.message_index, 0);
    }

    #[test]
    fn test_megolm_session_with_expiry() {
        let expires = chrono::Utc::now() + chrono::Duration::hours(24);
        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: "session123".to_string(),
            room_id: "!room:example.com".to_string(),
            sender_key: "sender_key123".to_string(),
            session_key: "session_key456".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 100,
            created_at: chrono::Utc::now(),
            last_used_at: chrono::Utc::now(),
            expires_at: Some(expires),
        };

        assert!(session.expires_at.is_some());
        assert!(session.expires_at.unwrap() > chrono::Utc::now());
    }

    #[test]
    fn test_encrypted_event_creation() {
        let event = EncryptedEvent {
            room_id: "!room:example.com".to_string(),
            event_id: "$event123".to_string(),
            sender: "@test:example.com".to_string(),
            content: serde_json::json!({
                "msgtype": "m.room.encrypted",
                "body": "encrypted_content"
            }),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            session_id: "session123".to_string(),
            ciphertext: "base64_encrypted_data".to_string(),
            device_id: "DEVICE123".to_string(),
        };

        assert_eq!(event.room_id, "!room:example.com");
        assert_eq!(event.algorithm, "m.megolm.v1.aes-sha2");
        assert!(event.ciphertext.starts_with("base64"));
    }

    #[test]
    fn test_megolm_session_id_format() {
        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: "megolm_session_id_123".to_string(),
            room_id: "!room:example.com".to_string(),
            sender_key: "sender_key".to_string(),
            session_key: "session_key".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 0,
            created_at: chrono::Utc::now(),
            last_used_at: chrono::Utc::now(),
            expires_at: None,
        };

        assert!(session.session_id.starts_with("megolm"));
    }

    #[test]
    fn test_encrypted_event_content_format() {
        let content = serde_json::json!({
            "msgtype": "m.room.encrypted",
            "ciphertext": "encrypted_data",
            "device_id": "DEVICE123",
            "sender_key": "sender_key_123"
        });

        let event = EncryptedEvent {
            room_id: "!room:example.com".to_string(),
            event_id: "$event123".to_string(),
            sender: "@test:example.com".to_string(),
            content,
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            session_id: "session123".to_string(),
            ciphertext: "data".to_string(),
            device_id: "DEVICE123".to_string(),
        };

        assert_eq!(event.content["msgtype"], "m.room.encrypted");
        assert_eq!(event.content["device_id"], "DEVICE123");
    }

    #[test]
    fn test_megolm_algorithm_types() {
        let algorithms = vec![
            "m.megolm.v1.aes-sha2",
            "m.olm.v1.curve25519-aes-sha2",
        ];

        for algo in algorithms {
            let session = MegolmSession {
                id: uuid::Uuid::new_v4(),
                session_id: "test".to_string(),
                room_id: "!room:example.com".to_string(),
                sender_key: "key".to_string(),
                session_key: "key".to_string(),
                algorithm: algo.to_string(),
                message_index: 0,
                created_at: chrono::Utc::now(),
                last_used_at: chrono::Utc::now(),
                expires_at: None,
            };

            assert_eq!(session.algorithm, algo);
        }
    }

    #[test]
    fn test_megolm_session_serialization() {
        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: "session123".to_string(),
            room_id: "!room:example.com".to_string(),
            sender_key: "sender_key".to_string(),
            session_key: "session_key".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 50,
            created_at: chrono::Utc::now(),
            last_used_at: chrono::Utc::now(),
            expires_at: None,
        };

        let json = serde_json::to_string(&session).unwrap();
        let deserialized: MegolmSession = serde_json::from_str(&json).unwrap();

        assert_eq!(session.session_id, deserialized.session_id);
        assert_eq!(session.room_id, deserialized.room_id);
        assert_eq!(session.message_index, deserialized.message_index);
    }
}
