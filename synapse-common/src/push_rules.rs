//! Default push-rule computation (pure logic, no HTTP or storage deps).
//!
//! This module is the single source of truth for the default push-rule set and
//! the merge logic that reconciles a persisted user's rule set with the spec
//! defaults. Both the HTTP layer (`web/routes/push_rules`) and the sync service
//! (`synapse-services/src/sync_service/push_rules`) consume these functions; do
//! not duplicate the JSON definitions elsewhere.
//!
//! Since Matrix v1.17 the default set is the v1.11 set **minus** the three
//! legacy mention rules that MSC4210 removed; that removal is gated by
//! `experimental.msc4210_enabled` (default `true`) so an operator can opt back
//! in for clients that still depend on plaintext mentions.
//!
//! Extracted from the web layer's HTTP handler module so that the sync service
//! can compute default rules without depending on the web layer.

use serde_json::{json, Value};

/// Returns the default push-rule set, parameterised with this user's localpart
/// so `.m.rule.contains_user_name` matches their own MXID.
///
/// `msc4210_enabled` (the shipped default) hides the three **legacy** mention
/// rules that Matrix v1.17 removed — see [`MSC4210_LEGACY_MENTION_RULE_IDS`].
/// The intentional-mention rules are unaffected.
pub fn default_push_rules_for_user(user_id: &str, username: &str, msc4210_enabled: bool) -> Value {
    let mut rules = json!({
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
                    "rule_id": ".org.matrix.msc3786.rule.room.server_acl",
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
                    "rule_id": ".org.matrix.msc3914.rule.room.call",
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
    });

    if msc4210_enabled {
        remove_msc4210_legacy_rules(&mut rules);
    }

    rules
}

/// The three **legacy** mention push rules that Matrix v1.17 (MSC4210) removed
/// from the specification.
///
/// They are *hidden*, not deleted from storage: a user who had customised one
/// of them keeps the row, it simply stops being served. `.m.rule.is_user_mention`
/// and `.m.rule.is_room_mention` are **intentional**-mention rules and MUST keep
/// being served — conflating the two would silently break `m.mentions`.
pub const MSC4210_LEGACY_MENTION_RULE_IDS: &[&str] =
    &[".m.rule.contains_display_name", ".m.rule.contains_user_name", ".m.rule.roomnotif"];

/// Drop the [`MSC4210_LEGACY_MENTION_RULE_IDS`] from every rule kind of a
/// `/pushrules`-shaped document.
///
/// `roomnotif` lives in `override` and `contains_user_name` in `content`, so all
/// three kinds are scanned. Tolerates a missing or malformed `global` object so
/// it is safe to call on a document that came straight from the database.
pub fn remove_msc4210_legacy_rules(rules: &mut Value) {
    let Some(global) = rules.get_mut("global").and_then(|g| g.as_object_mut()) else {
        return;
    };
    for kind in ["content", "override", "underride"] {
        let Some(list) = global.get_mut(kind).and_then(|v| v.as_array_mut()) else {
            continue;
        };
        list.retain(|rule| {
            rule.get("rule_id")
                .and_then(Value::as_str)
                .is_none_or(|rule_id| !MSC4210_LEGACY_MENTION_RULE_IDS.contains(&rule_id))
        });
    }
}

