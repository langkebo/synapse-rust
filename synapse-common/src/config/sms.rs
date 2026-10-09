use educe::Educe;
use serde::Deserialize;

/// SMS provider configuration.
///
/// Supports multiple SMS provider backends (aliyun, twilio, etc.)
/// with provider-specific credentials.
#[derive(Clone, Deserialize, Default, Educe)]
#[educe(Debug)]
pub struct SmsConfig {
    /// Whether SMS captcha delivery is enabled
    #[serde(default = "default_sms_enabled")]
    pub enabled: bool,
    /// SMS provider type: "aliyun", "twilio", "custom"
    #[serde(default)]
    pub provider: String,
    /// Provider-specific API key / AccessKey ID
    #[serde(default)]
    #[educe(Debug(ignore))]
    pub api_key: String,
    /// Provider-specific API secret / AccessKey Secret
    #[serde(default)]
    #[educe(Debug(ignore))]
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
}

fn default_sms_enabled() -> bool {
    false
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
    }

    #[test]
    fn test_default_values() {
        assert!(!default_sms_enabled());
    }
}
