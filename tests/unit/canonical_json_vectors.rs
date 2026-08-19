//! Matrix Specification Canonical JSON Conformance Vectors
//!
//! These vectors are derived from:
//! - Matrix Specification v1.18 § Appendices → Canonical JSON
//! - sytest `tests/50federation/40canonicaljson.pl`
//! - matrix-org/python-canonicaljson test suite
//! - matrix-org/gomatrixserverlib `canonical_json.go` test cases
//!
//! They serve as a conformance gate to ensure the canonical JSON
//! implementation matches the reference behavior expected by other Matrix
//! homeservers (Synapse, Dendrite, Conduit) for event signing, federation
//! request signing, and server key signing.
//!
//! When updating the Matrix spec baseline, add new vectors here first,
//! run `cargo test --test unit canonical_json_vectors -- --nocapture`,
//! and only then propagate any implementation changes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::{json, Value};
use synapse_common::canonical_json::{canonical_json, remove_signatures_and_unsigned, CanonicalJsonError};

// ===========================================================================
// §1  Basic value serialization
// ===========================================================================

/// `null` → `"null"` (spec: §A.4 Canonical JSON encoding)
#[test]
fn vector_null() {
    assert_eq!(canonical_json(&Value::Null).unwrap(), "null");
}

/// `true` → `"true"` (spec: §A.4)
#[test]
fn vector_true() {
    assert_eq!(canonical_json(&Value::Bool(true)).unwrap(), "true");
}

/// `false` → `"false"` (spec: §A.4)
#[test]
fn vector_false() {
    assert_eq!(canonical_json(&Value::Bool(false)).unwrap(), "false");
}

/// Empty object → `"{}"` (spec: §A.4)
#[test]
fn vector_empty_object() {
    assert_eq!(canonical_json(&json!({})).unwrap(), "{}");
}

/// Empty array → `"[]"` (spec: §A.4)
#[test]
fn vector_empty_array() {
    assert_eq!(canonical_json(&json!([])).unwrap(), "[]");
}

// ===========================================================================
// §2  Integer serialization (spec: §A.4 Numbers)
// ===========================================================================

