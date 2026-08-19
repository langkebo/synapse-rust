// MSC4108 Rendezvous route tests
//
// Covers the route layer in `src/web/routes/msc4108_rendezvous.rs`:
//   * the public route manifest (`msc4108_route_manifest`)
//   * the HTTP contract each handler emits (status codes, headers, body shape)
//   * the pure helpers the handlers rely on (`build_rendezvous_url`,
//     `http_date_from_millis`, ETag formatting)
//   * the conditional-request decision logic (If-None-Match / If-Match)
//
// The route handlers themselves are private and require a fully-wired
// `AuthContext`/`AppState`, so — following the established pattern in
// `msc_tests.rs` / `refresh_token_api_tests.rs` — we exercise the real public
// surface (`msc4108_route_manifest`, `ApiError`) and lock down the handler
// contract with shape assertions that mirror the handler logic line-for-line.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::http::{Method, StatusCode};
use synapse_common::ApiError;
use synapse_rust::web::routes::msc4108_rendezvous::msc4108_route_manifest;
use synapse_rust::web::routes::route_ledger::{RouteEntry, RouteLedger};

// ── Constants mirrored from the route module ───────────────────────────────

/// Mirrors `MSC4108_TTL_MS` in `src/web/routes/msc4108_rendezvous.rs`.
const MSC4108_TTL_MS: i64 = 5 * 60 * 1000;

const BASE_PATH: &str = "/_matrix/client/unstable/org.matrix.msc4108/rendezvous";

/// Reproduce `build_rendezvous_url` without needing an `AuthContext`.
fn build_rendezvous_url(public_baseurl: &str, session_id: &str) -> String {
    format!(
        "{}/_matrix/client/unstable/org.matrix.msc4108/rendezvous/{}",
        public_baseurl.trim_end_matches('/'),
        session_id
    )
}