/// Merge any spec-default rules that the persisted user rule set is missing,
/// and ensure the resulting array is in matrix-js-sdk's expected canonical
/// order (otherwise its `mergeRulesWithDefaults` will keep warning about
/// "missing" rules whenever the order differs from `EXPECTED_DEFAULT_*_RULE_IDS`).
///
/// User-customised rules (those without `default: true`) are appended after the
/// default rules. For default rules already present in the stored set, the
/// stored `enabled` / `actions` values are preserved so the user's tweaks
/// survive the merge.
///
/// A `default: true` row whose id is no longer in the canonical set (i.e. the
/// MSC4210 legacy mention rules when `msc4210_enabled`) is therefore dropped
/// from the merged output — the rule stops being served without the stored row
/// being deleted.
pub fn merge_default_push_rules(content: &mut Value, user_id: &str, username: &str, msc4210_enabled: bool) {
    let defaults = default_push_rules_for_user(user_id, username, msc4210_enabled);
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

        let mut stored_by_id: std::collections::HashMap<String, Value> = stored
            .iter()
            .filter_map(|r| r.get("rule_id").and_then(|v| v.as_str()).map(|id| (id.to_string(), r.clone())))
            .collect();

        // Backward compat: map old rule IDs to their MSC-prefixed replacements
        // so user customisations on the old ID survive the rename.
        if let Some(old_call) = stored_by_id.get(".m.rule.call").cloned() {
            stored_by_id.entry(".org.matrix.msc3914.rule.room.call".to_string()).or_insert(old_call);
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rules_have_required_top_level_keys() {
        let rules = default_push_rules_for_user("@alice:example.com", "alice", true);
        let global = rules.get("global").unwrap();
        for k in ["content", "override", "room", "sender", "underride"] {
            assert!(global.get(k).is_some(), "missing {k}");
        }
    }

    #[test]
    fn default_rules_include_required_override_ids() {
        let rules = default_push_rules_for_user("@alice:example.com", "alice", true);
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
            ".org.matrix.msc3786.rule.room.server_acl",
            ".m.rule.suppress_edits",
        ] {
            assert!(ids.contains(&required), "missing override {required}");
        }
    }

    #[test]
    fn default_rules_include_msc_prefixed_underride_ids() {
        let rules = default_push_rules_for_user("@alice:example.com", "alice", true);
        let underride_rules = rules["global"]["underride"].as_array().unwrap();
        let ids: Vec<&str> = underride_rules.iter().filter_map(|r| r["rule_id"].as_str()).collect();
        assert!(ids.contains(&".org.matrix.msc3914.rule.room.call"), "missing MSC3914 call rule in underride");
        // Old rule ID must not be emitted.
        assert!(!ids.contains(&".m.rule.call"), "old .m.rule.call should be replaced by MSC3914 ID");
    }

    #[test]
    fn merge_preserves_old_call_rule_customisations() {
        let mut content = json!({
            "global": {
                "underride": [
                    {
                        "rule_id": ".m.rule.call",
                        "default": true,
                        "enabled": false,
                        "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.call.invite"}],
                        "actions": []
                    }
                ]
            }
        });
        merge_default_push_rules(&mut content, "@alice:example.com", "alice", true);
        let underrides = content["global"]["underride"].as_array().unwrap();
        let call_rule = underrides.iter().find(|r| r["rule_id"] == ".org.matrix.msc3914.rule.room.call").unwrap();
        // User's enabled=false should survive the ID migration.
        assert_eq!(call_rule["enabled"], false);
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
        merge_default_push_rules(&mut content, "@alice:example.com", "alice", true);
        let overrides = content["global"]["override"].as_array().unwrap();
        let master = overrides.iter().find(|r| r["rule_id"] == ".m.rule.master").unwrap();
        assert_eq!(master["enabled"], true, "user-customised value preserved");
        let ids: Vec<&str> = overrides.iter().filter_map(|r| r["rule_id"].as_str()).collect();
        assert!(ids.contains(&".m.rule.suppress_edits"));
    }

    // -----------------------------------------------------------------------
    // MSC4210 —— 旧版提及规则的移除（Matrix v1.17）
    // -----------------------------------------------------------------------

    /// Collect every `rule_id` in a `/pushrules`-shaped document, across all
    /// three global kinds.
    fn all_rule_ids(rules: &Value) -> Vec<String> {
        let mut ids = Vec::new();
        for kind in ["content", "override", "underride"] {
            if let Some(list) = rules["global"][kind].as_array() {
                ids.extend(list.iter().filter_map(|rule| rule["rule_id"].as_str().map(str::to_string)));
            }
        }
        ids
    }

    /// MSC4210 原文只移除这三条 —— 把 `is_user_mention` / `is_room_mention`
    /// 混进来会破坏有意提及，所以清单本身也要被钉住。
    #[test]
    fn msc4210_legacy_id_list_matches_the_msc() {
        let mut sorted = MSC4210_LEGACY_MENTION_RULE_IDS.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![".m.rule.contains_display_name", ".m.rule.contains_user_name", ".m.rule.roomnotif"]);
    }

    #[test]
    fn msc4210_hides_exactly_the_three_legacy_mention_rules() {
        let hidden_ids = all_rule_ids(&default_push_rules_for_user("@alice:example.com", "alice", true));
        for legacy in MSC4210_LEGACY_MENTION_RULE_IDS {
            assert!(!hidden_ids.iter().any(|id| id == legacy), "{legacy} 必须停止下发：{hidden_ids:?}");
        }
        // 有意提及规则必须**保留**：连它们一起删会静默破坏 `m.mentions`。
        for intentional in [".m.rule.is_user_mention", ".m.rule.is_room_mention"] {
            assert!(hidden_ids.iter().any(|id| id == intentional), "{intentional} 必须保留：{hidden_ids:?}");
        }

        let kept_ids = all_rule_ids(&default_push_rules_for_user("@alice:example.com", "alice", false));
        for legacy in MSC4210_LEGACY_MENTION_RULE_IDS {
            assert!(kept_ids.iter().any(|id| id == legacy), "opt-out 时必须保留 {legacy}");
        }
    }

    #[test]
    fn msc4210_merge_drops_stored_legacy_rules_but_keeps_custom_ones() {
        let stored = json!({
            "global": {
                "content": [
                    {"rule_id": ".m.rule.contains_user_name", "default": true, "enabled": true,
                     "pattern": "alice", "actions": []}
                ],
                "override": [
                    {"rule_id": ".m.rule.contains_display_name", "default": true, "enabled": true,
                     "conditions": [], "actions": []},
                    {"rule_id": ".m.rule.roomnotif", "default": true, "enabled": true,
                     "conditions": [], "actions": []},
                    {"rule_id": "com.example.custom", "enabled": true, "conditions": [], "actions": []}
                ]
            }
        });

        let mut on = stored.clone();
        merge_default_push_rules(&mut on, "@alice:example.com", "alice", true);
        let on_ids = all_rule_ids(&on);
        for legacy in MSC4210_LEGACY_MENTION_RULE_IDS {
            assert!(!on_ids.iter().any(|id| id == legacy), "已持久化的 `default: true` 旧规则也应停止下发：{on_ids:?}");
        }
        assert!(on_ids.iter().any(|id| id == "com.example.custom"), "用户自定义规则不能被连带丢掉：{on_ids:?}");

        let mut off = stored.clone();
        merge_default_push_rules(&mut off, "@alice:example.com", "alice", false);
        let off_ids = all_rule_ids(&off);
        for legacy in MSC4210_LEGACY_MENTION_RULE_IDS {
            assert!(off_ids.iter().any(|id| id == legacy), "opt-out 时必须保留 {legacy}");
        }
    }

    #[test]
    fn remove_msc4210_legacy_rules_tolerates_a_malformed_document() {
        let mut empty = json!({});
        remove_msc4210_legacy_rules(&mut empty);
        assert_eq!(empty, json!({}), "空文档不应被改写");

        let mut no_global = json!({"foo": 1});
        remove_msc4210_legacy_rules(&mut no_global);
        assert_eq!(no_global, json!({"foo": 1}), "缺 global 不应 panic 也不应改写");
    }
}
