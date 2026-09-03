use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// ─────────────────────────────────────────────────────────────────────────────
// P3-9: Matrix ID newtypes — single source of truth
//
// All Matrix identifiers (user, room, event, alias, device, server) are wrapped
// in typed newtypes so that:
//   * the type system prevents `RoomId` being passed where `UserId` is wanted,
//   * `Eq + Hash` enables direct use in HashSet / HashMap / ordering,
//   * `FromStr` provides a single validation entry-point,
//   * `Display` emits the canonical Matrix wire form.
//
// Migration note: these types are introduced alongside existing `String`
// usage and may be adopted incrementally. They are *not* `Deref<Target=str>`
// to avoid accidentally bypassing `Display`; use `.as_str()` or `&*id`.
// ─────────────────────────────────────────────────────────────────────────────

/// Internal macro: define a Matrix ID newtype with the standard impls.
///
/// Usage:
///   `matrix_id!(UserId, "matrix.user_id");`
///
/// Generates:
///   * `pub struct $name(pub String);` (pub field for incremental adoption)
///   * `Display`, `FromStr`, `AsRef<str>`, `From<String>`, `From<&str>`
///   * `Serialize` / `Deserialize` (transparent string pass-through)
///   * `PartialEq` / `Eq` / `Hash`
///   * `Deref<Target = str>` (read-only convenience)
///
/// `kind` is a stable name used for error messages and Debug output.
macro_rules! matrix_id {
    ($name:ident, $kind:literal $(, doc = $doc:literal)?) => {
        $(#[doc = $doc])?
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            /// Construct from a raw, **unvalidated** string.
            ///
            /// Use `FromStr::from_str` for validated construction.
            #[inline]
            pub fn new_unchecked(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            /// Borrow the underlying raw string slice.
            #[inline]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            #[inline]
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl std::ops::Deref for $name {
            type Target = str;
            #[inline]
            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            #[inline]
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            #[inline]
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }

        impl PartialEq<str> for $name {
            #[inline]
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<&str> for $name {
            #[inline]
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }

        impl FromStr for $name {
            type Err = $crate::types::IdParseError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                if s.is_empty() {
                    return Err(Self::Err { kind: $kind, reason: "empty".into() });
                }
                Ok(Self(s.to_string()))
            }
        }
    };
}

/// Error returned by `FromStr` impls when a Matrix ID fails validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdParseError {
    pub kind: &'static str,
    pub reason: String,
}

impl fmt::Display for IdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {} id: {}", self.kind, self.reason)
    }
}

impl std::error::Error for IdParseError {}

matrix_id!(ServerName, "server_name", doc = "Homeserver name, e.g. `matrix.org`.");
matrix_id!(UserId, "user_id", doc = "Matrix user ID, e.g. `@alice:matrix.org`.");
matrix_id!(RoomId, "room_id", doc = "Matrix room ID, e.g. `!room:matrix.org`.");
matrix_id!(EventId, "event_id", doc = "Matrix event ID, e.g. `$event:matrix.org`.");
matrix_id!(RoomAlias, "room_alias", doc = "Matrix room alias, e.g. `#room:matrix.org`.");
matrix_id!(DeviceId, "device_id", doc = "Matrix device ID, e.g. `JLAIKJWLEI`.");
matrix_id!(TransactionId, "transaction_id", doc = "Client-generated transaction ID, e.g. `tn12345`.");
matrix_id!(MxcUri, "mxc_uri", doc = "MXC media URI, e.g. `mxc://matrix.org/AQDaVF...`.");

// ─────────────────────────────────────────────────────────────────────────────
// P3-9 backward-compat shim: legacy structured fields kept as deprecated accessors
// so the original `UserId::new("alice", "server.com")` shape still compiles.
// These may be removed once all callers migrate.
// ─────────────────────────────────────────────────────────────────────────────

#[deprecated(note = "use `UserId::from_str(...)` or `UserId::new_unchecked(s)` instead")]
impl UserId {
    /// Construct a `UserId` from localpart + server name (legacy API).
    /// Emits the full `@localpart:server_name` form.
    pub fn new(localpart: &str, server_name: &str) -> Self {
        Self(format!("@{localpart}:{server_name}"))
    }
}

#[deprecated(note = "use `RoomAlias::from_str(...)` or `RoomAlias::new_unchecked(s)` instead")]
impl RoomAlias {
    pub fn new(localpart: &str, server_name: &str) -> Self {
        Self(format!("#{localpart}:{server_name}"))
    }
}

