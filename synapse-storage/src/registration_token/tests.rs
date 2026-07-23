use super::{decode_registration_token_cursor, encode_registration_token_cursor, RegistrationTokenCursor};
use synapse_common::current_timestamp_millis;

#[test]
fn registration_token_cursor_round_trip() {
    let cursor = RegistrationTokenCursor { created_ts: 1_746_700_000_000, id: 42 };

    let encoded = encode_registration_token_cursor(&cursor);
    assert_eq!(decode_registration_token_cursor(Some(&encoded)), Some(cursor));
}

#[test]
fn registration_token_cursor_rejects_invalid_values() {
    assert_eq!(decode_registration_token_cursor(None), None);
    assert_eq!(decode_registration_token_cursor(Some("bad")), None);
    assert_eq!(decode_registration_token_cursor(Some("123|")), None);
    assert_eq!(decode_registration_token_cursor(Some("123|456|789")), None);
}

use super::*;

fn create_test_token() -> RegistrationToken {
    RegistrationToken {
        id: 1,
        token: "TestToken123456789".to_string(),
        token_type: "single_use".to_string(),
        description: Some("Test token for unit tests".to_string()),
        max_uses: 1,
        uses_count: 0,
        is_used: false,
        is_enabled: true,
        expires_at: None,
        created_by: Some("@admin:example.com".to_string()),
        created_ts: 1700000000000,
        updated_ts: Some(1700000000000),
        last_used_ts: None,
        allowed_email_domains: Some(vec!["example.com".to_string()]),
        allowed_user_ids: Some(vec!["@user1:example.com".to_string()]),
        auto_join_rooms: Some(vec!["!room1:example.com".to_string()]),
        display_name: Some("Test User".to_string()),
        email: Some("test@example.com".to_string()),
    }
}

#[test]
fn test_registration_token_creation() {
    let token = create_test_token();

    assert_eq!(token.id, 1);
    assert_eq!(token.token, "TestToken123456789");
    assert_eq!(token.token_type, "single_use");
    assert_eq!(token.description, Some("Test token for unit tests".to_string()));
    assert_eq!(token.max_uses, 1);
    assert_eq!(token.uses_count, 0);
    assert!(!token.is_used);
    assert!(token.is_enabled);
    assert!(token.expires_at.is_none());
    assert_eq!(token.created_by, Some("@admin:example.com".to_string()));
}

#[test]
fn test_registration_token_optional_fields() {
    let token = RegistrationToken {
        id: 2,
        token: "MinimalToken".to_string(),
        token_type: "multi_use".to_string(),
        description: None,
        max_uses: 10,
        uses_count: 5,
        is_used: false,
        is_enabled: true,
        expires_at: Some(1800000000000),
        created_by: None,
        created_ts: 1700000000000,
        updated_ts: Some(1700000000000),
        last_used_ts: Some(1750000000000),
        allowed_email_domains: None,
        allowed_user_ids: None,
        auto_join_rooms: None,
        display_name: None,
        email: None,
    };

    assert!(token.description.is_none());
    assert!(token.created_by.is_none());
    assert!(token.allowed_email_domains.is_none());
    assert!(token.allowed_user_ids.is_none());
    assert!(token.auto_join_rooms.is_none());
    assert!(token.display_name.is_none());
    assert!(token.email.is_none());
    assert!(token.expires_at.is_some());
    assert!(token.last_used_ts.is_some());
}

#[test]
fn test_token_validation_result_valid() {
    let result = TokenValidationResult { is_valid: true, token_id: Some(1), error_message: None };

    assert!(result.is_valid);
    assert_eq!(result.token_id, Some(1));
    assert!(result.error_message.is_none());
}

#[test]
fn test_token_validation_result_invalid() {
    let result = TokenValidationResult {
        is_valid: false,
        token_id: Some(2),
        error_message: Some("Token has expired".to_string()),
    };

    assert!(!result.is_valid);
    assert_eq!(result.token_id, Some(2));
    assert_eq!(result.error_message, Some("Token has expired".to_string()));
}

#[test]
fn test_token_validation_result_not_found() {
    let result =
        TokenValidationResult { is_valid: false, token_id: None, error_message: Some("Token not found".to_string()) };

    assert!(!result.is_valid);
    assert!(result.token_id.is_none());
    assert_eq!(result.error_message, Some("Token not found".to_string()));
}

