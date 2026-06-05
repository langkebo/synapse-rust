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
    /// 验证码有效期（秒）
    #[serde(default = "default_verification_expire")]
    pub verification_token_expire: i64,
    /// 速率限制配置
    #[serde(default)]
    pub rate_limit: SmtpRateLimitConfig,
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

fn default_verification_expire() -> i64 {
    900 // 15分钟
}

/// SMTP发送速率限制配置。
#[derive(Debug, Clone, Deserialize)]
pub struct SmtpRateLimitConfig {
    /// 每分钟最大发送数
    #[serde(default = "default_smtp_per_minute")]
    pub per_minute: u32,
    /// 每小时最大发送数
    #[serde(default = "default_smtp_per_hour")]
    pub per_hour: u32,
}

impl Default for SmtpRateLimitConfig {
    fn default() -> Self {
        Self { per_minute: default_smtp_per_minute(), per_hour: default_smtp_per_hour() }
    }
}

fn default_smtp_per_minute() -> u32 {
    3
}

fn default_smtp_per_hour() -> u32 {
    10
}