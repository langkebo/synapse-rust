//! The room **creator set** — who founded a room, per Matrix and MSC4289.
//!
//! A room's creators are:
//!
//! * `content.creator` (room versions 1–10; the field was the creator's
//!   declaration until v11 removed it);
//! * the `m.room.create` event's `sender` (the creator from v11 on, and equal to
//!   `content.creator` before that);
//! * every entry of `content.additional_creators` (MSC4289, room version 12+).
//!
//! All three are collected for every version. On a valid v1–v10 event the first
//! two coincide, and `additional_creators` is simply absent, so collecting the
//! union is correct everywhere and needs no version dispatch here.
//!
//! This module is the **single** implementation of that rule. The authorisation
//! service (`synapse-services`) and the inbound federation auth-rules seam
//! (`synapse-federation`) both call it, so "who is a creator" cannot drift
//! between the local and the federated view of the same room.

use std::collections::BTreeSet;

use serde_json::Value;

/// Extracts the creator set from an `m.room.create` event's `sender` and
/// `content`.
///
/// `sender` may be empty when the caller only has the content; empty strings are
/// ignored rather than inserted. Malformed `additional_creators` entries are
/// ignored here — validating them is the auth rule's job (MSC4289 rule 1.4,
/// [`crate::validation::is_well_formed_user_id`]); this function answers "who is
/// named as a creator", not "is the event well-formed".
pub fn creators_from_create_event(sender: &str, content: &Value) -> BTreeSet<String> {
    let mut creators = BTreeSet::new();

    if let Some(creator) = content.get("creator").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        creators.insert(creator.to_string());
    }
    if !sender.is_empty() {
        creators.insert(sender.to_string());
    }
    if let Some(extra) = content.get("additional_creators").and_then(Value::as_array) {
        for entry in extra {
            if let Some(user_id) = entry.as_str().filter(|s| !s.is_empty()) {
                creators.insert(user_id.to_string());
            }
        }
    }

    creators
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn collects_creator_sender_and_additional_creators() {
        let content = json!({
            "creator": "@alice:example.org",
            "additional_creators": ["@bob:example.org", "@carol:other.example"],
        });
        let creators = creators_from_create_event("@alice:example.org", &content);
        assert_eq!(creators.len(), 3);
        assert!(creators.contains("@alice:example.org"));
        assert!(creators.contains("@bob:example.org"));
        assert!(creators.contains("@carol:other.example"));
    }

    /// v11+ removed `content.creator`: the sender alone is the creator.
    #[test]
    fn sender_alone_is_a_creator_when_content_creator_is_absent() {
        let creators = creators_from_create_event("@alice:example.org", &json!({"room_version": "11"}));
        assert_eq!(creators, BTreeSet::from(["@alice:example.org".to_string()]));
    }

    #[test]
    fn ignores_empty_and_non_string_entries() {
        let content = json!({"creator": "", "additional_creators": ["@ok:example.org", "", 7, null]});
        let creators = creators_from_create_event("", &content);
        assert_eq!(creators, BTreeSet::from(["@ok:example.org".to_string()]));
    }

    #[test]
    fn a_missing_or_malformed_additional_creators_is_not_a_creator() {
        assert!(creators_from_create_event("@a:b", &json!({"additional_creators": "@b:c"})).len() == 1);
        assert!(creators_from_create_event("@a:b", &json!({})).len() == 1);
    }
}
