//! MSC3861: MAS admin REST client.
//!
//! When MAS (Matrix Authentication Service) is deployed, the homeserver may
//! need to query the MAS admin API to look up user accounts or list devices
//! by OIDC `sub`. This module provides a thin HTTP client over
//! `reqwest::Client` that:
//!
//! - Reads its base URL and admin bearer token from [`MasConfig`].
//! - Refuses to call the API when `admin_token` is `None` (fail-closed)
//!   so a misconfigured deployment does not leak unauthenticated requests.
//! - Maps 404 to `Ok(None)` / `Ok(vec![])` and any other non-2xx status to
//!   `Err(MasError::HttpStatus)` so callers can distinguish "not found"
//!   from "MAS is broken".

use serde::Deserialize;
use synapse_common::config::MasConfig;

/// Errors returned by [`MasRestClient`] calls.
#[derive(Debug, thiserror::Error)]
pub enum MasError {
    /// The MAS admin REST client was asked to make a call but no
    /// `admin_token` is configured. Fail-closed: refuse to send an
    /// unauthenticated request rather than risk a silent 401/403.
    #[error("MAS admin REST client is not configured: admin_token is missing")]
    /// The `NotConfigured` variant.
    NotConfigured,
    /// The MAS admin API returned a non-2xx status other than 404.
    #[error("MAS admin API request failed with status {status}: {body}")]
    /// The `HttpStatus` variant.
    HttpStatus {
        /// The `status` field.
        status: u16,
        /// The `body` field.
        body: String,
    },
    /// The underlying HTTP transport failed (DNS, connection, timeout).
    #[error("MAS admin API transport error: {0}")]
    /// The `Transport` variant.
    Transport(#[from] reqwest::Error),
}

/// A user account record returned by the MAS admin API
/// (`GET /account/{sub}`).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct MasUserAccount {
    /// The OIDC subject identifier. Always present.
    pub sub: String,
    /// The MAS-local username, if the account has one.
    pub username: Option<String>,
    /// The primary email address, if verified.
    pub email: Option<String>,
    /// The display name, if set.
    pub display_name: Option<String>,
}

/// A device record returned by the MAS admin API
/// (`GET /device/{sub}`).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct MasDevice {
    /// The Matrix device ID. Always present.
    pub device_id: String,
    /// The human-readable device display name, if set.
    pub display_name: Option<String>,
    /// Last-seen timestamp in milliseconds since the Unix epoch, if known.
    pub last_seen_ts: Option<i64>,
}

/// Thin HTTP client for the MAS admin REST API.
///
/// Constructed from [`MasConfig`]. Holds a `reqwest::Client` with a sane
/// default timeout. The `base_url` mirrors `MasConfig::issuer_url` (without
/// trailing slash normalization beyond stripping a single trailing `/`).
pub struct MasRestClient {
    /// Base URL for MAS API calls, e.g. `https://mas.example.com`.
    pub base_url: String,
    /// Optional bearer token for `Authorization: Bearer <token>` on admin
    /// endpoints. When `None`, admin calls return `Err(MasError::NotConfigured)`.
    pub admin_token: Option<String>,
    http_client: reqwest::Client,
}

impl MasRestClient {
    /// Build a client from a [`MasConfig`]. Always succeeds — even when MAS
    /// is disabled — so callers can hold the client cheaply and gate calls
    /// on `MasConfig::is_configured()` themselves.
    pub fn new(config: &MasConfig) -> Self {
        let http_client =
            reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build().unwrap_or_else(|e| {
                // F-1: builder 失败不再静默 unwrap_or_default，记录 warn 并回退共享默认 client
                tracing::warn!(error = %e, "Failed to build MAS REST HTTP client, using shared default");
                synapse_common::http_client::default_client()
            });
        // Strip a single trailing slash so `{base_url}/account/{sub}` does
        // not produce a double slash.
        let base_url = if config.issuer_url.ends_with('/') {
            config.issuer_url.trim_end_matches('/').to_string()
        } else {
            config.issuer_url.clone()
        };
        Self { base_url, admin_token: config.admin_token.clone(), http_client }
    }

    /// Returns the `Authorization: Bearer <token>` header value, or
    /// `Err(MasError::NotConfigured)` when no admin token is configured.
    fn require_admin_bearer(&self) -> Result<String, MasError> {
        self.admin_token.as_ref().map(|tok| format!("Bearer {tok}")).ok_or(MasError::NotConfigured)
    }

