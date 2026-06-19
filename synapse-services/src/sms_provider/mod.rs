pub mod aliyun;

use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use synapse_common::config::sms::SmsConfig;
use synapse_common::error::ApiError;
use tracing::warn;

/// SMS provider abstraction for captcha delivery.
///
/// Implementations can wrap real provider SDKs (Aliyun, Twilio, etc.)
/// or operate as a no-op stub for environments where SMS is not configured.
#[async_trait]
pub trait SmsProvider: Send + Sync {
    /// Send an SMS message to the given phone number.
    ///
    /// Returns `Ok(())` on successful delivery, or an `ApiError` describing
    /// the failure. The caller is responsible for rate limiting.
    async fn send(&self, to: &str, content: &str) -> Result<(), ApiError>;

    /// Human-readable provider name for logging (e.g. "aliyun", "twilio").
    fn provider_name(&self) -> &'static str;
}

/// No-op SMS provider — always returns `not_implemented`.
///
/// This is the default when no SMS provider is configured, ensuring the
/// server never panics at the SMS delivery boundary.
pub struct NoopSmsProvider;

#[async_trait]
impl SmsProvider for NoopSmsProvider {
    async fn send(&self, _to: &str, _content: &str) -> Result<(), ApiError> {
        Err(ApiError::not_implemented(
            "Captcha SMS delivery is not configured. Connect an SMS provider before using sms captcha.",
        ))
    }

    fn provider_name(&self) -> &'static str {
        "noop"
    }
}

#[derive(Clone)]
pub struct HttpSmsProvider {
    client: Client,
    endpoint: String,
    api_key: Option<String>,
    api_secret: Option<String>,
    sender_id: Option<String>,
}

impl HttpSmsProvider {
    pub fn new(config: &SmsConfig) -> Self {
        Self {
            client: Client::new(),
            endpoint: config.endpoint.trim().to_string(),
            api_key: (!config.api_key.trim().is_empty()).then(|| config.api_key.clone()),
            api_secret: (!config.api_secret.trim().is_empty()).then(|| config.api_secret.clone()),
            sender_id: (!config.sender_id.trim().is_empty()).then(|| config.sender_id.clone()),
        }
    }
}

#[derive(Debug, Serialize)]
struct HttpSmsRequest<'a> {
    to: &'a str,
    content: &'a str,
    provider: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    sender_id: Option<&'a str>,
}

#[async_trait]
impl SmsProvider for HttpSmsProvider {
    async fn send(&self, to: &str, content: &str) -> Result<(), ApiError> {
        let payload =
            HttpSmsRequest { to, content, provider: self.provider_name(), sender_id: self.sender_id.as_deref() };

        let mut request = self.client.post(&self.endpoint).json(&payload);
        if let Some(api_key) = &self.api_key {
            request = request.header("x-api-key", api_key);
        }
        if let Some(api_secret) = &self.api_secret {
            request = request.header("x-api-secret", api_secret);
        }

        let response =
            request.send().await.map_err(|e| ApiError::internal_with_log("Failed to call captcha SMS provider", &e))?;

        if response.status().is_success() {
            return Ok(());
        }

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let body = body.trim();
        let detail = if body.is_empty() {
            format!("HTTP {}", status.as_u16())
        } else {
            let truncated = if body.len() > 160 { format!("{}...", &body[..160]) } else { body.to_string() };
            format!("HTTP {}: {}", status.as_u16(), truncated)
        };

        Err(ApiError::internal(format!("Captcha SMS provider request failed: {detail}")))
    }

    fn provider_name(&self) -> &'static str {
        "http"
    }
}

