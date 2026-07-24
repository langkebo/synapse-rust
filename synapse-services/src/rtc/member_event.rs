//! MSC4143 MatrixRTC — `m.rtc.member` event parsing and focus election.
//!
//! MSC4143 defines `m.rtc.member` as a state event type that clients send
//! to manage their membership in a MatrixRTC session. The `state_key` is
//! the `device_id`, and the `sender` is the `user_id`. The event content
//! carries the application, focus preferences, and expiration.
//!
//! This module provides pure-function helpers that complete the MSC4143
//! implementation on top of the existing `RtcSessionService` storage
//! layer:
//!
//! - [`parse_m_rtc_member_event`]: parse a state event content into a
//!   structured [`ParsedRtcMember`] that can be converted to
//!   [`CreateMembershipParams`] for storage.
//! - [`filter_active_memberships`]: filter out expired memberships at
//!   read time (defense-in-depth — the background cleanup task may lag).
//! - [`elect_focus_id`]: elect a focus identifier from a list of active
//!   memberships, used by SFU routing decisions.
//!
//! All functions are pure (no async, no DB) to enable fast unit testing
//! and to keep the MSC4143 logic decoupled from the storage layer.

use synapse_storage::matrixrtc::{CreateMembershipParams, RTCMembership};

/// Parsed representation of an `m.rtc.member` state event content.
///
/// Per MSC4143, this event represents a user's membership in a MatrixRTC
/// session. The `state_key` is the device_id, and the event `sender` is
/// the user_id. Required content fields are `application` and
/// `membership_id`; all other fields are optional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRtcMember {
    /// Device ID (from event `state_key`).
    pub device_id: String,
    /// User ID (from event `sender`).
    pub user_id: String,
    /// Application identifier (e.g., `m.call`, `m.branding.call`).
    pub application: String,
    /// Optional call ID for grouping related sessions.
    pub call_id: Option<String>,
    /// Optional application-specific data (e.g., SDP, streams).
    pub application_data: Option<serde_json::Value>,
    /// Focus IDs this member prefers to connect to.
    pub foci_preferred: Vec<String>,
    /// Focus IDs this member is actively serving as a focus for.
    pub foci_active: Vec<String>,
    /// Optional expiration timestamp (ms since epoch). When present, the
    /// membership is considered expired after this timestamp.
    pub expires_at: Option<i64>,
    /// Unique identifier for this membership instance. Required by
    /// MSC4143 for idempotent updates.
    pub membership_id: Option<String>,
}

impl ParsedRtcMember {
    /// Convert the parsed member into [`CreateMembershipParams`] for
    /// storage via `RtcSessionService::create_membership`.
    ///
    /// The `foci_preferred` and `foci_active` vectors are serialized as
    /// JSON arrays to fit the existing storage schema (which uses
    /// `Option<String>` / `Option<Value>` columns).
    pub fn to_create_membership_params(
        &self,
        room_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> CreateMembershipParams {
        let foci_active = if self.foci_active.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&self.foci_active).unwrap_or_else(|_| String::from("[]")))
        };
        let foci_preferred = if self.foci_preferred.is_empty() {
            None
        } else {
            Some(serde_json::Value::Array(
                self.foci_preferred.iter().map(|s| serde_json::Value::String(s.clone())).collect(),
            ))
        };

        CreateMembershipParams {
            room_id: room_id.into(),
            session_id: session_id.into(),
            user_id: self.user_id.clone(),
            device_id: self.device_id.clone(),
            membership_id: self.membership_id.clone().unwrap_or_else(|| {
                // Fallback: derive a deterministic ID from user+device.
                format!("{}-{}", self.user_id, self.device_id)
            }),
            application: self.application.clone(),
            call_id: self.call_id.clone(),
            foci_active,
            foci_preferred,
            application_data: self.application_data.clone(),
        }
    }
}

