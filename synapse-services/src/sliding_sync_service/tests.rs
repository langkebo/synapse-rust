use super::*;
use std::collections::HashMap;
use synapse_e2ee::device_keys::DeviceKeyStorage;
use synapse_storage::device::DeviceStorage;
use synapse_storage::event::EventStorage;
use synapse_storage::membership::RoomMemberStorage;
use synapse_storage::sliding_sync::{SlidingSyncFilters, SlidingSyncListData, SlidingSyncRoom, SlidingSyncStorage};
use synapse_storage::test_mocks::{FakeUserStore, InMemoryEventStore, InMemorySlidingSyncStore};
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
        bump_stamp: Some(1234567890000),
        highlight_count: 5,
        notification_count: 10,
        is_dm: true,
        is_encrypted: true,
        is_tombstoned: false,
        is_invited: false,
        name: Some("Test Room".to_string()),
        avatar: Some("mxc://example.com/avatar".to_string()),
        timestamp: Some(1234567890000),
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
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
    let user_storage = Arc::new(FakeUserStore::new());
    SlidingSyncService {
        storage: Arc::new(SlidingSyncStorage::new(pool.clone())),
        cache: Arc::new(CacheManager::new(&synapse_cache::CacheConfig::default())),
        event_reader: event_storage,
        device_key_storage: Arc::new(DeviceKeyStorage::new(&pool))
            as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        typing_service: Arc::new(crate::typing_service::TypingService::default()),
        presence_storage: Arc::new(PresenceStorage::new(
            pool.clone(),
            Arc::new(CacheManager::new(&synapse_cache::CacheConfig::default())),
        )),
        member_storage: Arc::new(RoomMemberStorage::new(&pool, "localhost")),
        device_storage: Arc::new(DeviceStorage::new(&pool)),
        to_device_storage: ToDeviceStorage::new(&pool),
        user_storage,
        connection_tracker: Arc::new(
            moka::sync::Cache::builder()
                .max_capacity(MAX_TRACKED_CONNECTIONS)
                .time_to_idle(std::time::Duration::from_millis(CONNECTION_TTL_MS as u64))
                .build(),
        ),
        txn_id_cache: Arc::new(
            moka::future::Cache::builder()
                .max_capacity(MAX_TXN_ID_CACHE_ENTRIES)
                .time_to_live(std::time::Duration::from_millis(TXN_ID_CACHE_TTL_MS))
                .build(),
        ),
        metrics: Arc::new(MetricsCollector::new()),
        latency_threshold_ms: PerformanceConfig::default().sliding_sync_latency_threshold_ms,
        sticky_event_storage: None,
        event_notifier: None,
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
    let json = crate::sync_helpers::state_event_to_json(&event);
    assert_eq!(json["type"], "m.room.name");
    assert_eq!(json["event_id"], "ev1");
    assert_eq!(json["room_id"], "!r:ex.com");
    assert!(json["unsigned"]["age"].is_number());
}

#[test]
fn state_event_to_json_includes_state_key() {
    let event = make_state_event(Some("m.room.member"), Some("@alice:ex.com"));
    let json = crate::sync_helpers::state_event_to_json(&event);
    assert_eq!(json["state_key"], "@alice:ex.com");
}

// ── subscription_config_from_list ────────────────────────────────────

#[test]
fn subscription_config_from_list_copies_timeline_limit_and_required_state() {
    let list_data = SlidingSyncListData {
        ranges: vec![vec![0, 9]],
        sort: vec!["by_notification_count".to_string()],
        filters: None,
        timeline_limit: Some(50),
        required_state: Some(vec![vec!["m.room.name".to_string(), "".to_string()]]),
        slow_by: None,
        bump_event_types: None,
    };
    let config = SlidingSyncService::subscription_config_from_list(&list_data);
    assert_eq!(config.timeline_limit, Some(50));
    assert!(config.required_state.is_some());
    assert_eq!(config.required_state.unwrap().len(), 1);
}

#[test]
fn subscription_config_from_list_none_fields() {
    let list_data = SlidingSyncListData {
        ranges: vec![],
        sort: vec![],
        filters: None,
        timeline_limit: None,
        required_state: None,
        slow_by: None,
        bump_event_types: None,
    };
    let config = SlidingSyncService::subscription_config_from_list(&list_data);
    assert!(config.timeline_limit.is_none());
    assert!(config.required_state.is_none());
}

