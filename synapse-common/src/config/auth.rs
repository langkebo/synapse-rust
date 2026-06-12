use serde::Deserialize;

// ============================================================================
// SECTION: Authentication (OIDC, SAML)
// ============================================================================

/// OpenID Connect configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct OidcConfig {
    #[serde(default)]
    pub enabled: bool,
    pub issuer: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    #[serde(default = "default_oidc_scopes")]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub attribute_mapping: OidcAttributeMapping,
    pub callback_url: Option<String>,
    #[serde(default)]
    pub allow_existing_users: bool,
    #[serde(default)]
    pub block_unknown_users: bool,
    #[serde(default)]
    pub user_mapping_provider: Option<String>,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub userinfo_endpoint: Option<String>,
    pub jwks_uri: Option<String>,
    #[serde(default = "default_oidc_timeout")]
    pub timeout: u64,
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            issuer: String::new(),
            client_id: String::new(),
            client_secret: None,
            scopes: default_oidc_scopes(),
            attribute_mapping: OidcAttributeMapping::default(),
            callback_url: None,
            allow_existing_users: false,
            block_unknown_users: false,
            user_mapping_provider: None,
            authorization_endpoint: None,
            token_endpoint: None,
            userinfo_endpoint: None,
            jwks_uri: None,
            timeout: default_oidc_timeout(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct OidcAttributeMapping {
    pub localpart: Option<String>,
    pub displayname: Option<String>,
    pub email: Option<String>,
}

fn default_oidc_scopes() -> Vec<String> {
    vec!["openid".to_string(), "profile".to_string(), "email".to_string()]
}

fn default_oidc_timeout() -> u64 {
    10
}

impl OidcConfig {
    pub fn is_enabled(&self) -> bool {
        self.enabled && !self.issuer.is_empty() && !self.client_id.is_empty()
    }
}

/// SAML 2.0 configuration.
///
/// Official Synapse configuration documentation: https://element-hq.github.io/synapse/latest/openid.html#saml
#[derive(Debug, Clone, Deserialize)]
pub struct SamlConfig {
    /// Whether to enable SAML authentication
    #[serde(default)]
    pub enabled: bool,

    /// SAML IdP metadata URL
    pub metadata_url: Option<String>,

    /// SAML IdP metadata XML (direct configuration)
    pub metadata_xml: Option<String>,

    /// SP entity ID
    #[serde(default = "default_saml_sp_entity_id")]
    pub sp_entity_id: String,

    /// SP ACS (Assertion Consumer Service) URL
    pub sp_acs_url: Option<String>,

    /// SP SLS (Single Logout Service) URL
    pub sp_sls_url: Option<String>,

    /// SP private key (PEM format)
    pub sp_private_key: Option<String>,

    /// SP private key file path
    pub sp_private_key_path: Option<String>,

    /// SP certificate (PEM format)
    pub sp_certificate: Option<String>,

    /// SP certificate file path
    pub sp_certificate_path: Option<String>,

    /// Attribute mapping configuration
    #[serde(default)]
    pub attribute_mapping: SamlAttributeMapping,

    /// NameID format
    #[serde(default = "default_saml_nameid_format")]
    pub nameid_format: String,

    /// Whether to allow existing users to log in
    #[serde(default = "default_saml_allow_existing_users")]
    pub allow_existing_users: bool,

    /// Whether to block unknown users
    #[serde(default)]
    pub block_unknown_users: bool,

    /// User ID template
    #[serde(default = "default_saml_user_id_template")]
    pub user_id_template: String,

    /// Whether to use NameID as user identifier
    #[serde(default)]
    pub use_name_id_for_user_id: bool,

    /// SAML request signing
    #[serde(default = "default_saml_sign_requests")]
    pub sign_requests: bool,

    /// SAML response signature verification
    #[serde(default = "default_saml_want_response_signed")]
    pub want_response_signed: bool,

    /// SAML assertion signature verification
    #[serde(default = "default_saml_want_assertions_signed")]
    pub want_assertions_signed: bool,

    /// SAML assertion encryption verification
    #[serde(default)]
    pub want_assertions_encrypted: bool,

    /// Authentication context class
    #[serde(default)]
    pub authn_context_class_ref: Option<String>,

    /// Session lifetime (seconds)
    #[serde(default = "default_saml_session_lifetime")]
    pub session_lifetime: u64,

    /// Metadata refresh interval (seconds)
    #[serde(default = "default_saml_metadata_refresh_interval")]
    pub metadata_refresh_interval: u64,

    /// Allowed IdP entity ID list
    #[serde(default)]
    pub allowed_idp_entity_ids: Vec<String>,

    /// Timeout (seconds)
    #[serde(default = "default_saml_timeout")]
    pub timeout: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SamlAttributeMapping {
    /// Username attribute
    pub uid: Option<String>,
    /// Display name attribute
    pub displayname: Option<String>,
    /// Email attribute
    pub email: Option<String>,
}

fn default_saml_sp_entity_id() -> String {
    "https://matrix.example.com".to_string()
}

fn default_saml_nameid_format() -> String {
    "urn:oasis:names:tc:SAML:2.0:nameid-format:persistent".to_string()
}

fn default_saml_allow_existing_users() -> bool {
    true
}

fn default_saml_user_id_template() -> String {
    "{uid}".to_string()
}

fn default_saml_sign_requests() -> bool {
    false
}

fn default_saml_want_response_signed() -> bool {
    true
}

fn default_saml_want_assertions_signed() -> bool {
    true
}

fn default_saml_session_lifetime() -> u64 {
    28800
}

fn default_saml_metadata_refresh_interval() -> u64 {
    3600
}

fn default_saml_timeout() -> u64 {
    10
}

impl Default for SamlConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            metadata_url: None,
            metadata_xml: None,
            sp_entity_id: default_saml_sp_entity_id(),
            sp_acs_url: None,
            sp_sls_url: None,
            sp_private_key: None,
            sp_private_key_path: None,
            sp_certificate: None,
            sp_certificate_path: None,
            attribute_mapping: SamlAttributeMapping::default(),
            nameid_format: default_saml_nameid_format(),
            allow_existing_users: default_saml_allow_existing_users(),
            block_unknown_users: false,
            user_id_template: default_saml_user_id_template(),
            use_name_id_for_user_id: false,
            sign_requests: default_saml_sign_requests(),
            want_response_signed: default_saml_want_response_signed(),
            want_assertions_signed: default_saml_want_assertions_signed(),
            want_assertions_encrypted: false,
            authn_context_class_ref: None,
            session_lifetime: default_saml_session_lifetime(),
            metadata_refresh_interval: default_saml_metadata_refresh_interval(),
            allowed_idp_entity_ids: Vec::new(),
            timeout: default_saml_timeout(),
        }
    }
}

impl SamlConfig {
    pub fn is_enabled(&self) -> bool {
        self.enabled && (self.metadata_url.is_some() || self.metadata_xml.is_some())
    }

    pub fn get_sp_acs_url(&self, server_name: &str) -> String {
        self.sp_acs_url
            .clone()
            .unwrap_or_else(|| format!("https://{server_name}/_matrix/client/r0/login/sso/redirect/saml"))
    }

    pub fn get_sp_sls_url(&self, server_name: &str) -> Option<String> {
        self.sp_sls_url.clone().or_else(|| Some(format!("https://{server_name}/_matrix/client/r0/logout/saml")))
    }
}
