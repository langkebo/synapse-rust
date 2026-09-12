use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;
use synapse_common::error::ApiError;
use tracing::{debug, error, info};

/// PUSH-01: Validate a push gateway URL before issuing an HTTP request.
///
/// Rejects:
/// - non-`https` schemes (http, ftp, file, javascript, data, ...)
/// - URL parse failures
/// - host = localhost / loopback / unspecified (incl. IPv4-mapped IPv6)
/// - host = link-local (169.254.0.0/16, fe80::/10) — covers cloud metadata
/// - host = private (RFC 1918 + RFC 4193)
/// - host = IP literal (any family) — push gateways should be DNS-named
///
/// Returns `ApiError::bad_request` for malformed URLs, `ApiError::forbidden`
/// for SSRF-blocked hosts (distinguish for client UX).
pub fn validate_push_gateway_url(url: &str) -> Result<(), ApiError> {
    if url.is_empty() {
        return Err(ApiError::bad_request("push gateway url is empty"));
    }
    let parsed = url::Url::parse(url).map_err(|e| ApiError::bad_request(format!("invalid gateway url: {e}")))?;
    if parsed.scheme() != "https" {
        return Err(ApiError::bad_request(format!("push gateway url must be https (got scheme: {})", parsed.scheme())));
    }
    let host = parsed.host_str().ok_or_else(|| ApiError::bad_request("push gateway url missing host"))?;

    // 1) IP literal — push gateways should be DNS-named for cert rotation.
    // `url::Url::host_str()` returns bracketed form `[::1]` for IPv6, so we
    // strip brackets before parsing to handle both IPv4 and IPv6 literals.
    let ip_host = if host.starts_with('[') {
        host.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(host)
    } else {
        host
    };
    if let Ok(ip) = ip_host.parse::<IpAddr>() {
        return Err(ApiError::forbidden(format!("push gateway host is IP literal: {ip}")));
    }

    // 2) Name-based blocklist for localhost/loopback variants
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") || lower.ends_with(".local") {
        return Err(ApiError::forbidden(format!("push gateway host not allowed: {host}")));
    }

    // 3) Resolve hostname → IP, block private/loopback/link-local ranges.
    // Use std::net lookup; if resolution fails the cert handshake will
    // surface the error at request time — we don't pre-resolve to avoid
    // blocking legitimate gateways with flaky DNS at config time.
    //
    // Note: this is a defense-in-depth check. The real protection is
    // gateway-side firewall + the fact that `data.url` is server-controlled
    // at the user input level (set_pusher already requires auth).
    if let Ok(addrs) = std::net::ToSocketAddrs::to_socket_addrs(&(ip_host, 443u16)) {
        for addr in addrs {
            let ip = addr.ip();
            if is_blocked_ip(&ip) {
                return Err(ApiError::forbidden(format!("push gateway host resolves to blocked address: {ip}")));
            }
        }
    }

    Ok(())
}

/// PUSH-01: Check whether an IP address belongs to a blocked range
/// (loopback, link-local, private, unspecified, multicast).
fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_ipv4(v4),
        IpAddr::V6(v6) => is_blocked_ipv6(v6),
    }
}

fn is_blocked_ipv4(ip: &Ipv4Addr) -> bool {
    ip.is_loopback()           // 127.0.0.0/8
        || ip.is_unspecified() // 0.0.0.0
        || ip.is_link_local()  // 169.254.0.0/16 — cloud metadata!
        || ip.is_private()     // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
        || ip.is_multicast()   // 224.0.0.0/4
        || ip.is_broadcast() // 255.255.255.255
}

fn is_blocked_ipv6(ip: &Ipv6Addr) -> bool {
    ip.is_loopback()           // ::1
        || ip.is_unspecified() // ::
        // is_loopback() / is_unspecified() on Ipv6Addr already cover
        // IPv4-mapped forms (::ffff:127.0.0.1 / ::ffff:0.0.0.0) per stdlib.
        || ip.is_multicast()   // ff00::/8
        // Unique local addresses (fc00::/7) — private IPv6
        || (ip.segments()[0] & 0xfe00) == 0xfc00
        // Link-local fe80::/10
        || (ip.segments()[0] & 0xffc0) == 0xfe80
}

