use super::types::*;
use super::SyncService;
use crate::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use synapse_storage::UserRoomMembership;

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
        redacts: None,
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
        redacts: None,
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
        redacts: None,
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
                redacts: None,
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
        redacts: None,
    }
}

fn sample_state_event() -> StateEvent {
    StateEvent {
        event_id: "$state_event:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        sender: "@sender:example.com".to_string(),
        event_type: Some("m.room.name".to_string()),
        content: json!({ "name": "Test Room" }),
        state_key: Some("".to_string()),
        unsigned: None,
        is_redacted: None,
        origin_server_ts: 1_777_000_000_000,
        depth: Some(5),
        processed_ts: None,
        not_before: None,
        status: None,
        reference_image: None,
        origin: Some("example.com".to_string()),
        user_id: Some("@sender:example.com".to_string()),
        stream_ordering: Some(10),
    }
}

// ===========================================================================
// response.rs — event_to_json
// ===========================================================================

#[test]
fn test_event_to_json_client_format() {
    let event = sample_room_event("_client");
    let value = SyncService::event_to_json(&event, SyncEventFormat::Client);

    assert_eq!(value["type"], "m.room.message");
    assert_eq!(value["sender"], "@user:example.com");
    assert_eq!(value["event_id"], "$event_client");
    assert_eq!(value["room_id"], "!room:example.com");
    assert_eq!(value["origin_server_ts"], 1_777_000_000_000_i64);
    assert!(value["unsigned"]["age"].is_i64());
    // Client format must not include depth/origin
    assert!(value.get("depth").is_none());
    assert!(value.get("origin").is_none());
}

#[test]
fn test_event_to_json_federation_format() {
    let event = sample_room_event("_fed");
    let value = SyncService::event_to_json(&event, SyncEventFormat::Federation);

    assert_eq!(value["type"], "m.room.message");
    assert_eq!(value["depth"], 1);
    assert_eq!(value["origin"], "example.com");
}

#[test]
fn test_event_to_json_with_state_key() {
    let mut event = sample_room_event("_state");
    event.state_key = Some("".to_string());
    let value = SyncService::event_to_json(&event, SyncEventFormat::Client);
    assert_eq!(value["state_key"], "");
}

#[test]
fn test_event_to_json_without_state_key() {
    let event = sample_room_event("_nosk");
    let value = SyncService::event_to_json(&event, SyncEventFormat::Client);
    assert!(value.get("state_key").is_none());
}

// ===========================================================================
// response.rs — state_event_to_json
// ===========================================================================

#[test]
fn test_state_event_to_json_client_format() {
    let event = sample_state_event();
    let value = SyncService::state_event_to_json(&event, SyncEventFormat::Client);

    assert_eq!(value["type"], "m.room.name");
    assert_eq!(value["sender"], "@sender:example.com");
    assert_eq!(value["event_id"], "$state_event:example.com");
    assert_eq!(value["room_id"], "!room:example.com");
    assert_eq!(value["origin_server_ts"], 1_777_000_000_000_i64);
    assert_eq!(value["state_key"], "");
    assert!(value["unsigned"]["age"].is_i64());
    assert!(value.get("depth").is_none());
    assert!(value.get("origin").is_none());
}

#[test]
fn test_state_event_to_json_federation_format() {
    let event = sample_state_event();
    let value = SyncService::state_event_to_json(&event, SyncEventFormat::Federation);

    assert_eq!(value["depth"], 5);
    assert_eq!(value["origin"], "example.com");
}

#[test]
fn test_state_event_to_json_falls_back_to_sender_when_user_id_none() {
    let mut event = sample_state_event();
    event.user_id = None;
    event.sender = "@fallback:example.com".to_string();
    let value = SyncService::state_event_to_json(&event, SyncEventFormat::Client);
    assert_eq!(value["sender"], "@fallback:example.com");
}

#[test]
fn test_state_event_to_json_uses_user_id_when_present() {
    let mut event = sample_state_event();
    event.sender = "@legacy:example.com".to_string();
    event.user_id = Some("@actual:example.com".to_string());
    let value = SyncService::state_event_to_json(&event, SyncEventFormat::Client);
    assert_eq!(value["sender"], "@actual:example.com");
}

