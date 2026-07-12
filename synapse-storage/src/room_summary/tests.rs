use super::*;

#[test]
fn test_room_summary_creation() {
    let summary = RoomSummary {
        id: Some(1),
        room_id: "!room:example.com".to_string(),
        room_type: None,
        name: Some("Test Room".to_string()),
        topic: Some("A test room".to_string()),
        avatar_url: Some("mxc://avatar".to_string()),
        canonical_alias: None,
        join_rule: "public".to_string(),
        history_visibility: "shared".to_string(),
        guest_access: "forbidden".to_string(),
        is_direct: false,
        is_space: false,
        is_encrypted: false,
        member_count: Some(10),
        joined_member_count: Some(8),
        invited_member_count: Some(2),
        hero_users: serde_json::json!([]),
        last_event_id: None,
        last_event_ts: None,
        last_message_ts: None,
        unread_notifications: 0,
        unread_highlight: 0,
        updated_ts: Some(1234567890),
        created_ts: Some(1234567800),
    };
    assert_eq!(summary.room_id, "!room:example.com");
    assert!(summary.name.is_some());
}

#[test]
fn test_room_summary_member_creation() {
    let member = RoomSummaryMember {
        id: 1,
        room_id: "!room:example.com".to_string(),
        user_id: "@alice:example.com".to_string(),
        display_name: Some("Alice".to_string()),
        avatar_url: Some("mxc://alice".to_string()),
        membership: "join".to_string(),
        is_hero: true,
        last_active_ts: Some(1234567890),
        updated_ts: 1234567890,
        created_ts: 1234567800,
    };
    assert_eq!(member.user_id, "@alice:example.com");
    assert!(member.is_hero);
}

#[test]
fn test_room_summary_state_creation() {
    let state = RoomSummaryState {
        id: 1,
        room_id: "!room:example.com".to_string(),
        event_type: "m.room.create".to_string(),
        state_key: "".to_string(),
        event_id: None,
        content: serde_json::json!({"creator": "@admin:example.com"}),
        updated_ts: 1234567890,
    };
    assert_eq!(state.room_id, "!room:example.com");
}

#[test]
fn test_room_summary_stats_creation() {
    let stats = RoomSummaryStats {
        id: 1,
        room_id: "!room:example.com".to_string(),
        total_events: 100,
        total_state_events: 20,
        total_messages: 100,
        total_media: 10,
        storage_size: 1048576,
        last_updated_ts: 1234567890,
    };
    assert_eq!(stats.total_messages, 100);
}

#[test]
fn test_create_room_summary_request() {
    let request = CreateRoomSummaryRequest {
        room_id: "!room:example.com".to_string(),
        room_type: None,
        name: Some("New Room".to_string()),
        topic: Some("Topic".to_string()),
        avatar_url: None,
        canonical_alias: None,
        join_rule: Some("public".to_string()),
        history_visibility: Some("shared".to_string()),
        guest_access: Some("forbidden".to_string()),
        is_direct: Some(false),
        is_space: Some(false),
    };
    assert_eq!(request.room_id, "!room:example.com");
}

#[test]
fn test_update_room_summary_request() {
    let request = UpdateRoomSummaryRequest {
        name: Some("Updated Name".to_string()),
        topic: None,
        avatar_url: Some("mxc://new".to_string()),
        canonical_alias: None,
        join_rule: None,
        history_visibility: None,
        guest_access: None,
        is_direct: None,
        is_space: None,
        is_encrypted: None,
        last_event_id: None,
        last_event_ts: None,
        last_message_ts: None,
        hero_users: None,
    };
    assert!(request.name.is_some());
}

#[test]
fn test_hero_users_json() {
    let heroes = vec![
        RoomSummaryHero {
            user_id: "@alice:example.com".to_string(),
            display_name: Some("Alice".to_string()),
            avatar_url: None,
        },
        RoomSummaryHero {
            user_id: "@bob:example.com".to_string(),
            display_name: Some("Bob".to_string()),
            avatar_url: Some("mxc://bob".to_string()),
        },
    ];
    let json = serde_json::to_string(&heroes).unwrap();
    assert!(json.contains("@alice:example.com"));
}

