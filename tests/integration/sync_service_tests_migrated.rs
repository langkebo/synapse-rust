#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_federation::event_broadcaster::EventBroadcaster;
use synapse_rust::common::config::PerformanceConfig;
use synapse_rust::common::metrics::MetricsCollector;

use synapse_e2ee::device_keys::DeviceKeyStorage;
use synapse_e2ee::key_rotation::KeyRotationStorage;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::Validator;
use synapse_rust::e2ee::to_device::ToDeviceStorage;
use synapse_services::room::service::{CreateRoomConfig, RoomService};
use synapse_services::room::summary::RoomSummaryService;
use synapse_services::sync_service::SyncService;
use synapse_storage::device::DeviceStorage;
use synapse_storage::event::{CreateEventParams, EventStorage};
use synapse_storage::membership::RoomMemberStorage;
use synapse_storage::relations::RelationsStorage;
use synapse_storage::room::RoomStorage;
use synapse_storage::room_summary::RoomSummaryStorage;
use synapse_storage::sticky_event::StickyEventStorage;
use synapse_storage::user::UserStorage;
use synapse_storage::user::UserStore;
use synapse_storage::PresenceStorage;
use synapse_storage::{AccountDataStorage, CreateFilterRequest, FilterStorage, FilterStoreApi, RoomAccountDataStorage};

async fn create_test_user(pool: &Pool<Postgres>, user_id: &str, username: &str) {
    sqlx::query(
        r#"
            INSERT INTO users (user_id, username, created_ts)
            VALUES ($1, $2, $3)
            "#,
    )
    .bind(user_id)
    .bind(username)
    .bind(current_timestamp_millis())
    .execute(pool)
    .await
    .expect("Failed to create test user");
}

fn create_room_service(
    pool: &Arc<Pool<Postgres>>,
    room_storage: Arc<synapse_storage::room::RoomStorage>,
    member_storage: Arc<synapse_storage::membership::RoomMemberStorage>,
    event_storage: Arc<synapse_storage::event::EventStorage>,
    user_storage: Arc<dyn UserStore>,
) -> RoomService {
    let room_summary_storage = Arc::new(RoomSummaryStorage::new(pool));
    let room_summary_service =
        Arc::new(RoomSummaryService::new(room_summary_storage, event_storage.clone(), Some(member_storage.clone())));
    let cache = Arc::new(synapse_rust::cache::CacheManager::new(&synapse_rust::cache::CacheConfig::default()));
    // The real gate, not a test double — see `room_service_tests_migrated`.
    let invite_policy_gate = Arc::new(synapse_services::invite_blocklist_service::InviteBlocklistService::new(
        Arc::new(synapse_storage::invite_blocklist::InviteBlocklistStorage::new(pool.clone())),
        Arc::new(synapse_storage::account_data::AccountDataStorage::new(pool)),
    ));

    RoomService::new(synapse_services::room::service::RoomServiceConfig {
        room_storage,
        member_storage,
        event_reader: Some(event_storage.clone()),
        event_writer: Some(event_storage),
        room_tag_storage: Arc::new(synapse_storage::room_tag::RoomTagStorage::new(pool.clone())),
        user_storage,
        room_auth: Arc::new(synapse_services::auth::AuthService::new(
            pool,
            cache.clone(),
            Arc::new(synapse_rust::common::metrics::MetricsCollector::new()),
            &synapse_rust::common::config::SecurityConfig::default(),
            "localhost",
        )),
        room_summary_service,
        validator: Arc::new(Validator::default()),
        server_name: "localhost".to_string(),
        task_queue: None,
        relations_storage: Arc::new(RelationsStorage::new(pool)),
        event_broadcaster: Some(Arc::new(EventBroadcaster::new("localhost".to_string()))),
        app_service_manager: None,
        key_rotation_manager: None,
        federation_client: None,
        beacon_service: None,
        sticky_event_storage: Arc::new(StickyEventStorage::new(pool.clone())),
        cache,
        key_rotation_storage: None,
        db_pool: None,
        policy_service: None,
        invite_policy_gate,
        // No third-party rules are registered in these tests, so the gate takes
        // its zero-cost fast path on every write.
        event_admission_gate: Arc::new(synapse_services::test_mocks::FakeEventAdmissionGate::new()),
    })
}

