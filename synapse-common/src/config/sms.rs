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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sms_config_default() {
        let config = SmsConfig::default();
        assert!(!config.enabled);
        assert!(config.provider.is_empty());
        assert!(config.api_key.is_empty());
        assert!(config.api_secret.is_empty());
        assert!(config.endpoint.is_empty());
        assert!(config.sender_id.is_empty());
        assert!(config.template_code.is_empty());
        assert_eq!(config.rate_limit_per_minute, 0);
        assert_eq!(config.rate_limit_per_hour, 0);
    }

    #[test]
    fn test_default_values() {
        assert_eq!(default_sms_enabled(), false);
        assert_eq!(default_sms_per_minute(), 1);
        assert_eq!(default_sms_per_hour(), 5);
    }
}