#[test]
fn test_room_summary_optional_fields() {
    let summary = RoomSummary {
        id: Some(2),
        room_id: "!room2:example.com".to_string(),
        room_type: None,
        name: None,
        topic: None,
        avatar_url: None,
        canonical_alias: None,
        join_rule: "public".to_string(),
        history_visibility: "shared".to_string(),
        guest_access: "forbidden".to_string(),
        is_direct: false,
        is_space: false,
        is_encrypted: false,
        member_count: Some(0),
        joined_member_count: Some(0),
        invited_member_count: Some(0),
        hero_users: serde_json::json!([]),
        last_event_id: None,
        last_event_ts: None,
        last_message_ts: None,
        unread_notifications: 0,
        unread_highlight: 0,
        updated_ts: Some(0),
        created_ts: Some(0),
    };
    assert!(summary.name.is_none());
    assert!(summary.topic.is_none());
}

// ── to_response tests ──

#[test]
fn test_to_response_empty_heroes() {
    let summary = RoomSummary {
        id: Some(1),
        room_id: "!room:example.com".to_string(),
        room_type: None,
        name: Some("Test Room".to_string()),
        topic: Some("A test room".to_string()),
        avatar_url: Some("mxc://avatar".to_string()),
        canonical_alias: None,
        join_rule: "public".to_string(),
        history_visibility: "shared".to_string(),
        guest_access: "forbidden".to_string(),
        is_direct: false,
        is_space: false,
        is_encrypted: true,
        member_count: Some(5),
        joined_member_count: Some(3),
        invited_member_count: Some(2),
        hero_users: serde_json::json!([]),
        last_event_id: None,
        last_event_ts: Some(1_700_000_000_000i64),
        last_message_ts: Some(1_700_000_000_001i64),
        unread_notifications: 0,
        unread_highlight: 0,
        updated_ts: Some(1_700_000_000_000i64),
        created_ts: Some(1_700_000_000_000i64),
    };

    let response = summary.to_response(vec![]);

    assert_eq!(response.room_id, "!room:example.com");
    assert_eq!(response.name, Some("Test Room".to_string()));
    assert_eq!(response.is_encrypted, true);
    assert_eq!(response.member_count, 5);
    assert!(response.heroes.is_empty());
    assert_eq!(response.last_event_ts, Some(1_700_000_000_000i64));
    assert_eq!(response.last_message_ts, Some(1_700_000_000_001i64));
    assert!(response.room_type.is_none());
}

#[test]
fn test_to_response_with_heroes() {
    let summary = RoomSummary {
        id: Some(1),
        room_id: "!room:example.com".to_string(),
        room_type: None,
        name: Some("Test Room".to_string()),
        topic: None,
        avatar_url: None,
        canonical_alias: None,
        join_rule: "public".to_string(),
        history_visibility: "shared".to_string(),
        guest_access: "forbidden".to_string(),
        is_direct: true,
        is_space: false,
        is_encrypted: false,
        member_count: Some(3),
        joined_member_count: Some(3),
        invited_member_count: Some(0),
        hero_users: serde_json::json!([]),
        last_event_id: None,
        last_event_ts: None,
        last_message_ts: None,
        unread_notifications: 0,
        unread_highlight: 0,
        updated_ts: None,
        created_ts: None,
    };

    let heroes = vec![
        RoomSummaryHero {
            user_id: "@alice:example.com".to_string(),
            display_name: Some("Alice".to_string()),
            avatar_url: Some("mxc://alice".to_string()),
        },
        RoomSummaryHero {
            user_id: "@bob:example.com".to_string(),
            display_name: Some("Bob".to_string()),
            avatar_url: None,
        },
    ];

    let response = summary.to_response(heroes);

    assert_eq!(response.room_id, "!room:example.com");
    assert_eq!(response.is_direct, true);
    assert_eq!(response.heroes.len(), 2);
    assert_eq!(response.heroes[0].user_id, "@alice:example.com");
    assert_eq!(response.heroes[0].display_name, Some("Alice".to_string()));
    assert_eq!(response.heroes[0].avatar_url, Some("mxc://alice".to_string()));
    assert_eq!(response.heroes[1].user_id, "@bob:example.com");
    assert_eq!(response.heroes[1].display_name, Some("Bob".to_string()));
    assert!(response.heroes[1].avatar_url.is_none());
    assert!(response.last_event_ts.is_none());
    assert!(response.topic.is_none());
}

