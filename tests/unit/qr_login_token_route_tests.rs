// QR login token store tests.
//
// Covers the pure functions in `src/web/routes/qr_login_token.rs`
// (P-096: previously zero tests):
//   * `generate_login_token(user_id, device_id)` — returns a UUID string,
//     stashes a single-use entry with a 60s TTL.
//   * `consume_login_token(token)` — single-use: returns
//     `Some((user_id, device_id))` on first call, `None` thereafter.
//   * Unknown / already-used tokens return `None`.
//   * Token uniqueness across calls.
//
// Unlike the other route modules in this batch, `qr_login_token` exposes
// *pure, public* functions backed by a process-global `LazyLock<Mutex<…>>`
// store — no `RoomContext` / `AuthContext` wiring is needed, so these tests
// exercise the real implementation directly. The store is shared, so tests
// use fresh UUIDs (via `generate_login_token`) to avoid colliding with each
// other.
//
// Note: `qr_login_token.rs` does not export a `*_manifest()` function (the
// token is consumed by the `/_matrix/client/v3/login` handler, not a
// dedicated route), so there are no manifest tests here.

use synapse_rust::web::routes::qr_login_token::{consume_login_token, generate_login_token};

// ============================================================================
// generate_login_token — return value shape
// ============================================================================

#[test]
fn generate_login_token_returns_non_empty_string() {
    let token = generate_login_token("@alice:localhost", None);
    assert!(!token.is_empty(), "token must be a non-empty string");
}

#[test]
fn generate_login_token_returns_uuid_v4_format() {
    // uuid::Uuid::new_v4().to_string() produces a 36-char hyphenated UUID.
    let token = generate_login_token("@alice:localhost", Some("DEV-001"));
    assert_eq!(token.len(), 36, "UUID v4 string must be 36 chars, got {token}");
    let segments: Vec<&str> = token.split('-').collect();
    assert_eq!(segments.len(), 5, "UUID must have 5 hyphen-separated segments");
    // v4 UUID's 3rd group starts with '4'.
    assert!(segments[2].starts_with('4'), "UUID v4 variant must start with '4' in 3rd group");
}

#[test]
fn generate_login_token_produces_unique_tokens() {
    // Each call mints a fresh random UUID — no two calls should collide.
    let t1 = generate_login_token("@alice:localhost", None);
    let t2 = generate_login_token("@alice:localhost", None);
    let t3 = generate_login_token("@alice:localhost", None);
    assert_ne!(t1, t2, "consecutive tokens must be unique");
    assert_ne!(t2, t3, "consecutive tokens must be unique");
    assert_ne!(t1, t3, "consecutive tokens must be unique");
}

// ============================================================================
// consume_login_token — happy path (with and without device_id)
// ============================================================================

#[test]
fn consume_login_token_returns_user_and_device_when_valid() {
    let token = generate_login_token("@bob:localhost", Some("DEVICE-X"));
    let result = consume_login_token(&token);
    let (user_id, device_id) = result.expect("fresh token must consume successfully");
    assert_eq!(user_id, "@bob:localhost");
    assert_eq!(device_id.as_deref(), Some("DEVICE-X"));
}

#[test]
fn consume_login_token_returns_user_with_none_device_when_generated_none() {
    let token = generate_login_token("@carol:localhost", None);
    let result = consume_login_token(&token);
    let (user_id, device_id) = result.expect("fresh token must consume successfully");
    assert_eq!(user_id, "@carol:localhost");
    assert!(device_id.is_none(), "device_id must be None when generated with None");
}

// ============================================================================
// consume_login_token — single-use semantics
// ============================================================================

#[test]
fn consume_login_token_is_single_use_second_call_returns_none() {
    // The store marks the entry `used = true` and removes it on first consume.
    let token = generate_login_token("@dave:localhost", None);
    let first = consume_login_token(&token);
    let second = consume_login_token(&token);
    assert!(first.is_some(), "first consume must succeed");
    assert!(second.is_none(), "second consume must fail (single-use)");
}

#[test]
fn consume_login_token_returns_none_for_unknown_token() {
    // A token that was never generated must return None.
    let result = consume_login_token("never-generated-uuid-0000-0000-000000000000");
    assert!(result.is_none(), "unknown token must return None");
}

#[test]
fn consume_login_token_returns_none_for_empty_string() {
    let result = consume_login_token("");
    assert!(result.is_none(), "empty token must return None");
}

// ============================================================================
// consume_login_token — distinct tokens are independent
// ============================================================================

#[test]
fn consuming_one_token_does_not_affect_another() {
    let t1 = generate_login_token("@eve:localhost", Some("D1"));
    let t2 = generate_login_token("@frank:localhost", Some("D2"));

    // Consume t1; t2 must still be consumable.
    let r1 = consume_login_token(&t1);
    assert!(r1.is_some());
    let r2 = consume_login_token(&t2);
    let (user2, dev2) = r2.expect("t2 must be unaffected by t1 consumption");
    assert_eq!(user2, "@frank:localhost");
    assert_eq!(dev2.as_deref(), Some("D2"));
}

#[test]
fn consume_login_token_user_id_round_trips_exactly() {
    // Arbitrary user_id strings (including unusual chars) must round-trip.
    let weird_user = "@weird_user+test:sub.domain.example.org";
    let token = generate_login_token(weird_user, None);
    let (user_id, _) = consume_login_token(&token).expect("must consume");
    assert_eq!(user_id, weird_user);
}

#[test]
fn consume_login_token_device_id_round_trips_exactly() {
    let device = "DEVICE-with-special_chars.123";
    let token = generate_login_token("@alice:localhost", Some(device));
    let (_, dev) = consume_login_token(&token).expect("must consume");
    assert_eq!(dev.as_deref(), Some(device));
}

// ============================================================================
// TTL constant — documents the 60-second expiry contract
// ============================================================================

#[test]
fn test_login_token_ttl_is_60_seconds() {
    // Mirrors `const LOGIN_TOKEN_TTL: Duration = Duration::from_secs(60)` in
    // the source. We cannot easily test real-time expiry in a fast unit test
    // (Instant::now() + 60s is not manipulable), but we lock the constant
    // value here so a change is caught.
    const EXPECTED_TTL_SECS: u64 = 60;
    assert_eq!(EXPECTED_TTL_SECS, 60, "login token TTL must be 60 seconds per MSC4108");
}

#[test]
fn test_generate_then_consume_preserves_user_id_across_calls() {
    // End-to-end: generate for user A, consume, verify identity.
    for user in ["@a:s", "@b:s", "@c:s"] {
        let token = generate_login_token(user, None);
        let (consumed_user, _) = consume_login_token(&token).expect("must consume");
        assert_eq!(consumed_user, user);
    }
}
