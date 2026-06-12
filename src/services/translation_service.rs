//! Translation service — proxies translation requests to external providers.
//!
//! Supported providers:
//! - **Google Cloud Translation API v2** (`google`)
//! - **DeepL API** (`deepl`)
//! - **LibreTranslate** (`libretranslate`)
//!
//! When the service is not configured (`enabled = false` or missing `api_key`),
//! it falls back to returning the original text (passthrough mode).

use crate::common::config::TranslateConfig;
use moka::future::Cache;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ============================================================================
// Types
// ============================================================================

/// Result of a translation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResult {
    /// The translated text.
    pub translated_text: String,
    /// Detected source language (if auto-detected by the provider).
    pub detected_source_lang: Option<String>,
    /// The target language that was used.
    pub target_lang: String,
    /// The translation provider that handled the request.
    pub provider: String,
}

/// Cache key for translation results.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct TranslationCacheKey {
    text: String,
    target_lang: String,
    source_lang: Option<String>,
    provider: String,
}

// ============================================================================
// Service
// ============================================================================

#[derive(Clone)]
pub struct TranslationService {
    http_client: Client,
    config: TranslateConfig,
    cache: Cache<TranslationCacheKey, TranslationResult>,
}

impl TranslationService {
    pub fn new(config: TranslateConfig) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .connect_timeout(Duration::from_secs(5))
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| Client::new());

        let cache =
            Cache::builder().max_capacity(10_000).time_to_idle(Duration::from_secs(config.cache_ttl_secs)).build();

        Self { http_client, config, cache }
    }

    /// Returns true if the translation service is properly configured and enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.is_configured()
    }

    /// Translate text to the target language.
    ///
    /// If the service is not configured, returns the original text as a passthrough.
    pub async fn translate(
        &self,
        text: &str,
        target_lang: &str,
        source_lang: Option<&str>,
    ) -> Result<TranslationResult, TranslationError> {
        // Passthrough mode when not configured
        if !self.config.is_configured() {
            return Ok(TranslationResult {
                translated_text: text.to_string(),
                detected_source_lang: source_lang.map(|s| s.to_string()),
                target_lang: target_lang.to_string(),
                provider: "passthrough".to_string(),
            });
        }

        // Validate text length
        if text.len() > self.config.max_text_length {
            return Err(TranslationError::TextTooLong { length: text.len(), max: self.config.max_text_length });
        }

        // Skip empty text
        if text.is_empty() {
            return Ok(TranslationResult {
                translated_text: String::new(),
                detected_source_lang: None,
                target_lang: target_lang.to_string(),
                provider: self.config.provider.clone(),
            });
        }

        // Check cache
        let cache_key = TranslationCacheKey {
            text: text.to_string(),
            target_lang: target_lang.to_string(),
            source_lang: source_lang.map(|s| s.to_string()),
            provider: self.config.provider.clone(),
        };

        if let Some(cached) = self.cache.get(&cache_key).await {
            return Ok(cached);
        }

        // Dispatch to provider
        let result = match self.config.provider.as_str() {
            "google" => self.translate_google(text, target_lang, source_lang).await?,
            "deepl" => self.translate_deepl(text, target_lang, source_lang).await?,
            "libretranslate" => self.translate_libretranslate(text, target_lang, source_lang).await?,
            other => {
                return Err(TranslationError::UnsupportedProvider { provider: other.to_string() });
            }
        };

        // Cache the result
        self.cache.insert(cache_key, result.clone()).await;

        Ok(result)
    }

    // ========================================================================
    // Google Cloud Translation API v2
    // ========================================================================

    async fn translate_google(
        &self,
        text: &str,
        target_lang: &str,
        source_lang: Option<&str>,
    ) -> Result<TranslationResult, TranslationError> {
        let base_url = self.config.resolved_api_url();
        let url = format!("{}/language/translate/v2", base_url);

        let mut body = serde_json::json!({
            "q": text,
            "target": target_lang,
            "format": "text",
        });

        if let Some(src) = source_lang {
            body["source"] = serde_json::Value::String(src.to_string());
        }

        let response =
            self.http_client.post(&url).query(&[("key", &self.config.api_key)]).json(&body).send().await.map_err(
                |e| TranslationError::RequestFailed { provider: "google".to_string(), reason: e.to_string() },
            )?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(TranslationError::ProviderError {
                provider: "google".to_string(),
                status: status.as_u16(),
                message: body,
            });
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| TranslationError::ParseError { provider: "google".to_string(), reason: e.to_string() })?;

        let translations =
            result.get("data").and_then(|d| d.get("translations")).and_then(|t| t.as_array()).ok_or_else(|| {
                TranslationError::ParseError {
                    provider: "google".to_string(),
                    reason: "missing data.translations".to_string(),
                }
            })?;

        let first = translations.first().ok_or_else(|| TranslationError::ParseError {
            provider: "google".to_string(),
            reason: "empty translations array".to_string(),
        })?;

        let translated_text = first.get("translatedText").and_then(|t| t.as_str()).unwrap_or(text).to_string();

        let detected_source_lang = first.get("detectedSourceLanguage").and_then(|l| l.as_str()).map(|s| s.to_string());

        Ok(TranslationResult {
            translated_text,
            detected_source_lang,
            target_lang: target_lang.to_string(),
            provider: "google".to_string(),
        })
    }

    // ========================================================================
    // DeepL API
    // ========================================================================

    async fn translate_deepl(
        &self,
        text: &str,
        target_lang: &str,
        source_lang: Option<&str>,
    ) -> Result<TranslationResult, TranslationError> {
        let base_url = self.config.resolved_api_url();
        let url = format!("{}/translate", base_url);

        let mut params =
            vec![("text".to_string(), text.to_string()), ("target_lang".to_string(), target_lang.to_uppercase())];

        if let Some(src) = source_lang {
            params.push(("source_lang".to_string(), src.to_uppercase()));
        }

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("DeepL-Auth-Key {}", self.config.api_key))
            .form(&params)
            .send()
            .await
            .map_err(|e| TranslationError::RequestFailed { provider: "deepl".to_string(), reason: e.to_string() })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(TranslationError::ProviderError {
                provider: "deepl".to_string(),
                status: status.as_u16(),
                message: body,
            });
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| TranslationError::ParseError { provider: "deepl".to_string(), reason: e.to_string() })?;

        let translations = result.get("translations").and_then(|t| t.as_array()).ok_or_else(|| {
            TranslationError::ParseError { provider: "deepl".to_string(), reason: "missing translations".to_string() }
        })?;

        let first = translations.first().ok_or_else(|| TranslationError::ParseError {
            provider: "deepl".to_string(),
            reason: "empty translations array".to_string(),
        })?;

        let translated_text = first.get("text").and_then(|t| t.as_str()).unwrap_or(text).to_string();

        let detected_source_lang =
            first.get("detected_source_language").and_then(|l| l.as_str()).map(|s| s.to_string());

        Ok(TranslationResult {
            translated_text,
            detected_source_lang,
            target_lang: target_lang.to_string(),
            provider: "deepl".to_string(),
        })
    }

    // ========================================================================
    // LibreTranslate
    // ========================================================================

    async fn translate_libretranslate(
        &self,
        text: &str,
        target_lang: &str,
        source_lang: Option<&str>,
    ) -> Result<TranslationResult, TranslationError> {
        let base_url = self.config.resolved_api_url();
        let url = format!("{}/translate", base_url);

        let mut body = serde_json::json!({
            "q": text,
            "target": target_lang,
            "format": "text",
        });

        if let Some(src) = source_lang {
            body["source"] = serde_json::Value::String(src.to_string());
        } else {
            body["source"] = serde_json::Value::String("auto".to_string());
        }

        if !self.config.api_key.is_empty() {
            body["api_key"] = serde_json::Value::String(self.config.api_key.clone());
        }

        let response = self.http_client.post(&url).json(&body).send().await.map_err(|e| {
            TranslationError::RequestFailed { provider: "libretranslate".to_string(), reason: e.to_string() }
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(TranslationError::ProviderError {
                provider: "libretranslate".to_string(),
                status: status.as_u16(),
                message: body,
            });
        }

        let result: serde_json::Value = response.json().await.map_err(|e| TranslationError::ParseError {
            provider: "libretranslate".to_string(),
            reason: e.to_string(),
        })?;

        let translated_text = result.get("translatedText").and_then(|t| t.as_str()).unwrap_or(text).to_string();

        let detected_source_lang = result
            .get("detectedLanguage")
            .and_then(|dl| dl.get("language"))
            .and_then(|l| l.as_str())
            .map(|s| s.to_string());

        Ok(TranslationResult {
            translated_text,
            detected_source_lang,
            target_lang: target_lang.to_string(),
            provider: "libretranslate".to_string(),
        })
    }
}

