use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSignature {
    pub id: Uuid,
    pub event_id: String,
    pub user_id: String,
    pub device_id: String,
    pub signature: String,
    pub key_id: String,
    pub created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_signature_creation() {
        let sig = EventSignature {
            id: uuid::Uuid::new_v4(),
            event_id: "$event123".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            signature: "base64_signature_data".to_string(),
            key_id: "ed25519:DEVICE123".to_string(),
            created_at: chrono::Utc::now().timestamp(),
        };

        assert_eq!(sig.event_id, "$event123");
        assert_eq!(sig.user_id, "@test:example.com");
        assert_eq!(sig.device_id, "DEVICE123");
    }

    #[test]
    fn test_event_signature_key_id_format() {
        let sig = EventSignature {
            id: uuid::Uuid::new_v4(),
            event_id: "$event123".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            signature: "sig".to_string(),
            key_id: "ed25519:DEVICE123".to_string(),
            created_at: chrono::Utc::now().timestamp(),
        };

        assert!(sig.key_id.starts_with("ed25519:"));
    }

    #[test]
    fn test_signature_serialization() {
        let sig = EventSignature {
            id: uuid::Uuid::new_v4(),
            event_id: "$event123".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            signature: "signature_data".to_string(),
            key_id: "ed25519:KEY1".to_string(),
            created_at: chrono::Utc::now().timestamp(),
        };

        let json = serde_json::to_string(&sig).unwrap();
        let deserialized: EventSignature = serde_json::from_str(&json).unwrap();

        assert_eq!(sig.event_id, deserialized.event_id);
        assert_eq!(sig.signature, deserialized.signature);
        assert_eq!(sig.key_id, deserialized.key_id);
    }

    #[test]
    fn test_signature_created_at() {
        let now = chrono::Utc::now().timestamp();
        let sig = EventSignature {
            id: uuid::Uuid::new_v4(),
            event_id: "$event123".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            signature: "sig".to_string(),
            key_id: "ed25519:DEVICE123".to_string(),
            created_at: now,
        };

        assert_eq!(sig.created_at, now);
        assert!(sig.created_at > 0);
    }

    #[test]
    fn test_signature_with_different_key_types() {
        let key_types = vec!["ed25519:KEY1", "curve25519:KEY1"];

        for key_id in key_types {
            let sig = EventSignature {
                id: uuid::Uuid::new_v4(),
                event_id: "$event123".to_string(),
                user_id: "@test:example.com".to_string(),
                device_id: "DEVICE123".to_string(),
                signature: "sig".to_string(),
                key_id: key_id.to_string(),
                created_at: chrono::Utc::now().timestamp(),
            };

            assert_eq!(sig.key_id, key_id);
        }
    }
}