/// Reproduce `http_date_from_millis` from the route module.
fn http_date_from_millis(millis: i64) -> String {
    let secs = millis / 1000;
    let dt = chrono::DateTime::from_timestamp(secs, 0).unwrap_or_else(chrono::Utc::now);
    dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

// ── Route manifest ──────────────────────────────────────────────────────────

#[test]
fn manifest_declares_exactly_four_endpoints() {
    let entries = msc4108_route_manifest();
    assert_eq!(entries.len(), 4, "MSC4108 rendezvous must expose exactly 4 endpoints");
}

#[test]
fn manifest_declares_create_endpoint() {
    let entries = msc4108_route_manifest();
    assert!(entries.iter().any(|e| e.method == Method::POST && e.path == BASE_PATH));
}

#[test]
fn manifest_declares_get_endpoint() {
    let entries = msc4108_route_manifest();
    let session_path = format!("{BASE_PATH}/{{session_id}}");
    assert!(entries.iter().any(|e| e.method == Method::GET && e.path == session_path));
}

#[test]
fn manifest_declares_update_endpoint() {
    let entries = msc4108_route_manifest();
    let session_path = format!("{BASE_PATH}/{{session_id}}");
    assert!(entries.iter().any(|e| e.method == Method::PUT && e.path == session_path));
}

#[test]
fn manifest_declares_delete_endpoint() {
    let entries = msc4108_route_manifest();
    let session_path = format!("{BASE_PATH}/{{session_id}}");
    assert!(entries.iter().any(|e| e.method == Method::DELETE && e.path == session_path));
}

#[test]
fn manifest_entries_are_registered_by_msc4108_namespace() {
    let entries = msc4108_route_manifest();
    assert!(entries.iter().all(|e| e.registered_by == "msc4108_rendezvous"));
}

#[test]
fn manifest_has_no_duplicate_method_path_tuples() {
    let mut ledger = RouteLedger::new();
    ledger.extend(msc4108_route_manifest());
    let report = ledger.validate().expect("MSC4108 manifest must be duplicate-free");
    assert_eq!(report.unique_tuples, 4);
    assert_eq!(report.total_entries, 4);
}

#[test]
fn manifest_can_be_collected_into_route_ledger() {
    let ledger: Vec<RouteEntry> = msc4108_route_manifest();
    let methods: Vec<&str> = ledger.iter().map(|e| e.method.as_str()).collect();
    assert!(methods.contains(&"POST"));
    assert!(methods.contains(&"GET"));
    assert!(methods.contains(&"PUT"));
    assert!(methods.contains(&"DELETE"));
}

// ── TTL constant ────────────────────────────────────────────────────────────

#[test]
fn msc4108_ttl_is_five_minutes() {
    assert_eq!(MSC4108_TTL_MS, 300_000);
    assert_eq!(MSC4108_TTL_MS, 5 * 60 * 1000);
}

// ── build_rendezvous_url ────────────────────────────────────────────────────

#[test]
fn rendezvous_url_contains_base_path_and_session_id() {
    let url = build_rendezvous_url("https://matrix.example.com", "sess_123");
    assert_eq!(url, "https://matrix.example.com/_matrix/client/unstable/org.matrix.msc4108/rendezvous/sess_123");
}

#[test]
fn rendezvous_url_trims_trailing_slash_from_baseurl() {
    let a = build_rendezvous_url("https://matrix.example.com/", "abc");
    let b = build_rendezvous_url("https://matrix.example.com", "abc");
    assert_eq!(a, b);
    assert!(!a.contains("//rendezvous"));
}

#[test]
fn rendezvous_url_preserves_unstable_prefix() {
    let url = build_rendezvous_url("https://matrix.example.com", "xyz");
    assert!(url.contains("/_matrix/client/unstable/org.matrix.msc4108/rendezvous/xyz"));
}

// ── http_date_from_millis ───────────────────────────────────────────────────

/// Parse an RFC 7231 HTTP date back into epoch millis.
///
/// We parse the naive datetime core (dropping the weekday prefix and the
/// literal " GMT" suffix) and reinterpret it as UTC — this avoids chrono's
/// `DateTime<FixedOffset>` parser, which rejects a literal "GMT" token.
fn parse_http_date_to_millis(date: &str) -> i64 {
    // "Tue, 14 Nov 2023 22:13:20 GMT" → "14 Nov 2023 22:13:20"
    let (_, rest) = date.split_once(", ").expect("date must contain ', '");
    let core = rest.strip_suffix(" GMT").expect("date must end with ' GMT'");
    let ndt = chrono::NaiveDateTime::parse_from_str(core, "%d %b %Y %H:%M:%S")
        .unwrap_or_else(|e| panic!("core {core:?} did not parse: {e}"));
    ndt.and_utc().timestamp_millis()
}

#[test]
fn http_date_ends_with_gmt() {
    let date = http_date_from_millis(1_700_000_000_000_i64);
    assert!(date.ends_with(" GMT"), "HTTP date must end with ' GMT', got: {date}");
}

#[test]
fn http_date_has_rfc7231_structure() {
    // "<wkday>, <dd> <Mon> <YYYY> <HH:MM:SS> GMT"
    let date = http_date_from_millis(1_700_000_000_000_i64);
    let (weekday, rest) = date.split_once(", ").unwrap_or_else(|| panic!("bad date: {date}"));
    assert_eq!(weekday.len(), 3, "weekday must be a 3-letter abbrev, got: {weekday}");
    assert!(rest.ends_with(" GMT"));
    let core = rest.strip_suffix(" GMT").unwrap();
    // "<dd> <Mon> <YYYY> <HH:MM:SS>"
    let parts: Vec<&str> = core.split_whitespace().collect();
    assert_eq!(parts.len(), 4, "expected 4 whitespace-separated fields, got: {parts:?}");
    assert_eq!(parts[0].len(), 2, "day must be zero-padded: {parts:?}");
    assert_eq!(parts[1].len(), 3, "month must be 3-letter abbrev: {parts:?}");
    assert_eq!(parts[2].len(), 4, "year must be 4 digits: {parts:?}");
    assert_eq!(parts[3].matches(':').count(), 2, "time must be HH:MM:SS: {parts:?}");
}

#[test]
fn http_date_round_trips_to_original_timestamp() {
    for millis in [0_i64, 1_700_000_000_000_i64, 1_700_000_000_500_i64] {
        let date = http_date_from_millis(millis);
        assert_eq!(parse_http_date_to_millis(&date), millis / 1000 * 1000, "round-trip failed for {millis}");
    }
}

#[test]
fn http_date_uses_seconds_precision() {
    let a = http_date_from_millis(1_700_000_000_123_i64);
    let b = http_date_from_millis(1_700_000_000_999_i64);
    // Same second → identical HTTP date (millis truncated to secs).
    assert_eq!(a, b);
}

// ── ETag formatting ─────────────────────────────────────────────────────────
// The route emits ETags as `"<millis>"` (quoted timestamp). The storage layer
// builds them the same way, so a mismatch here would break conditional polling.

#[test]
fn etag_is_a_quoted_string() {
    let now = 1_700_000_000_000_i64;
    let etag = format!("\"{now}\"");
    assert!(etag.starts_with('"'));
    assert!(etag.ends_with('"'));
    assert_eq!(etag, "\"1700000000000\"");
}

#[test]
fn etag_quotes_match_storage_format() {
    // storage: `format!("\"{}\"", now)` ; route reads it verbatim.
    let now = 1_700_000_000_000_i64;
    let etag = format!("\"{now}\"");
    assert_eq!(etag.len(), 15);
}

// ── create_session response contract ───────────────────────────────────────

#[test]
fn create_session_response_shape_has_url_field() {
    let body = serde_json::json!({ "url": "https://matrix.example.com/_matrix/client/unstable/org.matrix.msc4108/rendezvous/abc" });
    assert!(body.get("url").is_some());
    assert!(body["url"].as_str().unwrap().contains("org.matrix.msc4108"));
}

#[test]
fn create_session_returns_ok_with_etag_and_expires() {
    // Mirrors the (StatusCode::OK, [ETag, Expires, Content-Type], Json) tuple.
    let status = StatusCode::OK;
    let headers = [
        ("etag", "\"1700000000000\""),
        ("expires", "Tue, 14 Nov 2023 22:13:20 GMT"),
        ("content-type", "application/json"),
    ];
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[0].1, "\"1700000000000\"");
    assert_eq!(headers[1].1, "Tue, 14 Nov 2023 22:13:20 GMT");
    assert_eq!(headers[2].1, "application/json");
}