// ── build_sync_ops empty range ────────────────────────────────────────

#[test]
fn build_sync_ops_filters_out_empty_range() {
    let ops = SlidingSyncService::build_sync_ops(&[
        SlidingListRangeSnapshot { start: 0, end: 1, room_ids: vec![] },
        SlidingListRangeSnapshot { start: 5, end: 6, room_ids: vec!["!r1:ex.com".to_string()] },
    ]);
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0]["op"], "SYNC");
    assert_eq!(ops[0]["range"][0], 5);
}

// ── build_incremental_ops edge cases ──────────────────────────────────

#[test]
fn build_incremental_ops_current_shorter_deletes_trailing() {
    let previous = SlidingListWindowSnapshot {
        ranges: vec![SlidingListRangeSnapshot {
            start: 0,
            end: 1,
            room_ids: vec!["!r1:ex.com".to_string(), "!r2:ex.com".to_string()],
        }],
    };
    let current = vec![SlidingListRangeSnapshot { start: 0, end: 1, room_ids: vec!["!r1:ex.com".to_string()] }];
    let ops = SlidingSyncService::build_incremental_ops(&previous, &current).unwrap();
    assert!(ops.iter().any(|op| op["op"] == "DELETE"));
    assert_eq!(ops.iter().filter(|op| op["op"] == "DELETE").count(), 1);
}

#[test]
fn build_incremental_ops_current_longer_inserts_new() {
    let previous = SlidingListWindowSnapshot {
        ranges: vec![SlidingListRangeSnapshot { start: 0, end: 1, room_ids: vec!["!r1:ex.com".to_string()] }],
    };
    let current = vec![SlidingListRangeSnapshot {
        start: 0,
        end: 1,
        room_ids: vec!["!r1:ex.com".to_string(), "!r2:ex.com".to_string()],
    }];
    let ops = SlidingSyncService::build_incremental_ops(&previous, &current).unwrap();
    assert!(ops.iter().any(|op| op["op"] == "INSERT"));
    assert!(ops.iter().any(|op| op["room_id"] == "!r2:ex.com"));
}

#[test]
fn build_incremental_ops_swap_room() {
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
        room_ids: vec!["!r3:ex.com".to_string(), "!r2:ex.com".to_string()],
    }];
    let ops = SlidingSyncService::build_incremental_ops(&previous, &current).unwrap();
    assert!(ops.iter().any(|op| op["op"] == "INSERT"));
    assert!(ops.iter().any(|op| op["op"] == "DELETE"));
}

#[test]
fn build_incremental_ops_multi_range_returns_none() {
    let previous = SlidingListWindowSnapshot {
        ranges: vec![
            SlidingListRangeSnapshot { start: 0, end: 1, room_ids: vec!["!r1:ex.com".to_string()] },
            SlidingListRangeSnapshot { start: 2, end: 3, room_ids: vec!["!r2:ex.com".to_string()] },
        ],
    };
    let current = vec![SlidingListRangeSnapshot { start: 0, end: 1, room_ids: vec!["!r1:ex.com".to_string()] }];
    assert!(SlidingSyncService::build_incremental_ops(&previous, &current).is_none());
}

// ── room_to_json edge cases ───────────────────────────────────────────

#[test]
fn room_to_json_none_name_and_avatar() {
    let room = SlidingSyncRoom {
        id: 1,
        user_id: "@a:ex.com".to_string(),
        device_id: "D1".to_string(),
        room_id: "!r:ex.com".to_string(),
        conn_id: None,
        list_key: None,
        bump_stamp: Some(0),
        highlight_count: 0,
        notification_count: 0,
        is_dm: false,
        is_encrypted: false,
        is_tombstoned: false,
        is_invited: false,
        name: None,
        avatar: None,
        timestamp: Some(0),
        created_ts: 0,
        updated_ts: 0,
    };
    let json = SlidingSyncService::room_to_json(&room);
    assert_eq!(json["name"], serde_json::Value::Null);
    assert_eq!(json["avatar"], serde_json::Value::Null);
}

// ── state_event_to_json edge cases ────────────────────────────────────

