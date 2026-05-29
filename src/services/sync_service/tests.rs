use super::types::*;
use super::SyncService;
use crate::services::*;
use crate::storage::UserRoomMembership;
use serde_json::json;
use std::collections::{HashMap, HashSet};

#[test]
fn test_sync_token_parse() {
    let token = SyncToken::parse("s1234567890");
    assert!(token.is_some());
    let token = token.unwrap();
    assert_eq!(token.stream_id, 1234567890);
}

#[test]
fn test_sync_token_encode() {
    let token = SyncToken {
        stream_id: 1234567890,
        room_id: None,
        event_type: None,
        to_device_stream_id: None,
        device_list_stream_id: None,
    };
    assert_eq!(token.encode(), "s1234567890");
}

#[test]
fn test_sync_token_roundtrip() {
    let original = SyncToken {
        stream_id: 9876543210,
        room_id: None,
        event_type: None,
        to_device_stream_id: None,
        device_list_stream_id: None,
    };
    let encoded = original.encode();
    let parsed = SyncToken::parse(&encoded).unwrap();
    assert_eq!(original.stream_id, parsed.stream_id);
}

#[test]
fn test_sync_token_multistream_roundtrip() {
    let original = SyncToken {
        stream_id: 1_777_000_000_000,
        room_id: None,
        event_type: None,
        to_device_stream_id: Some(4321),
        device_list_stream_id: Some(9876),
    };
    let encoded = original.encode();
    assert_eq!(encoded, "s1777000000000_4321_9876");
    let parsed = SyncToken::parse(&encoded).unwrap();
    assert_eq!(parsed.stream_id, original.stream_id);
    assert_eq!(parsed.to_device_stream_id, original.to_device_stream_id);
    assert_eq!(parsed.device_list_stream_id, original.device_list_stream_id);
}

#[test]
fn test_sync_filter_default() {
    let filter = SyncFilter::default();
    assert_eq!(filter.limit, Some(100));
    assert!(filter.types.is_none());
    assert!(filter.rooms.is_none());
}

#[test]
fn test_room_filter_default() {
    let filter = RoomFilter::default();
    assert_eq!(filter.include_leave, Some(false));
    assert!(filter.state.is_some());
    assert!(filter.timeline.is_some());
    assert_eq!(filter.timeline.unwrap().limit, Some(50));
}

#[test]
fn test_sync_response_format() {
    let response = json!({
        "next_batch": "s1234567890",
        "rooms": {
            "join": {},
            "invite": {},
            "leave": {}
        },
        "presence": json!({
            "events": []
        }),
        "account_data": json!({
            "events": []
        }),
        "to_device": json!({
            "events": []
        }),
        "device_lists": {
            "changed": [],
            "left": []
        }
    });

    assert!(response.get("next_batch").is_some());
    assert!(response["rooms"]["join"].is_object());
    assert!(response["presence"]["events"].is_array());
    assert!(response["device_lists"]["changed"].is_array());
}

#[test]
fn test_room_timeline_format() {
    let timeline = json!({
        "events": [],
        "limited": true,
        "prev_batch": "t1234567890"
    });

    assert!(timeline["events"].is_array());
    assert!(timeline["limited"].is_boolean());
    assert_eq!(timeline["prev_batch"], "t1234567890");
}

#[test]
fn test_room_state_format() {
    let state = json!({
        "events": []
    });
    assert!(state["events"].is_array());
}

#[test]
fn test_presence_format() {
    let presence = json!({
        "events": []
    });
    assert!(presence["events"].is_array());
}

#[test]
fn test_account_data_format() {
    let account_data = json!({
        "events": []
    });
    assert!(account_data["events"].is_array());
}

#[test]
fn test_to_device_format() {
    let to_device = json!({
        "events": []
    });
    assert!(to_device["events"].is_array());
}

#[test]
fn test_device_lists_format() {
    let device_lists = json!({
        "changed": ["@user1:example.com"],
        "left": ["@user2:example.com"]
    });

    assert!(device_lists["changed"].is_array());
    assert!(device_lists["left"].is_array());
}

#[test]
fn test_unread_notifications_format() {
    let notifications = json!({
        "highlight_count": 0,
        "notification_count": 0
    });

    assert_eq!(notifications["highlight_count"], 0);
    assert_eq!(notifications["notification_count"], 0);
}

