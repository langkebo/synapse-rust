mod apns;
mod fcm;
mod webpush;

pub use apns::ApnsProvider;
pub use fcm::FcmProvider;
pub use webpush::WebPushProvider;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::warn;

/// Maximum number of retry attempts for transient push failures.
const MAX_PUSH_RETRIES: u32 = 3;

/// Base delay in milliseconds for exponential backoff between retries.
const RETRY_BASE_DELAY_MS: u64 = 1000;

/// Determines if an error from a push provider is retryable.
///
/// Checks for common transient error indicators across APNs, FCM, and WebPush
/// providers, including HTTP status codes (429, 500, 503) and provider-specific
/// error strings.
pub fn is_retryable_error(error: &str) -> bool {
    if error.contains("429") || error.contains("500") || error.contains("503") {
        return true;
    }
    if error.contains("InternalServerError")
        || error.contains("ServiceUnavailable")
        || error.contains("TooManyRequests")
    {
        return true;
    }
    if error.contains("Unavailable") || error.contains("DeviceMessageRateExceeded") {
        return true;
    }
    false
}

/// Send a push notification with automatic retry on transient failures.
///
/// Uses exponential backoff (1s, 2s, 4s) and retries up to `MAX_PUSH_RETRIES`
/// when the provider signals a retryable failure.
pub async fn send_with_retry<P: PushProvider + ?Sized>(
    provider: &P,
    token: &str,
    payload: &NotificationPayload,
) -> PushResult {
    let mut last_result = provider.send(token, payload).await;

    if last_result.is_success || !last_result.should_retry {
        return last_result;
    }

    for attempt in 1..=MAX_PUSH_RETRIES {
        let delay_ms = RETRY_BASE_DELAY_MS * 2u64.pow(attempt - 1);
        warn!(
            provider = provider.name(),
            token_present = !token.is_empty(),
            token_len = token.len(),
            title_present = !payload.title.is_empty(),
            room_id = ?payload.room_id,
            event_id = ?payload.event_id,
            attempt,
            max_retries = MAX_PUSH_RETRIES,
            delay_ms,
            error = %last_result.error.as_deref().unwrap_or("unknown error"),
            "Push send failed, retrying"
        );
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;

        last_result = provider.send(token, payload).await;

        if last_result.is_success || !last_result.should_retry {
            return last_result;
        }
    }

    warn!(
        provider = provider.name(),
        token_present = !token.is_empty(),
        token_len = token.len(),
        title_present = !payload.title.is_empty(),
        room_id = ?payload.room_id,
        event_id = ?payload.event_id,
        max_retries = MAX_PUSH_RETRIES,
        error = %last_result.error.as_deref().unwrap_or("unknown error"),
        "Push send failed after retries"
    );
    last_result
}

/// Enum representing the type of push gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PushGatewayType {
    /// The `Apns` variant.
    Apns,
    /// The `Fcm` variant.
    Fcm,
    /// The `WebPush` variant.
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

/// The `PushResult` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResult {
    #[serde(rename = "success")]
    /// The `is_success` field.
    pub is_success: bool,
    /// The `error` field.
    pub error: Option<String>,
    /// The `provider_response` field.
    pub provider_response: Option<String>,
    /// The `should_retry` field.
    pub should_retry: bool,
}

impl PushResult {
    /// See [`success`].
    pub fn success() -> Self {
        Self { is_success: true, error: None, provider_response: None, should_retry: false }
    }

    /// See [`success_with_response`].
    pub fn success_with_response(response: &str) -> Self {
        Self { is_success: true, error: None, provider_response: Some(response.to_string()), should_retry: false }
    }

    /// See [`failure`].
    pub fn failure(error: &str) -> Self {
        Self { is_success: false, error: Some(error.to_string()), provider_response: None, should_retry: false }
    }

    /// See [`retryable_failure`].
    pub fn retryable_failure(error: &str) -> Self {
        Self { is_success: false, error: Some(error.to_string()), provider_response: None, should_retry: true }
    }
}