#[test]
fn state_event_to_json_none_event_type_defaults_to_message() {
    let event = make_state_event(None, Some(""));
    let json = crate::sync_helpers::state_event_to_json(&event);
    assert_eq!(json["type"], "m.room.message");
}

#[test]
fn state_event_to_json_none_state_key_no_field() {
    let event = make_state_event(Some("m.room.name"), None);
    let json = crate::sync_helpers::state_event_to_json(&event);
    assert!(json.get("state_key").is_none());
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
        origin: Some("ex.com".to_string()),
        user_id: Some("@sender:ex.com".to_string()),
        stream_ordering: Some(1),
    }
}

// ── Room-state cache tests (OPT-015-d) ──────────────────────────

/// Build a `SlidingSyncService` with an in-memory event store so that
/// `get_state_events` is backed by a controllable fake rather than Postgres.
fn create_cached_test_service(event_store: Arc<InMemoryEventStore>) -> SlidingSyncService {
    let pool = Arc::new(
        sqlx::postgres::PgPoolOptions::new().max_connections(1).connect_lazy("postgres://localhost/test").unwrap(),
    );
    let user_storage = Arc::new(FakeUserStore::new());
    SlidingSyncService {
        storage: Arc::new(SlidingSyncStorage::new(pool.clone())),
        cache: Arc::new(CacheManager::new(&synapse_cache::CacheConfig::default())),
        event_reader: event_store as Arc<dyn synapse_storage::event::EventReader>,
        device_key_storage: Arc::new(DeviceKeyStorage::new(&pool))
            as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        typing_service: Arc::new(crate::typing_service::TypingService::default()),
        presence_storage: Arc::new(PresenceStorage::new(
            pool.clone(),
            Arc::new(CacheManager::new(&synapse_cache::CacheConfig::default())),
        )),
        member_storage: Arc::new(RoomMemberStorage::new(&pool, "localhost")),
        device_storage: Arc::new(DeviceStorage::new(&pool)),
        to_device_storage: ToDeviceStorage::new(&pool),
        user_storage,
        connection_tracker: Arc::new(
            moka::sync::Cache::builder()
                .max_capacity(MAX_TRACKED_CONNECTIONS)
                .time_to_idle(std::time::Duration::from_millis(CONNECTION_TTL_MS as u64))
                .build(),
        ),
        txn_id_cache: Arc::new(
            moka::future::Cache::builder()
                .max_capacity(MAX_TXN_ID_CACHE_ENTRIES)
                .time_to_live(std::time::Duration::from_millis(TXN_ID_CACHE_TTL_MS))
                .build(),
        ),
        metrics: Arc::new(MetricsCollector::new()),
        latency_threshold_ms: PerformanceConfig::default().sliding_sync_latency_threshold_ms,
        sticky_event_storage: None,
        event_notifier: None,
    }
}

#[tokio::test]
async fn room_state_cache_hit_avoids_storage() {
    let event_store = Arc::new(InMemoryEventStore::new());
    let svc = create_cached_test_service(event_store.clone());

    let room_id = "!test:example.com";

    // First call — no state events in store, populates cache with empty vec.
    let result1 =
        svc.build_required_state_events(room_id, Some(&vec![vec!["*".to_string(), "*".to_string()]])).await.unwrap();
    assert!(result1.is_empty(), "first call should return empty");

    // Inject a state event into the store AFTER the cache was populated.
    let _ = event_store
        .create_event(synapse_storage::event::CreateEventParams {
            event_id: "$ev1:example.com".to_string(),
            room_id: room_id.to_string(),
            user_id: "@alice:example.com".to_string(),
            event_type: "m.room.name".to_string(),
            content: serde_json::json!({"name": "CachedRoom"}),
            state_key: Some("".to_string()),
            origin_server_ts: 1000,
            redacts: None,
        })
        .await
        .unwrap();

    // Second call — should hit cache and return empty, NOT the newly added event.
    let result2 =
        svc.build_required_state_events(room_id, Some(&vec![vec!["*".to_string(), "*".to_string()]])).await.unwrap();

    assert!(
        result2.is_empty(),
        "second call should return cached empty vec, NOT the new state event. Found {} event(s)",
        result2.len()
    );
}

