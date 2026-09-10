//! Unit tests for user storage models (no DB required).
//!
//! Split from the former `user::tests` inline module.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn test_escape_like_pattern_backslash() {
    assert_eq!(escape_like_pattern(r"test\user"), r"test\\user");
}

#[test]
fn test_escape_like_pattern_percent() {
    assert_eq!(escape_like_pattern("100%"), "100\\%");
}

#[test]
fn test_escape_like_pattern_underscore() {
    assert_eq!(escape_like_pattern("a_b"), "a\\_b");
}

#[test]
fn test_escape_like_pattern_combined() {
    assert_eq!(escape_like_pattern(r"50%_test\foo"), r"50\%\_test\\foo");
}

#[test]
fn test_escape_like_pattern_no_special_chars() {
    let input = "simpleusername";
    assert_eq!(escape_like_pattern(input), input);
}

#[test]
fn test_escape_like_pattern_empty() {
    assert_eq!(escape_like_pattern(""), "");
}

#[test]
fn test_user_struct_fields() {
    let user = User {
        user_id: "@alice:example.com".to_string(),
        username: "alice".to_string(),
        password_hash: Some("hash123".to_string()),
        is_admin: true,
        is_guest: false,
        is_shadow_banned: false,
        is_deactivated: false,
        created_ts: 1700000000000,
        updated_ts: Some(1700000000001),
        displayname: Some("Alice".to_string()),
        avatar_url: Some("mxc://example.com/avatar".to_string()),
        email: Some("alice@example.com".to_string()),
        phone: None,
        generation: Some(1),
        consent_version: Some("1.0".to_string()),
        appservice_id: None,
        user_type: None,
        invalid_update_at: None,
        migration_state: None,
        password_changed_ts: Some(1700000000000),
        is_password_change_required: false,
        password_expires_at: None,
        failed_login_attempts: 0,
        locked_until: None,
        must_change_password: false,
    };

    assert_eq!(user.user_id(), "@alice:example.com");
    assert_eq!(user.username, "alice");
    assert!(user.is_admin);
    assert!(!user.is_guest);
    assert_eq!(user.displayname.as_deref(), Some("Alice"));
    assert_eq!(user.generation, Some(1));
}

#[test]
fn test_user_struct_minimal() {
    let user = User {
        user_id: "@bob:example.com".to_string(),
        username: "bob".to_string(),
        password_hash: None,
        is_admin: false,
        is_guest: false,
        is_shadow_banned: false,
        is_deactivated: false,
        created_ts: 0,
        updated_ts: None,
        displayname: None,
        avatar_url: None,
        email: None,
        phone: None,
        generation: Some(0),
        consent_version: None,
        appservice_id: None,
        user_type: None,
        invalid_update_at: None,
        migration_state: None,
        password_changed_ts: None,
        is_password_change_required: false,
        password_expires_at: None,
        failed_login_attempts: 0,
        locked_until: None,
        must_change_password: false,
    };

    assert_eq!(user.user_id, "@bob:example.com");
    assert!(!user.is_admin);
    assert!(user.password_hash.is_none());
    assert!(user.displayname.is_none());
}

#[test]
fn test_user_serde_roundtrip() {
    let user = User {
        user_id: "@charlie:example.com".to_string(),
        username: "charlie".to_string(),
        password_hash: None,
        is_admin: false,
        is_guest: false,
        is_shadow_banned: false,
        is_deactivated: true,
        created_ts: 1700000000000,
        updated_ts: None,
        displayname: Some("Charlie".to_string()),
        avatar_url: None,
        email: None,
        phone: None,
        generation: Some(0),
        consent_version: None,
        appservice_id: None,
        user_type: Some("support".to_string()),
        invalid_update_at: None,
        migration_state: None,
        password_changed_ts: None,
        is_password_change_required: false,
        password_expires_at: None,
        failed_login_attempts: 0,
        locked_until: None,
        must_change_password: false,
    };

    let json = serde_json::to_string(&user).unwrap();
    let deserialized: User = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.user_id, user.user_id);
    assert_eq!(deserialized.username, user.username);
    assert_eq!(deserialized.is_deactivated, user.is_deactivated);
    assert_eq!(deserialized.user_type, user.user_type);
    // password_hash should NOT be serialized
    assert!(!json.contains("password_hash"));
}

#[test]
fn test_user_profile_serde() {
    let profile = UserProfile {
        user_id: "@dave:example.com".to_string(),
        username: "dave".to_string(),
        displayname: Some("Dave".to_string()),
        avatar_url: Some("mxc://example.com/dave".to_string()),
        created_ts: 1700000000000,
        updated_ts: None,
    };

    let json = serde_json::to_string(&profile).unwrap();
    let deserialized: UserProfile = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.user_id, profile.user_id);
    assert_eq!(deserialized.displayname, profile.displayname);
}

#[test]
fn test_user_search_result_serde() {
    let result = UserSearchResult {
        user_id: "@eve:example.com".to_string(),
        username: "eve".to_string(),
        displayname: Some("Eve".to_string()),
        avatar_url: None,
        created_ts: 1700000000000,
    };

    let json = serde_json::to_string(&result).unwrap();
    let deserialized: UserSearchResult = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.user_id, result.user_id);
    assert_eq!(deserialized.username, result.username);
}

#[test]
fn test_locked_user_serde() {
    let locked = LockedUser {
        id: 1,
        user_id: "@baduser:example.com".to_string(),
        reason: Some("Too many failed attempts".to_string()),
        locked_by: "@admin:example.com".to_string(),
        created_ts: 1700000000000,
        unlocked_ts: None,
        is_active: true,
    };

    let json = serde_json::to_string(&locked).unwrap();
    let deserialized: LockedUser = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.user_id, locked.user_id);
    assert_eq!(deserialized.reason, locked.reason);
    assert!(deserialized.is_active);
}

#[test]
fn test_user_stats_summary_serde() {
    let stats = UserStatsSummary {
        total_users: 100,
        active_users: 50,
        admin_users: 5,
        deactivated_users: 10,
        guest_users: 20,
    };

    let json = serde_json::to_string(&stats).unwrap();
    let deserialized: UserStatsSummary = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.total_users, 100);
    assert_eq!(deserialized.active_users, 50);
    assert_eq!(deserialized.admin_users, 5);
}

#[test]
fn test_user_search_result_with_presence_serde() {
    let result = UserSearchResultWithPresence {
        user_id: "@frank:example.com".to_string(),
        username: "frank".to_string(),
        displayname: Some("Frank".to_string()),
        avatar_url: None,
        created_ts: 1700000000000,
        presence: Some("online".to_string()),
        last_active_ts: Some(1700000000000),
    };

    let json = serde_json::to_string(&result).unwrap();
    let deserialized: UserSearchResultWithPresence = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.user_id, result.user_id);
    assert_eq!(deserialized.presence.as_deref(), Some("online"));
    assert_eq!(deserialized.last_active_ts, Some(1700000000000));
}

#[test]
fn test_user_directory_search_result_serde() {
    let result = UserDirectorySearchResult {
        user_id: "@grace:example.com".to_string(),
        username: "grace".to_string(),
        displayname: Some("Grace".to_string()),
        avatar_url: None,
        created_ts: 1700000000000,
        presence: Some("offline".to_string()),
        last_active_ts: None,
        match_score: 95,
        match_type: "trigram".to_string(),
    };

    let json = serde_json::to_string(&result).unwrap();
    let deserialized: UserDirectorySearchResult = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.user_id, result.user_id);
    assert_eq!(deserialized.match_score, 95);
    assert_eq!(deserialized.match_type, "trigram");
}