#[test]
fn test_ephemeral_format() {
    let ephemeral = json!({
        "events": []
    });
    assert!(ephemeral["events"].is_array());
}

#[test]
fn test_room_messages_response_format() {
    let response = json!({
        "chunk": [],
        "start": "t1234567890",
        "end": "t1234567899"
    });

    assert!(response.get("chunk").is_some());
    assert!(response.get("start").is_some());
    assert!(response.get("end").is_some());
}

#[test]
fn test_public_rooms_response_format() {
    let response = json!({
        "chunk": [],
        "total_room_count_estimate": 0,
        "next_batch": "p1234567890"
    });

    assert!(response.get("chunk").is_some());
    assert!(response.get("total_room_count_estimate").is_some());
    assert!(response.get("next_batch").is_some());
}

#[test]
fn test_count_events_by_room() {
    let room_events = HashMap::from([
        ("!a:example.com".to_string(), vec![sample_room_event("1"), sample_room_event("2")]),
        ("!b:example.com".to_string(), vec![sample_room_event("3")]),
    ]);

    assert_eq!(SyncService::count_events_by_room(&room_events), 3);
}

#[test]
fn test_timeline_limit_from_room_filter() {
    let filter = json!({
        "room": {
            "timeline": {
                "limit": 8
            }
        }
    });
    let room_filter = SyncService::room_filter_from_filter_json(&filter);

    assert_eq!(SyncService::timeline_limit_from_room_filter(Some(&room_filter), 50), 8);
}

#[test]
fn test_event_query_filter_from_sync_filter_ignores_limit_only_filter() {
    let filter = SyncFilter {
        limit: Some(3),
        types: None,
        not_types: None,
        rooms: None,
        not_rooms: None,
        contains_url: None,
        lazy_load_members: None,
        include_redundant_members: None,
        senders: None,
        not_senders: None,
    };

    assert!(SyncService::event_query_filter_from_sync_filter(Some(&filter)).is_none());
}

#[test]
fn test_event_query_filter_from_sync_filter_preserves_matchers() {
    let filter = SyncFilter {
        limit: Some(5),
        types: Some(vec!["m.room.message".to_string()]),
        not_types: Some(vec!["m.room.redaction".to_string()]),
        rooms: None,
        not_rooms: None,
        contains_url: None,
        lazy_load_members: None,
        include_redundant_members: None,
        senders: Some(vec!["@alice:localhost".to_string()]),
        not_senders: Some(vec!["@mallory:localhost".to_string()]),
    };

    let query_filter = SyncService::event_query_filter_from_sync_filter(Some(&filter))
        .expect("timeline matcher filter should be pushed to query layer");

    assert_eq!(query_filter.types, Some(vec!["m.room.message".to_string()]));
    assert_eq!(query_filter.not_types, Some(vec!["m.room.redaction".to_string()]));
    assert_eq!(query_filter.senders, Some(vec!["@alice:localhost".to_string()]));
    assert_eq!(query_filter.not_senders, Some(vec!["@mallory:localhost".to_string()]));
}

#[test]
fn test_timeline_limit_from_room_filter_ignores_missing_limit() {
    let filter = json!({
        "room": {
            "timeline": {}
        }
    });
    let room_filter = SyncService::room_filter_from_filter_json(&filter);

    assert_eq!(SyncService::timeline_limit_from_room_filter(Some(&room_filter), 50), 50);
}

#[test]
fn test_sync_filter_from_json_parses_matchers() {
    let filter = json!({
        "limit": 12,
        "types": ["m.room.member"],
        "not_types": ["m.room.redaction"],
        "rooms": ["!room:localhost"],
        "not_rooms": ["!blocked:localhost"],
        "contains_url": true,
        "lazy_load_members": true,
        "include_redundant_members": true,
        "senders": ["@alice:localhost"],
        "not_senders": ["@mallory:localhost"]
    });

    let parsed = SyncService::sync_filter_from_json(Some(&filter)).unwrap();

    assert_eq!(parsed.limit, Some(12));
    assert_eq!(parsed.types, Some(vec!["m.room.member".to_string()]));
    assert_eq!(parsed.not_types, Some(vec!["m.room.redaction".to_string()]));
    assert_eq!(parsed.rooms, Some(vec!["!room:localhost".to_string()]));
    assert_eq!(parsed.not_rooms, Some(vec!["!blocked:localhost".to_string()]));
    assert_eq!(parsed.contains_url, Some(true));
    assert_eq!(parsed.lazy_load_members, Some(true));
    assert_eq!(parsed.include_redundant_members, Some(true));
    assert_eq!(parsed.senders, Some(vec!["@alice:localhost".to_string()]));
    assert_eq!(parsed.not_senders, Some(vec!["@mallory:localhost".to_string()]));
}