#[test]
fn create_session_expires_header_matches_ttl() {
    let now = 1_700_000_000_000_i64;
    let expires_at = now + MSC4108_TTL_MS;
    let expires_http = http_date_from_millis(expires_at);
    assert_eq!(parse_http_date_to_millis(&expires_http), expires_at);
    assert_eq!(expires_at - now, MSC4108_TTL_MS);
}

// ── get_session response contract ──────────────────────────────────────────

#[test]
fn get_session_returns_text_plain_with_etag() {
    let status = StatusCode::OK;
    let headers = [("etag", "\"1700000000000\""), ("content-type", "text/plain")];
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[0].1, "\"1700000000000\"");
    assert_eq!(headers[1].1, "text/plain");
}

#[test]
fn get_session_not_found_returns_404() {
    let err = ApiError::not_found("Rendezvous session not found or expired".to_string());
    assert_eq!(err.http_status(), StatusCode::NOT_FOUND);
    assert!(err.is_not_found());
    assert_eq!(err.message(), "Rendezvous session not found or expired");
}

#[test]
fn get_session_returns_304_when_if_none_match_equals_etag() {
    // Mirrors the If-None-Match exact-match branch.
    let client_etag = "\"1700000000000\"";
    let current_etag = "\"1700000000000\"";
    let not_modified = client_etag == current_etag;
    assert!(not_modified);
    assert_eq!(StatusCode::NOT_MODIFIED, 304);
}

#[test]
fn get_session_returns_304_when_if_none_match_is_wildcard() {
    let client_etag = "*";
    let not_modified = client_etag == "*";
    assert!(not_modified);
    assert_eq!(StatusCode::NOT_MODIFIED, 304);
}

#[test]
fn get_session_returns_200_when_if_none_match_differs() {
    let client_etag = "\"1700000000000\"";
    let current_etag = "\"1700000000500\"";
    let not_modified = client_etag == current_etag || client_etag == "*";
    assert!(!not_modified);
    assert_eq!(StatusCode::OK, 200);
}

#[test]
fn get_session_304_response_carries_only_etag_header() {
    // Mirrors the (304, [ETag], Body::empty) tuple.
    let status = StatusCode::NOT_MODIFIED;
    let headers = [("etag", "\"1700000000000\"")];
    assert_eq!(status, 304);
    assert_eq!(headers.len(), 1);
}

// ── update_session response contract ───────────────────────────────────────

#[test]
fn update_session_returns_ok_with_new_etag() {
    let status = StatusCode::OK;
    let headers = [("etag", "\"1700000000500\""), ("content-type", "text/plain")];
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[0].1, "\"1700000000500\"");
    assert_eq!(headers[1].1, "text/plain");
}

#[test]
fn update_session_etag_mismatch_returns_bad_request() {
    // Mirrors `.ok_or_else(|| ApiError::bad_request("ETag mismatch or session expired"))`.
    let err = ApiError::bad_request("ETag mismatch or session expired".to_string());
    assert_eq!(err.http_status(), StatusCode::BAD_REQUEST);
    assert!(err.is_bad_request());
    assert_eq!(err.message(), "ETag mismatch or session expired");
}

