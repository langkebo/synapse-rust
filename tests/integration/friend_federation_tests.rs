//! Federation-layer tests for the friend module.
//!
//! Covers `FriendFederation::on_receive_friend_request` (all 5 branches) and
//! `FriendFederationClient` construction + HTTP-path smoke tests.
//!
//! Since `FriendFederationClient` enforces HTTPS (`https://{destination}{path}`)
//! and its internal fields are private, the HTTP-path tests use unreachable
//! destinations to verify that the signing key loads successfully (i.e. the
//! error is a network error, NOT "Federation signing key not configured").
//! This covers the `sign_request` success branch + `reqwest::Client::send`
//! invocation, even though the request itself fails.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{json, Value};
use std::sync::{Arc, Mutex, OnceLock};

use synapse_common::federation_test_keys::generate_federation_test_keypair;
use synapse_common::traits::FriendRoomProvider;
use synapse_common::ApiError;
use synapse_federation::friend::{FriendFederation, FriendFederationClient};

// ---------------------------------------------------------------------------
// Signing key bootstrap (set ONCE per test binary; all HTTP tests share it)
// ---------------------------------------------------------------------------

static TEST_SIGNING_KEY: OnceLock<()> = OnceLock::new();

/// Generates an ed25519 signing key via the project's `test-utils` helper and
/// sets `FEDERATION_SIGNING_KEY` / `FEDERATION_SIGNING_KEY_ID` env vars exactly
/// once per test binary. Subsequent calls are no-ops.
fn ensure_test_signing_key() {
    TEST_SIGNING_KEY.get_or_init(|| {
        let keypair = generate_federation_test_keypair();
        // SAFETY: Integration tests run in a single binary process; this env var
        // is set exactly once via `OnceLock` and never modified afterward. Other
        // tests in the same binary that read the env var will see a stable value.
        // The `unsafe` block silences Rust 1.93's `env::set_var` deprecation.
        unsafe {
            std::env::set_var("FEDERATION_SIGNING_KEY", &keypair.secret_key);
            std::env::set_var("FEDERATION_SIGNING_KEY_ID", &keypair.key_id);
        }
        ()
    });
}

// ---------------------------------------------------------------------------
// Mock FriendRoomProvider
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MockFriendRoomProvider {
    received: Mutex<Option<(String, String, Value)>>,
}

#[async_trait::async_trait]
impl FriendRoomProvider for MockFriendRoomProvider {
    async fn handle_incoming_friend_request(
        &self,
        user_id: &str,
        requester_id: &str,
        content: Value,
    ) -> Result<(), ApiError> {
        *self.received.lock().unwrap() =
            Some((user_id.to_string(), requester_id.to_string(), content));
        Ok(())
    }
}

/// Builds a `FriendFederation` backed by a fresh `MockFriendRoomProvider`.
fn build_federation() -> (FriendFederation, Arc<MockFriendRoomProvider>) {
    let provider = Arc::new(MockFriendRoomProvider::default());
    let federation = FriendFederation::new(provider.clone());
    (federation, provider)
}

// ===========================================================================
// Group A: FriendFederation::on_receive_friend_request (5 branches)
// ===========================================================================

#[tokio::test]
async fn test_on_receive_empty_origin_returns_forbidden() {
    let (federation, _) = build_federation();
    let content = json!({
        "target_user_id": "@bob:foo.bar",
        "requester_id": "@alice:foo.bar",
    });
    let result = federation.on_receive_friend_request("", content).await;
    assert!(result.is_err(), "expected error for empty origin");
    let msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        msg.contains("origin"),
        "expected error message to mention 'origin', got: {msg}"
    );
}

#[tokio::test]
async fn test_on_receive_missing_target_user_id_returns_bad_request() {
    let (federation, _) = build_federation();
    let content = json!({
        "requester_id": "@alice:foo.bar",
    });
    let result = federation.on_receive_friend_request("foo.bar", content).await;
    assert!(result.is_err(), "expected error for missing target_user_id");
    let msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        msg.contains("target_user_id"),
        "expected error message to mention 'target_user_id', got: {msg}"
    );
}

#[tokio::test]
async fn test_on_receive_missing_requester_id_returns_bad_request() {
    let (federation, _) = build_federation();
    let content = json!({
        "target_user_id": "@bob:foo.bar",
    });
    let result = federation.on_receive_friend_request("foo.bar", content).await;
    assert!(result.is_err(), "expected error for missing requester_id");
    let msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        msg.contains("requester_id"),
        "expected error message to mention 'requester_id', got: {msg}"
    );
}