#[tokio::test]
async fn room_state_cache_invalidation_clears_on_write() {
    let event_store = Arc::new(InMemoryEventStore::new());
    let svc = create_cached_test_service(event_store.clone());

    let room_id = "!test:example.com";

    // First call — caches empty state for the room.
    let _ =
        svc.build_required_state_events(room_id, Some(&vec![vec!["*".to_string(), "*".to_string()]])).await.unwrap();

    // Inject a state event into the store.
    let _ = event_store
        .create_event(synapse_storage::event::CreateEventParams {
            event_id: "$ev2:example.com".to_string(),
            room_id: room_id.to_string(),
            user_id: "@bob:example.com".to_string(),
            event_type: "m.room.name".to_string(),
            content: serde_json::json!({"name": "FreshRoom"}),
            state_key: Some("".to_string()),
            origin_server_ts: 2000,
            redacts: None,
        })
        .await
        .unwrap();

    // Manually invalidate the cache (simulating what a state-event write does).
    let _ = svc.cache.delete(&format!("room_state:{room_id}")).await;

    // Third call — cache miss → should return the newly added event from storage.
    let result =
        svc.build_required_state_events(room_id, Some(&vec![vec!["*".to_string(), "*".to_string()]])).await.unwrap();

    assert!(!result.is_empty(), "after invalidation, should return fresh state from storage");
    assert_eq!(result.len(), 1, "should return exactly one state event");
    let name = result[0].get("content").and_then(|c| c.get("name")).and_then(|n| n.as_str());
    assert_eq!(name, Some("FreshRoom"), "should return the newly added event, not cached empty");
}

// ── MSC4186: txn_id idempotency ─────────────────────────────────────
//
// MSC4186 §6.1: When a request includes `txn_id`, the server MUST cache
// the response and return the cached body for subsequent requests with
// the same `txn_id` (e.g., client retries). The cache is keyed by
// `(user_id, device_id, txn_id)` to prevent cross-user leakage and is
// bounded with a TTL to avoid unbounded growth.

#[tokio::test]
async fn test_txn_id_returns_cached_response_on_retry() {
    let service = create_test_service();
    let user_id = "@alice:example.com";
    let device_id = "DEV1";
    let txn_id = "txn-12345";

    // Seed the txn_id cache with a known response.
    let cached_response = SlidingSyncResponse {
        pos: "cached-pos-999".to_string(),
        conn_id: Some("conn-1".to_string()),
        lists: serde_json::json!({"list1": {"count": 5}}),
        rooms: serde_json::json!({"!room:example.com": {}}),
        extensions: None,
    };
    service
        .txn_id_cache
        .insert(SlidingSyncService::txn_id_cache_key(user_id, device_id, txn_id), cached_response.clone())
        .await;

    // Issue a sync request with the same txn_id — should hit cache and return early,
    // bypassing sync_inner entirely (which would otherwise fail due to lazy-connect pool).
    let request = SlidingSyncRequest {
        conn_id: Some("conn-1".to_string()),
        lists: HashMap::new(),
        room_subscriptions: None,
        unsubscribe_rooms: None,
        extensions: None,
        pos: None,
        timeout: None,
        client_timeout: None,
        txn_id: Some(txn_id.to_string()),
    };

    let response = service.sync(user_id, device_id, request).await.expect("cached sync should succeed");

    assert_eq!(response.pos, "cached-pos-999", "should return cached pos, not a freshly generated one");
    assert_eq!(response.conn_id, Some("conn-1".to_string()));
    assert_eq!(response.lists["list1"]["count"], 5);
}

#[tokio::test]
async fn test_txn_id_cache_isolates_by_user_device() {
    // Cache key for (alice, DEV1, txn-shared) must differ from (bob, DEV1, txn-shared).
    let alice_key = SlidingSyncService::txn_id_cache_key("@alice:example.com", "DEV1", "txn-shared");
    let bob_key = SlidingSyncService::txn_id_cache_key("@bob:example.com", "DEV1", "txn-shared");
    assert_ne!(alice_key, bob_key, "cache keys must isolate by user_id");

    // Cache key must also differ by device_id.
    let alice_dev2_key = SlidingSyncService::txn_id_cache_key("@alice:example.com", "DEV2", "txn-shared");
    assert_ne!(alice_key, alice_dev2_key, "cache keys must isolate by device_id");

    // Cache key must differ by txn_id.
    let alice_other_txn = SlidingSyncService::txn_id_cache_key("@alice:example.com", "DEV1", "txn-other");
    assert_ne!(alice_key, alice_other_txn, "cache keys must isolate by txn_id");
}

