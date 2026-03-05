use super::{NotificationPayload, PushProvider, PushResult};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct FcmConfig {
    pub api_key: String,
    pub endpoint: String,
    pub timeout_secs: u64,
    pub max_retries: u32,
}

impl Default for FcmConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            endpoint: "https://fcm.googleapis.com/fcm/send".to_string(),
            timeout_secs: 30,
            max_retries: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct FcmMessage {
    to: String,
    notification: FcmNotification,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    priority: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_available: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct FcmNotification {
    title: String,
    body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    badge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sound: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct FcmResponse {
    #[serde(default)]
    success: u32,
    #[serde(default)]
    failure: u32,
    #[serde(default)]
    results: Vec<FcmResult>,
    #[serde(default)]
    multicast_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct FcmResult {
    #[serde(default)]
    message_id: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug)]
pub struct FcmProvider {
    config: FcmConfig,
    client: Client,
    enabled: bool,
}

impl FcmProvider {
    pub fn new(config: FcmConfig) -> Self {
        let enabled = !config.api_key.is_empty();

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

    pub fn with_api_key(api_key: String) -> Self {
        let config = FcmConfig {
            api_key,
            ..Default::default()
        };
        Self::new(config)
    }

    fn build_message(&self, token: &str, payload: &NotificationPayload) -> FcmMessage {
        FcmMessage {
            to: token.to_string(),
            notification: FcmNotification {
                title: payload.title.clone(),
                body: payload.body.clone(),
                icon: payload.icon.clone(),
                badge: payload.badge.clone(),
                sound: payload.sound.clone(),
                tag: payload.tag.clone(),
            },
            data: if payload.data.is_null() {
                None
            } else {
                Some(payload.data.clone())
            },
            priority: "high".to_string(),
            content_available: Some(true),
        }
    }

    async fn send_request(&self, message: &FcmMessage) -> Result<FcmResponse, String> {
        let response = self
            .client
            .post(&self.config.endpoint)
            .header("Authorization", format!("key={}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(message)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if !status.is_success() {
            error!("FCM request failed with status {}: {}", status, body);
            return Err(format!("FCM returned status {}: {}", status, body));
        }

        serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse FCM response: {} - Body: {}", e, body))
    }
}

#[async_trait]
impl PushProvider for FcmProvider {
    fn name(&self) -> &str {
        "fcm"
    }

    async fn send(&self, token: &str, payload: &NotificationPayload) -> PushResult {
        if !self.enabled {
            debug!("FCM provider is disabled");
            return PushResult::success();
        }

        info!(
            "Sending FCM notification to token: {}...",
            &token[..20.min(token.len())]
        );

        let message = self.build_message(token, payload);

        match self.send_request(&message).await {
            Ok(response) => {
                if response.failure > 0 {
                    if let Some(result) = response.results.first() {
                        if let Some(error) = &result.error {
                            warn!("FCM push failed: {}", error);

                            let should_retry = matches!(
                                error.as_str(),
                                "Unavailable" | "InternalServerError" | "DeviceMessageRateExceeded"
                            );

                            if should_retry {
                                return PushResult::retryable_failure(error);
                            }
                            return PushResult::failure(error);
                        }
                    }
                }

                debug!(
                    "FCM push successful: multicast_id={:?}",
                    response.multicast_id
                );
                PushResult::success_with_response(&format!(
                    "multicast_id:{:?}",
                    response.multicast_id
                ))
            }
            Err(e) => {
                error!("FCM push error: {}", e);
                PushResult::retryable_failure(&e)
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
    fn test_fcm_config_default() {
        let config = FcmConfig::default();
        assert_eq!(config.endpoint, "https://fcm.googleapis.com/fcm/send");
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_fcm_provider_creation() {
        let provider = FcmProvider::with_api_key("test_key".to_string());
        assert!(provider.is_enabled());
        assert_eq!(provider.name(), "fcm");
    }

    #[test]
    fn test_fcm_provider_disabled() {
        let config = FcmConfig::default();
        let provider = FcmProvider::new(config);
        assert!(!provider.is_enabled());
    }

    #[test]
    fn test_build_message() {
        let provider = FcmProvider::with_api_key("test_key".to_string());
        let payload = NotificationPayload {
            title: "Test".to_string(),
            body: "Body".to_string(),
            icon: Some("icon.png".to_string()),
            badge: None,
            sound: None,
            tag: None,
            data: serde_json::json!({"key": "value"}),
            event_id: None,
            room_id: None,
            room_name: None,
            sender: None,
            counts: None,
        };

        let message = provider.build_message("token123", &payload);
        assert_eq!(message.to, "token123");
        assert_eq!(message.notification.title, "Test");
        assert!(message.data.is_some());
    }

    #[tokio::test]
    async fn test_send_when_disabled() {
        let config = FcmConfig::default();
        let provider = FcmProvider::new(config);

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