#[test]
fn test_create_registration_token_request() {
    let request = CreateRegistrationTokenRequest {
        token: Some("CustomToken123".to_string()),
        token_type: Some("multi_use".to_string()),
        description: Some("Custom token".to_string()),
        max_uses: Some(5),
        expires_at: Some(1800000000000),
        created_by: Some("@admin:example.com".to_string()),
        allowed_email_domains: Some(vec!["test.com".to_string()]),
        allowed_user_ids: Some(vec!["@user:test.com".to_string()]),
        auto_join_rooms: Some(vec!["!room:test.com".to_string()]),
        display_name: Some("Display Name".to_string()),
        email: Some("email@test.com".to_string()),
    };

    assert_eq!(request.token, Some("CustomToken123".to_string()));
    assert_eq!(request.token_type, Some("multi_use".to_string()));
    assert_eq!(request.max_uses, Some(5));
    assert!(request.expires_at.is_some());
}

#[test]
fn test_create_registration_token_request_minimal() {
    let request = CreateRegistrationTokenRequest {
        token: None,
        token_type: None,
        description: None,
        max_uses: None,
        expires_at: None,
        created_by: None,
        allowed_email_domains: None,
        allowed_user_ids: None,
        auto_join_rooms: None,
        display_name: None,
        email: None,
    };

    assert!(request.token.is_none());
    assert!(request.token_type.is_none());
    assert!(request.max_uses.is_none());
    assert!(request.expires_at.is_none());
}

#[test]
fn test_update_registration_token_request() {
    let request = UpdateRegistrationTokenRequest {
        description: Some("Updated description".to_string()),
        max_uses: Some(10),
        is_enabled: Some(false),
        expires_at: Some(1900000000000),
    };

    assert_eq!(request.description, Some("Updated description".to_string()));
    assert_eq!(request.max_uses, Some(10));
    assert_eq!(request.is_enabled, Some(false));
    assert!(request.expires_at.is_some());
}

#[test]
fn test_update_registration_token_request_default() {
    let request = UpdateRegistrationTokenRequest::default();

    assert!(request.description.is_none());
    assert!(request.max_uses.is_none());
    assert!(request.is_enabled.is_none());
    assert!(request.expires_at.is_none());
}

#[test]
fn test_generate_token_length() {
    let token = RegistrationTokenStorage::generate_token();
    assert_eq!(token.len(), 32);
}

#[test]
fn test_generate_token_uniqueness() {
    let token1 = RegistrationTokenStorage::generate_token();
    let token2 = RegistrationTokenStorage::generate_token();
    let token3 = RegistrationTokenStorage::generate_token();

    assert_ne!(token1, token2);
    assert_ne!(token2, token3);
    assert_ne!(token1, token3);
}

#[test]
fn test_generate_token_valid_characters() {
    let token = RegistrationTokenStorage::generate_token();
    let valid_chars: std::collections::HashSet<char> =
        "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789".chars().collect();

    for c in token.chars() {
        assert!(valid_chars.contains(&c), "Invalid character: {c}");
    }
}

#[test]
fn test_generate_token_no_ambiguous_chars() {
    let token = RegistrationTokenStorage::generate_token();
    let ambiguous_chars = ['I', 'O', 'l', 'i', 'o', '0', '1'];

    for c in token.chars() {
        assert!(!ambiguous_chars.contains(&c), "Ambiguous character found: {c}");
    }
}

#[test]
fn test_registration_token_expiry_logic() {
    let now = current_timestamp_millis();

    let valid_token = RegistrationToken {
        id: 1,
        token: "ValidToken".to_string(),
        token_type: "single_use".to_string(),
        description: None,
        max_uses: 1,
        uses_count: 0,
        is_used: false,
        is_enabled: true,
        expires_at: Some(now + 86_400_000),
        created_by: None,
        created_ts: now,
        updated_ts: Some(now),
        last_used_ts: None,
        allowed_email_domains: None,
        allowed_user_ids: None,
        auto_join_rooms: None,
        display_name: None,
        email: None,
    };

    let expired_token = RegistrationToken {
        id: 2,
        token: "ExpiredToken".to_string(),
        token_type: "single_use".to_string(),
        description: None,
        max_uses: 1,
        uses_count: 0,
        is_used: false,
        is_enabled: true,
        expires_at: Some(now - 86_400_000),
        created_by: None,
        created_ts: now - 172800000,
        updated_ts: Some(now - 172800000),
        last_used_ts: None,
        allowed_email_domains: None,
        allowed_user_ids: None,
        auto_join_rooms: None,
        display_name: None,
        email: None,
    };

    assert!(valid_token.expires_at.unwrap() > now);
    assert!(expired_token.expires_at.unwrap() < now);
}