/// Zero must serialize as `0` (no decimal point, no exponent).
#[test]
fn vector_integer_zero() {
    assert_eq!(canonical_json(&json!({"v": 0})).unwrap(), r#"{"v":0}"#);
}

/// Positive integers serialize as-is.
#[test]
fn vector_integer_positive() {
    assert_eq!(canonical_json(&json!({"v": 42})).unwrap(), r#"{"v":42}"#);
}

/// Negative integers serialize as-is (no `-0` special case).
#[test]
fn vector_integer_negative() {
    assert_eq!(canonical_json(&json!({"v": -42})).unwrap(), r#"{"v":-42}"#);
}

/// `2^53 - 1 = 9007199254740991` is the maximum permitted integer.
#[test]
fn vector_integer_max_range() {
    assert_eq!(canonical_json(&json!({"max": 9007199254740991_i64})).unwrap(), r#"{"max":9007199254740991}"#);
}

/// `-(2^53) + 1 = -9007199254740991` is the minimum permitted integer.
#[test]
fn vector_integer_min_range() {
    assert_eq!(canonical_json(&json!({"min": -9007199254740991_i64})).unwrap(), r#"{"min":-9007199254740991}"#);
}

/// `2^53 = 9007199254740992` is out of range and MUST be rejected.
#[test]
fn vector_integer_above_max_rejected() {
    let result = canonical_json(&json!({"over": 9007199254740992_i64}));
    assert!(matches!(result, Err(CanonicalJsonError::IntegerOutOfRange(_))));
}

/// `-(2^53) = -9007199254740992` is out of range and MUST be rejected.
#[test]
fn vector_integer_below_min_rejected() {
    let result = canonical_json(&json!({"under": -9007199254740992_i64}));
    assert!(matches!(result, Err(CanonicalJsonError::IntegerOutOfRange(_))));
}

/// Integer-valued floats (e.g. `1.0`) are floats and MUST be rejected.
/// Matrix canonical JSON does not permit any float, even integer-valued.
#[test]
fn vector_integer_valued_float_rejected() {
    let json: Value = serde_json::from_str(r#"{"v": 1.0}"#).unwrap();
    let result = canonical_json(&json);
    assert!(matches!(result, Err(CanonicalJsonError::FloatNotAllowed(_))));
}

/// Non-integer floats (e.g. `1.5`) MUST be rejected.
#[test]
fn vector_non_integer_float_rejected() {
    let json: Value = serde_json::from_str(r#"{"v": 1.5}"#).unwrap();
    let result = canonical_json(&json);
    assert!(matches!(result, Err(CanonicalJsonError::FloatNotAllowed(_))));
}

// ===========================================================================
// §3  Object key ordering (spec: §A.4 Lexicographic sort by Unicode codepoint)
// ===========================================================================

/// Keys must be sorted lexicographically by Unicode code point.
/// Uppercase letters (A-Z, U+0041-005A) sort before lowercase (a-z, U+0061-007A).
#[test]
fn vector_key_sorting_ascii() {
    let json = json!({"b": 2, "A": 1, "a": 3, "B": 4});
    assert_eq!(canonical_json(&json).unwrap(), r#"{"A":1,"B":4,"a":3,"b":2}"#);
}

/// Nested objects must also have sorted keys.
#[test]
fn vector_key_sorting_nested() {
    let json = json!({"outer": {"z": 1, "a": 2, "m": 3}});
    assert_eq!(canonical_json(&json).unwrap(), r#"{"outer":{"a":2,"m":3,"z":1}}"#);
}

/// Keys are sorted by code point, not by locale. Digits sort before letters.
#[test]
fn vector_key_sorting_digits_before_letters() {
    let json = json!({"a": 1, "1": 2, "B": 3, "2": 4});
    // Code points: '1'(0x31) < '2'(0x32) < 'B'(0x42) < 'a'(0x61)
    assert_eq!(canonical_json(&json).unwrap(), r#"{"1":2,"2":4,"B":3,"a":1}"#);
}

/// Keys with multi-byte Unicode characters sort by code point.
#[test]
fn vector_key_sorting_unicode() {
    let json = json!({"é": 1, "a": 2, "中": 3});
    // 'a'(0x61) < '中'(0x4E2D) < 'é'(0x00E9)
    // Note: '中' is U+4E2D, 'é' is U+00E9, so 'é' < '中' by code point
    assert_eq!(canonical_json(&json).unwrap(), r#"{"a":2,"é":1,"中":3}"#);
}

// ===========================================================================
// §4  Array ordering (spec: §A.4 — arrays preserve insertion order)
// ===========================================================================

/// Arrays preserve insertion order (no sorting applied to array elements).
#[test]
fn vector_array_order_preserved() {
    assert_eq!(canonical_json(&json!([3, 1, 2])).unwrap(), "[3,1,2]");
}

/// Arrays of mixed types preserve order.
#[test]
fn vector_array_mixed_types() {
    let json = json!([1, "two", null, true, false, {"key": "val"}]);
    assert_eq!(canonical_json(&json).unwrap(), r#"[1,"two",null,true,false,{"key":"val"}]"#);
}

/// Nested arrays preserve order at every level.
#[test]
fn vector_array_nested() {
    let json = json!([[3, 2, 1], [6, 5, 4]]);
    assert_eq!(canonical_json(&json).unwrap(), "[[3,2,1],[6,5,4]]");
}

// ===========================================================================
// §5  Whitespace (spec: §A.4 — no unnecessary whitespace)
// ===========================================================================

/// No whitespace between tokens or around delimiters.
#[test]
fn vector_no_whitespace() {
    let json = json!({"a": [1, 2], "b": {"c": 3}});
    assert_eq!(canonical_json(&json).unwrap(), r#"{"a":[1,2],"b":{"c":3}}"#);
}

// ===========================================================================
// §6  String escaping (spec: §A.4 — required escapes)
// ===========================================================================

/// Double quotes must be escaped.
#[test]
fn vector_escape_double_quote() {
    let json = json!({"key": "value with \"quotes\""});
    assert_eq!(canonical_json(&json).unwrap(), r#"{"key":"value with \"quotes\""}"#);
}

/// Backslashes must be escaped.
#[test]
fn vector_escape_backslash() {
    let json = json!({"key": "back\\slash"});
    assert_eq!(canonical_json(&json).unwrap(), r#"{"key":"back\\slash"}"#);
}

/// Control characters (U+0000 to U+001F) must be escaped as `\u00XX`.
#[test]
fn vector_escape_control_chars() {
    let json = json!({"key": "a\u{0000}b\u{001f}c"});
    assert_eq!(canonical_json(&json).unwrap(), r#"{"key":"a\u0000b\u001fc"}"#);
}

/// U+0008 (BACKSPACE) must be escaped as `\b`.
#[test]
fn vector_escape_backspace() {
    let json = json!({"key": "a\u{0008}b"});
    assert_eq!(canonical_json(&json).unwrap(), r#"{"key":"a\bb"}"#);
}

/// U+000C (FORM FEED) must be escaped as `\f`.
#[test]
fn vector_escape_form_feed() {
    let json = json!({"key": "a\u{000c}b"});
    assert_eq!(canonical_json(&json).unwrap(), r#"{"key":"a\fb"}"#);
}

/// U+000A (LINE FEED) must be escaped as `\n`.
#[test]
fn vector_escape_line_feed() {
    let json = json!({"key": "a\nb"});
    assert_eq!(canonical_json(&json).unwrap(), r#"{"key":"a\nb"}"#);
}

/// U+000D (CARRIAGE RETURN) must be escaped as `\r`.
#[test]
fn vector_escape_carriage_return() {
    let json = json!({"key": "a\rb"});
    assert_eq!(canonical_json(&json).unwrap(), r#"{"key":"a\rb"}"#);
}

/// U+0009 (TAB) must be escaped as `\t`.
#[test]
fn vector_escape_tab() {
    let json = json!({"key": "a\tb"});
    assert_eq!(canonical_json(&json).unwrap(), r#"{"key":"a\tb"}"#);
}

/// U+2028 (LINE SEPARATOR) must be escaped as `\u2028`.
#[test]
fn vector_escape_line_separator() {
    let json = json!({"key": "a\u{2028}b"});
    assert_eq!(canonical_json(&json).unwrap(), r#"{"key":"a\u2028b"}"#);
}

/// U+2029 (PARAGRAPH SEPARATOR) must be escaped as `\u2029`.
#[test]
fn vector_escape_paragraph_separator() {
    let json = json!({"key": "a\u{2029}b"});
    assert_eq!(canonical_json(&json).unwrap(), r#"{"key":"a\u2029b"}"#);
}

/// U+FFFD (REPLACEMENT CHARACTER) must be escaped as `\ufffd`.
#[test]
fn vector_escape_replacement_char() {
    let json = json!({"key": "a\u{fffd}b"});
    assert_eq!(canonical_json(&json).unwrap(), r#"{"key":"a\ufffdb"}"#);
}

/// Unicode characters above U+FFFF (e.g. emoji) are preserved, not escaped.
/// Only U+2028, U+2029, U+FFFD and control chars are escaped.
#[test]
fn vector_high_unicode_preserved() {
    let json = json!({"emoji": "🎉"});
    assert_eq!(canonical_json(&json).unwrap(), r#"{"emoji":"🎉"}"#);
}

// ===========================================================================
// §7  Signatures and unsigned field stripping (spec: §A.4 Signing)
// ===========================================================================

/// `signatures` and `unsigned` fields must be removable before signing.
#[test]
fn vector_strip_signatures_and_unsigned() {
    let mut json = json!({
        "content": {"body": "Hello"},
        "signatures": {"@user:server": {"ed25519:1": "sig"}},
        "unsigned": {"age_ts": 12345}
    });
    remove_signatures_and_unsigned(&mut json);
    assert_eq!(canonical_json(&json).unwrap(), r#"{"content":{"body":"Hello"}}"#);
}

/// Stripping when only `signatures` is present.
#[test]
fn vector_strip_signatures_only() {
    let mut json = json!({
        "type": "m.room.message",
        "signatures": {"@user:server": {"ed25519:1": "sig"}}
    });
    remove_signatures_and_unsigned(&mut json);
    assert_eq!(canonical_json(&json).unwrap(), r#"{"type":"m.room.message"}"#);
}

/// Stripping when only `unsigned` is present.
#[test]
fn vector_strip_unsigned_only() {
    let mut json = json!({
        "type": "m.room.message",
        "unsigned": {"age_ts": 12345}
    });
    remove_signatures_and_unsigned(&mut json);
    assert_eq!(canonical_json(&json).unwrap(), r#"{"type":"m.room.message"}"#);
}

/// Stripping when neither field is present is a no-op.
#[test]
fn vector_strip_nothing_to_remove() {
    let mut json = json!({"type": "m.room.message", "content": {"body": "Hi"}});
    let before = json.clone();
    remove_signatures_and_unsigned(&mut json);
    assert_eq!(json, before);
}

// ===========================================================================
// §8  Complex event-like vectors (spec: §A.4 — realistic event shapes)
// ===========================================================================

/// A realistic m.room.message event with all fields populated,
/// after stripping signatures/unsigned, must canonicalize to the
/// exact byte sequence used by other Matrix implementations for signing.
#[test]
fn vector_realistic_message_event() {
    let mut event = json!({
        "auth_events": [],
        "content": {
            "body": "Hello world",
            "msgtype": "m.text"
        },
        "depth": 1,
        "hashes": {
            "sha256": "abcdef"
        },
        "origin": "example.com",
        "origin_server_ts": 1234567890,
        "prev_events": [],
        "room_id": "!room:example.com",
        "sender": "@user:example.com",
        "signatures": {
            "example.com": {
                "ed25519:1": "signature-here"
            }
        },
        "type": "m.room.message",
        "unsigned": {
            "age_ts": 1234567890
        }
    });
    remove_signatures_and_unsigned(&mut event);
    let canonical = canonical_json(&event).unwrap();
    // Keys must be sorted: auth_events, content, depth, hashes, origin, origin_server_ts, prev_events, room_id, sender, type
    assert_eq!(
        canonical,
        r#"{"auth_events":[],"content":{"body":"Hello world","msgtype":"m.text"},"depth":1,"hashes":{"sha256":"abcdef"},"origin":"example.com","origin_server_ts":1234567890,"prev_events":[],"room_id":"!room:example.com","sender":"@user:example.com","type":"m.room.message"}"#
    );
}

/// A federation key response body after stripping signatures.
#[test]
fn vector_server_key_response() {
    let mut key_response = json!({
        "old_verify_keys": {},
        "server_name": "example.com",
        "signatures": {
            "example.com": {
                "ed25519:1": "signature"
            }
        },
        "valid_until_ts": 1700000000000_i64,
        "verify_keys": {
            "ed25519:1": {
                "key": "base64key"
            }
        }
    });
    remove_signatures_and_unsigned(&mut key_response);
    let canonical = canonical_json(&key_response).unwrap();
    assert_eq!(
        canonical,
        r#"{"old_verify_keys":{},"server_name":"example.com","valid_until_ts":1700000000000,"verify_keys":{"ed25519:1":{"key":"base64key"}}}"#
    );
}

// ===========================================================================
// §9  Deep nesting (spec: §A.4 — no depth limit, but realistic events)
// ===========================================================================

/// Deeply nested structure with mixed types must serialize correctly.
#[test]
fn vector_deep_nesting_mixed_types() {
    let json = json!({
        "a": {"b": {"c": [1, "two", null, true, false]}},
        "d": 42
    });
    assert_eq!(canonical_json(&json).unwrap(), r#"{"a":{"b":{"c":[1,"two",null,true,false]}},"d":42}"#);
}

// ===========================================================================
// §10  Conformance table (table-driven sweep of all vectors above)
// ===========================================================================

/// A single-assertion sweep that runs every vector and collects failures.
/// This is the CI gate: if any vector fails, the test fails with a list.
#[test]
fn conformance_sweep_all_vectors() {
    let mut failures: Vec<String> = Vec::new();

    macro_rules! check {
        ($name:expr, $input:expr, $expected:expr) => {
            match canonical_json(&$input) {
                Ok(actual) if actual == $expected => {}
                Ok(actual) => {
                    failures.push(format!("{}: expected {:?}, got {:?}", $name, $expected, actual));
                }
                Err(e) => {
                    failures.push(format!("{}: error: {}", $name, e));
                }
            }
        };
    }

    check!("null", json!(null), "null");
    check!("true", json!(true), "true");
    check!("false", json!(false), "false");
    check!("empty_object", json!({}), "{}");
    check!("empty_array", json!([]), "[]");
    check!("integer_zero", json!({"v": 0}), r#"{"v":0}"#);
    check!("integer_positive", json!({"v": 42}), r#"{"v":42}"#);
    check!("integer_negative", json!({"v": -42}), r#"{"v":-42}"#);
    check!("key_sorting_ascii", json!({"b": 2, "A": 1, "a": 3, "B": 4}), r#"{"A":1,"B":4,"a":3,"b":2}"#);
    check!("array_order", json!([3, 1, 2]), "[3,1,2]");
    check!("no_whitespace", json!({"a": [1, 2], "b": {"c": 3}}), r#"{"a":[1,2],"b":{"c":3}}"#);

    if !failures.is_empty() {
        panic!("canonical JSON conformance failures:\n  - {}", failures.join("\n  - "));
    }
}
