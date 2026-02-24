use argon2::Params;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use thiserror::Error;

static GLOBAL_ARGON2_CONFIG: OnceLock<Argon2Config> = OnceLock::new();

#[derive(Debug, Error)]
pub enum Argon2ConfigError {
    #[error("Invalid Argon2 parameters: {0}")]
    InvalidParams(String),

    #[error("OWASP validation failed: {0}")]
    OwaspValidation(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Argon2Config {
    #[serde(default = "default_m_cost")]
    pub m_cost: u32,

    #[serde(default = "default_t_cost")]
    pub t_cost: u32,

    #[serde(default = "default_p_cost")]
    pub p_cost: u32,

    #[serde(default = "default_output_len")]
    pub output_len: Option<usize>,
}

fn default_m_cost() -> u32 {
    65536
}

fn default_t_cost() -> u32 {
    3
}

fn default_p_cost() -> u32 {
    1
}

fn default_output_len() -> Option<usize> {
    Some(32)
}

impl Default for Argon2Config {
    fn default() -> Self {
        Self {
            m_cost: default_m_cost(),
            t_cost: default_t_cost(),
            p_cost: default_p_cost(),
            output_len: default_output_len(),
        }
    }
}

impl Argon2Config {
    pub const OWASP_MIN_M_COST: u32 = 65536;
    pub const OWASP_MIN_T_COST: u32 = 3;
    pub const OWASP_MIN_P_COST: u32 = 1;

    pub fn new(m_cost: u32, t_cost: u32, p_cost: u32) -> Result<Self, Argon2ConfigError> {
        Self::new_with_output_len(m_cost, t_cost, p_cost, Some(32))
    }

    pub fn new_with_output_len(
        m_cost: u32,
        t_cost: u32,
        p_cost: u32,
        output_len: Option<usize>,
    ) -> Result<Self, Argon2ConfigError> {
        let config = Self {
            m_cost,
            t_cost,
            p_cost,
            output_len,
        };
        config.validate()?;
        config.validate_owasp()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), Argon2ConfigError> {
        if self.m_cost < 8 {
            return Err(Argon2ConfigError::InvalidParams(
                "m_cost must be at least 8 (Argon2 minimum)".to_string(),
            ));
        }

        if self.t_cost < 1 {
            return Err(Argon2ConfigError::InvalidParams(
                "t_cost must be at least 1".to_string(),
            ));
        }

        if self.p_cost < 1 {
            return Err(Argon2ConfigError::InvalidParams(
                "p_cost must be at least 1".to_string(),
            ));
        }

        if let Some(len) = self.output_len {
            if len < 4 {
                return Err(Argon2ConfigError::InvalidParams(
                    "output_len must be at least 4 bytes".to_string(),
                ));
            }
            if len > 256 {
                return Err(Argon2ConfigError::InvalidParams(
                    "output_len must not exceed 256 bytes".to_string(),
                ));
            }
        }

        Params::new(self.m_cost, self.t_cost, self.p_cost, self.output_len)
            .map_err(|e| Argon2ConfigError::InvalidParams(e.to_string()))?;

        Ok(())
    }

    pub fn validate_owasp(&self) -> Result<(), Argon2ConfigError> {
        if self.m_cost < Self::OWASP_MIN_M_COST {
            return Err(Argon2ConfigError::OwaspValidation(format!(
                "m_cost ({}) is below OWASP minimum ({}). \
                 For security compliance, m_cost should be at least 65536 (64 MiB).",
                self.m_cost,
                Self::OWASP_MIN_M_COST
            )));
        }

        if self.t_cost < Self::OWASP_MIN_T_COST {
            return Err(Argon2ConfigError::OwaspValidation(format!(
                "t_cost ({}) is below OWASP minimum ({}). \
                 For security compliance, t_cost should be at least 3 iterations.",
                self.t_cost,
                Self::OWASP_MIN_T_COST
            )));
        }

        if self.p_cost < Self::OWASP_MIN_P_COST {
            return Err(Argon2ConfigError::OwaspValidation(format!(
                "p_cost ({}) is below OWASP minimum ({}). \
                 For security compliance, p_cost should be at least 1.",
                self.p_cost,
                Self::OWASP_MIN_P_COST
            )));
        }

        Ok(())
    }

    pub fn to_argon2_params(&self) -> Result<Params, Argon2ConfigError> {
        self.validate()?;
        Params::new(self.m_cost, self.t_cost, self.p_cost, self.output_len)
            .map_err(|e| Argon2ConfigError::InvalidParams(e.to_string()))
    }

    pub fn initialize_global(config: Argon2Config) -> Result<(), Argon2ConfigError> {
        config.validate()?;
        config.validate_owasp()?;

        let _ = GLOBAL_ARGON2_CONFIG.set(config);
        Ok(())
    }

    pub fn get_global() -> Argon2Config {
        GLOBAL_ARGON2_CONFIG.get().copied().unwrap_or_default()
    }

    pub fn is_global_initialized() -> bool {
        GLOBAL_ARGON2_CONFIG.get().is_some()
    }

    pub fn memory_cost_bytes(&self) -> u64 {
        (self.m_cost as u64) * 1024
    }

    pub fn memory_cost_mb(&self) -> u64 {
        self.memory_cost_bytes() / (1024 * 1024)
    }

    pub fn estimated_hash_time_ms(&self) -> u64 {
        let base_time_per_iteration_ms = 1;
        let memory_factor = self.m_cost as f64 / 65536.0;
        let parallelism_factor = 1.0 / self.p_cost as f64;

        (base_time_per_iteration_ms as f64
            * self.t_cost as f64
            * memory_factor
            * parallelism_factor) as u64
    }
}

impl std::fmt::Display for Argon2Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Argon2Config(m_cost={}, t_cost={}, p_cost={}, output_len={:?})",
            self.m_cost, self.t_cost, self.p_cost, self.output_len
        )
    }
}