#[deprecated(note = "use `EventId::from_str(...)` or `EventId::new_unchecked(s)` instead")]
impl EventId {
    pub fn new(value: &str, _server_name: &str) -> Self {
        // EventId wire form is `$value` (server_name is implicit); preserve legacy
        // signature by ignoring the second argument rather than panicking.
        Self(format!("${value}"))
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
        let user_id = UserId("@alice:example.com".to_string());
        assert_eq!(user_id.as_str(), "@alice:example.com");
        assert_eq!(format!("{user_id}"), "@alice:example.com");
    }

    #[test]
    fn test_user_id_serialization() {
        let user_id = UserId("@bob:matrix.org".to_string());
        let json = serde_json::to_string(&user_id).unwrap();
        assert_eq!(json, "\"@bob:matrix.org\"");
    }

    #[test]
    fn test_room_alias_creation() {
        let alias = RoomAlias("#general:example.com".to_string());
        assert_eq!(alias.as_str(), "#general:example.com");
        assert_eq!(format!("{alias}"), "#general:example.com");
    }

    #[test]
    fn test_event_id_creation() {
        let event_id = EventId("$abc123:example.com".to_string());
        assert_eq!(event_id.as_str(), "$abc123:example.com");
        assert_eq!(format!("{event_id}"), "$abc123:example.com");
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // P3-9: Matrix ID newtype tests
    // ─────────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_matrix_id_constructors() {
        // ServerName
        let server: ServerName = "matrix.org".parse().unwrap();
        assert_eq!(server.as_str(), "matrix.org");

        // RoomId
        let room = RoomId::new_unchecked("!abc:matrix.org");
        assert_eq!(room.as_str(), "!abc:matrix.org");
        assert_eq!(format!("{room}"), "!abc:matrix.org");

        // DeviceId
        let dev = DeviceId::new_unchecked("JLAIKJWLEI");
        assert_eq!(dev.as_str(), "JLAIKJWLEI");

        // TransactionId
        let txn = TransactionId::new_unchecked("tn123");
        assert_eq!(txn.as_str(), "tn123");

        // MxcUri
        let mxc = MxcUri::new_unchecked("mxc://matrix.org/AQDaVFlbkQoErdOgqWRgiGSV");
        assert_eq!(mxc.as_str(), "mxc://matrix.org/AQDaVFlbkQoErdOgqWRgiGSV");
    }

    #[test]
    fn test_matrix_id_hash_eq() {
        use std::collections::HashSet;
        let mut set: HashSet<RoomId> = HashSet::new();
        set.insert(RoomId::new_unchecked("!a:m.org"));
        set.insert(RoomId::new_unchecked("!a:m.org"));
        set.insert(RoomId::new_unchecked("!b:m.org"));
        assert_eq!(set.len(), 2);

        // Equality
        assert_eq!(RoomId::new_unchecked("!a:m.org"), RoomId::new_unchecked("!a:m.org"));
        assert_ne!(RoomId::new_unchecked("!a:m.org"), RoomId::new_unchecked("!b:m.org"));
    }

    #[test]
    fn test_matrix_id_partial_eq_str() {
        let room = RoomId::new_unchecked("!a:m.org");
        assert!(room == "!a:m.org");
        assert!(room != "!b:m.org");
    }

    #[test]
    fn test_matrix_id_from_str_validates_nonempty() {
        assert!("".parse::<UserId>().is_err());
        assert!("non-empty".parse::<RoomId>().is_ok());
    }

    #[test]
    fn test_matrix_id_serde_transparent() {
        // `#[serde(transparent)]` means the wire form is just the inner string.
        let user: UserId = "@a:b.org".to_string().into();
        let json = serde_json::to_string(&user).unwrap();
        assert_eq!(json, "\"@a:b.org\"");

        let parsed: UserId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, user);
    }

    #[test]
    fn test_matrix_id_deref_and_as_ref() {
        let room = RoomId::new_unchecked("!r:s");
        // Deref<Target = str>
        let s: &str = &room;
        assert_eq!(s, "!r:s");
        // AsRef<str>
        let s: &str = room.as_ref();
        assert_eq!(s, "!r:s");
        // .as_str()
        assert_eq!(room.as_str(), "!r:s");
    }

    #[test]
    fn test_id_parse_error_display() {
        let err: IdParseError = "".parse::<UserId>().unwrap_err();
        assert_eq!(format!("{err}"), "invalid user_id id: empty");
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