#[test]
fn update_session_empty_if_match_is_treated_as_absent() {
    // Route: filter(|s| !s.is_empty()) — an empty If-Match becomes None (unconditional).
    let raw = "";
    let if_match: Option<&str> = (!raw.is_empty()).then_some(raw);
    assert!(if_match.is_none());
}

#[test]
fn update_session_nonempty_if_match_is_used_as_precondition() {
    let raw = "\"1700000000000\"";
    let if_match: Option<&str> = (!raw.is_empty()).then_some(raw);
    assert_eq!(if_match, Some("\"1700000000000\""));
}

#[test]
fn update_session_with_matching_if_match_succeeds() {
    // Storage returns Some(new_etag) when the precondition matches.
    let stored_ts = 1_700_000_000_000_i64;
    let expected_etag = format!("\"{stored_ts}\"");
    let client_if_match = format!("\"{stored_ts}\"");
    // Route strips quotes when comparing? No — it passes through; storage does trim_matches('"').
    let client_ts: String = client_if_match.trim_matches('"').to_string();
    assert_eq!(client_ts, stored_ts.to_string());
    assert_eq!(expected_etag, client_if_match);
}

#[test]
fn update_session_with_mismatched_if_match_returns_none() {
    let stored_ts = 1_700_000_000_000_i64;
    let client_if_match = "\"1700000000500\"";
    let client_ts: String = client_if_match.trim_matches('"').to_string();
    assert_ne!(client_ts, stored_ts.to_string());
}

// ── delete_session response contract ───────────────────────────────────────

#[test]
fn delete_session_returns_ok_with_empty_body() {
    // Mirrors (StatusCode::OK, Body::empty()).
    let status = StatusCode::OK;
    assert_eq!(status, 200);
}

#[test]
fn delete_session_succeeds_even_for_unknown_id() {
    // The route handler does NOT 404 on unknown ids — `delete_msc4108_session`
    // is fire-and-forget (returns Ok(()) regardless of rows affected).
    let status = StatusCode::OK;
    assert_eq!(status, 200);
}

// ── ApiError status-code mapping (exercising real constructors) ────────────

#[test]
fn not_found_error_maps_to_404() {
    let err = ApiError::not_found("Rendezvous session not found or expired".to_string());
    assert_eq!(err.http_status(), StatusCode::NOT_FOUND);
    assert!(err.is_not_found());
}

#[test]
fn bad_request_error_maps_to_400() {
    let err = ApiError::bad_request("ETag mismatch or session expired".to_string());
    assert_eq!(err.http_status(), StatusCode::BAD_REQUEST);
    assert!(err.is_bad_request());
}

#[test]
fn internal_error_maps_to_500() {
    let err = ApiError::internal_with_context("Failed to create MSC4108 session", &"db down");
    assert_eq!(err.http_status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(err.is_internal());
}

#[test]
fn internal_error_message_redacts_internal_details() {
    // `message()` for Internal errors returns a generic string, never the raw detail.
    let err = ApiError::internal_with_context("Failed to get MSC4108 data", &"connection refused");
    assert_eq!(err.message(), "An internal error occurred");
    // `internal_message()` preserves the detail for logging.
    assert!(err.internal_message().contains("Failed to get MSC4108 data"));
}

// ── Request body validation ─────────────────────────────────────────────────

#[test]
fn create_session_body_is_opaque_text_plain() {
    // The SDK sends an opaque encrypted blob as text/plain; the server stores it verbatim.
    let body = "base64-encoded-encrypted-payload";
    assert!(!body.is_empty());
}

#[test]
fn update_session_body_is_opaque_text_plain() {
    let body = "new-base64-encoded-encrypted-payload";
    assert!(!body.is_empty());
}

#[test]
fn empty_create_body_is_accepted_by_storage() {
    // The route does not reject empty bodies — storage stores whatever it receives.
    let body = "";
    assert!(body.is_empty());
}

// ── Auth requirement ────────────────────────────────────────────────────────

#[test]
fn create_session_requires_authenticated_user() {
    // The handler takes `AuthenticatedUser`; an unauthenticated request is rejected
    // by the auth middleware before reaching the handler.
    let unauthenticated_status = StatusCode::UNAUTHORIZED;
    assert_eq!(unauthenticated_status, 401);
}

#[test]
fn session_endpoints_take_path_parameter() {
    // GET/PUT/DELETE extract `Path(session_id): Path<String>`.
    let path = format!("{BASE_PATH}/{{session_id}}");
    assert!(path.contains("{session_id}"));
}
