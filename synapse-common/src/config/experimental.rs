use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
/// Represents ExperimentalConfig.
pub struct ExperimentalConfig {
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
    /// `msc4452_enabled` field.
    pub msc4452_enabled: bool,

    /// MSC3720: Account status endpoint.
    ///
    /// When enabled, the `org.matrix.msc3720.account_status` capability is
    /// declared in `GET /_matrix/client/v3/capabilities`, and the
    /// `POST /_matrix/client/unstable/org.matrix.msc3720/account_status` and
    /// `POST /_matrix/federation/unstable/org.matrix.msc3720/account_status`
    /// endpoints serve account statuses.
    ///
    /// Mirrors upstream Synapse's `experimental.msc3720_enabled` (default
    /// false). When disabled the endpoints fail closed with 403
    /// `M_FORBIDDEN`, matching the MSC's "server administrators might not want
    /// to disclose too much information about their users" security
    /// consideration.
    #[serde(default)]
    /// `msc3720_enabled` field.
    pub msc3720_enabled: bool,

    /// Controls whether private `io.hula.*` extensions (friends,
    /// burn_after_read, voice_extended) are declared in the authenticated
    /// `/capabilities` surface.
    ///
    /// Set to `false` when deploying behind stock Element Web to suppress
    /// capability declarations for features that have no corresponding UI in
    /// the default client. Defaults to `true` for backward compatibility.
    #[serde(default = "default_true")]
    /// `declare_private_extensions` field.
    pub declare_private_extensions: bool,

    /// MSC4210: stop serving the **legacy** mention push rules.
    ///
    /// When enabled (the default, matching upstream Synapse and Matrix v1.17),
    /// these three deprecated rules are hidden from every served rule set:
    ///
    /// - `.m.rule.contains_display_name`
    /// - `.m.rule.contains_user_name`
    /// - `.m.rule.roomnotif`
    ///
    /// The intentional-mention rules (`.m.rule.is_user_mention` and
    /// `.m.rule.is_room_mention`) are **not** affected — MSC4210 removes
    /// plaintext-matching rules only.
    ///
    /// ⚠️ This is a client-visible behaviour change: clients that rely on the
    /// server-served legacy rules to turn a display-name or `@room` hit into a
    /// notification will stop being notified for plaintext mentions. The MSC
    /// itself calls that out as a known trade-off. Set to `false` to keep the
    /// pre-v1.17 rule set.
    #[serde(default = "default_true")]
    /// `msc4210_enabled` field.
    pub msc4210_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ExperimentalConfig {
    fn default() -> Self {
        Self { msc4452_enabled: false, msc3720_enabled: false, declare_private_extensions: true, msc4210_enabled: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let cfg = ExperimentalConfig::default();
        assert!(!cfg.msc4452_enabled, "msc4452 should default to false");
        assert!(!cfg.msc3720_enabled, "msc3720 should default to false");
        assert!(cfg.declare_private_extensions, "declare_private_extensions should default to true");
        assert!(cfg.msc4210_enabled, "msc4210 should default to true (Matrix v1.17 removed the legacy rules)");
    }

    #[test]
    fn default_true_helper_returns_true() {
        assert!(default_true());
    }

    #[test]
    fn deserialize_empty_uses_defaults() {
        let yaml = "{}\n";
        let cfg: ExperimentalConfig =
            serde_norway::from_str(yaml).expect("empty YAML should deserialize with defaults");
        assert!(!cfg.msc4452_enabled);
        assert!(!cfg.msc3720_enabled);
        assert!(cfg.declare_private_extensions);
        assert!(cfg.msc4210_enabled);
    }

    #[test]
    fn deserialize_explicit_values_override_defaults() {
        let yaml =
            "msc4452_enabled: true\nmsc3720_enabled: true\ndeclare_private_extensions: false\nmsc4210_enabled: false\n";
        let cfg: ExperimentalConfig = serde_norway::from_str(yaml).expect("explicit YAML should override defaults");
        assert!(cfg.msc4452_enabled);
        assert!(cfg.msc3720_enabled);
        assert!(!cfg.declare_private_extensions);
        assert!(!cfg.msc4210_enabled, "an explicit false must survive deserialization");
    }

    #[test]
    fn clone_preserves_values() {
        let cfg = ExperimentalConfig {
            msc4452_enabled: true,
            msc3720_enabled: false,
            declare_private_extensions: false,
            msc4210_enabled: false,
        };
        let cloned = cfg.clone();
        assert_eq!(cfg.msc4452_enabled, cloned.msc4452_enabled);
        assert_eq!(cfg.msc3720_enabled, cloned.msc3720_enabled);
        assert_eq!(cfg.declare_private_extensions, cloned.declare_private_extensions);
        assert_eq!(cfg.msc4210_enabled, cloned.msc4210_enabled);
    }

    #[test]
    fn debug_format_does_not_panic() {
        let cfg = ExperimentalConfig::default();
        let debug_str = format!("{cfg:?}");
        assert!(debug_str.contains("ExperimentalConfig"));
    }
}
