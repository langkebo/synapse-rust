use super::models::*;
use super::storage::IdentityStorage;
use crate::ApiResult;
use futures::future;
use reqwest::Client;
use synapse_common::error::ApiError;

/// The `IdentityService` struct.
pub struct IdentityService {
    storage: IdentityStorage,
    http_client: Client,
    trusted_servers: Vec<String>,
    /// W5 test-utils 接缝：注入的 base URL（用于 wiremock mock server）。
    /// 仅在 test-utils feature 下使用，生产代码走默认的 https://{id_server}。
    test_base_url: Option<String>,
}

impl IdentityService {
    /// See [`new`].
    pub fn new(storage: IdentityStorage, trusted_servers: Vec<String>) -> Self {
        // F-1: 复用共享 HTTP client（带超时与连接池），不再裸用 Client::new()
        Self {
            storage,
            http_client: synapse_common::http_client::default_client(),
            trusted_servers,
            test_base_url: None,
        }
    }

    #[cfg(feature = "test-utils")]
    /// See [`new`]. 供 test-utils feature 下的单元测试使用。
    /// 传入 base_url 以绕过 SSRF 校验，mock wiremock server 的 URL。
    pub fn with_test_base_url(storage: IdentityStorage, trusted_servers: Vec<String>, base_url: String) -> Self {
        Self {
            storage,
            http_client: synapse_common::http_client::default_client(),
            trusted_servers,
            test_base_url: Some(base_url),
        }
    }

    /// W5 test-utils 接缝：构建 identity server URL。
    /// 生产路径：https://{id_server}{path}（SSRF 防护由 validate_id_server 保证）。
    /// test-utils 注入路径：{base_url}{path}（wiremock mock server，SSRF 校验 bypass）。
    fn id_server_url(&self, id_server: &str, path: &str) -> String {
        match &self.test_base_url {
            Some(base) => format!("{base}{path}"),
            None => format!("https://{id_server}{path}"),
        }
    }

    /// W5 test-utils 接缝：SSRF 校验门。
    /// 注入 test_base_url 时跳过（mock server 是 127.0.0.1:port，本来就会被拒）；
    /// 生产路径必须走完整校验。
    fn validate_id_server_for_request(&self, id_server: &str) -> ApiResult<()> {
        if self.test_base_url.is_some() {
            return Ok(());
        }
        self.validate_id_server(id_server)
    }

    /// See [`get_user_three_pids`].
    pub async fn get_user_three_pids(&self, user_id: &str) -> ApiResult<Vec<ThirdPartyId>> {
        self.storage.get_user_three_pids(user_id).await
    }

    /// See [`add_three_pid`].
    pub async fn add_three_pid(&self, address: &str, medium: &str, user_id: &str) -> ApiResult<()> {
        let three_pid = ThirdPartyId::new(address, medium, user_id);
        self.storage.add_three_pid(&three_pid).await
    }

    /// See [`remove_three_pid`].
    pub async fn remove_three_pid(&self, address: &str, medium: &str, user_id: &str) -> ApiResult<()> {
        self.storage.remove_three_pid(address, medium, user_id).await
    }

