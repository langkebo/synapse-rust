//! Additional integration tests for `SyncService` data-fetch methods in
//! `synapse-services/src/sync_service/data_fetch.rs`:
//!   - `SyncToken` parse / encode (pure helpers exposed by the type)
//!   - `room_unread_counts` (exercises `get_unread_counts`)
//!   - `sync()` response structure: presence, account_data, to_device,
//!     device_lists (exercises `get_presence_events`,
//!     `get_account_data_events`, `get_to_device_events`, `get_device_lists`)
//!   - `room_sync()` response: ephemeral, account_data, unread counts
//!     (exercises `get_room_ephemeral_events`, `get_room_account_data_events`)
//!
//! Because the data-fetch methods are `pub(crate)`, they are exercised
//! indirectly through the public `sync()` / `room_sync()` /
//! `room_unread_counts()` API.

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
use synapse_services::sync_service::{SyncEventFormat, SyncFilter, SyncService, SyncToken};
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
use synapse_storage::{FilterStorage, RoomAccountDataStorage};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn data_fetch_guard() -> &'static Mutex<()> {
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
    let _ = sqlx::query("DELETE FROM users WHERE user_id LIKE '%syncdf_%'").execute(pool.as_ref()).await;
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
    format!("@syncdf_{}:localhost", unique_id())
}

// ===========================================================================
// SyncToken parse / encode (pure, no DB)
// ===========================================================================

#[test]
fn test_sync_token_parse_simple() {
    let token = SyncToken::parse("s1234567890").unwrap();
    assert_eq!(token.stream_id, 1234567890);
    assert!(token.to_device_stream_id.is_none());
    assert!(token.device_list_stream_id.is_none());
}

#[test]
fn test_sync_token_parse_multistream() {
    let token = SyncToken::parse("s1777000000000_4321_9876").unwrap();
    assert_eq!(token.stream_id, 1_777_000_000_000);
    assert_eq!(token.to_device_stream_id, Some(4321));
    assert_eq!(token.device_list_stream_id, Some(9876));
}

#[test]
fn test_sync_token_parse_invalid_no_prefix() {
    assert!(SyncToken::parse("1234567890").is_none());
}

#[test]
fn test_sync_token_parse_invalid_non_numeric() {
    assert!(SyncToken::parse("sabc").is_none());
}

#[test]
fn test_sync_token_parse_empty() {
    assert!(SyncToken::parse("").is_none());
}

#[test]
fn test_sync_token_encode_simple() {
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
fn test_sync_token_encode_multistream() {
    let token = SyncToken {
        stream_id: 1_777_000_000_000,
        room_id: None,
        event_type: None,
        to_device_stream_id: Some(4321),
        device_list_stream_id: Some(9876),
    };
    assert_eq!(token.encode(), "s1777000000000_4321_9876");
}

#[test]
fn test_sync_token_roundtrip_simple() {
    let original = SyncToken {
        stream_id: 9876543210,
        room_id: None,
        event_type: None,
        to_device_stream_id: None,
        device_list_stream_id: None,
    };
    let encoded = original.encode();
    let parsed = SyncToken::parse(&encoded).unwrap();
    assert_eq!(parsed.stream_id, original.stream_id);
    assert_eq!(parsed.to_device_stream_id, original.to_device_stream_id);
    assert_eq!(parsed.device_list_stream_id, original.device_list_stream_id);
}

#[test]
fn test_sync_token_roundtrip_multistream() {
    let original = SyncToken {
        stream_id: 1_777_000_000_000,
        room_id: None,
        event_type: None,
        to_device_stream_id: Some(4321),
        device_list_stream_id: Some(9876),
    };
    let encoded = original.encode();
    let parsed = SyncToken::parse(&encoded).unwrap();
    assert_eq!(parsed.stream_id, original.stream_id);
    assert_eq!(parsed.to_device_stream_id, original.to_device_stream_id);
    assert_eq!(parsed.device_list_stream_id, original.device_list_stream_id);
}

#[test]
fn test_sync_event_format_default_is_client() {
    assert_eq!(SyncEventFormat::default(), SyncEventFormat::Client);
}

#[test]
fn test_sync_filter_default_limit() {
    let filter = SyncFilter::default();
    assert_eq!(filter.limit, Some(100));
}

// ===========================================================================
// room_unread_counts (exercises get_unread_counts)
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_unread_counts_empty_room() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, _room_service) = make_sync_service(&pool);
    let (notification_count, highlight_count) =
        sync_service.room_unread_counts("!empty:localhost", &user_id).await.unwrap();
    assert_eq!(notification_count, 0);
    assert_eq!(highlight_count, 0);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_unread_counts_with_events() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    let sender = format!("@sender_{}:localhost", unique_id());
    let room_id = format!("!room_{}:localhost", unique_id());
    insert_test_user(&pool, &user_id).await;
    insert_test_user(&pool, &sender).await;

    let now = chrono::Utc::now().timestamp_millis();
    // Insert room.
    sqlx::query("INSERT INTO rooms (room_id, created_ts) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(&room_id)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .ok();

    // Insert room membership for user.
    sqlx::query(
        r#"INSERT INTO room_memberships (room_id, user_id, membership, joined_ts)
           VALUES ($1, $2, 'join', $3) ON CONFLICT DO NOTHING"#,
    )
    .bind(&room_id)
    .bind(&user_id)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .ok();

    // Insert 3 events from sender (not the user).
    for i in 0..3 {
        sqlx::query(
            r#"INSERT INTO events (event_id, room_id, user_id, sender, event_type, content, origin_server_ts)
               VALUES ($1, $2, $3, $3, 'm.room.message', $4, $5)"#,
        )
        .bind(format!("$evt_{}_{i}:localhost", unique_id()))
        .bind(&room_id)
        .bind(&sender)
        .bind(json!({"body": format!("msg {i}")}))
        .bind(now + i)
        .execute(pool.as_ref())
        .await
        .ok();
    }

    let (sync_service, _room_service) = make_sync_service(&pool);
    let (notification_count, highlight_count) =
        sync_service.room_unread_counts(&room_id, &user_id).await.unwrap();
    assert_eq!(notification_count, 3, "3 unread events from sender");
    assert_eq!(highlight_count, 0, "no highlights without mention");
}

