use serde::Deserialize;

/// MSC4284: Policy Server configuration.
///
/// Allows configuring an external policy server to moderate rooms, users,
/// and content. When enabled, the homeserver consults the policy server
/// before allowing room creation, joins, and invites.
#[derive(Debug, Clone, Deserialize)]
pub struct PolicyServerConfig {
    /// Whether the policy server is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// The URL of the policy server.
    /// Example: "https://policy.example.com"
    pub endpoint: Option<String>,

    /// API key for authenticating with the policy server.
    pub api_key: Option<String>,

    /// Timeout in seconds for policy server requests.
    #[serde(default = "default_policy_server_timeout")]
    pub timeout_secs: u64,

    /// Whether to allow operations when the policy server is unreachable.
    /// If true, operations proceed on failure (fail-open).
    /// If false, operations are denied on failure (fail-closed).
    #[serde(default)]
    pub fail_open: bool,
}

impl Default for PolicyServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: None,
            api_key: None,
            timeout_secs: default_policy_server_timeout(),
            fail_open: true,
        }
    }
}

impl PolicyServerConfig {
    pub fn is_configured(&self) -> bool {
        self.enabled && self.endpoint.is_some()
    }
}

fn default_policy_server_timeout() -> u64 {
    5
}
