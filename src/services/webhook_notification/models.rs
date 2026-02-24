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
            events: vec![
                WebhookEventType::UserLogin,
                WebhookEventType::UserLogout,
                WebhookEventType::UserFailedLogin,
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDeliveryResult {
    pub success: bool,
    pub status_code: Option<u16>,
    pub response_body: Option<String>,
    pub attempts: u32,
    pub error_message: Option<String>,
}
