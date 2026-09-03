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

        // 审查 Ticket #16：csrf_secret 显式配置为空字符串时会绕过 default_fn，
        // 导致 CSRF token 签名密钥为空，使 CSRF 保护完全失效。强制要求非空。
        if self.security.csrf_secret.is_empty() {
            return Err("security.csrf_secret is empty. CSRF protection requires a non-empty secret. \
                 Either remove the csrf_secret field to auto-generate one, \
                 or set it to a random string of at least 32 characters."
                .to_string());
        }

        // 审查 #15：security.secret 是 HS256/JWT 签名密钥，除长度外还必须校验
        // 熵，防止弱熵密钥（如 32 个 'a'）被接受后可被离线爆破伪造。
        // 复用 SecurityValidator::validate_jwt_secret（此前为死代码，仅测试引用）。
        crate::security::SecurityValidator::validate_jwt_secret(&self.security.secret)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> Config {
        let mut c = Config::default();
        c.security.secret = "a-very-secure-secret-that-is-long-enough".to_string();
        c
    }

    #[test]
    fn validate_ok_with_valid_config() {
        let config = valid_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_rejects_empty_secret() {
        let mut config = Config::default();
        config.security.secret = String::new();
        let err = config.validate().unwrap_err();
        assert!(err.contains("secret is not configured"));
    }

    #[test]
    fn validate_rejects_short_secret() {
        let mut config = Config::default();
        config.security.secret = "too-short".to_string();
        let err = config.validate().unwrap_err();
        assert!(err.contains("at least 32 characters"));
    }

    // 审查 #15：弱熵密钥（32 个 'a'）长度达标但熵不足，应被拒绝，
    // 否则 HS256 token 可被离线爆破伪造。
    #[test]
    fn validate_rejects_low_entropy_secret() {
        let mut config = Config::default();
        config.security.secret = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(); // 32 个 'a'
        let err = config.validate().unwrap_err();
        assert!(err.contains("low entropy"), "low-entropy secret must be rejected: {err}");
    }

    #[test]
    fn validate_rejects_admin_registration_without_shared_secret() {
        let mut config = valid_config();
        config.admin_registration.enabled = true;
        config.admin_registration.shared_secret = String::new();
        let err = config.validate().unwrap_err();
        assert!(err.contains("shared_secret is not configured"));
    }

    #[test]
    fn validate_allows_admin_registration_with_shared_secret() {
        let mut config = valid_config();
        config.admin_registration.enabled = true;
        config.admin_registration.shared_secret = "a-shared-secret".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_rejects_admin_mfa_without_shared_secret() {
        let mut config = valid_config();
        config.security.admin_mfa_required = true;
        config.security.admin_mfa_shared_secret = String::new();
        let err = config.validate().unwrap_err();
        assert!(err.contains("admin_mfa_shared_secret is not configured"));
    }

    #[test]
    fn validate_allows_admin_mfa_with_shared_secret() {
        let mut config = valid_config();
        config.security.admin_mfa_required = true;
        config.security.admin_mfa_shared_secret = "mfa-secret".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_short_secret_error_includes_actual_length() {
        let mut config = Config::default();
        config.security.secret = "abc".to_string();
        let err = config.validate().unwrap_err();
        assert!(err.contains("3"));
    }

    // 审查 Ticket #16：csrf_secret 显式配置为空字符串必须被拒绝，
    // 否则 CSRF token 签名密钥为空使保护完全失效。
    #[test]
    fn validate_rejects_empty_csrf_secret() {
        let mut config = valid_config();
        config.security.csrf_secret = String::new();
        let err = config.validate().unwrap_err();
        assert!(err.contains("csrf_secret is empty"), "empty csrf_secret must be rejected: {err}");
    }

    #[test]
    fn validate_accepts_non_empty_csrf_secret() {
        let mut config = valid_config();
        config.security.csrf_secret = "a-32-byte-random-string-cere-1234".to_string();
        assert!(config.validate().is_ok());
    }
}
