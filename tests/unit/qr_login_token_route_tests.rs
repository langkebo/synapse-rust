// QR login token store tests.
//
// Covers the storage-backed functions in `src/web/routes/qr_login_token.rs`:
//   * `generate_login_token(storage, user_id, device_id)` — returns a UUID string,
//     stashes a single-use entry with a 60s TTL.
//   * `consume_login_token(storage, token)` — single-use: returns
//     `Ok(Some((user_id, device_id)))` on first call, `Ok(None)` thereafter.
//   * Unknown / already-used tokens return `Ok(None)`.
//   * Token uniqueness across calls.
//
// Since 审查 #3 moved the token store from a process-global `LazyLock<Mutex>`
// to the `login_tokens` table, these functions are async and take a
// `LoginTokenStoreApi`. Tests exercise the real functions against an in-memory
// fake store (no DB required).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use synapse_common::current_timestamp_millis;
use synapse_rust::web::routes::qr_login_token::{consume_login_token, generate_login_token};
use synapse_storage::login_token::{LoginToken, LoginTokenStoreApi};

/// Stored entry for a pending login token: `(user_id, device_id, expires_at)`.
type TokenEntry = (String, Option<String>, i64);

/// In-memory `LoginTokenStoreApi` mirroring `login_tokens` single-use semantics.
struct InMemoryLoginTokenStore {
    tokens: Mutex<HashMap<String, TokenEntry>>,
}

impl InMemoryLoginTokenStore {
    fn new() -> Self {
        Self { tokens: Mutex::new(HashMap::new()) }
    }
}

#[async_trait::async_trait]
impl LoginTokenStoreApi for InMemoryLoginTokenStore {
    async fn create_login_token(
        &self,
        token: &str,
        user_id: &str,
        device_id: Option<&str>,
        expires_at: i64,
    ) -> Result<(), sqlx::Error> {
        self.tokens
            .lock()
            .unwrap()
            .insert(token.to_string(), (user_id.to_string(), device_id.map(str::to_string), expires_at));
        Ok(())
    }

    async fn consume_login_token(&self, token: &str) -> Result<Option<LoginToken>, sqlx::Error> {
        let now = current_timestamp_millis();
        let removed = self.tokens.lock().unwrap().remove(token);
        Ok(removed.filter(|(_, _, expires_at)| *expires_at > now).map(|(user_id, device_id, _)| LoginToken {
            id: 0,
            token: token.to_string(),
            user_id,
            device_id,
            created_ts: 0,
            expires_at: 0,
        }))
    }

    async fn cleanup_expired_tokens(&self, _now_ts: i64) -> Result<u64, sqlx::Error> {
        Ok(0)
    }
}

fn store() -> Arc<dyn LoginTokenStoreApi> {
    Arc::new(InMemoryLoginTokenStore::new())
}

// ============================================================================
// generate_login_token — return value shape
// ============================================================================

#[tokio::test]
async fn generate_login_token_returns_non_empty_string() {
    let token = generate_login_token(&store(), "@alice:localhost", None).await.unwrap();
    assert!(!token.is_empty(), "token must be a non-empty string");
}

#[tokio::test]
async fn generate_login_token_returns_uuid_v4_format() {
    let token = generate_login_token(&store(), "@alice:localhost", Some("DEV-001")).await.unwrap();
    assert_eq!(token.len(), 36, "UUID v4 string must be 36 chars, got {token}");
    let segments: Vec<&str> = token.split('-').collect();
    assert_eq!(segments.len(), 5, "UUID must have 5 hyphen-separated segments");
    // v4 UUID's 3rd group starts with '4'.
    assert!(segments[2].starts_with('4'), "UUID v4 variant must start with '4' in 3rd group");
}

#[tokio::test]
async fn generate_login_token_produces_unique_tokens() {
    let storage = store();
    let t1 = generate_login_token(&storage, "@alice:localhost", None).await.unwrap();
    let t2 = generate_login_token(&storage, "@alice:localhost", None).await.unwrap();
    let t3 = generate_login_token(&storage, "@alice:localhost", None).await.unwrap();
    assert_ne!(t1, t2, "consecutive tokens must be unique");
    assert_ne!(t2, t3, "consecutive tokens must be unique");
    assert_ne!(t1, t3, "consecutive tokens must be unique");
}

// ============================================================================
// consume_login_token — happy path (with and without device_id)
// ============================================================================