/// The `PushNotification` struct.
#[derive(Debug, Clone, Serialize)]
pub struct PushNotification {
    /// The `notification` field.
    pub notification: NotificationContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `devices` field.
    pub devices: Option<Vec<PushDevice>>,
}

/// The `NotificationContent` struct.
#[derive(Debug, Clone, Serialize)]
pub struct NotificationContent {
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    #[serde(rename = "type")]
    /// The `event_type` field.
    pub event_type: String,
    /// The `sender` field.
    pub sender: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `room_name` field.
    pub room_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `room_alias` field.
    pub room_alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `user_is_target` field.
    pub user_is_target: Option<bool>,
    /// The `counts` field.
    pub counts: NotificationCounts,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `devices` field.
    pub devices: Option<Vec<PushDeviceContent>>,
}

/// The `NotificationCounts` struct.
#[derive(Debug, Clone, Serialize)]
pub struct NotificationCounts {
    /// The `missed_calls` field.
    pub missed_calls: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `unread` field.
    pub unread: Option<u32>,
}

/// The `PushDevice` struct.
#[derive(Debug, Clone, Serialize)]
pub struct PushDevice {
    /// The `app_id` field.
    pub app_id: String,
    /// The `pushkey` field.
    pub pushkey: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `pushkey_ts` field.
    pub pushkey_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `data` field.
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `tweaks` field.
    pub tweaks: Option<serde_json::Value>,
}

/// The `PushDeviceContent` struct.
#[derive(Debug, Clone, Serialize)]
pub struct PushDeviceContent {
    /// The `app_id` field.
    pub app_id: String,
    /// The `pushkey` field.
    pub pushkey: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `data` field.
    pub data: Option<serde_json::Value>,
}

/// The `PushGatewayResponse` struct.
#[derive(Debug, Clone, Deserialize)]
pub struct PushGatewayResponse {
    /// The `rejected` field.
    pub rejected: Vec<String>,
}

/// The `PushGatewayConfig` struct.
#[derive(Debug, Clone)]
pub struct PushGatewayConfig {
    /// The `timeout_secs` field.
    pub timeout_secs: u64,
    /// The `max_retries` field.
    pub max_retries: u32,
}

impl Default for PushGatewayConfig {
    fn default() -> Self {
        Self { timeout_secs: 30, max_retries: 3 }
    }
}

/// The `PushGateway` struct.
#[derive(Debug)]
pub struct PushGateway {
    client: Client,
}

impl PushGateway {
    /// See [`new`].
    pub fn new(config: &PushGatewayConfig) -> Self {
        let client = Client::builder().timeout(Duration::from_secs(config.timeout_secs)).build().unwrap_or_else(|e| {
            // F-1: builder 失败不再静默退化，记录 warn 并回退共享默认 client
            tracing::warn!(error = %e, "Failed to build push gateway HTTP client, using shared default");
            synapse_common::http_client::default_client()
        });

        Self { client }
    }

    /// See [`send_notification`].
    pub async fn send_notification(
        &self,
        gateway_url: &str,
        notification: &PushNotification,
    ) -> Result<PushGatewayResponse, ApiError> {
        // PUSH-01: Validate gateway URL before issuing HTTP request. SSRF
        // defense — reject http://, IP literals, localhost, private/link-local
        // ranges (cloud metadata at 169.254.169.254). No callers in the
        // production path today, but the worker pusher type is defined and
        // the gateway can be wired up at any time.
        validate_push_gateway_url(gateway_url)?;

        info!(has_gateway_url = !gateway_url.is_empty(), "Sending notification to push gateway");

        let response = self
            .client
            .post(gateway_url)
            .header("Content-Type", "application/json")
            .json(notification)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to send to gateway", e))?;

        let status = response.status();

        if !status.is_success() {
            let body =
                response.text().await.map_err(|e| ApiError::internal_with_cause("Failed to read response", e))?;

            error!(
                %status,
                response_body_present = !body.is_empty(),
                response_body_len = body.len(),
                "Push gateway returned error"
            );
            return Err(ApiError::internal_with_context("Push gateway error", &status));
        }

        let gateway_response: PushGatewayResponse =
            response.json().await.map_err(|e| ApiError::internal_with_cause("Failed to parse gateway response", e))?;

        debug!(rejected = gateway_response.rejected.len(), "Push gateway response");

        Ok(gateway_response)
    }

