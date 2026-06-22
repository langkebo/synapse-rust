use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ExperimentalConfig {
    #[cfg(feature = "openclaw-routes")]
    #[serde(default)]
    pub openclaw_routes_enabled: bool,

    #[serde(default)]
    pub msc3814_enabled: bool,

    /// MSC4452: Preview URL capabilities API.
    ///
    /// When enabled, the `io.element.msc4452.preview_url` capability is
    /// declared in `GET /_matrix/client/v3/capabilities`, and the
    /// `GET /_matrix/media/v3/preview_url` endpoint enforces the capability
    /// (returning 403 when the capability is disabled).
    ///
    /// This is a capability-driven feature gate, as introduced in Synapse
    /// v1.154 (#19715).
    #[serde(default)]
    pub msc4452_enabled: bool,

    /// Controls whether private `io.hula.*` extensions (friends,
    /// burn_after_read, voice_extended) are declared in the authenticated
    /// `/capabilities` surface.
    ///
    /// Set to `false` when deploying behind stock Element Web to suppress
    /// capability declarations for features that have no corresponding UI in
    /// the default client. Defaults to `true` for backward compatibility.
    #[serde(default = "default_true")]
    pub declare_private_extensions: bool,
}

fn default_true() -> bool {
    true
}

#[allow(clippy::derivable_impls)]
impl Default for ExperimentalConfig {
    fn default() -> Self {
        Self {
            #[cfg(feature = "openclaw-routes")]
            openclaw_routes_enabled: true,
            msc3814_enabled: false,
            msc4452_enabled: false,
            declare_private_extensions: true,
        }
    }
}
