use super::*;

#[test]
fn test_room_member_struct() {
    let member = RoomMember {
        room_id: "!room123:example.com".to_string(),
        user_id: "@alice:example.com".to_string(),
        sender: Some("@alice:example.com".to_string()),
        membership: "join".to_string(),
        event_id: Some("$event123:example.com".to_string()),
        event_type: Some("m.room.member".to_string()),
        display_name: Some("Alice".to_string()),
        avatar_url: Some("mxc://example.com/avatar".to_string()),
        is_banned: Some(false),
        invite_token: None,
        updated_ts: Some(1234567890),
        joined_ts: Some(1234567890),
        left_ts: None,
        reason: None,
        banned_by: None,
        ban_reason: None,
        banned_ts: None,
        join_reason: None,
    };

    assert_eq!(member.room_id, "!room123:example.com");
    assert_eq!(member.user_id, "@alice:example.com");
    assert_eq!(member.membership, "join");
    assert!(member.is_banned.is_some());
    assert!(!member.is_banned.unwrap());
}

#[test]
fn test_membership_types() {
    let join_membership = "join";
    let leave_membership = "leave";
    let invite_membership = "invite";
    let ban_membership = "ban";
    let forget_membership = "forget";

    assert_eq!(join_membership, "join");
    assert_eq!(leave_membership, "leave");
    assert_eq!(invite_membership, "invite");
    assert_eq!(ban_membership, "ban");
    assert_eq!(forget_membership, "forget");
}

#[test]
fn test_room_member_banned() {
    let banned_member = RoomMember {
        room_id: "!room:example.com".to_string(),
        user_id: "@bob:example.com".to_string(),
        sender: Some("@admin:example.com".to_string()),
        membership: "ban".to_string(),
        event_id: Some("$ban_event:example.com".to_string()),
        event_type: Some("m.room.member".to_string()),
        display_name: None,
        avatar_url: None,
        is_banned: Some(true),
        invite_token: None,
        updated_ts: Some(1234567890),
        joined_ts: None,
        left_ts: Some(1234567890),
        reason: Some("Spam".to_string()),
        banned_by: Some("@admin:example.com".to_string()),
        ban_reason: Some("Spam behavior".to_string()),
        banned_ts: Some(1234567890),
        join_reason: None,
    };

    assert_eq!(banned_member.membership, "ban");
    assert!(banned_member.is_banned.unwrap_or(false));
    assert!(banned_member.banned_by.is_some());
    assert!(banned_member.ban_reason.is_some());
}

#[test]
fn test_room_member_invited() {
    let invited_member = RoomMember {
        room_id: "!room:example.com".to_string(),
        user_id: "@charlie:example.com".to_string(),
        sender: Some("@alice:example.com".to_string()),
        membership: "invite".to_string(),
        event_id: Some("$invite_event:example.com".to_string()),
        event_type: Some("m.room.member".to_string()),
        display_name: Some("Charlie".to_string()),
        avatar_url: None,
        is_banned: Some(false),
        invite_token: Some("token123".to_string()),
        updated_ts: Some(1234567890),
        joined_ts: None,
        left_ts: None,
        reason: None,
        banned_by: None,
        ban_reason: None,
        banned_ts: None,
        join_reason: None,
    };

    assert_eq!(invited_member.membership, "invite");
    assert!(invited_member.invite_token.is_some());
    assert!(invited_member.joined_ts.is_none());
}

#[test]
fn test_room_member_left() {
    let left_member = RoomMember {
        room_id: "!room:example.com".to_string(),
        user_id: "@dave:example.com".to_string(),
        sender: Some("@dave:example.com".to_string()),
        membership: "leave".to_string(),
        event_id: Some("$leave_event:example.com".to_string()),
        event_type: Some("m.room.member".to_string()),
        display_name: Some("Dave".to_string()),
        avatar_url: None,
        is_banned: Some(false),
        invite_token: None,
        updated_ts: Some(1234567900),
        joined_ts: Some(1234567800),
        left_ts: Some(1234567900),
        reason: Some("Leaving room".to_string()),
        banned_by: None,
        ban_reason: None,
        banned_ts: None,
        join_reason: None,
    };

    assert_eq!(left_member.membership, "leave");
    assert!(left_member.left_ts.is_some());
    assert!(left_member.joined_ts.is_some());
}

#[test]
fn test_room_member_serialization() {
    let member = RoomMember {
        room_id: "!room:example.com".to_string(),
        user_id: "@user:example.com".to_string(),
        sender: Some("@user:example.com".to_string()),
        membership: "join".to_string(),
        event_id: Some("$event:example.com".to_string()),
        event_type: Some("m.room.member".to_string()),
        display_name: Some("User".to_string()),
        avatar_url: Some("mxc://example.com/avatar".to_string()),
        is_banned: Some(false),
        invite_token: None,
        updated_ts: Some(1234567890),
        joined_ts: Some(1234567890),
        left_ts: None,
        reason: None,
        banned_by: None,
        ban_reason: None,
        banned_ts: None,
        join_reason: None,
    };

    let json = serde_json::to_string(&member).unwrap();
    assert!(json.contains("join"));
    assert!(json.contains("@user:example.com"));
    assert!(json.contains("!room:example.com"));
}

#[test]
fn test_room_member_storage_creation() {
    let member = RoomMember {
        room_id: "!test:example.com".to_string(),
        user_id: "@test:example.com".to_string(),
        sender: None,
        membership: "join".to_string(),
        event_id: None,
        event_type: None,
        display_name: None,
        avatar_url: None,
        is_banned: None,
        invite_token: None,
        updated_ts: None,
        joined_ts: None,
        left_ts: None,
        reason: None,
        banned_by: None,
        ban_reason: None,
        banned_ts: None,
        join_reason: None,
    };

    assert_eq!(member.membership, "join");
    assert!(member.sender.is_none());
    assert!(member.event_id.is_none());
}