impl From<crate::common::config::SecurityConfig> for Argon2Config {
    fn from(security: crate::common::config::SecurityConfig) -> Self {
        Self {
            m_cost: security.argon2_m_cost,
            t_cost: security.argon2_t_cost,
            p_cost: security.argon2_p_cost,
            output_len: Some(32),
        }
    }
}

impl From<&crate::common::config::SecurityConfig> for Argon2Config {
    fn from(security: &crate::common::config::SecurityConfig) -> Self {
        Self {
            m_cost: security.argon2_m_cost,
            t_cost: security.argon2_t_cost,
            p_cost: security.argon2_p_cost,
            output_len: Some(32),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Argon2Config::default();
        assert_eq!(config.m_cost, 65536);
        assert_eq!(config.t_cost, 3);
        assert_eq!(config.p_cost, 1);
        assert_eq!(config.output_len, Some(32));
    }

    #[test]
    fn test_valid_config_creation() {
        let config = Argon2Config::new(65536, 3, 1).unwrap();
        assert_eq!(config.m_cost, 65536);
        assert_eq!(config.t_cost, 3);
        assert_eq!(config.p_cost, 1);
    }

    #[test]
    fn test_invalid_m_cost() {
        let result = Argon2Config::new(4, 3, 1);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Argon2ConfigError::InvalidParams(_)
        ));
    }

    #[test]
    fn test_invalid_t_cost() {
        let result = Argon2Config::new(65536, 0, 1);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Argon2ConfigError::InvalidParams(_)
        ));
    }

    #[test]
    fn test_invalid_p_cost() {
        let result = Argon2Config::new(65536, 3, 0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Argon2ConfigError::InvalidParams(_)
        ));
    }

    #[test]
    fn test_owasp_validation_m_cost_too_low() {
        let config = Argon2Config {
            m_cost: 4096,
            t_cost: 3,
            p_cost: 1,
            output_len: Some(32),
        };
        let result = config.validate_owasp();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Argon2ConfigError::OwaspValidation(_)
        ));
    }

    #[test]
    fn test_owasp_validation_t_cost_too_low() {
        let config = Argon2Config {
            m_cost: 65536,
            t_cost: 1,
            p_cost: 1,
            output_len: Some(32),
        };
        let result = config.validate_owasp();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Argon2ConfigError::OwaspValidation(_)
        ));
    }

    #[test]
    fn test_owasp_validation_passes() {
        let config = Argon2Config::default();
        let result = config.validate_owasp();
        assert!(result.is_ok());
    }

    #[test]
    fn test_to_argon2_params() {
        let config = Argon2Config::default();
        let params = config.to_argon2_params().unwrap();
        assert_eq!(params.m_cost(), 65536);
        assert_eq!(params.t_cost(), 3);
        assert_eq!(params.p_cost(), 1);
    }

    #[test]
    fn test_output_len_validation() {
        let result = Argon2Config::new_with_output_len(65536, 3, 1, Some(2));
        assert!(result.is_err());

        let result = Argon2Config::new_with_output_len(65536, 3, 1, Some(300));
        assert!(result.is_err());

        let result = Argon2Config::new_with_output_len(65536, 3, 1, Some(64));
        assert!(result.is_ok());
    }

    #[test]
    fn test_global_config() {
        assert!(!Argon2Config::is_global_initialized());

        let config = Argon2Config::new(65536, 3, 1).unwrap();
        Argon2Config::initialize_global(config).unwrap();

        assert!(Argon2Config::is_global_initialized());

        let global = Argon2Config::get_global();
        assert_eq!(global.m_cost, 65536);
        assert_eq!(global.t_cost, 3);
        assert_eq!(global.p_cost, 1);
    }

    #[test]
    fn test_memory_cost_calculation() {
        let config = Argon2Config::default();
        assert_eq!(config.memory_cost_bytes(), 67108864);
        assert_eq!(config.memory_cost_mb(), 64);
    }

    #[test]
    fn test_display() {
        let config = Argon2Config::default();
        let display = format!("{}", config);
        assert!(display.contains("m_cost=65536"));
        assert!(display.contains("t_cost=3"));
        assert!(display.contains("p_cost=1"));
    }

    #[test]
    fn test_backward_compatible_low_cost() {
        let config = Argon2Config {
            m_cost: 4096,
            t_cost: 3,
            p_cost: 1,
            output_len: Some(32),
        };

        assert!(config.validate().is_ok());
        assert!(config.validate_owasp().is_err());
    }

    #[test]
    fn test_from_security_config() {
        use crate::common::config::SecurityConfig;

        let security = SecurityConfig {
            secret: "test_secret".to_string(),
            expiry_time: 3600,
            refresh_token_expiry: 604800,
            argon2_m_cost: 65536,
            argon2_t_cost: 3,
            argon2_p_cost: 1,
            allow_legacy_hashes: false,
            login_failure_lockout_threshold: 5,
            login_lockout_duration_seconds: 900,
        };

        let config = Argon2Config::from(security);
        assert_eq!(config.m_cost, 65536);
        assert_eq!(config.t_cost, 3);
        assert_eq!(config.p_cost, 1);
    }
}
