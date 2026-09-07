use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

/// Pickle 格式标识（Phase 2 引入：megolm_sessions.pickle_format 列）
///
/// - `Legacy`:     自研 AES-256-GCM pickle，写在 `session_key` 列
/// - `Vodozemac`:  vodozemac 0.9 pickle，写在 `session_key` 列
/// - `Dual`:       同时持有两种 pickle（`session_key`=legacy, `vodozemac_pickle`=vodozemac）
///
/// 历史数据全部回填为 `Legacy`；新增 session 在 `MegolmProvider::Vodozemac`
/// 路径下会同时写两种 pickle 以支持平滑回滚。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PickleFormat {
    /// Legacy libolm pickle format (default).
    #[default]
    Legacy,
    /// The `Vodozemac` variant.
    /// The `Dual` variant.
    Vodozemac,
    /// The `Dual` variant.
    Dual,
}

/// (see code)
impl PickleFormat {
    /// See [`as_str`].
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Vodozemac => "vodozemac",
            Self::Dual => "dual",
        }
    }
}

/// (see code)
impl FromStr for PickleFormat {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "vodozemac" => Self::Vodozemac,
            "dual" => Self::Dual,
            _ => Self::Legacy,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `MegolmSession` type.
pub struct MegolmSession {
    /// The `id` field.
    /// The `session_id` field.
    /// The `room_id` field.
    /// The `sender_key` field.
    /// The `session_key` field.
    /// The `algorithm` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub id: Uuid,
    /// The `session_id` field.
    /// The `room_id` field.
    /// The `sender_key` field.
    /// The `session_key` field.
    /// The `algorithm` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub session_id: String,
    /// The `room_id` field.
    /// The `sender_key` field.
    /// The `session_key` field.
    /// The `algorithm` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub room_id: String,
    /// The `sender_key` field.
    /// The `session_key` field.
    /// The `algorithm` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub sender_key: String,
    /// The `session_key` field.
    /// The `algorithm` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub session_key: String,
    /// The `algorithm` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub algorithm: String,
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub message_index: i64,
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub created_ts: DateTime<Utc>,
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub last_used_ts: DateTime<Utc>,
    /// The `expires_at` field.
    pub expires_at: Option<DateTime<Utc>>,
    /// Pickle 格式（Phase 2 引入，默认 `Legacy`）
    #[serde(default)]
    pub pickle_format: PickleFormat,
    /// vodozemac 0.9 pickle 副本（当 `pickle_format` 为 `Vodozemac` 或 `Dual` 时非空）
    #[serde(default)]
    pub vodozemac_pickle: Option<String>,
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
            pickle_format: PickleFormat::Legacy,
            vodozemac_pickle: None,
        };

        assert_eq!(session.room_id, "!room:example.com");
        assert_eq!(session.algorithm, "m.megolm.v1.aes-sha2");
        assert_eq!(session.message_index, 0);
        assert_eq!(session.pickle_format, PickleFormat::Legacy);
        assert!(session.vodozemac_pickle.is_none());
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
            pickle_format: PickleFormat::Dual,
            vodozemac_pickle: Some("base64_pickle".to_string()),
        };

        assert!(session.expires_at.is_some());
        assert!(session.expires_at.unwrap() > current_timestamp_utc());
        assert_eq!(session.pickle_format, PickleFormat::Dual);
        assert!(session.vodozemac_pickle.is_some());
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
            pickle_format: PickleFormat::Legacy,
            vodozemac_pickle: None,
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
                pickle_format: PickleFormat::Legacy,
                vodozemac_pickle: None,
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
            vodozemac_pickle: Some("abc123".to_string()),
        };

        let json = serde_json::to_string(&session).unwrap();
        let deserialized: MegolmSession = serde_json::from_str(&json).unwrap();

        assert_eq!(session.session_id, deserialized.session_id);
        assert_eq!(session.room_id, deserialized.room_id);
        assert_eq!(session.message_index, deserialized.message_index);
        assert_eq!(deserialized.pickle_format, PickleFormat::Vodozemac);
        assert_eq!(deserialized.vodozemac_pickle.as_deref(), Some("abc123"));
    }

    #[test]
    fn test_pickle_format_default_and_roundtrip() {
        assert_eq!(PickleFormat::default(), PickleFormat::Legacy);
        assert_eq!(PickleFormat::Legacy.as_str(), "legacy");
        assert_eq!(PickleFormat::Vodozemac.as_str(), "vodozemac");
        assert_eq!(PickleFormat::Dual.as_str(), "dual");

        // 兼容未知字符串（fallback 到 legacy）
        assert_eq!(PickleFormat::from_str("unknown").unwrap(), PickleFormat::Legacy);
        assert_eq!(PickleFormat::from_str("vodozemac").unwrap(), PickleFormat::Vodozemac);
        assert_eq!(PickleFormat::from_str("dual").unwrap(), PickleFormat::Dual);
    }
}
