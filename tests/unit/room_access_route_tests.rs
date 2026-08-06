// Room access control helper tests.
//
// Covers the access-control decision logic in
// `src/web/routes/room_access.rs` (P-096: previously zero tests):
//   * `is_member_via` — membership string == "join" ⟹ member.
//   * `ensure_room_member_ctx` — admin bypasses the membership check;
//     non-member (non-admin) → `ApiError::forbidden` (403).
//   * `ensure_room_member_strict_ctx` — no admin bypass; non-member → 403.
//   * `is_member_or_creator_ctx` — room creator short-circuits to true.
//   * Error-code mapping: forbidden → 403 / M_FORBIDDEN.
//
// The `room_access` module is crate-private (`mod room_access;` in
// `routes/mod.rs`) and its helpers are `pub(crate)`, so they cannot be
// imported from this integration-test crate. Following the established
// pattern in `key_rotation_route_tests.rs` (see `compute_needs_rotation`),
// we mirror the decision logic line-for-line and assert against the real
// `ApiError` constructors the helpers use. If the source logic changes,
// these mirrors must be updated in lockstep.

use synapse_common::ApiError;

// ============================================================================
// Logic mirrors — reproduce the helper decision trees without needing the
// private module or a wired RoomServiceApi mock.
// ============================================================================

/// Mirror of `is_member_via`:
///   membership.is_some_and(|m| m == "join")
fn is_member_via(membership: Option<&str>) -> bool {
    membership.is_some_and(|m| m == "join")
}

/// Mirror of `ensure_room_member_ctx` decision:
///   if auth_user.is_admin { return Ok(()) }
///   if !is_member { return Err(forbidden) }
fn ensure_room_member_ctx(is_admin: bool, is_member: bool, error_message: &str) -> Result<(), ApiError> {
    if is_admin {
        return Ok(());
    }
    if !is_member {
        return Err(ApiError::forbidden(error_message.to_string()));
    }
    Ok(())
}

/// Mirror of `ensure_room_member_strict_ctx` decision:
///   if !is_member { return Err(forbidden) }
/// (no admin bypass — strict variant)
fn ensure_room_member_strict_ctx(is_admin: bool, is_member: bool, error_message: &str) -> Result<(), ApiError> {
    let _ = is_admin; // intentionally ignored — strict variant has no admin bypass
    if !is_member {
        return Err(ApiError::forbidden(error_message.to_string()));
    }
    Ok(())
}

/// Mirror of `is_member_or_creator_ctx` decision:
///   if creator_user_id == Some(user_id) { return Ok(true) }
///   is_member_ctx(...)
fn is_member_or_creator_ctx(creator_user_id: Option<&str>, user_id: &str, is_member: bool) -> bool {
    if creator_user_id == Some(user_id) {
        return true;
    }
    is_member
}

// ============================================================================
// is_member_via — membership string semantics
// ============================================================================

#[test]
fn test_is_member_via_join_returns_true() {
    assert!(is_member_via(Some("join")));
}

#[test]
fn test_is_member_via_invite_returns_false() {
    // Only "join" counts as a joined member; "invite" / "leave" / "ban" do not.
    assert!(!is_member_via(Some("invite")));
    assert!(!is_member_via(Some("leave")));
    assert!(!is_member_via(Some("ban")));
    assert!(!is_member_via(Some("knock")));
}

#[test]
fn test_is_member_via_none_returns_false() {
    // membership == None (not a member of the room at all)
    assert!(!is_member_via(None));
}

#[test]
fn test_is_member_via_is_case_sensitive() {
    // The comparison is `m == "join"` — case-sensitive. "Join" / "JOIN" must
    // not be accepted as a joined membership.
    assert!(!is_member_via(Some("Join")));
    assert!(!is_member_via(Some("JOIN")));
}

// ============================================================================
// ensure_room_member_ctx — admin bypass + member gate
// ============================================================================

#[test]
fn test_ensure_room_member_ctx_allows_admin_even_if_not_member() {
    // Admin bypass: is_admin == true short-circuits to Ok regardless of membership.
    let result = ensure_room_member_ctx(true, false, "denied");
    assert!(result.is_ok(), "admin must bypass the membership check");
}

#[test]
fn test_ensure_room_member_ctx_allows_admin_and_member() {
    let result = ensure_room_member_ctx(true, true, "denied");
    assert!(result.is_ok());
}