#[test]
fn test_room_filter_from_filter_json_parses_sections() {
    let filter = json!({
        "room": {
            "rooms": ["!allowed:localhost"],
            "not_rooms": ["!blocked:localhost"],
            "include_leave": true,
            "state": {
                "types": ["m.room.name"]
            },
            "timeline": {
                "limit": 5
            },
            "ephemeral": {
                "senders": ["@alice:localhost"]
            },
            "account_data": {
                "not_types": ["m.tag"]
            }
        }
    });

    let parsed = SyncService::room_filter_from_filter_json(&filter);

    assert_eq!(parsed.rooms, Some(vec!["!allowed:localhost".to_string()]));
    assert_eq!(parsed.not_rooms, Some(vec!["!blocked:localhost".to_string()]));
    assert_eq!(parsed.include_leave, Some(true));
    assert_eq!(parsed.state.and_then(|state| state.types), Some(vec!["m.room.name".to_string()]));
    assert_eq!(parsed.timeline.and_then(|timeline| timeline.limit), Some(5));
    assert_eq!(parsed.ephemeral.and_then(|ephemeral| ephemeral.senders), Some(vec!["@alice:localhost".to_string()]));
    assert_eq!(parsed.account_data.and_then(|account_data| account_data.not_types), Some(vec!["m.tag".to_string()]));
}

#[test]
fn test_sync_response_filter_from_filter_json_parses_presence() {
    let filter = json!({
        "event_fields": ["type", "content.body", "unsigned.age"],
        "event_format": "federation",
        "presence": {
            "types": ["m.presence"],
            "not_senders": ["@mallory:localhost"]
        },
        "room": {
            "timeline": {
                "limit": 7
            }
        }
    });

    let parsed = SyncService::sync_response_filter_from_filter_json(&filter);

    assert_eq!(
        parsed.event_fields,
        Some(vec!["type".to_string(), "content.body".to_string(), "unsigned.age".to_string()])
    );
    assert_eq!(parsed.event_format, SyncEventFormat::Federation);
    assert_eq!(
        parsed.presence.as_ref().and_then(|presence| presence.types.as_ref()),
        Some(&vec!["m.presence".to_string()])
    );
    assert_eq!(
        parsed.presence.as_ref().and_then(|presence| presence.not_senders.as_ref()),
        Some(&vec!["@mallory:localhost".to_string()])
    );
    assert_eq!(
        parsed.room.as_ref().and_then(|room| room.timeline.as_ref()).and_then(|timeline| timeline.limit),
        Some(7)
    );
}

#[test]
fn test_apply_sync_filter_to_values_filters_types_and_senders() {
    let events = vec![
        json!({
            "type": "m.receipt",
            "sender": "@alice:localhost",
            "content": {}
        }),
        json!({
            "type": "m.typing",
            "sender": "@bob:localhost",
            "content": {}
        }),
        json!({
            "type": "m.receipt",
            "sender": "@mallory:localhost",
            "content": {}
        }),
    ];
    let filter = SyncFilter {
        limit: None,
        types: Some(vec!["m.receipt".to_string()]),
        not_types: None,
        rooms: None,
        not_rooms: None,
        contains_url: None,
        lazy_load_members: None,
        include_redundant_members: None,
        senders: None,
        not_senders: Some(vec!["@mallory:localhost".to_string()]),
    };

    let filtered = SyncService::apply_sync_filter_to_values(events, Some(&filter));

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0]["type"], "m.receipt");
    assert_eq!(filtered[0]["sender"], "@alice:localhost");
}

