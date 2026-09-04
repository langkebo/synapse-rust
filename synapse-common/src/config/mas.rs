//! MSC3861: Matrix Authentication Service (MAS) configuration.
//!
//! When MAS is deployed alongside the homeserver, clients authenticate
//! directly with MAS using OIDC. The homeserver validates MAS-issued
//! access tokens (JWTs signed with RS256/ES256/EdDSA) and maps the OIDC
//! `sub` claim to a local Matrix user_id via `oidc_user_mapping`.
//!
//! This config gates the MAS integration. When `enabled = false` (the
//! default), the homeserver uses its built-in HS256 token validation and
//! no MAS REST API calls are made.

use educe::Educe;
use serde::Deserialize;

/// MSC3861: Matrix Authentication Service (MAS) configuration.
///
/// Defaults to disabled for backward compatibility. When `enabled = true`
/// and `issuer_url` is non-empty, `is_configured()` returns true and the
/// homeserver activates MAS token validation and the MAS REST client.
#[derive(Clone, Default, Deserialize, Educe)]
#[educe(Debug)]
pub struct MasConfig {
    /// Whether MAS integration is enabled. Defaults to `false`.
    #[serde(default)]
    pub enabled: bool,

    /// The MAS OIDC issuer URL (e.g. `https://mas.example.com`).
    /// Used both for JWKS discovery and as the base URL for the MAS admin
    /// REST API. Defaults to an empty string.
    #[serde(default)]
    pub issuer_url: String,

    /// The `client_id` the homeserver uses to identify itself to MAS.
    /// Defaults to an empty string.
    #[serde(default)]
    pub client_id: String,

    /// The `client_secret` paired with `client_id`. Defaults to an empty
    /// string.
    #[serde(default)]
    #[educe(Debug(ignore))]
    pub client_secret: String,

    /// Optional bearer token for the MAS admin REST API
    /// (`/admin/v1/...` endpoints). When `None`, admin REST calls are
    /// refused by the `MasRestClient`.
    #[serde(default)]
    #[educe(Debug(ignore))]
    pub admin_token: Option<String>,
}

impl MasConfig {
    /// Returns `true` when MAS integration is fully configured and should
    /// be activated. Requires both `enabled = true` AND a non-empty
    /// `issuer_url` so a partially-configured MAS section does not silently
    /// enable MAS token validation.
    pub fn is_configured(&self) -> bool {
        self.enabled && !self.issuer_url.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mas_config_default_is_disabled() {
        let cfg = MasConfig::default();
        assert!(!cfg.enabled, "MAS should default to disabled");
        assert!(cfg.issuer_url.is_empty(), "issuer_url should default to empty");
        assert!(cfg.client_id.is_empty(), "client_id should default to empty");
        assert!(cfg.client_secret.is_empty(), "client_secret should default to empty");
        assert!(cfg.admin_token.is_none(), "admin_token should default to None");
    }

    #[test]
    fn mas_config_deserialize_empty_yaml_uses_defaults() {
        // Backward compatibility: an empty `mas:` section (or missing field
        // when `#[serde(default)]` is applied at the parent) must deserialize
        // using defaults without error.
        let yaml = "{}\n";
        let cfg: MasConfig = serde_yaml::from_str(yaml).expect("empty YAML should deserialize with defaults");
        assert!(!cfg.enabled);
        assert!(cfg.issuer_url.is_empty());
        assert!(cfg.client_id.is_empty());
        assert!(cfg.client_secret.is_empty());
        assert!(cfg.admin_token.is_none());
    }

    #[test]
    fn mas_config_deserialize_explicit_values() {
        let yaml = "enabled: true\nissuer_url: https://mas.example.com\nclient_id: synapse-rust\nclient_secret: s3cret\nadmin_token: admin-tok\n";
        let cfg: MasConfig = serde_yaml::from_str(yaml).expect("explicit YAML should deserialize");
        assert!(cfg.enabled);
        assert_eq!(cfg.issuer_url, "https://mas.example.com");
        assert_eq!(cfg.client_id, "synapse-rust");
        assert_eq!(cfg.client_secret, "s3cret");
        assert_eq!(cfg.admin_token.as_deref(), Some("admin-tok"));
    }

    #[test]
    fn mas_config_clone_preserves_values() {
        let cfg = MasConfig {
            enabled: true,
            issuer_url: "https://mas.example.com".to_string(),
            client_id: "synapse-rust".to_string(),
            client_secret: "s3cret".to_string(),
            admin_token: Some("admin-tok".to_string()),
        };
        let cloned = cfg.clone();
        assert_eq!(cfg.enabled, cloned.enabled);
        assert_eq!(cfg.issuer_url, cloned.issuer_url);
        assert_eq!(cfg.client_id, cloned.client_id);
        assert_eq!(cfg.client_secret, cloned.client_secret);
        assert_eq!(cfg.admin_token, cloned.admin_token);
    }

    #[test]
    fn mas_config_debug_format_does_not_panic() {
        let cfg = MasConfig::default();
        let debug_str = format!("{cfg:?}");
        assert!(debug_str.contains("MasConfig"));
    }

    #[test]
    fn mas_config_is_configured_requires_enabled_and_issuer_url() {
        // is_configured() gates whether MAS code paths activate. It must
        // require both `enabled = true` AND a non-empty `issuer_url` so
        // that a partially-configured MAS section does not silently enable
        // MAS token validation.
        let disabled = MasConfig::default();
        assert!(!disabled.is_configured());

        let enabled_no_url = MasConfig { enabled: true, ..Default::default() };
        assert!(!enabled_no_url.is_configured());

        let enabled_with_url =
            MasConfig { enabled: true, issuer_url: "https://mas.example.com".to_string(), ..Default::default() };
        assert!(enabled_with_url.is_configured());
    }
}
