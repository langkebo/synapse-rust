use super::*;
use synapse_storage::device::DeviceStorage;
use synapse_storage::event::EventStorage;
use synapse_storage::membership::RoomMemberStorage;
use synapse_storage::sliding_sync::{SlidingSyncFilters, SlidingSyncRoom};

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
        typing_service: Arc::new(crate::typing_service::TypingService::default()),
        presence_storage: PresenceStorage::new(
            pool.clone(),
            Arc::new(CacheManager::new(&synapse_cache::CacheConfig::default())),
        ),
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
