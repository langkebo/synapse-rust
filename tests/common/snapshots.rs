#![allow(clippy::expect_used)]
//! insta snapshot test helpers for the synapse-rust project.
//!
//! Provides reusable redaction filters for Matrix API responses, so
//! snapshot tests lock output format without false diffs from dynamic
//! fields (tokens, timestamps, UUIDs, event IDs).
//!
//! # Usage
//!
//! ```no_run
//! use crate::common::snapshots;
//!
//! let response = serde_json::json!({"access_token": "abc123", "user_id": "@alice:example.com"});
//! snapshots::assert_json_snapshot("login_response", &response);
//! ```
//!
//! # Snapshot review
//!
//! ```bash
//! cargo test --all-features          # new/changed snapshots will fail
//! cargo insta review                 # review and accept interactively
//! ```

/// Redact common dynamic fields from a JSON value before snapshotting.
///
/// Applies these redactions:
/// - `access_token`, `refresh_token`, `token` → `[REDACTED]`
/// - `origin_server_ts`, `age`, `valid_until_ts` → `[TIMESTAMP]`
/// - UUIDs (RFC 4122 hex pattern) → `[UUID]`
/// - `event_id` values (prefixed with `$`) → `[EVENT_ID]`
/// - `room_id` values (prefixed with `!`) → `[ROOM_ID]`
pub fn redact_matrix_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            // Redact known sensitive / dynamic keys by name
            let token_keys: &[&str] = &["access_token", "refresh_token", "token", "device_id"];
            let ts_keys: &[&str] = &["origin_server_ts", "age", "valid_until_ts", "server_ts", "ts"];

            for key in token_keys {
                if map.contains_key(*key) {
                    map.insert(key.to_string(), serde_json::Value::String("[REDACTED]".into()));
                }
            }
            for key in ts_keys {
                if map.contains_key(*key) {
                    map.insert(key.to_string(), serde_json::Value::String("[TIMESTAMP]".into()));
                }
            }

            // Redact specific value patterns
            for (k, v) in map.iter_mut() {
                match k.as_str() {
                    "event_id" | "redacts" => {
                        if v.as_str().is_some_and(|s| s.starts_with('$')) {
                            *v = serde_json::Value::String("[EVENT_ID]".into());
                        }
                    }
                    "room_id" => {
                        if v.as_str().is_some_and(|s| s.starts_with('!')) {
                            *v = serde_json::Value::String("[ROOM_ID]".into());
                        }
                    }
                    _ => {}
                }
                redact_matrix_json(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                redact_matrix_json(item);
            }
        }
        _ => {}
    }
}

