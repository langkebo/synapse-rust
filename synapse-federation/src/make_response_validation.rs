//! Validation of `make_join` / `make_leave` templates before signing them.
//!
//! Matrix-spec PR #2284 ("Specify basic validation for federation membership
//! endpoints") requires the **sending** server to verify the template returned
//! by `make_join` / `make_leave` / `make_knock` before it signs it:
//!
//! > Before signing the returned template and calling `/send_join`, the sending
//! > server MUST verify that: the `room_id` is equal to the `roomId` path
//! > parameter; both the `sender` and `state_key` are equal to the `userId` path
//! > parameter; the `type` of the event is `m.room.member`; the `membership`
//! > field inside `content` is `join`.  In case any of the above checks fail,
//! > the response MUST be treated as malformed and discarded.
//!
//! Upstream Synapse implements this as a single assertion on the membership
//! (element-hq/synapse #20189).  Without it a malicious resident server can
//! return a template whose `content.membership` is `ban`, and the joining
//! server will put **its own signature** on that membership event.
//!
//! ## Strict-when-present
//!
//! `room_id` / `sender` / `state_key` are checked only when the template
//! actually carries them.  Real deployments differ here: some resident servers
//! omit `room_id` (the joining server is expected to add it), and rejecting a
//! merely-absent field would break otherwise-valid joins — a worse outcome than
//! the attack this guards.  The fields that decide the event's effect (`type`
//! and `content.membership`) are always required.

use crate::client::FederationClientError;

/// Validate a `make_*` membership template against the request that produced it.
///
/// `expected_membership` is `"join"`, `"leave"` or `"knock"`.
pub fn validate_make_membership_template(
    event: &serde_json::Value,
    room_id: &str,
    user_id: &str,
    expected_membership: &str,
) -> Result<(), FederationClientError> {
    let Some(object) = event.as_object() else {
        return Err(FederationClientError::InvalidResponse(
            "make_* response `event` must be a JSON object".to_string(),
        ));
    };

    match object.get("type").and_then(|value| value.as_str()) {
        Some("m.room.member") => {}
        other => {
            return Err(FederationClientError::InvalidResponse(format!(
                "make_* response event type must be m.room.member, got {other:?}"
            )))
        }
    }

    for (field, expected) in [("room_id", room_id), ("sender", user_id), ("state_key", user_id)] {
        if let Some(actual) = object.get(field).and_then(|value| value.as_str()) {
            if actual != expected {
                return Err(FederationClientError::InvalidResponse(format!(
                    "make_* response `{field}` is {actual:?}, expected {expected:?}"
                )));
            }
        }
    }

    let membership =
        object.get("content").and_then(|content| content.get("membership")).and_then(|membership| membership.as_str());

    match membership {
        Some(value) if value == expected_membership => Ok(()),
        other => Err(FederationClientError::InvalidResponse(format!(
            "make_* response membership must be {expected_membership:?}, got {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use serde_json::json;

    fn template(event_type: &str, room_id: Option<&str>, sender: Option<&str>, membership: &str) -> serde_json::Value {
        let mut value = json!({
            "type": event_type,
            "content": { "membership": membership },
            "sender": sender,
            "state_key": sender,
        });
        if let Some(room_id) = room_id {
            value["room_id"] = json!(room_id);
        }
        if sender.is_none() {
            value.as_object_mut().unwrap().remove("sender");
            value.as_object_mut().unwrap().remove("state_key");
        }
        value
    }

    #[test]
    fn accepts_a_correct_join_template() {
        let value = template("m.room.member", Some("!r:remote"), Some("@a:local"), "join");
        assert!(validate_make_membership_template(&value, "!r:remote", "@a:local", "join").is_ok());
    }

    #[test]
    fn accepts_a_template_without_room_id_or_sender() {
        // Some resident servers omit these; the joining server fills them in.
        let value = json!({"type": "m.room.member", "content": {"membership": "join"}});
        assert!(validate_make_membership_template(&value, "!r:remote", "@a:local", "join").is_ok());
    }

    #[test]
    fn rejects_wrong_membership() {
        // The attack #20189 fixed: a template that would make us sign a ban.
        let value = template("m.room.member", Some("!r:remote"), Some("@a:local"), "ban");
        let err = validate_make_membership_template(&value, "!r:remote", "@a:local", "join").unwrap_err();
        assert!(err.to_string().contains("membership"), "{err}");
    }

    #[test]
    fn rejects_join_for_a_leave_request() {
        let value = template("m.room.member", Some("!r:remote"), Some("@a:local"), "join");
        assert!(validate_make_membership_template(&value, "!r:remote", "@a:local", "leave").is_err());
    }

    #[test]
    fn rejects_missing_membership() {
        let value = json!({"type": "m.room.member", "content": {}});
        assert!(validate_make_membership_template(&value, "!r:remote", "@a:local", "join").is_err());
    }

    #[test]
    fn rejects_wrong_type() {
        let value = json!({"type": "m.room.message", "content": {"membership": "join"}});
        let err = validate_make_membership_template(&value, "!r:remote", "@a:local", "join").unwrap_err();
        assert!(err.to_string().contains("m.room.member"), "{err}");
    }

    #[test]
    fn rejects_mismatched_room_id_sender_and_state_key() {
        let cases = [
            json!({"type": "m.room.member", "content": {"membership": "join"}, "room_id": "!other:remote"}),
            json!({"type": "m.room.member", "content": {"membership": "join"}, "sender": "@evil:remote"}),
            json!({"type": "m.room.member", "content": {"membership": "join"}, "state_key": "@evil:remote"}),
        ];
        for value in cases {
            assert!(
                validate_make_membership_template(&value, "!r:remote", "@a:local", "join").is_err(),
                "must reject {value}"
            );
        }
    }

    #[test]
    fn rejects_non_object_event() {
        for value in [json!("nope"), json!([1, 2]), json!(null)] {
            assert!(validate_make_membership_template(&value, "!r:remote", "@a:local", "join").is_err());
        }
    }
}
