use serde::{Deserialize, Serialize};
use std::fmt;
use synapse_common::current_timestamp_millis;

/// The `ReplicationCommand` enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReplicationCommand {
    /// The `Ping` variant.
    Ping {
        /// The `timestamp` field.
        timestamp: i64,
    },
    /// The `Pong` variant.
    Pong {
        /// The `timestamp` field.
        timestamp: i64,
        /// The `server_name` field.
        server_name: String,
    },
    /// The `Name` variant.
    Name {
        /// The `name` field.
        name: String,
    },
    /// The `Replicate` variant.
    Replicate {
        /// The `stream_name` field.
        stream_name: String,
        /// The `token` field.
        token: String,
        /// The `data` field.
        data: serde_json::Value,
    },
    /// The `Rdata` variant.
    Rdata {
        /// The `stream_name` field.
        stream_name: String,
        /// The `token` field.
        token: String,
        /// The `rows` field.
        rows: Vec<ReplicationRow>,
    },
    /// The `Position` variant.
    Position {
        /// The `stream_name` field.
        stream_name: String,
        /// The `position` field.
        position: i64,
    },
    /// The `Error` variant.
    Error {
        /// The `message` field.
        message: String,
    },
    /// The `Sync` variant.
    Sync {
        /// The `stream_name` field.
        stream_name: String,
        /// The `position` field.
        position: i64,
    },
    /// The `UserSync` variant.
    UserSync {
        /// The `user_id` field.
        user_id: String,
        /// The `state` field.
        state: UserSyncState,
    },
    /// The `FederationAck` variant.
    FederationAck {
        /// The `origin` field.
        origin: String,
    },
    /// The `RemovePushers` variant.
    RemovePushers {
        /// The `app_id` field.
        app_id: String,
        /// The `push_key` field.
        push_key: String,
    },
}

/// The `ReplicationRow` struct.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplicationRow {
    /// The `stream_id` field.
    pub stream_id: i64,
    /// The `data` field.
    pub data: serde_json::Value,
}

/// The `UserSyncState` enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserSyncState {
    /// The `Online` variant.
    Online,
    /// The `Offline` variant.
    Offline,
}

/// The `ReplicationEvent` enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReplicationEvent {
    /// The `Events` variant.
    Events {
        /// The `stream_id` field.
        stream_id: i64,
        /// The `events` field.
        events: Vec<EventData>,
    },
    /// The `Federation` variant.
    Federation {
        /// The `stream_id` field.
        stream_id: i64,
        /// The `origin` field.
        origin: String,
        /// The `events` field.
        events: Vec<serde_json::Value>,
    },
    /// The `Presence` variant.
    Presence {
        /// The `stream_id` field.
        stream_id: i64,
        /// The `user_id` field.
        user_id: String,
        /// The `state` field.
        state: PresenceState,
        /// The `last_active_ts` field.
        last_active_ts: i64,
    },
    /// The `Receipts` variant.
    Receipts {
        /// The `stream_id` field.
        stream_id: i64,
        /// The `room_id` field.
        room_id: String,
        /// The `receipt_type` field.
        receipt_type: String,
        /// The `user_id` field.
        user_id: String,
        /// The `event_id` field.
        event_id: String,
        /// The `data` field.
        data: serde_json::Value,
    },
    /// The `Typing` variant.
    Typing {
        /// The `stream_id` field.
        stream_id: i64,
        /// The `room_id` field.
        room_id: String,
        /// The `user_ids` field.
        user_ids: Vec<String>,
    },
    /// The `Pushers` variant.
    Pushers {
        /// The `stream_id` field.
        stream_id: i64,
        /// The `user_id` field.
        user_id: String,
        /// The `app_id` field.
        app_id: String,
        /// The `push_key` field.
        push_key: String,
        /// The `push_key_ts` field.
        push_key_ts: i64,
        /// The `data` field.
        data: Option<serde_json::Value>,
        /// The `deleted` field.
        deleted: bool,
    },
    /// The `Caches` variant.
    Caches {
        /// The `stream_id` field.
        stream_id: i64,
        /// The `cache_name` field.
        cache_name: String,
        /// The `cache_key` field.
        cache_key: String,
        /// The `invalidation_ts` field.
        invalidation_ts: i64,
    },
    /// The `PublicRooms` variant.
    PublicRooms {
        /// The `stream_id` field.
        stream_id: i64,
        /// The `room_id` field.
        room_id: String,
        /// The `visibility` field.
        visibility: String,
    },
    /// The `DeviceLists` variant.
    DeviceLists {
        /// The `stream_id` field.
        stream_id: i64,
        /// The `user_id` field.
        user_id: String,
        /// The `device_id` field.
        device_id: Option<String>,
    },
    /// The `ToDevice` variant.
    ToDevice {
        /// The `stream_id` field.
        stream_id: i64,
        /// The `user_id` field.
        user_id: String,
        /// The `device_id` field.
        device_id: String,
        /// The `message` field.
        message: serde_json::Value,
    },
    /// The `AccountData` variant.
    AccountData {
        /// The `stream_id` field.
        stream_id: i64,
        /// The `user_id` field.
        user_id: String,
        /// The `room_id` field.
        room_id: Option<String>,
        /// The `data_type` field.
        data_type: String,
    },
    /// The `Tags` variant.
    Tags {
        /// The `stream_id` field.
        stream_id: i64,
        /// The `user_id` field.
        user_id: String,
        /// The `room_id` field.
        room_id: String,
    },
    /// The `Backfill` variant.
    Backfill {
        /// The `stream_id` field.
        stream_id: i64,
        /// The `room_id` field.
        room_id: String,
        /// The `events` field.
        events: Vec<serde_json::Value>,
    },
}

