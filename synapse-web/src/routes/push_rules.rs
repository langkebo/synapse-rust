//! HTTP handlers for the Matrix Client-Server push rules API.
//!
//! All pure push-rule logic (default rule generation, merge, validation) lives
//! in `synapse_common::push_rules`. This module only contains the Axum HTTP
//! handlers that call into that shared logic.

use crate::routes::context::SyncContext;
use crate::routes::extractors::AuthenticatedUser;
use axum::{extract::State, Json};
use serde_json::Value;
use synapse_common::ApiError;

// Re-export shared logic from synapse-common (single source of truth).
pub use synapse_common::push_rules::{default_push_rules_for_user, get_default_push_rules, merge_default_push_rules};

/// See [`get_push_rules_default`].
pub async fn get_push_rules_default(
    State(ctx): State<SyncContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let row: Option<Value> = ctx.client_push_service.get_push_rules_content(&auth_user.user_id).await?;

    let username: &str = auth_user.user_id.trim_start_matches('@').split(':').next().unwrap_or("");

    if let Some(mut content) = row {
        merge_default_push_rules(&mut content, &auth_user.user_id, username);
        return Ok(Json(content));
    }

    Ok(Json(default_push_rules_for_user(&auth_user.user_id, username)))
}

/// See [`get_push_rules_global_default`].
pub async fn get_push_rules_global_default(
    State(ctx): State<SyncContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let username: String = auth_user.user_id.trim_start_matches('@').split(':').next().unwrap_or("").to_string();
    let user_id: String = auth_user.user_id.clone();
    let rules: Json<Value> = get_push_rules_default(State(ctx), auth_user).await?;
    if let Some(global) = rules.0.get("global") {
        Ok(Json(global.clone()))
    } else {
        Ok(Json(default_push_rules_for_user(&user_id, &username)["global"].clone()))
    }
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
    fn default_rules_include_required_override_ids() {
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
            ".org.matrix.msc3786.rule.room.server_acl",
            ".m.rule.suppress_edits",
        ] {
            assert!(ids.contains(&required), "missing override {required}");
        }
    }

    #[test]
    fn default_rules_include_msc_prefixed_underride_ids() {
        let rules = default_push_rules_for_user("@alice:example.com", "alice");
        let underride_rules = rules["global"]["underride"].as_array().unwrap();
        let ids: Vec<&str> = underride_rules.iter().filter_map(|r| r["rule_id"].as_str()).collect();
        assert!(ids.contains(&".org.matrix.msc3914.rule.room.call"), "missing MSC3914 call rule in underride");
        // Old rule ID must not be emitted.
        assert!(!ids.contains(&".m.rule.call"), "old .m.rule.call should be replaced by MSC3914 ID");
    }

    #[test]
    fn merge_preserves_old_call_rule_customisations() {
        let mut content = serde_json::json!({
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
        merge_default_push_rules(&mut content, "@alice:example.com", "alice");
        let underrides = content["global"]["underride"].as_array().unwrap();
        let call_rule = underrides.iter().find(|r| r["rule_id"] == ".org.matrix.msc3914.rule.room.call").unwrap();
        // User's enabled=false should survive the ID migration.
        assert_eq!(call_rule["enabled"], false);
    }

    #[test]
    fn merge_adds_missing_rules_without_clobbering() {
        let mut content = serde_json::json!({
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
