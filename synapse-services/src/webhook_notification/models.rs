use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub event_type: WebhookEventType,
    pub timestamp: i64,
    pub payload: WebhookPayload,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEventType {
    UserLogin,
    UserLogout,
    UserFailedLogin,
    UserPasswordChanged,
    UserAccountCreated,
    UserAccountDeleted,
    UserDeviceAdded,
    UserDeviceRemoved,
    UserTokenRefreshed,
}

impl WebhookEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            WebhookEventType::UserLogin => "user_login",
            WebhookEventType::UserLogout => "user_logout",
            WebhookEventType::UserFailedLogin => "user_failed_login",
            WebhookEventType::UserPasswordChanged => "user_password_changed",
            WebhookEventType::UserAccountCreated => "user_account_created",
            WebhookEventType::UserAccountDeleted => "user_account_deleted",
            WebhookEventType::UserDeviceAdded => "user_device_added",
            WebhookEventType::UserDeviceRemoved => "user_device_removed",
            WebhookEventType::UserTokenRefreshed => "user_token_refreshed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    pub user_id: String,
    pub device_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub country: Option<String>,
    pub city: Option<String>,
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub enabled: bool,
    pub url: String,
    pub secret: Option<String>,
    pub timeout_ms: u64,
    pub retry_count: u32,
    pub retry_delay_ms: u64,
    pub events: Vec<WebhookEventType>,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: String::new(),
            secret: None,
            timeout_ms: 5000,
            retry_count: 3,
            retry_delay_ms: 1000,
            events: vec![WebhookEventType::UserLogin, WebhookEventType::UserLogout, WebhookEventType::UserFailedLogin],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDeliveryResult {
    #[serde(rename = "success")]
    pub is_success: bool,
    pub status_code: Option<u16>,
    pub response_body: Option<String>,
    pub attempts: u32,
    pub error_message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_event_type_as_str() {
        assert_eq!(WebhookEventType::UserLogin.as_str(), "user_login");
        assert_eq!(WebhookEventType::UserLogout.as_str(), "user_logout");
        assert_eq!(WebhookEventType::UserFailedLogin.as_str(), "user_failed_login");
        assert_eq!(WebhookEventType::UserPasswordChanged.as_str(), "user_password_changed");
        assert_eq!(WebhookEventType::UserAccountCreated.as_str(), "user_account_created");
        assert_eq!(WebhookEventType::UserAccountDeleted.as_str(), "user_account_deleted");
        assert_eq!(WebhookEventType::UserDeviceAdded.as_str(), "user_device_added");
        assert_eq!(WebhookEventType::UserDeviceRemoved.as_str(), "user_device_removed");
        assert_eq!(WebhookEventType::UserTokenRefreshed.as_str(), "user_token_refreshed");
    }

    #[test]
    fn test_webhook_event_type_all_variants() {
        let variants = [
            WebhookEventType::UserLogin,
            WebhookEventType::UserLogout,
            WebhookEventType::UserFailedLogin,
            WebhookEventType::UserPasswordChanged,
            WebhookEventType::UserAccountCreated,
            WebhookEventType::UserAccountDeleted,
            WebhookEventType::UserDeviceAdded,
            WebhookEventType::UserDeviceRemoved,
            WebhookEventType::UserTokenRefreshed,
        ];
        for v in &variants {
            assert!(!v.as_str().is_empty());
        }
    }

    #[test]
    fn test_webhook_event_serialization() {
        let event = WebhookEvent {
            event_type: WebhookEventType::UserLogin,
            timestamp: 1700000000000,
            payload: WebhookPayload {
                user_id: "@user:example.com".to_string(),
                device_id: Some("device1".to_string()),
                ip_address: Some("1.2.3.4".to_string()),
                user_agent: None,
                country: None,
                city: None,
                extra: None,
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("user_login"));
        assert!(json.contains("@user:example.com"));
    }

    #[test]
    fn test_webhook_config_default() {
        let config = WebhookConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.url, "");
        assert!(config.secret.is_none());
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.retry_count, 3);
        assert_eq!(config.retry_delay_ms, 1000);
        assert_eq!(config.events.len(), 3);
    }

    #[test]
    fn test_webhook_config_custom() {
        let config = WebhookConfig {
            enabled: true,
            url: "https://hooks.example.com/webhook".to_string(),
            secret: Some("secret123".to_string()),
            timeout_ms: 10000,
            retry_count: 5,
            retry_delay_ms: 2000,
            events: vec![WebhookEventType::UserLogin, WebhookEventType::UserPasswordChanged],
        };
        assert!(config.enabled);
        assert_eq!(config.url, "https://hooks.example.com/webhook");
        assert_eq!(config.secret.as_deref(), Some("secret123"));
        assert_eq!(config.timeout_ms, 10000);
        assert_eq!(config.retry_count, 5);
        assert_eq!(config.events.len(), 2);
    }

    #[test]
    fn test_webhook_payload() {
        let payload = WebhookPayload {
            user_id: "@user:example.com".to_string(),
            device_id: Some("device_id".to_string()),
            ip_address: Some("10.0.0.1".to_string()),
            user_agent: Some("Matrix/1.0".to_string()),
            country: Some("CN".to_string()),
            city: Some("Beijing".to_string()),
            extra: Some(serde_json::json!({"key": "value"})),
        };
        assert_eq!(payload.user_id, "@user:example.com");
        assert_eq!(payload.device_id.as_deref(), Some("device_id"));
        assert_eq!(payload.country.as_deref(), Some("CN"));
    }

    #[test]
    fn test_webhook_delivery_result_success() {
        let result = WebhookDeliveryResult {
            is_success: true,
            status_code: Some(200),
            response_body: Some("OK".to_string()),
            attempts: 1,
            error_message: None,
        };
        assert!(result.is_success);
        assert_eq!(result.status_code, Some(200));
        assert_eq!(result.attempts, 1);
    }

    #[test]
    fn test_webhook_delivery_result_failure() {
        let result = WebhookDeliveryResult {
            is_success: false,
            status_code: Some(500),
            response_body: None,
            attempts: 3,
            error_message: Some("Internal server error".to_string()),
        };
        assert!(!result.is_success);
        assert_eq!(result.status_code, Some(500));
        assert_eq!(result.attempts, 3);
        assert_eq!(result.error_message.as_deref(), Some("Internal server error"));
    }
}