/// Factory: create an SMS provider from configuration.
///
/// When `config.enabled` is false or the provider type is unrecognised,
/// a `NoopSmsProvider` is returned (safe fallback).
pub fn create_sms_provider(config: &SmsConfig) -> Box<dyn SmsProvider> {
    if !config.enabled {
        return Box::new(NoopSmsProvider);
    }

    match config.provider.trim() {
        "aliyun" => {
            if config.api_key.is_empty() || config.api_secret.is_empty() {
                warn!("sms provider is enabled with provider=aliyun but credentials are missing; falling back to noop");
                Box::new(NoopSmsProvider)
            } else if config.sender_id.is_empty() || config.template_code.is_empty() {
                warn!("sms provider is enabled with provider=aliyun but sign_name or template_code is missing; falling back to noop");
                Box::new(NoopSmsProvider)
            } else {
                Box::new(aliyun::AliyunSmsProvider::new(config))
            }
        }
        "http" | "generic_http" | "generic-http" => {
            if config.endpoint.trim().is_empty() {
                warn!(
                    "sms provider is enabled with provider=http but endpoint is empty; falling back to noop provider"
                );
                Box::new(NoopSmsProvider)
            } else {
                Box::new(HttpSmsProvider::new(config))
            }
        }
        "" => {
            warn!("sms provider is enabled but provider type is empty; falling back to noop provider");
            Box::new(NoopSmsProvider)
        }
        provider => {
            warn!(provider = %provider, "sms provider is not recognised; falling back to noop provider");
            Box::new(NoopSmsProvider)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_noop_sms_provider_returns_not_implemented() {
        let provider = NoopSmsProvider;
        let result = provider.send("+8613800138000", "test code").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not configured"), "expected not-implemented message, got: {msg}");
    }

    #[tokio::test]
    async fn test_noop_sms_provider_name() {
        let provider = NoopSmsProvider;
        assert_eq!(provider.provider_name(), "noop");
    }

    #[test]
    fn test_create_sms_provider_disabled_config_returns_noop() {
        let config = SmsConfig { enabled: false, ..Default::default() };
        let provider = create_sms_provider(&config);
        assert_eq!(provider.provider_name(), "noop");
    }

    #[test]
    fn test_create_sms_provider_unknown_provider_returns_noop() {
        let config = SmsConfig { enabled: true, provider: "unknown".to_string(), ..Default::default() };
        let provider = create_sms_provider(&config);
        assert_eq!(provider.provider_name(), "noop");
    }

    #[test]
    fn test_create_sms_provider_http_returns_http_provider() {
        let config = SmsConfig {
            enabled: true,
            provider: "http".to_string(),
            endpoint: "http://127.0.0.1:18080/sms".to_string(),
            ..Default::default()
        };
        let provider = create_sms_provider(&config);
        assert_eq!(provider.provider_name(), "http");
    }

    #[test]
    fn test_create_sms_provider_aliyun_returns_aliyun_provider() {
        let config = SmsConfig {
            enabled: true,
            provider: "aliyun".to_string(),
            api_key: "test-key".to_string(),
            api_secret: "test-secret".to_string(),
            sender_id: "TestSign".to_string(),
            template_code: "SMS_001".to_string(),
            ..Default::default()
        };
        let provider = create_sms_provider(&config);
        assert_eq!(provider.provider_name(), "aliyun");
    }

    #[test]
    fn test_create_sms_provider_aliyun_missing_credentials_returns_noop() {
        let config = SmsConfig { enabled: true, provider: "aliyun".to_string(), ..Default::default() };
        let provider = create_sms_provider(&config);
        assert_eq!(provider.provider_name(), "noop");
    }

    #[tokio::test]
    async fn test_http_sms_provider_sends_expected_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/sms"))
            .and(header("x-api-key", "test-key"))
            .and(header("x-api-secret", "test-secret"))
            .and(body_json(json!({
                "to": "+8613800138000",
                "content": "code 123456",
                "provider": "http",
                "sender_id": "synapse"
            })))
            .respond_with(ResponseTemplate::new(202))
            .mount(&server)
            .await;

        let provider = HttpSmsProvider::new(&SmsConfig {
            enabled: true,
            provider: "http".to_string(),
            endpoint: format!("{}/sms", server.uri()),
            api_key: "test-key".to_string(),
            api_secret: "test-secret".to_string(),
            sender_id: "synapse".to_string(),
            ..Default::default()
        });

        provider.send("+8613800138000", "code 123456").await.expect("http sms provider should accept 202");
    }

    #[tokio::test]
    async fn test_http_sms_provider_surfaces_non_success_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/sms"))
            .respond_with(ResponseTemplate::new(502).set_body_string("upstream unavailable"))
            .mount(&server)
            .await;

        let provider = HttpSmsProvider::new(&SmsConfig {
            enabled: true,
            provider: "http".to_string(),
            endpoint: format!("{}/sms", server.uri()),
            ..Default::default()
        });

        let err =
            provider.send("+8613800138000", "code 123456").await.expect_err("non-2xx provider response should fail");
        assert!(err.is_internal());
        assert!(err.to_string().contains("HTTP 502"));
    }
}
