mod apns;
mod fcm;
mod webpush;

pub use apns::ApnsProvider;
pub use fcm::FcmProvider;
pub use webpush::WebPushProvider;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Enum representing the type of push gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PushGatewayType {
    Apns,
    Fcm,
    WebPush,
}

impl std::fmt::Display for PushGatewayType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PushGatewayType::Apns => write!(f, "apns"),
            PushGatewayType::Fcm => write!(f, "fcm"),
            PushGatewayType::WebPush => write!(f, "webpush"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResult {
    #[serde(rename = "success")]
    pub is_success: bool,
    pub error: Option<String>,
    pub provider_response: Option<String>,
    pub should_retry: bool,
}

impl PushResult {
    pub fn success() -> Self {
        Self { is_success: true, error: None, provider_response: None, should_retry: false }
    }

    pub fn success_with_response(response: &str) -> Self {
        Self { is_success: true, error: None, provider_response: Some(response.to_string()), should_retry: false }
    }

    pub fn failure(error: &str) -> Self {
        Self { is_success: false, error: Some(error.to_string()), provider_response: None, should_retry: false }
    }

    pub fn retryable_failure(error: &str) -> Self {
        Self { is_success: false, error: Some(error.to_string()), provider_response: None, should_retry: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPayload {
    pub title: String,
    pub body: String,
    pub icon: Option<String>,
    pub badge: Option<String>,
    pub sound: Option<String>,
    pub tag: Option<String>,
    pub data: serde_json::Value,
    pub event_id: Option<String>,
    pub room_id: Option<String>,
    pub room_name: Option<String>,
    pub sender: Option<String>,
    pub counts: Option<NotificationCounts>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationCounts {
    pub unread: u32,
    pub missed_calls: u32,
}

#[async_trait]
pub trait PushProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn send(&self, token: &str, payload: &NotificationPayload) -> PushResult;

    async fn send_batch(&self, messages: Vec<(String, NotificationPayload)>) -> Vec<(String, PushResult)> {
        let mut results = Vec::new();
        for (token, payload) in messages {
            let result = self.send(&token, &payload).await;
            results.push((token, result));
        }
        results
    }

    fn is_enabled(&self) -> bool;

    /// Returns the type of this push gateway.
    fn gateway_type(&self) -> PushGatewayType;

    /// Returns the configured endpoint URL for this push gateway.
    fn endpoint(&self) -> &str;
}
