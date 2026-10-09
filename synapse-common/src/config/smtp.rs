use serde::Deserialize;

/// SMTP邮件服务配置。
///
/// 配置用于发送验证邮件的SMTP服务器参数。
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SmtpConfig {
    /// 是否启用SMTP功能
    #[serde(default = "default_smtp_enabled")]
    pub enabled: bool,
    /// SMTP服务器地址
    #[serde(default)]
    pub host: String,
    /// SMTP服务器端口
    #[serde(default = "default_smtp_port")]
    pub port: u16,
    /// SMTP用户名
    #[serde(default)]
    pub username: String,
    /// SMTP密码
    #[serde(default)]
    pub password: String,
    /// 发件人地址
    #[serde(default)]
    pub from: String,
    /// 是否使用TLS
    #[serde(default = "default_true")]
    pub tls: bool,
}

fn default_smtp_enabled() -> bool {
    false
}

fn default_smtp_port() -> u16 {
    587
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smtp_config_default() {
        let config = SmtpConfig::default();
        assert!(!config.enabled);
        assert!(config.host.is_empty());
        // Note: Default derive gives u16 default of 0, not 587
        // The serde default only applies during deserialization
        assert_eq!(config.port, 0);
        assert!(config.username.is_empty());
        assert!(config.password.is_empty());
        assert!(config.from.is_empty());
        // Default derive gives bool default of false, not true
        assert!(!config.tls);
    }

    #[test]
    fn test_default_values() {
        assert!(!default_smtp_enabled());
        assert_eq!(default_smtp_port(), 587);
        assert!(default_true());
    }
}
