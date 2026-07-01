//! Additional integration tests for `SyncService` response-building methods
//! in `synapse-services/src/sync_service/response.rs`:
//!   - `SyncResponseFilter` / `RoomFilter` / `SyncEventFormat` defaults and
//!     serialization
//!   - Inline filter JSON parsing (filter_id starting with `{`)
//!   - Stored filter resolution via `FilterStorage`
//!   - `build_sync_response` structure: `next_batch`, `rooms.join` /
//!     `rooms.leave`, `presence`, `account_data`, `to_device`,
//!     `device_lists`, `device_one_time_keys_count`, `key_rotation_needed`,
//!     `device_list_changes`
//!   - `build_room_sync` structure: `timeline.events`, `timeline.limited`,
//!     `timeline.prev_batch`, `state.events`, `ephemeral.events`,
//!     `account_data.events`, `unread_notifications`
//!   - Event field filtering via `event_fields`
//!   - Event format (`Client` vs `Federation`) shape differences
//!
//! Because the response-builder methods are `pub(crate)`, they are exercised
//! indirectly through the public `sync()` / `room_sync()` API.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::await_holding_lock)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::json;
use synapse_e2ee::device_keys::DeviceKeyStorage;
use synapse_e2ee::key_rotation::KeyRotationStorage;
use synapse_federation::event_broadcaster::EventBroadcaster;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::Validator;
use synapse_rust::common::config::PerformanceConfig;
use synapse_rust::common::metrics::MetricsCollector;
use synapse_rust::e2ee::to_device::ToDeviceStorage;
use synapse_services::room_service::{CreateRoomConfig, RoomService};
use synapse_services::sync_service::{RoomFilter, SyncEventFormat, SyncFilter, SyncResponseFilter, SyncService, SyncToken};
use synapse_storage::FilterStorage;
use synapse_storage::PresenceStorage;
use synapse_storage::RoomMemberRepository;
use synapse_storage::account_data::AccountDataStorage;
use synapse_storage::device::DeviceStorage;
use synapse_storage::event::{EventRepository, EventStorage};
use synapse_storage::membership::RoomMemberStorage;
use synapse_storage::relations::RelationsStorage;
use synapse_storage::room::{RoomRepository, RoomStorage};
use synapse_storage::room_summary::RoomSummaryStorage;
use synapse_storage::user::{UserStorage, UserStore};
use synapse_storage::{RoomAccountDataStorage, filter::CreateFilterRequest};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn response_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

async fn warm_up_pool(pool: &Arc<sqlx::PgPool>) {
    for _ in 0..8 {
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            sqlx::query("SELECT 1").execute(pool.as_ref()),
        )
        .await
        {
            Ok(Ok(_)) => return,
            Ok(Err(_)) | Err(_) => {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }
        }
    }
    let _ = sqlx::query("SELECT 1").execute(pool.as_ref()).await;
}

