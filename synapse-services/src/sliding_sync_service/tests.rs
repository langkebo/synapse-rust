use super::*;
use synapse_storage::device::DeviceStorage;
use synapse_storage::event::EventStorage;
use synapse_storage::membership::RoomMemberStorage;
use synapse_storage::sliding_sync::{SlidingSyncFilters, SlidingSyncRoom};
use synapse_storage::PresenceStorage;

#[tokio::test]
async fn test_room_to_json() {
    let _service = create_test_service();
    let room = SlidingSyncRoom {
        id: 1,
        user_id: "@alice:example.com".to_string(),
        device_id: "DEVICE123".to_string(),
        room_id: "!room:example.com".to_string(),
        conn_id: None,
        list_key: Some("main".to_string()),
        bump_stamp: 1234567890000,
        highlight_count: 5,
        notification_count: 10,
        is_dm: true,
        is_encrypted: true,
        is_tombstoned: false,
        is_invited: false,
        name: Some("Test Room".to_string()),
        avatar: Some("mxc://example.com/avatar".to_string()),
        timestamp: 1234567890000,
        created_ts: 1234567890000,
        updated_ts: 1234567890000,
    };

    let json = SlidingSyncService::room_to_json(&room);

    assert_eq!(json["room_id"], "!room:example.com");
    assert_eq!(json["name"], "Test Room");
    assert_eq!(json["highlight_count"], 5);
    assert!(json["is_dm"].as_bool().unwrap());
}

#[tokio::test]
async fn test_build_ops_empty() {
    let ops = SlidingSyncService::build_sync_ops(&[]);
    assert!(ops.is_empty());
}

#[tokio::test]
async fn test_build_ops_with_rooms() {
    let ops = SlidingSyncService::build_sync_ops(&[SlidingListRangeSnapshot {
        start: 0,
        end: 1,
        room_ids: vec!["!room1:example.com".to_string(), "!room2:example.com".to_string()],
    }]);

    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0]["op"], "SYNC");
}

#[tokio::test]
async fn test_build_incremental_ops_uses_insert_and_delete() {
    let _service = create_test_service();
    let previous = SlidingListWindowSnapshot {
        ranges: vec![SlidingListRangeSnapshot {
            start: 0,
            end: 1,
            room_ids: vec!["!room1:example.com".to_string(), "!room2:example.com".to_string()],
        }],
    };
    let current = vec![SlidingListRangeSnapshot {
        start: 0,
        end: 1,
        room_ids: vec!["!room0:example.com".to_string(), "!room1:example.com".to_string()],
    }];

    let ops = SlidingSyncService::build_incremental_ops(&previous, &current).unwrap();

    assert!(ops.iter().any(|op| op["op"] == "INSERT"));
    assert!(ops.iter().any(|op| op["op"] == "DELETE"));
}

fn create_test_service() -> SlidingSyncService {
    let pool = Arc::new(
        sqlx::postgres::PgPoolOptions::new().max_connections(1).connect_lazy("postgres://localhost/test").unwrap(),
    );
    SlidingSyncService {
        storage: SlidingSyncStorage::new(pool.clone()),
        cache: Arc::new(CacheManager::new(&synapse_cache::CacheConfig::default())),
        event_storage: Arc::new(EventStorage::new(&pool, "localhost".to_string())),
        device_key_storage: DeviceKeyStorage::new(&pool),
        typing_service: Arc::new(crate::typing_service::TypingService::default()),
        presence_storage: Arc::new(PresenceStorage::new(
            pool.clone(),
            Arc::new(CacheManager::new(&synapse_cache::CacheConfig::default())),
        )),
        member_storage: Arc::new(RoomMemberStorage::new(&pool, "localhost")),
        device_storage: Arc::new(DeviceStorage::new(&pool)),
        to_device_storage: ToDeviceStorage::new(&pool),
        connection_tracker: Arc::new(
            moka::sync::Cache::builder()
                .max_capacity(MAX_TRACKED_CONNECTIONS)
                .time_to_idle(std::time::Duration::from_millis(CONNECTION_TTL_MS as u64))
                .build(),
        ),
        metrics: Arc::new(MetricsCollector::new()),
        latency_threshold_ms: PerformanceConfig::default().sliding_sync_latency_threshold_ms,
    }
}

#[tokio::test]
async fn test_sliding_sync_filters_serialization() {
    let filters = SlidingSyncFilters {
        is_invite: Some(false),
        is_tombstoned: None,
        room_name_like: Some("test".to_string()),
        ..Default::default()
    };

    let json = serde_json::to_value(&filters).unwrap();

    assert!(json.get("is_invite").is_some());
    assert!(json.get("is_tombstoned").is_none());
    assert_eq!(json.get("room_name_like").unwrap().as_str().unwrap(), "test");
}