/// The `NotificationPayload` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPayload {
    /// The `title` field.
    pub title: String,
    /// The `body` field.
    pub body: String,
    /// The `icon` field.
    pub icon: Option<String>,
    /// The `badge` field.
    pub badge: Option<String>,
    /// The `sound` field.
    pub sound: Option<String>,
    /// The `tag` field.
    pub tag: Option<String>,
    /// The `data` field.
    pub data: serde_json::Value,
    /// The `event_id` field.
    pub event_id: Option<String>,
    /// The `room_id` field.
    pub room_id: Option<String>,
    /// The `room_name` field.
    pub room_name: Option<String>,
    /// The `sender` field.
    pub sender: Option<String>,
    /// The `counts` field.
    pub counts: Option<NotificationCounts>,
}

/// The `NotificationCounts` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationCounts {
    /// The `unread` field.
    pub unread: u32,
    /// The `missed_calls` field.
    pub missed_calls: u32,
}

/// The `PushProvider` trait.
#[async_trait]
pub trait PushProvider: Send + Sync {
    /// See [`name`].
    fn name(&self) -> &str;

    /// See [`send`].
    async fn send(&self, token: &str, payload: &NotificationPayload) -> PushResult;

    /// See [`send_batch`].
    async fn send_batch(&self, messages: Vec<(String, NotificationPayload)>) -> Vec<(String, PushResult)> {
        let mut results = Vec::new();
        for (token, payload) in messages {
            let result = self.send(&token, &payload).await;
            results.push((token, result));
        }
        results
    }

    /// See [`is_enabled`].
    fn is_enabled(&self) -> bool;

    /// Returns the type of this push gateway.
    fn gateway_type(&self) -> PushGatewayType;

    /// Returns the configured endpoint URL for this push gateway.
    fn endpoint(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable_error_http_status_codes() {
        assert!(is_retryable_error("HTTP 429 Too Many Requests"));
        assert!(is_retryable_error("HTTP 500 Internal Server Error"));
        assert!(is_retryable_error("HTTP 503 Service Unavailable"));
    }

    #[test]
    fn test_is_retryable_error_apns() {
        assert!(is_retryable_error("APNS error: 500 - InternalServerError"));
        assert!(is_retryable_error("APNS error: 503 - ServiceUnavailable"));
        assert!(is_retryable_error("APNS error: 429 - TooManyRequests"));
    }

    #[test]
    fn test_is_retryable_error_fcm() {
        assert!(is_retryable_error("Unavailable"));
        assert!(is_retryable_error("DeviceMessageRateExceeded"));
    }

    #[test]
    fn test_is_retryable_error_non_retryable() {
        assert!(!is_retryable_error("BadDeviceToken"));
        assert!(!is_retryable_error("InvalidRegistration"));
        assert!(!is_retryable_error("MismatchSenderId"));
        assert!(!is_retryable_error("NotRegistered"));
    }

    #[test]
    fn test_push_result_success() {
        let result = PushResult::success();
        assert!(result.is_success);
        assert!(result.error.is_none());
        assert!(!result.should_retry);
    }

    #[test]
    fn test_push_result_success_with_response() {
        let result = PushResult::success_with_response("ok");
        assert!(result.is_success);
        assert_eq!(result.provider_response, Some("ok".to_string()));
    }

    #[test]
    fn test_push_result_failure() {
        let result = PushResult::failure("timeout");
        assert!(!result.is_success);
        assert_eq!(result.error, Some("timeout".to_string()));
        assert!(!result.should_retry);
    }

    #[test]
    fn test_push_result_retryable_failure() {
        let result = PushResult::retryable_failure("503");
        assert!(!result.is_success);
        assert_eq!(result.error, Some("503".to_string()));
        assert!(result.should_retry);
    }

    #[test]
    fn test_push_gateway_type_display() {
        assert_eq!(PushGatewayType::Apns.to_string(), "apns");
        assert_eq!(PushGatewayType::Fcm.to_string(), "fcm");
        assert_eq!(PushGatewayType::WebPush.to_string(), "webpush");
    }

    #[test]
    fn test_notification_payload_default() {
        let payload = NotificationPayload {
            title: "Hello".to_string(),
            body: "World".to_string(),
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
        assert_eq!(payload.title, "Hello");
        assert!(payload.icon.is_none());
    }

    #[test]
    fn test_notification_counts_default() {
        let counts = NotificationCounts { unread: 5, missed_calls: 1 };
        assert_eq!(counts.unread, 5);
        assert_eq!(counts.missed_calls, 1);
    }
}