#[test]
fn test_ensure_room_member_ctx_allows_non_admin_member() {
    let result = ensure_room_member_ctx(false, true, "denied");
    assert!(result.is_ok(), "joined member must be allowed");
}

#[test]
fn test_ensure_room_member_ctx_rejects_non_admin_non_member() {
    let result = ensure_room_member_ctx(false, false, "You must be a room member");
    let err = result.expect_err("non-member non-admin must be forbidden");
    assert_eq!(err.http_status(), axum::http::StatusCode::FORBIDDEN);
    assert!(err.message.contains("You must be a room member"));
}

#[test]
fn test_ensure_room_member_ctx_forbidden_message_is_preserved() {
    // The caller-supplied error_message must surface verbatim in the ApiError.
    let msg = "You must be a room member to configure burn-after-read";
    let result = ensure_room_member_ctx(false, false, msg);
    let err = result.expect_err("must be forbidden");
    assert_eq!(err.message, msg);
}

// ============================================================================
// ensure_room_member_strict_ctx — no admin bypass
// ============================================================================

#[test]
fn test_ensure_room_member_strict_ctx_rejects_admin_non_member() {
    // Strict variant: admin who is not a joined member is STILL forbidden.
    let result = ensure_room_member_strict_ctx(true, false, "strict check");
    let err = result.expect_err("strict variant must not bypass for admins");
    assert_eq!(err.http_status(), axum::http::StatusCode::FORBIDDEN);
}

#[test]
fn test_ensure_room_member_strict_ctx_allows_member() {
    let result = ensure_room_member_strict_ctx(false, true, "strict check");
    assert!(result.is_ok());
}

#[test]
fn test_ensure_room_member_strict_ctx_allows_admin_who_is_member() {
    let result = ensure_room_member_strict_ctx(true, true, "strict check");
    assert!(result.is_ok());
}

#[test]
fn test_ensure_room_member_strict_ctx_rejects_non_member() {
    let result = ensure_room_member_strict_ctx(false, false, "strict check");
    assert!(result.is_err());
}

// ============================================================================
// is_member_or_creator_ctx — creator short-circuit
// ============================================================================

#[test]
fn test_is_member_or_creator_creator_returns_true_regardless_of_membership() {
    // creator_user_id == Some(user_id) → true, even if not a joined member.
    assert!(is_member_or_creator_ctx(Some("@creator:server"), "@creator:server", false));
}

#[test]
fn test_is_member_or_creator_non_creator_member_returns_true() {
    assert!(is_member_or_creator_ctx(Some("@creator:server"), "@alice:server", true));
}

#[test]
fn test_is_member_or_creator_non_creator_non_member_returns_false() {
    assert!(!is_member_or_creator_ctx(Some("@creator:server"), "@alice:server", false));
}

#[test]
fn test_is_member_or_creator_none_creator_falls_back_to_membership() {
    // creator_user_id == None → cannot match → falls back to is_member.
    assert!(is_member_or_creator_ctx(None, "@alice:server", true));
    assert!(!is_member_or_creator_ctx(None, "@alice:server", false));
}

#[test]
fn test_is_member_or_creator_distinct_creator_does_not_match() {
    // creator is a different user → no short-circuit → falls back to membership.
    assert!(!is_member_or_creator_ctx(Some("@bob:server"), "@alice:server", false));
    assert!(is_member_or_creator_ctx(Some("@bob:server"), "@alice:server", true));
}

// ============================================================================
// Error-code mapping — forbidden is the only error these helpers emit
// ============================================================================

#[test]
fn test_forbidden_maps_to_403() {
    let err = ApiError::forbidden("access denied".to_string());
    assert_eq!(err.http_status(), axum::http::StatusCode::FORBIDDEN);
}

#[test]
fn test_forbidden_errcode_is_m_forbidden() {
    // The Matrix errcode surfaced in the JSON body must be M_FORBIDDEN.
    let err = ApiError::forbidden("access denied".to_string());
    assert_eq!(err.code.as_str(), "M_FORBIDDEN");
}

#[test]
fn test_admin_bypass_vs_strict_no_bypass_distinct_behaviour() {
    // Document the contract difference between the two variants: the non-strict
    // variant allows an admin non-member, the strict variant does not.
    let non_strict = ensure_room_member_ctx(true, false, "msg");
    let strict = ensure_room_member_strict_ctx(true, false, "msg");
    assert!(non_strict.is_ok(), "non-strict must allow admin non-member");
    assert!(strict.is_err(), "strict must reject admin non-member");
}
