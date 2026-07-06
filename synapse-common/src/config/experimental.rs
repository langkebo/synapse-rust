use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ExperimentalConfig {
    #[cfg(feature = "openclaw-routes")]
    #[serde(default = "default_true")]
    pub openclaw_routes_enabled: bool,

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
            msc4452_enabled: false,
            declare_private_extensions: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let cfg = ExperimentalConfig::default();
        assert!(!cfg.msc4452_enabled, "msc4452 should default to false");
        assert!(cfg.declare_private_extensions, "declare_private_extensions should default to true");
        #[cfg(feature = "openclaw-routes")]
        assert!(cfg.openclaw_routes_enabled, "openclaw_routes_enabled should default to true");
    }

    #[test]
    fn default_true_helper_returns_true() {
        assert!(default_true());
    }

    #[test]
    fn deserialize_empty_uses_defaults() {
        let yaml = "{}\n";
        let cfg: ExperimentalConfig = serde_yaml::from_str(yaml).expect("empty YAML should deserialize with defaults");
        assert!(!cfg.msc4452_enabled);
        assert!(cfg.declare_private_extensions);
        #[cfg(feature = "openclaw-routes")]
        assert!(cfg.openclaw_routes_enabled);
    }

    #[test]
    fn deserialize_explicit_values_override_defaults() {
        let yaml = "msc4452_enabled: true\ndeclare_private_extensions: false\n";
        let cfg: ExperimentalConfig = serde_yaml::from_str(yaml).expect("explicit YAML should override defaults");
        assert!(cfg.msc4452_enabled);
        assert!(!cfg.declare_private_extensions);
    }

    #[test]
    fn deserialize_openclaw_field_when_feature_enabled() {
        // openclaw_routes_enabled is only present when the openclaw-routes feature is enabled.
        #[cfg(feature = "openclaw-routes")]
        {
            let yaml = "openclaw_routes_enabled: false\n";
            let cfg: ExperimentalConfig =
                serde_yaml::from_str(yaml).expect("openclaw field should deserialize when feature is enabled");
            assert!(!cfg.openclaw_routes_enabled);
        }
        #[cfg(not(feature = "openclaw-routes"))]
        {
            // When feature is disabled, the field does not exist; the default impl still works.
            let cfg = ExperimentalConfig::default();
            assert!(!cfg.msc4452_enabled);
        }
    }

    #[test]
    fn clone_preserves_values() {
        let cfg = ExperimentalConfig {
            #[cfg(feature = "openclaw-routes")]
            openclaw_routes_enabled: false,
            msc4452_enabled: true,
            declare_private_extensions: false,
        };
        let cloned = cfg.clone();
        assert_eq!(cfg.msc4452_enabled, cloned.msc4452_enabled);
        assert_eq!(cfg.declare_private_extensions, cloned.declare_private_extensions);
        #[cfg(feature = "openclaw-routes")]
        assert_eq!(cfg.openclaw_routes_enabled, cloned.openclaw_routes_enabled);
    }

    #[test]
    fn debug_format_does_not_panic() {
        let cfg = ExperimentalConfig::default();
        let debug_str = format!("{cfg:?}");
        assert!(debug_str.contains("ExperimentalConfig"));
    }
}