#[test]
fn test_registration_token_usage_limit() {
    let unlimited_token = RegistrationToken {
        id: 1,
        token: "UnlimitedToken".to_string(),
        token_type: "multi_use".to_string(),
        description: None,
        max_uses: 0,
        uses_count: 100,
        is_used: false,
        is_enabled: true,
        expires_at: None,
        created_by: None,
        created_ts: 1700000000000,
        updated_ts: Some(1700000000000),
        last_used_ts: None,
        allowed_email_domains: None,
        allowed_user_ids: None,
        auto_join_rooms: None,
        display_name: None,
        email: None,
    };

    let limited_token_available = RegistrationToken {
        id: 2,
        token: "LimitedTokenAvailable".to_string(),
        token_type: "multi_use".to_string(),
        description: None,
        max_uses: 10,
        uses_count: 5,
        is_used: false,
        is_enabled: true,
        expires_at: None,
        created_by: None,
        created_ts: 1700000000000,
        updated_ts: Some(1700000000000),
        last_used_ts: None,
        allowed_email_domains: None,
        allowed_user_ids: None,
        auto_join_rooms: None,
        display_name: None,
        email: None,
    };

    let limited_token_exhausted = RegistrationToken {
        id: 3,
        token: "LimitedTokenExhausted".to_string(),
        token_type: "multi_use".to_string(),
        description: None,
        max_uses: 10,
        uses_count: 10,
        is_used: false,
        is_enabled: true,
        expires_at: None,
        created_by: None,
        created_ts: 1700000000000,
        updated_ts: Some(1700000000000),
        last_used_ts: None,
        allowed_email_domains: None,
        allowed_user_ids: None,
        auto_join_rooms: None,
        display_name: None,
        email: None,
    };

    assert!(unlimited_token.max_uses == 0 || unlimited_token.uses_count < unlimited_token.max_uses);
    assert!(limited_token_available.uses_count < limited_token_available.max_uses);
    assert!(limited_token_exhausted.uses_count >= limited_token_exhausted.max_uses);
}

#[test]
fn test_registration_token_disabled() {
    let disabled_token = RegistrationToken {
        id: 1,
        token: "DisabledToken".to_string(),
        token_type: "single_use".to_string(),
        description: None,
        max_uses: 1,
        uses_count: 0,
        is_used: false,
        is_enabled: false,
        expires_at: None,
        created_by: None,
        created_ts: 1700000000000,
        updated_ts: Some(1700000000000),
        last_used_ts: None,
        allowed_email_domains: None,
        allowed_user_ids: None,
        auto_join_rooms: None,
        display_name: None,
        email: None,
    };

    assert!(!disabled_token.is_enabled);
    assert!(!disabled_token.is_used);
}

#[test]
fn test_registration_token_single_use_used() {
    let used_token = RegistrationToken {
        id: 1,
        token: "UsedToken".to_string(),
        token_type: "single_use".to_string(),
        description: None,
        max_uses: 1,
        uses_count: 1,
        is_used: true,
        is_enabled: true,
        expires_at: None,
        created_by: None,
        created_ts: 1700000000000,
        updated_ts: Some(1700000000000),
        last_used_ts: Some(1700000100000),
        allowed_email_domains: None,
        allowed_user_ids: None,
        auto_join_rooms: None,
        display_name: None,
        email: None,
    };

    assert!(used_token.is_used);
    assert_eq!(used_token.token_type, "single_use");
    assert_eq!(used_token.uses_count, used_token.max_uses);
}

#[test]
fn test_room_invite_creation() {
    let invite = RoomInvite {
        id: 1,
        invite_code: "InviteCode123".to_string(),
        room_id: "!room:example.com".to_string(),
        inviter_user_id: "@admin:example.com".to_string(),
        invitee_email: Some("guest@example.com".to_string()),
        invitee_user_id: None,
        is_used: false,
        is_revoked: false,
        expires_at: Some(1800000000000),
        created_ts: 1700000000000,
        used_ts: None,
        revoked_at: None,
        revoked_reason: None,
    };

    assert_eq!(invite.invite_code, "InviteCode123");
    assert_eq!(invite.room_id, "!room:example.com");
    assert!(!invite.is_used);
    assert!(!invite.is_revoked);
    assert!(invite.invitee_user_id.is_none());
}