/// Parse an `m.rtc.member` state event into a structured representation.
///
/// # Arguments
///
/// - `content`: The event `content` JSON object.
/// - `sender`: The event `sender` (user_id).
/// - `state_key`: The event `state_key` (device_id).
///
/// # Fail-closed behaviour
///
/// Returns `None` when any required field is missing or empty:
/// - `state_key` (device_id) must be non-empty
/// - `sender` (user_id) must be non-empty
/// - `content.application` must be a non-empty string
///
/// Callers MUST treat `None` as "reject the event" — do not fall back
/// to defaults. This prevents malformed events from creating spurious
/// memberships.
pub fn parse_m_rtc_member_event(content: &serde_json::Value, sender: &str, state_key: &str) -> Option<ParsedRtcMember> {
    // state_key = device_id (required, non-empty)
    if state_key.is_empty() {
        return None;
    }
    // sender = user_id (required, non-empty)
    if sender.is_empty() {
        return None;
    }
    // application (required, non-empty string)
    let application = content.get("application").and_then(|v| v.as_str())?;
    if application.is_empty() {
        return None;
    }

    let call_id = content.get("call_id").and_then(|v| v.as_str()).map(|s| s.to_string());
    let application_data = content.get("application_data").cloned();
    let foci_preferred = parse_string_array(content, "foci_preferred");
    let foci_active = parse_string_array(content, "foci_active");
    let expires_at = content.get("expires_at").and_then(|v| v.as_i64());
    let membership_id = content.get("membership_id").and_then(|v| v.as_str()).map(|s| s.to_string());

    Some(ParsedRtcMember {
        device_id: state_key.to_string(),
        user_id: sender.to_string(),
        application: application.to_string(),
        call_id,
        application_data,
        foci_preferred,
        foci_active,
        expires_at,
        membership_id,
    })
}

/// Helper: parse a JSON field as an array of strings.
///
/// Returns an empty vec when the field is missing, null, or not an
/// array. Non-string entries are silently skipped (lenient parsing).
fn parse_string_array(content: &serde_json::Value, field: &str) -> Vec<String> {
    content
        .get(field)
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default()
}

/// Filter out expired memberships from a list.
///
/// A membership is considered active when:
/// - `is_active` is `true`, AND
/// - `expires_at` is `None` (never expires) OR `expires_at > now`
///
/// This is a defense-in-depth filter: the background cleanup task
/// (`cleanup_expired_memberships`) may lag, so reads should also filter
/// to avoid returning stale entries.
pub fn filter_active_memberships(memberships: &[RTCMembership], now: i64) -> Vec<&RTCMembership> {
    memberships
        .iter()
        .filter(|m| {
            if !m.is_active {
                return false;
            }
            match m.expires_at {
                None => true,
                Some(exp) => exp > now,
            }
        })
        .collect()
}