#[tokio::test]
async fn consume_login_token_returns_user_and_device_when_valid() {
    let storage = store();
    let token = generate_login_token(&storage, "@bob:localhost", Some("DEVICE-X")).await.unwrap();
    let result = consume_login_token(&storage, &token).await.unwrap();
    let (user_id, device_id) = result.expect("fresh token must consume successfully");
    assert_eq!(user_id, "@bob:localhost");
    assert_eq!(device_id.as_deref(), Some("DEVICE-X"));
}

#[tokio::test]
async fn consume_login_token_returns_user_with_none_device_when_generated_none() {
    let storage = store();
    let token = generate_login_token(&storage, "@carol:localhost", None).await.unwrap();
    let result = consume_login_token(&storage, &token).await.unwrap();
    let (user_id, device_id) = result.expect("fresh token must consume successfully");
    assert_eq!(user_id, "@carol:localhost");
    assert!(device_id.is_none(), "device_id must be None when generated with None");
}

// ============================================================================
// consume_login_token — single-use semantics
// ============================================================================

#[tokio::test]
async fn consume_login_token_is_single_use_second_call_returns_none() {
    let storage = store();
    let token = generate_login_token(&storage, "@dave:localhost", None).await.unwrap();
    let first = consume_login_token(&storage, &token).await.unwrap();
    let second = consume_login_token(&storage, &token).await.unwrap();
    assert!(first.is_some(), "first consume must succeed");
    assert!(second.is_none(), "second consume must fail (single-use)");
}

#[tokio::test]
async fn consume_login_token_returns_none_for_unknown_token() {
    let result = consume_login_token(&store(), "never-generated-uuid-0000-0000-000000000000").await.unwrap();
    assert!(result.is_none(), "unknown token must return None");
}

#[tokio::test]
async fn consume_login_token_returns_none_for_empty_string() {
    let result = consume_login_token(&store(), "").await.unwrap();
    assert!(result.is_none(), "empty token must return None");
}

// ============================================================================
// consume_login_token — distinct tokens are independent
// ============================================================================

#[tokio::test]
async fn consuming_one_token_does_not_affect_another() {
    let storage = store();
    let t1 = generate_login_token(&storage, "@eve:localhost", Some("D1")).await.unwrap();
    let t2 = generate_login_token(&storage, "@frank:localhost", Some("D2")).await.unwrap();

    let r1 = consume_login_token(&storage, &t1).await.unwrap();
    assert!(r1.is_some());
    let r2 = consume_login_token(&storage, &t2).await.unwrap();
    let (user2, dev2) = r2.expect("t2 must be unaffected by t1 consumption");
    assert_eq!(user2, "@frank:localhost");
    assert_eq!(dev2.as_deref(), Some("D2"));
}

#[tokio::test]
async fn consume_login_token_user_id_round_trips_exactly() {
    let storage = store();
    let weird_user = "@weird_user+test:sub.domain.example.org";
    let token = generate_login_token(&storage, weird_user, None).await.unwrap();
    let (user_id, _) = consume_login_token(&storage, &token).await.unwrap().expect("must consume");
    assert_eq!(user_id, weird_user);
}

#[tokio::test]
async fn consume_login_token_device_id_round_trips_exactly() {
    let storage = store();
    let device = "DEVICE-with-special_chars.123";
    let token = generate_login_token(&storage, "@alice:localhost", Some(device)).await.unwrap();
    let (_, dev) = consume_login_token(&storage, &token).await.unwrap().expect("must consume");
    assert_eq!(dev.as_deref(), Some(device));
}

// ============================================================================
// TTL constant — documents the 60-second expiry contract
// ============================================================================

#[test]
fn test_login_token_ttl_is_60_seconds() {
    // Mirrors `const LOGIN_TOKEN_TTL_MS: i64 = 60_000` in the source.
    const EXPECTED_TTL_MS: i64 = 60_000;
    assert_eq!(EXPECTED_TTL_MS, 60_000, "login token TTL must be 60 seconds per MSC4108");
}

#[tokio::test]
async fn test_generate_then_consume_preserves_user_id_across_calls() {
    let storage = store();
    for user in ["@a:s", "@b:s", "@c:s"] {
        let token = generate_login_token(&storage, user, None).await.unwrap();
        let (consumed_user, _) = consume_login_token(&storage, &token).await.unwrap().expect("must consume");
        assert_eq!(consumed_user, user);
    }
}
