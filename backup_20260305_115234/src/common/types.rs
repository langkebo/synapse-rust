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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_id_creation() {
        let user_id = UserId::new("alice", "example.com");
        assert_eq!(user_id.localpart, "alice");
        assert_eq!(user_id.server_name, "example.com");
        assert_eq!(format!("{}", user_id), "@alice:example.com");
    }

    #[test]
    fn test_user_id_serialization() {
        let user_id = UserId::new("bob", "matrix.org");
        let json = serde_json::to_string(&user_id).unwrap();
        assert!(json.contains("bob"));
        assert!(json.contains("matrix.org"));
    }

    #[test]
    fn test_room_alias_creation() {
        let alias = RoomAlias::new("general", "example.com");
        assert_eq!(alias.localpart, "general");
        assert_eq!(alias.server_name, "example.com");
        assert_eq!(format!("{}", alias), "#general:example.com");
    }

    #[test]
    fn test_event_id_creation() {
        let event_id = EventId::new("abc123", "example.com");
        assert_eq!(event_id.value, "abc123");
        assert_eq!(event_id.server_name, "example.com");
        assert_eq!(format!("{}", event_id), "$abc123");
    }

    #[test]
    fn test_room_version_v1() {
        let v1 = RoomVersion::v1();
        assert_eq!(v1.identifier, "1");
        assert!(!v1.needs_authentication);
        assert!(v1.unstable_features.is_none());
    }

    #[test]
    fn test_room_version_v2() {
        let v2 = RoomVersion::v2();
        assert_eq!(v2.identifier, "2");
        assert!(v2.needs_authentication);
        assert!(v2.unstable_features.is_some());
    }

    #[test]
    fn test_membership_display() {
        assert_eq!(format!("{}", Membership::Join), "join");
        assert_eq!(format!("{}", Membership::Leave), "leave");
        assert_eq!(format!("{}", Membership::Invite), "invite");
        assert_eq!(format!("{}", Membership::Ban), "ban");
        assert_eq!(format!("{}", Membership::Knock), "knock");
    }

    #[test]
    fn test_presence_display() {
        assert_eq!(format!("{}", Presence::Online), "online");
        assert_eq!(format!("{}", Presence::Offline), "offline");
        assert_eq!(format!("{}", Presence::Unavailable), "unavailable");
    }

    #[test]
    fn test_secret_string_creation() {
        let secret = SecretString::new("my_secret".to_string());
        assert_eq!(secret.expose(), "my_secret");
    }

    #[test]
    fn test_secret_string_redacted() {
        let secret = SecretString::new("my_secret".to_string());
        assert_eq!(format!("{:?}", secret), "SecretString([REDACTED])");
        assert_eq!(format!("{}", secret), "[REDACTED]");
    }

    #[test]
    fn test_secret_string_from_str() {
        let secret: SecretString = "test_value".into();
        assert_eq!(secret.expose(), "test_value");
    }

    #[test]
    fn test_secret_string_serialization() {
        let secret = SecretString::new("hidden".to_string());
        let json = serde_json::to_string(&secret).unwrap();
        assert_eq!(json, "\"[REDACTED]\"");
    }
}