#[test]
fn test_apply_sync_filter_to_values_filters_presence_events() {
    let events = vec![
        json!({
            "type": "m.presence",
            "sender": "@alice:localhost",
            "content": { "presence": "online" }
        }),
        json!({
            "type": "m.presence",
            "sender": "@mallory:localhost",
            "content": { "presence": "online" }
        }),
    ];
    let filter = SyncFilter {
        limit: None,
        types: Some(vec!["m.presence".to_string()]),
        not_types: None,
        rooms: None,
        not_rooms: None,
        contains_url: None,
        lazy_load_members: None,
        include_redundant_members: None,
        senders: Some(vec!["@alice:localhost".to_string()]),
        not_senders: None,
    };

    let filtered = SyncService::apply_sync_filter_to_values(events, Some(&filter));

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0]["type"], "m.presence");
    assert_eq!(filtered[0]["sender"], "@alice:localhost");
}

#[test]
fn test_apply_timeline_limit_truncates_events() {
    let (events, limited) =
        SyncService::apply_timeline_limit(&[sample_room_event("1"), sample_room_event("2"), sample_room_event("3")], 2);

    assert!(limited);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_id, "$event2");
    assert_eq!(events[1].event_id, "$event1");
}

#[test]
fn test_slow_request_threshold() {
    assert!(SyncService::is_slow_request_for(750.0, 750));
    assert!(!SyncService::is_slow_request_for(749.0, 750));
}

#[test]
fn test_apply_sync_filter_to_values_filters_rooms_and_wildcard_types() {
    let events = vec![
        json!({
            "room_id": "!allowed:localhost",
            "type": "m.room.message",
            "sender": "@alice:localhost",
            "content": {}
        }),
        json!({
            "room_id": "!blocked:localhost",
            "type": "m.room.member",
            "sender": "@alice:localhost",
            "content": {}
        }),
    ];
    let filter = SyncFilter {
        limit: None,
        types: Some(vec!["m.room.*".to_string()]),
        not_types: Some(vec!["m.room.member".to_string()]),
        rooms: Some(vec!["!allowed:localhost".to_string()]),
        not_rooms: Some(vec!["!blocked:localhost".to_string()]),
        contains_url: None,
        lazy_load_members: None,
        include_redundant_members: None,
        senders: None,
        not_senders: None,
    };

    let filtered = SyncService::apply_sync_filter_to_values(events, Some(&filter));

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0]["room_id"], "!allowed:localhost");
    assert_eq!(filtered[0]["type"], "m.room.message");
}

#[test]
fn test_filter_event_fields_keeps_nested_fields() {
    let event = json!({
        "type": "m.room.message",
        "content": {
            "body": "hello",
            "msgtype": "m.text"
        },
        "unsigned": {
            "age": 12,
            "transaction_id": "t1"
        },
        "sender": "@alice:localhost"
    });

    let filtered = SyncService::filter_event_fields(
        event,
        Some(&["type".to_string(), "content.body".to_string(), "unsigned.age".to_string()]),
    );

    assert_eq!(filtered["type"], "m.room.message");
    assert_eq!(filtered["content"]["body"], "hello");
    assert!(filtered["content"].get("msgtype").is_none());
    assert_eq!(filtered["unsigned"]["age"], 12);
    assert!(filtered["unsigned"].get("transaction_id").is_none());
    assert!(filtered.get("sender").is_none());
}

#[test]
fn test_apply_sync_filter_to_values_filters_contains_url() {
    let events = vec![
        json!({
            "type": "m.room.message",
            "sender": "@alice:localhost",
            "content": { "body": "file", "url": "mxc://example.com/file" }
        }),
        json!({
            "type": "m.room.message",
            "sender": "@alice:localhost",
            "content": { "body": "plain text" }
        }),
    ];
    let filter = SyncFilter {
        limit: None,
        types: Some(vec!["m.room.message".to_string()]),
        not_types: None,
        rooms: None,
        not_rooms: None,
        contains_url: Some(true),
        lazy_load_members: None,
        include_redundant_members: None,
        senders: None,
        not_senders: None,
    };

    let filtered = SyncService::apply_sync_filter_to_values(events, Some(&filter));

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0]["content"]["url"], "mxc://example.com/file");
}

