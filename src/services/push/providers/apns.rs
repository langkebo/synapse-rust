use super::{NotificationPayload, PushProvider, PushResult};
use async_trait::async_trait;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;
use tracing::{debug, error, info};

#[derive(Debug, Clone)]
pub struct ApnsConfig {
    pub topic: String,
    pub endpoint: String,
    pub key_id: Option<String>,
    pub team_id: Option<String>,
    pub private_key: Option<String>,
    pub timeout_secs: u64,
}

impl Default for ApnsConfig {
    fn default() -> Self {
        Self {
            topic: String::new(),
            endpoint: "https://api.push.apple.com".to_string(),
            key_id: None,
            team_id: None,
            private_key: None,
            timeout_secs: 30,
        }
    }
}

impl ApnsConfig {
    pub fn sandbox() -> Self {
        Self {
            endpoint: "https://api.sandbox.push.apple.com".to_string(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ApnsPayload {
    aps: ApnsAps,
}

#[derive(Debug, Clone, Serialize)]
struct ApnsAps {
    alert: ApnsAlert,
    #[serde(skip_serializing_if = "Option::is_none")]
    badge: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sound: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_available: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mutable_content: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
struct ApnsAlert {
    title: String,
    body: String,
}

#[derive(Debug, Clone, Serialize)]
struct ApnsJwtClaims {
    iss: String,
    iat: i64,
}

#[derive(Debug)]
pub struct ApnsProvider {
    config: ApnsConfig,
    client: Client,
    enabled: bool,
}

impl ApnsProvider {
    pub fn new(config: ApnsConfig) -> Self {
        let enabled = !config.topic.is_empty();

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

    pub fn with_topic(topic: String) -> Self {
        let config = ApnsConfig {
            topic,
            ..Default::default()
        };
        Self::new(config)
    }

    fn build_payload(&self, payload: &NotificationPayload) -> ApnsPayload {
        let badge = payload.counts.as_ref().map(|c| c.unread);

        ApnsPayload {
            aps: ApnsAps {
                alert: ApnsAlert {
                    title: payload.title.clone(),
                    body: payload.body.clone(),
                },
                badge,
                sound: payload.sound.clone(),
                content_available: Some(1),
                mutable_content: Some(1),
            },
        }
    }

    fn generate_jwt(&self) -> Result<String, String> {
        let key_id = self
            .config
            .key_id
            .clone()
            .ok_or_else(|| "APNS key_id not configured".to_string())?;
        let team_id = self
            .config
            .team_id
            .clone()
            .ok_or_else(|| "APNS team_id not configured".to_string())?;
        let private_key = self
            .config
            .private_key
            .as_deref()
            .ok_or_else(|| "APNS private_key not configured".to_string())?;

        if !private_key.contains("BEGIN") {
            return Err("APNS JWT credentials not configured".to_string());
        }

        let now = chrono::Utc::now().timestamp();
        let claims = ApnsJwtClaims {
            iss: team_id,
            iat: now,
        };

        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(key_id);

        let encoding_key = EncodingKey::from_ec_pem(private_key.as_bytes())
            .map_err(|e| format!("Invalid APNS EC private key: {}", e))?;

        encode(&header, &claims, &encoding_key)
            .map_err(|e| format!("Failed to sign APNS JWT: {}", e))
    }

    async fn send_request(&self, token: &str, payload: &ApnsPayload) -> Result<(), String> {
        let url = format!("{}/3/device/{}", self.config.endpoint, token);

        let jwt = self.generate_jwt()?;

        let response = self
            .client
            .post(&url)
            .header("authorization", format!("bearer {}", jwt))
            .header("apns-topic", &self.config.topic)
            .header("apns-push-type", "alert")
            .header("apns-priority", "10")
            .header("content-type", "application/json")
            .json(payload)
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

        let error_info: serde_json::Value =
            serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({"reason": body}));

        let reason = error_info
            .get("reason")
            .and_then(|r| r.as_str())
            .unwrap_or("Unknown error");

        Err(format!("APNS error: {} - {}", status, reason))
    }
}

#[async_trait]
impl PushProvider for ApnsProvider {
    fn name(&self) -> &str {
        "apns"
    }

    async fn send(&self, token: &str, payload: &NotificationPayload) -> PushResult {
        if !self.enabled {
            debug!("APNS provider is disabled");
            return PushResult::success();
        }

        info!(
            "Sending APNS notification to token: {}...",
            &token[..20.min(token.len())]
        );

        let apns_payload = self.build_payload(payload);

        match self.send_request(token, &apns_payload).await {
            Ok(_) => {
                debug!("APNS push successful");
                PushResult::success()
            }
            Err(e) => {
                let should_retry = e.contains("InternalServerError")
                    || e.contains("ServiceUnavailable")
                    || e.contains("TooManyRequests");

                error!("APNS push error: {}", e);

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
    use crate::services::push::providers::NotificationCounts;

    const TEST_EC_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg/GwGC9O9l7Sbthjv\n\
v8PGbBpPXRyuIyQoooKWcdokN62hRANCAASrFgTXKOydK6UzmGQ/iGevi9IZWynS\n\
+cK8VH35KLhR93tYV48MUdE2UOl6yw7TLogf3lqlu+HohfI+xhYOfC6i\n\
-----END PRIVATE KEY-----\n";

    #[test]
    fn test_apns_config_default() {
        let config = ApnsConfig::default();
        assert_eq!(config.endpoint, "https://api.push.apple.com");
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_apns_config_sandbox() {
        let config = ApnsConfig::sandbox();
        assert_eq!(config.endpoint, "https://api.sandbox.push.apple.com");
    }

    #[test]
    fn test_apns_provider_creation() {
        let provider = ApnsProvider::with_topic("com.example.app".to_string());
        assert!(provider.is_enabled());
        assert_eq!(provider.name(), "apns");
    }

    #[test]
    fn test_apns_provider_disabled() {
        let config = ApnsConfig::default();
        let provider = ApnsProvider::new(config);
        assert!(!provider.is_enabled());
    }

    #[test]
    fn test_build_payload() {
        let provider = ApnsProvider::with_topic("com.example.app".to_string());
        let payload = NotificationPayload {
            title: "Test".to_string(),
            body: "Body".to_string(),
            icon: None,
            badge: None,
            sound: Some("default".to_string()),
            tag: None,
            data: serde_json::json!({}),
            event_id: None,
            room_id: None,
            room_name: None,
            sender: None,
            counts: Some(NotificationCounts {
                unread: 5,
                missed_calls: 0,
            }),
        };

        let apns_payload = provider.build_payload(&payload);
        assert_eq!(apns_payload.aps.alert.title, "Test");
        assert_eq!(apns_payload.aps.badge, Some(5));
        assert_eq!(apns_payload.aps.sound, Some("default".to_string()));
    }

    #[test]
    fn test_generate_jwt_signs_real_token() {
        let provider = ApnsProvider::new(ApnsConfig {
            topic: "com.example.app".to_string(),
            key_id: Some("ABC123DEFG".to_string()),
            team_id: Some("TEAM123456".to_string()),
            private_key: Some(TEST_EC_PRIVATE_KEY.to_string()),
            ..Default::default()
        });

        let jwt = provider.generate_jwt().expect("jwt should be signed");
        let segments: Vec<&str> = jwt.split('.').collect();

        assert_eq!(segments.len(), 3);
        assert!(segments.iter().all(|segment| !segment.is_empty()));
        assert!(!jwt.contains("placeholder"));
    }

    #[tokio::test]
    async fn test_send_when_disabled() {
        let config = ApnsConfig::default();
        let provider = ApnsProvider::new(config);

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