#[tokio::test]
async fn test_sync_success() {
    let pool = crate::require_test_pool().await;
    create_test_user(&pool, "@alice:localhost", "alice").await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let presence_storage = Arc::new(PresenceStorage::new(pool.clone(), canonical_cache.clone()));
    let member_storage = Arc::new(RoomMemberStorage::new(&pool, "localhost"));
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
    let room_storage = Arc::new(RoomStorage::new(&pool));
    let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache));

    let room_service = create_room_service(
        &pool,
        room_storage.clone(),
        member_storage.clone(),
        event_storage.clone(),
        user_storage.clone(),
    );

    let sync_service = SyncService::new(
        presence_storage,
        member_storage,
        event_storage,
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(&pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    // Create a room and send a message
    let config = CreateRoomConfig { name: Some("Test Room".to_string()), ..Default::default() };
    let room_val = room_service.lifecycle.create_room("@alice:localhost", config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let content = json!({"msgtype": "m.text", "body": "Hello"});
    room_service.messaging.send_message(room_id, "@alice:localhost", "m.room.message", &content).await.unwrap();

    let result = sync_service.sync("@alice:localhost", None, 0, false, "online", None, None, false, false).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert!(val["rooms"]["join"].is_object());

    assert!(val["rooms"]["join"].as_object().unwrap().contains_key(room_id));
    let room_data = &val["rooms"]["join"][room_id];
    let events = room_data["timeline"]["events"].as_array().unwrap();
    assert!(events.iter().any(|event| { event["type"] == "m.room.message" && event["content"]["body"] == "Hello" }));
}

#[tokio::test]
async fn test_incremental_sync_does_not_replay_old_timeline() {
    let pool = crate::require_test_pool().await;
    create_test_user(&pool, "@alice:localhost", "alice").await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let presence_storage = Arc::new(PresenceStorage::new(pool.clone(), canonical_cache.clone()));
    let member_storage = Arc::new(RoomMemberStorage::new(&pool, "localhost"));
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
    let room_storage = Arc::new(RoomStorage::new(&pool));
    let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache));

    let room_service = create_room_service(
        &pool,
        room_storage.clone(),
        member_storage.clone(),
        event_storage.clone(),
        user_storage.clone(),
    );

    let sync_service = SyncService::new(
        presence_storage,
        member_storage,
        event_storage,
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(&pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    let config = CreateRoomConfig { name: Some("Incremental Room".to_string()), ..Default::default() };
    let room_val = room_service.lifecycle.create_room("@alice:localhost", config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap().to_string();

    let content = json!({"msgtype": "m.text", "body": "Hello once"});
    room_service.messaging.send_message(&room_id, "@alice:localhost", "m.room.message", &content).await.unwrap();

    let first_sync =
        sync_service.sync("@alice:localhost", None, 0, false, "offline", None, None, false, false).await.unwrap();
    let since = first_sync["next_batch"].as_str().unwrap().to_string();

    let second_sync = sync_service
        .sync("@alice:localhost", None, 0, false, "offline", None, Some(since.as_str()), false, false)
        .await
        .unwrap();

    let joined_rooms = second_sync["rooms"]["join"].as_object().unwrap();
    assert!(joined_rooms.is_empty(), "incremental sync should not replay unchanged rooms");
}

#[tokio::test]
async fn test_sync_offline_presence_overwrites_previous_presence_state() {
    let pool = crate::require_test_pool().await;
    create_test_user(&pool, "@alice:localhost", "alice").await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let presence_storage = Arc::new(PresenceStorage::new(pool.clone(), canonical_cache));
    let member_storage = Arc::new(RoomMemberStorage::new(&pool, "localhost"));
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));

    let sync_service = SyncService::new(
        presence_storage.clone(),
        member_storage,
        event_storage,
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(&pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    sync_service.sync("@alice:localhost", None, 0, false, "online", None, None, false, false).await.unwrap();
    sync_service.sync("@alice:localhost", None, 0, false, "offline", None, None, false, false).await.unwrap();

    let persisted: (String, Option<String>) =
        PresenceStorage::get_presence(&presence_storage, "@alice:localhost").await.unwrap().unwrap();
    assert_eq!(persisted.0, "offline");
}

#[tokio::test]
async fn test_sync_presence_events_reflect_persisted_presence_state() {
    let pool = crate::require_test_pool().await;
    create_test_user(&pool, "@alice:localhost", "alice").await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let presence_storage = Arc::new(PresenceStorage::new(pool.clone(), canonical_cache));
    let member_storage = Arc::new(RoomMemberStorage::new(&pool, "localhost"));
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));

    let sync_service = SyncService::new(
        presence_storage,
        member_storage,
        event_storage,
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(&pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    let response =
        sync_service.sync("@alice:localhost", None, 0, false, "unavailable", None, None, false, false).await.unwrap();

    let presence_events = response["presence"]["events"].as_array().unwrap();
    assert_eq!(presence_events.len(), 1);
    assert_eq!(presence_events[0]["type"], "m.presence");
    assert_eq!(presence_events[0]["sender"], "@alice:localhost");
    assert_eq!(presence_events[0]["content"]["presence"], "unavailable");
    assert_eq!(presence_events[0]["content"]["currently_active"], json!(false));
}

#[tokio::test]
async fn test_incremental_lazy_load_does_not_repeat_unchanged_non_member_state() {
    let pool = crate::require_test_pool().await;
    create_test_user(&pool, "@alice:localhost", "alice").await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let member_storage = Arc::new(RoomMemberStorage::new(&pool, "localhost"));
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
    let room_storage = Arc::new(RoomStorage::new(&pool));
    let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache));

    let room_service =
        create_room_service(&pool, room_storage.clone(), member_storage.clone(), event_storage.clone(), user_storage);

    let sync_service = SyncService::new(
        Arc::new(PresenceStorage::new(pool.clone(), cache.clone())),
        member_storage,
        event_storage,
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(&pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    DeviceStorage::new(&pool).create_device("ALICEDEVICE", "@alice:localhost", Some("Alice phone")).await.unwrap();

    let room_val = room_service
        .lifecycle
        .create_room(
            "@alice:localhost",
            CreateRoomConfig { name: Some("Lazy Delta Room".to_string()), ..Default::default() },
        )
        .await
        .unwrap();
    let room_id = room_val["room_id"].as_str().unwrap().to_string();

    room_service
        .messaging
        .send_message(
            &room_id,
            "@alice:localhost",
            "m.room.message",
            &json!({"msgtype": "m.text", "body": "First hello"}),
        )
        .await
        .unwrap();

    let filter = json!({
        "room": {
            "state": {
                "lazy_load_members": true
            }
        }
    })
    .to_string();

    let first_sync = sync_service
        .sync("@alice:localhost", Some("ALICEDEVICE"), 0, false, "online", Some(filter.as_str()), None, false, false)
        .await
        .unwrap();
    let first_state_events = first_sync["rooms"]["join"][&room_id]["state"]["events"].as_array().unwrap();
    let first_non_member_types: Vec<String> = first_state_events
        .iter()
        .filter_map(|event| {
            let event_type = event["type"].as_str()?;
            (event_type != "m.room.member").then(|| event_type.to_string())
        })
        .collect();
    assert!(!first_non_member_types.is_empty(), "initial sync should include at least one non-member state event");
    let since = first_sync["next_batch"].as_str().unwrap().to_string();

    room_service
        .messaging
        .send_message(
            &room_id,
            "@alice:localhost",
            "m.room.message",
            &json!({"msgtype": "m.text", "body": "Second hello"}),
        )
        .await
        .unwrap();

    let second_sync = sync_service
        .sync(
            "@alice:localhost",
            Some("ALICEDEVICE"),
            0,
            false,
            "online",
            Some(filter.as_str()),
            Some(since.as_str()),
            false,
            false,
        )
        .await
        .unwrap();

    let second_state_events = second_sync["rooms"]["join"][&room_id]["state"]["events"].as_array().unwrap();
    assert!(
        !second_state_events.iter().any(|event| {
            event["type"].as_str().is_some_and(|event_type| first_non_member_types.iter().any(|ty| ty == event_type))
        }),
        "incremental lazy-load sync should not repeat unchanged non-member state types"
    );
}

#[tokio::test]
async fn test_incremental_sync_includes_state_only_change_without_lazy_load() {
    let pool = crate::require_test_pool().await;
    create_test_user(&pool, "@alice:localhost", "alice").await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let member_storage = Arc::new(RoomMemberStorage::new(&pool, "localhost"));
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
    let room_storage = Arc::new(RoomStorage::new(&pool));
    let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache));

    let room_service =
        create_room_service(&pool, room_storage.clone(), member_storage.clone(), event_storage.clone(), user_storage);

    let sync_service = SyncService::new(
        Arc::new(PresenceStorage::new(pool.clone(), cache.clone())),
        member_storage,
        event_storage.clone(),
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(&pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    DeviceStorage::new(&pool).create_device("ALICEDEVICE", "@alice:localhost", Some("Alice phone")).await.unwrap();

    let room_val = room_service
        .lifecycle
        .create_room(
            "@alice:localhost",
            CreateRoomConfig { visibility: Some("public".to_string()), ..Default::default() },
        )
        .await
        .unwrap();
    let room_id = room_val["room_id"].as_str().unwrap().to_string();

    let filter = json!({
        "room": {
            "timeline": {
                "types": ["m.room.message"]
            }
        }
    })
    .to_string();

    let first_sync = sync_service
        .sync("@alice:localhost", Some("ALICEDEVICE"), 0, false, "online", Some(filter.as_str()), None, false, false)
        .await
        .unwrap();
    let since = first_sync["next_batch"].as_str().unwrap().to_string();

    let topic_ts = current_timestamp_millis() + 1_000;
    event_storage
        .create_event(
            CreateEventParams {
                event_id: "$topic_state_only_no_lazy:localhost".to_string(),
                room_id: room_id.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.topic".to_string(),
                content: json!({"topic": "State delta without lazy load"}),
                state_key: Some(String::new()),
                origin_server_ts: topic_ts,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    let second_sync = sync_service
        .sync(
            "@alice:localhost",
            Some("ALICEDEVICE"),
            0,
            false,
            "online",
            Some(filter.as_str()),
            Some(since.as_str()),
            false,
            false,
        )
        .await
        .unwrap();

    let second_room = &second_sync["rooms"]["join"][&room_id];
    assert!(
        second_room.is_object(),
        "room with state-only changes should be included in incremental sync without lazy-load"
    );
    let second_timeline_events = second_room["timeline"]["events"].as_array().unwrap();
    assert!(second_timeline_events.is_empty(), "timeline filter should still exclude state-only event from timeline");
    let second_state_events = second_room["state"]["events"].as_array().unwrap();
    assert!(
        second_state_events.iter().any(|event| {
            event["type"] == "m.room.topic" && event["content"]["topic"] == "State delta without lazy load"
        }),
        "incremental sync should include state delta even without lazy-load"
    );

    let second_since = second_sync["next_batch"].as_str().unwrap().to_string();
    assert_ne!(second_since, since, "state-only update should advance the sync token without lazy-load");

    let third_sync = sync_service
        .sync(
            "@alice:localhost",
            Some("ALICEDEVICE"),
            0,
            false,
            "online",
            Some(filter.as_str()),
            Some(second_since.as_str()),
            false,
            false,
        )
        .await
        .unwrap();
    assert!(
        third_sync["rooms"]["join"].get(&room_id).is_none(),
        "state-only update should not repeat after token advances without lazy-load"
    );
}

#[tokio::test]
async fn test_incremental_lazy_load_includes_room_with_state_only_change_despite_timeline_filter() {
    let pool = crate::require_test_pool().await;
    create_test_user(&pool, "@alice:localhost", "alice").await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let member_storage = Arc::new(RoomMemberStorage::new(&pool, "localhost"));
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
    let room_storage = Arc::new(RoomStorage::new(&pool));
    let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache));

    let room_service =
        create_room_service(&pool, room_storage.clone(), member_storage.clone(), event_storage.clone(), user_storage);

    let sync_service = SyncService::new(
        Arc::new(PresenceStorage::new(pool.clone(), cache.clone())),
        member_storage,
        event_storage.clone(),
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(&pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    DeviceStorage::new(&pool).create_device("ALICEDEVICE", "@alice:localhost", Some("Alice phone")).await.unwrap();

    let room_val = room_service.lifecycle.create_room("@alice:localhost", CreateRoomConfig::default()).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap().to_string();

    let filter = json!({
        "room": {
            "state": {
                "lazy_load_members": true
            },
            "timeline": {
                "types": ["m.room.message"]
            }
        }
    })
    .to_string();

    let first_sync = sync_service
        .sync("@alice:localhost", Some("ALICEDEVICE"), 0, false, "online", Some(filter.as_str()), None, false, false)
        .await
        .unwrap();
    let since = first_sync["next_batch"].as_str().unwrap().to_string();

    let topic_ts = current_timestamp_millis() + 1_000;
    event_storage
        .create_event(
            CreateEventParams {
                event_id: "$topic_state_only:localhost".to_string(),
                room_id: room_id.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.topic".to_string(),
                content: json!({"topic": "State only update"}),
                state_key: Some(String::new()),
                origin_server_ts: topic_ts,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    let second_sync = sync_service
        .sync(
            "@alice:localhost",
            Some("ALICEDEVICE"),
            0,
            false,
            "online",
            Some(filter.as_str()),
            Some(since.as_str()),
            false,
            false,
        )
        .await
        .unwrap();

    let second_room = &second_sync["rooms"]["join"][&room_id];
    assert!(second_room.is_object(), "room with state-only changes should be included in incremental sync");
    let second_timeline_events = second_room["timeline"]["events"].as_array().unwrap();
    assert!(second_timeline_events.is_empty(), "timeline filter should still exclude state-only event from timeline");
    let second_state_events = second_room["state"]["events"].as_array().unwrap();
    assert!(
        second_state_events
            .iter()
            .any(|event| { event["type"] == "m.room.topic" && event["content"]["topic"] == "State only update" }),
        "state-only change should still appear in state.events"
    );

    let second_since = second_sync["next_batch"].as_str().unwrap().to_string();
    assert_ne!(second_since, since, "state-only update should advance the sync token");

    let third_sync = sync_service
        .sync(
            "@alice:localhost",
            Some("ALICEDEVICE"),
            0,
            false,
            "online",
            Some(filter.as_str()),
            Some(second_since.as_str()),
            false,
            false,
        )
        .await
        .unwrap();
    assert!(
        third_sync["rooms"]["join"].get(&room_id).is_none(),
        "state-only update should not repeat after token advances"
    );
}

#[tokio::test]
async fn test_sync_timeline_limit_preserves_chronological_order_without_false_limited_flag() {
    let pool = crate::require_test_pool().await;
    create_test_user(&pool, "@alice:localhost", "alice").await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let member_storage = Arc::new(RoomMemberStorage::new(&pool, "localhost"));
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
    let room_storage = Arc::new(RoomStorage::new(&pool));
    let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache));

    let room_service =
        create_room_service(&pool, room_storage.clone(), member_storage.clone(), event_storage.clone(), user_storage);

    let sync_service = SyncService::new(
        Arc::new(PresenceStorage::new(pool.clone(), cache.clone())),
        member_storage,
        event_storage,
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(&pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    DeviceStorage::new(&pool).create_device("ALICEDEVICE", "@alice:localhost", Some("Alice phone")).await.unwrap();

    let room_val = room_service.lifecycle.create_room("@alice:localhost", CreateRoomConfig::default()).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap().to_string();

    room_service
        .messaging
        .send_message(
            &room_id,
            "@alice:localhost",
            "m.room.message",
            &json!({"msgtype": "m.text", "body": "First hello"}),
        )
        .await
        .unwrap();
    room_service
        .messaging
        .send_message(
            &room_id,
            "@alice:localhost",
            "m.room.message",
            &json!({"msgtype": "m.text", "body": "Second hello"}),
        )
        .await
        .unwrap();

    let filter = json!({
        "room": {
            "timeline": {
                "types": ["m.room.message"],
                "limit": 2
            }
        }
    })
    .to_string();

    let sync = sync_service
        .sync("@alice:localhost", Some("ALICEDEVICE"), 0, false, "online", Some(filter.as_str()), None, false, false)
        .await
        .unwrap();

    let room = &sync["rooms"]["join"][&room_id];
    let timeline_events = room["timeline"]["events"].as_array().unwrap();
    assert_eq!(
        room["timeline"]["limited"],
        json!(false),
        "timeline should only be marked limited when more events exist than the requested limit"
    );

    let bodies: Vec<&str> = timeline_events.iter().map(|event| event["content"]["body"].as_str().unwrap()).collect();
    assert_eq!(bodies, vec!["First hello", "Second hello"], "sync timeline should be returned in chronological order");
}

#[tokio::test]
async fn test_incremental_lazy_load_limited_timeline_does_not_replay_state_delta_members() {
    let pool = crate::require_test_pool().await;
    create_test_user(&pool, "@alice:localhost", "alice").await;
    create_test_user(&pool, "@bob:localhost", "bob").await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let member_storage = Arc::new(RoomMemberStorage::new(&pool, "localhost"));
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
    let room_storage = Arc::new(RoomStorage::new(&pool));
    let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache));

    let room_service =
        create_room_service(&pool, room_storage.clone(), member_storage.clone(), event_storage.clone(), user_storage);

    let sync_service = SyncService::new(
        Arc::new(PresenceStorage::new(pool.clone(), cache.clone())),
        member_storage,
        event_storage.clone(),
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(&pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    DeviceStorage::new(&pool).create_device("ALICEDEVICE", "@alice:localhost", Some("Alice phone")).await.unwrap();

    let room_val = room_service
        .lifecycle
        .create_room(
            "@alice:localhost",
            CreateRoomConfig { visibility: Some("public".to_string()), ..Default::default() },
        )
        .await
        .unwrap();
    let room_id = room_val["room_id"].as_str().unwrap().to_string();

    room_service.membership.join_room(&room_id, "@bob:localhost").await.unwrap();

    let base_ts = current_timestamp_millis();
    event_storage
        .create_event(
            CreateEventParams {
                event_id: "$alice_member_limited:localhost".to_string(),
                room_id: room_id.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.member".to_string(),
                content: json!({"membership": "join"}),
                state_key: Some("@alice:localhost".to_string()),
                origin_server_ts: base_ts,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();
    event_storage
        .create_event(
            CreateEventParams {
                event_id: "$bob_member_limited:localhost".to_string(),
                room_id: room_id.clone(),
                user_id: "@bob:localhost".to_string(),
                event_type: "m.room.member".to_string(),
                content: json!({"membership": "join"}),
                state_key: Some("@bob:localhost".to_string()),
                origin_server_ts: base_ts + 1,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    room_service
        .messaging
        .send_message(&room_id, "@bob:localhost", "m.room.message", &json!({"msgtype": "m.text", "body": "Warm cache"}))
        .await
        .unwrap();

    let filter = json!({
        "room": {
            "state": {
                "lazy_load_members": true
            },
            "timeline": {
                "limit": 1
            }
        }
    })
    .to_string();

    let first_sync = sync_service
        .sync("@alice:localhost", Some("ALICEDEVICE"), 0, false, "online", Some(filter.as_str()), None, false, false)
        .await
        .unwrap();
    let first_state_events = first_sync["rooms"]["join"][&room_id]["state"]["events"].as_array().unwrap();
    assert!(first_state_events
        .iter()
        .any(|event| { event["type"] == "m.room.member" && event["state_key"] == "@bob:localhost" }));
    let since = first_sync["next_batch"].as_str().unwrap().to_string();

    event_storage
        .create_event(
            CreateEventParams {
                event_id: "$bob_leave_limited:localhost".to_string(),
                room_id: room_id.clone(),
                user_id: "@bob:localhost".to_string(),
                event_type: "m.room.member".to_string(),
                content: json!({"membership": "leave"}),
                state_key: Some("@bob:localhost".to_string()),
                origin_server_ts: base_ts + 2,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    room_service
        .messaging
        .send_message(
            &room_id,
            "@alice:localhost",
            "m.room.message",
            &json!({"msgtype": "m.text", "body": "Newest message"}),
        )
        .await
        .unwrap();
    room_service
        .messaging
        .send_message(
            &room_id,
            "@alice:localhost",
            "m.room.message",
            &json!({"msgtype": "m.text", "body": "Older message"}),
        )
        .await
        .unwrap();

    let second_sync = sync_service
        .sync(
            "@alice:localhost",
            Some("ALICEDEVICE"),
            0,
            false,
            "online",
            Some(filter.as_str()),
            Some(since.as_str()),
            false,
            false,
        )
        .await
        .unwrap();

    let second_room = &second_sync["rooms"]["join"][&room_id];
    assert!(
        second_room["timeline"]["limited"] == json!(true),
        "timeline should be marked limited when more events exist than the requested limit"
    );
    let second_timeline_events = second_room["timeline"]["events"].as_array().unwrap();
    assert_eq!(second_timeline_events.len(), 1);
    assert!(
        second_timeline_events.iter().all(|event| event["sender"] == "@alice:localhost"),
        "returned limited timeline should only contain alice messages"
    );

    let second_state_events = second_room["state"]["events"].as_array().unwrap();
    assert!(
        !second_state_events
            .iter()
            .any(|event| { event["type"] == "m.room.member" && event["state_key"] == "@bob:localhost" }),
        "limited timeline should not replay state-delta membership changes that are outside the returned timeline"
    );
}

#[tokio::test]
async fn test_lazy_loaded_members_restore_from_db_after_service_restart() {
    let pool = crate::require_test_pool().await;
    create_test_user(&pool, "@alice:localhost", "alice").await;
    create_test_user(&pool, "@bob:localhost", "bob").await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let member_storage = Arc::new(RoomMemberStorage::new(&pool, "localhost"));
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
    let room_storage = Arc::new(RoomStorage::new(&pool));
    let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache));

    let room_service = create_room_service(
        &pool,
        room_storage.clone(),
        member_storage.clone(),
        event_storage.clone(),
        user_storage.clone(),
    );

    let sync_service = SyncService::new(
        Arc::new(PresenceStorage::new(pool.clone(), cache.clone())),
        member_storage.clone(),
        event_storage.clone(),
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(&pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    let device_storage = DeviceStorage::new(&pool);
    device_storage.create_device("ALICEDEVICE", "@alice:localhost", Some("Alice phone")).await.unwrap();

    let room_val = room_service
        .lifecycle
        .create_room(
            "@alice:localhost",
            CreateRoomConfig { visibility: Some("public".to_string()), ..Default::default() },
        )
        .await
        .unwrap();
    let room_id = room_val["room_id"].as_str().unwrap().to_string();

    room_service.membership.join_room(&room_id, "@bob:localhost").await.unwrap();

    let base_ts = current_timestamp_millis();
    event_storage
        .create_event(
            CreateEventParams {
                event_id: "$alice_member:localhost".to_string(),
                room_id: room_id.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.member".to_string(),
                content: json!({"membership": "join"}),
                state_key: Some("@alice:localhost".to_string()),
                origin_server_ts: base_ts,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();
    event_storage
        .create_event(
            CreateEventParams {
                event_id: "$bob_member:localhost".to_string(),
                room_id: room_id.clone(),
                user_id: "@bob:localhost".to_string(),
                event_type: "m.room.member".to_string(),
                content: json!({"membership": "join"}),
                state_key: Some("@bob:localhost".to_string()),
                origin_server_ts: base_ts + 1,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    room_service
        .messaging
        .send_message(
            &room_id,
            "@bob:localhost",
            "m.room.message",
            &json!({"msgtype": "m.text", "body": "First hello"}),
        )
        .await
        .unwrap();

    let filter = json!({
        "room": {
            "state": {
                "lazy_load_members": true
            }
        }
    })
    .to_string();

    let first_sync = sync_service
        .sync("@alice:localhost", Some("ALICEDEVICE"), 0, false, "online", Some(filter.as_str()), None, false, false)
        .await
        .unwrap();
    let since = first_sync["next_batch"].as_str().unwrap().to_string();
    let first_state_events = first_sync["rooms"]["join"][&room_id]["state"]["events"].as_array().unwrap();
    assert!(first_state_events
        .iter()
        .any(|event| { event["type"] == "m.room.member" && event["state_key"] == "@bob:localhost" }));

    let persisted_count: i64 = sqlx::query_scalar(
        r#"
            SELECT COUNT(*)
            FROM lazy_loaded_members
            WHERE user_id = $1 AND device_id = $2 AND room_id = $3
            "#,
    )
    .bind("@alice:localhost")
    .bind("ALICEDEVICE")
    .bind(&room_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert!(persisted_count >= 2, "expected persisted lazy-load members for alice and bob");

    let restarted_sync_service = SyncService::new(
        Arc::new(PresenceStorage::new(pool.clone(), cache.clone())),
        member_storage,
        event_storage.clone(),
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(&pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    room_service
        .messaging
        .send_message(
            &room_id,
            "@bob:localhost",
            "m.room.message",
            &json!({"msgtype": "m.text", "body": "Second hello"}),
        )
        .await
        .unwrap();

    let second_sync = restarted_sync_service
        .sync(
            "@alice:localhost",
            Some("ALICEDEVICE"),
            0,
            false,
            "online",
            Some(filter.as_str()),
            Some(since.as_str()),
            false,
            false,
        )
        .await
        .unwrap();

    let second_timeline_events = second_sync["rooms"]["join"][&room_id]["timeline"]["events"].as_array().unwrap();
    assert!(second_timeline_events
        .iter()
        .any(|event| { event["type"] == "m.room.message" && event["content"]["body"] == "Second hello" }));

    let second_state_events = second_sync["rooms"]["join"][&room_id]["state"]["events"].as_array().unwrap();
    assert!(
        !second_state_events
            .iter()
            .any(|event| { event["type"] == "m.room.member" && event["state_key"] == "@bob:localhost" }),
        "restarted sync service should restore lazy-load cache from database"
    );
}

#[tokio::test]
async fn test_include_redundant_members_survives_service_restart_with_persisted_cache() {
    let pool = crate::require_test_pool().await;
    create_test_user(&pool, "@alice:localhost", "alice").await;
    create_test_user(&pool, "@bob:localhost", "bob").await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let member_storage = Arc::new(RoomMemberStorage::new(&pool, "localhost"));
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
    let room_storage = Arc::new(RoomStorage::new(&pool));
    let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache));

    let room_service = create_room_service(
        &pool,
        room_storage.clone(),
        member_storage.clone(),
        event_storage.clone(),
        user_storage.clone(),
    );

    let sync_service = SyncService::new(
        Arc::new(PresenceStorage::new(pool.clone(), cache.clone())),
        member_storage.clone(),
        event_storage.clone(),
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(&pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    DeviceStorage::new(&pool).create_device("ALICEDEVICE", "@alice:localhost", Some("Alice phone")).await.unwrap();

    let room_val = room_service
        .lifecycle
        .create_room(
            "@alice:localhost",
            CreateRoomConfig { visibility: Some("public".to_string()), ..Default::default() },
        )
        .await
        .unwrap();
    let room_id = room_val["room_id"].as_str().unwrap().to_string();

    room_service.membership.join_room(&room_id, "@bob:localhost").await.unwrap();

    let base_ts = current_timestamp_millis();
    event_storage
        .create_event(
            CreateEventParams {
                event_id: "$alice_member_redundant:localhost".to_string(),
                room_id: room_id.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.member".to_string(),
                content: json!({"membership": "join"}),
                state_key: Some("@alice:localhost".to_string()),
                origin_server_ts: base_ts,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();
    event_storage
        .create_event(
            CreateEventParams {
                event_id: "$bob_member_redundant:localhost".to_string(),
                room_id: room_id.clone(),
                user_id: "@bob:localhost".to_string(),
                event_type: "m.room.member".to_string(),
                content: json!({"membership": "join"}),
                state_key: Some("@bob:localhost".to_string()),
                origin_server_ts: base_ts + 1,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    room_service
        .messaging
        .send_message(
            &room_id,
            "@bob:localhost",
            "m.room.message",
            &json!({"msgtype": "m.text", "body": "First hello"}),
        )
        .await
        .unwrap();

    let filter = json!({
        "room": {
            "state": {
                "lazy_load_members": true,
                "include_redundant_members": true
            }
        }
    })
    .to_string();

    let first_sync = sync_service
        .sync("@alice:localhost", Some("ALICEDEVICE"), 0, false, "online", Some(filter.as_str()), None, false, false)
        .await
        .unwrap();
    let since = first_sync["next_batch"].as_str().unwrap().to_string();

    let restarted_sync_service = SyncService::new(
        Arc::new(PresenceStorage::new(pool.clone(), cache.clone())),
        member_storage,
        event_storage,
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(&pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    room_service
        .messaging
        .send_message(
            &room_id,
            "@bob:localhost",
            "m.room.message",
            &json!({"msgtype": "m.text", "body": "Second hello"}),
        )
        .await
        .unwrap();

    let second_sync = restarted_sync_service
        .sync(
            "@alice:localhost",
            Some("ALICEDEVICE"),
            0,
            false,
            "online",
            Some(filter.as_str()),
            Some(since.as_str()),
            false,
            false,
        )
        .await
        .unwrap();

    let second_state_events = second_sync["rooms"]["join"][&room_id]["state"]["events"].as_array().unwrap();
    assert!(
        second_state_events
            .iter()
            .any(|event| { event["type"] == "m.room.member" && event["state_key"] == "@bob:localhost" }),
        "include_redundant_members should keep member state even after cache restore"
    );
}

#[tokio::test]
async fn test_stored_filter_id_restores_lazy_loaded_cache_after_service_restart() {
    let pool = crate::require_test_pool().await;
    create_test_user(&pool, "@alice:localhost", "alice").await;
    create_test_user(&pool, "@bob:localhost", "bob").await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let member_storage = Arc::new(RoomMemberStorage::new(&pool, "localhost"));
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
    let room_storage = Arc::new(RoomStorage::new(&pool));
    let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache));
    let filter_storage: Arc<dyn FilterStoreApi> = Arc::new(FilterStorage::new(&pool));

    filter_storage
        .create_filter(CreateFilterRequest {
            user_id: "@alice:localhost".to_string(),
            filter_id: "lazy-load-filter".to_string(),
            content: json!({
                "room": {
                    "state": {
                        "lazy_load_members": true
                    }
                }
            }),
        })
        .await
        .unwrap();

    let room_service = create_room_service(
        &pool,
        room_storage.clone(),
        member_storage.clone(),
        event_storage.clone(),
        user_storage.clone(),
    );

    let sync_service = SyncService::new(
        Arc::new(PresenceStorage::new(pool.clone(), cache.clone())),
        member_storage.clone(),
        event_storage.clone(),
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(&pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    DeviceStorage::new(&pool).create_device("ALICEDEVICE", "@alice:localhost", Some("Alice phone")).await.unwrap();

    let room_val = room_service
        .lifecycle
        .create_room(
            "@alice:localhost",
            CreateRoomConfig { visibility: Some("public".to_string()), ..Default::default() },
        )
        .await
        .unwrap();
    let room_id = room_val["room_id"].as_str().unwrap().to_string();

    room_service.membership.join_room(&room_id, "@bob:localhost").await.unwrap();

    let base_ts = current_timestamp_millis();
    event_storage
        .create_event(
            CreateEventParams {
                event_id: "$alice_member_saved_filter:localhost".to_string(),
                room_id: room_id.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.member".to_string(),
                content: json!({"membership": "join"}),
                state_key: Some("@alice:localhost".to_string()),
                origin_server_ts: base_ts,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();
    event_storage
        .create_event(
            CreateEventParams {
                event_id: "$bob_member_saved_filter:localhost".to_string(),
                room_id: room_id.clone(),
                user_id: "@bob:localhost".to_string(),
                event_type: "m.room.member".to_string(),
                content: json!({"membership": "join"}),
                state_key: Some("@bob:localhost".to_string()),
                origin_server_ts: base_ts + 1,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    room_service
        .messaging
        .send_message(
            &room_id,
            "@bob:localhost",
            "m.room.message",
            &json!({"msgtype": "m.text", "body": "First hello"}),
        )
        .await
        .unwrap();

    let first_sync = sync_service
        .sync("@alice:localhost", Some("ALICEDEVICE"), 0, false, "online", Some("lazy-load-filter"), None, false, false)
        .await
        .unwrap();
    let since = first_sync["next_batch"].as_str().unwrap().to_string();

    let restarted_sync_service = SyncService::new(
        Arc::new(PresenceStorage::new(pool.clone(), cache.clone())),
        member_storage,
        event_storage,
        Arc::new(RoomAccountDataStorage::new(&pool)),
        Arc::new(AccountDataStorage::new(&pool)),
        Arc::new(FilterStorage::new(&pool)),
        Arc::new(DeviceStorage::new(&pool)),
        Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(&pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
        Arc::new(CacheManager::new(&CacheConfig::default())),
        None,
    );

    room_service
        .messaging
        .send_message(
            &room_id,
            "@bob:localhost",
            "m.room.message",
            &json!({"msgtype": "m.text", "body": "Second hello"}),
        )
        .await
        .unwrap();

    let second_sync = restarted_sync_service
        .sync(
            "@alice:localhost",
            Some("ALICEDEVICE"),
            0,
            false,
            "online",
            Some("lazy-load-filter"),
            Some(since.as_str()),
            false,
            false,
        )
        .await
        .unwrap();

    let second_state_events = second_sync["rooms"]["join"][&room_id]["state"]["events"].as_array().unwrap();
    assert!(
        !second_state_events
            .iter()
            .any(|event| { event["type"] == "m.room.member" && event["state_key"] == "@bob:localhost" }),
        "stored filter id should resolve lazy-load settings and restore cache after restart"
    );
}