#[test]
fn test_to_response_preserves_all_fields() {
    let summary = RoomSummary {
        id: Some(1),
        room_id: "!room:example.com".to_string(),
        room_type: Some("m.space".to_string()),
        name: Some("Space".to_string()),
        topic: Some("A space room".to_string()),
        avatar_url: Some("mxc://space_avatar".to_string()),
        canonical_alias: Some("#space:example.com".to_string()),
        join_rule: "knock".to_string(),
        history_visibility: "world_readable".to_string(),
        guest_access: "can_join".to_string(),
        is_direct: false,
        is_space: true,
        is_encrypted: false,
        member_count: Some(10),
        joined_member_count: Some(8),
        invited_member_count: Some(2),
        hero_users: serde_json::json!([]),
        last_event_id: None,
        last_event_ts: Some(1_700_000_000_000i64),
        last_message_ts: Some(1_700_000_000_001i64),
        unread_notifications: 0,
        unread_highlight: 0,
        updated_ts: Some(1_700_000_000_000i64),
        created_ts: Some(1_700_000_000_000i64),
    };

    let hero = RoomSummaryHero {
        user_id: "@admin:example.com".to_string(),
        display_name: Some("Admin".to_string()),
        avatar_url: Some("mxc://admin".to_string()),
    };

    let response = summary.to_response(vec![hero]);

    assert_eq!(response.room_id, "!room:example.com");
    assert_eq!(response.room_type, Some("m.space".to_string()));
    assert_eq!(response.name, Some("Space".to_string()));
    assert_eq!(response.topic, Some("A space room".to_string()));
    assert_eq!(response.avatar_url, Some("mxc://space_avatar".to_string()));
    assert_eq!(response.canonical_alias, Some("#space:example.com".to_string()));
    assert_eq!(response.join_rule, "knock");
    assert_eq!(response.history_visibility, "world_readable");
    assert_eq!(response.guest_access, "can_join");
    assert_eq!(response.is_space, true);
    assert_eq!(response.is_direct, false);
    assert_eq!(response.is_encrypted, false);
    assert_eq!(response.member_count, 10);
    assert_eq!(response.joined_member_count, 8);
    assert_eq!(response.invited_member_count, 2);
    assert_eq!(response.heroes.len(), 1);
    assert_eq!(response.heroes[0].user_id, "@admin:example.com");
    assert_eq!(response.last_event_ts, Some(1_700_000_000_000i64));
    assert_eq!(response.last_message_ts, Some(1_700_000_000_001i64));
}

// ── RoomSummaryMember → RoomSummaryHero conversion tests ──

#[test]
fn test_member_to_hero_conversion_all_fields() {
    let member = RoomSummaryMember {
        id: 1,
        room_id: "!room:example.com".to_string(),
        user_id: "@alice:example.com".to_string(),
        display_name: Some("Alice".to_string()),
        avatar_url: Some("mxc://alice".to_string()),
        membership: "join".to_string(),
        is_hero: true,
        last_active_ts: Some(1_700_000_000_000i64),
        updated_ts: 1_700_000_000_000i64,
        created_ts: 1_700_000_000_000i64,
    };

    let hero: RoomSummaryHero = member.into();

    assert_eq!(hero.user_id, "@alice:example.com");
    assert_eq!(hero.display_name, Some("Alice".to_string()));
    assert_eq!(hero.avatar_url, Some("mxc://alice".to_string()));
}

#[test]
fn test_member_to_hero_conversion_optional_none() {
    let member = RoomSummaryMember {
        id: 2,
        room_id: "!room:example.com".to_string(),
        user_id: "@bob:example.com".to_string(),
        display_name: None,
        avatar_url: None,
        membership: "leave".to_string(),
        is_hero: false,
        last_active_ts: None,
        updated_ts: 1_700_000_000_000i64,
        created_ts: 1_700_000_000_000i64,
    };

    let hero: RoomSummaryHero = RoomSummaryHero::from(member);

    assert_eq!(hero.user_id, "@bob:example.com");
    assert!(hero.display_name.is_none());
    assert!(hero.avatar_url.is_none());
}