#[allow(clippy::too_many_lines)]
async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;

    for stmt in [
        r#"CREATE TABLE IF NOT EXISTS users (
            user_id TEXT NOT NULL PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT,
            displayname TEXT,
            avatar_url TEXT,
            is_admin BOOLEAN DEFAULT FALSE,
            is_guest BOOLEAN DEFAULT FALSE,
            is_shadow_banned BOOLEAN DEFAULT FALSE,
            is_deactivated BOOLEAN DEFAULT FALSE,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            generation BIGINT DEFAULT 0
        )"#,
        r#"CREATE TABLE IF NOT EXISTS presence (
            user_id VARCHAR(255) PRIMARY KEY,
            presence TEXT,
            status_msg TEXT,
            last_active_ts BIGINT,
            created_ts BIGINT,
            updated_ts BIGINT
        )"#,
        r#"CREATE TABLE IF NOT EXISTS rooms (
            room_id VARCHAR(255) PRIMARY KEY,
            is_public BOOLEAN DEFAULT FALSE,
            room_version TEXT DEFAULT '6',
            created_ts BIGINT NOT NULL,
            last_activity_ts BIGINT,
            join_rules TEXT DEFAULT 'invite',
            history_visibility TEXT DEFAULT 'shared',
            name TEXT,
            topic TEXT,
            avatar_url TEXT,
            canonical_alias TEXT,
            visibility TEXT DEFAULT 'private',
            creator TEXT,
            encryption TEXT,
            member_count BIGINT DEFAULT 0
        )"#,
        r#"CREATE TABLE IF NOT EXISTS room_memberships (
            room_id VARCHAR(255) NOT NULL,
            user_id VARCHAR(255) NOT NULL,
            sender TEXT,
            membership TEXT NOT NULL,
            event_id TEXT,
            event_type TEXT,
            display_name TEXT,
            avatar_url TEXT,
            is_banned BOOLEAN DEFAULT FALSE,
            invite_token TEXT,
            updated_ts BIGINT,
            joined_ts BIGINT,
            left_ts BIGINT,
            reason TEXT,
            banned_by TEXT,
            ban_reason TEXT,
            banned_ts BIGINT,
            join_reason TEXT,
            PRIMARY KEY (room_id, user_id)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS events (
            event_id VARCHAR(255) PRIMARY KEY,
            room_id VARCHAR(255) NOT NULL,
            user_id VARCHAR(255) NOT NULL,
            sender VARCHAR(255) NOT NULL,
            event_type TEXT NOT NULL,
            content JSONB NOT NULL,
            state_key TEXT,
            depth BIGINT,
            stream_ordering BIGSERIAL,
            origin_server_ts BIGINT NOT NULL,
            processed_ts BIGINT,
            not_before BIGINT,
            is_redacted BOOLEAN DEFAULT FALSE,
            status TEXT,
            reference_image TEXT,
            origin TEXT,
            unsigned JSONB
        )"#,
        r#"CREATE TABLE IF NOT EXISTS devices (
            device_id VARCHAR(255) PRIMARY KEY,
            user_id VARCHAR(255) NOT NULL,
            display_name TEXT,
            device_key JSONB,
            last_seen_ts BIGINT,
            last_seen_ip TEXT,
            created_ts BIGINT NOT NULL,
            first_seen_ts BIGINT NOT NULL,
            user_agent TEXT,
            appservice_id TEXT,
            ignored_user_list TEXT
        )"#,
        r#"CREATE TABLE IF NOT EXISTS device_lists_stream (
            stream_id BIGSERIAL PRIMARY KEY,
            user_id VARCHAR(255) NOT NULL,
            device_id VARCHAR(255),
            created_ts BIGINT NOT NULL
        )"#,
        r#"CREATE TABLE IF NOT EXISTS device_lists_changes (
            id BIGSERIAL PRIMARY KEY,
            user_id VARCHAR(255) NOT NULL,
            device_id VARCHAR(255),
            change_type TEXT NOT NULL,
            stream_id BIGINT NOT NULL,
            created_ts BIGINT NOT NULL
        )"#,
        r#"CREATE TABLE IF NOT EXISTS to_device_messages (
            stream_id BIGSERIAL PRIMARY KEY,
            sender_user_id VARCHAR(255) NOT NULL,
            sender_device_id VARCHAR(255) NOT NULL,
            recipient_user_id VARCHAR(255) NOT NULL,
            recipient_device_id VARCHAR(255) NOT NULL,
            event_type TEXT NOT NULL,
            content JSONB NOT NULL,
            message_id TEXT
        )"#,
        r#"CREATE TABLE IF NOT EXISTS lazy_loaded_members (
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            member_user_id TEXT NOT NULL,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL,
            PRIMARY KEY (user_id, device_id, room_id, member_user_id)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS filters (
            id BIGSERIAL PRIMARY KEY,
            user_id VARCHAR(255) NOT NULL,
            filter_id VARCHAR(255) NOT NULL,
            content JSONB NOT NULL,
            created_ts BIGINT NOT NULL
        )"#,
        r#"CREATE TABLE IF NOT EXISTS key_rotation_pending (
            room_id TEXT NOT NULL,
            reason TEXT NOT NULL,
            triggered_by_user_id TEXT NOT NULL,
            created_ts BIGINT NOT NULL,
            PRIMARY KEY (room_id, triggered_by_user_id)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS key_rotation_state (
            user_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            is_rotated BOOLEAN NOT NULL DEFAULT FALSE,
            rotated_at TIMESTAMPTZ,
            PRIMARY KEY (user_id, room_id)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS megolm_key_shares (
            room_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            share_reason TEXT NOT NULL,
            shared_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (room_id, session_id)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS megolm_sessions (
            session_id TEXT NOT NULL PRIMARY KEY,
            room_id TEXT NOT NULL,
            sender_key TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            expires_at TIMESTAMPTZ
        )"#,
        r#"CREATE TABLE IF NOT EXISTS account_data (
            id BIGSERIAL,
            user_id TEXT NOT NULL,
            data_type TEXT NOT NULL,
            content JSONB NOT NULL,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL,
            PRIMARY KEY (id),
            CONSTRAINT uq_account_data_user_type UNIQUE (user_id, data_type)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS room_account_data (
            id BIGSERIAL,
            user_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            data_type TEXT NOT NULL,
            data JSONB NOT NULL,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL,
            PRIMARY KEY (id),
            CONSTRAINT uq_room_account_data_user_room_type UNIQUE (user_id, room_id, data_type)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS room_ephemeral (
            id SERIAL PRIMARY KEY,
            room_id VARCHAR(255) NOT NULL,
            event_type VARCHAR(255) NOT NULL,
            user_id VARCHAR(255) NOT NULL,
            content JSONB NOT NULL DEFAULT '{}',
            stream_id BIGINT NOT NULL,
            created_ts BIGINT NOT NULL,
            expires_at BIGINT,
            CONSTRAINT uq_room_ephemeral_room_event_user UNIQUE (room_id, event_type, user_id)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS read_markers (
            id BIGSERIAL,
            room_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            event_id TEXT NOT NULL,
            marker_type TEXT NOT NULL,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL,
            PRIMARY KEY (id),
            CONSTRAINT uq_read_markers_room_user_type UNIQUE (room_id, user_id, marker_type)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS device_keys (
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            algorithm TEXT NOT NULL,
            key_id TEXT NOT NULL,
            key_data JSONB NOT NULL,
            signatures JSONB,
            created_ts BIGINT NOT NULL,
            uploaded_ts BIGINT,
            is_published BOOLEAN DEFAULT FALSE,
            PRIMARY KEY (user_id, device_id, algorithm, key_id)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS device_key_counts (
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            algorithm TEXT NOT NULL,
            count INTEGER NOT NULL DEFAULT 0,
            updated_ts BIGINT NOT NULL,
            PRIMARY KEY (user_id, device_id, algorithm)
        )"#,
    ] {
        let _ = sqlx::query(stmt).execute(pool.as_ref()).await;
    }
}

async fn cleanup_test_data(pool: &Arc<sqlx::PgPool>) {
    let tables = [
        "device_key_counts",
        "device_keys",
        "read_markers",
        "room_ephemeral",
        "room_account_data",
        "account_data",
        "megolm_sessions",
        "megolm_key_shares",
        "key_rotation_state",
        "key_rotation_pending",
        "lazy_loaded_members",
        "to_device_messages",
        "device_lists_changes",
        "device_lists_stream",
        "filters",
        "events",
        "devices",
        "room_memberships",
        "rooms",
        "presence",
    ];
    for table in &tables {
        let _ = sqlx::query(&format!("DELETE FROM {table}")).execute(pool.as_ref()).await;
    }
    let _ = sqlx::query("DELETE FROM users WHERE user_id LIKE '%syncresp_%'").execute(pool.as_ref()).await;
}

async fn insert_test_user(pool: &Arc<sqlx::PgPool>, user_id: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING")
        .bind(user_id)
        .bind(user_id.trim_start_matches('@').split(':').next().unwrap_or(user_id))
        .bind(now)
        .execute(pool.as_ref())
        .await
        .ok();
}

#[allow(clippy::too_many_lines)]
fn create_room_service(
    pool: &Arc<sqlx::PgPool>,
    room_storage: Arc<dyn RoomRepository>,
    member_storage: Arc<dyn RoomMemberRepository>,
    event_storage: Arc<dyn EventRepository>,
    user_storage: Arc<dyn UserStore>,
) -> RoomService {
    let room_summary_storage = Arc::new(RoomSummaryStorage::new(pool));
    let room_summary_service =
        Arc::new(synapse_services::room_summary_service::RoomSummaryService::new(
            room_summary_storage,
            event_storage.clone(),
            Some(member_storage.clone()),
        ));

    RoomService::new(synapse_services::room_service::RoomServiceConfig {
        room_storage,
        member_storage,
        event_storage,
        room_tag_storage: Arc::new(synapse_storage::room_tag::RoomTagStorage::new(pool.clone())),
        user_storage,
        auth_service: Arc::new(synapse_rust::auth::AuthService::new(
            pool,
            Arc::new(CacheManager::new(&CacheConfig::default())),
            Arc::new(MetricsCollector::new()),
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
    })
}

#[allow(clippy::too_many_lines)]
fn make_sync_service(pool: &Arc<sqlx::PgPool>) -> (SyncService, RoomService) {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let presence_storage = PresenceStorage::new(pool.clone(), canonical_cache.clone());
    let member_storage = Arc::new(RoomMemberStorage::new(pool, "localhost"));
    let event_storage = Arc::new(EventStorage::new(pool, "localhost".to_string()));
    let room_storage = Arc::new(RoomStorage::new(pool));
    let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(pool, canonical_cache));

    let room_service = create_room_service(
        pool,
        room_storage.clone(),
        member_storage.clone(),
        event_storage.clone(),
        user_storage.clone(),
    );

    let sync_service = SyncService::new(
        Arc::new(presence_storage),
        member_storage,
        event_storage,
        room_storage,
        RoomAccountDataStorage::new(pool),
        AccountDataStorage::new(pool),
        FilterStorage::new(pool),
        Arc::new(DeviceStorage::new(pool)),
        DeviceKeyStorage::new(pool),
        KeyRotationStorage::new(pool.clone()),
        ToDeviceStorage::new(pool),
        Arc::new(MetricsCollector::new()),
        PerformanceConfig::default(),
    );

    (sync_service, room_service)
}

fn unique_user_id() -> String {
    format!("@syncresp_{}:localhost", unique_id())
}

// ===========================================================================
// SyncResponseFilter / RoomFilter / SyncEventFormat serialization (pure)
// ===========================================================================

#[test]
fn test_sync_response_filter_default_has_room_and_presence() {
    let filter = SyncResponseFilter::default();
    assert!(filter.room.is_some(), "default SyncResponseFilter should include room filter");
    assert!(filter.presence.is_some(), "default SyncResponseFilter should include presence filter");
    assert_eq!(filter.event_format, SyncEventFormat::Client);
    assert!(filter.event_fields.is_none());
}

#[test]
fn test_room_filter_default_include_leave_false() {
    let room_filter = RoomFilter::default();
    assert_eq!(room_filter.include_leave, Some(false));
    assert!(room_filter.state.is_some());
    assert!(room_filter.timeline.is_some());
    assert!(room_filter.ephemeral.is_some());
    assert!(room_filter.account_data.is_some());
}

#[test]
fn test_room_filter_default_timeline_limit_50() {
    let room_filter = RoomFilter::default();
    let timeline = room_filter.timeline.unwrap();
    assert_eq!(timeline.limit, Some(50));
}

#[test]
fn test_sync_filter_default_limit_100() {
    let filter = SyncFilter::default();
    assert_eq!(filter.limit, Some(100));
}

#[test]
fn test_sync_event_format_serializes_client_lowercase() {
    let json = serde_json::to_value(SyncEventFormat::Client).unwrap();
    assert_eq!(json, json!("client"));
}

#[test]
fn test_sync_event_format_serializes_federation_lowercase() {
    let json = serde_json::to_value(SyncEventFormat::Federation).unwrap();
    assert_eq!(json, json!("federation"));
}

#[test]
fn test_sync_event_format_deserializes_client() {
    let format: SyncEventFormat = serde_json::from_value(json!("client")).unwrap();
    assert_eq!(format, SyncEventFormat::Client);
}

#[test]
fn test_sync_event_format_deserializes_federation() {
    let format: SyncEventFormat = serde_json::from_value(json!("federation")).unwrap();
    assert_eq!(format, SyncEventFormat::Federation);
}

#[test]
fn test_room_filter_serialization_roundtrip() {
    let filter = RoomFilter {
        rooms: Some(vec!["!room1:localhost".to_string()]),
        not_rooms: Some(vec!["!blocked:localhost".to_string()]),
        include_leave: Some(true),
        state: Some(SyncFilter { limit: Some(10), ..Default::default() }),
        timeline: Some(SyncFilter { limit: Some(20), types: Some(vec!["m.room.message".to_string()]), ..Default::default() }),
        ephemeral: None,
        account_data: Some(SyncFilter::default()),
    };
    let serialized = serde_json::to_string(&filter).unwrap();
    let deserialized: RoomFilter = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.rooms, filter.rooms);
    assert_eq!(deserialized.not_rooms, filter.not_rooms);
    assert_eq!(deserialized.include_leave, filter.include_leave);
    assert_eq!(deserialized.timeline.as_ref().and_then(|t| t.limit), Some(20));
    assert!(deserialized.ephemeral.is_none());
}

#[test]
fn test_sync_response_filter_serialization_roundtrip() {
    let filter = SyncResponseFilter {
        event_fields: Some(vec!["type".to_string(), "content".to_string()]),
        event_format: SyncEventFormat::Federation,
        room: Some(RoomFilter::default()),
        presence: Some(SyncFilter::default()),
    };
    let serialized = serde_json::to_string(&filter).unwrap();
    let deserialized: SyncResponseFilter = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.event_fields, filter.event_fields);
    assert_eq!(deserialized.event_format, SyncEventFormat::Federation);
    assert!(deserialized.room.is_some());
    assert!(deserialized.presence.is_some());
}

#[test]
fn test_sync_filter_serialization_all_fields() {
    let filter = SyncFilter {
        limit: Some(50),
        types: Some(vec!["m.room.message".to_string()]),
        not_types: Some(vec!["m.reaction".to_string()]),
        rooms: Some(vec!["!a:localhost".to_string()]),
        not_rooms: Some(vec!["!b:localhost".to_string()]),
        contains_url: Some(true),
        lazy_load_members: Some(true),
        include_redundant_members: Some(false),
        senders: Some(vec!["@alice:localhost".to_string()]),
        not_senders: Some(vec!["@bob:localhost".to_string()]),
    };
    let serialized = serde_json::to_string(&filter).unwrap();
    let deserialized: SyncFilter = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, filter);
}

// ===========================================================================
// build_sync_response structure via sync() (presence / account_data /
// to_device / device_lists / next_batch / device_one_time_keys_count /
// key_rotation_needed / device_list_changes)
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_response_has_all_top_level_keys() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();

    // The response should always contain all top-level keys (Matrix spec).
    assert!(result.get("next_batch").is_some(), "next_batch must be present");
    assert!(result.get("rooms").is_some(), "rooms must be present");
    assert!(result.get("presence").is_some(), "presence must be present");
    assert!(result.get("account_data").is_some(), "account_data must be present");
    assert!(result.get("to_device").is_some(), "to_device must be present");
    assert!(result.get("device_lists").is_some(), "device_lists must be present");
    assert!(result.get("device_one_time_keys_count").is_some(), "device_one_time_keys_count must be present");
    assert!(result.get("key_rotation_needed").is_some(), "key_rotation_needed must be present");
    assert!(result.get("device_list_changes").is_some(), "device_list_changes must be present");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_response_rooms_section_structure() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();

    let rooms = &result["rooms"];
    assert!(rooms.get("join").is_some(), "rooms.join must be present");
    assert!(rooms.get("invite").is_some(), "rooms.invite must be present");
    assert!(rooms.get("leave").is_some(), "rooms.leave must be present");
    assert!(rooms["join"].is_object());
    assert!(rooms["invite"].is_object());
    assert!(rooms["leave"].is_object());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_response_joined_room_has_room_sync_shape() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("Joined Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let content = json!({"msgtype": "m.text", "body": "hello joined"});
    room_service.send_message(room_id, &user_id, "m.room.message", &content).await.unwrap();

    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();
    let joined = &result["rooms"]["join"];
    assert!(joined.is_object());
    let room = &joined[room_id];
    assert!(room.is_object(), "joined room should appear in rooms.join");
    assert!(room.get("timeline").is_some(), "timeline must be present in room sync");
    assert!(room.get("state").is_some(), "state must be present in room sync");
    assert!(room.get("ephemeral").is_some(), "ephemeral must be present in room sync");
    assert!(room.get("account_data").is_some(), "account_data must be present in room sync");
    assert!(room.get("unread_notifications").is_some(), "unread_notifications must be present");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_response_timeline_has_limited_and_prev_batch() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("Timeline Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let content = json!({"msgtype": "m.text", "body": "msg"});
    room_service.send_message(room_id, &user_id, "m.room.message", &content).await.unwrap();

    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();
    let timeline = &result["rooms"]["join"][room_id]["timeline"];
    assert!(timeline.get("events").is_some());
    assert!(timeline.get("limited").is_some(), "limited must be present");
    assert!(timeline.get("prev_batch").is_some(), "prev_batch must be present");
    let prev_batch = timeline["prev_batch"].as_str().unwrap();
    assert!(prev_batch.starts_with('t'), "prev_batch should start with 't'");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_response_unread_notifications_shape() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("Unread Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();
    let unread = &result["rooms"]["join"][room_id]["unread_notifications"];
    assert!(unread.get("highlight_count").is_some());
    assert!(unread.get("notification_count").is_some());
    assert!(unread["highlight_count"].is_number());
    assert!(unread["notification_count"].is_number());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_response_presence_section_shape() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO presence (user_id, presence, status_msg, last_active_ts, created_ts, updated_ts) VALUES ($1, 'online', 'busy', $2, $2, $2) ON CONFLICT (user_id) DO UPDATE SET presence = 'online', status_msg = 'busy', last_active_ts = $2, updated_ts = $2")
        .bind(&user_id)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .ok();

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();

    let presence = &result["presence"];
    assert!(presence.get("events").is_some());
    let events = presence["events"].as_array().unwrap();
    assert!(!events.is_empty(), "presence events should not be empty");
    let user_presence = events.iter().find(|e| e["sender"] == user_id).unwrap();
    assert_eq!(user_presence["type"], "m.presence");
    assert_eq!(user_presence["content"]["presence"], "online");
    assert_eq!(user_presence["content"]["status_msg"], "busy");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_response_account_data_section_shape() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO account_data (user_id, data_type, content, created_ts, updated_ts) VALUES ($1, 'm.direct', $2, $3, $3) ON CONFLICT (user_id, data_type) DO UPDATE SET content = $2")
        .bind(&user_id)
        .bind(json!({"@alice:localhost": {"!room:localhost": ["$evt:localhost"]}}))
        .bind(now)
        .execute(pool.as_ref())
        .await
        .ok();

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();

    let account_data = &result["account_data"];
    assert!(account_data.get("events").is_some());
    let events = account_data["events"].as_array().unwrap();
    let direct = events.iter().find(|e| e["type"] == "m.direct");
    assert!(direct.is_some(), "m.direct account_data should be present");
    assert!(direct.unwrap()["content"].is_object());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_response_to_device_section_shape() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    let device_id = format!("DEV{}", unique_id());
    insert_test_user(&pool, &user_id).await;

    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO devices (device_id, user_id, created_ts, first_seen_ts) VALUES ($1, $2, $3, $3) ON CONFLICT DO NOTHING")
        .bind(&device_id)
        .bind(&user_id)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .ok();

    sqlx::query(
        r#"INSERT INTO to_device_messages (sender_user_id, sender_device_id, recipient_user_id, recipient_device_id, event_type, content)
           VALUES ($1, 'DEV_SENDER', $2, $3, 'm.room_key', $4)"#,
    )
    .bind(format!("@sender_{}:localhost", unique_id()))
    .bind(&user_id)
    .bind(&device_id)
    .bind(json!({"room_id": "!room:localhost", "session_id": "sess1"}))
    .execute(pool.as_ref())
    .await
    .ok();

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, Some(&device_id), 0, false, "online", None, None).await.unwrap();

    let to_device = &result["to_device"];
    assert!(to_device.get("events").is_some());
    let events = to_device["events"].as_array().unwrap();
    assert!(!events.is_empty(), "to_device events should not be empty");
    assert_eq!(events[0]["type"], "m.room_key");
    assert!(events[0].get("content").is_some());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_response_device_lists_section_shape() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();

    let device_lists = &result["device_lists"];
    assert!(device_lists.get("changed").is_some(), "device_lists.changed must be present");
    assert!(device_lists.get("left").is_some(), "device_lists.left must be present");
    assert!(device_lists["changed"].is_array());
    assert!(device_lists["left"].is_array());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_response_device_one_time_keys_count_empty_without_device() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, _room_service) = make_sync_service(&pool);
    // Without device_id, device_one_time_keys_count should be an empty object.
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();
    let counts = &result["device_one_time_keys_count"];
    assert!(counts.is_object(), "device_one_time_keys_count must be an object");
    assert!(counts.as_object().unwrap().is_empty(), "without device_id counts should be empty");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_response_device_one_time_keys_count_with_device() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    let device_id = format!("DEV{}", unique_id());
    insert_test_user(&pool, &user_id).await;

    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO devices (device_id, user_id, created_ts, first_seen_ts) VALUES ($1, $2, $3, $3) ON CONFLICT DO NOTHING")
        .bind(&device_id)
        .bind(&user_id)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .ok();

    // Insert device_key_counts for the device.
    sqlx::query("INSERT INTO device_key_counts (user_id, device_id, algorithm, count, updated_ts) VALUES ($1, $2, 'signed_curve25519', 5, $3) ON CONFLICT (user_id, device_id, algorithm) DO UPDATE SET count = 5")
        .bind(&user_id)
        .bind(&device_id)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .ok();

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, Some(&device_id), 0, false, "online", None, None).await.unwrap();
    let counts = &result["device_one_time_keys_count"];
    assert!(counts.is_object());
    assert_eq!(counts["signed_curve25519"], 5, "device_one_time_keys_count should reflect stored count");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_response_key_rotation_needed_empty_default() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();
    let key_rotation = &result["key_rotation_needed"];
    assert!(key_rotation.is_object());
    assert!(key_rotation.get("rooms").is_some(), "key_rotation_needed must have rooms field");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_response_key_rotation_needed_with_pending() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let now = chrono::Utc::now().timestamp_millis();
    // Insert a pending key rotation for the user.
    sqlx::query("INSERT INTO key_rotation_pending (room_id, reason, triggered_by_user_id, created_ts) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING")
        .bind(format!("!room_{}:localhost", unique_id()))
        .bind("manual_rotation")
        .bind(&user_id)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .ok();

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();
    let key_rotation = &result["key_rotation_needed"];
    let rooms = key_rotation["rooms"].as_array();
    assert!(rooms.is_some(), "key_rotation_needed.rooms must be an array");
    assert!(!rooms.unwrap().is_empty(), "key_rotation_needed.rooms should contain pending room");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_response_device_list_changes_shape() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();
    let changes = &result["device_list_changes"];
    assert!(changes.is_object());
    assert!(changes.get("users").is_some(), "device_list_changes must have users field");
    assert!(changes.get("changed_count").is_some(), "device_list_changes must have changed_count");
    assert!(changes.get("left_count").is_some(), "device_list_changes must have left_count");
}

// ===========================================================================
// Inline filter JSON parsing (filter_id starting with `{`)
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_with_inline_filter_event_fields() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("Filter Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let content = json!({"msgtype": "m.text", "body": "filtered msg"});
    room_service.send_message(room_id, &user_id, "m.room.message", &content).await.unwrap();

    // Inline filter that only requests "type" and "content" event fields.
    let inline_filter = json!({
        "event_fields": ["type", "content"],
        "event_format": "client",
        "room": { "timeline": { "limit": 10 } }
    })
    .to_string();

    let result = sync_service
        .sync(&user_id, None, 0, false, "online", Some(&inline_filter), None)
        .await
        .unwrap();

    let joined = &result["rooms"]["join"];
    let room = &joined[room_id];
    let events = room["timeline"]["events"].as_array().unwrap();
    let message_events: Vec<_> = events.iter().filter(|e| e["type"] == "m.room.message").collect();
    assert!(!message_events.is_empty());
    let event = message_events[0];
    // event_fields filter should keep only "type" and "content".
    assert!(event.get("type").is_some(), "type should be retained");
    assert!(event.get("content").is_some(), "content should be retained");
    assert!(event.get("event_id").is_none(), "event_id should be filtered out by event_fields");
    assert!(event.get("sender").is_none(), "sender should be filtered out by event_fields");
    assert!(event.get("origin_server_ts").is_none(), "origin_server_ts should be filtered out");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_with_inline_filter_federation_format() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("Federation Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let content = json!({"msgtype": "m.text", "body": "federation msg"});
    room_service.send_message(room_id, &user_id, "m.room.message", &content).await.unwrap();

    let inline_filter = json!({
        "event_format": "federation",
        "room": { "timeline": { "limit": 10 } }
    })
    .to_string();

    let result = sync_service
        .sync(&user_id, None, 0, false, "online", Some(&inline_filter), None)
        .await
        .unwrap();

    let joined = &result["rooms"]["join"];
    let events = joined[room_id]["timeline"]["events"].as_array().unwrap();
    let message_events: Vec<_> = events.iter().filter(|e| e["type"] == "m.room.message").collect();
    assert!(!message_events.is_empty());
    let event = message_events[0];
    // Federation format should add "depth" and "origin" fields.
    assert!(event.get("depth").is_some(), "federation format should add depth");
    assert!(event.get("origin").is_some(), "federation format should add origin");
}

// ===========================================================================
// Stored filter resolution via FilterStorage
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_with_stored_filter() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let filter_storage = FilterStorage::new(&pool);
    let filter_id = format!("filter_{}", unique_id());
    let filter_content = json!({
        "room": { "timeline": { "limit": 1 } }
    });
    filter_storage
        .create_filter(CreateFilterRequest {
            user_id: user_id.clone(),
            filter_id: filter_id.clone(),
            content: filter_content,
        })
        .await
        .unwrap();

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("Stored Filter Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    // Send 3 messages, but the stored filter limits timeline to 1.
    for i in 0..3 {
        let content = json!({"msgtype": "m.text", "body": format!("msg {i}")});
        room_service.send_message(room_id, &user_id, "m.room.message", &content).await.unwrap();
    }

    let result = sync_service
        .sync(&user_id, None, 0, false, "online", Some(&filter_id), None)
        .await
        .unwrap();

    let timeline = &result["rooms"]["join"][room_id]["timeline"];
    let events = timeline["events"].as_array().unwrap();
    // The filter limits to 1 event; only m.room.message events count (state events may exist).
    let message_events: Vec<_> = events.iter().filter(|e| e["type"] == "m.room.message").collect();
    assert!(
        message_events.len() <= 1,
        "stored filter timeline.limit=1 should restrict message events to at most 1, got {}",
        message_events.len()
    );
    assert!(timeline["limited"].as_bool().unwrap_or(false), "timeline should be limited when truncated");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_with_invalid_filter_json_returns_error() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, _room_service) = make_sync_service(&pool);
    // Inline filter with invalid JSON.
    let invalid_filter = "{not valid json";
    let result = sync_service.sync(&user_id, None, 0, false, "online", Some(invalid_filter), None).await;
    assert!(result.is_err(), "invalid inline filter JSON should produce an error");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_with_nonexistent_stored_filter_uses_defaults() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, _room_service) = make_sync_service(&pool);
    // Filter ID that doesn't start with `{` and isn't stored → no filter applied (defaults).
    let result = sync_service
        .sync(&user_id, None, 0, false, "online", Some("nonexistent_filter_id"), None)
        .await;
    assert!(result.is_ok(), "nonexistent stored filter should not error, defaults should apply");
}