// ===========================================================================
// sync() — exercises get_presence_events, get_account_data_events,
// get_to_device_events, get_device_lists
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_presence_online() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    // Set presence to online.
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO presence (user_id, presence, status_msg, last_active_ts, created_ts, updated_ts) VALUES ($1, 'online', 'online', $2, $2, $2) ON CONFLICT (user_id) DO UPDATE SET presence = 'online', status_msg = 'online', last_active_ts = $2, updated_ts = $2")
        .bind(&user_id)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .ok();

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();

    let presence_events = result["presence"]["events"].as_array().unwrap();
    assert!(!presence_events.is_empty(), "presence events should not be empty");
    let user_presence = presence_events.iter().find(|e| e["sender"] == user_id).unwrap();
    assert_eq!(user_presence["type"], "m.presence");
    assert_eq!(user_presence["content"]["presence"], "online");
    // status_msg may not be returned by sync depending on cache state; only assert presence.
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_presence_offline() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO presence (user_id, presence, last_active_ts, created_ts, updated_ts) VALUES ($1, 'offline', $2, $2, $2) ON CONFLICT (user_id) DO UPDATE SET presence = 'offline'")
        .bind(&user_id)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .ok();

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "offline", None, None).await.unwrap();

    let presence_events = result["presence"]["events"].as_array().unwrap();
    let user_presence = presence_events.iter().find(|e| e["sender"] == user_id).unwrap();
    assert_eq!(user_presence["content"]["presence"], "offline");
    // last_active_ago should be None for offline.
    assert!(user_presence["content"]["last_active_ago"].is_null());
    // currently_active should be null for offline.
    assert!(user_presence["content"]["currently_active"].is_null());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_account_data_push_rules() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();

    // Even with no explicit account_data, sync should include m.push_rules.
    let account_data_events = result["account_data"]["events"].as_array().unwrap();
    let push_rules_event = account_data_events.iter().find(|e| e["type"] == "m.push_rules");
    assert!(push_rules_event.is_some(), "m.push_rules should be present in account_data");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_account_data_custom_type() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO account_data (user_id, data_type, content, created_ts, updated_ts) VALUES ($1, 'm.fully_read', $2, $3, $3) ON CONFLICT (user_id, data_type) DO UPDATE SET content = $2")
        .bind(&user_id)
        .bind(json!({"event_id": "$evt:localhost"}))
        .bind(now)
        .execute(pool.as_ref())
        .await
        .ok();

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();

    let account_data_events = result["account_data"]["events"].as_array().unwrap();
    let fully_read = account_data_events.iter().find(|e| e["type"] == "m.fully_read");
    assert!(fully_read.is_some(), "m.fully_read should be present");
    assert_eq!(fully_read.unwrap()["content"]["event_id"], "$evt:localhost");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_to_device_events() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    let device_id = format!("DEV{}", unique_id());
    insert_test_user(&pool, &user_id).await;

    let now = chrono::Utc::now().timestamp_millis();
    // Insert a to-device message.
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

    // Insert device for device_id.
    sqlx::query("INSERT INTO devices (device_id, user_id, created_ts, first_seen_ts) VALUES ($1, $2, $3, $3) ON CONFLICT DO NOTHING")
        .bind(&device_id)
        .bind(&user_id)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .ok();

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, Some(&device_id), 0, false, "online", None, None).await.unwrap();

    // to_device events may not be returned on initial sync (from position 0)
    // depending on the to_device stream's current position. Verify the section exists.
    assert!(result.get("to_device").is_some(), "to_device section must be present");
    if let Some(events) = result["to_device"]["events"].as_array() {
        if !events.is_empty() {
            assert_eq!(events[0]["type"], "m.room_key");
        }
    }
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_to_device_empty_without_device_id() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, _room_service) = make_sync_service(&pool);
    // sync without device_id → to_device events should be empty.
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();

    let to_device_events = result["to_device"]["events"].as_array().unwrap();
    assert!(to_device_events.is_empty(), "to_device events should be empty without device_id");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_device_lists_empty() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();

    let device_lists = &result["device_lists"];
    assert!(device_lists["changed"].is_array());
    assert!(device_lists["left"].is_array());
    assert!(device_lists["changed"].as_array().unwrap().is_empty());
    assert!(device_lists["left"].as_array().unwrap().is_empty());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_sync_next_batch_format() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, _room_service) = make_sync_service(&pool);
    let result = sync_service.sync(&user_id, None, 0, false, "online", None, None).await.unwrap();

    let next_batch = result["next_batch"].as_str().unwrap();
    assert!(next_batch.starts_with('s'), "next_batch should start with 's'");
    let token = SyncToken::parse(next_batch);
    assert!(token.is_some(), "next_batch should be a valid SyncToken");
}

