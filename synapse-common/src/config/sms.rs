use serde::Deserialize;

/// SMS provider configuration.
///
/// Supports multiple SMS provider backends (aliyun, twilio, etc.)
/// with provider-specific credentials.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SmsConfig {
    /// Whether SMS captcha delivery is enabled
    #[serde(default = "default_sms_enabled")]
    pub enabled: bool,
    /// SMS provider type: "aliyun", "twilio", "custom"
    #[serde(default)]
    pub provider: String,
    /// Provider-specific API key / AccessKey ID
    #[serde(default)]
    pub api_key: String,
    /// Provider-specific API secret / AccessKey Secret
    #[serde(default)]
    pub api_secret: String,
    /// Provider-specific endpoint URL (e.g. SMS API endpoint)
    #[serde(default)]
    pub endpoint: String,
    /// Sender ID / signature / template code
    #[serde(default)]
    pub sender_id: String,
    /// SMS template code (e.g. Aliyun SMS_123456789)
    #[serde(default)]
    pub template_code: String,
    /// Rate limit: max SMS per minute
    #[serde(default = "default_sms_per_minute")]
    pub rate_limit_per_minute: u32,
    /// Rate limit: max SMS per hour
    #[serde(default = "default_sms_per_hour")]
    pub rate_limit_per_hour: u32,
}

fn default_sms_enabled() -> bool {
    false
}

fn default_sms_per_minute() -> u32 {
    1
}

fn default_sms_per_hour() -> u32 {
    5
}