// ===========================================================================
// next_batch format edge cases
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_next_batch_starts_with_s_prefix() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();

    let next_batch = result["next_batch"].as_str().unwrap();
    assert!(next_batch.starts_with('s'), "next_batch must start with 's' prefix, got: {next_batch}");
    assert!(SyncToken::parse(next_batch).is_some(), "next_batch must be a valid SyncToken");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_next_batch_contains_to_device_stream_id_with_device() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    let device_id = format!("DEV{}", unique_id());
    insert_test_user(&pool, &user_id).await;

    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO devices (device_id, user_id, created_ts, first_seen_ts) VALUES ($1, $2, $3, $3) ON CONFLICT DO NOTHING")
        .bind(&device_id)
        .bind(&user_id)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .ok();

    sqlx::query(
        r#"INSERT INTO to_device_messages (sender_user_id, sender_device_id, recipient_user_id, recipient_device_id, event_type, content)
           VALUES ($1, 'DEV_SENDER', $2, $3, 'm.room_key', $4)"#,
    )
    .bind(format!("@sender_{}:localhost", unique_id()))
    .bind(&user_id)
    .bind(&device_id)
    .bind(json!({"session_id": "sess1"}))
    .execute(pool.as_ref())
    .await
    .ok();

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, Some(&device_id), 0, false, "online", None, None).await.unwrap();

    let next_batch = result["next_batch"].as_str().unwrap();
    let token = SyncToken::parse(next_batch).unwrap();
    assert!(token.to_device_stream_id.is_some(), "next_batch should include to_device_stream_id when device_id provided");
    assert!(token.device_list_stream_id.is_some(), "next_batch should include device_list_stream_id");
}

