use serde::Deserialize;

fn default_trusted_identity_servers() -> Vec<String> {
    vec!["vector.im".to_string(), "matrix.org".to_string()]
}

#[derive(Debug, Clone, Deserialize)]
pub struct IdentityConfig {
    /// 信任的 Identity Server 列表
    #[serde(default = "default_trusted_identity_servers")]
    pub trusted_servers: Vec<String>,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self { trusted_servers: default_trusted_identity_servers() }
    }
}