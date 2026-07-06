use serde::Deserialize;

// ============================================================================
// SECTION: Translation Configuration
// ============================================================================

fn default_translate_provider() -> String {
    "google".to_string()
}

fn default_translate_enabled() -> bool {
    false
}

fn default_translate_cache_ttl_secs() -> u64 {
    86400 // 24 hours
}

fn default_translate_timeout_secs() -> u64 {
    10
}

fn default_translate_max_text_length() -> usize {
    5000
}

/// Translation service configuration.
///
/// Supports multiple providers: `google` (Google Cloud Translation API),
/// `deepl` (DeepL API), `libretranslate` (self-hosted LibreTranslate).
///
/// When `enabled` is `false`, the translate endpoint returns the original text
/// (passthrough/stub behavior).
#[derive(Debug, Clone, Deserialize)]
pub struct TranslateConfig {
    /// Whether the translation service is enabled.
    /// When disabled, the translate endpoint returns the original text.
    #[serde(default = "default_translate_enabled")]
    pub enabled: bool,

    /// Translation provider: `google`, `deepl`, or `libretranslate`.
    #[serde(default = "default_translate_provider")]
    pub provider: String,

    /// API key for the translation provider.
    /// For Google: the Cloud Translation API key.
    /// For DeepL: the DeepL API key.
    /// For LibreTranslate: optional (if the instance requires one).
    #[serde(default)]
    pub api_key: String,

    /// Base URL for the translation API.
    /// Required for LibreTranslate (e.g., `http://localhost:5000`).
    /// For DeepL, defaults to `https://api.deepl.com/v2` (or `https://api-free.deepl.com/v2` for free tier).
    /// For Google, defaults to `https://translation.googleapis.com`.
    #[serde(default)]
    pub api_url: String,

    /// Default target language code (e.g., "en", "zh", "ja").
    /// Used when the client does not specify a target language.
    #[serde(default = "default_translate_target_lang")]
    pub default_target_lang: String,

    /// Cache TTL for translated results (in seconds).
    #[serde(default = "default_translate_cache_ttl_secs")]
    pub cache_ttl_secs: u64,

    /// HTTP request timeout for translation API calls (in seconds).
    #[serde(default = "default_translate_timeout_secs")]
    pub timeout_secs: u64,

    /// Maximum text length allowed per translation request.
    #[serde(default = "default_translate_max_text_length")]
    pub max_text_length: usize,
}

fn default_translate_target_lang() -> String {
    "en".to_string()
}

impl Default for TranslateConfig {
    fn default() -> Self {
        Self {
            enabled: default_translate_enabled(),
            provider: default_translate_provider(),
            api_key: String::new(),
            api_url: String::new(),
            default_target_lang: default_translate_target_lang(),
            cache_ttl_secs: default_translate_cache_ttl_secs(),
            timeout_secs: default_translate_timeout_secs(),
            max_text_length: default_translate_max_text_length(),
        }
    }
}

impl TranslateConfig {
    /// Returns the resolved API URL for the configured provider.
    pub fn resolved_api_url(&self) -> String {
        if !self.api_url.is_empty() {
            return self.api_url.clone();
        }
        match self.provider.as_str() {
            "google" => "https://translation.googleapis.com".to_string(),
            "deepl" => "https://api.deepl.com/v2".to_string(),
            "libretranslate" => "http://localhost:5000".to_string(),
            _ => self.api_url.clone(),
        }
    }

    /// Returns true if the configuration is sufficient to make translation requests.
    pub fn is_configured(&self) -> bool {
        self.enabled && !self.api_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let cfg = TranslateConfig::default();
        assert!(!cfg.enabled, "enabled should default to false");
        assert_eq!(cfg.provider, "google", "provider should default to 'google'");
        assert!(cfg.api_key.is_empty(), "api_key should default to empty");
        assert!(cfg.api_url.is_empty(), "api_url should default to empty");
        assert_eq!(cfg.default_target_lang, "en");
        assert_eq!(cfg.cache_ttl_secs, 86400);
        assert_eq!(cfg.timeout_secs, 10);
        assert_eq!(cfg.max_text_length, 5000);
    }

    #[test]
    fn default_helpers_return_expected_values() {
        assert_eq!(default_translate_provider(), "google");
        assert!(!default_translate_enabled());
        assert_eq!(default_translate_cache_ttl_secs(), 86400);
        assert_eq!(default_translate_timeout_secs(), 10);
        assert_eq!(default_translate_max_text_length(), 5000);
        assert_eq!(default_translate_target_lang(), "en");
    }

    #[test]
    fn deserialize_empty_uses_defaults() {
        let yaml = "{}\n";
        let cfg: TranslateConfig = serde_yaml::from_str(yaml).expect("empty YAML should deserialize with defaults");
        assert!(!cfg.enabled);
        assert_eq!(cfg.provider, "google");
        assert!(cfg.api_key.is_empty());
        assert_eq!(cfg.default_target_lang, "en");
        assert_eq!(cfg.cache_ttl_secs, 86400);
        assert_eq!(cfg.timeout_secs, 10);
        assert_eq!(cfg.max_text_length, 5000);
    }

