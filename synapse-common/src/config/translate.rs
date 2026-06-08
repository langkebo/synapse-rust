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