#[test]
fn test_apply_lazy_load_members_keeps_only_timeline_members_and_self() {
    let state_events = vec![
        json!({
            "type": "m.room.member",
            "state_key": "@alice:localhost",
            "content": { "membership": "join" }
        }),
        json!({
            "type": "m.room.member",
            "state_key": "@bob:localhost",
            "content": { "membership": "join" }
        }),
        json!({
            "type": "m.room.member",
            "state_key": "@carol:localhost",
            "content": { "membership": "join" }
        }),
        json!({
            "type": "m.room.name",
            "state_key": "",
            "content": { "name": "Test Room" }
        }),
    ];
    let timeline_events = vec![RoomEvent {
        event_id: "$event".to_string(),
        room_id: "!room:localhost".to_string(),
        user_id: "@bob:localhost".to_string(),
        event_type: "m.room.message".to_string(),
        content: json!({ "body": "hello" }),
        state_key: None,
        depth: 1,
        origin_server_ts: 1,
        processed_ts: 1,
        not_before: 0,
        status: None,
        reference_image: None,
        origin: "self".to_string(),
        stream_ordering: Some(1),
    }];

    let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &timeline_events,
        "@alice:localhost",
        &HashSet::new(),
        false,
        &HashSet::new(),
        false,
    );

    assert_eq!(filtered.len(), 3);
    assert!(filtered.iter().any(|event| event["state_key"] == "@alice:localhost"));
    assert!(filtered.iter().any(|event| event["state_key"] == "@bob:localhost"));
    assert!(filtered.iter().any(|event| event["type"] == "m.room.name"));
    assert!(!filtered.iter().any(|event| event["state_key"] == "@carol:localhost"));
    assert!(known_now.contains("@bob:localhost"));
    assert!(known_now.contains("@alice:localhost"));
}

#[test]
fn test_apply_lazy_load_members_skips_cached_members_by_default() {
    let state_events = vec![
        json!({
            "type": "m.room.member",
            "state_key": "@alice:localhost",
            "content": { "membership": "join" }
        }),
        json!({
            "type": "m.room.member",
            "state_key": "@bob:localhost",
            "content": { "membership": "join" }
        }),
        json!({
            "type": "m.room.name",
            "state_key": "",
            "content": { "name": "Test Room" }
        }),
    ];
    let timeline_events = vec![RoomEvent {
        event_id: "$event".to_string(),
        room_id: "!room:localhost".to_string(),
        user_id: "@bob:localhost".to_string(),
        event_type: "m.room.message".to_string(),
        content: json!({ "body": "hello" }),
        state_key: None,
        depth: 1,
        origin_server_ts: 1,
        processed_ts: 1,
        not_before: 0,
        status: None,
        reference_image: None,
        origin: "self".to_string(),
        stream_ordering: Some(1),
    }];

    let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &timeline_events,
        "@alice:localhost",
        &HashSet::from([String::from("@bob:localhost")]),
        false,
        &HashSet::new(),
        false,
    );

    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().any(|event| event["state_key"] == "@alice:localhost"));
    assert!(!filtered.iter().any(|event| event["state_key"] == "@bob:localhost"));
    assert!(filtered.iter().any(|event| event["type"] == "m.room.name"));
    assert!(known_now.contains("@bob:localhost"));
}

#[test]
fn test_apply_lazy_load_members_can_include_redundant_members() {
    let state_events = vec![
        json!({
            "type": "m.room.member",
            "state_key": "@alice:localhost",
            "content": { "membership": "join" }
        }),
        json!({
            "type": "m.room.member",
            "state_key": "@bob:localhost",
            "content": { "membership": "join" }
        }),
    ];
    let timeline_events = vec![RoomEvent {
        event_id: "$event".to_string(),
        room_id: "!room:localhost".to_string(),
        user_id: "@bob:localhost".to_string(),
        event_type: "m.room.message".to_string(),
        content: json!({ "body": "hello" }),
        state_key: None,
        depth: 1,
        origin_server_ts: 1,
        processed_ts: 1,
        not_before: 0,
        status: None,
        reference_image: None,
        origin: "self".to_string(),
        stream_ordering: Some(1),
    }];

    let (filtered, _) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &timeline_events,
        "@alice:localhost",
        &HashSet::from([String::from("@bob:localhost")]),
        true,
        &HashSet::new(),
        false,
    );

    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().any(|event| event["state_key"] == "@alice:localhost"));
    assert!(filtered.iter().any(|event| event["state_key"] == "@bob:localhost"));
}

