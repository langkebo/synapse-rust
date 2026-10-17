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

    /// MSC4075: MatrixRTC invites and notifications.
    ///
    /// When enabled, the two default override push rules added by upstream
    /// Synapse v1.163.0 (#20227) are served to clients:
    ///
    /// - `.org.matrix.msc4075.rule.rtc.invite_for_me`
    /// - `.org.matrix.msc4075.rule.rtc.invite_for_room`
    ///
    /// Both act with `notify` + `{"set_tweak": "sound", "value": "ring"}`, so a
    /// client that receives an `org.matrix.msc4075.rtc.notification` event it was
    /// mentioned by starts ringing. They are placed **immediately before**
    /// `.m.rule.is_user_mention` / `.m.rule.is_room_mention`: override rules are
    /// first-match-wins, so the generic mention rules would otherwise mask them.
    ///
    /// **Default `false`, matching upstream** (`experimental.msc4075_enabled`
    /// defaults to `False` there too, since the MSC is still unstable). Set to
    /// `true` to actually deliver RTC call invitations and ringing to clients.
    ///
    /// ⚠️ Nothing else changes when this is off: a client that never receives a
    /// `org.matrix.msc4075.rtc.notification` event sees no behavioural
    /// difference, which is why the flag is safe to leave at the default.
    #[serde(default)]
    /// `msc4075_enabled` field.
    pub msc4075_enabled: bool,

    /// MSC4512: Application service proxying.
    ///
    /// When enabled, an application service that declares a `proxy_prefix`
    /// (and matching `proxy_url`) in its registration file has requests under
    /// `/_matrix/{client,federation}/<version>/<proxy_prefix>/...` forwarded to
    /// that URL — see `synapse-services::application_service::proxy`.
    ///
    /// **Default `false`, matching upstream** (`experimental.msc4512_enabled`
    /// defaults to `False` there too, since the MSC is still unstable). While
    /// disabled the proxy surface is simply absent, so no appservice can claim
    /// a prefix even if one is present in its registration.
    #[serde(default)]
    /// `msc4512_enabled` field.
    pub msc4512_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ExperimentalConfig {
    fn default() -> Self {
        Self {
            msc4452_enabled: false,
            msc3720_enabled: false,
            declare_private_extensions: true,
            msc4210_enabled: true,
            msc4075_enabled: false,
            msc4512_enabled: false,
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
        assert!(!cfg.msc3720_enabled, "msc3720 should default to false");
        assert!(cfg.declare_private_extensions, "declare_private_extensions should default to true");
        assert!(cfg.msc4210_enabled, "msc4210 should default to true (Matrix v1.17 removed the legacy rules)");
        assert!(
            !cfg.msc4075_enabled,
            "msc4075 should default to false — same as upstream's experimental.msc4075_enabled"
        );
        assert!(
            !cfg.msc4512_enabled,
            "msc4512 should default to false — same as upstream's experimental.msc4512_enabled"
        );
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
        assert!(!cfg.msc4075_enabled, "a missing msc4075_enabled key must fall back to false");
        assert!(!cfg.msc4512_enabled, "a missing msc4512_enabled key must fall back to false");
    }

    #[test]
    fn deserialize_explicit_values_override_defaults() {
        let yaml = "msc4452_enabled: true\nmsc3720_enabled: true\ndeclare_private_extensions: false\nmsc4210_enabled: false\nmsc4075_enabled: true\nmsc4512_enabled: true\n";
        let cfg: ExperimentalConfig = serde_norway::from_str(yaml).expect("explicit YAML should override defaults");
        assert!(cfg.msc4452_enabled);
        assert!(cfg.msc3720_enabled);
        assert!(!cfg.declare_private_extensions);
        assert!(!cfg.msc4210_enabled, "an explicit false must survive deserialization");
        assert!(
            cfg.msc4075_enabled,
            "an explicit true must survive deserialization — otherwise the RTC rules could never be switched on"
        );
        assert!(
            cfg.msc4512_enabled,
            "an explicit true must survive deserialization — otherwise the appservice proxy could never be switched on"
        );
    }

    #[test]
    fn clone_preserves_values() {
        let cfg = ExperimentalConfig {
            msc4452_enabled: true,
            msc3720_enabled: false,
            declare_private_extensions: false,
            msc4210_enabled: false,
            msc4075_enabled: true,
            msc4512_enabled: true,
        };
        let cloned = cfg.clone();
        assert_eq!(cfg.msc4452_enabled, cloned.msc4452_enabled);
        assert_eq!(cfg.msc3720_enabled, cloned.msc3720_enabled);
        assert_eq!(cfg.declare_private_extensions, cloned.declare_private_extensions);
        assert_eq!(cfg.msc4210_enabled, cloned.msc4210_enabled);
        assert_eq!(cfg.msc4075_enabled, cloned.msc4075_enabled);
        assert_eq!(cfg.msc4512_enabled, cloned.msc4512_enabled);
    }

    #[test]
    fn debug_format_does_not_panic() {
        let cfg = ExperimentalConfig::default();
        let debug_str = format!("{cfg:?}");
        assert!(debug_str.contains("ExperimentalConfig"));
    }
}
