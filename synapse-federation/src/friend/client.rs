use crate::signing::canonical_federation_request_bytes;
use crate::KeyRotationManager;
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
use ed25519_dalek::{Signer, SigningKey};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use synapse_common::{ApiError, ApiResult};

pub struct FriendFederationClient {
    client: Client,
    server_name: String,
    signing_key_id: String,
    signing_key: Option<SigningKey>,
    key_rotation_manager: Option<Arc<KeyRotationManager>>,
    missing_signing_key_logged: AtomicBool,
}

impl FriendFederationClient {
    pub fn new(server_name: String, key_rotation_manager: Option<Arc<KeyRotationManager>>) -> Self {
        let signing_key_id = std::env::var("FEDERATION_SIGNING_KEY_ID").unwrap_or_else(|_| "ed25519:0".to_string());

        let signing_key =
            std::env::var("FEDERATION_SIGNING_KEY").ok().and_then(|key_b64| Self::decode_signing_key(&key_b64));

        Self {
            client: Client::new(),
            server_name,
            signing_key_id,
            signing_key,
            key_rotation_manager,
            missing_signing_key_logged: AtomicBool::new(false),
        }
    }

    fn decode_signing_key(key_b64: &str) -> Option<SigningKey> {
        STANDARD_NO_PAD.decode(key_b64).ok().and_then(|bytes| {
            if bytes.len() == 32 {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                Some(SigningKey::from_bytes(&arr))
            } else {
                None
            }
        })
    }

    fn build_auth_header(
        &self,
        key_id: &str,
        signing_key: &SigningKey,
        method: &str,
        path: &str,
        destination: &str,
        content: Option<&Value>,
    ) -> Result<String, ApiError> {
        let message = canonical_federation_request_bytes(method, path, &self.server_name, destination, content)
            .map_err(|e| ApiError::internal(format!("Canonical JSON error: {e}")))?;

        let signature = signing_key.sign(&message);
        let sig_b64 = STANDARD_NO_PAD.encode(signature.to_bytes());

        Ok(format!(
            "X-Matrix origin={},destination={},key=\"{}\",sig=\"{}\"",
            self.server_name, destination, key_id, sig_b64
        ))
    }

    async fn sign_request(
        &self,
        method: &str,
        path: &str,
        destination: &str,
        content: Option<&Value>,
    ) -> Result<String, ApiError> {
        if let Some(signing_key) = self.signing_key.as_ref() {
            return self.build_auth_header(&self.signing_key_id, signing_key, method, path, destination, content);
        }

        if let Some(key_rotation_manager) = &self.key_rotation_manager {
            if let Some(current_key) = key_rotation_manager
                .get_current_key()
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to load federation signing key", &e))?
            {
                if let Some(signing_key) = Self::decode_signing_key(&current_key.secret_key) {
                    return self.build_auth_header(
                        &current_key.key_id,
                        &signing_key,
                        method,
                        path,
                        destination,
                        content,
                    );
                }
            }
        }

        if !self.missing_signing_key_logged.swap(true, Ordering::Relaxed) {
            tracing::warn!(
                "Friend federation signing key unavailable; checked FEDERATION_SIGNING_KEY and database-managed federation keys"
            );
        }

        Err(ApiError::internal("Federation signing key not configured".to_string()))
    }

    pub async fn send_invite(&self, destination: &str, _room_id: &str, content: &Value) -> ApiResult<()> {
        let path = format!("/_matrix/federation/v1/send/{}", uuid::Uuid::new_v4());
        let url = format!("https://{destination}{path}");

        let body_str =
            serde_json::to_string(content).map_err(|e| ApiError::internal_with_log("Failed to serialize body", &e))?;

        let auth_header = self.sign_request("PUT", &path, destination, Some(content)).await?;

        tracing::info!("Sending federation invite to {}", url);
        let response = self
            .client
            .put(&url)
            .header("Authorization", auth_header)
            .header("Content-Type", "application/json")
            .body(body_str)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_log("Federation request failed", &e))?;

        if !response.status().is_success() {
            return Err(ApiError::internal_with_log("Remote server returned error", &response.status()));
        }