#[test]
fn test_apply_lazy_load_members_includes_state_delta_members_on_incremental_sync() {
    let state_events = vec![
        json!({
            "type": "m.room.member",
            "state_key": "@alice:localhost",
            "content": { "membership": "join" }
        }),
        json!({
            "type": "m.room.member",
            "state_key": "@dave:localhost",
            "content": { "membership": "join" }
        }),
    ];

    let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &[],
        "@alice:localhost",
        &HashSet::new(),
        false,
        &HashSet::from([String::from("@dave:localhost")]),
        false,
    );

    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().any(|event| event["state_key"] == "@alice:localhost"));
    assert!(filtered.iter().any(|event| event["state_key"] == "@dave:localhost"));
    assert!(known_now.contains("@dave:localhost"));
}

#[test]
fn test_apply_lazy_load_members_replays_cached_state_delta_members() {
    let state_events = vec![json!({
        "type": "m.room.member",
        "state_key": "@dave:localhost",
        "content": { "membership": "join" }
    })];

    let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &[],
        "@alice:localhost",
        &HashSet::from([String::from("@dave:localhost")]),
        false,
        &HashSet::from([String::from("@dave:localhost")]),
        false,
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0]["state_key"], "@dave:localhost");
    assert!(known_now.contains("@dave:localhost"));
}

#[test]
fn test_rooms_to_include_keeps_rooms_with_state_delta_membership() {
    let room_ids =
        vec!["!timeline:localhost".to_string(), "!state:localhost".to_string(), "!skip:localhost".to_string()];
    let room_events = HashMap::from([
        (
            "!timeline:localhost".to_string(),
            vec![RoomEvent {
                event_id: "$event".to_string(),
                room_id: "!timeline:localhost".to_string(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: json!({ "body": "hello" }),
                state_key: None,
                depth: 1,
                origin_server_ts: 1,
                processed_ts: 1,
                not_before: 0,
                status: None,
                reference_image: None,
                origin: "self".to_string(),
                stream_ordering: Some(1),
            }],
        ),
        ("!state:localhost".to_string(), Vec::new()),
        ("!skip:localhost".to_string(), Vec::new()),
    ]);
    let changed_members_by_room = HashMap::from([
        ("!state:localhost".to_string(), HashSet::from([String::from("@dave:localhost")])),
        ("!skip:localhost".to_string(), HashSet::new()),
    ]);

    let non_member_state_change_ts_by_room =
        HashMap::from([("!state:localhost".to_string(), 2_i64), ("!skip:localhost".to_string(), 0_i64)]);

    let rooms = SyncService::rooms_to_include(
        &room_ids,
        &room_events,
        &changed_members_by_room,
        &non_member_state_change_ts_by_room,
        true,
    );

    assert_eq!(rooms, vec!["!timeline:localhost".to_string(), "!state:localhost".to_string()]);
}

#[test]
fn test_filter_sync_rooms_respects_room_lists() {
    let memberships = vec![
        UserRoomMembership { room_id: "!keep:localhost".to_string(), membership: "join".to_string() },
        UserRoomMembership { room_id: "!drop:localhost".to_string(), membership: "leave".to_string() },
    ];
    let room_filter = RoomFilter {
        rooms: Some(vec!["!keep:localhost".to_string()]),
        not_rooms: Some(vec!["!drop:localhost".to_string()]),
        include_leave: Some(true),
        state: None,
        timeline: None,
        ephemeral: None,
        account_data: None,
    };

    let filtered = SyncService::filter_sync_rooms(memberships, Some(&room_filter));

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].room_id, "!keep:localhost");
    assert_eq!(filtered[0].membership, "join");
}

fn sample_room_event(event_id_suffix: &str) -> RoomEvent {
    RoomEvent {
        event_id: format!("$event{event_id_suffix}"),
        room_id: "!room:example.com".to_string(),
        user_id: "@user:example.com".to_string(),
        event_type: "m.room.message".to_string(),
        content: json!({
            "body": "hello",
            "msgtype": "m.text"
        }),
        state_key: None,
        depth: 1,
        origin_server_ts: 1_777_000_000_000,
        processed_ts: 1_777_000_000_000,
        not_before: 0,
        status: None,
        reference_image: None,
        origin: "example.com".to_string(),
        stream_ordering: Some(1),
    }
}
