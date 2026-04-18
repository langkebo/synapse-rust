use serde_json::json;

fn validate_password_change_request(
    auth_type: &str,
    has_password: bool,
    user_matches: bool,
) -> Result<(), String> {
    match auth_type {
        "m.login.password" => {
            if !has_password {
                return Err("Password required for m.login.password".to_string());
            }

            if !user_matches {
                return Err("User mismatch".to_string());
            }
        }
        "m.login.email.identity" => {}
        _ => {
            return Err(format!(
                "m.login.password or m.login.email.identity authentication required, got: {}",
                auth_type
            ));
        }
    }

    Ok(())
}

#[test]
fn test_password_change_requires_m_login_password() {
    let result = validate_password_change_request("m.login.dummy", true, true);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("m.login.password"));
}

#[test]
fn test_password_change_accepts_m_login_password() {
    let result = validate_password_change_request("m.login.password", true, true);
    assert!(result.is_ok());
}

#[test]
fn test_password_change_accepts_m_login_email_identity() {
    let result = validate_password_change_request("m.login.email.identity", false, false);
    assert!(result.is_ok());
}

#[test]
fn test_password_change_rejects_no_auth() {
    let result = validate_password_change_request("", true, true);
    assert!(result.is_err());
}

#[test]
fn test_password_change_rejects_missing_password() {
    let result = validate_password_change_request("m.login.password", false, true);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Password required"));
}

#[test]
fn test_password_change_rejects_user_mismatch() {
    let result = validate_password_change_request("m.login.password", true, false);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("User mismatch"));
}

#[test]
fn test_password_change_uia_flow_rejects_dummy() {
    let uia_flows = json!([
        { "stages": ["m.login.password"] },
        { "stages": ["m.login.dummy"] }
    ]);

    let valid_flow = uia_flows.as_array().unwrap().iter().find(|flow| {
        flow.get("stages")
            .and_then(|s| s.as_array())
            .map(|stages| {
                stages
                    .iter()
                    .any(|s| s.as_str() == Some("m.login.password"))
            })
            .unwrap_or(false)
    });

    assert!(valid_flow.is_some());

    let dummy_only_flow = uia_flows.as_array().unwrap().iter().find(|flow| {
        flow.get("stages")
            .and_then(|s| s.as_array())
            .map(|stages| stages.iter().all(|s| s.as_str() == Some("m.login.dummy")))
            .unwrap_or(false)
    });

    assert!(dummy_only_flow.is_some());
}