    /// See [`bind_three_pid`].
    pub async fn bind_three_pid(
        &self,
        id_server: &str,
        id_access_token: &str,
        sid: &str,
        client_secret: &str,
        user_id: &str,
    ) -> ApiResult<()> {
        self.validate_id_server_for_request(id_server)?;
        let url = self.id_server_url(id_server, "/_matrix/identity/v3/3pid/bind");

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

    /// See [`unbind_three_pid`].
    pub async fn unbind_three_pid(
        &self,
        id_server: &str,
        id_access_token: &str,
        address: &str,
        medium: &str,
    ) -> ApiResult<()> {
        self.validate_id_server_for_request(id_server)?;
        let url = self.id_server_url(id_server, "/_matrix/identity/v3/3pid/unbind");

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

    /// See [`request_3pid_verification`].
    pub async fn request_3pid_verification(
        &self,
        id_server: &str,
        id_access_token: &str,
        medium: &str,
        address: &str,
        user_id: &str,
    ) -> ApiResult<String> {
        self.validate_id_server_for_request(id_server)?;
        let url = self.id_server_url(id_server, "/_matrix/identity/v3/3pid/requestAuth");

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

    /// See [`check_3pid_validity`].
    pub async fn check_3pid_validity(&self, id_server: &str, sid: &str, client_secret: &str) -> ApiResult<bool> {
        self.validate_id_server_for_request(id_server)?;
        let url = self.id_server_url(id_server, "/_matrix/identity/v3/3pid/getValidationStatus");

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

    /// See [`lookup_3pid`].
    pub async fn lookup_3pid(&self, medium: &str, address: &str) -> ApiResult<Option<String>> {
        self.storage.get_three_pid_user(address, medium).await
    }

    /// See [`hash_lookup`].
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

    /// See [`invite_3pid`].
    pub async fn invite_3pid(
        &self,
        room_id: &str,
        inviter: &str,
        medium: &str,
        address: &str,
        id_server: &str,
        id_access_token: &str,
    ) -> ApiResult<InvitationResponse> {
        self.validate_id_server_for_request(id_server)?;
        let url = self.id_server_url(id_server, "/_matrix/identity/v1/invite");

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

    /// See [`get_trusted_servers`].
    pub fn get_trusted_servers(&self) -> &[String] {
        &self.trusted_servers
    }

    /// See [`validate_id_server`].
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
    //! are exercised via `wiremock` + the `test-utils` base-URL seam
    //! (see `make_service_with_mock`) — the SSRF guard would otherwise
    //! reject the mock server's 127.0.0.1 loopback address.
    //!
    //! `bind_three_pid`'s success path writes through to storage, so only its
    //! pre-DB error branches are covered here; the full round-trip stays in
    //! the integration tests under `tests/integration/`.

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
            test_base_url: None,
        }
    }

    #[cfg(feature = "test-utils")]
    /// Build service with test_base_url 接缝，用于 wiremock mock identity server。
    fn make_service_with_mock(base_url: String) -> IdentityService {
        let pool = std::sync::Arc::new(
            sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect_lazy("postgresql://x:x@127.0.0.1:1/__test__")
                .expect("connect_lazy should not fail at construction"),
        );
        IdentityService::with_test_base_url(IdentityStorage::new(&pool), vec![], base_url)
    }

    // --- validate_id_server 测试（已存在） ---
    // 这些验证的是纯函数，覆盖 SSRF 防护的分支。

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

    // --- wiremock-backed HTTP method tests (test-utils only) ---
    // 这些测试用 make_service_with_mock 注入 wiremock mock server 的 base_url，
    // 绕过 SSRF 校验（mock server 是 127.0.0.1:port，本来就会被 validate_id_server 拒绝），
    // 从而对 5 个 HTTP 方法做真实请求-响应路径的单元测试。
    //
    // 边界说明：
    // - request_3pid_verification / check_3pid_validity / unbind_three_pid / invite_3pid
    //   不写入 DB，可完整覆盖成功与失败分支。
    // - bind_three_pid 成功路径会调用 storage.add_three_pid（真实 DB），
    //   make_service_with_mock 用 connect_lazy 的惰性 pool 兜底，此处只测
    //   不落库的失败分支（响应缺 address / 非 2xx）。

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn request_3pid_verification_success_returns_sid() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_matrix/identity/v3/3pid/requestAuth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "sid": "sid_abc123" })))
            .mount(&mock_server)
            .await;

        let svc = make_service_with_mock(mock_server.uri());
        // 控制台信息：即使 id_server 是 "127.0.0.1" 也会被接缝跳过 SSRF，
        // 实际用的是 with_test_base_url 注入的 base_url。
        let sid = svc
            .request_3pid_verification("127.0.0.1", "token", "email", "user@example.com", "@user:example.com")
            .await
            .unwrap();
        assert_eq!(sid, "sid_abc123");
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn request_3pid_verification_missing_sid_errors() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_matrix/identity/v3/3pid/requestAuth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "medium": "email" })))
            .mount(&mock_server)
            .await;

        let svc = make_service_with_mock(mock_server.uri());
        let err = svc
            .request_3pid_verification("127.0.0.1", "token", "email", "user@example.com", "@user:example.com")
            .await
            .unwrap_err();
        assert!(err.message.contains("Missing sid"), "msg: {}", err.message);
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn request_3pid_verification_non_2xx_errors() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_matrix/identity/v3/3pid/requestAuth"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let svc = make_service_with_mock(mock_server.uri());
        let err = svc
            .request_3pid_verification("127.0.0.1", "token", "email", "user@example.com", "@user:example.com")
            .await
            .unwrap_err();
        assert!(err.message.contains("Identity server returned error"), "msg: {}", err.message);
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn check_3pid_validity_valid_true() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_matrix/identity/v3/3pid/getValidationStatus"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "valid": true })))
            .mount(&mock_server)
            .await;

        let svc = make_service_with_mock(mock_server.uri());
        let valid = svc.check_3pid_validity("127.0.0.1", "sid_abc", "secret").await.unwrap();
        assert!(valid);
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn check_3pid_validity_valid_false() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_matrix/identity/v3/3pid/getValidationStatus"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "valid": false })))
            .mount(&mock_server)
            .await;

        let svc = make_service_with_mock(mock_server.uri());
        let valid = svc.check_3pid_validity("127.0.0.1", "sid_abc", "secret").await.unwrap();
        assert!(!valid);
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn check_3pid_validity_non_2xx_returns_false() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_matrix/identity/v3/3pid/getValidationStatus"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let svc = make_service_with_mock(mock_server.uri());
        // 非 2xx（如 404）→ Ok(false)，不报错。
        let valid = svc.check_3pid_validity("127.0.0.1", "sid_abc", "secret").await.unwrap();
        assert!(!valid);
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn unbind_three_pid_success() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_matrix/identity/v3/3pid/unbind"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let svc = make_service_with_mock(mock_server.uri());
        svc.unbind_three_pid("127.0.0.1", "token", "user@example.com", "email").await.unwrap();
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn unbind_three_pid_not_found_is_ok() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_matrix/identity/v3/3pid/unbind"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let svc = make_service_with_mock(mock_server.uri());
        // 404 → 容错（idempotent unbind），不报错。
        svc.unbind_three_pid("127.0.0.1", "token", "user@example.com", "email").await.unwrap();
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn unbind_three_pid_5xx_errors() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_matrix/identity/v3/3pid/unbind"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let svc = make_service_with_mock(mock_server.uri());
        let err = svc.unbind_three_pid("127.0.0.1", "token", "user@example.com", "email").await.unwrap_err();
        assert!(err.message.contains("Identity server returned error"), "msg: {}", err.message);
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn invite_3pid_success_returns_user_id() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_matrix/identity/v1/invite"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "user_id": "@invitee:example.com",
                "signed": { "signatures": {} }
            })))
            .mount(&mock_server)
            .await;

        let svc = make_service_with_mock(mock_server.uri());
        let resp = svc
            .invite_3pid(
                "!room:example.com",
                "@inviter:example.com",
                "email",
                "invitee@example.com",
                "127.0.0.1",
                "token",
            )
            .await
            .unwrap();
        assert_eq!(resp.user_id.as_deref(), Some("@invitee:example.com"));
        assert!(resp.signed.is_some());
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn invite_3pid_not_found_returns_empty() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_matrix/identity/v1/invite"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let svc = make_service_with_mock(mock_server.uri());
        // 404 → 返回空 InvitationResponse（address 无对应用户的场景）。
        let resp = svc
            .invite_3pid(
                "!room:example.com",
                "@inviter:example.com",
                "email",
                "invitee@example.com",
                "127.0.0.1",
                "token",
            )
            .await
            .unwrap();
        assert_eq!(resp.user_id, None);
        assert_eq!(resp.signed, None);
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn invite_3pid_5xx_errors() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_matrix/identity/v1/invite"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let svc = make_service_with_mock(mock_server.uri());
        let err = svc
            .invite_3pid(
                "!room:example.com",
                "@inviter:example.com",
                "email",
                "invitee@example.com",
                "127.0.0.1",
                "token",
            )
            .await
            .unwrap_err();
        assert!(err.message.contains("Identity server returned error"), "msg: {}", err.message);
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn bind_three_pid_missing_address_errors() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_matrix/identity/v3/3pid/bind"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "medium": "email" })))
            .mount(&mock_server)
            .await;

        let svc = make_service_with_mock(mock_server.uri());
        // 响应缺 address → 在写库前返回错误（不触发 DB 写入）。
        let err = svc.bind_three_pid("127.0.0.1", "token", "sid", "secret", "@user:example.com").await.unwrap_err();
        assert!(err.message.contains("did not contain a valid address"), "msg: {}", err.message);
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn bind_three_pid_non_2xx_errors() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_matrix/identity/v3/3pid/bind"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let svc = make_service_with_mock(mock_server.uri());
        let err = svc.bind_three_pid("127.0.0.1", "token", "sid", "secret", "@user:example.com").await.unwrap_err();
        assert!(err.message.contains("Identity server returned error"), "msg: {}", err.message);
    }
}