#[test]
fn test_state_event_to_json_defaults_event_type_when_none() {
    let mut event = sample_state_event();
    event.event_type = None;
    let value = SyncService::state_event_to_json(&event, SyncEventFormat::Client);
    assert_eq!(value["type"], "m.room.message");
}

#[test]
fn test_state_event_to_json_without_state_key() {
    let mut event = sample_state_event();
    event.state_key = None;
    let value = SyncService::state_event_to_json(&event, SyncEventFormat::Client);
    assert!(value.get("state_key").is_none());
}

// ===========================================================================
// response.rs — build_room_sync_value
// ===========================================================================

#[test]
fn test_build_room_sync_value_empty_events() {
    let value = SyncService::build_room_sync_value(BuildRoomSyncValueRequest {
        events: Vec::new(),
        state_list: Vec::new(),
        ephemeral_events: Vec::new(),
        account_data_events: Vec::new(),
        timeline_limit: 10,
        counts: RoomSyncCounts { highlight_count: 0, notification_count: 0 },
        event_fields: None,
        event_format: SyncEventFormat::Client,
    });

    assert!(value["timeline"]["events"].is_array());
    assert_eq!(value["timeline"]["events"].as_array().unwrap().len(), 0);
    assert_eq!(value["timeline"]["limited"], false);
    assert!(value["timeline"]["prev_batch"].is_string());
    assert_eq!(value["state"]["events"].as_array().unwrap().len(), 0);
    assert_eq!(value["ephemeral"]["events"].as_array().unwrap().len(), 0);
    assert_eq!(value["account_data"]["events"].as_array().unwrap().len(), 0);
    assert_eq!(value["unread_notifications"]["highlight_count"], 0);
    assert_eq!(value["unread_notifications"]["notification_count"], 0);
}

#[test]
fn test_build_room_sync_value_with_events() {
    let mut e1 = sample_room_event("_a");
    e1.origin_server_ts = 1_000;
    let mut e2 = sample_room_event("_b");
    e2.origin_server_ts = 2_000;

    let value = SyncService::build_room_sync_value(BuildRoomSyncValueRequest {
        events: vec![e1, e2],
        state_list: Vec::new(),
        ephemeral_events: Vec::new(),
        account_data_events: Vec::new(),
        timeline_limit: 10,
        counts: RoomSyncCounts { highlight_count: 1, notification_count: 5 },
        event_fields: None,
        event_format: SyncEventFormat::Client,
    });

    let timeline_events = value["timeline"]["events"].as_array().unwrap();
    assert_eq!(timeline_events.len(), 2);
    // apply_timeline_limit reverses: newest first
    assert_eq!(timeline_events[0]["event_id"], "$event_b");
    assert_eq!(timeline_events[1]["event_id"], "$event_a");
    assert_eq!(value["timeline"]["limited"], false);
    // prev_batch uses first event of the reversed list (e2, ts=2000)
    assert_eq!(value["timeline"]["prev_batch"], "t2000");
    assert_eq!(value["unread_notifications"]["highlight_count"], 1);
    assert_eq!(value["unread_notifications"]["notification_count"], 5);
}

#[test]
fn test_build_room_sync_value_applies_timeline_limit() {
    let events = vec![sample_room_event("_1"), sample_room_event("_2"), sample_room_event("_3")];

    let value = SyncService::build_room_sync_value(BuildRoomSyncValueRequest {
        events,
        state_list: Vec::new(),
        ephemeral_events: Vec::new(),
        account_data_events: Vec::new(),
        timeline_limit: 2,
        counts: RoomSyncCounts { highlight_count: 0, notification_count: 0 },
        event_fields: None,
        event_format: SyncEventFormat::Client,
    });

    let timeline_events = value["timeline"]["events"].as_array().unwrap();
    assert_eq!(timeline_events.len(), 2);
    assert_eq!(value["timeline"]["limited"], true);
}

#[test]
fn test_build_room_sync_value_prev_batch_from_first_event() {
    let mut event = sample_room_event("_pb");
    event.origin_server_ts = 1_500;

    let value = SyncService::build_room_sync_value(BuildRoomSyncValueRequest {
        events: vec![event],
        state_list: Vec::new(),
        ephemeral_events: Vec::new(),
        account_data_events: Vec::new(),
        timeline_limit: 10,
        counts: RoomSyncCounts { highlight_count: 0, notification_count: 0 },
        event_fields: None,
        event_format: SyncEventFormat::Client,
    });

    assert_eq!(value["timeline"]["prev_batch"], "t1500");
}