    /// Look up a user account by OIDC `sub`.
    ///
    /// Calls `GET {base_url}/account/{sub}` with the admin bearer token.
    /// Returns `Ok(None)` on 404, `Ok(Some(account))` on 200, and
    /// `Err(MasError::HttpStatus)` on any other non-2xx status.
    pub async fn get_user_account(&self, sub: &str) -> Result<Option<MasUserAccount>, MasError> {
        let bearer = self.require_admin_bearer()?;
        let url = format!("{}/account/{}", self.base_url, sub);
        let resp = self.http_client.get(&url).header("Authorization", bearer).send().await?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(MasError::HttpStatus { status: status.as_u16(), body });
        }
        let account = resp.json::<MasUserAccount>().await?;
        Ok(Some(account))
    }

    /// List devices for a user by OIDC `sub`.
    ///
    /// Calls `GET {base_url}/device/{sub}` with the admin bearer token.
    /// Returns `Ok(vec)` on 200 (possibly empty), `Err(MasError::HttpStatus)`
    /// on any non-2xx status. Unlike [`get_user_account`](Self::get_user_account),
    /// a 404 is treated as an error because the MAS device-list endpoint
    /// returns an empty array (not 404) for a known user with no devices.
    pub async fn list_user_devices(&self, sub: &str) -> Result<Vec<MasDevice>, MasError> {
        let bearer = self.require_admin_bearer()?;
        let url = format!("{}/device/{}", self.base_url, sub);
        let resp = self.http_client.get(&url).header("Authorization", bearer).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(MasError::HttpStatus { status: status.as_u16(), body });
        }
        let devices = resp.json::<Vec<MasDevice>>().await?;
        Ok(devices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mas_rest_client_new_from_disabled_config_still_constructs() {
        // Even when MAS is disabled, the client must construct successfully
        // so it can be held cheaply by AuthService and only invoked when
        // is_configured() is true.
        let config = synapse_common::config::MasConfig::default();
        let client = MasRestClient::new(&config);
        assert!(client.base_url.is_empty(), "base_url should mirror issuer_url (empty by default)");
        assert!(client.admin_token.is_none(), "admin_token should be None by default");
    }

    #[test]
    fn mas_rest_client_new_from_enabled_config_copies_fields() {
        let config = synapse_common::config::MasConfig {
            enabled: true,
            issuer_url: "https://mas.example.com".to_string(),
            client_id: "synapse-rust".to_string(),
            client_secret: "s3cret".to_string(),
            admin_token: Some("admin-tok".to_string()),
        };
        let client = MasRestClient::new(&config);
        assert_eq!(client.base_url, "https://mas.example.com");
        assert_eq!(client.admin_token.as_deref(), Some("admin-tok"));
    }

    #[test]
    fn mas_rest_client_strips_single_trailing_slash_from_base_url() {
        let config = synapse_common::config::MasConfig {
            enabled: true,
            issuer_url: "https://mas.example.com/".to_string(),
            ..Default::default()
        };
        let client = MasRestClient::new(&config);
        assert_eq!(client.base_url, "https://mas.example.com");
    }

    #[test]
    fn mas_user_account_deserializes_from_mas_api_json() {
        // Shape mirrors the MAS admin API user account response.
        let json = r#"{
            "sub": "mf3kjlo4ir8",
            "username": "alice",
            "email": "alice@example.com",
            "display_name": "Alice"
        }"#;
        let account: MasUserAccount = serde_json::from_str(json).expect("MAS user account JSON should deserialize");
        assert_eq!(account.sub, "mf3kjlo4ir8");
        assert_eq!(account.username.as_deref(), Some("alice"));
        assert_eq!(account.email.as_deref(), Some("alice@example.com"));
        assert_eq!(account.display_name.as_deref(), Some("Alice"));
    }

    #[test]
    fn mas_user_account_deserializes_with_only_required_sub() {
        // Only `sub` is required; all other fields are optional.
        let json = r#"{"sub": "abc123"}"#;
        let account: MasUserAccount =
            serde_json::from_str(json).expect("minimal MAS user account JSON should deserialize");
        assert_eq!(account.sub, "abc123");
        assert!(account.username.is_none());
        assert!(account.email.is_none());
        assert!(account.display_name.is_none());
    }

    #[test]
    fn mas_device_deserializes_from_mas_api_json() {
        let json = r#"{
            "device_id": "DEVALICE1",
            "display_name": "Alice Laptop",
            "last_seen_ts": 1700000000000
        }"#;
        let device: MasDevice = serde_json::from_str(json).expect("MAS device JSON should deserialize");
        assert_eq!(device.device_id, "DEVALICE1");
        assert_eq!(device.display_name.as_deref(), Some("Alice Laptop"));
        assert_eq!(device.last_seen_ts, Some(1700000000000));
    }

    #[test]
    fn mas_device_deserializes_with_only_required_device_id() {
        let json = r#"{"device_id": "DEV1"}"#;
        let device: MasDevice = serde_json::from_str(json).expect("minimal MAS device JSON should deserialize");
        assert_eq!(device.device_id, "DEV1");
        assert!(device.display_name.is_none());
        assert!(device.last_seen_ts.is_none());
    }

    #[tokio::test]
    async fn get_user_account_returns_account_when_mas_responds_200() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let config = synapse_common::config::MasConfig {
            enabled: true,
            issuer_url: server.uri(),
            client_id: "synapse-rust".to_string(),
            client_secret: "s3cret".to_string(),
            admin_token: Some("admin-tok".to_string()),
        };
        let client = MasRestClient::new(&config);

        Mock::given(method("GET"))
            .and(path("/account/mf3kjlo4ir8"))
            .and(header("Authorization", "Bearer admin-tok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sub": "mf3kjlo4ir8",
                "username": "alice",
                "email": "alice@example.com",
                "display_name": "Alice"
            })))
            .mount(&server)
            .await;

        let result = client.get_user_account("mf3kjlo4ir8").await.expect("200 response should yield Ok(Some)");
        let account = result.expect("200 with body should yield Some(account)");
        assert_eq!(account.sub, "mf3kjlo4ir8");
        assert_eq!(account.username.as_deref(), Some("alice"));
    }

    #[tokio::test]
    async fn get_user_account_returns_none_when_mas_responds_404() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let config = synapse_common::config::MasConfig {
            enabled: true,
            issuer_url: server.uri(),
            admin_token: Some("admin-tok".to_string()),
            ..Default::default()
        };
        let client = MasRestClient::new(&config);

        Mock::given(method("GET"))
            .and(path("/account/unknown-sub"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let result = client.get_user_account("unknown-sub").await.expect("404 should yield Ok(None)");
        assert!(result.is_none(), "404 should map to None, not an error");
    }

    #[tokio::test]
    async fn get_user_account_returns_error_when_admin_token_missing() {
        // Fail-closed: without an admin_token, the client must refuse to call
        // the MAS admin API rather than sending an unauthenticated request.
        let config = synapse_common::config::MasConfig {
            enabled: true,
            issuer_url: "https://mas.example.com".to_string(),
            admin_token: None,
            ..Default::default()
        };
        let client = MasRestClient::new(&config);

        let result = client.get_user_account("any-sub").await;
        assert!(result.is_err(), "missing admin_token should produce an error, not a silent call");
        assert!(matches!(result, Err(MasError::NotConfigured)));
    }

    #[tokio::test]
    async fn list_user_devices_returns_devices_when_mas_responds_200() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let config = synapse_common::config::MasConfig {
            enabled: true,
            issuer_url: server.uri(),
            admin_token: Some("admin-tok".to_string()),
            ..Default::default()
        };
        let client = MasRestClient::new(&config);

        Mock::given(method("GET"))
            .and(path("/device/mf3kjlo4ir8"))
            .and(header("Authorization", "Bearer admin-tok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "device_id": "DEV1", "display_name": "Laptop", "last_seen_ts": 1700000000000_i64 },
                { "device_id": "DEV2" }
            ])))
            .mount(&server)
            .await;

        let devices = client.list_user_devices("mf3kjlo4ir8").await.expect("200 response should yield Ok(vec)");
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].device_id, "DEV1");
        assert_eq!(devices[0].display_name.as_deref(), Some("Laptop"));
        assert_eq!(devices[1].device_id, "DEV2");
        assert!(devices[1].display_name.is_none());
    }

    #[tokio::test]
    async fn list_user_devices_returns_error_on_500() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let config = synapse_common::config::MasConfig {
            enabled: true,
            issuer_url: server.uri(),
            admin_token: Some("admin-tok".to_string()),
            ..Default::default()
        };
        let client = MasRestClient::new(&config);

        Mock::given(method("GET"))
            .and(path("/device/mf3kjlo4ir8"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let result = client.list_user_devices("mf3kjlo4ir8").await;
        assert!(result.is_err(), "500 should produce an error");
    }
}
