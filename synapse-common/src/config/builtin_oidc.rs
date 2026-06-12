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
    /// Persistence path for the built-in OIDC Provider's RSA signing key (PKCS#8 PEM).
    /// If empty, a temporary key is generated in-process; old tokens become invalid after process restart.
    #[serde(default)]
    pub signing_key_path: Option<std::path::PathBuf>,
}

fn default_builtin_oidc_client_id() -> String {
    "builtin-oidc-client".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct BuiltinOidcUser {
    pub id: String,
    pub username: String,
    /// Plaintext password (development/testing only). Use password_hash in production.
    #[serde(default)]
    pub password: Option<String>,
    /// Argon2 PHC string. Takes priority over password.
    #[serde(default)]
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
            signing_key_path: None,
        }
    }
}

impl BuiltinOidcConfig {
    pub fn is_enabled(&self) -> bool {
        self.enabled && !self.issuer.is_empty() && !self.users.is_empty()
    }
}