#[test]
fn test_room_invite_revoked() {
    let revoked_invite = RoomInvite {
        id: 1,
        invite_code: "RevokedCode".to_string(),
        room_id: "!room:example.com".to_string(),
        inviter_user_id: "@admin:example.com".to_string(),
        invitee_email: None,
        invitee_user_id: None,
        is_used: false,
        is_revoked: true,
        expires_at: None,
        created_ts: 1700000000000,
        used_ts: None,
        revoked_at: Some(1700000100000),
        revoked_reason: Some("No longer needed".to_string()),
    };

    assert!(revoked_invite.is_revoked);
    assert!(revoked_invite.revoked_at.is_some());
    assert_eq!(revoked_invite.revoked_reason, Some("No longer needed".to_string()));
}

#[test]
fn test_registration_token_batch() {
    let batch = RegistrationTokenBatch {
        id: 1,
        batch_id: "batch-uuid-123".to_string(),
        description: Some("Batch for testing".to_string()),
        token_count: 10,
        tokens_used: 3,
        created_by: Some("@admin:example.com".to_string()),
        created_ts: 1700000000000,
        expires_at: Some(1800000000000),
        is_enabled: true,
        allowed_email_domains: Some(vec!["test.com".to_string()]),
        auto_join_rooms: Some(vec!["!room:test.com".to_string()]),
    };

    assert_eq!(batch.batch_id, "batch-uuid-123");
    assert_eq!(batch.token_count, 10);
    assert_eq!(batch.tokens_used, 3);
    assert!(batch.is_enabled);
}

#[test]
fn test_registration_token_usage() {
    let usage = RegistrationTokenUsage {
        id: 1,
        token_id: Some(100),
        token: "UsedToken123".to_string(),
        user_id: "@user:example.com".to_string(),
        username: Some("testuser".to_string()),
        email: Some("user@example.com".to_string()),
        ip_address: Some("192.168.1.1".to_string()),
        user_agent: Some("Mozilla/5.0".to_string()),
        used_ts: 1700000000000,
        is_success: true,
        error_message: None,
    };

    assert_eq!(usage.token_id, Some(100));
    assert_eq!(usage.user_id, "@user:example.com");
    assert!(usage.is_success);
    assert!(usage.error_message.is_none());
}

#[test]
fn test_registration_token_usage_failed() {
    let failed_usage = RegistrationTokenUsage {
        id: 2,
        token_id: Some(100),
        token: "FailedToken".to_string(),
        user_id: "@user:example.com".to_string(),
        username: None,
        email: None,
        ip_address: Some("192.168.1.2".to_string()),
        user_agent: None,
        used_ts: 1700000000000,
        is_success: false,
        error_message: Some("Token expired".to_string()),
    };

    assert!(!failed_usage.is_success);
    assert_eq!(failed_usage.error_message, Some("Token expired".to_string()));
}

#[test]
fn test_create_room_invite_request() {
    let request = CreateRoomInviteRequest {
        room_id: "!room:example.com".to_string(),
        inviter_user_id: "@admin:example.com".to_string(),
        invitee_email: Some("guest@example.com".to_string()),
        expires_at: Some(1800000000000),
    };

    assert_eq!(request.room_id, "!room:example.com");
    assert_eq!(request.inviter_user_id, "@admin:example.com");
    assert!(request.invitee_email.is_some());
}

#[test]
fn test_token_serialization() {
    let token = create_test_token();
    let json = serde_json::to_string(&token).expect("Failed to serialize");
    let deserialized: RegistrationToken = serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(token.id, deserialized.id);
    assert_eq!(token.token, deserialized.token);
    assert_eq!(token.token_type, deserialized.token_type);
    assert_eq!(token.max_uses, deserialized.max_uses);
}

#[test]
fn test_token_validation_result_serialization() {
    let result = TokenValidationResult { is_valid: true, token_id: Some(42), error_message: None };

    let json = serde_json::to_string(&result).expect("Failed to serialize");
    let deserialized: TokenValidationResult = serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(result.is_valid, deserialized.is_valid);
    assert_eq!(result.token_id, deserialized.token_id);
}
