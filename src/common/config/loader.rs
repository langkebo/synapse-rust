//! Configuration loading and environment variable resolution.
//!
//! Handles loading `Config` from file + environment, and resolving
//! `${ENV_VAR}` placeholders in config values.

use config::Config as ConfigBuilder;
use regex::Regex;
use std::path::PathBuf;

use super::Config;

impl Config {
    /// Load configuration from file (`SYNAPSE_CONFIG_PATH`) and environment overrides.
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = std::env::var("SYNAPSE_CONFIG_PATH").unwrap_or_else(|_| "./homeserver.yaml".to_string());

        tracing::info!("Loading configuration from: {}", config_path);

        let config = ConfigBuilder::builder()
            .add_source(config::File::with_name(&config_path))
            .add_source(config::Environment::with_prefix("SYNAPSE").separator("__"))
            .build()?;

        let mut config_values: Self = config.try_deserialize()?;

        tracing::info!("Configuration loaded, resolving environment variables...");
        tracing::debug!(
            "Before resolution - federation.signing_key: [REDACTED] ({} chars)",
            config_values.federation.signing_key.as_ref().map_or(0, |k| k.len())
        );
        tracing::debug!(
            "Before resolution - security.secret: [REDACTED] ({} chars)",
            config_values.security.secret.len()
        );

        config_values
            .resolve_env_variables()
            .map_err(|e| format!("Failed to resolve environment variables: {e}"))?;

        config_values
            .validate()
            .map_err(|e| format!("Configuration validation failed: {e}"))?;

        tracing::info!("Environment variables resolved successfully");
        tracing::debug!(
            "After resolution - federation.signing_key: [REDACTED] ({} chars)",
            config_values.federation.signing_key.as_ref().map_or(0, |k| k.len())
        );
        tracing::debug!(
            "After resolution - security.secret: [REDACTED] ({} chars)",
            config_values.security.secret.len()
        );

        Ok(config_values)
    }

    /// Resolve `${ENV_VAR}`, `${ENV_VAR:-default}`, `${ENV_VAR:=assign}`,
    /// and `${ENV_VAR:?error}` placeholders in all config fields.
    pub(crate) fn resolve_env_variables(&mut self) -> Result<(), String> {
        self.server.name = resolve_env_in_string(&self.server.name)?;
        self.server.host = resolve_env_in_string(&self.server.host)?;
        self.server.public_baseurl =
            self.server.public_baseurl.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.signing_key_path =
            self.server.signing_key_path.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.macaroon_secret_key =
            self.server.macaroon_secret_key.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.form_secret =
            self.server.form_secret.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.server_name =
            self.server.server_name.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.registration_shared_secret =
            self.server.registration_shared_secret.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.admin_contact =
            self.server.admin_contact.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.user_agent_suffix =
            self.server.user_agent_suffix.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.web_client_location =
            self.server.web_client_location.take().map(|v| resolve_env_in_string(&v)).transpose()?;

        self.database.host = resolve_env_in_string(&self.database.host)?;
        self.database.username = resolve_env_in_string(&self.database.username)?;
        self.database.password = resolve_env_in_string(&self.database.password)?;
        self.database.name = resolve_env_in_string(&self.database.name)?;

        self.redis.host = resolve_env_in_string(&self.redis.host)?;
        self.redis.password = self.redis.password.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.redis.key_prefix = resolve_env_in_string(&self.redis.key_prefix)?;

        self.logging.level = resolve_env_in_string(&self.logging.level)?;
        self.logging.format = resolve_env_in_string(&self.logging.format)?;
        self.logging.log_file = self.logging.log_file.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.logging.log_dir = self.logging.log_dir.take().map(|v| resolve_env_in_string(&v)).transpose()?;

        self.federation.server_name = resolve_env_in_string(&self.federation.server_name)?;
        self.federation.signing_key =
            self.federation.signing_key.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.federation.key_id =
            self.federation.key_id.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.federation.signing_key_master_key = self
            .federation
            .signing_key_master_key
            .take()
            .map(|v| resolve_env_in_string(&v))
            .transpose()?;
        self.federation.ca_file = self
            .federation
            .ca_file
            .take()
            .map(|v| resolve_env_in_string(&v.to_string_lossy()).map(PathBuf::from))
            .transpose()?;
        self.federation.client_ca_file = self
            .federation
            .client_ca_file
            .take()
            .map(|v| resolve_env_in_string(&v.to_string_lossy()).map(PathBuf::from))
            .transpose()?;

        for server in &mut self.federation.trusted_key_servers {
            server.server_name = resolve_env_in_string(&server.server_name)?;
        }

        self.security.secret = resolve_env_in_string(&self.security.secret)?;
        self.security.admin_mfa_shared_secret = resolve_env_in_string(&self.security.admin_mfa_shared_secret)?;

        self.search.elasticsearch_url = resolve_env_in_string(&self.search.elasticsearch_url)?;

        if self.smtp.enabled {
            self.smtp.host = resolve_env_in_string(&self.smtp.host)?;
            self.smtp.username = resolve_env_in_string(&self.smtp.username)?;
            self.smtp.password = resolve_env_in_string(&self.smtp.password)?;
            self.smtp.from = resolve_env_in_string(&self.smtp.from)?;
        }

        if self.oidc.enabled {
            self.oidc.issuer = resolve_env_in_string(&self.oidc.issuer)?;
            self.oidc.client_id = resolve_env_in_string(&self.oidc.client_id)?;
            self.oidc.client_secret =
                self.oidc.client_secret.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        }

        if self.saml.enabled {
            self.saml.metadata_url =
                self.saml.metadata_url.take().map(|v| resolve_env_in_string(&v)).transpose()?;
            self.saml.sp_entity_id = resolve_env_in_string(&self.saml.sp_entity_id)?;
        }

        self.admin_registration.shared_secret = resolve_env_in_string(&self.admin_registration.shared_secret)?;
        self.admin_registration.ip_whitelist = self
            .admin_registration
            .ip_whitelist
            .iter()
            .map(|value| resolve_env_in_string(value))
            .collect::<Result<Vec<_>, _>>()?;
        self.admin_registration.approval_tokens = self
            .admin_registration
            .approval_tokens
            .iter()
            .map(|value| resolve_env_in_string(value))
            .collect::<Result<Vec<_>, _>>()?;

        if self.voip.is_enabled() {
            self.voip.turn_shared_secret =
                self.voip.turn_shared_secret.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        }

        if self.push.is_enabled() {
            self.push.push_gateway_url =
                self.push.push_gateway_url.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        }

        Ok(())
    }
}

