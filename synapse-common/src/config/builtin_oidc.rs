use educe::Educe;
use serde::Deserialize;

/// 服务器配置结构。
///
/// 内置 OIDC Provider 配置
#[derive(Debug, Clone, Deserialize)]
pub struct BuiltinOidcConfig {
    #[serde(default)]
    pub enabled: bool,
    pub issuer: String,
    #[serde(default = "default_builtin_oidc_client_id")]
    pub client_id: String,
    #[serde(default)]
    pub allow_redirect_uris: Vec<String>,
    #[serde(default)]
    pub allow_client_ids: Vec<String>,
    #[serde(default)]
    pub users: Vec<BuiltinOidcUser>,
    /// SEC-03: 是否允许用户使用明文密码登录。默认 false（生产拒绝），
    /// 仅开发/测试环境显式开启；开启后仍会打 warn 日志。
    #[serde(default)]
    pub allow_plaintext_passwords: bool,
    /// Persistence path for the built-in OIDC Provider's RSA signing key (PKCS#8 PEM).
    /// If empty, a temporary key is generated in-process; old tokens become invalid after process restart.
    #[serde(default)]
    pub signing_key_path: Option<std::path::PathBuf>,
    /// Persistence path for the built-in OIDC Provider's P-256 EC signing key (PKCS#8 PEM)
    /// used to sign ES256 tokens (RFC 7518 §3.4).
    /// If empty, an ephemeral EC key is generated in-process (default for dev/test).
    /// Note: EC and RSA keys are independent — both are exposed in the JWKS endpoint.
    #[serde(default)]
    pub signing_key_ec_path: Option<std::path::PathBuf>,
}

fn default_builtin_oidc_client_id() -> String {
    "builtin-oidc-client".to_string()
}

#[derive(Clone, Deserialize, Educe)]
#[educe(Debug)]
pub struct BuiltinOidcUser {
    pub id: String,
    pub username: String,
    /// Plaintext password (development/testing only). Use password_hash in production.
    #[serde(default)]
    #[educe(Debug(ignore))]
    pub password: Option<String>,
    /// Argon2 PHC string. Takes priority over password.
    #[serde(default)]
    #[educe(Debug(ignore))]
    pub password_hash: Option<String>,
    pub email: String,
    pub displayname: Option<String>,
}

impl Default for BuiltinOidcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            issuer: String::new(),
            client_id: default_builtin_oidc_client_id(),
            allow_redirect_uris: vec![],
            allow_client_ids: vec![],
            users: vec![],
            allow_plaintext_passwords: false,
            signing_key_path: None,
            signing_key_ec_path: None,
        }
    }
}

impl BuiltinOidcConfig {
    pub fn is_enabled(&self) -> bool {
        self.enabled && !self.issuer.is_empty() && !self.users.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_oidc_config_default() {
        let config = BuiltinOidcConfig::default();
        assert!(!config.enabled);
        assert!(config.issuer.is_empty());
        assert_eq!(config.client_id, "builtin-oidc-client");
        assert!(config.allow_redirect_uris.is_empty());
        assert!(config.allow_client_ids.is_empty());
        assert!(config.users.is_empty());
        assert!(config.signing_key_path.is_none());
        assert!(config.signing_key_ec_path.is_none());
    }

    #[test]
    fn test_builtin_oidc_config_is_enabled_disabled() {
        let config = BuiltinOidcConfig::default();
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_builtin_oidc_config_is_enabled_no_issuer() {
        let config = BuiltinOidcConfig { enabled: true, issuer: String::new(), ..Default::default() };
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_builtin_oidc_config_is_enabled_no_users() {
        let config = BuiltinOidcConfig {
            enabled: true,
            issuer: "https://example.com".to_string(),
            users: vec![],
            ..Default::default()
        };
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_builtin_oidc_config_is_enabled_with_users() {
        let user = BuiltinOidcUser {
            id: "user1".to_string(),
            username: "testuser".to_string(),
            password: None,
            password_hash: None,
            email: "test@example.com".to_string(),
            displayname: None,
        };
        let config = BuiltinOidcConfig {
            enabled: true,
            issuer: "https://example.com".to_string(),
            users: vec![user],
            ..Default::default()
        };
        assert!(config.is_enabled());
    }

    #[test]
    fn test_builtin_oidc_user_creation() {
        let user = BuiltinOidcUser {
            id: "user1".to_string(),
            username: "testuser".to_string(),
            password: Some("password123".to_string()),
            password_hash: None,
            email: "test@example.com".to_string(),
            displayname: Some("Test User".to_string()),
        };
        assert_eq!(user.id, "user1");
        assert_eq!(user.username, "testuser");
        assert_eq!(user.password, Some("password123".to_string()));
        assert_eq!(user.email, "test@example.com");
        assert_eq!(user.displayname, Some("Test User".to_string()));
    }

    #[test]
    fn test_default_builtin_oidc_client_id() {
        assert_eq!(default_builtin_oidc_client_id(), "builtin-oidc-client");
    }
}