/// Elect a focus identifier from a list of memberships.
///
/// Per MSC4143, a member signals focus availability via `foci_active`
/// (the focus IDs it is serving) and `foci_preferred` (the focus IDs it
/// wants to connect to). The election strategy is:
///
/// 1. Collect all `foci_active` IDs across all memberships (these are
///    members willing to act as focus).
/// 2. Find the first `foci_active` ID that also appears in at least one
///    membership's `foci_preferred` (consensus focus).
/// 3. If no consensus, return the first `foci_active` ID.
/// 4. If no member has `foci_active`, return `None` (no focus available
///    — caller should fall back to client-side SFU selection).
///
/// # Arguments
///
/// - `memberships`: the list of memberships to consider. Callers should
///   pre-filter with [`filter_active_memberships`] to exclude expired
///   entries.
///
/// # Fail-closed behaviour
///
/// Returns `None` when no member is willing to act as focus. Callers
/// MUST treat `None` as "no SFU routing available" rather than guessing
/// a focus ID.
pub fn elect_focus_id(memberships: &[RTCMembership]) -> Option<String> {
    // Parse foci_active and foci_preferred from each membership.
    // The storage schema stores these as String / Value, so we need to
    // deserialize them back into vectors.
    let mut all_active: Vec<String> = Vec::new();
    let mut all_preferred: Vec<String> = Vec::new();

    for m in memberships {
        if let Some(ref active) = m.foci_active {
            // foci_active is stored as a JSON-encoded array string.
            if let Ok(arr) = serde_json::from_str::<Vec<String>>(active) {
                all_active.extend(arr);
            } else if !active.is_empty() {
                // Fallback: treat as a single focus ID.
                all_active.push(active.clone());
            }
        }
        if let Some(ref preferred) = m.foci_preferred {
            if let Some(arr) = preferred.as_array() {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        all_preferred.push(s.to_string());
                    }
                }
            }
        }
    }

    if all_active.is_empty() {
        return None;
    }

    // Step 2: consensus — first active ID that is also preferred.
    for active_id in &all_active {
        if all_preferred.iter().any(|p| p == active_id) {
            return Some(active_id.clone());
        }
    }

    // Step 3: no consensus — return the first active ID.
    all_active.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use synapse_storage::matrixrtc::RTCMembership;

    // -----------------------------------------------------------------------
    // parse_m_rtc_member_event
    // -----------------------------------------------------------------------

    fn make_valid_content() -> serde_json::Value {
        json!({
            "application": "m.call",
            "call_id": "call123",
            "application_data": {"sdp": "v=0"},
            "foci_preferred": ["livekit:room1"],
            "foci_active": ["livekit:room1"],
            "expires_at": 2000000,
            "membership_id": "mem-abc-123"
        })
    }

    #[test]
    fn test_parse_valid_m_rtc_member_event() {
        let content = make_valid_content();
        let parsed = parse_m_rtc_member_event(&content, "@alice:ex.com", "DEVICE1").expect("valid event should parse");

        assert_eq!(parsed.device_id, "DEVICE1");
        assert_eq!(parsed.user_id, "@alice:ex.com");
        assert_eq!(parsed.application, "m.call");
        assert_eq!(parsed.call_id.as_deref(), Some("call123"));
        assert_eq!(parsed.application_data, Some(json!({"sdp": "v=0"})));
        assert_eq!(parsed.foci_preferred, vec!["livekit:room1"]);
        assert_eq!(parsed.foci_active, vec!["livekit:room1"]);
        assert_eq!(parsed.expires_at, Some(2000000));
        assert_eq!(parsed.membership_id.as_deref(), Some("mem-abc-123"));
    }

    #[test]
    fn test_parse_returns_none_for_empty_state_key() {
        let content = make_valid_content();
        assert!(parse_m_rtc_member_event(&content, "@alice:ex.com", "").is_none());
    }

    #[test]
    fn test_parse_returns_none_for_empty_sender() {
        let content = make_valid_content();
        assert!(parse_m_rtc_member_event(&content, "", "DEVICE1").is_none());
    }

    #[test]
    fn test_parse_returns_none_for_missing_application() {
        let content = json!({"call_id": "call123"});
        assert!(parse_m_rtc_member_event(&content, "@alice:ex.com", "DEVICE1").is_none());
    }

    #[test]
    fn test_parse_returns_none_for_empty_application() {
        let content = json!({"application": ""});
        assert!(parse_m_rtc_member_event(&content, "@alice:ex.com", "DEVICE1").is_none());
    }

    #[test]
    fn test_parse_returns_none_for_non_string_application() {
        // Fail-closed: numeric application should not be coerced.
        let content = json!({"application": 123});
        assert!(parse_m_rtc_member_event(&content, "@alice:ex.com", "DEVICE1").is_none());
    }

    #[test]
    fn test_parse_handles_missing_optional_fields() {
        let content = json!({"application": "m.call"});
        let parsed = parse_m_rtc_member_event(&content, "@alice:ex.com", "DEVICE1").expect("minimal valid event");

        assert_eq!(parsed.application, "m.call");
        assert!(parsed.call_id.is_none());
        assert!(parsed.application_data.is_none());
        assert!(parsed.foci_preferred.is_empty());
        assert!(parsed.foci_active.is_empty());
        assert!(parsed.expires_at.is_none());
        assert!(parsed.membership_id.is_none());
    }

    #[test]
    fn test_parse_skips_non_string_entries_in_foci_arrays() {
        // Lenient parsing: non-string entries are silently skipped.
        let content = json!({
            "application": "m.call",
            "foci_preferred": ["valid", 123, null, "also-valid"],
            "foci_active": [true, "focus1"]
        });
        let parsed = parse_m_rtc_member_event(&content, "@alice:ex.com", "DEV1").unwrap();
        assert_eq!(parsed.foci_preferred, vec!["valid", "also-valid"]);
        assert_eq!(parsed.foci_active, vec!["focus1"]);
    }

    // -----------------------------------------------------------------------
    // ParsedRtcMember::to_create_membership_params
    // -----------------------------------------------------------------------

    #[test]
    fn test_to_create_membership_params_serializes_foci_arrays() {
        let parsed = ParsedRtcMember {
            device_id: "DEV1".into(),
            user_id: "@alice:ex.com".into(),
            application: "m.call".into(),
            call_id: Some("call1".into()),
            application_data: Some(json!({"streams": []})),
            foci_preferred: vec!["focusA".into(), "focusB".into()],
            foci_active: vec!["focusA".into()],
            expires_at: Some(9999),
            membership_id: Some("mem-1".into()),
        };

        let params = parsed.to_create_membership_params("!room:ex.com", "sess1");

        assert_eq!(params.room_id, "!room:ex.com");
        assert_eq!(params.session_id, "sess1");
        assert_eq!(params.user_id, "@alice:ex.com");
        assert_eq!(params.device_id, "DEV1");
        assert_eq!(params.membership_id, "mem-1");
        assert_eq!(params.application, "m.call");
        assert_eq!(params.call_id.as_deref(), Some("call1"));
        // foci_active is JSON-encoded string
        assert_eq!(params.foci_active.as_deref(), Some("[\"focusA\"]"));
        // foci_preferred is JSON array value
        assert_eq!(params.foci_preferred, Some(json!(["focusA", "focusB"])));
    }

    #[test]
    fn test_to_create_membership_params_uses_fallback_membership_id() {
        // When membership_id is missing, derive a deterministic fallback.
        let parsed = ParsedRtcMember {
            device_id: "DEV1".into(),
            user_id: "@alice:ex.com".into(),
            application: "m.call".into(),
            call_id: None,
            application_data: None,
            foci_preferred: vec![],
            foci_active: vec![],
            expires_at: None,
            membership_id: None,
        };

        let params = parsed.to_create_membership_params("!room:ex.com", "sess1");
        assert_eq!(params.membership_id, "@alice:ex.com-DEV1");
        assert!(params.foci_active.is_none());
        assert!(params.foci_preferred.is_none());
    }

    // -----------------------------------------------------------------------
    // filter_active_memberships
    // -----------------------------------------------------------------------

    fn make_membership(user_id: &str, device_id: &str, is_active: bool, expires_at: Option<i64>) -> RTCMembership {
        RTCMembership {
            id: 1,
            room_id: "!room:ex.com".into(),
            session_id: "sess1".into(),
            user_id: user_id.into(),
            device_id: device_id.into(),
            membership_id: format!("mem-{}", user_id),
            application: "m.call".into(),
            call_id: Some("call1".into()),
            created_ts: 1000,
            updated_ts: 2000,
            expires_at,
            foci_active: None,
            foci_preferred: None,
            application_data: None,
            is_active,
        }
    }

    #[test]
    fn test_filter_returns_only_active_unexpired_memberships() {
        let now = 10000;
        let memberships = vec![
            make_membership("@alice:ex.com", "D1", true, Some(20000)), // active, not expired
            make_membership("@bob:ex.com", "D2", true, Some(5000)),    // active but expired
            make_membership("@carol:ex.com", "D3", false, Some(20000)), // inactive
        ];

        let active = filter_active_memberships(&memberships, now);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].user_id, "@alice:ex.com");
    }

    #[test]
    fn test_filter_keeps_memberships_without_expiration() {
        let now = 10000;
        let memberships = vec![
            make_membership("@alice:ex.com", "D1", true, None), // never expires
        ];

        let active = filter_active_memberships(&memberships, now);
        assert_eq!(active.len(), 1);
    }

    #[test]
    fn test_filter_boundary_expires_at_equal_to_now_is_expired() {
        // A membership with expires_at == now is considered expired
        // (strict greater-than comparison). This matches MSC4143's
        // "expires after this timestamp" semantics.
        let now = 10000;
        let memberships = vec![make_membership("@alice:ex.com", "D1", true, Some(10000))];

        let active = filter_active_memberships(&memberships, now);
        assert!(active.is_empty(), "expires_at == now should be expired");
    }

    #[test]
    fn test_filter_returns_empty_for_empty_input() {
        let active = filter_active_memberships(&[], 10000);
        assert!(active.is_empty());
    }

    // -----------------------------------------------------------------------
    // elect_focus_id
    // -----------------------------------------------------------------------

    fn make_focus_membership(
        user_id: &str,
        foci_active: Option<&str>,
        foci_preferred: Option<serde_json::Value>,
    ) -> RTCMembership {
        RTCMembership {
            id: 1,
            room_id: "!room:ex.com".into(),
            session_id: "sess1".into(),
            user_id: user_id.into(),
            device_id: "DEV1".into(),
            membership_id: format!("mem-{}", user_id),
            application: "m.call".into(),
            call_id: None,
            created_ts: 1000,
            updated_ts: 2000,
            expires_at: None,
            foci_active: foci_active.map(|s| s.to_string()),
            foci_preferred,
            application_data: None,
            is_active: true,
        }
    }

    #[test]
    fn test_elect_focus_returns_consensus_when_preferred_matches_active() {
        // Member A is focus (foci_active = "focus1")
        // Member B prefers "focus1"
        // Expected: "focus1" (consensus)
        let memberships = vec![
            make_focus_membership("@alice:ex.com", Some(r#"["focus1"]"#), None),
            make_focus_membership("@bob:ex.com", None, Some(json!(["focus1"]))),
        ];

        let focus = elect_focus_id(&memberships);
        assert_eq!(focus.as_deref(), Some("focus1"));
    }

    #[test]
    fn test_elect_focus_returns_first_active_when_no_consensus() {
        // No member prefers any active focus → fall back to first active.
        let memberships = vec![
            make_focus_membership("@alice:ex.com", Some(r#"["focusA"]"#), Some(json!(["focusB"]))),
            make_focus_membership("@bob:ex.com", Some(r#"["focusC"]"#), None),
        ];

        let focus = elect_focus_id(&memberships);
        assert_eq!(focus.as_deref(), Some("focusA"));
    }

    #[test]
    fn test_elect_focus_returns_none_when_no_active_foci() {
        // No member is willing to act as focus.
        let memberships = vec![
            make_focus_membership("@alice:ex.com", None, Some(json!(["focus1"]))),
            make_focus_membership("@bob:ex.com", None, None),
        ];

        let focus = elect_focus_id(&memberships);
        assert!(focus.is_none(), "no foci_active should return None");
    }

    #[test]
    fn test_elect_focus_returns_none_for_empty_input() {
        let focus = elect_focus_id(&[]);
        assert!(focus.is_none());
    }

    #[test]
    fn test_elect_focus_handles_legacy_string_foci_active() {
        // Legacy storage may store foci_active as a plain string rather
        // than a JSON array. The election should treat it as a single
        // focus ID.
        let memberships = vec![make_focus_membership("@alice:ex.com", Some("legacy-focus"), None)];

        let focus = elect_focus_id(&memberships);
        assert_eq!(focus.as_deref(), Some("legacy-focus"));
    }

    #[test]
    fn test_elect_focus_handles_malformed_foci_active_gracefully() {
        // Malformed JSON in foci_active should not crash — the entry is
        // skipped, and if no valid active focus remains, return None.
        let memberships = vec![
            make_focus_membership("@alice:ex.com", Some("not-json"), None),
            make_focus_membership("@bob:ex.com", Some(r#"["valid-focus"]"#), None),
        ];

        // The malformed "not-json" is treated as a single focus ID
        // (fallback path), so it becomes the first active entry.
        let focus = elect_focus_id(&memberships);
        assert_eq!(focus.as_deref(), Some("not-json"));
    }
}
