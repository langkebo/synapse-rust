//! Configuration validation logic.
//!
//! Validates the loaded `Config` for correctness, security requirements,
//! and best-practice recommendations.

use super::Config;

impl Config {
    /// Validate the configuration for correctness and security.
    pub fn validate(&self) -> Result<(), String> {
        if self.admin_registration.enabled && self.admin_registration.shared_secret.is_empty() {
            return Err("admin_registration.enabled is true but shared_secret is not configured. \
                 Please set admin_registration.shared_secret in your configuration file."
                .to_string());
        }

        if self.security.admin_mfa_required && self.security.admin_mfa_shared_secret.is_empty() {
            return Err(
                "security.admin_mfa_required is true but admin_mfa_shared_secret is not configured.".to_string()
            );
        }

        if self.security.secret.is_empty() {
            return Err("security.secret is not configured. \
                 Please set security.secret in your configuration file."
                .to_string());
        }

        if self.security.secret.len() < 32 {
            return Err("security.secret must be at least 32 characters for adequate security. \
                 Current length: {}. \
                 Generate a secure secret with: openssl rand -hex 32"
                .replace("{}", &self.security.secret.len().to_string()));
        }

        if self.cors.allowed_origins.iter().any(|o| o == "*") && self.cors.allow_credentials {
            tracing::warn!(
                "CORS is configured to allow all origins ('*') with credentials. \
                 This is not recommended for production. \
                 Consider specifying explicit allowed origins."
            );
        }

        if self.security.allow_legacy_hashes {
            tracing::warn!(
                "DEPRECATED: security.allow_legacy_hashes is enabled. \
                 Legacy SHA-256 password hashes are deprecated and will be removed in a future version. \
                 Please migrate all passwords to Argon2 by forcing password resets. \
                 Set allow_legacy_hashes: false after migration is complete."
            );
        }

        // Argon2 parameter floor enforcement: auto-raise below minimum and warn
        let argon2_config = crate::argon2_config::Argon2Config::from(&self.security);
        if argon2_config.m_cost != self.security.argon2_m_cost
            || argon2_config.t_cost != self.security.argon2_t_cost
            || argon2_config.p_cost != self.security.argon2_p_cost
        {
            tracing::warn!(
                "Argon2 parameters were below enforced floor and have been automatically raised. \
                 Config: m_cost={}, t_cost={}, p_cost={}. \
                 Effective: m_cost={}, t_cost={}, p_cost={}.",
                self.security.argon2_m_cost,
                self.security.argon2_t_cost,
                self.security.argon2_p_cost,
                argon2_config.m_cost,
                argon2_config.t_cost,
                argon2_config.p_cost
            );
        }

        // OWASP recommendation warning (below recommended but above floor)
        if let Err(e) = argon2_config.validate_owasp() {
            tracing::warn!(
                "Argon2 parameters do not meet OWASP recommendations: {}. \
                 Current: m_cost={}, t_cost={}, p_cost={}. \
                 Recommended minimum: m_cost=65536, t_cost=3, p_cost=1.",
                e,
                argon2_config.m_cost,
                argon2_config.t_cost,
                argon2_config.p_cost
            );
        }

        Ok(())
    }
}
