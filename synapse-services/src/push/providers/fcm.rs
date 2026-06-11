use super::{NotificationPayload, PushGatewayType, PushProvider, PushResult};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct FcmProviderConfig {
    pub api_key: String,
    pub endpoint: String,
    pub timeout_secs: u64,
    pub max_retries: u32,
}

impl Default for FcmProviderConfig {
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
struct FcmResponse {
    #[serde(default)]
    #[serde(rename = "success")]
    _success: u32,
    #[serde(default)]
    failure: u32,
    #[serde(default)]
    results: Vec<FcmResult>,
    #[serde(default)]
    multicast_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct FcmResult {
    #[serde(default)]
    #[serde(rename = "message_id")]
    _message_id: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug)]
pub struct FcmProvider {
    config: FcmProviderConfig,
    client: Client,
    enabled: bool,
}

impl FcmProvider {
    pub fn new(config: FcmProviderConfig) -> Self {
        let enabled = !config.api_key.is_empty();

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { config, client, enabled }
    }

    pub fn with_api_key(api_key: String) -> Self {
        let config = FcmProviderConfig { api_key, ..Default::default() };
        Self::new(config)
    }

    fn build_message(token: &str, payload: &NotificationPayload) -> FcmMessage {
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
            data: if payload.data.is_null() { None } else { Some(payload.data.clone()) },
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
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        let status = response.status();
        let body = response.text().await.map_err(|e| format!("Failed to read response: {e}"))?;

        if !status.is_success() {
            error!(
                %status,
                response_body_present = !body.is_empty(),
                response_body_len = body.len(),
                "FCM request failed"
            );
            return Err(format!("FCM returned status {status}: {body}"));
        }

        serde_json::from_str(&body).map_err(|e| format!("Failed to parse FCM response: {e} - Body: {body}"))
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
            token_present = !token.is_empty(),
            token_len = token.len(),
            title_present = !payload.title.is_empty(),
            room_id = payload.room_id,
            event_id = payload.event_id,
            "Sending FCM notification"
        );

        let message = Self::build_message(token, payload);

        match self.send_request(&message).await {
            Ok(response) => {
                if response.failure > 0 {
                    if let Some(result) = response.results.first() {
                        if let Some(error) = &result.error {
                            warn!(%error, title_present = !payload.title.is_empty(), room_id = payload.room_id, event_id = payload.event_id, "FCM push failed");

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

                debug!(multicast_id = ?response.multicast_id, title_present = !payload.title.is_empty(), room_id = payload.room_id, event_id = payload.event_id, "FCM push successful");
                PushResult::success_with_response(&format!("multicast_id:{:?}", response.multicast_id))
            }
            Err(e) => {
                error!(%e, title_present = !payload.title.is_empty(), room_id = payload.room_id, event_id = payload.event_id, "FCM push error");
                PushResult::retryable_failure(&e)
            }
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn gateway_type(&self) -> PushGatewayType {
        PushGatewayType::Fcm
    }

    fn endpoint(&self) -> &str {
        &self.config.endpoint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fcm_provider_config_default() {
        let config = FcmProviderConfig::default();
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
        let config = FcmProviderConfig::default();
        let provider = FcmProvider::new(config);
        assert!(!provider.is_enabled());
    }

    #[test]
    fn test_build_message() {
        let _provider = FcmProvider::with_api_key("test_key".to_string());
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

        let message = FcmProvider::build_message("token123", &payload);
        assert_eq!(message.to, "token123");
        assert_eq!(message.notification.title, "Test");
        assert!(message.data.is_some());
    }

    #[tokio::test]
    async fn test_send_when_disabled() {
        let config = FcmProviderConfig::default();
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
        assert!(result.is_success);
    }
}