    #[test]
    fn deserialize_explicit_values_override_defaults() {
        let yaml = "\
enabled: true
provider: deepl
api_key: secret-key-12345
api_url: https://custom.api.example.com
default_target_lang: zh
cache_ttl_secs: 3600
timeout_secs: 30
max_text_length: 10000
";
        let cfg: TranslateConfig = serde_yaml::from_str(yaml).expect("explicit YAML should override defaults");
        assert!(cfg.enabled);
        assert_eq!(cfg.provider, "deepl");
        assert_eq!(cfg.api_key, "secret-key-12345");
        assert_eq!(cfg.api_url, "https://custom.api.example.com");
        assert_eq!(cfg.default_target_lang, "zh");
        assert_eq!(cfg.cache_ttl_secs, 3600);
        assert_eq!(cfg.timeout_secs, 30);
        assert_eq!(cfg.max_text_length, 10000);
    }

    #[test]
    fn resolved_api_url_returns_custom_url_when_set() {
        let cfg = TranslateConfig {
            api_url: "https://custom.example.com".to_string(),
            provider: "google".to_string(),
            ..Default::default()
        };
        assert_eq!(cfg.resolved_api_url(), "https://custom.example.com");
    }

    #[test]
    fn resolved_api_url_returns_provider_default_for_google() {
        let cfg = TranslateConfig { api_url: String::new(), provider: "google".to_string(), ..Default::default() };
        assert_eq!(cfg.resolved_api_url(), "https://translation.googleapis.com");
    }

    #[test]
    fn resolved_api_url_returns_provider_default_for_deepl() {
        let cfg = TranslateConfig { api_url: String::new(), provider: "deepl".to_string(), ..Default::default() };
        assert_eq!(cfg.resolved_api_url(), "https://api.deepl.com/v2");
    }

    #[test]
    fn resolved_api_url_returns_provider_default_for_libretranslate() {
        let cfg =
            TranslateConfig { api_url: String::new(), provider: "libretranslate".to_string(), ..Default::default() };
        assert_eq!(cfg.resolved_api_url(), "http://localhost:5000");
    }

    #[test]
    fn resolved_api_url_returns_empty_for_unknown_provider_without_explicit_url() {
        let cfg =
            TranslateConfig { api_url: String::new(), provider: "unknown_provider".to_string(), ..Default::default() };
        // Unknown provider with no explicit api_url falls through to empty string.
        assert_eq!(cfg.resolved_api_url(), "");
    }

    #[test]
    fn is_configured_returns_true_when_enabled_and_api_key_present() {
        let cfg = TranslateConfig { enabled: true, api_key: "key-12345".to_string(), ..Default::default() };
        assert!(cfg.is_configured());
    }

    #[test]
    fn is_configured_returns_false_when_disabled() {
        let cfg = TranslateConfig { enabled: false, api_key: "key-12345".to_string(), ..Default::default() };
        assert!(!cfg.is_configured());
    }

    #[test]
    fn is_configured_returns_false_when_enabled_but_no_api_key() {
        let cfg = TranslateConfig { enabled: true, api_key: String::new(), ..Default::default() };
        assert!(!cfg.is_configured());
    }

    #[test]
    fn is_configured_returns_false_when_disabled_and_no_api_key() {
        let cfg = TranslateConfig::default();
        assert!(!cfg.is_configured());
    }

    #[test]
    fn clone_preserves_all_fields() {
        let cfg = TranslateConfig {
            enabled: true,
            provider: "deepl".to_string(),
            api_key: "key".to_string(),
            api_url: "https://example.com".to_string(),
            default_target_lang: "fr".to_string(),
            cache_ttl_secs: 7200,
            timeout_secs: 25,
            max_text_length: 2500,
        };
        let cloned = cfg.clone();
        assert_eq!(cfg.enabled, cloned.enabled);
        assert_eq!(cfg.provider, cloned.provider);
        assert_eq!(cfg.api_key, cloned.api_key);
        assert_eq!(cfg.api_url, cloned.api_url);
        assert_eq!(cfg.default_target_lang, cloned.default_target_lang);
        assert_eq!(cfg.cache_ttl_secs, cloned.cache_ttl_secs);
        assert_eq!(cfg.timeout_secs, cloned.timeout_secs);
        assert_eq!(cfg.max_text_length, cloned.max_text_length);
    }

    #[test]
    fn debug_format_contains_struct_name() {
        let cfg = TranslateConfig::default();
        let debug_str = format!("{cfg:?}");
        assert!(debug_str.contains("TranslateConfig"));
    }
}
