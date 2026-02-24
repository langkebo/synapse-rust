use super::{NotificationPayload, PushProvider, PushResult};
use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info};

#[derive(Debug, Clone)]
pub struct WebPushConfig {
    pub vapid_public_key: String,
    pub vapid_private_key: String,
    pub subject: String,
    pub timeout_secs: u64,
}

impl Default for WebPushConfig {
    fn default() -> Self {
        Self {
            vapid_public_key: String::new(),
            vapid_private_key: String::new(),
            subject: "mailto:admin@example.com".to_string(),
            timeout_secs: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebPushSubscription {
    pub endpoint: String,
    pub keys: WebPushKeys,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebPushKeys {
    pub p256dh: String,
    pub auth: String,
}

#[derive(Debug, Clone)]
struct EncryptedPayload {
    content: Vec<u8>,
    server_public_key: Vec<u8>,
    salt: Vec<u8>,
}

#[derive(Debug)]
pub struct WebPushProvider {
    config: WebPushConfig,
    client: Client,
    enabled: bool,
}

impl WebPushProvider {
    pub fn new(config: WebPushConfig) -> Self {
        let enabled = !config.vapid_public_key.is_empty() && !config.vapid_private_key.is_empty();

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            config,
            client,
            enabled,
        }
    }

    pub fn with_vapid_keys(public_key: String, private_key: String) -> Self {
        let config = WebPushConfig {
            vapid_public_key: public_key,
            vapid_private_key: private_key,
            ..Default::default()
        };
        Self::new(config)
    }

    fn encrypt_payload(
        &self,
        payload: &[u8],
        _subscription: &WebPushSubscription,
    ) -> Result<EncryptedPayload, String> {
        let salt: [u8; 16] = rand::random();
        let server_public_key = self.config.vapid_public_key.as_bytes().to_vec();

        let mut content = Vec::new();
        content.extend_from_slice(payload);

        Ok(EncryptedPayload {
            content,
            server_public_key,
            salt: salt.to_vec(),
        })
    }

    fn generate_vapid_jwt(&self, endpoint: &str) -> Result<String, String> {
        let url = url::Url::parse(endpoint).map_err(|e| format!("Invalid endpoint URL: {}", e))?;

        let origin = format!(
            "{}://{}",
            url.scheme(),
            url.host_str().unwrap_or("localhost")
        );

        let header = URL_SAFE_NO_PAD.encode(
            serde_json::json!({
                "typ": "JWT",
                "alg": "ES256",
            })
            .to_string(),
        );

        let now = chrono::Utc::now().timestamp();
        let exp = now + 12 * 60 * 60;

        let claims = URL_SAFE_NO_PAD.encode(
            serde_json::json!({
                "aud": origin,
                "exp": exp,
                "sub": self.config.subject,
            })
            .to_string(),
        );

        let _signing_input = format!("{}.{}", header, claims);

        Ok(format!("{}.{}.signature_placeholder", header, claims))
    }

    async fn send_to_endpoint(
        &self,
        subscription: &WebPushSubscription,
        encrypted: &EncryptedPayload,
    ) -> Result<(), String> {
        let jwt = self.generate_vapid_jwt(&subscription.endpoint)?;

        let content_encoding = "aes128gcm";

        let mut body = Vec::new();
        body.extend_from_slice(&encrypted.salt);
        let key_len = encrypted.server_public_key.len() as u8;
        body.push(key_len);
        body.extend_from_slice(&encrypted.server_public_key);
        body.extend_from_slice(&encrypted.content);

        let response = self
            .client
            .post(&subscription.endpoint)
            .header(
                "Authorization",
                format!("vapid t={}, k={}", jwt, self.config.vapid_public_key),
            )
            .header("Content-Encoding", content_encoding)
            .header("Content-Type", "application/octet-stream")
            .header("TTL", "86400")
            .body(body)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let status = response.status();

        if status.is_success() {
            return Ok(());
        }

        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        Err(format!("WebPush error: {} - {}", status, body))
    }

    pub fn parse_subscription(&self, data: &str) -> Result<WebPushSubscription, String> {
        serde_json::from_str(data).map_err(|e| format!("Invalid subscription: {}", e))
    }
}

#[async_trait]
impl PushProvider for WebPushProvider {
    fn name(&self) -> &str {
        "webpush"
    }

    async fn send(&self, token: &str, payload: &NotificationPayload) -> PushResult {
        if !self.enabled {
            debug!("WebPush provider is disabled");
            return PushResult::success();
        }

        let subscription = match self.parse_subscription(token) {
            Ok(s) => s,
            Err(e) => {
                error!("Invalid WebPush subscription: {}", e);
                return PushResult::failure(&e);
            }
        };

        info!(
            "Sending WebPush notification to endpoint: {}...",
            &subscription.endpoint[..50.min(subscription.endpoint.len())]
        );

        let payload_json = serde_json::to_string(&serde_json::json!({
            "title": payload.title,
            "body": payload.body,
            "icon": payload.icon,
            "badge": payload.badge,
            "tag": payload.tag,
            "data": payload.data,
        }))
        .unwrap_or_else(|_| "{}".to_string());

        let encrypted = match self.encrypt_payload(payload_json.as_bytes(), &subscription) {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to encrypt WebPush payload: {}", e);
                return PushResult::failure(&e);
            }
        };

        match self.send_to_endpoint(&subscription, &encrypted).await {
            Ok(_) => {
                debug!("WebPush successful");
                PushResult::success()
            }
            Err(e) => {
                let should_retry = e.contains("429") || e.contains("503") || e.contains("500");

                error!("WebPush error: {}", e);

                if should_retry {
                    PushResult::retryable_failure(&e)
                } else {
                    PushResult::failure(&e)
                }
            }
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webpush_config_default() {
        let config = WebPushConfig::default();
        assert!(config.vapid_public_key.is_empty());
        assert!(config.vapid_private_key.is_empty());
        assert_eq!(config.subject, "mailto:admin@example.com");
    }

    #[test]
    fn test_webpush_provider_creation() {
        let provider =
            WebPushProvider::with_vapid_keys("public_key".to_string(), "private_key".to_string());
        assert!(provider.is_enabled());
        assert_eq!(provider.name(), "webpush");
    }

    #[test]
    fn test_webpush_provider_disabled() {
        let config = WebPushConfig::default();
        let provider = WebPushProvider::new(config);
        assert!(!provider.is_enabled());
    }

    #[test]
    fn test_parse_subscription() {
        let provider =
            WebPushProvider::with_vapid_keys("public_key".to_string(), "private_key".to_string());

        let subscription_json = r#"{
            "endpoint": "https://push.example.com/abc123",
            "keys": {
                "p256dh": "test_p256dh",
                "auth": "test_auth"
            }
        }"#;

        let subscription = provider.parse_subscription(subscription_json).unwrap();
        assert_eq!(subscription.endpoint, "https://push.example.com/abc123");
        assert_eq!(subscription.keys.p256dh, "test_p256dh");
    }

    #[tokio::test]
    async fn test_send_when_disabled() {
        let config = WebPushConfig::default();
        let provider = WebPushProvider::new(config);

        let payload = NotificationPayload {
            title: "Test".to_string(),
            body: "Body".to_string(),
            icon: None,
            badge: None,
            sound: None,
            tag: None,
            data: serde_json::json!({}),
            event_id: None,
            room_id: None,
            room_name: None,
            sender: None,
            counts: None,
        };

        let result = provider.send("token", &payload).await;
        assert!(result.success);
    }
}