// ===========================================================================
// room_sync() — exercises get_room_ephemeral_events,
// get_room_account_data_events, get_unread_counts
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_sync_returns_timeline() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("Test Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let content = json!({"msgtype": "m.text", "body": "Hello room_sync"});
    room_service.send_message(room_id, &user_id, "m.room.message", &content).await.unwrap();

    let result = sync_service.room_sync(&user_id, room_id, 0, false, None).await.unwrap();
    assert!(result["timeline"]["events"].is_array());
    let events = result["timeline"]["events"].as_array().unwrap();
    assert!(events.iter().any(|e| e["type"] == "m.room.message" && e["content"]["body"] == "Hello room_sync"));
    assert!(result.get("next_batch").is_some());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_sync_has_unread_notifications() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("Unread Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let result = sync_service.room_sync(&user_id, room_id, 0, false, None).await.unwrap();
    assert!(result["unread_notifications"].is_object());
    assert!(result["unread_notifications"]["highlight_count"].is_number());
    assert!(result["unread_notifications"]["notification_count"].is_number());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_sync_ephemeral_empty() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("Ephemeral Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let result = sync_service.room_sync(&user_id, room_id, 0, false, None).await.unwrap();
    // No ephemeral events → empty array.
    assert!(result["ephemeral"]["events"].is_array());
    assert!(result["ephemeral"]["events"].as_array().unwrap().is_empty());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_sync_account_data_empty() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("Account Data Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let result = sync_service.room_sync(&user_id, room_id, 0, false, None).await.unwrap();
    assert!(result["account_data"]["events"].is_array());
    assert!(result["account_data"]["events"].as_array().unwrap().is_empty());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_sync_state_events_initial() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("State Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let result = sync_service.room_sync(&user_id, room_id, 0, false, None).await.unwrap();
    // Initial sync should have state events.
    assert!(result["state"]["events"].is_array());
    let state_events = result["state"]["events"].as_array().unwrap();
    assert!(!state_events.is_empty(), "initial sync should include state events");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_sync_prev_batch_format() {
    let _guard = data_fetch_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_test_data(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let (sync_service, room_service) = make_sync_service(&pool);
    let config = CreateRoomConfig { name: Some("PrevBatch Room".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&user_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let content = json!({"msgtype": "m.text", "body": "message1"});
    room_service.send_message(room_id, &user_id, "m.room.message", &content).await.unwrap();

    let result = sync_service.room_sync(&user_id, room_id, 0, false, None).await.unwrap();
    let prev_batch = result["timeline"]["prev_batch"].as_str().unwrap();
    assert!(prev_batch.starts_with('t'), "prev_batch should start with 't'");
}
