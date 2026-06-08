//! Default push-rule computation (pure logic, no HTTP dependencies).
//!
//! Extracted from `web/routes/push_rules.rs` so that the sync service can
//! call these functions without depending on the web layer.

use serde_json::{json, Value};

/// Returns the Matrix v1.11 default push-rule set, parameterised with this
/// user's localpart so `.m.rule.contains_user_name` matches their own MXID.
pub fn default_push_rules_for_user(user_id: &str, username: &str) -> Value {
    json!({
        "global": {
            "content": [
                {
                    "rule_id": ".m.rule.contains_user_name",
                    "default": true,
                    "enabled": true,
                    "pattern": username,
                    "actions": [
                        "notify",
                        {"set_tweak": "highlight"},
                        {"set_tweak": "sound", "value": "default"}
                    ]
                }
            ],
            "override": [
                {
                    "rule_id": ".m.rule.master",
                    "default": true,
                    "enabled": false,
                    "conditions": [],
                    "actions": []
                },
                {
                    "rule_id": ".m.rule.suppress_notices",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "event_match", "key": "content.msgtype", "pattern": "m.notice"}],
                    "actions": []
                },
                {
                    "rule_id": ".m.rule.invite_for_me",
                    "default": true,
                    "enabled": true,
                    "conditions": [
                        {"kind": "event_match", "key": "type", "pattern": "m.room.member"},
                        {"kind": "event_match", "key": "content.membership", "pattern": "invite"},
                        {"kind": "event_match", "key": "state_key", "pattern": user_id}
                    ],
                    "actions": [
                        "notify",
                        {"set_tweak": "sound", "value": "default"},
                        {"set_tweak": "highlight", "value": false}
                    ]
                },
                {
                    "rule_id": ".m.rule.member_event",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.room.member"}],
                    "actions": []
                },
                {
                    "rule_id": ".m.rule.is_user_mention",
                    "default": true,
                    "enabled": true,
                    "conditions": [
                        {"kind": "event_property_contains", "key": "content.m\\.mentions.user_ids", "value": user_id}
                    ],
                    "actions": [
                        "notify",
                        {"set_tweak": "highlight"},
                        {"set_tweak": "sound", "value": "default"}
                    ]
                },
                {
                    "rule_id": ".m.rule.contains_display_name",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "contains_display_name"}],
                    "actions": [
                        "notify",
                        {"set_tweak": "highlight"},
                        {"set_tweak": "sound", "value": "default"}
                    ]
                },
                {
                    "rule_id": ".m.rule.is_room_mention",
                    "default": true,
                    "enabled": true,
                    "conditions": [
                        {"kind": "event_property_is", "key": "content.m\\.mentions.room", "value": true},
                        {"kind": "sender_notification_permission", "key": "room"}
                    ],
                    "actions": ["notify", {"set_tweak": "highlight"}]
                },
                {
                    "rule_id": ".m.rule.roomnotif",
                    "default": true,
                    "enabled": true,
                    "conditions": [
                        {"kind": "event_match", "key": "content.body", "pattern": "@room"},
                        {"kind": "sender_notification_permission", "key": "room"}
                    ],
                    "actions": ["notify", {"set_tweak": "highlight"}]
                },
                {
                    "rule_id": ".m.rule.tombstone",
                    "default": true,
                    "enabled": true,
                    "conditions": [
                        {"kind": "event_match", "key": "type", "pattern": "m.room.tombstone"},
                        {"kind": "event_match", "key": "state_key", "pattern": ""}
                    ],
                    "actions": ["notify", {"set_tweak": "highlight"}]
                },
                {
                    "rule_id": ".m.rule.reaction",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.reaction"}],
                    "actions": []
                },
                {
                    "rule_id": ".m.rule.room.server_acl",
                    "default": true,
                    "enabled": true,
                    "conditions": [
                        {"kind": "event_match", "key": "type", "pattern": "m.room.server_acl"},
                        {"kind": "event_match", "key": "state_key", "pattern": ""}
                    ],
                    "actions": []
                },
                {
                    "rule_id": ".m.rule.suppress_edits",
                    "default": true,
                    "enabled": true,
                    "conditions": [
                        {"kind": "event_property_is", "key": "content.m\\.relates_to.rel_type", "value": "m.replace"}
                    ],
                    "actions": []
                }
            ],
            "room": [],
            "sender": [],
            "underride": [
                {
                    "rule_id": ".m.rule.call",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.call.invite"}],
                    "actions": [
                        "notify",
                        {"set_tweak": "sound", "value": "ring"},
                        {"set_tweak": "highlight", "value": false}
                    ]
                },
                {
                    "rule_id": ".m.rule.encrypted_room_one_to_one",
                    "default": true,
                    "enabled": true,
                    "conditions": [
                        {"kind": "room_member_count", "is": "2"},
                        {"kind": "event_match", "key": "type", "pattern": "m.room.encrypted"}
                    ],
                    "actions": [
                        "notify",
                        {"set_tweak": "sound", "value": "default"},
                        {"set_tweak": "highlight", "value": false}
                    ]
                },
                {
                    "rule_id": ".m.rule.room_one_to_one",
                    "default": true,
                    "enabled": true,
                    "conditions": [
                        {"kind": "room_member_count", "is": "2"},
                        {"kind": "event_match", "key": "type", "pattern": "m.room.message"}
                    ],
                    "actions": [
                        "notify",
                        {"set_tweak": "sound", "value": "default"},
                        {"set_tweak": "highlight", "value": false}
                    ]
                },
                {
                    "rule_id": ".m.rule.message",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.room.message"}],
                    "actions": ["notify", {"set_tweak": "highlight", "value": false}]
                },
                {
                    "rule_id": ".m.rule.encrypted",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.room.encrypted"}],
                    "actions": ["notify", {"set_tweak": "highlight", "value": false}]
                }
            ]
        }
    })
}