        Ok(())
    }

    pub async fn query_remote_friends(&self, destination: &str, user_id: &str) -> ApiResult<Vec<String>> {
        let path = format!("/_matrix/federation/v1/user/friends/{user_id}");
        let url = format!("https://{destination}{path}");

        let auth_header = self.sign_request("GET", &path, destination, None).await?;

        tracing::info!("Querying remote friends from {}", url);
        let response = self
            .client
            .get(&url)
            .header("Authorization", auth_header)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_log("Federation request failed", &e))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }

        if !response.status().is_success() {
            return Err(ApiError::internal_with_log("Remote server returned error", &response.status()));
        }

        let body: Value =
            response.json().await.map_err(|e| ApiError::internal_with_log("Failed to parse response", &e))?;

        let friends = body
            .get("friends")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ApiError::internal("Invalid response format"))?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        Ok(friends)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_friend_federation_client_creation() {
        let client = FriendFederationClient::new("example.com".to_string(), None);
        assert_eq!(client.server_name, "example.com");
    }

    #[test]
    fn test_server_name_format() {
        let server_names = vec!["matrix.org", "example.com:8448", "server.local"];

        for name in server_names {
            let client = FriendFederationClient::new(name.to_string(), None);
            assert_eq!(client.server_name, name);
        }
    }

    #[test]
    fn test_federation_path_format() {
        let user_id = "@alice:example.com";
        let path = format!("/_matrix/federation/v1/user/friends/{user_id}");

        assert!(path.starts_with("/_matrix/federation/"));
        assert!(path.contains(user_id));
    }

    #[test]
    fn test_invite_path_format() {
        let event_id = uuid::Uuid::new_v4();
        let path = format!("/_matrix/federation/v1/send/{event_id}");

        assert!(path.starts_with("/_matrix/federation/v1/send/"));
    }

    #[test]
    fn test_friends_response_parsing() {
        let response = serde_json::json!({
            "friends": ["@alice:example.com", "@bob:example.com"]
        });

        let friends: Vec<String> = response
            .get("friends")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        assert_eq!(friends.len(), 2);
        assert!(friends.contains(&"@alice:example.com".to_string()));
    }

    #[test]
    fn test_empty_friends_response() {
        let response = serde_json::json!({
            "friends": []
        });

        let friends: Vec<String> = response
            .get("friends")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        assert!(friends.is_empty());
    }

    // ── B.3 batch 5/6 — real coverage for FriendFederationClient production paths ──
    //
    // These tests exercise `decode_signing_key`, `sign_request`, and
    // `build_auth_header` indirectly via the public `new()` constructor and
    // the public `query_remote_friends` / `send_invite` methods. Private
    // fields are inspected directly because tests live in the same module.

    use std::sync::{Mutex, OnceLock};

    /// Serialize tests that touch the process-global `FEDERATION_SIGNING_KEY*`
    /// env vars. Without this lock, parallel `cargo test` runs would race on
    /// `std::env::set_var` and produce flaky results.
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn env_lock() -> &'static Mutex<()> {
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    /// RAII guard that saves an env var, sets it to a new value, and restores
    /// (or removes) the original on drop.
    struct EnvVarGuard {
        key: &'static str,
        original: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let original = std::env::var_os(key);
            // SAFETY (2021 edition): set_var is safe in 2021; the global
            // `ENV_LOCK` serializes access so there's no data race with
            // other tests in this binary.
            std::env::set_var(key, value);
            Self { key, original }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(val) => std::env::set_var(self.key, val),
                None => std::env::remove_var(self.key),
            }
        }
    }

    /// Generate a real ed25519 signing key and base64-encode it (no pad) —
    /// exactly what `decode_signing_key` expects. Uses a fixed 32-byte seed
    /// so the test is deterministic; any 32 bytes are a valid ed25519 secret.
    fn valid_b64_signing_key() -> String {
        let signing_key = SigningKey::from_bytes(&[0x42u8; 32]);
        STANDARD_NO_PAD.encode(signing_key.to_bytes())
    }

    #[test]
    fn new_reads_signing_key_from_env_when_valid() {
        let _guard = env_lock().lock().unwrap();
        let _k = EnvVarGuard::set("FEDERATION_SIGNING_KEY", &valid_b64_signing_key());
        let _id = EnvVarGuard::set("FEDERATION_SIGNING_KEY_ID", "ed25519:test123");
        let client = FriendFederationClient::new("example.com".to_string(), None);
        assert!(client.signing_key.is_some(), "signing_key should be populated from env");
        assert_eq!(client.signing_key_id, "ed25519:test123");
    }

    #[test]
    fn new_ignores_invalid_base64_signing_key() {
        let _guard = env_lock().lock().unwrap();
        let _k = EnvVarGuard::set("FEDERATION_SIGNING_KEY", "!!!not-base64!!!");
        let client = FriendFederationClient::new("example.com".to_string(), None);
        assert!(client.signing_key.is_none(), "invalid base64 should leave signing_key None");
    }

    #[test]
    fn new_ignores_signing_key_with_wrong_byte_length() {
        let _guard = env_lock().lock().unwrap();
        // 16 bytes (valid base64, wrong length — decode_signing_key needs 32)
        let short = STANDARD_NO_PAD.encode([0u8; 16]);
        let _k = EnvVarGuard::set("FEDERATION_SIGNING_KEY", &short);
        let client = FriendFederationClient::new("example.com".to_string(), None);
        assert!(client.signing_key.is_none(), "wrong-length key should leave signing_key None");
    }

    #[test]
    fn new_uses_default_signing_key_id_when_env_unset() {
        let _guard = env_lock().lock().unwrap();
        let _id = EnvVarGuard::set("FEDERATION_SIGNING_KEY_ID", "");
        // An empty value still counts as "set" — explicitly remove it.
        std::env::remove_var("FEDERATION_SIGNING_KEY_ID");
        let client = FriendFederationClient::new("example.com".to_string(), None);
        assert_eq!(client.signing_key_id, "ed25519:0", "default should be ed25519:0");
    }

    #[tokio::test]
    async fn query_remote_friends_fails_when_no_signing_key_configured() {
        let _guard = env_lock().lock().unwrap();
        std::env::remove_var("FEDERATION_SIGNING_KEY");
        let client = FriendFederationClient::new("example.com".to_string(), None);
        let err = client.query_remote_friends("remote.example.com", "@alice:example.com").await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not configured") || msg.contains("signing key"),
            "expected signing-key-not-configured error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn query_remote_friends_builds_auth_header_then_fails_at_http() {
        // With a valid signing key configured via env, sign_request succeeds
        // and build_auth_header runs; the HTTP GET then fails because the
        // destination is non-routable.
        let _guard = env_lock().lock().unwrap();
        let _k = EnvVarGuard::set("FEDERATION_SIGNING_KEY", &valid_b64_signing_key());
        let client = FriendFederationClient::new("example.com".to_string(), None);
        // 127.0.0.1:1 reliably refuses TCP connections on most dev machines.
        let err = client.query_remote_friends("127.0.0.1:1", "@alice:example.com").await.unwrap_err();
        let msg = err.to_string();
        // The HTTP layer fails before any 2xx/4xx check — the error should
        // mention the federation request, not "not configured".
        assert!(
            msg.contains("Federation request failed") || msg.contains("request failed") || msg.contains("connect"),
            "expected HTTP/network error after sign_request success, got: {msg}"
        );
        assert!(!msg.contains("not configured"), "signing key was set; should not reach not-configured branch");
    }

    #[tokio::test]
    async fn send_invite_fails_when_no_signing_key_configured() {
        let _guard = env_lock().lock().unwrap();
        std::env::remove_var("FEDERATION_SIGNING_KEY");
        let client = FriendFederationClient::new("example.com".to_string(), None);
        let err = client
            .send_invite("remote.example.com", "!room:example.com", &serde_json::json!({}))
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not configured") || msg.contains("signing key"),
            "expected signing-key-not-configured error, got: {msg}"
        );
    }

    /// Construct a `KeyRotationManager` backed by a lazy (never-connecting)
    /// Postgres pool. The manager's in-memory `current_key` cache starts empty,
    /// so `get_current_key()` returns `Ok(None)` without touching the DB.
    /// This lets us exercise the `key_rotation_manager` branch of `sign_request`
    /// (lines 81-87) without a real database.
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    fn make_key_rotation_manager(server_name: &str) -> std::sync::Arc<crate::KeyRotationManager> {
        let pool = std::sync::Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap());
        std::sync::Arc::new(crate::KeyRotationManager::new(&pool, server_name))
    }

    #[tokio::test]
    async fn query_remote_friends_with_key_rotation_manager_but_no_key_falls_through_to_not_configured() {
        // Covers the `if let Some(key_rotation_manager)` branch (lines 81-87):
        // get_current_key() returns Ok(None) because the in-memory cache is
        // empty and no DB query is attempted. sign_request then falls through
        // to the "not configured" error.
        let _guard = env_lock().lock().unwrap();
        std::env::remove_var("FEDERATION_SIGNING_KEY");
        let manager = make_key_rotation_manager("example.com");
        let client =
            FriendFederationClient::new("example.com".to_string(), Some(manager));
        let err = client.query_remote_friends("remote.example.com", "@alice:example.com").await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not configured") || msg.contains("signing key"),
            "manager with no current_key → not-configured error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn send_invite_builds_auth_header_then_fails_at_http() {
        let _guard = env_lock().lock().unwrap();
        let _k = EnvVarGuard::set("FEDERATION_SIGNING_KEY", &valid_b64_signing_key());
        let client = FriendFederationClient::new("example.com".to_string(), None);
        let err = client
            .send_invite("127.0.0.1:1", "!room:example.com", &serde_json::json!({"k": "v"}))
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Federation request failed") || msg.contains("request failed") || msg.contains("connect"),
            "expected HTTP/network error after sign_request success, got: {msg}"
        );
    }

    #[tokio::test]
    async fn missing_signing_key_warning_logged_at_most_once() {
        // The AtomicBool::swap(true, Relaxed) gate ensures the tracing::warn
        // fires only on the first call. Two consecutive failures should both
        // return the same not-configured error.
        let _guard = env_lock().lock().unwrap();
        std::env::remove_var("FEDERATION_SIGNING_KEY");
        let client = FriendFederationClient::new("example.com".to_string(), None);
        let err1 = client.query_remote_friends("remote.example.com", "@a:ex.com").await.unwrap_err();
        let err2 = client.query_remote_friends("remote.example.com", "@b:ex.com").await.unwrap_err();
        // Both errors should mention "not configured" — the warning gate
        // doesn't change the error, only the log volume.
        assert!(err1.to_string().contains("not configured") || err1.to_string().contains("signing key"));
        assert!(err2.to_string().contains("not configured") || err2.to_string().contains("signing key"));
        // The flag should now be set.
        assert!(
            client.missing_signing_key_logged.load(std::sync::atomic::Ordering::Relaxed),
            "missing_signing_key_logged flag must be set after first call"
        );
    }
}
