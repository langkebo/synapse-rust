use super::models::*;
use super::storage::IdentityStorage;
use crate::ApiResult;
use futures::future;
use reqwest::Client;
use synapse_common::error::ApiError;

pub struct IdentityService {
    storage: IdentityStorage,
    http_client: Client,
    trusted_servers: Vec<String>,
}

impl IdentityService {
    pub fn new(storage: IdentityStorage, trusted_servers: Vec<String>) -> Self {
        // F-1: 复用共享 HTTP client（带超时与连接池），不再裸用 Client::new()
        Self { storage, http_client: synapse_common::http_client::default_client(), trusted_servers }
    }

    pub async fn get_user_three_pids(&self, user_id: &str) -> ApiResult<Vec<ThirdPartyId>> {
        self.storage.get_user_three_pids(user_id).await
    }

    pub async fn add_three_pid(&self, address: &str, medium: &str, user_id: &str) -> ApiResult<()> {
        let three_pid = ThirdPartyId::new(address, medium, user_id);
        self.storage.add_three_pid(&three_pid).await
    }

    pub async fn remove_three_pid(&self, address: &str, medium: &str, user_id: &str) -> ApiResult<()> {
        self.storage.remove_three_pid(address, medium, user_id).await
    }

    pub async fn bind_three_pid(
        &self,
        id_server: &str,
        id_access_token: &str,
        sid: &str,
        client_secret: &str,
        user_id: &str,
    ) -> ApiResult<()> {
        self.validate_id_server(id_server)?;
        let url = format!("https://{id_server}/_matrix/identity/v3/3pid/bind");

        let body = serde_json::json!({
            "sid": sid,
            "client_secret": client_secret,
            "mxid": user_id,
            "token": id_access_token
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to bind 3PID", &e))?;

        if !response.status().is_success() {
            return Err(ApiError::internal_with_context("Identity server returned error", &response.status()));
        }

        // Parse the bind response to extract the real address and medium.
        // Per MSC2133 / Matrix Identity Service v2, the response contains
        // `address`, `medium`, and `ts`. Fall back to the request parameters
        // if the response body is missing or malformed.
        let response_json: serde_json::Value = response.json().await.unwrap_or_else(|_| serde_json::json!({}));

        let address = response_json.get("address").and_then(|v| v.as_str()).unwrap_or("");
        let medium = response_json.get("medium").and_then(|v| v.as_str()).unwrap_or("email");

        if address.is_empty() {
            return Err(ApiError::internal(
                "Identity server bind response did not contain a valid address".to_string(),
            ));
        }

        let three_pid = ThirdPartyId::new(address, medium, user_id);
        self.storage.add_three_pid(&three_pid).await?;

        Ok(())
    }

    pub async fn unbind_three_pid(
        &self,
        id_server: &str,
        id_access_token: &str,
        address: &str,
        medium: &str,
    ) -> ApiResult<()> {
        self.validate_id_server(id_server)?;
        let url = format!("https://{id_server}/_matrix/identity/v3/3pid/unbind");

        let body = serde_json::json!({
            "address": address,
            "medium": medium,
            "id_server": id_server,
            "id_access_token": id_access_token
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to unbind 3PID", &e))?;

        if !response.status().is_success() && response.status().as_u16() != 404 {
            return Err(ApiError::internal_with_context("Identity server returned error", &response.status()));
        }

        Ok(())
    }

    pub async fn request_3pid_verification(
        &self,
        id_server: &str,
        id_access_token: &str,
        medium: &str,
        address: &str,
        user_id: &str,
    ) -> ApiResult<String> {
        self.validate_id_server(id_server)?;
        let url = format!("https://{id_server}/_matrix/identity/v3/3pid/requestAuth");

        let body = serde_json::json!({
            "medium": medium,
            "address": address,
            "client_secret": "synapse_rust",
            "send_attempt": 1,
            "mxid": user_id,
            "token": id_access_token
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to request verification", &e))?;

        if !response.status().is_success() {
            return Err(ApiError::internal_with_context("Identity server returned error", &response.status()));
        }

        let json: serde_json::Value =
            response.json().await.map_err(|e| ApiError::internal_with_context("Failed to parse response", &e))?;

        let sid = json
            .get("sid")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| ApiError::internal("Missing sid in response".to_string()))?;

        Ok(sid)
    }

    pub async fn check_3pid_validity(&self, id_server: &str, sid: &str, client_secret: &str) -> ApiResult<bool> {
        self.validate_id_server(id_server)?;
        let url = format!("https://{id_server}/_matrix/identity/v3/3pid/getValidationStatus");

        let body = serde_json::json!({
            "sid": sid,
            "client_secret": client_secret
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to check validity", &e))?;

        if !response.status().is_success() {
            return Ok(false);
        }

        let json: serde_json::Value =
            response.json().await.map_err(|e| ApiError::internal_with_context("Failed to parse response", &e))?;

        Ok(json.get("valid").and_then(|v| v.as_bool()).unwrap_or(false))
    }

    pub async fn lookup_3pid(&self, medium: &str, address: &str) -> ApiResult<Option<String>> {
        self.storage.get_three_pid_user(address, medium).await
    }

    pub async fn hash_lookup(&self, addresses: &[String], mediums: &[String]) -> ApiResult<Vec<serde_json::Value>> {
        // P3: Run all (address × medium) lookups concurrently with join_all.
        // Each lookup is an independent DB query — no ordering dependency.
        let addresses: Vec<String> = addresses.to_vec();
        let mediums: Vec<String> = mediums.to_vec();

        let futures: Vec<_> = addresses
            .iter()
            .flat_map(|address| mediums.iter().map(move |medium| (address.clone(), medium.clone())))
            .map(|(address, medium)| async move {
                let result = self.lookup_3pid(&medium, &address).await;
                (address, medium, result)
            })
            .collect();

        let results = future::join_all(futures).await;

        Ok(results
            .into_iter()
            .filter_map(|(address, medium, result)| {
                if let Ok(Some(_user_id)) = result {
                    Some(serde_json::json!({
                        "address": address,
                        "medium": medium,
                    }))
                } else {
                    None
                }
            })
            .collect())
    }

    pub async fn invite_3pid(
        &self,
        room_id: &str,
        inviter: &str,
        medium: &str,
        address: &str,
        id_server: &str,
        id_access_token: &str,
    ) -> ApiResult<InvitationResponse> {
        self.validate_id_server(id_server)?;
        let url = format!("https://{id_server}/_matrix/identity/v1/invite");

        let body = serde_json::json!({
            "room_id": room_id,
            "sender": inviter,
            "medium": medium,
            "address": address,
            "id_access_token": id_access_token
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to invite", &e))?;

        if !response.status().is_success() {
            let status = response.status();
            if status.as_u16() == 404 {
                return Ok(InvitationResponse { user_id: None, signed: None });
            }
            return Err(ApiError::internal_with_context("Identity server returned error", &status));
        }

        let json: serde_json::Value =
            response.json().await.map_err(|e| ApiError::internal_with_context("Failed to parse response", &e))?;

        let user_id = json.get("user_id").and_then(|v| v.as_str()).map(String::from);
        let signed = json.get("signed").cloned();

        Ok(InvitationResponse { user_id, signed })
    }

    pub fn get_trusted_servers(&self) -> &[String] {
        &self.trusted_servers
    }

    pub fn validate_id_server(&self, id_server: &str) -> ApiResult<()> {
        if id_server.is_empty() {
            return Err(ApiError::bad_request("id_server cannot be empty".to_string()));
        }

        if id_server.contains('/') || id_server.contains('\\') {
            return Err(ApiError::bad_request("id_server must be a hostname only".to_string()));
        }

        if id_server.starts_with('.') || id_server.ends_with('.') {
            return Err(ApiError::bad_request("id_server has invalid format".to_string()));
        }

        let host = id_server.split(':').next().unwrap_or("");
        if host.is_empty() {
            return Err(ApiError::bad_request("id_server has empty hostname".to_string()));
        }

        if host == "localhost"
            || host.starts_with("127.")
            || host.starts_with("10.")
            || host.starts_with("192.168.")
            || host.starts_with("169.254.")
        {
            return Err(ApiError::bad_request("id_server must not be a private/local address".to_string()));
        }

        if host.starts_with("0.") || host == "0.0.0.0" {
            return Err(ApiError::bad_request("id_server must not be a broadcast address".to_string()));
        }

        if !self.trusted_servers.is_empty() && !self.trusted_servers.iter().any(|s| s == id_server) {
            return Err(ApiError::bad_request(format!("id_server '{id_server}' is not in the trusted servers list")));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for [`IdentityService`] — focus on the pure validation
    //! path (`validate_id_server`) and the trusted-server accessors that
    //! have no HTTP/DB dependency.
    //!
    //! `validate_id_server` is security-critical (SSRF prevention + allow-list
    //! enforcement) and has many branches. Coverage is prioritized here.
    //!
    //! HTTP-backed methods (`bind_three_pid`, `unbind_three_pid`,
    //! `request_3pid_verification`, `check_3pid_validity`, `invite_3pid`)
    //! require a mock identity server and are exercised by the integration
    //! tests under `tests/integration/`.

    use super::*;
    // IdentityService fields are pub(crate), so tests in the same module can
    // access them directly. We bypass `new()` to avoid `default_client()` which
    // requires a Tokio runtime. The pool is `connect_lazy` so no real connection
    // is opened; storage is never queried by these pure-function tests.
    fn make_service(trusted: Vec<String>) -> IdentityService {
        let pool = std::sync::Arc::new(
            sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect_lazy("postgresql://x:x@127.0.0.1:1/__test__")
                .expect("connect_lazy should not fail at construction"),
        );
        IdentityService {
            storage: IdentityStorage::new(&pool),
            // Client::new() is fine here — we never send HTTP requests in these tests.
            http_client: reqwest::Client::new(),
            trusted_servers: trusted,
        }
    }

    // --- validate_id_server: basic format checks ---

    #[tokio::test]
    async fn validate_id_server_accepts_valid_hostname() {
        let svc = make_service(vec![]);
        assert!(svc.validate_id_server("identity.example.com").is_ok());
        assert!(svc.validate_id_server("id.matrix.org").is_ok());
    }

    #[tokio::test]
    async fn validate_id_server_accepts_hostname_with_port() {
        let svc = make_service(vec![]);
        // SSRF rule: host (before ':') must be a public hostname; port is fine.
        assert!(svc.validate_id_server("identity.example.com:8443").is_ok());
    }

    #[tokio::test]
    async fn validate_id_server_rejects_empty() {
        let svc = make_service(vec![]);
        let err = svc.validate_id_server("").unwrap_err();
        assert!(err.message.contains("cannot be empty"), "msg: {}", err.message);
    }

    #[tokio::test]
    async fn validate_id_server_rejects_path_traversal() {
        let svc = make_service(vec![]);
        assert!(svc.validate_id_server("example.com/foo").is_err());
        assert!(svc.validate_id_server("example.com\\bar").is_err());
    }

    #[tokio::test]
    async fn validate_id_server_rejects_leading_or_trailing_dot() {
        let svc = make_service(vec![]);
        assert!(svc.validate_id_server(".example.com").is_err());
        assert!(svc.validate_id_server("example.com.").is_err());
    }

    // --- SSRF / private address rejection ---

    #[tokio::test]
    async fn validate_id_server_rejects_localhost() {
        let svc = make_service(vec![]);
        let err = svc.validate_id_server("localhost").unwrap_err();
        assert!(err.message.contains("private/local"), "msg: {}", err.message);
    }

    #[tokio::test]
    async fn validate_id_server_rejects_loopback() {
        let svc = make_service(vec![]);
        assert!(svc.validate_id_server("127.0.0.1").is_err());
        assert!(svc.validate_id_server("127.0.0.1:8443").is_err());
        assert!(svc.validate_id_server("127.255.255.254").is_err());
    }

    #[tokio::test]
    async fn validate_id_server_rejects_private_10_dot() {
        let svc = make_service(vec![]);
        assert!(svc.validate_id_server("10.0.0.1").is_err());
        assert!(svc.validate_id_server("10.255.255.255").is_err());
    }

    #[tokio::test]
    async fn validate_id_server_rejects_private_192_168() {
        let svc = make_service(vec![]);
        assert!(svc.validate_id_server("192.168.1.1").is_err());
    }

    #[tokio::test]
    async fn validate_id_server_rejects_link_local_169_254() {
        let svc = make_service(vec![]);
        // AWS / cloud link-local — must not be reachable as an identity server.
        assert!(svc.validate_id_server("169.254.169.254").is_err());
    }

    #[tokio::test]
    async fn validate_id_server_rejects_broadcast() {
        let svc = make_service(vec![]);
        assert!(svc.validate_id_server("0.0.0.0").is_err());
        assert!(svc.validate_id_server("0.1.2.3").is_err());
    }

    // --- trusted-server allow-list ---

    #[tokio::test]
    async fn validate_id_server_trusted_empty_list_allows_any_public() {
        // Empty trusted_servers → no allow-list enforcement (back-compat).
        let svc = make_service(vec![]);
        assert!(svc.validate_id_server("any-id-server.example.org").is_ok());
    }

    #[tokio::test]
    async fn validate_id_server_trusted_list_accepts_member() {
        let svc = make_service(vec!["id.example.com".to_string(), "id.matrix.org".to_string()]);
        assert!(svc.validate_id_server("id.example.com").is_ok());
        assert!(svc.validate_id_server("id.matrix.org").is_ok());
    }

    #[tokio::test]
    async fn validate_id_server_trusted_list_rejects_non_member() {
        let svc = make_service(vec!["id.example.com".to_string()]);
        let err = svc.validate_id_server("id.attacker.org").unwrap_err();
        assert!(err.message.contains("not in the trusted servers list"), "msg: {}", err.message);
    }

    #[tokio::test]
    async fn validate_id_server_trusted_list_ignores_private_even_if_listed() {
        // Private/local rejection fires before trusted-list check, so listing
        // "localhost" as trusted does not bypass SSRF protection.
        let svc = make_service(vec!["localhost".to_string()]);
        assert!(svc.validate_id_server("localhost").is_err());
    }

    // --- trusted_servers accessor ---

    #[tokio::test]
    async fn get_trusted_servers_returns_configured_list() {
        let svc = make_service(vec!["a.example.com".to_string(), "b.example.com".to_string()]);
        assert_eq!(svc.get_trusted_servers(), &["a.example.com", "b.example.com"]);
    }

    #[tokio::test]
    async fn get_trusted_servers_returns_empty_when_unset() {
        let svc = make_service(vec![]);
        assert!(svc.get_trusted_servers().is_empty());
    }
}