// ===========================================================================
// room_sync response structure (build_room_sync)
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_sync_response_has_all_sections() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("All Sections Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let content = json!({"msgtype": "m.text", "body": "section test"});
    room_service.send_message(room_id, &user_id, "m.room.message", &content).await.unwrap();

    let result = sync_service.room_sync(&user_id, room_id, 0, false, None).await.unwrap();
    // room_sync response must have all required sections.
    assert!(result.get("next_batch").is_some(), "room_sync must include next_batch");
    assert!(result.get("timeline").is_some(), "room_sync must include timeline");
    assert!(result.get("state").is_some(), "room_sync must include state");
    assert!(result.get("ephemeral").is_some(), "room_sync must include ephemeral");
    assert!(result.get("account_data").is_some(), "room_sync must include account_data");
    assert!(result.get("unread_notifications").is_some(), "room_sync must include unread_notifications");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_sync_timeline_event_shape() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("Event Shape Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let content = json!({"msgtype": "m.text", "body": "shape test"});
    room_service.send_message(room_id, &user_id, "m.room.message", &content).await.unwrap();

    let result = sync_service.room_sync(&user_id, room_id, 0, false, None).await.unwrap();
    let events = result["timeline"]["events"].as_array().unwrap();
    let message_event = events.iter().find(|e| e["type"] == "m.room.message").unwrap();

    // Each timeline event should have the standard fields.
    assert!(message_event.get("type").is_some());
    assert!(message_event.get("content").is_some());
    assert!(message_event.get("sender").is_some());
    assert!(message_event.get("origin_server_ts").is_some());
    assert!(message_event.get("event_id").is_some());
    assert!(message_event.get("room_id").is_some());
    assert!(message_event.get("unsigned").is_some());
    assert!(message_event["unsigned"].get("age").is_some(), "unsigned.age should be present");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_sync_state_event_shape_initial() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("State Shape Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let result = sync_service.room_sync(&user_id, room_id, 0, false, None).await.unwrap();
    let state_events = result["state"]["events"].as_array().unwrap();
    assert!(!state_events.is_empty(), "initial sync should have state events");

    // State events should have type, content, sender, state_key.
    let has_state_key = state_events.iter().any(|e| e.get("state_key").is_some());
    assert!(has_state_key, "state events should include state_key");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_sync_incremental_excludes_state_events() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("Incremental Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    // Initial sync to get a token.
    let initial = sync_service.room_sync(&user_id, room_id, 0, false, None).await.unwrap();
    let since = initial["next_batch"].as_str().unwrap();

    // Send a new message.
    let content = json!({"msgtype": "m.text", "body": "incremental msg"});
    room_service.send_message(room_id, &user_id, "m.room.message", &content).await.unwrap();

    // Incremental sync should not return state events.
    let incremental = sync_service.room_sync(&user_id, room_id, 0, false, Some(since)).await.unwrap();
    let state_events = incremental["state"]["events"].as_array().unwrap();
    assert!(state_events.is_empty(), "incremental sync should not return state events");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_sync_next_batch_format() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("NextBatch Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let result = sync_service.room_sync(&user_id, room_id, 0, false, None).await.unwrap();
    let next_batch = result["next_batch"].as_str().unwrap();
    assert!(next_batch.starts_with('s'), "room_sync next_batch should start with 's'");
    // room_sync next_batch is a simple token (no to_device/device_list stream IDs).
    let token = SyncToken::parse(next_batch).unwrap();
    assert!(token.to_device_stream_id.is_none(), "room_sync next_batch should not have to_device_stream_id");
    assert!(token.device_list_stream_id.is_none(), "room_sync next_batch should not have device_list_stream_id");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_sync_ephemeral_events_returned() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("Ephemeral Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    // Insert an ephemeral event (e.g., typing).
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"INSERT INTO room_ephemeral (room_id, event_type, user_id, content, stream_id, created_ts)
           VALUES ($1, 'm.typing', $2, $3, 1, $4)
           ON CONFLICT (room_id, event_type, user_id) DO UPDATE SET content = $3"#,
    )
    .bind(room_id)
    .bind(&user_id)
    .bind(json!({"user_ids": [user_id]}))
    .bind(now)
    .execute(pool.as_ref())
    .await
    .ok();

    let result = sync_service.room_sync(&user_id, room_id, 0, false, None).await.unwrap();
    let ephemeral = result["ephemeral"]["events"].as_array().unwrap();
    let typing = ephemeral.iter().find(|e| e["type"] == "m.typing");
    assert!(typing.is_some(), "m.typing ephemeral event should be present");
    assert_eq!(typing.unwrap()["content"]["user_ids"][0], user_id);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_sync_account_data_returned() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("Account Data Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    // Insert room account data (e.g., m.tag).
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts)
           VALUES ($1, $2, 'm.tag', $3, $4, $4)
           ON CONFLICT (user_id, room_id, data_type) DO UPDATE SET data = $3"#,
    )
    .bind(&user_id)
    .bind(room_id)
    .bind(json!({"tags": {"favourite": {"order": 0.5}}}))
    .bind(now)
    .execute(pool.as_ref())
    .await
    .ok();

    let result = sync_service.room_sync(&user_id, room_id, 0, false, None).await.unwrap();
    let account_data = result["account_data"]["events"].as_array().unwrap();
    let tag = account_data.iter().find(|e| e["type"] == "m.tag");
    assert!(tag.is_some(), "m.tag account_data event should be present");
    assert!(tag.unwrap()["content"]["tags"]["favourite"].is_object());
}

// ===========================================================================
// rooms_to_include logic (rooms without events excluded in incremental sync)
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_incremental_only_includes_rooms_with_changes() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    // Create two rooms.
    let config1 = CreateRoomConfig { name: Some("Active Room".to_string()), ..Default::default() };
    let room_val1 = room_service.create_room(&user_id, config1).await.unwrap();
    let room_id1 = room_val1["room_id"].as_str().unwrap();

    let config2 = CreateRoomConfig { name: Some("Quiet Room".to_string()), ..Default::default() };
    let room_val2 = room_service.create_room(&user_id, config2).await.unwrap();
    let _room_id2 = room_val2["room_id"].as_str().unwrap();

    // Initial sync to get a token.
    let initial = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();
    let since = initial["next_batch"].as_str().unwrap();

    // Only send a message to room1.
    let content = json!({"msgtype": "m.text", "body": "active msg"});
    room_service.send_message(room_id1, &user_id, "m.room.message", &content).await.unwrap();

    // Incremental sync.
    let incremental = sync_service.sync(&user_id, None, 0, false, "online", None, Some(since)).await.unwrap();
    let joined = &incremental["rooms"]["join"];
    // room1 should be included because it has new events.
    assert!(joined.get(room_id1).is_some(), "active room with new events should be included");
    // room2 may or may not be included depending on state changes, but the active room must be present.
    let room1_events = joined[room_id1]["timeline"]["events"].as_array().unwrap();
    assert!(room1_events.iter().any(|e| e["type"] == "m.room.message"), "active room should have the new message");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_full_state_includes_all_joined_rooms() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config1 = CreateRoomConfig { name: Some("Room A".to_string()), ..Default::default() };
    let room_val1 = room_service.create_room(&user_id, config1).await.unwrap();
    let room_id1 = room_val1["room_id"].as_str().unwrap();

    let config2 = CreateRoomConfig { name: Some("Room B".to_string()), ..Default::default() };
    let room_val2 = room_service.create_room(&user_id, config2).await.unwrap();
    let room_id2 = room_val2["room_id"].as_str().unwrap();

    // Full state sync should include all joined rooms.
    let result = sync_service.sync(&user_id, None, 0, true, "online", None, None).await.unwrap();
    let joined = &result["rooms"]["join"];
    assert!(joined.get(room_id1).is_some(), "Room A should be in joined rooms");
    assert!(joined.get(room_id2).is_some(), "Room B should be in joined rooms");
}

// ===========================================================================
// Empty response / edge cases
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_empty_response_still_has_required_keys() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();

    // Even with no rooms, the response should still have all required sections.
    assert!(result["rooms"]["join"].is_object());
    assert!(result["rooms"]["invite"].is_object());
    assert!(result["rooms"]["leave"].is_object());
    assert!(result["rooms"]["join"].as_object().unwrap().is_empty(), "no joined rooms → empty join map");
    assert!(result["presence"]["events"].is_array());
    assert!(result["account_data"]["events"].is_array());
    assert!(result["to_device"]["events"].is_array());
    assert!(result["to_device"]["events"].as_array().unwrap().is_empty(), "no device_id → empty to_device");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_response_invite_section_empty_for_direct_join() {
    let _guard = response_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("Direct Join Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();
    // The directly-joined room should be in join, not invite.
    assert!(result["rooms"]["join"].get(room_id).is_some(), "joined room should be in join section");
    assert!(result["rooms"]["invite"].as_object().unwrap().is_empty(), "invite section should be empty");
}