#[test]
fn test_build_room_sync_value_applies_event_fields_filter() {
    let event = sample_room_event("_filter");

    let value = SyncService::build_room_sync_value(BuildRoomSyncValueRequest {
        events: vec![event],
        state_list: Vec::new(),
        ephemeral_events: Vec::new(),
        account_data_events: Vec::new(),
        timeline_limit: 10,
        counts: RoomSyncCounts { highlight_count: 0, notification_count: 0 },
        event_fields: Some(&["type".to_string(), "event_id".to_string(), "unsigned.age".to_string()]),
        event_format: SyncEventFormat::Client,
    });

    let timeline_event = &value["timeline"]["events"][0];
    assert_eq!(timeline_event["type"], "m.room.message");
    assert_eq!(timeline_event["event_id"], "$event_filter");
    assert!(timeline_event["unsigned"]["age"].is_i64());
    // Filtered-out fields should not be present
    assert!(timeline_event.get("sender").is_none());
    assert!(timeline_event.get("content").is_none());
    assert!(timeline_event.get("room_id").is_none());
}

// ===========================================================================
// data_fetch.rs — aggregate_ephemeral_events
// ===========================================================================

#[test]
fn test_aggregate_ephemeral_events_empty() {
    let result = SyncService::aggregate_ephemeral_events(Vec::new());
    assert!(result.is_empty());
}

#[test]
fn test_aggregate_ephemeral_events_typing_only() {
    let typing = json!({
        "type": "m.typing",
        "content": { "user_ids": ["@alice:example.com"] }
    });
    let result = SyncService::aggregate_ephemeral_events(vec![typing.clone()]);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0]["type"], "m.typing");
}

#[test]
fn test_aggregate_ephemeral_events_receipt_only() {
    let receipt = json!({
        "type": "m.receipt",
        "content": {
            "$event1:example.com": { "m.read": { "@alice:example.com": { "ts": 1000 } } }
        }
    });
    let result = SyncService::aggregate_ephemeral_events(vec![receipt]);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0]["type"], "m.receipt");
    assert!(result[0]["content"]["$event1:example.com"]["m.read"].is_object());
}

#[test]
fn test_aggregate_ephemeral_events_merges_receipts_for_different_events() {
    let r1 = json!({
        "type": "m.receipt",
        "content": {
            "$event1:example.com": { "m.read": { "@alice:example.com": { "ts": 1000 } } }
        }
    });
    let r2 = json!({
        "type": "m.receipt",
        "content": {
            "$event2:example.com": { "m.read": { "@bob:example.com": { "ts": 2000 } } }
        }
    });
    let result = SyncService::aggregate_ephemeral_events(vec![r1, r2]);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0]["type"], "m.receipt");
    let content = &result[0]["content"];
    assert!(content["$event1:example.com"]["m.read"]["@alice:example.com"].is_object());
    assert!(content["$event2:example.com"]["m.read"]["@bob:example.com"].is_object());
}

#[test]
fn test_aggregate_ephemeral_events_same_event_same_receipt_type_overwrites() {
    // When two receipts target the same event_id with the same receipt_type,
    // the second overwrites the first (last-write-wins per receipt_type).
    let r1 = json!({
        "type": "m.receipt",
        "content": {
            "$event1:example.com": { "m.read": { "@alice:example.com": { "ts": 1000 } } }
        }
    });
    let r2 = json!({
        "type": "m.receipt",
        "content": {
            "$event1:example.com": { "m.read": { "@bob:example.com": { "ts": 2000 } } }
        }
    });
    let result = SyncService::aggregate_ephemeral_events(vec![r1, r2]);
    assert_eq!(result.len(), 1);
    let merged = &result[0]["content"]["$event1:example.com"]["m.read"];
    // @bob overwrites @alice
    assert!(merged.get("@alice:example.com").is_none());
    assert!(merged["@bob:example.com"].is_object());
}

