use serde::{Deserialize, Serialize};
use std::fmt;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompactMembership {
    Join = 0,
    Leave = 1,
    Invite = 2,
    Ban = 3,
    Knock = 4,
}

impl CompactMembership {
    pub fn as_str(&self) -> &'static str {
        match self {
            CompactMembership::Join => "join",
            CompactMembership::Leave => "leave",
            CompactMembership::Invite => "invite",
            CompactMembership::Ban => "ban",
            CompactMembership::Knock => "knock",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "join" => Some(CompactMembership::Join),
            "leave" => Some(CompactMembership::Leave),
            "invite" => Some(CompactMembership::Invite),
            "ban" => Some(CompactMembership::Ban),
            "knock" => Some(CompactMembership::Knock),
            _ => None,
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(CompactMembership::Join),
            1 => Some(CompactMembership::Leave),
            2 => Some(CompactMembership::Invite),
            3 => Some(CompactMembership::Ban),
            4 => Some(CompactMembership::Knock),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }

    pub const fn size() -> usize {
        std::mem::size_of::<Self>()
    }
}

impl fmt::Display for CompactMembership {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Default for CompactMembership {
    fn default() -> Self {
        CompactMembership::Leave
    }
}

impl From<crate::common::types::Membership> for CompactMembership {
    fn from(m: crate::common::types::Membership) -> Self {
        match m {
            crate::common::types::Membership::Join => CompactMembership::Join,
            crate::common::types::Membership::Leave => CompactMembership::Leave,
            crate::common::types::Membership::Invite => CompactMembership::Invite,
            crate::common::types::Membership::Ban => CompactMembership::Ban,
            crate::common::types::Membership::Knock => CompactMembership::Knock,
        }
    }
}

impl From<CompactMembership> for crate::common::types::Membership {
    fn from(m: CompactMembership) -> Self {
        match m {
            CompactMembership::Join => crate::common::types::Membership::Join,
            CompactMembership::Leave => crate::common::types::Membership::Leave,
            CompactMembership::Invite => crate::common::types::Membership::Invite,
            CompactMembership::Ban => crate::common::types::Membership::Ban,
            CompactMembership::Knock => crate::common::types::Membership::Knock,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompactPresence {
    Online = 0,
    Offline = 1,
    Unavailable = 2,
}

impl CompactPresence {
    pub fn as_str(&self) -> &'static str {
        match self {
            CompactPresence::Online => "online",
            CompactPresence::Offline => "offline",
            CompactPresence::Unavailable => "unavailable",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "online" => Some(CompactPresence::Online),
            "offline" => Some(CompactPresence::Offline),
            "unavailable" => Some(CompactPresence::Unavailable),
            _ => None,
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(CompactPresence::Online),
            1 => Some(CompactPresence::Offline),
            2 => Some(CompactPresence::Unavailable),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }

    pub const fn size() -> usize {
        std::mem::size_of::<Self>()
    }
}

impl fmt::Display for CompactPresence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Default for CompactPresence {
    fn default() -> Self {
        CompactPresence::Offline
    }
}

impl From<crate::common::types::Presence> for CompactPresence {
    fn from(p: crate::common::types::Presence) -> Self {
        match p {
            crate::common::types::Presence::Online => CompactPresence::Online,
            crate::common::types::Presence::Offline => CompactPresence::Offline,
            crate::common::types::Presence::Unavailable => CompactPresence::Unavailable,
        }
    }
}

impl From<CompactPresence> for crate::common::types::Presence {
    fn from(p: CompactPresence) -> Self {
        match p {
            CompactPresence::Online => crate::common::types::Presence::Online,
            CompactPresence::Offline => crate::common::types::Presence::Offline,
            CompactPresence::Unavailable => crate::common::types::Presence::Unavailable,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompactEventState {
    Original = 0,
    Redacted = 1,
    SoftFailed = 2,
}

impl CompactEventState {
    pub fn as_str(&self) -> &'static str {
        match self {
            CompactEventState::Original => "original",
            CompactEventState::Redacted => "redacted",
            CompactEventState::SoftFailed => "soft_failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "original" => Some(CompactEventState::Original),
            "redacted" => Some(CompactEventState::Redacted),
            "soft_failed" => Some(CompactEventState::SoftFailed),
            _ => None,
        }
    }
}

impl Default for CompactEventState {
    fn default() -> Self {
        CompactEventState::Original
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompactRoomVisibility {
    Public = 0,
    Private = 1,
    InviteOnly = 2,
    KnockOnly = 3,
}

impl CompactRoomVisibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            CompactRoomVisibility::Public => "public",
            CompactRoomVisibility::Private => "private",
            CompactRoomVisibility::InviteOnly => "invite_only",
            CompactRoomVisibility::KnockOnly => "knock_only",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "public" => Some(CompactRoomVisibility::Public),
            "private" => Some(CompactRoomVisibility::Private),
            "invite_only" => Some(CompactRoomVisibility::InviteOnly),
            "knock_only" => Some(CompactRoomVisibility::KnockOnly),
            _ => None,
        }
    }
}

impl Default for CompactRoomVisibility {
    fn default() -> Self {
        CompactRoomVisibility::Private
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompactPushRuleKind {
    Override = 0,
    Content = 1,
    Room = 2,
    Sender = 3,
    Underride = 4,
}

impl CompactPushRuleKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CompactPushRuleKind::Override => "override",
            CompactPushRuleKind::Content => "content",
            CompactPushRuleKind::Room => "room",
            CompactPushRuleKind::Sender => "sender",
            CompactPushRuleKind::Underride => "underride",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "override" => Some(CompactPushRuleKind::Override),
            "content" => Some(CompactPushRuleKind::Content),
            "room" => Some(CompactPushRuleKind::Room),
            "sender" => Some(CompactPushRuleKind::Sender),
            "underride" => Some(CompactPushRuleKind::Underride),
            _ => None,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompactDeviceType {
    Web = 0,
    Desktop = 1,
    Mobile = 2,
    Unknown = 3,
}

impl CompactDeviceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CompactDeviceType::Web => "web",
            CompactDeviceType::Desktop => "desktop",
            CompactDeviceType::Mobile => "mobile",
            CompactDeviceType::Unknown => "unknown",
        }
    }

    pub fn from_user_agent(user_agent: &str) -> Self {
        let ua_lower = user_agent.to_lowercase();
        if ua_lower.contains("mobile") || ua_lower.contains("android") || ua_lower.contains("iphone") {
            CompactDeviceType::Mobile
        } else if ua_lower.contains("electron") || ua_lower.contains("desktop") {
            CompactDeviceType::Desktop
        } else if ua_lower.contains("mozilla") || ua_lower.contains("chrome") || ua_lower.contains("safari") {
            CompactDeviceType::Web
        } else {
            CompactDeviceType::Unknown
        }
    }
}

impl Default for CompactDeviceType {
    fn default() -> Self {
        CompactDeviceType::Unknown
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompactAuthType {
    Password = 0,
    Token = 1,
    SSO = 2,
    OIDC = 3,
    SAML = 4,
    CAS = 5,
}

impl CompactAuthType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CompactAuthType::Password => "password",
            CompactAuthType::Token => "token",
            CompactAuthType::SSO => "sso",
            CompactAuthType::OIDC => "oidc",
            CompactAuthType::SAML => "saml",
            CompactAuthType::CAS => "cas",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_membership_size() {
        assert_eq!(CompactMembership::size(), 1);
    }

    #[test]
    fn test_compact_membership_roundtrip() {
        for variant in [
            CompactMembership::Join,
            CompactMembership::Leave,
            CompactMembership::Invite,
            CompactMembership::Ban,
            CompactMembership::Knock,
        ] {
            let u8_val = variant.to_u8();
            let back = CompactMembership::from_u8(u8_val).unwrap();
            assert_eq!(variant, back);
        }
    }

    #[test]
    fn test_compact_membership_from_str() {
        assert_eq!(CompactMembership::from_str("join"), Some(CompactMembership::Join));
        assert_eq!(CompactMembership::from_str("leave"), Some(CompactMembership::Leave));
        assert_eq!(CompactMembership::from_str("invalid"), None);
    }

    #[test]
    fn test_compact_presence_size() {
        assert_eq!(CompactPresence::size(), 1);
    }

    #[test]
    fn test_compact_presence_roundtrip() {
        for variant in [
            CompactPresence::Online,
            CompactPresence::Offline,
            CompactPresence::Unavailable,
        ] {
            let u8_val = variant.to_u8();
            let back = CompactPresence::from_u8(u8_val).unwrap();
            assert_eq!(variant, back);
        }
    }

    #[test]
    fn test_compact_device_type_from_user_agent() {
        assert_eq!(
            CompactDeviceType::from_user_agent("Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)"),
            CompactDeviceType::Mobile
        );
        assert_eq!(
            CompactDeviceType::from_user_agent("Mozilla/5.0 (Android 11; Mobile)"),
            CompactDeviceType::Mobile
        );
        assert_eq!(
            CompactDeviceType::from_user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/91.0"),
            CompactDeviceType::Web
        );
        assert_eq!(
            CompactDeviceType::from_user_agent("Electron/15.0.0"),
            CompactDeviceType::Desktop
        );
        assert_eq!(
            CompactDeviceType::from_user_agent("Unknown Client"),
            CompactDeviceType::Unknown
        );
    }

    #[test]
    fn test_compact_membership_conversion() {
        let original = crate::common::types::Membership::Join;
        let compact: CompactMembership = original.into();
        assert_eq!(compact, CompactMembership::Join);
        
        let back: crate::common::types::Membership = compact.into();
        assert!(matches!(back, crate::common::types::Membership::Join));
    }

    #[test]
    fn test_compact_presence_conversion() {
        let original = crate::common::types::Presence::Online;
        let compact: CompactPresence = original.into();
        assert_eq!(compact, CompactPresence::Online);
        
        let back: crate::common::types::Presence = compact.into();
        assert!(matches!(back, crate::common::types::Presence::Online));
    }

    #[test]
    fn test_compact_membership_default() {
        let default = CompactMembership::default();
        assert_eq!(default, CompactMembership::Leave);
    }

    #[test]
    fn test_compact_presence_default() {
        let default = CompactPresence::default();
        assert_eq!(default, CompactPresence::Offline);
    }

    #[test]
    fn test_compact_membership_serde() {
        let membership = CompactMembership::Join;
        let json = serde_json::to_string(&membership).unwrap();
        let deserialized: CompactMembership = serde_json::from_str(&json).unwrap();
        assert_eq!(membership, deserialized);
    }

    #[test]
    fn test_compact_presence_serde() {
        let presence = CompactPresence::Online;
        let json = serde_json::to_string(&presence).unwrap();
        let deserialized: CompactPresence = serde_json::from_str(&json).unwrap();
        assert_eq!(presence, deserialized);
    }
}