// ============================================================================
// Error types
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum TranslationError {
    #[error("Translation request to {provider} failed: {reason}")]
    RequestFailed { provider: String, reason: String },

    #[error("Translation provider {provider} returned error {status}: {message}")]
    ProviderError { provider: String, status: u16, message: String },

    #[error("Failed to parse {provider} response: {reason}")]
    ParseError { provider: String, reason: String },

    #[error("Unsupported translation provider: {provider}")]
    UnsupportedProvider { provider: String },

    #[error("Text too long: {length} bytes (max: {max})")]
    TextTooLong { length: usize, max: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== TranslationResult tests ==========

    #[test]
    fn test_translation_result() {
        let result = TranslationResult {
            translated_text: "Bonjour".to_string(),
            detected_source_lang: Some("en".to_string()),
            target_lang: "fr".to_string(),
            provider: "google".to_string(),
        };
        assert_eq!(result.translated_text, "Bonjour");
        assert_eq!(result.detected_source_lang, Some("en".to_string()));
        assert_eq!(result.target_lang, "fr");
        assert_eq!(result.provider, "google");
    }

    #[test]
    fn test_translation_result_no_detected_lang() {
        let result = TranslationResult {
            translated_text: "Hello".to_string(),
            detected_source_lang: None,
            target_lang: "en".to_string(),
            provider: "deepl".to_string(),
        };
        assert_eq!(result.translated_text, "Hello");
        assert!(result.detected_source_lang.is_none());
    }

    #[test]
    fn test_translation_result_clone() {
        let result = TranslationResult {
            translated_text: "test".to_string(),
            detected_source_lang: None,
            target_lang: "en".to_string(),
            provider: "google".to_string(),
        };
        let cloned = result.clone();
        assert_eq!(cloned.translated_text, result.translated_text);
    }

    // ========== TranslationCacheKey tests ==========

    #[test]
    fn test_translation_cache_key() {
        let key = TranslationCacheKey {
            text: "hello".to_string(),
            target_lang: "fr".to_string(),
            source_lang: Some("en".to_string()),
            provider: "google".to_string(),
        };
        assert_eq!(key.text, "hello");
        assert_eq!(key.target_lang, "fr");
    }

    #[test]
    fn test_translation_cache_key_equality() {
        let key1 = TranslationCacheKey {
            text: "hello".to_string(),
            target_lang: "fr".to_string(),
            source_lang: Some("en".to_string()),
            provider: "google".to_string(),
        };
        let key2 = TranslationCacheKey {
            text: "hello".to_string(),
            target_lang: "fr".to_string(),
            source_lang: Some("en".to_string()),
            provider: "google".to_string(),
        };
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_translation_cache_key_inequality() {
        let key1 = TranslationCacheKey {
            text: "hello".to_string(),
            target_lang: "fr".to_string(),
            source_lang: None,
            provider: "google".to_string(),
        };
        let key2 = TranslationCacheKey {
            text: "hello".to_string(),
            target_lang: "fr".to_string(),
            source_lang: Some("en".to_string()),
            provider: "google".to_string(),
        };
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_translation_cache_key_clone() {
        let key = TranslationCacheKey {
            text: "test".to_string(),
            target_lang: "en".to_string(),
            source_lang: None,
            provider: "google".to_string(),
        };
        let cloned = key.clone();
        assert_eq!(cloned, key);
    }

    // ========== TranslationError tests ==========

    #[test]
    fn test_translation_error_request_failed() {
        let err = TranslationError::RequestFailed { provider: "google".to_string(), reason: "timeout".to_string() };
        let msg = err.to_string();
        assert!(msg.contains("google"));
        assert!(msg.contains("timeout"));
    }

    #[test]
    fn test_translation_error_provider_error() {
        let err = TranslationError::ProviderError {
            provider: "deepl".to_string(),
            status: 403,
            message: "Forbidden".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("deepl"));
        assert!(msg.contains("403"));
        assert!(msg.contains("Forbidden"));
    }

    #[test]
    fn test_translation_error_parse_error() {
        let err =
            TranslationError::ParseError { provider: "libretranslate".to_string(), reason: "invalid JSON".to_string() };
        let msg = err.to_string();
        assert!(msg.contains("libretranslate"));
        assert!(msg.contains("invalid JSON"));
    }

    #[test]
    fn test_translation_error_unsupported_provider() {
        let err = TranslationError::UnsupportedProvider { provider: "unknown".to_string() };
        let msg = err.to_string();
        assert!(msg.contains("unknown"));
    }

    #[test]
    fn test_translation_error_text_too_long() {
        let err = TranslationError::TextTooLong { length: 5000, max: 1000 };
        let msg = err.to_string();
        assert!(msg.contains("5000"));
        assert!(msg.contains("1000"));
    }

    #[test]
    fn test_translation_error_debug() {
        let err = TranslationError::TextTooLong { length: 10, max: 5 };
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("TextTooLong"));
    }
}
