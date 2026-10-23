//! Push notification configuration sub-module.
//!
//! This file is part of the M-2 refactor that splits the previous
//! 4081-line `common/config/mod.rs` god file into focused sub-modules.

use serde::Deserialize;

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