#[tokio::test]
async fn test_on_receive_origin_mismatch_returns_forbidden() {
    let (federation, _) = build_federation();
    // requester_id ends with ":other.bar" but origin is "foo.bar"
    let content = json!({
        "target_user_id": "@bob:foo.bar",
        "requester_id": "@alice:other.bar",
    });
    let result = federation.on_receive_friend_request("foo.bar", content).await;
    assert!(result.is_err(), "expected error for origin mismatch");
    let msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        msg.contains("origin") || msg.contains("match") || msg.contains("requester"),
        "expected error message about origin/requester mismatch, got: {msg}"
    );
}

#[tokio::test]
async fn test_on_receive_success_calls_provider() {
    let (federation, provider) = build_federation();
    let content = json!({
        "target_user_id": "@bob:foo.bar",
        "requester_id": "@alice:foo.bar",
        "message": "hi from afar",
    });
    let result = federation.on_receive_friend_request("foo.bar", content.clone()).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());

    let received = provider.received.lock().unwrap().clone();
    let (target, requester, content) = received.expect("provider should have been called");
    assert_eq!(target, "@bob:foo.bar");
    assert_eq!(requester, "@alice:foo.bar");
    assert_eq!(content.get("message").and_then(|v| v.as_str()), Some("hi from afar"));
}

// ===========================================================================
// Group B: FriendFederationClient smoke tests (5 tests)
// ===========================================================================
//
// All HTTP-path tests call `ensure_test_signing_key()` first so that
// `sign_request` succeeds and the request reaches `reqwest::Client::send`,
// which then fails with a network/DNS error. This covers the sign_request
// happy path + HTTP send invocation, even though the request itself fails.

#[tokio::test]
async fn test_client_creation_does_not_panic() {
    ensure_test_signing_key();
    let _client = FriendFederationClient::new("local.test".to_string(), None);
    // Constructing with a key rotation manager (None) also should not panic.
    let _client2 = FriendFederationClient::new("matrix.org:8448".to_string(), None);
}

#[tokio::test]
async fn test_client_creation_with_various_server_names() {
    ensure_test_signing_key();
    for name in &["matrix.org", "example.com:8448", "server.local", "localhost"] {
        let _client = FriendFederationClient::new(name.to_string(), None);
        // No panic; construction succeeds for each name.
    }
}

#[tokio::test]
async fn test_client_send_invite_to_unreachable_returns_network_err_not_signing_err() {
    ensure_test_signing_key();
    let client = FriendFederationClient::new("local.test".to_string(), None);
    // Use a guaranteed-unresolvable hostname. The TLD ".invalid" is reserved
    // by RFC 2606 and will yield a DNS error.
    let result = client
        .send_invite("nonexistent.invalid", "!room:local.test", &json!({ "type": "m.room.member" }))
        .await;
    assert!(result.is_err(), "expected error for unreachable destination");
    let msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        !msg.contains("signing key not configured"),
        "signing key should be loaded; got signing-key error: {msg}"
    );
    // The error should be a network/DNS error.
    assert!(
        msg.contains("federation request failed") || msg.contains("error") || msg.contains("dns"),
        "expected network error, got: {msg}"
    );
}

#[tokio::test]
async fn test_client_query_remote_friends_to_unreachable_returns_network_err_not_signing_err() {
    ensure_test_signing_key();
    let client = FriendFederationClient::new("local.test".to_string(), None);
    let result = client
        .query_remote_friends("nonexistent.invalid", "@user:local.test")
        .await;
    assert!(result.is_err(), "expected error for unreachable destination");
    let msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        !msg.contains("signing key not configured"),
        "signing key should be loaded; got signing-key error: {msg}"
    );
}

#[tokio::test]
async fn test_client_query_remote_friends_with_invalid_user_id_format_does_not_panic() {
    ensure_test_signing_key();
    let client = FriendFederationClient::new("local.test".to_string(), None);
    // Oddly-formatted user_ids should be substituted into the path without panic.
    // The request will fail at the network layer, not at string formatting.
    let result = client.query_remote_friends("nonexistent.invalid", "not-a-valid-user-id").await;
    assert!(result.is_err(), "expected error");
    let msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        !msg.contains("signing key not configured"),
        "signing key should be loaded; got signing-key error: {msg}"
    );
}
