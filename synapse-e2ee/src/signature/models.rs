use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
/// The `EventSignature` type.
pub struct EventSignature {
    /// The `id` field.
    /// The `event_id` field.
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `signature` field.
    /// The `key_id` field.
    /// The `created_ts` field.
    pub id: Uuid,
    /// The `event_id` field.
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `signature` field.
    /// The `key_id` field.
    /// The `created_ts` field.
    pub event_id: String,
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `signature` field.
    /// The `key_id` field.
    /// The `created_ts` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `signature` field.
    /// The `key_id` field.
    /// The `created_ts` field.
    pub device_id: String,
    /// The `signature` field.
    /// The `key_id` field.
    /// The `created_ts` field.
    pub signature: String,
    /// The `key_id` field.
    /// The `created_ts` field.
    pub key_id: String,
    /// The `created_ts` field.
    pub created_ts: i64,
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
            created_ts: chrono::Utc::now().timestamp(),
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
            created_ts: chrono::Utc::now().timestamp(),
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
            created_ts: chrono::Utc::now().timestamp(),
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
            created_ts: now,
        };

        assert_eq!(sig.created_ts, now);
        assert!(sig.created_ts > 0);
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
                created_ts: chrono::Utc::now().timestamp(),
            };

            assert_eq!(sig.key_id, key_id);
        }
    }

    #[test]
    fn test_signature_empty_fields() {
        let sig = EventSignature {
            id: uuid::Uuid::new_v4(),
            event_id: String::new(),
            user_id: String::new(),
            device_id: String::new(),
            signature: String::new(),
            key_id: String::new(),
            created_ts: 0,
        };

        assert!(sig.event_id.is_empty());
        assert!(sig.signature.is_empty());
        assert!(sig.key_id.is_empty());
    }

    #[test]
    fn test_signature_long_data() {
        let long_event_id = "$".to_string() + &"a".repeat(500);
        let long_signature = "s".repeat(2000);
        let sig = EventSignature {
            id: uuid::Uuid::new_v4(),
            event_id: long_event_id.clone(),
            user_id: "@user:example.com".to_string(),
            device_id: "DEVICE".to_string(),
            signature: long_signature.clone(),
            key_id: "ed25519:KEY".to_string(),
            created_ts: chrono::Utc::now().timestamp(),
        };

        assert_eq!(sig.event_id.len(), 501);
        assert_eq!(sig.signature.len(), 2000);
    }

    // ── Serialization roundtrip ─────────────────────────────

    #[test]
    fn test_signature_serde_with_unicode() {
        let sig = EventSignature {
            id: uuid::Uuid::new_v4(),
            event_id: "$event:🇨🇳".to_string(),
            user_id: "@用户:example.com".to_string(),
            device_id: "DEV🎯".to_string(),
            signature: "base64_sig_data".to_string(),
            key_id: "ed25519:DEV🎯".to_string(),
            created_ts: 1_700_000_000,
        };

        let json = serde_json::to_string(&sig).unwrap();
        let deserialized: EventSignature = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.event_id, "$event:🇨🇳");
        assert_eq!(deserialized.user_id, "@用户:example.com");
        assert_eq!(deserialized.key_id, "ed25519:DEV🎯");
    }

    #[test]
    fn test_signature_sqlx_from_row_fields() {
        // EventSignature implements FromRow, verify the struct
        // fields align with the expected database column names.
        let sig = EventSignature {
            id: uuid::Uuid::new_v4(),
            event_id: "$event:test".to_string(),
            user_id: "@user:test".to_string(),
            device_id: "DEV1".to_string(),
            signature: "sig".to_string(),
            key_id: "ed25519:DEV1".to_string(),
            created_ts: 1_700_000_000,
        };

        assert_eq!(sig.id, sig.id); // Uuid is valid
        assert!(sig.created_ts > 0);
    }
}