/// Redact UUIDs (RFC 4122 hex pattern) from string values.
///
/// Matches patterns like `550e8400-e29b-41d4-a716-446655440000`.
fn redact_uuids(value: &mut serde_json::Value) {
    let uuid_re = regex::Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
        .expect("UUID regex pattern");

    match value {
        serde_json::Value::String(s) => {
            if uuid_re.is_match(s) {
                *s = "[UUID]".into();
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                redact_uuids(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                redact_uuids(item);
            }
        }
        _ => {}
    }
}

/// Assert a JSON snapshot with Matrix-specific redactions applied.
///
/// Use this for API response bodies, Matrix events, and error responses.
/// It applies [`redact_matrix_json`] and [`redact_uuids`] before snapshotting.
///
/// # Panics
///
/// Panics if the value does not match the stored snapshot.
#[track_caller]
pub fn assert_json_snapshot(name: &str, value: &serde_json::Value) {
    let mut cleaned = value.clone();
    redact_matrix_json(&mut cleaned);
    redact_uuids(&mut cleaned);

    // Use a deterministic serialization for reproducible snapshots
    let formatted = serde_json::to_string_pretty(&cleaned).expect("failed to serialize snapshot value");

    insta::with_settings!({
        snapshot_path => "../snapshots",
        prepend_module_to_snapshot => false,
    }, {
        insta::assert_snapshot!(name, formatted);
    });
}

/// Assert a JSON snapshot using custom redaction rules for a specific test.
///
/// Use this when you need additional redactions beyond the defaults in
/// [`assert_json_snapshot`]. The `extra_redact` closure is applied after
/// the standard redactions.
///
/// # Example
///
/// ```no_run
/// use crate::common::snapshots;
///
/// let response = serde_json::json!({"custom_id": "abc-123"});
/// snapshots::assert_json_snapshot_with("custom_response", &response, |v| {
///     if let Some(obj) = v.as_object_mut() {
///         obj.insert("custom_id".into(), serde_json::Value::String("[CUSTOM_ID]".into()));
///     }
/// });
/// ```
#[track_caller]
pub fn assert_json_snapshot_with(
    name: &str,
    value: &serde_json::Value,
    extra_redact: impl FnOnce(&mut serde_json::Value),
) {
    let mut cleaned = value.clone();
    redact_matrix_json(&mut cleaned);
    redact_uuids(&mut cleaned);
    extra_redact(&mut cleaned);

    let formatted = serde_json::to_string_pretty(&cleaned).expect("failed to serialize snapshot value");

    insta::with_settings!({
        snapshot_path => "../snapshots",
        prepend_module_to_snapshot => false,
    }, {
        insta::assert_snapshot!(name, formatted);
    });
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_tokens_in_json() {
        let mut value = serde_json::json!({
            "access_token": "secret_token_abc123",
            "refresh_token": "refresh_secret",
            "user_id": "@alice:example.com",
            "device_id": "DEVICE001",
        });
        redact_matrix_json(&mut value);
        assert_eq!(value["access_token"], "[REDACTED]");
        assert_eq!(value["refresh_token"], "[REDACTED]");
        assert_eq!(value["device_id"], "[REDACTED]");
        assert_eq!(value["user_id"], "@alice:example.com");
    }

    #[test]
    fn redact_timestamps_in_json() {
        let mut value = serde_json::json!({
            "origin_server_ts": 1700000000000_i64,
            "age": 3600_i64,
            "content": {"body": "hello"}
        });
        redact_matrix_json(&mut value);
        assert_eq!(value["origin_server_ts"], "[TIMESTAMP]");
        assert_eq!(value["age"], "[TIMESTAMP]");
        assert_eq!(value["content"]["body"], "hello");
    }

    #[test]
    fn redact_event_and_room_ids() {
        let mut value = serde_json::json!({
            "event_id": "$abc123def456:example.com",
            "room_id": "!xyz789:example.com",
            "content": {"body": "hello"}
        });
        redact_matrix_json(&mut value);
        assert_eq!(value["event_id"], "[EVENT_ID]");
        assert_eq!(value["room_id"], "[ROOM_ID]");
    }

    #[test]
    fn redact_uuids_from_strings() {
        let mut value = serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "alice"
        });
        redact_uuids(&mut value);
        assert_eq!(value["id"], "[UUID]");
        assert_eq!(value["name"], "alice");
    }

    #[test]
    fn nested_redaction_works_recursively() {
        let mut value = serde_json::json!({
            "rooms": {
                "join": {
                    "!room:example.com": {
                        "timeline": {
                            "events": [
                                {
                                    "event_id": "$event1:example.com",
                                    "origin_server_ts": 1700000000000_i64,
                                    "sender": "@alice:example.com"
                                }
                            ]
                        }
                    }
                }
            }
        });
        redact_matrix_json(&mut value);

        let event = &value["rooms"]["join"]["!room:example.com"]["timeline"]["events"][0];
        assert_eq!(event["event_id"], "[EVENT_ID]");
        assert_eq!(event["origin_server_ts"], "[TIMESTAMP]");
        assert_eq!(event["sender"], "@alice:example.com");
    }
}