#[test]
fn test_aggregate_ephemeral_events_mixed_receipts_and_typing() {
    let receipt = json!({
        "type": "m.receipt",
        "content": { "$e:example.com": { "m.read": { "@a:example.com": { "ts": 1 } } } }
    });
    let typing = json!({
        "type": "m.typing",
        "content": { "user_ids": ["@a:example.com"] }
    });
    let result = SyncService::aggregate_ephemeral_events(vec![receipt, typing.clone()]);
    // Receipt aggregated first, then typing appended
    assert_eq!(result.len(), 2);
    assert_eq!(result[0]["type"], "m.receipt");
    assert_eq!(result[1]["type"], "m.typing");
}

#[test]
fn test_aggregate_ephemeral_events_unknown_type_passed_through() {
    let custom = json!({
        "type": "org.example.custom",
        "content": { "foo": "bar" }
    });
    let result = SyncService::aggregate_ephemeral_events(vec![custom.clone()]);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0]["type"], "org.example.custom");
    assert_eq!(result[0]["content"]["foo"], "bar");
}

// ===========================================================================
// data_fetch.rs — to_device_since_stream_id
// ===========================================================================

#[test]
fn test_to_device_since_stream_id_none() {
    let since: Option<SyncToken> = None;
    assert_eq!(SyncService::to_device_since_stream_id(&since), 0);
}

#[test]
fn test_to_device_since_stream_id_without_to_device_id() {
    let since = Some(SyncToken {
        stream_id: 100,
        room_id: None,
        event_type: None,
        to_device_stream_id: None,
        device_list_stream_id: None,
    });
    assert_eq!(SyncService::to_device_since_stream_id(&since), 0);
}

#[test]
fn test_to_device_since_stream_id_with_to_device_id() {
    let since = Some(SyncToken {
        stream_id: 100,
        room_id: None,
        event_type: None,
        to_device_stream_id: Some(42),
        device_list_stream_id: None,
    });
    assert_eq!(SyncService::to_device_since_stream_id(&since), 42);
}

// ===========================================================================
// data_fetch.rs — device_list_since_stream_id
// ===========================================================================

#[test]
fn test_device_list_since_stream_id_none() {
    let since: Option<SyncToken> = None;
    assert_eq!(SyncService::device_list_since_stream_id(&since), 0);
}

#[test]
fn test_device_list_since_stream_id_without_device_list_id() {
    let since = Some(SyncToken {
        stream_id: 100,
        room_id: None,
        event_type: None,
        to_device_stream_id: None,
        device_list_stream_id: None,
    });
    assert_eq!(SyncService::device_list_since_stream_id(&since), 0);
}

#[test]
fn test_device_list_since_stream_id_with_device_list_id() {
    let since = Some(SyncToken {
        stream_id: 100,
        room_id: None,
        event_type: None,
        to_device_stream_id: None,
        device_list_stream_id: Some(99),
    });
    assert_eq!(SyncService::device_list_since_stream_id(&since), 99);
}

// ---------------------------------------------------------------------------
// apply_lazy_load_members_with_cache tests
// ---------------------------------------------------------------------------

fn make_member_event(state_key: &str) -> Value {
    json!({
        "type": "m.room.member",
        "state_key": state_key,
        "content": {"membership": "join"},
        "sender": state_key,
        "event_id": format!("$ev_{}", state_key)
    })
}

fn make_non_member_event(event_type: &str) -> Value {
    json!({
        "type": event_type,
        "state_key": "",
        "content": {},
        "sender": "@a:b",
        "event_id": format!("$ev_{}", event_type.replace('.', "_"))
    })
}

fn make_timeline_event(user_id: &str, event_type: &str, state_key: Option<&str>) -> synapse_storage::RoomEvent {
    synapse_storage::RoomEvent {
        event_id: format!("$tl_{}", user_id),
        room_id: "!room1:matrix.org".into(),
        user_id: user_id.into(),
        event_type: event_type.into(),
        content: json!({}),
        state_key: state_key.map(|s| s.to_string()),
        depth: 1,
        origin_server_ts: 1700000000000,
        processed_ts: 1700000001000,
        not_before: 0,
        status: None,
        reference_image: None,
        origin: "matrix.org".into(),
        stream_ordering: Some(1),
        redacts: None,
    }
}