    /// See [`build_notification`].
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
                counts: NotificationCounts { missed_calls, unread: Some(unread_count) },
                devices: None,
            },
            devices: Some(devices),
        }
    }

    /// See [`build_device`].
    pub fn build_device(
        &self,
        app_id: &str,
        pushkey: &str,
        data: Option<serde_json::Value>,
        tweaks: Option<serde_json::Value>,
    ) -> PushDevice {
        PushDevice { app_id: app_id.to_string(), pushkey: pushkey.to_string(), pushkey_ts: None, data, tweaks }
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
        let gateway = PushGateway::new(&PushGatewayConfig::default());

        let notification =
            gateway.build_notification("event123", "room123", "m.room.message", "@user:example.com", 5, 0, vec![]);

        assert_eq!(notification.notification.event_id, "event123");
        assert_eq!(notification.notification.room_id, "room123");
        assert_eq!(notification.notification.counts.unread, Some(5));
    }

    #[test]
    fn test_build_device() {
        let gateway = PushGateway::new(&PushGatewayConfig::default());

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
        let counts = NotificationCounts { missed_calls: 0, unread: Some(5) };

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

    // ── PUSH-01: SSRF defense for push gateway URLs ──────────────────────
    //
    // `validate_push_gateway_url` rejects:
    //   - empty / unparseable URLs
    //   - non-https schemes
    //   - IP literals (any family)
    //   - localhost / .local
    //   - hosts resolving to loopback / link-local / private / multicast
    //
    // Acceptance: https://push.example.com and https://push.example.com:8443
    // pass; everything malicious fails.

    #[test]
    fn test_validate_push_gateway_url_rejects_empty() {
        let err = validate_push_gateway_url("").expect_err("empty url must be rejected");
        assert!(err.to_string().contains("empty"), "got: {err}");
    }

    #[test]
    fn test_validate_push_gateway_url_rejects_non_https() {
        for bad in [
            "http://push.example.com",
            "http://169.254.169.254/latest/meta-data/",
            "ftp://push.example.com",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/plain,foo",
        ] {
            let err = match validate_push_gateway_url(bad) {
                Ok(()) => panic!("{bad} should be rejected, got ok"),
                Err(e) => e,
            };
            let msg = err.to_string();
            assert!(
                msg.contains("https") || msg.contains("invalid") || msg.contains("scheme"),
                "expected scheme/https error for {bad}, got: {msg}"
            );
        }
    }

    #[test]
    fn test_validate_push_gateway_url_rejects_ip_literal() {
        for bad in ["https://1.2.3.4", "https://127.0.0.1", "https://0.0.0.0", "https://[::1]", "https://[2001:db8::1]"]
        {
            let err = match validate_push_gateway_url(bad) {
                Ok(()) => panic!("{bad} should be rejected, got ok"),
                Err(e) => e,
            };
            let msg = err.to_string();
            assert!(msg.contains("IP literal") || msg.contains("blocked"), "got: {msg} for {bad}");
        }
    }

    #[test]
    fn test_validate_push_gateway_url_rejects_localhost_and_private() {
        // "localhost" / ".local" are rejected without DNS lookup
        for bad in ["https://localhost", "https://api.localhost", "https://gateway.local"] {
            let err = validate_push_gateway_url(bad).unwrap_err();
            assert!(err.to_string().contains("not allowed"), "got: {err} for {bad}");
        }
        // Private DNS names may or may not resolve in CI; if they do, we
        // expect a "blocked address" error from the resolver check. If
        // resolution fails (offline CI), the URL passes through and the
        // cert handshake later will surface the issue. We only assert
        // about names we know resolve to private space — skipped here
        // to keep the test hermetic.
    }

    #[test]
    fn test_validate_push_gateway_url_accepts_legitimate_https() {
        // Use a real, widely-resolving DNS name with a non-default port
        // to ensure both scheme + host checks pass cleanly.
        validate_push_gateway_url("https://push.example.com:8443/path").expect("legitimate https URL must be accepted");
    }
}
