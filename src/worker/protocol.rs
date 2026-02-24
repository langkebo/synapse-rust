use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReplicationCommand {
    Ping {
        timestamp: i64,
    },
    Pong {
        timestamp: i64,
        server_name: String,
    },
    Name {
        name: String,
    },
    Replicate {
        stream_name: String,
        token: String,
        data: serde_json::Value,
    },
    Rdata {
        stream_name: String,
        token: String,
        rows: Vec<ReplicationRow>,
    },
    Position {
        stream_name: String,
        position: i64,
    },
    Error {
        message: String,
    },
    Sync {
        stream_name: String,
        position: i64,
    },
    UserSync {
        user_id: String,
        state: UserSyncState,
    },
    FederationAck {
        origin: String,
    },
    RemovePushers {
        app_id: String,
        push_key: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplicationRow {
    pub stream_id: i64,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserSyncState {
    Online,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReplicationEvent {
    Events {
        stream_id: i64,
        events: Vec<EventData>,
    },
    Federation {
        stream_id: i64,
        origin: String,
        events: Vec<serde_json::Value>,
    },
    Presence {
        stream_id: i64,
        user_id: String,
        state: PresenceState,
        last_active_ts: i64,
    },
    Receipts {
        stream_id: i64,
        room_id: String,
        receipt_type: String,
        user_id: String,
        event_id: String,
        data: serde_json::Value,
    },
    Typing {
        stream_id: i64,
        room_id: String,
        user_ids: Vec<String>,
    },
    Pushers {
        stream_id: i64,
        user_id: String,
        app_id: String,
        push_key: String,
        push_key_ts: i64,
        data: Option<serde_json::Value>,
        deleted: bool,
    },
    Caches {
        stream_id: i64,
        cache_name: String,
        cache_key: String,
        invalidation_ts: i64,
    },
    PublicRooms {
        stream_id: i64,
        room_id: String,
        visibility: String,
    },
    DeviceLists {
        stream_id: i64,
        user_id: String,
        device_id: Option<String>,
    },
    ToDevice {
        stream_id: i64,
        user_id: String,
        device_id: String,
        message: serde_json::Value,
    },
    AccountData {
        stream_id: i64,
        user_id: String,
        room_id: Option<String>,
        data_type: String,
    },
    Tags {
        stream_id: i64,
        user_id: String,
        room_id: String,
    },
    Backfill {
        stream_id: i64,
        room_id: String,
        events: Vec<serde_json::Value>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventData {
    pub event_id: String,
    pub room_id: String,
    pub event_type: String,
    pub state_key: Option<String>,
    pub sender: String,
    pub content: serde_json::Value,
    pub origin_server_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PresenceState {
    Online,
    Unavailable,
    Offline,
    Busy,
}

impl fmt::Display for ReplicationCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplicationCommand::Ping { timestamp } => write!(f, "PING {}", timestamp),
            ReplicationCommand::Pong {
                timestamp,
                server_name,
            } => {
                write!(f, "PONG {} {}", timestamp, server_name)
            }
            ReplicationCommand::Name { name } => write!(f, "NAME {}", name),
            ReplicationCommand::Replicate {
                stream_name, token, ..
            } => {
                write!(f, "REPLICATE {} {}", stream_name, token)
            }
            ReplicationCommand::Rdata {
                stream_name, token, ..
            } => {
                write!(f, "RDATA {} {}", stream_name, token)
            }
            ReplicationCommand::Position {
                stream_name,
                position,
            } => {
                write!(f, "POSITION {} {}", stream_name, position)
            }
            ReplicationCommand::Error { message } => write!(f, "ERROR {}", message),
            ReplicationCommand::Sync {
                stream_name,
                position,
            } => {
                write!(f, "SYNC {} {}", stream_name, position)
            }
            ReplicationCommand::UserSync { user_id, state } => {
                write!(f, "USER_SYNC {} {:?}", user_id, state)
            }
            ReplicationCommand::FederationAck { origin } => {
                write!(f, "FEDERATION_ACK {}", origin)
            }
            ReplicationCommand::RemovePushers { app_id, push_key } => {
                write!(f, "REMOVE_PUSHERS {} {}", app_id, push_key)
            }
        }
    }
}

impl ReplicationCommand {
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
                Ok(ReplicationCommand::Ping { timestamp })
            }
            "PONG" => {
                let timestamp = parts
                    .get(1)
                    .ok_or_else(|| ReplicationError::MissingField("timestamp".to_string()))?
                    .parse::<i64>()
                    .map_err(|e| ReplicationError::ParseError(e.to_string()))?;
                let server_name = parts
                    .get(2)
                    .ok_or_else(|| ReplicationError::MissingField("server_name".to_string()))?
                    .to_string();
                Ok(ReplicationCommand::Pong {
                    timestamp,
                    server_name,
                })
            }
            "NAME" => {
                let name = parts
                    .get(1)
                    .ok_or_else(|| ReplicationError::MissingField("name".to_string()))?
                    .to_string();
                Ok(ReplicationCommand::Name { name })
            }
            "REPLICATE" => {
                let stream_name = parts
                    .get(1)
                    .ok_or_else(|| ReplicationError::MissingField("stream_name".to_string()))?
                    .to_string();
                let token = parts
                    .get(2)
                    .ok_or_else(|| ReplicationError::MissingField("token".to_string()))?
                    .to_string();
                Ok(ReplicationCommand::Replicate {
                    stream_name,
                    token,
                    data: serde_json::json!({}),
                })
            }
            "RDATA" => {
                let stream_name = parts
                    .get(1)
                    .ok_or_else(|| ReplicationError::MissingField("stream_name".to_string()))?
                    .to_string();
                let token = parts
                    .get(2)
                    .ok_or_else(|| ReplicationError::MissingField("token".to_string()))?
                    .to_string();
                Ok(ReplicationCommand::Rdata {
                    stream_name,
                    token,
                    rows: vec![],
                })
            }
            "POSITION" => {
                let stream_name = parts
                    .get(1)
                    .ok_or_else(|| ReplicationError::MissingField("stream_name".to_string()))?
                    .to_string();
                let position = parts
                    .get(2)
                    .ok_or_else(|| ReplicationError::MissingField("position".to_string()))?
                    .parse::<i64>()
                    .map_err(|e| ReplicationError::ParseError(e.to_string()))?;
                Ok(ReplicationCommand::Position {
                    stream_name,
                    position,
                })
            }
            "ERROR" => {
                let message = if parts.len() > 1 {
                    parts[1..].join(" ")
                } else {
                    "Unknown error".to_string()
                };
                Ok(ReplicationCommand::Error { message })
            }
            "SYNC" => {
                let stream_name = parts
                    .get(1)
                    .ok_or_else(|| ReplicationError::MissingField("stream_name".to_string()))?
                    .to_string();
                let position = parts
                    .get(2)
                    .ok_or_else(|| ReplicationError::MissingField("position".to_string()))?
                    .parse::<i64>()
                    .map_err(|e| ReplicationError::ParseError(e.to_string()))?;
                Ok(ReplicationCommand::Sync {
                    stream_name,
                    position,
                })
            }
            _ => Err(ReplicationError::UnknownCommand(parts[0].to_string())),
        }
    }

    pub fn to_line(&self) -> String {
        format!("{}\n", self)
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ReplicationError {
    #[error("Invalid format: {0}")]
    InvalidFormat(String),
    #[error("Missing field: {0}")]
    MissingField(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Unknown command: {0}")]
    UnknownCommand(String),
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Connection closed")]
    ConnectionClosed,
}

#[derive(Clone)]
pub struct ReplicationProtocol;

impl ReplicationProtocol {
    pub fn new() -> Self {
        Self
    }

    pub fn encode_command(&self, command: &ReplicationCommand) -> Vec<u8> {
        command.to_line().into_bytes()
    }

    pub fn decode_command(&self, data: &[u8]) -> Result<ReplicationCommand, ReplicationError> {
        let line = String::from_utf8_lossy(data);
        let line = line.trim_end_matches('\n').trim_end_matches('\r');
        ReplicationCommand::parse(line)
    }

    pub fn create_ping() -> ReplicationCommand {
        ReplicationCommand::Ping {
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn create_pong(server_name: &str) -> ReplicationCommand {
        ReplicationCommand::Pong {
            timestamp: chrono::Utc::now().timestamp_millis(),
            server_name: server_name.to_string(),
        }
    }

    pub fn create_position(stream_name: &str, position: i64) -> ReplicationCommand {
        ReplicationCommand::Position {
            stream_name: stream_name.to_string(),
            position,
        }
    }

    pub fn create_error(message: &str) -> ReplicationCommand {
        ReplicationCommand::Error {
            message: message.to_string(),
        }
    }

    pub fn create_sync(stream_name: &str, position: i64) -> ReplicationCommand {
        ReplicationCommand::Sync {
            stream_name: stream_name.to_string(),
            position,
        }
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
        let cmd = ReplicationCommand::Pong {
            timestamp: 12345,
            server_name: "example.com".to_string(),
        };
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
        assert_eq!(
            cmd,
            ReplicationCommand::Pong {
                timestamp: 12345,
                server_name: "example.com".to_string()
            }
        );
    }

    #[test]
    fn test_parse_position() {
        let cmd = ReplicationCommand::parse("POSITION events 100").unwrap();
        assert_eq!(
            cmd,
            ReplicationCommand::Position {
                stream_name: "events".to_string(),
                position: 100
            }
        );
    }

    #[test]
    fn test_parse_error() {
        let cmd = ReplicationCommand::parse("ERROR Something went wrong").unwrap();
        assert_eq!(
            cmd,
            ReplicationCommand::Error {
                message: "Something went wrong".to_string()
            }
        );
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