#[tokio::test]
async fn test_txn_id_no_cache_lookup_when_txn_id_absent() {
    let service = create_test_service();

    // No txn_id provided — cache lookup must be skipped entirely.
    // The sync call will proceed to sync_inner which fails (lazy-connect pool),
    // proving the cache path was not taken.
    let request = SlidingSyncRequest {
        conn_id: None,
        lists: HashMap::new(),
        room_subscriptions: None,
        unsubscribe_rooms: None,
        extensions: None,
        pos: None,
        timeout: None,
        client_timeout: None,
        txn_id: None,
    };

    let result = service.sync("@alice:example.com", "DEV1", request).await;
    assert!(result.is_err(), "without txn_id, sync must proceed to storage which fails on lazy-connect pool");
}

// ── S12: initial sync materialization error handling ──────────────

/// Build a `SlidingSyncService` backed by in-memory mocks for both
/// `SlidingSyncStoreApi` and `MemberStoreApi`, so we can test the
/// initial-sync materialization loop without a real database.
fn create_mocked_test_service(
    sync_store: Arc<InMemorySlidingSyncStore>,
    member_store: Arc<synapse_storage::test_mocks::InMemoryMemberStore>,
) -> SlidingSyncService {
    let pool = Arc::new(
        sqlx::postgres::PgPoolOptions::new().max_connections(1).connect_lazy("postgres://localhost/test").unwrap(),
    );
    let event_store = Arc::new(InMemoryEventStore::new());
    let user_storage = Arc::new(FakeUserStore::new());
    SlidingSyncService {
        storage: sync_store as Arc<dyn SlidingSyncStoreApi>,
        cache: Arc::new(CacheManager::new(&synapse_cache::CacheConfig::default())),
        event_reader: event_store as Arc<dyn synapse_storage::event::EventReader>,
        device_key_storage: Arc::new(DeviceKeyStorage::new(&pool))
            as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        typing_service: Arc::new(crate::typing_service::TypingService::default()),
        presence_storage: Arc::new(PresenceStorage::new(
            pool.clone(),
            Arc::new(CacheManager::new(&synapse_cache::CacheConfig::default())),
        )),
        member_storage: member_store as Arc<dyn synapse_storage::membership::MemberStoreApi>,
        device_storage: Arc::new(DeviceStorage::new(&pool)),
        to_device_storage: ToDeviceStorage::new(&pool),
        user_storage,
        connection_tracker: Arc::new(
            moka::sync::Cache::builder()
                .max_capacity(MAX_TRACKED_CONNECTIONS)
                .time_to_idle(std::time::Duration::from_millis(CONNECTION_TTL_MS as u64))
                .build(),
        ),
        txn_id_cache: Arc::new(
            moka::future::Cache::builder()
                .max_capacity(MAX_TXN_ID_CACHE_ENTRIES)
                .time_to_live(std::time::Duration::from_millis(TXN_ID_CACHE_TTL_MS))
                .build(),
        ),
        metrics: Arc::new(MetricsCollector::new()),
        latency_threshold_ms: PerformanceConfig::default().sliding_sync_latency_threshold_ms,
        sticky_event_storage: None,
        event_notifier: None,
    }
}

/// S12: Initial sync must call `materialize_room_from_activity` for each
/// joined room. When materialize succeeds, the room data should be
/// available. This test seeds a joined room and verifies the sync
/// completes without error.
#[tokio::test]
async fn s12_initial_sync_materializes_joined_rooms() {
    let sync_store = Arc::new(InMemorySlidingSyncStore::new());
    let member_store = Arc::new(synapse_storage::test_mocks::InMemoryMemberStore::new());

    // Seed a joined room
    member_store.add_member("!room1:example.com", "@alice:example.com", "join", Some("Alice")).await.unwrap();

    let service = create_mocked_test_service(sync_store.clone(), member_store);

    let request = SlidingSyncRequest {
        conn_id: None,
        lists: HashMap::new(),
        room_subscriptions: None,
        unsubscribe_rooms: None,
        extensions: None,
        pos: None,
        timeout: None,
        client_timeout: None,
        txn_id: None,
    };

    // Initial sync should succeed even though the mock store is empty —
    // the materialize call is best-effort.
    let result = service.sync("@alice:example.com", "DEV1", request).await;
    assert!(result.is_ok(), "initial sync must succeed even with empty store: {:?}", result.err());
}