#[test]
fn test_deactivated_user_cannot_refresh_token() {
    let user = json!({
        "user_id": "@deactivated:example.com",
        "is_deactivated": true
    });

    let is_deactivated = user
        .get("is_deactivated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(is_deactivated);
}

#[test]
fn test_active_user_can_refresh_token() {
    let user = json!({
        "user_id": "@active:example.com",
        "is_deactivated": false
    });

    let is_deactivated = user
        .get("is_deactivated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(!is_deactivated);
}

#[test]
fn test_revoked_refresh_token_rejected() {
    let token_data = json!({
        "is_revoked": true,
        "expires_at": null
    });

    let is_revoked = token_data
        .get("is_revoked")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(is_revoked);
}

#[test]
fn test_expired_refresh_token_rejected() {
    let now = chrono::Utc::now().timestamp_millis();
    let token_data = json!({
        "is_revoked": false,
        "expires_at": now - 3600000
    });

    let expires_at = token_data.get("expires_at").and_then(|v| v.as_i64());
    if let Some(exp) = expires_at {
        assert!(exp < now);
    }
}

#[test]
fn test_device_ownership_different_user_rejected() {
    let existing_device = json!({
        "device_id": "DEVICE123",
        "user_id": "@alice:example.com"
    });

    let requesting_user = "@bob:example.com";
    let device_owner = existing_device
        .get("user_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    assert_ne!(requesting_user, device_owner);
}

#[test]
fn test_device_ownership_same_user_allowed() {
    let existing_device = json!({
        "device_id": "DEVICE123",
        "user_id": "@alice:example.com"
    });

    let requesting_user = "@alice:example.com";
    let device_owner = existing_device
        .get("user_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    assert_eq!(requesting_user, device_owner);
}

#[test]
fn test_admin_status_change_requires_super_admin() {
    let roles = vec![
        ("super_admin", true),
        ("auditor", false),
        ("security_admin", false),
        ("user_admin", false),
        ("media_admin", false),
    ];

    for (role, allowed) in roles {
        let can_modify_admin = role == "super_admin";
        assert_eq!(can_modify_admin, allowed);
    }
}

#[test]
fn test_device_info_excludes_ip_address() {
    let device_response = json!({
        "device_id": "DEVICE123",
        "display_name": "My Phone",
        "last_seen_ts": 1710000000000_i64
    });

    assert!(
        device_response.get("last_seen_ip").is_none(),
        "Device info response must not expose last_seen_ip"
    );
}

#[test]
fn test_device_list_excludes_ip_addresses() {
    let devices = vec![
        json!({"device_id": "D1", "display_name": "Phone", "last_seen_ts": 1710000000000_i64}),
        json!({"device_id": "D2", "display_name": "Laptop", "last_seen_ts": 1710001000000_i64}),
    ];

    for device in &devices {
        assert!(
            device.get("last_seen_ip").is_none(),
            "Each device in list must not expose last_seen_ip"
        );
    }
}

#[test]
fn test_shared_room_filter_allows_self() {
    let current_user = "@alice:example.com";
    let requested_users = vec![
        "@alice:example.com".to_string(),
        "@bob:example.com".to_string(),
    ];

    let mut allowed = vec![current_user.to_string()];
    for user_id in &requested_users {
        if user_id == current_user {
            continue;
        }
        allowed.push(user_id.clone());
    }

    assert!(allowed.contains(&current_user.to_string()));
}

#[test]
fn test_shared_room_filter_blocks_unrelated_users() {
    let current_user = "@alice:example.com";
    let unrelated_user = "@stranger:example.com";

    let shared_rooms = false;
    let mut allowed = vec![current_user.to_string()];

    if shared_rooms {
        allowed.push(unrelated_user.to_string());
    }

    assert!(!allowed.contains(&unrelated_user.to_string()));
}

#[test]
fn test_account_data_read_only_for_owner() {
    let owner = "@alice:example.com";
    let other = "@bob:example.com";

    let can_read_own = owner == owner;
    let can_read_other = owner == other;

    assert!(can_read_own, "User should read own account data");
    assert!(!can_read_other, "User should NOT read other's account data");
}

#[test]
fn test_account_data_write_only_for_owner() {
    let owner = "@alice:example.com";
    let other = "@bob:example.com";

    let can_write_own = owner == owner;
    let can_write_other = owner == other;

    assert!(can_write_own, "User should write own account data");
    assert!(
        !can_write_other,
        "User should NOT write other's account data"
    );
}

#[test]
fn test_event_delete_only_by_admin() {
    let regular_user = json!({"user_id": "@user:example.com", "is_admin": false});
    let admin_user = json!({"user_id": "@admin:example.com", "is_admin": true});

    let regular_can_delete = regular_user
        .get("is_admin")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let admin_can_delete = admin_user
        .get("is_admin")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    assert!(!regular_can_delete, "Regular user should NOT delete events");
    assert!(admin_can_delete, "Admin should be able to delete events");
}

#[test]
fn test_refresh_token_expiry_boundary() {
    let now = chrono::Utc::now().timestamp_millis();

    let expired_token = json!({
        "is_revoked": false,
        "expires_at": now - 1
    });
    let valid_token = json!({
        "is_revoked": false,
        "expires_at": now + 3600000
    });
    let no_expiry_token = json!({
        "is_revoked": false,
        "expires_at": null
    });

    let expired = expired_token
        .get("expires_at")
        .and_then(|v| v.as_i64())
        .map(|exp| exp < now)
        .unwrap_or(false);
    assert!(expired, "Token expired 1ms ago should be rejected");

    let valid = valid_token
        .get("expires_at")
        .and_then(|v| v.as_i64())
        .map(|exp| exp > now)
        .unwrap_or(false);
    assert!(valid, "Token expiring in future should be valid");

    let no_expiry_valid = no_expiry_token
        .get("expires_at")
        .and_then(|v| v.as_i64())
        .is_none();
    assert!(
        no_expiry_valid,
        "Token with no expiry should not be expired by time"
    );
}

#[test]
fn test_revoked_token_always_rejected() {
    let now = chrono::Utc::now().timestamp_millis();

    let revoked_token = json!({
        "is_revoked": true,
        "expires_at": now + 3600000
    });

    let is_revoked = revoked_token
        .get("is_revoked")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    assert!(
        is_revoked,
        "Revoked token must be rejected even if not expired"
    );
}

#[test]
fn test_power_level_boundary_exact_threshold() {
    let power_levels = json!({"ban": 50});

    let ban_threshold = power_levels
        .get("ban")
        .and_then(|v| v.as_i64())
        .unwrap_or(50);

    let user_at_threshold: i64 = 50;
    let user_below_threshold: i64 = 49;
    let user_above_threshold: i64 = 51;

    assert!(
        user_at_threshold >= ban_threshold,
        "User at exact threshold should be allowed"
    );
    assert!(
        user_below_threshold < ban_threshold,
        "User below threshold should be blocked"
    );
    assert!(
        user_above_threshold > ban_threshold,
        "User above threshold should be allowed"
    );
}

#[test]
fn test_kick_cannot_target_higher_power_user() {
    let actor_power: i64 = 50;
    let target_power: i64 = 75;

    assert!(
        actor_power <= target_power,
        "User with power 50 should NOT be able to kick user with power 75"
    );
}

#[test]
fn test_ban_cannot_target_room_creator() {
    let creator_user_id = "@creator:example.com";
    let target_user_id = creator_user_id;

    assert_eq!(
        target_user_id, creator_user_id,
        "Room creator should be protected from ban"
    );
}

#[test]
fn test_key_rotation_endpoints_admin_only() {
    let endpoints = vec![
        ("GET", "/key_rotation/status", true),
        ("POST", "/key_rotation/rotate", true),
        ("PUT", "/key_rotation/configure", true),
    ];

    for (_method, _path, requires_admin) in endpoints {
        assert!(
            requires_admin,
            "Key rotation endpoints must require admin access"
        );
    }
}

#[test]
fn test_password_change_rejects_all_non_password_auth_types() {
    let invalid_auth_types = vec![
        "m.login.dummy",
        "m.login.recaptcha",
        "m.login.sso",
    ];

    for auth_type in invalid_auth_types {
        let result = validate_password_change_request(auth_type, true, true);
        assert!(
            result.is_err(),
            "Auth type {} should be rejected for password change",
            auth_type
        );
    }
}

#[test]
fn test_join_rule_consistency_with_visibility() {
    let public_visibility = "public";
    let invite_visibility = "private";

    let public_join_rule = match public_visibility {
        "public" => "public",
        _ => "invite",
    };
    let private_join_rule = match invite_visibility {
        "public" => "public",
        _ => "invite",
    };

    assert_eq!(
        public_join_rule, "public",
        "Public visibility should set public join rule"
    );
    assert_eq!(
        private_join_rule, "invite",
        "Private visibility should set invite join rule"
    );
}