#[test]
fn test_lazy_load_timeline_limited_non_members_pass_through() {
    let state_events = vec![make_non_member_event("m.room.topic"), make_non_member_event("m.room.name")];
    let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events.clone(),
        &[],
        "@alice:b",
        &HashSet::new(),
        false,
        &HashSet::new(),
        true,
    );
    assert_eq!(filtered.len(), 2);
    assert!(known_now.is_empty());
}

#[test]
fn test_lazy_load_timeline_limited_filters_known_members() {
    let state_events = vec![make_member_event("@alice:b"), make_member_event("@bob:b")];
    let mut known = HashSet::new();
    known.insert("@alice:b".to_string());
    let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &[],
        "@alice:b",
        &known,
        false,
        &HashSet::new(),
        true,
    );
    // Only @bob:b passes (not in known_members)
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0]["state_key"], json!("@bob:b"));
    // known_now collects all member state_keys
    assert_eq!(known_now.len(), 2);
}

#[test]
fn test_lazy_load_timeline_limited_include_redundant_keeps_all() {
    let state_events = vec![make_member_event("@alice:b"), make_member_event("@bob:b")];
    let mut known = HashSet::new();
    known.insert("@alice:b".to_string());
    let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &[],
        "@alice:b",
        &known,
        true, // include_redundant_members
        &HashSet::new(),
        true,
    );
    assert_eq!(filtered.len(), 2);
    assert_eq!(known_now.len(), 2);
}

#[test]
fn test_lazy_load_timeline_limited_member_without_state_key_filtered() {
    let mut ev = make_member_event("@alice:b");
    ev["state_key"] = Value::Null;
    let state_events = vec![ev, make_non_member_event("m.room.topic")];
    let (filtered, _known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &[],
        "@alice:b",
        &HashSet::new(),
        false,
        &HashSet::new(),
        true,
    );
    // Non-member passes, member without state_key is filtered out
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0]["type"], json!("m.room.topic"));
}

#[test]
fn test_lazy_load_required_members_from_user_id() {
    let state_events = vec![make_member_event("@alice:b")];
    let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &[],
        "@alice:b",
        &HashSet::new(),
        false,
        &HashSet::new(),
        false,
    );
    // @alice:b is required (the user themselves)
    assert_eq!(filtered.len(), 1);
    assert!(known_now.contains("@alice:b"));
}

#[test]
fn test_lazy_load_required_members_from_timeline_senders() {
    let state_events = vec![make_member_event("@bob:b")];
    let tl = make_timeline_event("@bob:b", "m.room.message", None);
    let (filtered, _known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &[tl],
        "@alice:b",
        &HashSet::new(),
        false,
        &HashSet::new(),
        false,
    );
    // @bob:b is required because they sent a timeline event
    assert_eq!(filtered.len(), 1);
}

#[test]
fn test_lazy_load_required_members_from_timeline_member_state_keys() {
    let tl = make_timeline_event("@bob:b", "m.room.member", Some("@charlie:b"));
    let state_events = vec![make_member_event("@charlie:b"), make_member_event("@dave:b")];
    let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &[tl],
        "@alice:b",
        &HashSet::new(),
        false,
        &HashSet::new(),
        false,
    );
    // @charlie:b is required (timeline member state_key), @dave:b is not
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0]["state_key"], json!("@charlie:b"));
    assert_eq!(known_now.len(), 1); // @charlie:b from timeline's member event
}

#[test]
fn test_lazy_load_required_members_from_changed_member_ids() {
    let state_events = vec![make_member_event("@dave:b"), make_member_event("@eve:b")];
    let mut changed = HashSet::new();
    changed.insert("@dave:b".to_string());
    let (filtered, _known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &[],
        "@alice:b",
        &HashSet::new(),
        false,
        &changed,
        false,
    );
    // @dave:b is required (changed member), @eve:b is not
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0]["state_key"], json!("@dave:b"));
}

#[test]
fn test_lazy_load_non_member_events_always_pass() {
    let state_events = vec![
        make_non_member_event("m.room.topic"),
        make_non_member_event("m.room.name"),
        make_non_member_event("m.room.canonical_alias"),
    ];
    let (filtered, _known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &[],
        "@alice:b",
        &HashSet::new(),
        false,
        &HashSet::new(),
        false,
    );
    assert_eq!(filtered.len(), 3);
}

