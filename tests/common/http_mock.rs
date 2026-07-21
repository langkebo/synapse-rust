use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Shared HTTP mock factory wrapping a `wiremock::MockServer`.
///
/// Provides convenience methods for common Matrix API response templates
/// so integration tests don't need to repeat the same `Mock::given(...)` setup.
///
/// # Example
///
/// ```ignore
/// let mock = HttpMock::new().await;
/// mock.mock_matrix_login().await;
/// let resp = reqwest::Client::new()
///     .post(format!("{}/_matrix/client/v3/login", mock.base_url()))
///     .json(&serde_json::json!({"type": "m.login.password"}))
///     .send()
///     .await
///     .unwrap();
/// assert_eq!(resp.status(), 200);
/// ```
pub struct HttpMock {
    server: MockServer,
}

impl HttpMock {
    /// Start a new mock server and return it wrapped in an `HttpMock`.
    pub async fn new() -> Self {
        Self {
            server: MockServer::start().await,
        }
    }

    /// Return the mock server's base URI (e.g., `http://127.0.0.1:54321`).
    pub fn base_url(&self) -> String {
        self.server.uri()
    }

    /// Convenience: build an absolute URL for a given path.
    ///
    /// Example: `mock.url("/_matrix/client/v3/login")` returns
    /// `"http://127.0.0.1:54321/_matrix/client/v3/login"`.
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.server.uri(), path)
    }

    /// Mount a handler for `POST /_matrix/client/v3/login` that returns a
    /// successful login response with a mock access token.
    pub async fn mock_matrix_login(&self) {
        Mock::given(method("POST"))
            .and(path("/_matrix/client/v3/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "user_id": "@test:localhost",
                "access_token": "mock_access_token",
                "device_id": "mock_device_id",
                "home_server": "localhost"
            })))
            .mount(&self.server)
            .await;
    }

    /// Mount a custom Matrix endpoint handler with the given HTTP method,
    /// path, status code, and JSON response body.
    pub async fn mock_matrix_endpoint(
        &self,
        http_method: &str,
        endpoint_path: &str,
        status: u16,
        body: Value,
    ) {
        Mock::given(method(http_method))
            .and(path(endpoint_path))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(&self.server)
            .await;
    }

    /// Access the underlying [`MockServer`] for advanced mocking scenarios
    /// (e.g., inspecting `received_requests()` or mounting custom matchers).
    pub fn inner(&self) -> &MockServer {
        &self.server
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn smoke_test_http_mock() {
        let http_mock = HttpMock::new().await;
        http_mock.mock_matrix_login().await;

        let client = reqwest::Client::new();
        let resp = client
            .post(http_mock.url("/_matrix/client/v3/login"))
            .json(&serde_json::json!({
                "type": "m.login.password",
                "user": "test",
                "password": "password"
            }))
            .send()
            .await
            .expect("HTTP request should succeed");

        assert_eq!(resp.status(), 200);

        let body: serde_json::Value =
            resp.json().await.expect("response body should be valid JSON");
        assert_eq!(body["user_id"], "@test:localhost");
        assert_eq!(body["access_token"], "mock_access_token");
    }

    #[tokio::test]
    async fn smoke_test_custom_endpoint() {
        let http_mock = HttpMock::new().await;
        http_mock
            .mock_matrix_endpoint(
                "GET",
                "/_matrix/client/v3/versions",
                200,
                serde_json::json!({
                    "versions": ["r0.6.1"],
                    "unstable_features": {}
                }),
            )
            .await;

        let client = reqwest::Client::new();
        let resp = client
            .get(http_mock.url("/_matrix/client/v3/versions"))
            .send()
            .await
            .expect("HTTP request should succeed");

        assert_eq!(resp.status(), 200);

        let body: serde_json::Value =
            resp.json().await.expect("response body should be valid JSON");
        assert_eq!(body["versions"][0], "r0.6.1");
    }

    #[tokio::test]
    async fn base_url_includes_scheme_and_port() {
        let http_mock = HttpMock::new().await;
        let url = http_mock.base_url();
        assert!(
            url.starts_with("http://"),
            "base_url should start with http://, got: {url}"
        );
        assert!(
            url.contains("127.0.0.1"),
            "base_url should contain 127.0.0.1, got: {url}"
        );
    }
}