/// The `EventData` struct.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventData {
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `state_key` field.
    pub state_key: Option<String>,
    /// The `sender` field.
    pub sender: String,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
}

/// The `PresenceState` enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PresenceState {
    /// The `Online` variant.
    Online,
    /// The `Unavailable` variant.
    Unavailable,
    /// The `Offline` variant.
    Offline,
    /// The `Busy` variant.
    Busy,
}

impl fmt::Display for ReplicationCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ping { timestamp } => write!(f, "PING {timestamp}"),
            Self::Pong { timestamp, server_name } => {
                write!(f, "PONG {timestamp} {server_name}")
            }
            Self::Name { name } => write!(f, "NAME {name}"),
            Self::Replicate { stream_name, token, .. } => {
                write!(f, "REPLICATE {stream_name} {token}")
            }
            Self::Rdata { stream_name, token, .. } => {
                write!(f, "RDATA {stream_name} {token}")
            }
            Self::Position { stream_name, position } => {
                write!(f, "POSITION {stream_name} {position}")
            }
            Self::Error { message } => write!(f, "ERROR {message}"),
            Self::Sync { stream_name, position } => {
                write!(f, "SYNC {stream_name} {position}")
            }
            Self::UserSync { user_id, state } => {
                write!(f, "USER_SYNC {user_id} {state:?}")
            }
            Self::FederationAck { origin } => {
                write!(f, "FEDERATION_ACK {origin}")
            }
            Self::RemovePushers { app_id, push_key } => {
                write!(f, "REMOVE_PUSHERS {app_id} {push_key}")
            }
        }
    }
}

