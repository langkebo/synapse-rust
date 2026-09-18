use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

/// Pickle format identifier for megolm sessions.
/// Since E-12, only Vodozemac pickle format is supported (legacy/dual removed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PickleFormat {
    /// The vodozemac 0.9 pickle format (default).
    #[default]
    Vodozemac,
}

impl PickleFormat {
    /// See [`as_str`].
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Vodozemac => "vodozemac",
        }
    }
}

impl FromStr for PickleFormat {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "vodozemac" => Self::Vodozemac,
            _ => Self::Vodozemac, // All unknown values default to Vodozemac
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `MegolmSession` type.
pub struct MegolmSession {
    /// The `id` field.
    pub id: Uuid,
    /// The `session_id` field.
    pub session_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender_key` field.
    pub sender_key: String,
    /// Session key: vodozemac pickle (outbound) or raw key bytes (inbound, base64 encoded)
    pub session_key: String,
    /// The `algorithm` field.
    pub algorithm: String,
    /// The `message_index` field.
    pub message_index: i64,
    /// The `created_ts` field.
    pub created_ts: DateTime<Utc>,
    /// The `last_used_ts` field.
    pub last_used_ts: DateTime<Utc>,
    /// The `expires_at` field.
    pub expires_at: Option<DateTime<Utc>>,
    /// Pickle format (default Vodozemac; kept for schema compatibility but always Vodozemac after E-12)
    #[serde(default)]
    pub pickle_format: PickleFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `EncryptedEvent` type.
pub struct EncryptedEvent {
    /// The `room_id` field.
    /// The `event_id` field.
    /// The `sender` field.
    /// The `content` field.
    /// The `algorithm` field.
    /// The `session_id` field.
    /// The `ciphertext` field.
    /// The `device_id` field.
    pub room_id: String,
    /// The `event_id` field.
    /// The `sender` field.
    /// The `content` field.
    /// The `algorithm` field.
    /// The `session_id` field.
    /// The `ciphertext` field.
    /// The `device_id` field.
    pub event_id: String,
    /// The `sender` field.
    /// The `content` field.
    /// The `algorithm` field.
    /// The `session_id` field.
    /// The `ciphertext` field.
    /// The `device_id` field.
    pub sender: String,
    /// The `content` field.
    /// The `algorithm` field.
    /// The `session_id` field.
    /// The `ciphertext` field.
    /// The `device_id` field.
    pub content: serde_json::Value,
    /// The `algorithm` field.
    /// The `session_id` field.
    /// The `ciphertext` field.
    /// The `device_id` field.
    pub algorithm: String,
    /// The `session_id` field.
    /// The `ciphertext` field.
    /// The `device_id` field.
    pub session_id: String,
    /// The `ciphertext` field.
    /// The `device_id` field.
    pub ciphertext: String,
    /// The `device_id` field.
    pub device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `RoomKeyDistributionData` type.
pub struct RoomKeyDistributionData {
    /// The `session_id` field.
    /// The `session_key` field.
    /// The `algorithm` field.
    /// The `room_id` field.
    pub session_id: String,
    /// The `session_key` field.
    /// The `algorithm` field.
    /// The `room_id` field.
    pub session_key: String,
    /// The `algorithm` field.
    /// The `room_id` field.
    pub algorithm: String,
    /// The `room_id` field.
    pub room_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_common::current_timestamp_utc;

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
            created_ts: current_timestamp_utc(),
            last_used_ts: current_timestamp_utc(),
            expires_at: None,
            pickle_format: PickleFormat::Vodozemac,
        };

        assert_eq!(session.room_id, "!room:example.com");
        assert_eq!(session.algorithm, "m.megolm.v1.aes-sha2");
        assert_eq!(session.message_index, 0);
        assert_eq!(session.pickle_format, PickleFormat::Vodozemac);
    }

    #[test]
    fn test_megolm_session_with_expiry() {
        let expires = current_timestamp_utc() + chrono::Duration::hours(24);
        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: "session123".to_string(),
            room_id: "!room:example.com".to_string(),
            sender_key: "sender_key123".to_string(),
            session_key: "session_key456".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 100,
            created_ts: current_timestamp_utc(),
            last_used_ts: current_timestamp_utc(),
            expires_at: Some(expires),
            pickle_format: PickleFormat::Vodozemac,
        };

        assert!(session.expires_at.is_some());
        assert!(session.expires_at.unwrap() > current_timestamp_utc());
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
            created_ts: current_timestamp_utc(),
            last_used_ts: current_timestamp_utc(),
            expires_at: None,
            pickle_format: PickleFormat::Vodozemac,
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
        let algorithms = vec!["m.megolm.v1.aes-sha2", "m.olm.v1.curve25519-aes-sha2"];

        for algo in algorithms {
            let session = MegolmSession {
                id: uuid::Uuid::new_v4(),
                session_id: "test".to_string(),
                room_id: "!room:example.com".to_string(),
                sender_key: "key".to_string(),
                session_key: "key".to_string(),
                algorithm: algo.to_string(),
                message_index: 0,
                created_ts: current_timestamp_utc(),
                last_used_ts: current_timestamp_utc(),
                expires_at: None,
                pickle_format: PickleFormat::Vodozemac,
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
            created_ts: current_timestamp_utc(),
            last_used_ts: current_timestamp_utc(),
            expires_at: None,
            pickle_format: PickleFormat::Vodozemac,
        };

        let json = serde_json::to_string(&session).unwrap();
        let deserialized: MegolmSession = serde_json::from_str(&json).unwrap();

        assert_eq!(session.session_id, deserialized.session_id);
        assert_eq!(session.room_id, deserialized.room_id);
        assert_eq!(session.message_index, deserialized.message_index);
        assert_eq!(deserialized.pickle_format, PickleFormat::Vodozemac);
    }

    #[test]
    fn test_pickle_format_default_and_roundtrip() {
        assert_eq!(PickleFormat::default(), PickleFormat::Vodozemac);
        assert_eq!(PickleFormat::Vodozemac.as_str(), "vodozemac");
    }
}