#[test]
fn test_lazy_load_known_members_filtered_when_not_redundant() {
    let state_events = vec![make_member_event("@bob:b")];
    let mut known = HashSet::new();
    known.insert("@bob:b".to_string());
    // Make @bob:b required via timeline
    let tl = make_timeline_event("@bob:b", "m.room.message", None);
    let (filtered, _known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &[tl],
        "@alice:b",
        &known,
        false, // not include_redundant
        &HashSet::new(),
        false,
    );
    // @bob:b is required but known and not changed → filtered out
    assert_eq!(filtered.len(), 0);
}

#[test]
fn test_lazy_load_changed_member_included_even_if_known() {
    let state_events = vec![make_member_event("@bob:b")];
    let mut known = HashSet::new();
    known.insert("@bob:b".to_string());
    let mut changed = HashSet::new();
    changed.insert("@bob:b".to_string());
    // Make @bob:b required via timeline
    let tl = make_timeline_event("@bob:b", "m.room.message", None);
    let (filtered, _known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &[tl],
        "@alice:b",
        &known,
        false,
        &changed,
        false,
    );
    // @bob:b is known but changed → still included
    assert_eq!(filtered.len(), 1);
}

#[test]
fn test_lazy_load_empty_state_events() {
    let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
        vec![],
        &[],
        "@alice:b",
        &HashSet::new(),
        false,
        &HashSet::new(),
        false,
    );
    assert!(filtered.is_empty());
    assert!(known_now.is_empty());
}

#[test]
fn test_lazy_load_no_member_events_in_state() {
    let state_events = vec![make_non_member_event("m.room.topic"), make_non_member_event("m.room.name")];
    let tl = make_timeline_event("@bob:b", "m.room.message", None);
    let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &[tl],
        "@alice:b",
        &HashSet::new(),
        false,
        &HashSet::new(),
        false,
    );
    assert_eq!(filtered.len(), 2);
    assert!(known_now.is_empty());
}

#[test]
fn test_lazy_load_timeline_limited_empty_state_and_timeline() {
    let (filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
        vec![],
        &[],
        "@alice:b",
        &HashSet::new(),
        false,
        &HashSet::new(),
        true,
    );
    assert!(filtered.is_empty());
    assert!(known_now.is_empty());
}

#[test]
fn test_lazy_load_known_now_excludes_non_members() {
    let timeline = vec![
        make_timeline_event("@bob:b", "m.room.message", None),
        make_timeline_event("@charlie:b", "m.room.member", Some("@charlie:b")),
    ];
    let state_events = vec![make_member_event("@charlie:b"), make_member_event("@dave:b")];
    // @dave:b is required via changed
    let mut changed = HashSet::new();
    changed.insert("@dave:b".to_string());

    let (_filtered, known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &timeline,
        "@alice:b",
        &HashSet::new(),
        false,
        &changed,
        false,
    );
    // known_now = members from timeline, so @charlie:b
    assert!(known_now.contains("@charlie:b"));
    // @bob:b is in timeline but NOT a member event → not in known_now
    assert!(!known_now.contains("@bob:b"));
}

#[test]
fn test_lazy_load_include_redundant_keeps_known_members() {
    let state_events = vec![make_member_event("@bob:b")];
    let mut known = HashSet::new();
    known.insert("@bob:b".to_string());
    // Make @bob:b required
    let tl = make_timeline_event("@bob:b", "m.room.message", None);
    let (filtered, _known_now) = SyncService::apply_lazy_load_members_with_cache(
        state_events,
        &[tl],
        "@alice:b",
        &known,
        true, // include_redundant_members
        &HashSet::new(),
        false,
    );
    // @bob:b is known but include_redundant → still passes
    assert_eq!(filtered.len(), 1);
}

// ---------------------------------------------------------------------------
// count_events_by_room tests
// ---------------------------------------------------------------------------

#[test]
fn test_count_events_by_room_empty() {
    let map: HashMap<String, Vec<synapse_storage::RoomEvent>> = HashMap::new();
    assert_eq!(SyncService::count_events_by_room(&map), 0);
}