impl ReplicationCommand {
    /// See [`parse`].
    pub fn parse(line: &str) -> Result<Self, ReplicationError> {
        let line = line.trim();
        if line.is_empty() {
            return Err(ReplicationError::InvalidFormat("Empty line".to_string()));
        }

        let parts: Vec<&str> = line.splitn(3, ' ').collect();

        match parts[0] {
            "PING" => {
                let timestamp = parts
                    .get(1)
                    .ok_or_else(|| ReplicationError::MissingField("timestamp".to_string()))?
                    .parse::<i64>()
                    .map_err(|e| ReplicationError::ParseError(e.to_string()))?;
                Ok(Self::Ping { timestamp })
            }
            "PONG" => {
                let timestamp = parts
                    .get(1)
                    .ok_or_else(|| ReplicationError::MissingField("timestamp".to_string()))?
                    .parse::<i64>()
                    .map_err(|e| ReplicationError::ParseError(e.to_string()))?;
                let server_name =
                    parts.get(2).ok_or_else(|| ReplicationError::MissingField("server_name".to_string()))?.to_string();
                Ok(Self::Pong { timestamp, server_name })
            }
            "NAME" => {
                let name = parts.get(1).ok_or_else(|| ReplicationError::MissingField("name".to_string()))?.to_string();
                Ok(Self::Name { name })
            }
            "REPLICATE" => {
                let stream_name =
                    parts.get(1).ok_or_else(|| ReplicationError::MissingField("stream_name".to_string()))?.to_string();
                let token =
                    parts.get(2).ok_or_else(|| ReplicationError::MissingField("token".to_string()))?.to_string();
                Ok(Self::Replicate { stream_name, token, data: serde_json::json!({}) })
            }
            "RDATA" => {
                let stream_name =
                    parts.get(1).ok_or_else(|| ReplicationError::MissingField("stream_name".to_string()))?.to_string();
                let token =
                    parts.get(2).ok_or_else(|| ReplicationError::MissingField("token".to_string()))?.to_string();
                Ok(Self::Rdata { stream_name, token, rows: vec![] })
            }
            "POSITION" => {
                let stream_name =
                    parts.get(1).ok_or_else(|| ReplicationError::MissingField("stream_name".to_string()))?.to_string();
                let position = parts
                    .get(2)
                    .ok_or_else(|| ReplicationError::MissingField("position".to_string()))?
                    .parse::<i64>()
                    .map_err(|e| ReplicationError::ParseError(e.to_string()))?;
                Ok(Self::Position { stream_name, position })
            }
            "ERROR" => {
                let message = if parts.len() > 1 { parts[1..].join(" ") } else { "Unknown error".to_string() };
                Ok(Self::Error { message })
            }
            "SYNC" => {
                let stream_name =
                    parts.get(1).ok_or_else(|| ReplicationError::MissingField("stream_name".to_string()))?.to_string();
                let position = parts
                    .get(2)
                    .ok_or_else(|| ReplicationError::MissingField("position".to_string()))?
                    .parse::<i64>()
                    .map_err(|e| ReplicationError::ParseError(e.to_string()))?;
                Ok(Self::Sync { stream_name, position })
            }
            "USER_SYNC" => {
                let user_id =
                    parts.get(1).ok_or_else(|| ReplicationError::MissingField("user_id".to_string()))?.to_string();
                let state_str = parts.get(2).ok_or_else(|| ReplicationError::MissingField("state".to_string()))?;
                let state = match *state_str {
                    "Online" => UserSyncState::Online,
                    "Offline" => UserSyncState::Offline,
                    _ => return Err(ReplicationError::ParseError(format!("Unknown state: {state_str}"))),
                };
                Ok(Self::UserSync { user_id, state })
            }
            "FEDERATION_ACK" => {
                let origin =
                    parts.get(1).ok_or_else(|| ReplicationError::MissingField("origin".to_string()))?.to_string();
                Ok(Self::FederationAck { origin })
            }
            "REMOVE_PUSHERS" => {
                let app_id =
                    parts.get(1).ok_or_else(|| ReplicationError::MissingField("app_id".to_string()))?.to_string();
                let push_key =
                    parts.get(2).ok_or_else(|| ReplicationError::MissingField("push_key".to_string()))?.to_string();
                Ok(Self::RemovePushers { app_id, push_key })
            }
            _ => Err(ReplicationError::UnknownCommand(parts[0].to_string())),
        }
    }

    /// See [`to_line`].
    pub fn to_line(&self) -> String {
        format!("{self}\n")
    }
}

/// The `ReplicationError` enum.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ReplicationError {
    #[error("Invalid format: {0}")]
    /// The `InvalidFormat` variant.
    InvalidFormat(String),
    #[error("Missing field: {0}")]
    /// The `MissingField` variant.
    MissingField(String),
    #[error("Parse error: {0}")]
    /// The `ParseError` variant.
    ParseError(String),
    #[error("Unknown command: {0}")]
    /// The `UnknownCommand` variant.
    UnknownCommand(String),
    #[error("IO error: {0}")]
    /// The `IoError` variant.
    IoError(String),
    #[error("Connection closed")]
    /// The `ConnectionClosed` variant.
    ConnectionClosed,
}

