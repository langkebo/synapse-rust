use serde::Deserialize;

// ============================================================================
// SECTION: VoIP & Push Notifications
// ============================================================================

/// VoIP configuration.
///
/// Official Synapse configuration documentation: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#voip>
#[derive(Debug, Clone, Deserialize)]
/// Represents VoipConfig.
pub struct VoipConfig {
    /// TURN server URL list
    #[serde(default)]
    /// `turn_uris` field.
    pub turn_uris: Vec<String>,

    /// TURN shared secret (for generating temporary credentials)
    pub turn_shared_secret: Option<String>,

    /// TURN shared secret file path
    pub turn_shared_secret_path: Option<String>,

    /// TURN static username (if not using shared secret)
    pub turn_username: Option<String>,

    /// TURN static password (if not using shared secret)
    pub turn_password: Option<String>,

    /// TURN credential lifetime
    #[serde(default = "default_turn_user_lifetime")]
    /// `turn_user_lifetime` field.
    pub turn_user_lifetime: String,

    /// Whether to allow guests to use the TURN server
    #[serde(default = "default_turn_allow_guests")]
    /// `turn_allow_guests` field.
    pub turn_allow_guests: bool,

    /// STUN server URL list
    #[serde(default)]
    /// `stun_uris` field.
    pub stun_uris: Vec<String>,
}

impl Default for VoipConfig {
    fn default() -> Self {
        Self {
            turn_uris: Vec::new(),
            turn_shared_secret: None,
            turn_shared_secret_path: None,
            turn_username: None,
            turn_password: None,
            turn_user_lifetime: default_turn_user_lifetime(),
            turn_allow_guests: default_turn_allow_guests(),
            stun_uris: Vec::new(),
        }
    }
}

fn default_turn_user_lifetime() -> String {
    "1h".to_string()
}

fn default_turn_allow_guests() -> bool {
    true
}

impl VoipConfig {
    /// Returns true if enabled.
    pub fn is_enabled(&self) -> bool {
        !self.turn_uris.is_empty() || !self.stun_uris.is_empty()
    }

    /// Lifetimes the seconds.
    pub fn lifetime_seconds(&self) -> i64 {
        parse_duration(&self.turn_user_lifetime).unwrap_or(3600)
    }
}

/// Livekit SFU configuration.
///
/// NB: there used to be a `ws_url` field here. It was **never read** by any code path
/// (`rtc/transports` only returns ICE candidates), i.e. a config knob that looked
/// applied but did nothing — removed under AGENTS.md 铁律 1. Wire it back only
/// together with the SFU transport that consumes it.
#[derive(Debug, Clone, Default, Deserialize)]
/// Represents LivekitConfig.
pub struct LivekitConfig {
    /// `api_key` field.
    pub api_key: String,
    /// `api_secret` field.
    pub api_secret: String,
    /// `host` field.
    pub host: String,
}

fn parse_duration(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (num_part, unit) = if let Some(stripped) = s.strip_suffix('s') {
        (stripped, 1i64)
    } else if s.ends_with('m') && !s.ends_with("ms") {
        (s.strip_suffix('m')?, 60i64)
    } else if let Some(stripped) = s.strip_suffix('h') {
        (stripped, 3600i64)
    } else if let Some(stripped) = s.strip_suffix('d') {
        (stripped, 86400i64)
    } else if let Some(stripped) = s.strip_suffix('w') {
        (stripped, 604800i64)
    } else {
        (s, 1i64)
    };

    num_part.parse::<i64>().ok().map(|n| n * unit)
}

/// Push configuration.
///
/// Official Synapse configuration documentation: <https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#push>
#[derive(Debug, Clone, Deserialize, Default)]
/// Represents PushConfig.
pub struct PushConfig {
    /// Whether to enable push
    #[serde(default)]
    /// `enabled` field.
    pub enabled: bool,

    /// APNs configuration
    #[serde(default)]
    /// `apns` field.
    pub apns: Option<ApnsConfig>,

    /// FCM configuration
    #[serde(default)]
    /// `fcm` field.
    pub fcm: Option<FcmConfig>,

    /// Web Push configuration
    #[serde(default)]
    /// `web_push` field.
    pub web_push: Option<WebPushConfig>,

    /// Push gateway URL (for HTTP push)
    #[serde(default)]
    /// `push_gateway_url` field.
    pub push_gateway_url: Option<String>,
}