// ── required_state_matches ──────────────────────────────────────────

#[test]
fn required_state_matches_exact_type_and_key() {
    let event = make_state_event(Some("m.room.name"), Some(""));
    let required_state = vec![vec!["m.room.name".to_string(), "".to_string()]];
    assert!(SlidingSyncService::required_state_matches(&required_state, &event));
}

#[test]
fn required_state_matches_wildcard_type() {
    let event = make_state_event(Some("m.room.name"), Some(""));
    let required_state = vec![vec!["*".to_string(), "".to_string()]];
    assert!(SlidingSyncService::required_state_matches(&required_state, &event));
}

#[test]
fn required_state_matches_wildcard_state_key() {
    let event = make_state_event(Some("m.room.member"), Some("@alice:example.com"));
    let required_state = vec![vec!["m.room.member".to_string(), "*".to_string()]];
    assert!(SlidingSyncService::required_state_matches(&required_state, &event));
}

#[test]
fn required_state_matches_both_wildcards() {
    let event = make_state_event(Some("m.room.topic"), Some(""));
    let required_state = vec![vec!["*".to_string(), "*".to_string()]];
    assert!(SlidingSyncService::required_state_matches(&required_state, &event));
}

#[test]
fn required_state_rejects_wrong_type() {
    let event = make_state_event(Some("m.room.name"), Some(""));
    let required_state = vec![vec!["m.room.topic".to_string(), "".to_string()]];
    assert!(!SlidingSyncService::required_state_matches(&required_state, &event));
}

#[test]
fn required_state_rejects_wrong_state_key() {
    let event = make_state_event(Some("m.room.name"), Some(""));
    let required_state = vec![vec!["m.room.name".to_string(), "alt".to_string()]];
    assert!(!SlidingSyncService::required_state_matches(&required_state, &event));
}

#[test]
fn required_state_matches_one_of_many() {
    let event = make_state_event(Some("m.room.topic"), Some(""));
    let required_state = vec![
        vec!["m.room.name".to_string(), "".to_string()],
        vec!["m.room.topic".to_string(), "".to_string()],
        vec!["m.room.member".to_string(), "*".to_string()],
    ];
    assert!(SlidingSyncService::required_state_matches(&required_state, &event));
}

#[test]
fn required_state_handles_none_event_type() {
    let event = make_state_event(None, Some(""));
    let required_state = vec![vec!["*".to_string(), "".to_string()]];
    assert!(SlidingSyncService::required_state_matches(&required_state, &event));
}

#[test]
fn required_state_handles_none_state_key() {
    let event = make_state_event(Some("m.room.message"), None);
    let required_state = vec![vec!["m.room.message".to_string(), "*".to_string()]];
    assert!(SlidingSyncService::required_state_matches(&required_state, &event));
}

// ── compute_left_shared_users ───────────────────────────────────────

#[test]
fn compute_left_shared_users_detects_leavers() {
    let previous = vec!["@a:example.com".to_string(), "@b:example.com".to_string()];
    let current = vec!["@a:example.com".to_string()];
    let left = SlidingSyncService::compute_left_shared_users(&previous, &current);
    assert_eq!(left, vec!["@b:example.com".to_string()]);
}

#[test]
fn compute_left_shared_users_returns_empty_when_all_stay() {
    let previous = vec!["@a:example.com".to_string(), "@b:example.com".to_string()];
    let current = vec!["@a:example.com".to_string(), "@b:example.com".to_string()];
    let left = SlidingSyncService::compute_left_shared_users(&previous, &current);
    assert!(left.is_empty());
}

#[test]
fn compute_left_shared_users_new_users_ignored() {
    let previous = vec!["@a:example.com".to_string()];
    let current = vec!["@a:example.com".to_string(), "@b:example.com".to_string()];
    let left = SlidingSyncService::compute_left_shared_users(&previous, &current);
    assert!(left.is_empty());
}

#[test]
fn compute_left_shared_users_all_left() {
    let previous = vec!["@a:example.com".to_string(), "@b:example.com".to_string()];
    let current: Vec<String> = vec![];
    let left = SlidingSyncService::compute_left_shared_users(&previous, &current);
    assert_eq!(left.len(), 2);
}

#[test]
fn compute_left_shared_users_empty_previous() {
    let previous: Vec<String> = vec![];
    let current = vec!["@a:example.com".to_string()];
    let left = SlidingSyncService::compute_left_shared_users(&previous, &current);
    assert!(left.is_empty());
}

#[test]
fn compute_left_shared_users_handles_unsorted_input() {
    let previous = vec!["@c:example.com".to_string(), "@a:example.com".to_string(), "@b:example.com".to_string()];
    let current = vec!["@b:example.com".to_string(), "@c:example.com".to_string()];
    let left = SlidingSyncService::compute_left_shared_users(&previous, &current);
    assert_eq!(left, vec!["@a:example.com".to_string()]);
}

