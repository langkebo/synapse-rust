use crate::web::routes::AppState;
use crate::web::extractors::AuthenticatedUser;
use crate::common::ApiError;
use axum::{
    extract::{State},
    Json,
};
use serde_json::Value;

pub async fn get_push_rules_default(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let rows = sqlx::query(
        "SELECT content FROM account_data WHERE user_id = $1 AND data_type = 'm.push_rules'",
    )
    .bind(&auth_user.user_id)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get push rules: {}", e)))?;

    if let Some(row) = rows {
        use sqlx::Row;
        let content: Value = row.get("content");
        return Ok(Json(content));
    }

    Ok(Json(get_default_push_rules()))
}

pub async fn get_push_rules_global_default(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let rules = get_push_rules_default(State(state), auth_user).await?;
    if let Some(global) = rules.0.get("global") {
        Ok(Json(global.clone()))
    } else {
        Ok(Json(get_default_push_rules()["global"].clone()))
    }
}

pub fn get_default_push_rules() -> Value {
    serde_json::json!({
        "global": {
            "content": [
                {
                    "rule_id": ".m.rule.contains_display_name",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "contains_display_name"}],
                    "actions": ["notify", {"set_tweak": "highlight"}, {"set_tweak": "sound", "value": "default"}]
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
                    "actions": ["dont_notify"]
                },
                {
                    "rule_id": ".m.rule.invite_for_me",
                    "default": true,
                    "enabled": true,
                    "conditions": [
                        {"kind": "event_match", "key": "type", "pattern": "m.room.member"},
                        {"kind": "event_match", "key": "content.membership", "pattern": "invite"},
                        {"kind": "event_state_key_is_me"}
                    ],
                    "actions": ["notify", {"set_tweak": "sound", "value": "default"}]
                },
                {
                    "rule_id": ".m.rule.member_event",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.room.member"}],
                    "actions": ["dont_notify"]
                },
                {
                    "rule_id": ".m.rule.call",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.call.invite"}],
                    "actions": ["notify", {"set_tweak": "sound", "value": "ring"}]
                }
            ],
            "room": [],
            "sender": [],
            "underride": [
                {
                    "rule_id": ".m.rule.message",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.room.message"}],
                    "actions": ["notify", {"set_tweak": "sound", "value": "default"}]
                },
                {
                    "rule_id": ".m.rule.encrypted",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.room.encrypted"}],
                    "actions": ["notify", {"set_tweak": "sound", "value": "default"}]
                },
                {
                    "rule_id": ".m.rule.room_one_to_one",
                    "default": true,
                    "enabled": true,
                    "conditions": [
                        {"kind": "room_member_count", "is": "2"},
                        {"kind": "event_match", "key": "type", "pattern": "m.room.message"}
                    ],
                    "actions": ["notify", {"set_tweak": "sound", "value": "default"}]
                }
            ]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_default_push_rules_returns_valid_structure() {
        let rules = get_default_push_rules();
        assert!(rules.get("global").is_some());
        let global = rules.get("global").unwrap();
        assert!(global.get("content").is_some());
        assert!(global.get("override").is_some());
        assert!(global.get("room").is_some());
        assert!(global.get("sender").is_some());
        assert!(global.get("underride").is_some());
    }

    #[test]
    fn test_default_push_rules_have_required_fields() {
        let rules = get_default_push_rules();
        let global = rules.get("global").unwrap();
        let content = global.get("content").unwrap().as_array().unwrap();
        assert!(!content.is_empty());
        let first_rule = &content[0];
        assert!(first_rule.get("rule_id").is_some());
        assert!(first_rule.get("enabled").is_some());
        assert!(first_rule.get("conditions").is_some());
        assert!(first_rule.get("actions").is_some());
    }
}
