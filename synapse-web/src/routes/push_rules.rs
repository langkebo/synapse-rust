use crate::routes::extractors::AuthenticatedUser;
use crate::routes::AppState;
use axum::{extract::State, Json};
use serde_json::Value;
use synapse_common::ApiError;

// Re-export from the extracted push_rules module in synapse-services
pub use synapse_services::sync_service::push_rules::{
    default_push_rules_for_user, get_default_push_rules, merge_default_push_rules,
};

pub async fn get_push_rules_default(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let username = auth_user.user_id.trim_start_matches('@').split(':').next().unwrap_or("");

    if let Some(mut content) = state
        .services
        .account
        .user_storage
        .get_account_data_content(&auth_user.user_id, "m.push_rules")
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get push rules", &e))?
    {
        merge_default_push_rules(&mut content, &auth_user.user_id, username);
        return Ok(Json(content));
    }

    Ok(Json(default_push_rules_for_user(&auth_user.user_id, username)))
}

pub async fn get_push_rules_global_default(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let username = auth_user.user_id.trim_start_matches('@').split(':').next().unwrap_or("").to_string();
    let user_id = auth_user.user_id.clone();
    let rules = get_push_rules_default(State(state), auth_user).await?;
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
