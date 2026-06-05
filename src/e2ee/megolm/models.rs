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
    #[default]
    Legacy,
    Vodozemac,
    Dual,
}

impl PickleFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Vodozemac => "vodozemac",
            Self::Dual => "dual",
        }
    }
}

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
pub struct MegolmSession {
    pub id: Uuid,
    pub session_id: String,
    pub room_id: String,
    pub sender_key: String,
    pub session_key: String,
    pub algorithm: String,
    pub message_index: i64,
    pub created_ts: DateTime<Utc>,
    pub last_used_ts: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    /// Pickle 格式（Phase 2 引入，默认 `Legacy`）
    #[serde(default)]
    pub pickle_format: PickleFormat,
    /// vodozemac 0.9 pickle 副本（当 `pickle_format` 为 `Vodozemac` 或 `Dual` 时非空）
    #[serde(default)]
    pub vodozemac_pickle: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomKeyDistributionData {
    pub session_id: String,
    pub session_key: String,
    pub algorithm: String,
    pub room_id: String,
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
            created_ts: chrono::Utc::now(),
            last_used_ts: chrono::Utc::now(),
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
        let expires = chrono::Utc::now() + chrono::Duration::hours(24);
        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: "session123".to_string(),
            room_id: "!room:example.com".to_string(),
            sender_key: "sender_key123".to_string(),
            session_key: "session_key456".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 100,
            created_ts: chrono::Utc::now(),
            last_used_ts: chrono::Utc::now(),
            expires_at: Some(expires),
            pickle_format: PickleFormat::Dual,
            vodozemac_pickle: Some("base64_pickle".to_string()),
        };

        assert!(session.expires_at.is_some());
        assert!(session.expires_at.unwrap() > chrono::Utc::now());
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
            created_ts: chrono::Utc::now(),
            last_used_ts: chrono::Utc::now(),
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
                created_ts: chrono::Utc::now(),
                last_used_ts: chrono::Utc::now(),
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
            created_ts: chrono::Utc::now(),
            last_used_ts: chrono::Utc::now(),
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