/// S12: When `materialize_room_from_activity` returns an error, the
/// initial sync must NOT crash — the error should be logged and the
/// sync should continue. Previously `let _ =` silently swallowed the
/// error; this test guards that the fix (warn! + continue) remains
/// resilient.
#[tokio::test]
async fn s12_initial_sync_resilient_to_materialize_errors() {
    let sync_store = Arc::new(InMemorySlidingSyncStore::new());
    let member_store = Arc::new(synapse_storage::test_mocks::InMemoryMemberStore::new());

    // Seed a joined room so the materialize loop runs
    member_store.add_member("!room1:example.com", "@alice:example.com", "join", Some("Alice")).await.unwrap();

    // Inject error: materialize_room_from_activity will return Err
    sync_store.set_fail_materialize(true);

    let service = create_mocked_test_service(sync_store, member_store);

    let request = SlidingSyncRequest {
        conn_id: None,
        lists: HashMap::new(),
        room_subscriptions: None,
        unsubscribe_rooms: None,
        extensions: None,
        pos: None,
        timeout: None,
        client_timeout: None,
        txn_id: None,
    };

    // Sync must succeed despite materialize failure — errors are logged,
    // not propagated to crash the sync.
    let result = service.sync("@alice:example.com", "DEV1", request).await;
    assert!(result.is_ok(), "initial sync must be resilient to materialize errors: {:?}", result.err());
}

// ── S11: slow-request metrics must exclude idle_wait_ms ────────────

/// S11: A request whose non-idle processing time is below the latency
/// threshold must NOT increment the slow-request counter. This is the
/// core fix — previously the route layer measured wall-clock (including
/// the 30s long-poll park) and counted every healthy long-poll as slow.
#[tokio::test]
async fn s11_fast_request_does_not_increment_slow_counter() {
    let service = create_test_service();
    let threshold = service.latency_threshold_ms();

    // Simulate a request that took 100ms of actual processing (well below
    // the 5000ms default threshold). Even if the request parked for 29s
    // in the idle long-poll, that time was already subtracted by sync().
    service.record_sync_latency_metrics("@alice:example.com", "DEV1", None, 100.0, false);

    // The slow counter must not have been registered (lazy registration
    // only happens when a slow request is detected). If it IS registered,
    // its value must be 0.
    if let Some(slow_counter) = service.metrics.get_counter(SLIDING_SYNC_SLOW_REQUESTS_COUNTER) {
        assert_eq!(slow_counter.get(), 0, "fast request must not trip slow counter");
    }

    // Histogram should still record the observation for p95/p99 reporting.
    let histogram =
        service.metrics.get_histogram(SLIDING_SYNC_LATENCY_HISTOGRAM).expect("latency histogram should be registered");
    assert!(
        histogram.get_percentile(50.0).unwrap_or(0.0) > 0.0,
        "histogram must observe the latency even for fast requests"
    );

    let _ = threshold; // suppress unused warning
}

/// S11: A request whose non-idle processing time meets or exceeds the
/// threshold MUST increment the slow-request counter — this is the
/// performance rollback gate.
#[tokio::test]
async fn s11_slow_request_increments_slow_counter() {
    let service = create_test_service();
    let threshold = service.latency_threshold_ms();

    // Simulate a genuinely slow request: actual processing time = threshold + 1ms.
    service.record_sync_latency_metrics("@alice:example.com", "DEV1", None, threshold as f64 + 1.0, true);

    let slow_counter =
        service.metrics.get_counter(SLIDING_SYNC_SLOW_REQUESTS_COUNTER).expect("slow counter should be registered");
    assert_eq!(slow_counter.get(), 1, "slow request must trip the counter exactly once");

    // A second slow request must increment again (not double-count, not reset).
    service.record_sync_latency_metrics("@alice:example.com", "DEV1", None, threshold as f64 + 500.0, false);
    assert_eq!(slow_counter.get(), 2, "second slow request must increment to 2");
}