/// Resolve `${ENV_VAR}` placeholders in a string value.
///
/// Supports four syntaxes:
/// - `${VAR}` — replace with env var value, or empty string if unset
/// - `${VAR:-default}` — use default if env var is unset
/// - `${VAR:=assign}` — use default if env var is unset (deprecated, warns)
/// - `${VAR:?error}` — error if env var is unset
#[allow(clippy::expect_used)]
fn resolve_env_in_string(value: &str) -> Result<String, String> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\$\{([^}]+)\}").expect("static regex is valid"));
    let mut result = value.to_string();

    for cap in re.captures_iter(value) {
        let full_match = cap.get(0).expect("capture group 0 always exists in captures_iter").as_str();
        let inner = cap.get(1).expect("capture group 1 always exists for this regex").as_str();

        let replacement = if inner.contains(":-") {
            let parts: Vec<&str> = inner.splitn(2, ":-").collect();
            let var_name = parts[0];
            let default_value = parts[1];

            let resolved = std::env::var(var_name).unwrap_or_else(|_| default_value.to_string());
            tracing::debug!("Resolved env var {} (with default): {} -> {}", var_name, full_match, resolved);
            resolved
        } else if inner.contains(":=") {
            let parts: Vec<&str> = inner.splitn(2, ":=").collect();
            let var_name = parts[0];
            let default_value = parts[1];

            let val = std::env::var(var_name).unwrap_or_else(|_| default_value.to_string());
            tracing::warn!(
                "Config uses ':=' (assign) syntax for env var {} - this is a security risk and will be removed in a future version. Use ':-' (default) instead.",
                var_name
            );
            val
        } else if inner.contains(":?") {
            let parts: Vec<&str> = inner.splitn(2, ":?").collect();
            let var_name = parts[0];
            let error_msg = parts[1];

            let val = match std::env::var(var_name) {
                Ok(v) => v,
                Err(_) => {
                    return Err(format!("Environment variable {var_name} is required: {error_msg}"));
                }
            };
            tracing::debug!("Resolved required env var {}: {} -> {}", var_name, full_match, val);
            val
        } else {
            let resolved = std::env::var(inner).unwrap_or_else(|_| "".to_string());
            tracing::debug!("Resolved env var {}: {} -> {}", inner, full_match, resolved);
            resolved
        };

        result = result.replace(full_match, &replacement);
    }

    Ok(result)
}
