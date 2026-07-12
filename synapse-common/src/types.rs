use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserId {
    pub localpart: String,
    pub server_name: String,
}

impl UserId {
    pub fn new(localpart: &str, server_name: &str) -> Self {
        Self { localpart: localpart.to_string(), server_name: server_name.to_string() }
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
        Self { localpart: localpart.to_string(), server_name: server_name.to_string() }
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
        Self { value: value.to_string(), server_name: server_name.to_string() }
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
        Self { identifier: "1".to_string(), needs_authentication: false, unstable_features: None }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
            Self::Join => write!(f, "join"),
            Self::Leave => write!(f, "leave"),
            Self::Invite => write!(f, "invite"),
            Self::Ban => write!(f, "ban"),
            Self::Knock => write!(f, "knock"),
        }
    }
}

impl FromStr for Membership {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "join" => Ok(Self::Join),
            "leave" => Ok(Self::Leave),
            "invite" => Ok(Self::Invite),
            "ban" => Ok(Self::Ban),
            "knock" => Ok(Self::Knock),
            _ => Err(()),
        }
    }
}

/// Unified presence state enum used across the entire codebase.
///
/// Replaces the previously scattered `Presence` (common/types.rs),
/// `PresenceState` (worker/protocol.rs), and raw string comparisons
/// (`"online"`, `"offline"`, `"unavailable"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum PresenceState {
    Online,
    Unavailable,
    Offline,
    Busy,
}

impl PresenceState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Unavailable => "unavailable",
            Self::Busy => "busy",
        }
    }

    /// Derive `last_active_ago` and `currently_active` from the presence
    /// state and an optional absolute timestamp (ms).
    pub fn derive_activity(&self, last_active_ts: Option<i64>, now_ts: i64) -> (Option<i64>, Option<bool>) {
        const CURRENTLY_ACTIVE_THRESHOLD_MS: i64 = 5 * 60 * 1000;
        match self {
            PresenceState::Offline => (None, None),
            PresenceState::Online => {
                let last_active_ago = last_active_ts.map(|ts| (now_ts - ts).max(0));
                let currently_active =
                    Some(last_active_ts.is_some_and(|ts| (now_ts - ts) <= CURRENTLY_ACTIVE_THRESHOLD_MS));
                (last_active_ago, currently_active)
            }
            PresenceState::Unavailable | PresenceState::Busy => {
                let last_active_ago = last_active_ts.map(|ts| (now_ts - ts).max(0));
                (last_active_ago, Some(false))
            }
        }
    }

    /// Whether this state represents an active (non-offline) user.
    pub fn is_active(&self) -> bool {
        !matches!(self, PresenceState::Offline)
    }

    /// All valid presence status strings (for validation).
    pub fn valid_strs() -> &'static [&'static str] {
        &["online", "offline", "unavailable", "away", "busy"]
    }

    /// Attempt to parse a presence string, returning `None` for unknown values.
    /// Maps `"away"` to `Unavailable` for compatibility.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "online" => Some(PresenceState::Online),
            "offline" => Some(PresenceState::Offline),
            "unavailable" | "away" => Some(PresenceState::Unavailable),
            "busy" => Some(PresenceState::Busy),
            _ => None,
        }
    }
}

impl fmt::Display for PresenceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Online => write!(f, "online"),
            Self::Offline => write!(f, "offline"),
            Self::Unavailable => write!(f, "unavailable"),
            Self::Busy => write!(f, "busy"),
        }
    }
}

impl FromStr for PresenceState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str_opt(s).ok_or_else(|| format!("Invalid presence state: {}", s))
    }
}

impl From<&str> for PresenceState {
    fn from(s: &str) -> Self {
        Self::from_str_opt(s).unwrap_or(PresenceState::Offline)
    }
}

/// Backward-compatible alias so existing `Presence` references can be
/// updated incrementally if desired.
pub type Presence = PresenceState;

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
        assert_eq!(format!("{user_id}"), "@alice:example.com");
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
        assert_eq!(format!("{alias}"), "#general:example.com");
    }

    #[test]
    fn test_event_id_creation() {
        let event_id = EventId::new("abc123", "example.com");
        assert_eq!(event_id.value, "abc123");
        assert_eq!(event_id.server_name, "example.com");
        assert_eq!(format!("{event_id}"), "$abc123");
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
        assert_eq!(format!("{}", PresenceState::Online), "online");
        assert_eq!(format!("{}", PresenceState::Offline), "offline");
        assert_eq!(format!("{}", PresenceState::Unavailable), "unavailable");
        assert_eq!(format!("{}", PresenceState::Busy), "busy");
    }

    #[test]
    fn test_presence_from_str() {
        assert_eq!("online".parse::<PresenceState>(), Ok(PresenceState::Online));
        assert_eq!("offline".parse::<PresenceState>(), Ok(PresenceState::Offline));
        assert_eq!("unavailable".parse::<PresenceState>(), Ok(PresenceState::Unavailable));
        assert_eq!("away".parse::<PresenceState>(), Ok(PresenceState::Unavailable));
        assert_eq!("busy".parse::<PresenceState>(), Ok(PresenceState::Busy));
        assert!("unknown".parse::<PresenceState>().is_err());
    }

    #[test]
    fn test_presence_from_str_opt() {
        assert_eq!(PresenceState::from_str_opt("online"), Some(PresenceState::Online));
        assert_eq!(PresenceState::from_str_opt("away"), Some(PresenceState::Unavailable));
        assert_eq!(PresenceState::from_str_opt("unknown"), None);
    }

    #[test]
    fn test_presence_derive_activity() {
        let now = 1_000_000_000_000i64;

        let (ago, active) = PresenceState::Offline.derive_activity(Some(now - 1000), now);
        assert_eq!(ago, None);
        assert_eq!(active, None);

        let (ago, active) = PresenceState::Online.derive_activity(Some(now - 1000), now);
        assert_eq!(ago, Some(1000));
        assert_eq!(active, Some(true));

        let (_ago, active) = PresenceState::Online.derive_activity(Some(now - 400_000), now);
        assert_eq!(active, Some(false));

        let (ago, active) = PresenceState::Unavailable.derive_activity(Some(now - 1000), now);
        assert_eq!(ago, Some(1000));
        assert_eq!(active, Some(false));
    }

    #[test]
    fn test_presence_is_active() {
        assert!(PresenceState::Online.is_active());
        assert!(PresenceState::Unavailable.is_active());
        assert!(PresenceState::Busy.is_active());
        assert!(!PresenceState::Offline.is_active());
    }

    #[test]
    fn test_secret_string_creation() {
        let secret = SecretString::new("my_secret".to_string());
        assert_eq!(secret.expose(), "my_secret");
    }

    #[test]
    fn test_secret_string_redacted() {
        let secret = SecretString::new("my_secret".to_string());
        assert_eq!(format!("{secret:?}"), "SecretString([REDACTED])");
        assert_eq!(format!("{secret}"), "[REDACTED]");
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