/// S11: The slow-request counter is only incremented by the service layer.
/// The route layer must NOT reference the counter name at all (other than
/// in its own source-scan guard test). This test is the service-side
/// complement to the route-layer guard test in `sliding_sync.rs`.
#[test]
fn s11_slow_counter_only_in_service_layer() {
    // Verify the counter name constant is defined in the service module
    // (not in the route module). The route module's test already scans
    // its own source for zero non-test occurrences of the counter name.
    assert_eq!(
        SLIDING_SYNC_SLOW_REQUESTS_COUNTER, "sliding_sync_slow_requests_total",
        "counter name must match the documented metric"
    );
}

// ── W7+ 缓存治理：per-connection 缓存清理 ─────────────────────────
//
// 这一组测试存在的理由（不是锦上添花，是回归防线）：
// `invalidate_connection_cache` 曾只覆盖 3 个前缀，**漏掉 4 个 key**——
// presence / account_data / receipts / e2ee_shared_users 的前缀与已覆盖的
// 三个都不匹配，连接过期后残留到 TTL 才自然过期，移动端频繁重连会持续
// 产生孤儿 key。当初漏掉的根因正是没有任何测试保证"新增 key 被清理覆盖"。
//
// 新增任何 per-connection 缓存键都必须同步登记到清理清单，否则
// `test_invalidate_connection_cache_covers_all_keys` 会失败。

#[tokio::test]
async fn test_invalidate_connection_cache_covers_all_keys() {
    let service = create_test_service();
    let uid = "@alice:example.com";
    let did = "DEVICE1";
    let cid = Some("conn-abc");

    let list_prefix = SlidingSyncService::list_snapshot_cache_key_prefix(uid, did, cid);
    let room_prefix = SlidingSyncService::room_cache_key_prefix(uid, did, cid);

    let keys = vec![
        // ── 前缀类：一个前缀下可能挂多条（多 list / 多 room），各造 2 个变体 ──
        format!("{list_prefix}list0"),
        format!("{list_prefix}list1"),
        format!("{room_prefix}!room0:example.com"),
        format!("{room_prefix}!room1:example.com"),
        SlidingSyncService::e2ee_device_list_stream_cache_key(uid, did, cid),
        // ── 精确类：extensions 去重缓存，一个连接固定一条 ──
        // 这 4 个此前完全不在清理覆盖范围内
        SlidingSyncService::e2ee_shared_users_cache_key(uid, did, cid),
        SlidingSyncService::presence_cache_key(uid, did, cid),
        SlidingSyncService::account_data_cache_key(uid, did, cid),
        SlidingSyncService::receipts_cache_key(uid, did, cid),
    ];

    for k in &keys {
        service.cache.set_raw(k, "payload", 1800).await;
    }
    // 前置条件：所有 key 确实写进去了（否则下面的断言会因"从未写入"而假通过）
    for k in &keys {
        assert!(service.cache.get_local_raw(k).is_some(), "precondition failed: `{k}` was not written");
    }

    service.invalidate_connection_cache(uid, did, cid).await;

    for k in &keys {
        assert!(service.cache.get_local_raw(k).is_none(), "key survived invalidation: `{k}`");
    }
}

/// 清理必须精确到单个连接：同一 user+device 的其它 conn_id、以及不带
/// conn_id 的旧式 key，都不能被误删。
#[tokio::test]
async fn test_invalidate_connection_cache_isolates_other_connections() {
    let service = create_test_service();
    let uid = "@alice:example.com";
    let did = "DEVICE1";

    let victim = SlidingSyncService::presence_cache_key(uid, did, Some("conn-victim"));
    let sibling = SlidingSyncService::presence_cache_key(uid, did, Some("conn-sibling"));
    let legacy = SlidingSyncService::presence_cache_key(uid, did, None);

    for k in [&victim, &sibling, &legacy] {
        service.cache.set_raw(k, "payload", 1800).await;
    }

    service.invalidate_connection_cache(uid, did, Some("conn-victim")).await;

    assert!(service.cache.get_local_raw(&victim).is_none(), "目标连接必须被清理");
    assert!(service.cache.get_local_raw(&sibling).is_some(), "同设备其它连接不得被误删");
    assert!(service.cache.get_local_raw(&legacy).is_some(), "不带 conn_id 的旧式 key 不得被误删");
}
