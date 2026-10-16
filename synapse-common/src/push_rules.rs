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
///
/// `msc4075_enabled` (upstream's default is `false`) hides the two MSC4075
/// MatrixRTC rules — see [`MSC4075_RTC_RULE_IDS`].
pub fn default_push_rules_for_user(
    user_id: &str,
    username: &str,
    msc4210_enabled: bool,
    msc4075_enabled: bool,
) -> Value {
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
                    "rule_id": ".org.matrix.msc4075.rule.rtc.invite_for_me",
                    "default": true,
                    "enabled": true,
                    "conditions": [
                        {"kind": "event_match", "key": "type", "pattern": "org.matrix.msc4075.rtc.notification"},
                        {"kind": "event_property_contains", "key": "content.m\\.mentions.user_ids", "value": user_id}
                    ],
                    "actions": ["notify", {"set_tweak": "sound", "value": "ring"}]
                },
                {
                    "rule_id": ".org.matrix.msc4075.rule.rtc.invite_for_room",
                    "default": true,
                    "enabled": true,
                    "conditions": [
                        {"kind": "event_match", "key": "type", "pattern": "org.matrix.msc4075.rtc.notification"},
                        {"kind": "event_property_is", "key": "content.m\\.mentions.room", "value": true},
                        {"kind": "sender_notification_permission", "key": "room"}
                    ],
                    "actions": ["notify", {"set_tweak": "sound", "value": "ring"}]
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

    // MSC4075 is gated the other way round: the rules are built unconditionally
    // above (so the canonical set is one list, easy to reason about) and then
    // **removed** when the experiment is off. Removing rather than not-building
    // matters for `merge_default_push_rules`: a rule that is absent from the
    // canonical set is dropped from the merged output, so turning the experiment
    // off stops serving the rules without deleting the user's stored row.
    if !msc4075_enabled {
        remove_msc4075_rtc_rules(&mut rules);
    }

    rules
}

/// The two default override rules that upstream Synapse v1.163.0 (#20227)
/// added for [MSC4075](https://github.com/matrix-org/matrix-spec-proposals/pull/4075)
/// (`org.matrix.msc4075.rtc.notification` events).
///
/// Both notify **and ring**: the action list is `["notify", {"set_tweak":
/// "sound", "value": "ring"}]`, which is what makes an incoming MatrixRTC call
/// actually ring rather than merely badge. They must stay **immediately before**
/// `.m.rule.is_user_mention` / `.m.rule.is_room_mention`, because override rules
/// are first-match-wins and the generic mention rules would otherwise swallow
/// RTC notifications before these ever run.
///
/// Upstream gates them behind `experimental.msc4075_enabled` (default `false`);
/// this constant exists so the gate is expressed in one place and the removal
/// helper cannot drift from the ids actually built above.
pub const MSC4075_RTC_RULE_IDS: &[&str] =
    &[".org.matrix.msc4075.rule.rtc.invite_for_me", ".org.matrix.msc4075.rule.rtc.invite_for_room"];