// ---------------------------------------------------------------------------
// is_slow_request_for tests
// ---------------------------------------------------------------------------

#[test]
fn test_is_slow_request_for_below_threshold() {
    assert!(!SyncService::is_slow_request_for(99.0, 100));
}

#[test]
fn test_is_slow_request_for_at_threshold() {
    assert!(SyncService::is_slow_request_for(100.0, 100));
}

#[test]
fn test_is_slow_request_for_above_threshold() {
    assert!(SyncService::is_slow_request_for(500.0, 100));
}

#[test]
fn test_is_slow_request_for_zero_threshold() {
    // Zero threshold: any non-negative value is "slow"
    assert!(SyncService::is_slow_request_for(0.0, 0));
    assert!(SyncService::is_slow_request_for(1.0, 0));
}

// ---------------------------------------------------------------------------
// event_since_ts tests
// ---------------------------------------------------------------------------

fn make_token(stream_id: i64) -> SyncToken {
    SyncToken { stream_id, room_id: None, event_type: None, to_device_stream_id: None, device_list_stream_id: None }
}

#[test]
fn test_event_since_ts_none_token_returns_zero() {
    assert_eq!(SyncService::event_since_ts(&None), 0);
}

#[test]
fn test_event_since_ts_timestamp_token_returns_stream_id() {
    let token = make_token(1700000000000);
    assert_eq!(SyncService::event_since_ts(&Some(token)), 1700000000000);
}

#[test]
fn test_event_since_ts_below_timestamp_min_returns_zero() {
    let token = make_token(999_999_999_999);
    assert_eq!(SyncService::event_since_ts(&Some(token)), 0);
}

#[test]
fn test_event_since_ts_below_min_with_to_device_returns_stream_id() {
    let mut token = make_token(100);
    token.to_device_stream_id = Some(200);
    assert_eq!(SyncService::event_since_ts(&Some(token)), 100);
}

#[test]
fn test_event_since_ts_below_min_with_device_list_returns_stream_id() {
    let mut token = make_token(50);
    token.device_list_stream_id = Some(150);
    assert_eq!(SyncService::event_since_ts(&Some(token)), 50);
}

#[test]
fn test_event_since_ts_negative_stream_id_clamped_to_zero() {
    let mut token = make_token(-5);
    token.to_device_stream_id = Some(1);
    assert_eq!(SyncService::event_since_ts(&Some(token)), 0);
}

// ---------------------------------------------------------------------------
// next_event_stream_id tests
// ---------------------------------------------------------------------------

#[test]
fn test_next_event_stream_id_no_events_no_token_returns_current_time() {
    let room_events: HashMap<String, Vec<synapse_storage::RoomEvent>> = HashMap::new();
    let result = SyncService::next_event_stream_id(&None, &room_events, None);
    assert!(result > 1_700_000_000_000);
}

#[test]
fn test_next_event_stream_id_uses_max_stream_ordering() {
    let mut room_events: HashMap<String, Vec<synapse_storage::RoomEvent>> = HashMap::new();
    let mut event1 = make_timeline_event("@a:b", "m.room.message", None);
    event1.stream_ordering = Some(100);
    let mut event2 = make_timeline_event("@b:b", "m.room.message", None);
    event2.stream_ordering = Some(200);
    room_events.insert("!r1:b".into(), vec![event1, event2]);
    let result = SyncService::next_event_stream_id(&None, &room_events, None);
    assert_eq!(result, 200);
}

#[test]
fn test_next_event_stream_id_respects_since_token() {
    let mut room_events: HashMap<String, Vec<synapse_storage::RoomEvent>> = HashMap::new();
    let mut event1 = make_timeline_event("@a:b", "m.room.message", None);
    event1.stream_ordering = Some(100);
    room_events.insert("!r1:b".into(), vec![event1]);
    let result = SyncService::next_event_stream_id(&Some(make_token(300)), &room_events, None);
    assert_eq!(result, 300);
}