/// APNs push configuration marker.
///
/// The inner transport fields were never read by any code path (push delivery is
/// not wired to APNs yet); only the *presence* of this section matters
/// (`PushConfig::is_enabled`). Kept as an empty marker struct so existing YAML
/// that contains an `apns:` section still parses. Add fields back only together
/// with the APNs transport that consumes them.
#[derive(Debug, Clone, Deserialize)]
/// Represents ApnsConfig.
pub struct ApnsConfig {}

/// FCM push configuration marker.
///
/// See `ApnsConfig` for rationale: kept as an empty marker so presence-based
/// enablement keeps working and existing YAML still parses.
#[derive(Debug, Clone, Deserialize)]
/// Represents FcmConfig.
pub struct FcmConfig {}

/// Web Push configuration marker.
///
/// See `ApnsConfig` for rationale: kept as an empty marker so presence-based
/// enablement keeps working and existing YAML still parses.
#[derive(Debug, Clone, Deserialize)]
/// Represents WebPushConfig.
pub struct WebPushConfig {}

impl PushConfig {
    /// Returns true if enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
            && (self.fcm.is_some() || self.apns.is_some() || self.web_push.is_some() || self.push_gateway_url.is_some())
    }
}

fn default_ip_blacklist() -> Vec<String> {
    vec![
        "127.0.0.0/8".to_string(),
        "10.0.0.0/8".to_string(),
        "172.16.0.0/12".to_string(),
        "192.168.0.0/16".to_string(),
        "100.64.0.0/10".to_string(),
        "169.254.0.0/16".to_string(),
        "::1/128".to_string(),
        "fe80::/10".to_string(),
        "fc00::/7".to_string(),
    ]
}

/// URL preview configuration.
///
/// Only `ip_range_blacklist` is actually read by the URL preview fetch path; the
/// remaining knobs (enabled/spider/oembed/size/cache/ua/timeout/redirects) were
/// never wired and were removed. `url_blacklist` was also removed together with
/// its `UrlBlacklistRule` type.
#[derive(Debug, Clone, Deserialize)]
/// Represents UrlPreviewConfig.
pub struct UrlPreviewConfig {
    #[serde(default = "default_ip_blacklist")]
    /// `ip_range_blacklist` field.
    pub ip_range_blacklist: Vec<String>,
}

impl Default for UrlPreviewConfig {
    fn default() -> Self {
        Self { ip_range_blacklist: default_ip_blacklist() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_empty() {
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("   "), None);
    }

    #[test]
    fn parse_duration_seconds() {
        assert_eq!(parse_duration("0s"), Some(0));
        assert_eq!(parse_duration("30s"), Some(30));
        assert_eq!(parse_duration("3600s"), Some(3600));
    }

    #[test]
    fn parse_duration_minutes() {
        assert_eq!(parse_duration("0m"), Some(0));
        assert_eq!(parse_duration("5m"), Some(300));
        assert_eq!(parse_duration("90m"), Some(5400));
    }

    #[test]
    fn parse_duration_hours() {
        assert_eq!(parse_duration("1h"), Some(3600));
        assert_eq!(parse_duration("24h"), Some(86400));
    }

    #[test]
    fn parse_duration_days() {
        assert_eq!(parse_duration("1d"), Some(86400));
        assert_eq!(parse_duration("7d"), Some(604800));
    }

    #[test]
    fn parse_duration_weeks() {
        assert_eq!(parse_duration("1w"), Some(604800));
        assert_eq!(parse_duration("2w"), Some(1209600));
    }

    #[test]
    fn parse_duration_no_suffix_treated_as_seconds() {
        assert_eq!(parse_duration("3600"), Some(3600));
        assert_eq!(parse_duration("0"), Some(0));
    }

    #[test]
    fn parse_duration_invalid() {
        assert_eq!(parse_duration("abc"), None);
        assert_eq!(parse_duration("10x"), None);
        assert_eq!(parse_duration("-1h"), Some(-3600)); // negative is parseable
    }

    #[test]
    fn parse_duration_whitespace() {
        assert_eq!(parse_duration(" 1h "), Some(3600));
        assert_eq!(parse_duration("\t30s"), Some(30));
    }

    #[test]
    fn parse_duration_ms_not_treated_as_minutes() {
        // "10ms" must not be parsed as "10m" (600s); strip_suffix('s') yields "10m" which fails to parse
        assert_eq!(parse_duration("10ms"), None);
    }
}