/// The `ReplicationProtocol` struct.
#[derive(Clone)]
pub struct ReplicationProtocol;

impl ReplicationProtocol {
    /// See [`new`].
    pub fn new() -> Self {
        Self
    }

    /// See [`encode_command`].
    pub fn encode_command(&self, command: &ReplicationCommand) -> Vec<u8> {
        command.to_line().into_bytes()
    }

    /// See [`decode_command`].
    pub fn decode_command(&self, data: &[u8]) -> Result<ReplicationCommand, ReplicationError> {
        let line = String::from_utf8_lossy(data);
        let line = line.trim_end_matches('\n').trim_end_matches('\r');
        ReplicationCommand::parse(line)
    }

    /// See [`create_ping`].
    pub fn create_ping() -> ReplicationCommand {
        ReplicationCommand::Ping { timestamp: current_timestamp_millis() }
    }

    /// See [`create_pong`].
    pub fn create_pong(server_name: &str) -> ReplicationCommand {
        ReplicationCommand::Pong { timestamp: current_timestamp_millis(), server_name: server_name.to_string() }
    }

    /// See [`create_position`].
    pub fn create_position(stream_name: &str, position: i64) -> ReplicationCommand {
        ReplicationCommand::Position { stream_name: stream_name.to_string(), position }
    }

    /// See [`create_error`].
    pub fn create_error(message: &str) -> ReplicationCommand {
        ReplicationCommand::Error { message: message.to_string() }
    }

    /// See [`create_sync`].
    pub fn create_sync(stream_name: &str, position: i64) -> ReplicationCommand {
        ReplicationCommand::Sync { stream_name: stream_name.to_string(), position }
    }
}

impl Default for ReplicationProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ping_command() {
        let cmd = ReplicationCommand::Ping { timestamp: 12345 };
        assert_eq!(cmd.to_string(), "PING 12345");
    }

    #[test]
    fn test_pong_command() {
        let cmd = ReplicationCommand::Pong { timestamp: 12345, server_name: "example.com".to_string() };
        assert_eq!(cmd.to_string(), "PONG 12345 example.com");
    }

    #[test]
    fn test_parse_ping() {
        let cmd = ReplicationCommand::parse("PING 12345").unwrap();
        assert_eq!(cmd, ReplicationCommand::Ping { timestamp: 12345 });
    }

    #[test]
    fn test_parse_pong() {
        let cmd = ReplicationCommand::parse("PONG 12345 example.com").unwrap();
        assert_eq!(cmd, ReplicationCommand::Pong { timestamp: 12345, server_name: "example.com".to_string() });
    }

    #[test]
    fn test_parse_position() {
        let cmd = ReplicationCommand::parse("POSITION events 100").unwrap();
        assert_eq!(cmd, ReplicationCommand::Position { stream_name: "events".to_string(), position: 100 });
    }

    #[test]
    fn test_parse_error() {
        let cmd = ReplicationCommand::parse("ERROR Something went wrong").unwrap();
        assert_eq!(cmd, ReplicationCommand::Error { message: "Something went wrong".to_string() });
    }

    #[test]
    fn test_parse_invalid() {
        let result = ReplicationCommand::parse("INVALID");
        assert!(result.is_err());
    }

    #[test]
    fn test_protocol_encode_decode() {
        let protocol = ReplicationProtocol::new();
        let cmd = ReplicationCommand::Ping { timestamp: 12345 };
        let encoded = protocol.encode_command(&cmd);
        let decoded = protocol.decode_command(&encoded).unwrap();
        assert_eq!(cmd, decoded);
    }

    #[test]
    fn test_create_ping() {
        let cmd = ReplicationProtocol::create_ping();
        match cmd {
            ReplicationCommand::Ping { timestamp } => {
                assert!(timestamp > 0);
            }
            _ => panic!("Expected Ping command"),
        }
    }

    #[test]
    fn test_create_pong() {
        let cmd = ReplicationProtocol::create_pong("test.com");
        match cmd {
            ReplicationCommand::Pong { server_name, .. } => {
                assert_eq!(server_name, "test.com");
            }
            _ => panic!("Expected Pong command"),
        }
    }
}