#[test]
fn test_next_event_stream_id_event_max_exceeds_token() {
    let mut room_events: HashMap<String, Vec<synapse_storage::RoomEvent>> = HashMap::new();
    let mut event1 = make_timeline_event("@a:b", "m.room.message", None);
    event1.stream_ordering = Some(500);
    room_events.insert("!r1:b".into(), vec![event1]);
    let result = SyncService::next_event_stream_id(&Some(make_token(300)), &room_events, None);
    assert_eq!(result, 500);
}

#[test]
fn test_next_event_stream_id_falls_back_to_origin_server_ts() {
    let mut room_events: HashMap<String, Vec<synapse_storage::RoomEvent>> = HashMap::new();
    let mut event1 = make_timeline_event("@a:b", "m.room.message", None);
    event1.stream_ordering = None;
    event1.origin_server_ts = 1700000000500;
    let mut event2 = make_timeline_event("@b:b", "m.room.message", None);
    event2.stream_ordering = None;
    event2.origin_server_ts = 1700000001000;
    room_events.insert("!r1:b".into(), vec![event1, event2]);
    let result = SyncService::next_event_stream_id(&None, &room_events, None);
    assert_eq!(result, 1700000001000);
}

#[test]
fn test_next_event_stream_id_uses_state_change_ts() {
    let room_events: HashMap<String, Vec<synapse_storage::RoomEvent>> = HashMap::new();
    let mut state_ts: HashMap<String, i64> = HashMap::new();
    state_ts.insert("!r1:b".into(), 1700000002000);
    let result = SyncService::next_event_stream_id(&None, &room_events, Some(&state_ts));
    assert_eq!(result, 1700000002000);
}

#[test]
fn test_next_event_stream_id_event_origin_ts_over_state_ts() {
    let mut room_events: HashMap<String, Vec<synapse_storage::RoomEvent>> = HashMap::new();
    let mut event1 = make_timeline_event("@a:b", "m.room.message", None);
    event1.stream_ordering = None;
    event1.origin_server_ts = 1700000003000;
    room_events.insert("!r1:b".into(), vec![event1]);
    let mut state_ts: HashMap<String, i64> = HashMap::new();
    state_ts.insert("!r1:b".into(), 1700000002000);
    let result = SyncService::next_event_stream_id(&None, &room_events, Some(&state_ts));
    assert_eq!(result, 1700000003000);
}

// ---------------------------------------------------------------------------
// room_sections_from_memberships tests
// ---------------------------------------------------------------------------

#[test]
fn test_room_sections_join_membership_maps_to_join() {
    let memberships = vec![UserRoomMembership { room_id: "!r1:b".into(), membership: "join".into() }];
    let sections = SyncService::room_sections_from_memberships(&memberships);
    assert_eq!(sections.len(), 1);
    assert_eq!(sections.get("!r1:b").copied(), Some(SyncRoomSection::Join));
}

#[test]
fn test_room_sections_leave_membership_maps_to_leave() {
    let memberships = vec![UserRoomMembership { room_id: "!r2:b".into(), membership: "leave".into() }];
    let sections = SyncService::room_sections_from_memberships(&memberships);
    assert_eq!(sections.len(), 1);
    assert_eq!(sections.get("!r2:b").copied(), Some(SyncRoomSection::Leave));
}

#[test]
fn test_room_sections_invite_membership_maps_to_join() {
    let memberships = vec![UserRoomMembership { room_id: "!r3:b".into(), membership: "invite".into() }];
    let sections = SyncService::room_sections_from_memberships(&memberships);
    assert_eq!(sections.get("!r3:b").copied(), Some(SyncRoomSection::Join));
}

#[test]
fn test_room_sections_mixed_memberships() {
    let memberships = vec![
        UserRoomMembership { room_id: "!r1:b".into(), membership: "join".into() },
        UserRoomMembership { room_id: "!r2:b".into(), membership: "leave".into() },
    ];
    let sections = SyncService::room_sections_from_memberships(&memberships);
    assert_eq!(sections.len(), 2);
    assert_eq!(sections.get("!r1:b").copied(), Some(SyncRoomSection::Join));
    assert_eq!(sections.get("!r2:b").copied(), Some(SyncRoomSection::Leave));
}

#[test]
fn test_room_sections_empty() {
    let memberships: Vec<UserRoomMembership> = vec![];
    let sections = SyncService::room_sections_from_memberships(&memberships);
    assert!(sections.is_empty());
}
