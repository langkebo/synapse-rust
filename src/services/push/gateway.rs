use crate::error::ApiError;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info};

#[derive(Debug, Clone, Serialize)]
pub struct PushNotification {
    pub notification: NotificationContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devices: Option<Vec<PushDevice>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationContent {
    pub event_id: String,
    pub room_id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub sender: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_is_target: Option<bool>,
    pub counts: NotificationCounts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devices: Option<Vec<PushDeviceContent>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationCounts {
    pub missed_calls: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unread: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PushDevice {
    pub app_id: String,
    pub pushkey: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pushkey_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tweaks: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PushDeviceContent {
    pub app_id: String,
    pub pushkey: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PushGatewayResponse {
    pub rejected: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PushGatewayConfig {
    pub timeout_secs: u64,
    pub max_retries: u32,
}

impl Default for PushGatewayConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            max_retries: 3,
        }
    }
}

#[derive(Debug)]
pub struct PushGateway {
    client: Client,
    #[allow(dead_code)]
    config: PushGatewayConfig,
}

impl PushGateway {
    pub fn new(config: PushGatewayConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { client, config }
    }

    pub async fn send_notification(
        &self,
        gateway_url: &str,
        notification: &PushNotification,
    ) -> Result<PushGatewayResponse, ApiError> {
        info!("Sending notification to push gateway: {}", gateway_url);

        let response = self
            .client
            .post(gateway_url)
            .header("Content-Type", "application/json")
            .json(notification)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to send to gateway: {}", e)))?;

        let status = response.status();

        if !status.is_success() {
            let body = response
                .text()
                .await
                .map_err(|e| ApiError::internal(format!("Failed to read response: {}", e)))?;

            error!("Push gateway returned error: {} - {}", status, body);
            return Err(ApiError::internal(format!(
                "Push gateway error: {}",
                status
            )));
        }

        let gateway_response: PushGatewayResponse = response
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to parse gateway response: {}", e)))?;

        debug!(
            "Push gateway response: rejected {} devices",
            gateway_response.rejected.len()
        );

        Ok(gateway_response)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_notification(
        &self,
        event_id: &str,
        room_id: &str,
        event_type: &str,
        sender: &str,
        unread_count: u32,
        missed_calls: u32,
        devices: Vec<PushDevice>,
    ) -> PushNotification {
        PushNotification {
            notification: NotificationContent {
                event_id: event_id.to_string(),
                room_id: room_id.to_string(),
                event_type: event_type.to_string(),
                sender: sender.to_string(),
                room_name: None,
                room_alias: None,
                user_is_target: None,
                counts: NotificationCounts {
                    missed_calls,
                    unread: Some(unread_count),
                },
                devices: None,
            },
            devices: Some(devices),
        }
    }

    pub fn build_device(
        &self,
        app_id: &str,
        pushkey: &str,
        data: Option<serde_json::Value>,
        tweaks: Option<serde_json::Value>,
    ) -> PushDevice {
        PushDevice {
            app_id: app_id.to_string(),
            pushkey: pushkey.to_string(),
            pushkey_ts: None,
            data,
            tweaks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_gateway_config_default() {
        let config = PushGatewayConfig::default();
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_build_notification() {
        let gateway = PushGateway::new(PushGatewayConfig::default());

        let notification = gateway.build_notification(
            "event123",
            "room123",
            "m.room.message",
            "@user:example.com",
            5,
            0,
            vec![],
        );

        assert_eq!(notification.notification.event_id, "event123");
        assert_eq!(notification.notification.room_id, "room123");
        assert_eq!(notification.notification.counts.unread, Some(5));
    }

    #[test]
    fn test_build_device() {
        let gateway = PushGateway::new(PushGatewayConfig::default());

        let device = gateway.build_device(
            "com.example.app",
            "pushkey123",
            Some(serde_json::json!({"key": "value"})),
            Some(serde_json::json!({"sound": true})),
        );

        assert_eq!(device.app_id, "com.example.app");
        assert_eq!(device.pushkey, "pushkey123");
        assert!(device.data.is_some());
        assert!(device.tweaks.is_some());
    }

    #[test]
    fn test_notification_counts_serialization() {
        let counts = NotificationCounts {
            missed_calls: 0,
            unread: Some(5),
        };

        let json = serde_json::to_string(&counts).unwrap();
        assert!(json.contains("missed_calls"));
        assert!(json.contains("unread"));
    }

    #[test]
    fn test_push_device_serialization() {
        let device = PushDevice {
            app_id: "com.example.app".to_string(),
            pushkey: "key123".to_string(),
            pushkey_ts: Some(1234567890),
            data: None,
            tweaks: None,
        };

        let json = serde_json::to_string(&device).unwrap();
        assert!(json.contains("app_id"));
        assert!(json.contains("pushkey"));
    }
}