/// Merge any spec-default rules that the persisted user rule set is missing,
/// and ensure the resulting array is in matrix-js-sdk's expected canonical
/// order.
pub fn merge_default_push_rules(content: &mut Value, user_id: &str, username: &str) {
    let defaults = default_push_rules_for_user(user_id, username);
    let Some(default_global) = defaults.get("global").and_then(|g| g.as_object()) else {
        return;
    };
    let global = content.as_object_mut().and_then(|m| m.entry("global").or_insert_with(|| json!({})).as_object_mut());
    let Some(global) = global else { return };

    for kind in ["content", "override", "underride"] {
        let Some(canonical) = default_global.get(kind).and_then(|v| v.as_array()) else {
            continue;
        };

        let stored =
            global.entry(kind.to_string()).or_insert_with(|| json!([])).as_array().cloned().unwrap_or_default();

        let stored_by_id: std::collections::HashMap<String, Value> = stored
            .iter()
            .filter_map(|r| r.get("rule_id").and_then(|v| v.as_str()).map(|id| (id.to_string(), r.clone())))
            .collect();

        let mut merged: Vec<Value> = Vec::with_capacity(canonical.len() + stored.len());
        for rule in canonical {
            let rid = rule.get("rule_id").and_then(|v| v.as_str()).unwrap_or_default();
            if let Some(existing) = stored_by_id.get(rid) {
                let mut canonical_rule = rule.clone();
                if let Some(enabled) = existing.get("enabled") {
                    canonical_rule["enabled"] = enabled.clone();
                }
                if let Some(actions) = existing.get("actions") {
                    canonical_rule["actions"] = actions.clone();
                }
                merged.push(canonical_rule);
            } else {
                merged.push(rule.clone());
            }
        }

        for rule in stored {
            let is_default = rule.get("default").and_then(|v| v.as_bool()).unwrap_or(false);
            if !is_default {
                merged.push(rule);
            }
        }

        global.insert(kind.to_string(), Value::Array(merged));
    }

    for kind in ["room", "sender"] {
        global.entry(kind.to_string()).or_insert_with(|| json!([]));
    }
}

/// Back-compat for callers that don't have a user context.
pub fn get_default_push_rules() -> Value {
    default_push_rules_for_user("@user:localhost", "user")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rules_have_required_top_level_keys() {
        let rules = default_push_rules_for_user("@alice:example.com", "alice");
        let global = rules.get("global").unwrap();
        for k in ["content", "override", "room", "sender", "underride"] {
            assert!(global.get(k).is_some(), "missing {k}");
        }
    }

    #[test]
    fn default_rules_include_v1_11_required_ids() {
        let rules = default_push_rules_for_user("@alice:example.com", "alice");
        let override_rules = rules["global"]["override"].as_array().unwrap();
        let ids: Vec<&str> = override_rules.iter().filter_map(|r| r["rule_id"].as_str()).collect();
        for required in [
            ".m.rule.master",
            ".m.rule.suppress_notices",
            ".m.rule.invite_for_me",
            ".m.rule.member_event",
            ".m.rule.is_user_mention",
            ".m.rule.is_room_mention",
            ".m.rule.tombstone",
            ".m.rule.reaction",
            ".m.rule.room.server_acl",
            ".m.rule.suppress_edits",
        ] {
            assert!(ids.contains(&required), "missing override {required}");
        }
    }

    #[test]
    fn merge_adds_missing_rules_without_clobbering() {
        let mut content = json!({
            "global": {
                "override": [
                    {"rule_id": ".m.rule.master", "default": true, "enabled": true, "conditions": [], "actions": []}
                ]
            }
        });
        merge_default_push_rules(&mut content, "@alice:example.com", "alice");
        let overrides = content["global"]["override"].as_array().unwrap();
        let master = overrides.iter().find(|r| r["rule_id"] == ".m.rule.master").unwrap();
        assert_eq!(master["enabled"], true, "user-customised value preserved");
        let ids: Vec<&str> = overrides.iter().filter_map(|r| r["rule_id"].as_str()).collect();
        assert!(ids.contains(&".m.rule.suppress_edits"));
    }
}