// ── list_snapshot_cache_key ─────────────────────────────────────────

#[test]
fn list_snapshot_cache_key_with_conn_id() {
    let key = SlidingSyncService::list_snapshot_cache_key("alice", "D1", Some("conn1"), "main");
    assert!(key.contains("alice"));
    assert!(key.contains("D1"));
    assert!(key.contains("conn1"));
    assert!(key.contains("main"));
}

#[test]
fn list_snapshot_cache_key_without_conn_id() {
    let key = SlidingSyncService::list_snapshot_cache_key("bob", "D2", None, "list");
    assert_eq!(key, "sliding_sync:list:bob:D2::list");
}

// ── subscription_config_from_value ──────────────────────────────────

#[test]
fn subscription_config_from_value_none_returns_default() {
    let config = SlidingSyncService::subscription_config_from_value(None);
    assert!(config.timeline_limit.is_none());
    assert!(config.required_state.is_none());
}

#[test]
fn subscription_config_from_value_parses_timeline_limit() {
    let value = serde_json::json!({"timeline_limit": 100});
    let config = SlidingSyncService::subscription_config_from_value(Some(&value));
    assert_eq!(config.timeline_limit, Some(100));
}

#[test]
fn subscription_config_from_value_parses_camel_case() {
    let value = serde_json::json!({"timelineLimit": 50});
    let config = SlidingSyncService::subscription_config_from_value(Some(&value));
    assert_eq!(config.timeline_limit, Some(50));
}

#[test]
fn subscription_config_from_value_parses_required_state() {
    let value = serde_json::json!({
        "required_state": [["m.room.name", ""], ["m.room.topic", ""]]
    });
    let config = SlidingSyncService::subscription_config_from_value(Some(&value));
    assert!(config.required_state.is_some());
    assert_eq!(config.required_state.unwrap().len(), 2);
}

// ── incremental ops edge cases ──────────────────────────────────────

#[test]
fn build_incremental_ops_different_ranges_returns_none() {
    let previous = SlidingListWindowSnapshot {
        ranges: vec![SlidingListRangeSnapshot {
            start: 0,
            end: 1,
            room_ids: vec!["!r1:ex.com".to_string(), "!r2:ex.com".to_string()],
        }],
    };
    let current = vec![SlidingListRangeSnapshot {
        start: 5,
        end: 6,
        room_ids: vec!["!r1:ex.com".to_string(), "!r2:ex.com".to_string()],
    }];
    assert!(SlidingSyncService::build_incremental_ops(&previous, &current).is_none());
}

#[test]
fn build_incremental_ops_same_rooms_no_ops() {
    let previous = SlidingListWindowSnapshot {
        ranges: vec![SlidingListRangeSnapshot {
            start: 0,
            end: 1,
            room_ids: vec!["!r1:ex.com".to_string(), "!r2:ex.com".to_string()],
        }],
    };
    let current = vec![SlidingListRangeSnapshot {
        start: 0,
        end: 1,
        room_ids: vec!["!r1:ex.com".to_string(), "!r2:ex.com".to_string()],
    }];
    let ops = SlidingSyncService::build_incremental_ops(&previous, &current).unwrap();
    assert!(ops.is_empty());
}

// ── state_event_to_json ─────────────────────────────────────────────

#[test]
fn state_event_to_json_includes_all_fields() {
    let event = make_state_event(Some("m.room.name"), Some(""));
    let json = SlidingSyncService::state_event_to_json(&event);
    assert_eq!(json["type"], "m.room.name");
    assert_eq!(json["event_id"], "ev1");
    assert_eq!(json["room_id"], "!r:ex.com");
    assert!(json["unsigned"]["age"].is_number());
}

#[test]
fn state_event_to_json_includes_state_key() {
    let event = make_state_event(Some("m.room.member"), Some("@alice:ex.com"));
    let json = SlidingSyncService::state_event_to_json(&event);
    assert_eq!(json["state_key"], "@alice:ex.com");
}

fn make_state_event(event_type: Option<&str>, state_key: Option<&str>) -> synapse_storage::StateEvent {
    synapse_storage::StateEvent {
        event_id: "ev1".to_string(),
        room_id: "!r:ex.com".to_string(),
        sender: "@sender:ex.com".to_string(),
        event_type: event_type.map(str::to_string),
        content: serde_json::Value::Null,
        state_key: state_key.map(str::to_string),
        unsigned: None,
        is_redacted: None,
        origin_server_ts: 1000,
        depth: None,
        processed_ts: None,
        not_before: None,
        status: None,
        reference_image: None,
        origin: Some("ex.com".to_string()),
        user_id: Some("@sender:ex.com".to_string()),
        stream_ordering: Some(1),
    }
}