/// Drop the [`MSC4075_RTC_RULE_IDS`] from a `/pushrules`-shaped document.
///
/// Both rules live in `override`; the other kinds are scanned anyway so the
/// helper stays correct if a future rule of this family is placed elsewhere.
/// Tolerates a missing or malformed `global` object.
pub fn remove_msc4075_rtc_rules(rules: &mut Value) {
    let Some(global) = rules.get_mut("global").and_then(|g| g.as_object_mut()) else {
        return;
    };
    for kind in ["content", "override", "underride"] {
        let Some(list) = global.get_mut(kind).and_then(|v| v.as_array_mut()) else {
            continue;
        };
        list.retain(|rule| {
            rule.get("rule_id").and_then(Value::as_str).is_none_or(|rule_id| !MSC4075_RTC_RULE_IDS.contains(&rule_id))
        });
    }
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
/// MSC4210 legacy mention rules when `msc4210_enabled`, or the MSC4075 RTC rules
/// when `msc4075_enabled` is off) is therefore dropped from the merged output —
/// the rule stops being served without the stored row being deleted.
pub fn merge_default_push_rules(
    content: &mut Value,
    user_id: &str,
    username: &str,
    msc4210_enabled: bool,
    msc4075_enabled: bool,
) {
    let defaults = default_push_rules_for_user(user_id, username, msc4210_enabled, msc4075_enabled);
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
        let rules = default_push_rules_for_user("@alice:example.com", "alice", true, false);
        let global = rules.get("global").unwrap();
        for k in ["content", "override", "room", "sender", "underride"] {
            assert!(global.get(k).is_some(), "missing {k}");
        }
    }

    #[test]
    fn default_rules_include_required_override_ids() {
        let rules = default_push_rules_for_user("@alice:example.com", "alice", true, false);
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
        let rules = default_push_rules_for_user("@alice:example.com", "alice", true, false);
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
        merge_default_push_rules(&mut content, "@alice:example.com", "alice", true, false);
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
        merge_default_push_rules(&mut content, "@alice:example.com", "alice", true, false);
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
        let hidden_ids = all_rule_ids(&default_push_rules_for_user("@alice:example.com", "alice", true, false));
        for legacy in MSC4210_LEGACY_MENTION_RULE_IDS {
            assert!(!hidden_ids.iter().any(|id| id == legacy), "{legacy} 必须停止下发：{hidden_ids:?}");
        }
        // 有意提及规则必须**保留**：连它们一起删会静默破坏 `m.mentions`。
        for intentional in [".m.rule.is_user_mention", ".m.rule.is_room_mention"] {
            assert!(hidden_ids.iter().any(|id| id == intentional), "{intentional} 必须保留：{hidden_ids:?}");
        }

        let kept_ids = all_rule_ids(&default_push_rules_for_user("@alice:example.com", "alice", false, false));
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
        merge_default_push_rules(&mut on, "@alice:example.com", "alice", true, false);
        let on_ids = all_rule_ids(&on);
        for legacy in MSC4210_LEGACY_MENTION_RULE_IDS {
            assert!(!on_ids.iter().any(|id| id == legacy), "已持久化的 `default: true` 旧规则也应停止下发：{on_ids:?}");
        }
        assert!(on_ids.iter().any(|id| id == "com.example.custom"), "用户自定义规则不能被连带丢掉：{on_ids:?}");

        let mut off = stored.clone();
        merge_default_push_rules(&mut off, "@alice:example.com", "alice", false, false);
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

    // -----------------------------------------------------------------------
    // MSC4075 —— MatrixRTC 邀请/来电（上游 v1.163.0 #20227）
    // -----------------------------------------------------------------------

    fn rules_with_msc4075(enabled: bool) -> Value {
        default_push_rules_for_user("@alice:example.com", "alice", true, enabled)
    }

    fn override_rule<'a>(rules: &'a Value, rule_id: &str) -> Option<&'a Value> {
        rules["global"]["override"].as_array()?.iter().find(|rule| rule["rule_id"] == rule_id)
    }

    #[test]
    fn msc4075_rule_id_list_matches_the_msc() {
        assert_eq!(
            MSC4075_RTC_RULE_IDS,
            &[".org.matrix.msc4075.rule.rtc.invite_for_me", ".org.matrix.msc4075.rule.rtc.invite_for_room"]
        );
    }

    #[test]
    fn msc4075_rules_are_absent_when_the_experiment_is_off() {
        // Upstream's default. The rules must not leak into the served set: a
        // client that fetches `/pushrules` would otherwise start ringing for
        // RTC notifications on a server that never opted in.
        let ids = all_rule_ids(&rules_with_msc4075(false));
        for id in MSC4075_RTC_RULE_IDS {
            assert!(!ids.iter().any(|candidate| candidate == id), "{id} 在开关关闭时不得下发：{ids:?}");
        }
    }

    #[test]
    fn msc4075_rules_notify_and_ring_when_enabled() {
        let rules = rules_with_msc4075(true);

        for id in MSC4075_RTC_RULE_IDS {
            let rule = override_rule(&rules, id).unwrap_or_else(|| panic!("{id} 必须出现在 override 段"));
            assert_eq!(rule["default"], true, "{id} 必须是默认规则");
            assert_eq!(rule["enabled"], true, "{id} 必须默认启用");
            // `ring` is the whole point: without it the client badges a call
            // invitation instead of ringing. Assert the exact action list so a
            // future tweak cannot silently drop the sound tweak.
            assert_eq!(rule["actions"], json!(["notify", {"set_tweak": "sound", "value": "ring"}]), "{id} 动作不符");
            // Every MSC4075 rule is scoped to the RTC notification event type.
            let conditions = rule["conditions"].as_array().expect("conditions must be an array");
            assert_eq!(
                conditions.first(),
                Some(&json!({"kind": "event_match", "key": "type", "pattern": "org.matrix.msc4075.rtc.notification"})),
                "{id} 必须先按事件类型过滤"
            );
        }

        // `invite_for_me` must carry the concrete user id, not a `value_type`:
        // the client-facing form replaces `value_type: user_id` with the value
        // (upstream `clientformat._convert_type_to_value`), so serving the
        // template form would leave clients unable to match.
        let invite_for_me = override_rule(&rules, ".org.matrix.msc4075.rule.rtc.invite_for_me").unwrap();
        assert_eq!(
            invite_for_me["conditions"][1],
            json!({
                "kind": "event_property_contains",
                "key": "content.m\\.mentions.user_ids",
                "value": "@alice:example.com"
            }),
            "invite_for_me 必须带被提及者的具体 user id"
        );

        // `invite_for_room` additionally needs the power-level gate, exactly
        // like `.m.rule.is_room_mention`.
        let invite_for_room = override_rule(&rules, ".org.matrix.msc4075.rule.rtc.invite_for_room").unwrap();
        assert_eq!(
            invite_for_room["conditions"][1],
            json!({"kind": "event_property_is", "key": "content.m\\.mentions.room", "value": true})
        );
        assert_eq!(
            invite_for_room["conditions"][2],
            json!({"kind": "sender_notification_permission", "key": "room"}),
            "invite_for_room 必须带房间通知权限门（任何人不可随意让全房响铃）"
        );
    }

    /// Placement is behavioural, not cosmetic: override rules are
    /// first-match-wins, so an RTC notification that also mentions the user by
    /// id would be swallowed by `.m.rule.is_user_mention` (whose actions omit
    /// the ring tweak) if the MSC4075 rules came after it.
    #[test]
    fn msc4075_rules_precede_the_generic_mention_rules() {
        let rules = rules_with_msc4075(true);
        let overrides = rules["global"]["override"].as_array().unwrap();
        let index_of = |rule_id: &str| {
            overrides.iter().position(|rule| rule["rule_id"] == rule_id).unwrap_or_else(|| {
                panic!("{rule_id} 必须存在于 override 段");
            })
        };

        let first_msc4075 = index_of(".org.matrix.msc4075.rule.rtc.invite_for_me");
        let last_msc4075 = index_of(".org.matrix.msc4075.rule.rtc.invite_for_room");
        assert!(
            last_msc4075 < index_of(".m.rule.is_user_mention"),
            "MSC4075 规则必须排在 .m.rule.is_user_mention 之前（首个匹配生效）"
        );
        assert!(
            last_msc4075 < index_of(".m.rule.is_room_mention"),
            "MSC4075 规则必须排在 .m.rule.is_room_mention 之前"
        );
        assert!(first_msc4075 <= last_msc4075, "两条 MSC4075 规则的顺序应保持稳定");
    }

    #[test]
    fn msc4075_merge_stops_serving_stored_rules_when_off() {
        // A user who fetched their rules while the experiment was on has the two
        // rows persisted as `default: true`. Turning the experiment off must
        // stop serving them (the stored rows stay, mirroring MSC4210).
        let stored = json!({
            "global": {
                "override": [
                    {"rule_id": ".org.matrix.msc4075.rule.rtc.invite_for_me", "default": true,
                     "enabled": true, "conditions": [], "actions": ["notify"]},
                    {"rule_id": "com.example.custom", "enabled": true, "conditions": [], "actions": []}
                ]
            }
        });

        let mut off = stored.clone();
        merge_default_push_rules(&mut off, "@alice:example.com", "alice", true, false);
        let off_ids = all_rule_ids(&off);
        assert!(
            !off_ids.iter().any(|id| id == ".org.matrix.msc4075.rule.rtc.invite_for_me"),
            "开关关闭后已持久化的 MSC4075 默认规则也必须停止下发：{off_ids:?}"
        );
        assert!(off_ids.iter().any(|id| id == "com.example.custom"), "用户自定义规则不能被连带丢掉：{off_ids:?}");

        let mut on = stored.clone();
        merge_default_push_rules(&mut on, "@alice:example.com", "alice", true, true);
        let on_ids = all_rule_ids(&on);
        assert!(
            on_ids.iter().any(|id| id == ".org.matrix.msc4075.rule.rtc.invite_for_me"),
            "开关打开时必须下发：{on_ids:?}"
        );
    }

    #[test]
    fn remove_msc4075_rtc_rules_tolerates_a_malformed_document() {
        let mut empty = json!({});
        remove_msc4075_rtc_rules(&mut empty);
        assert_eq!(empty, json!({}), "空文档不应被改写");

        let mut no_global = json!({"foo": 1});
        remove_msc4075_rtc_rules(&mut no_global);
        assert_eq!(no_global, json!({"foo": 1}), "缺 global 不应 panic 也不应改写");
    }
}
