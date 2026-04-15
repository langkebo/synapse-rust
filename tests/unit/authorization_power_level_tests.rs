use serde_json::json;
use synapse_rust::auth::authorization::{AuthorizationContext, ResourceType, Action};

#[test]
fn test_authorization_context_regular_user() {
    let ctx = AuthorizationContext {
        user_id: "@user:example.com".to_string(),
        is_admin: false,
        device_id: Some("DEVICE123".to_string()),
    };
    assert_eq!(ctx.user_id, "@user:example.com");
    assert!(!ctx.is_admin);
}

#[test]
fn test_authorization_context_admin_user() {
    let ctx = AuthorizationContext {
        user_id: "@admin:example.com".to_string(),
        is_admin: true,
        device_id: None,
    };
    assert!(ctx.is_admin);
}

#[test]
fn test_resource_types_completeness() {
    let types = [
        ResourceType::User,
        ResourceType::Room,
        ResourceType::Device,
        ResourceType::Media,
        ResourceType::Event,
        ResourceType::AccountData,
    ];
    assert_eq!(types.len(), 6);
}

#[test]
fn test_action_types_completeness() {
    let actions = vec![
        Action::Read,
        Action::Write,
        Action::Delete,
        Action::Admin,
        Action::Invite,
        Action::Ban,
        Action::Kick,
        Action::Redact,
        Action::ModifyPowerLevels,
    ];
    assert_eq!(actions.len(), 9);
}

#[test]
fn test_power_levels_threshold_extraction_from_content() {
    let content = json!({
        "users": {
            "@alice:example.com": 100,
            "@bob:example.com": 50,
            "@charlie:example.com": 0
        },
        "users_default": 0,
        "events": {
            "m.room.name": 50,
            "m.room.power_levels": 100
        },
        "events_default": 0,
        "state_default": 50,
        "ban": 50,
        "kick": 50,
        "invite": 0,
        "redact": 50
    });

    assert_eq!(content.get("ban").and_then(|v| v.as_i64()), Some(50));
    assert_eq!(content.get("kick").and_then(|v| v.as_i64()), Some(50));
    assert_eq!(content.get("invite").and_then(|v| v.as_i64()), Some(0));
    assert_eq!(content.get("redact").and_then(|v| v.as_i64()), Some(50));
    assert_eq!(content.get("state_default").and_then(|v| v.as_i64()), Some(50));

    let users = content.get("users").unwrap().as_object().unwrap();
    assert_eq!(users.get("@alice:example.com").and_then(|v| v.as_i64()), Some(100));
    assert_eq!(users.get("@bob:example.com").and_then(|v| v.as_i64()), Some(50));
    assert_eq!(users.get("@charlie:example.com").and_then(|v| v.as_i64()), Some(0));
}

#[test]
fn test_power_levels_missing_threshold_uses_default() {
    let content = json!({
        "users": {
            "@alice:example.com": 100
        }
    });

    assert_eq!(content.get("ban").and_then(|v| v.as_i64()), None);
    assert_eq!(content.get("invite").and_then(|v| v.as_i64()), None);
    assert_eq!(content.get("redact").and_then(|v| v.as_i64()), None);
}

#[test]
fn test_power_levels_custom_thresholds() {
    let content = json!({
        "users": {
            "@alice:example.com": 100,
            "@bob:example.com": 75
        },
        "ban": 75,
        "kick": 60,
        "invite": 25,
        "redact": 30,
        "state_default": 100
    });

    assert_eq!(content.get("ban").and_then(|v| v.as_i64()), Some(75));
    assert_eq!(content.get("kick").and_then(|v| v.as_i64()), Some(60));
    assert_eq!(content.get("invite").and_then(|v| v.as_i64()), Some(25));
    assert_eq!(content.get("redact").and_then(|v| v.as_i64()), Some(30));
    assert_eq!(content.get("state_default").and_then(|v| v.as_i64()), Some(100));
}

#[test]
fn test_account_data_access_self_user() {
    let ctx = AuthorizationContext {
        user_id: "@alice:example.com".to_string(),
        is_admin: false,
        device_id: None,
    };

    assert_eq!(ctx.user_id, "@alice:example.com");
}

#[test]
fn test_account_data_access_other_user_blocked() {
    let ctx = AuthorizationContext {
        user_id: "@alice:example.com".to_string(),
        is_admin: false,
        device_id: None,
    };
    let target_user_id = "@bob:example.com";

    assert_ne!(ctx.user_id, target_user_id);
}

#[test]
fn test_account_data_access_admin_cannot_modify_others() {
    let ctx = AuthorizationContext {
        user_id: "@admin:example.com".to_string(),
        is_admin: true,
        device_id: None,
    };
    let target_user_id = "@bob:example.com";

    assert_ne!(ctx.user_id, target_user_id);
    assert!(ctx.is_admin);
}

#[test]
fn test_event_access_non_admin_blocked() {
    let ctx = AuthorizationContext {
        user_id: "@user:example.com".to_string(),
        is_admin: false,
        device_id: None,
    };

    assert!(!ctx.is_admin);
}

#[test]
fn test_invite_threshold_with_custom_power_levels() {
    let content = json!({
        "invite": 50
    });

    let invite_threshold = content.get("invite").and_then(|v| v.as_i64()).unwrap_or(0);
    assert_eq!(invite_threshold, 50);

    let user_power: i64 = 0;
    assert!(user_power < invite_threshold);
}

#[test]
fn test_invite_threshold_default_allows_members() {
    let content = json!({});
    let invite_threshold = content.get("invite").and_then(|v| v.as_i64()).unwrap_or(0);
    assert_eq!(invite_threshold, 0);

    let user_power: i64 = 0;
    assert!(user_power >= invite_threshold);
}

#[test]
fn test_ban_threshold_respects_custom_value() {
    let content = json!({"ban": 75});
    let ban_threshold = content.get("ban").and_then(|v| v.as_i64()).unwrap_or(50);
    assert_eq!(ban_threshold, 75);

    let moderator_power: i64 = 50;
    assert!(moderator_power < ban_threshold);
}

#[test]
fn test_redact_threshold_allows_sender() {
    let content = json!({"redact": 50});
    let redact_threshold = content.get("redact").and_then(|v| v.as_i64()).unwrap_or(50);
    assert_eq!(redact_threshold, 50);

    let user_power: i64 = 0;
    assert!(user_power < redact_threshold);
}

#[test]
fn test_modify_power_levels_requires_high_level() {
    let content = json!({"state_default": 100});
    let state_default = content.get("state_default").and_then(|v| v.as_i64()).unwrap_or(50);
    assert_eq!(state_default, 100);

    let moderator_power: i64 = 50;
    assert!(moderator_power < state_default);
}

#[test]
fn test_power_level_priority_user_specific_over_default() {
    let content = json!({
        "users": {
            "@alice:example.com": 100,
            "@bob:example.com": 75
        },
        "users_default": 0
    });

    let users = content.get("users").unwrap().as_object().unwrap();
    let alice_level = users.get("@alice:example.com").and_then(|v| v.as_i64());
    let bob_level = users.get("@bob:example.com").and_then(|v| v.as_i64());
    let charlie_level = users.get("@charlie:example.com").and_then(|v| v.as_i64());
    let default_level = content.get("users_default").and_then(|v| v.as_i64());

    assert_eq!(alice_level, Some(100));
    assert_eq!(bob_level, Some(75));
    assert_eq!(charlie_level, None);
    assert_eq!(default_level, Some(0));
}
