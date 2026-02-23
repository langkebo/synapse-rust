use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserId {
    pub localpart: String,
    pub server_name: String,
}

impl UserId {
    pub fn new(localpart: &str, server_name: &str) -> Self {
        Self {
            localpart: localpart.to_string(),
            server_name: server_name.to_string(),
        }
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}:{}", self.localpart, self.server_name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomAlias {
    pub localpart: String,
    pub server_name: String,
}

impl RoomAlias {
    pub fn new(localpart: &str, server_name: &str) -> Self {
        Self {
            localpart: localpart.to_string(),
            server_name: server_name.to_string(),
        }
    }
}

impl fmt::Display for RoomAlias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}:{}", self.localpart, self.server_name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventId {
    pub value: String,
    pub server_name: String,
}

impl EventId {
    pub fn new(value: &str, server_name: &str) -> Self {
        Self {
            value: value.to_string(),
            server_name: server_name.to_string(),
        }
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${}", self.value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomVersion {
    pub identifier: String,
    pub needs_authentication: bool,
    pub unstable_features: Option<serde_json::Value>,
}

impl RoomVersion {
    pub fn v1() -> Self {
        Self {
            identifier: "1".to_string(),
            needs_authentication: false,
            unstable_features: None,
        }
    }

    pub fn v2() -> Self {
        Self {
            identifier: "2".to_string(),
            needs_authentication: true,
            unstable_features: Some(serde_json::json!({
                "org.matrix.msc2705.avi": true
            })),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Membership {
    Join,
    Leave,
    Invite,
    Ban,
    Knock,
}

impl fmt::Display for Membership {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Membership::Join => write!(f, "join"),
            Membership::Leave => write!(f, "leave"),
            Membership::Invite => write!(f, "invite"),
            Membership::Ban => write!(f, "ban"),
            Membership::Knock => write!(f, "knock"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Presence {
    Online,
    Offline,
    Unavailable,
}

impl fmt::Display for Presence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Presence::Online => write!(f, "online"),
            Presence::Offline => write!(f, "offline"),
            Presence::Unavailable => write!(f, "unavailable"),
        }
    }
}

#[derive(Clone, Default)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn expose_owned(self) -> String {
        self.0
    }

    pub fn from_env_or(env_key: &str, default: &str) -> Self {
        Self(std::env::var(env_key).unwrap_or_else(|_| default.to_string()))
    }

    pub fn from_env(env_key: &str) -> Option<Self> {
        std::env::var(env_key).ok().map(Self)
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretString([REDACTED])")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl Serialize for SecretString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str("[REDACTED]")
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self(s))
    }
}

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for SecretString {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}
