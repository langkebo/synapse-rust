use serde::Deserialize;

// ============================================================================
// SECTION: VoIP & Push Notifications
// ============================================================================

/// VoIP configuration.
///
/// Official Synapse configuration documentation: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#voip
#[derive(Debug, Clone, Deserialize)]
pub struct VoipConfig {
    /// TURN server URL list
    #[serde(default)]
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
    pub turn_user_lifetime: String,

    /// Whether to allow guests to use the TURN server
    #[serde(default = "default_turn_allow_guests")]
    pub turn_allow_guests: bool,

    /// STUN server URL list
    #[serde(default)]
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
    pub fn is_enabled(&self) -> bool {
        !self.turn_uris.is_empty() || !self.stun_uris.is_empty()
    }

    pub fn lifetime_seconds(&self) -> i64 {
        parse_duration(&self.turn_user_lifetime).unwrap_or(3600)
    }
}

/// Livekit SFU configuration.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LivekitConfig {
    pub api_key: String,
    pub api_secret: String,
    pub host: String,
    pub ws_url: Option<String>,
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
/// Official Synapse configuration documentation: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#push
#[derive(Debug, Clone, Deserialize)]
pub struct PushConfig {
    /// Whether to enable push
    #[serde(default)]
    pub enabled: bool,

    /// Group unread counts by room
    #[serde(default = "default_group_unread")]
    pub group_unread_count_by_room: bool,

    /// Whether to include message content
    #[serde(default)]
    pub include_content: bool,

    /// Application ID
    pub app_id: Option<String>,

    /// APNs configuration
    #[serde(default)]
    pub apns: Option<ApnsConfig>,

    /// FCM configuration
    #[serde(default)]
    pub fcm: Option<FcmConfig>,

    /// Web Push configuration
    #[serde(default)]
    pub web_push: Option<WebPushConfig>,

    /// Push gateway URL (for HTTP push)
    #[serde(default)]
    pub push_gateway_url: Option<String>,

    /// Push retry count
    #[serde(default = "default_push_retry_count")]
    pub retry_count: u32,

    /// Push timeout (seconds)
    #[serde(default = "default_push_timeout")]
    pub timeout: u64,
}

impl Default for PushConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            group_unread_count_by_room: true,
            include_content: false,
            app_id: None,
            apns: None,
            fcm: None,
            web_push: None,
            push_gateway_url: None,
            retry_count: default_push_retry_count(),
            timeout: default_push_timeout(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApnsConfig {
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
    pub topic: String,
    #[serde(default = "default_apns_production")]
    pub production: bool,
    pub key_id: Option<String>,
    pub team_id: Option<String>,
    pub private_key_path: Option<String>,
    /// APNs endpoint URL. Defaults to `https://api.push.apple.com` (production)
    /// or `https://api.sandbox.push.apple.com` (sandbox).
    #[serde(default)]
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FcmConfig {
    pub api_key: Option<String>,
    pub project_id: Option<String>,
    pub service_account_file: Option<String>,
    /// FCM endpoint URL. Defaults to `https://fcm.googleapis.com/fcm/send`.
    #[serde(default)]
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebPushConfig {
    pub vapid_public_key: String,
    pub vapid_private_key: String,
    pub subject: String,
    /// Default gateway endpoint for WebPush relay service.
    /// Individual subscription endpoints take precedence at send time.
    #[serde(default)]
    pub gateway_endpoint: Option<String>,
}

fn default_group_unread() -> bool {
    true
}

fn default_push_retry_count() -> u32 {
    3
}

fn default_push_timeout() -> u64 {
    10
}

fn default_apns_production() -> bool {
    true
}

impl PushConfig {
    pub fn is_enabled(&self) -> bool {
        self.enabled
            && (self.fcm.is_some() || self.apns.is_some() || self.web_push.is_some() || self.push_gateway_url.is_some())
    }
}

fn default_spider_enabled() -> bool {
    true
}
fn default_max_spider_size() -> String {
    "10M".to_string()
}
fn default_preview_cache_duration() -> u64 {
    86400
}
fn default_user_agent() -> String {
    "Synapse-Rust/0.1.0 (Matrix Homeserver)".to_string()
}
fn default_preview_timeout() -> u64 {
    10
}
fn default_max_redirects() -> u32 {
    5
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

fn parse_size(s: &str) -> Option<usize> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (num_part, multiplier) = if s.ends_with('K') || s.ends_with('k') {
        (&s[..s.len() - 1], 1024usize)
    } else if s.ends_with('M') || s.ends_with('m') {
        (&s[..s.len() - 1], 1024 * 1024)
    } else if s.ends_with('G') || s.ends_with('g') {
        (&s[..s.len() - 1], 1024 * 1024 * 1024)
    } else {
        (s, 1usize)
    };
    num_part.parse::<usize>().ok().map(|n| n * multiplier)
}

/// URL preview configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct UrlPreviewConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_ip_blacklist")]
    pub ip_range_blacklist: Vec<String>,
    #[serde(default)]
    pub ip_range_whitelist: Vec<String>,
    #[serde(default)]
    pub url_blacklist: Vec<UrlBlacklistRule>,
    #[serde(default = "default_spider_enabled")]
    pub spider_enabled: bool,
    #[serde(default)]
    pub oembed_enabled: bool,
    #[serde(default = "default_max_spider_size")]
    pub max_spider_size: String,
    #[serde(default = "default_preview_cache_duration")]
    pub cache_duration: u64,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_preview_timeout")]
    pub timeout: u64,
    #[serde(default = "default_max_redirects")]
    pub max_redirects: u32,
}

impl Default for UrlPreviewConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ip_range_blacklist: default_ip_blacklist(),
            ip_range_whitelist: Vec::new(),
            url_blacklist: Vec::new(),
            spider_enabled: true,
            oembed_enabled: false,
            max_spider_size: default_max_spider_size(),
            cache_duration: default_preview_cache_duration(),
            user_agent: default_user_agent(),
            timeout: default_preview_timeout(),
            max_redirects: default_max_redirects(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UrlBlacklistRule {
    pub domain: Option<String>,
    pub regex: Option<String>,
}

impl UrlPreviewConfig {
    pub fn max_spider_size_bytes(&self) -> usize {
        parse_size(&self.max_spider_size).unwrap_or(10 * 1024 * 1024)
    }
}