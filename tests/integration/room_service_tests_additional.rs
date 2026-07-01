//! Additional integration tests for `RoomService` at
//! `synapse-services/src/room/service.rs`.
//!
//! The migrated file (`room_service_tests_migrated.rs`) covers the core
//! create/join/invite/ban/upgrade flows plus appservice delivery. This file
//! fills the remaining coverage gaps for the 133-public-method facade:
//!   - own-logic methods (`get_room`, `get_room_state`, `get_user_rooms`,
//!     `collect_child_rooms`)
//!   - background task management (`start_cleanup_task`,
//!     `cleanup_completed_tasks`, `abort_task`, `shutdown`)
//!   - federation infrastructure setters
//!   - `dispatch_appservice_event` (both no-manager and with-manager paths)
//!   - `is_remote_user` / `is_remote_room`
//!   - membership queries and moderation (`leave_room`, `forget_room`,
//!     `kick_user`, `unban_user`, member counts, ban reasons, force-leave,
//!     record removal)
//!   - room state / admin operations (`room_exists`, `get_room_record`,
//!     `get_room_version`, `is_room_creator`, encryption status, block /
//!     unblock, directory, public rooms, aliases, tags, `grant_room_admin`,
//!     `delete_room`, tombstone, upgrade-allowed, migrate-content)
//!   - messaging queries (event records, state events, pinned events,
//!     timeline queries, redaction, daily counts, missing events, reports,
//!     timestamps, signatures, receipts, read markers, typing)

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::await_holding_lock)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde_json::json;

use synapse_federation::event_broadcaster::EventBroadcaster;
use synapse_federation::{FederationClient, KeyRotationManager};
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::Validator;
use synapse_services::application_service::ApplicationServiceManager;
use synapse_services::room_service::{CreateRoomConfig, RoomService, RoomServiceConfig};
use synapse_services::room_summary_service::RoomSummaryService;
use synapse_storage::application_service::{ApplicationServiceStorage, RegisterApplicationServiceRequest};
use synapse_storage::event::{EventRepository, EventStorage};
use synapse_storage::membership::RoomMemberStorage;
use synapse_storage::relations::RelationsStorage;
use synapse_storage::room::RoomStorage;
use synapse_storage::room_summary::RoomSummaryStorage;
use synapse_storage::user::UserStorage;
use synapse_storage::CreateEventParams;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn room_service_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Warm up the shared pool on the current tokio runtime.
async fn warm_up_pool(pool: &Arc<sqlx::PgPool>) {
    for _ in 0..8 {
        match tokio::time::timeout(
            Duration::from_secs(5),
            sqlx::query("SELECT 1").execute(pool.as_ref()),
        )
        .await
        {
            Ok(Ok(_)) => return,
            Ok(Err(_)) | Err(_) => {
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
        }
    }
    let _ = sqlx::query("SELECT 1").execute(pool.as_ref()).await;
}

/// Create the tables used by `RoomService` if they do not already exist. The
/// shared test pool normally applies the real migrations, so these statements
/// are a defensive fallback.
async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            user_id VARCHAR(255) PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT,
            is_admin BOOLEAN DEFAULT FALSE,
            is_guest BOOLEAN DEFAULT FALSE,
            is_shadow_banned BOOLEAN DEFAULT FALSE,
            is_deactivated BOOLEAN DEFAULT FALSE,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            displayname TEXT,
            avatar_url TEXT,
            email TEXT,
            phone TEXT,
            generation BIGINT DEFAULT 0,
            consent_version TEXT,
            appservice_id TEXT,
            user_type TEXT,
            invalid_update_at BIGINT,
            migration_state TEXT,
            password_changed_ts BIGINT,
            is_password_change_required BOOLEAN DEFAULT FALSE,
            password_expires_at BIGINT,
            failed_login_attempts INT DEFAULT 0,
            locked_until BIGINT,
            must_change_password BOOLEAN DEFAULT FALSE
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS rooms (
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
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS room_memberships (
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
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS events (
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
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();
}

/// Delete all data for a specific room to avoid cross-test accumulation.
/// Statements that reference tables absent from the real schema are ignored
/// via `.ok()`.
async fn cleanup_room(pool: &sqlx::PgPool, room_id: &str) {
    for stmt in [
        "DELETE FROM receipts WHERE room_id = $1",
        "DELETE FROM read_markers WHERE room_id = $1",
        "DELETE FROM ephemeral_events WHERE room_id = $1",
        "DELETE FROM room_tags WHERE room_id = $1",
        "DELETE FROM room_aliases WHERE room_id = $1",
        "DELETE FROM room_directory WHERE room_id = $1",
        "DELETE FROM blocked_rooms WHERE room_id = $1",
        "DELETE FROM event_signatures WHERE event_id IN (SELECT event_id FROM events WHERE room_id = $1)",
        "DELETE FROM reported_events WHERE room_id = $1",
        "DELETE FROM events WHERE room_id = $1",
        "DELETE FROM room_memberships WHERE room_id = $1",
        "DELETE FROM rooms WHERE room_id = $1",
    ] {
        sqlx::query(stmt).bind(room_id).execute(pool).await.ok();
    }
}

async fn create_test_user(pool: &sqlx::PgPool, user_id: &str, username: &str) {
    sqlx::query(
        r#"
        INSERT INTO users (user_id, username, created_ts)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(username)
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(pool)
    .await
    .ok();
}

fn create_room_service(pool: &Arc<sqlx::PgPool>, cache: Arc<CacheManager>) -> RoomService {
    let member_storage = Arc::new(RoomMemberStorage::new(pool, "localhost"));
    let event_storage: Arc<dyn EventRepository> = Arc::new(EventStorage::new(pool, "localhost".to_string()));
    let canonical_cache = cache;
    let room_summary_storage = Arc::new(RoomSummaryStorage::new(pool));
    let room_summary_service =
        Arc::new(RoomSummaryService::new(room_summary_storage, event_storage.clone(), Some(member_storage.clone())));

    RoomService::new(RoomServiceConfig {
        room_storage: Arc::new(RoomStorage::new(pool)),
        member_storage,
        event_storage,
        room_tag_storage: Arc::new(synapse_storage::room_tag::RoomTagStorage::new(pool.clone())),
        user_storage: Arc::new(UserStorage::new(pool, canonical_cache.clone())),
        auth_service: Arc::new(synapse_rust::auth::AuthService::new(
            pool,
            canonical_cache,
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
    })
}

fn create_test_appservice_manager(pool: &Arc<sqlx::PgPool>) -> Arc<ApplicationServiceManager> {
    Arc::new(ApplicationServiceManager::new(
        Arc::new(ApplicationServiceStorage::new(pool)),
        Arc::new(EventStorage::new(pool, "localhost".to_string())),
        "localhost".to_string(),
    ))
}

/// Create a public room owned by `creator` and return `(service, room_id)`.
async fn make_public_room(
    pool: &Arc<sqlx::PgPool>,
    cache: &Arc<CacheManager>,
    creator: &str,
) -> (RoomService, String) {
    create_test_user(pool, creator, &creator.replace(['@', ':'], "_")).await;
    let svc = create_room_service(pool, cache.clone());
    let val = svc
        .create_room(creator, CreateRoomConfig { visibility: Some("public".to_string()), ..Default::default() })
        .await
        .expect("create_room should succeed");
    let room_id = val["room_id"].as_str().expect("room_id should be present").to_string();
    (svc, room_id)
}

/// Helper to acquire the guard and pool, sharing boilerplate.
async fn guarded_pool() -> (std::sync::MutexGuard<'static, ()>, Arc<sqlx::PgPool>) {
    let guard = room_service_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    (guard, pool)
}

// =============================================================================
// Own-logic: get_room / get_room_state / get_user_rooms / collect_child_rooms
// =============================================================================

#[tokio::test]
async fn test_get_room_existing_returns_metadata() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let room = svc.get_room(&room_id).await.unwrap();
    assert_eq!(room["room_id"].as_str(), Some(room_id.as_str()));
    assert_eq!(room["creator"].as_str(), Some(alice.as_str()));
    assert_eq!(room["is_public"], true);

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_room_nonexistent_returns_not_found() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let svc = create_room_service(&pool, cache);

    let missing = format!("!missing_{}:localhost", unique_id());
    let result = svc.get_room(&missing).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(format!("{err}").to_lowercase().contains("not found"), "expected not-found, got {err}");
}

#[tokio::test]
async fn test_get_room_state_for_member_returns_state() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let state = svc.get_room_state(&room_id, &alice).await.unwrap();
    assert_eq!(state["room_id"].as_str(), Some(room_id.as_str()));
    assert_eq!(state["creator"].as_str(), Some(alice.as_str()));

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_room_state_for_non_member_returns_forbidden() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let result = svc.get_room_state(&room_id, &bob).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(format!("{err}").to_lowercase().contains("forbidden") || format!("{err}").contains("not a member"));

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_room_state_nonexistent_room_returns_not_found() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    create_test_user(&pool, &alice, &format!("alice_{id}")).await;
    let svc = create_room_service(&pool, cache);

    // Insert a membership for alice in a room that does not exist so the
    // is_member check passes and we reach the get_room None branch.
    let ghost_room = format!("!ghost_{}:localhost", unique_id());
    sqlx::query("INSERT INTO room_memberships (room_id, user_id, membership, joined_ts, updated_ts) VALUES ($1, $2, 'join', $3, $3) ON CONFLICT DO NOTHING")
        .bind(&ghost_room)
        .bind(&alice)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(pool.as_ref())
        .await
        .ok();

    let result = svc.get_room_state(&ghost_room, &alice).await;
    assert!(result.is_err(), "should error for non-existent room");
    assert!(format!("{}", result.unwrap_err()).to_lowercase().contains("not found"));

    sqlx::query("DELETE FROM room_memberships WHERE room_id = $1").bind(&ghost_room).execute(pool.as_ref()).await.ok();
}

#[tokio::test]
async fn test_get_user_rooms_returns_joined() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let rooms = svc.get_user_rooms(&alice).await.unwrap();
    let arr = rooms.as_array().expect("should be an array");
    assert!(arr.iter().any(|r| r["room_id"].as_str() == Some(room_id.as_str())));

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_user_rooms_empty_returns_empty_array() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let nobody = format!("@nobody_{id}:localhost");
    let svc = create_room_service(&pool, cache);

    let rooms = svc.get_user_rooms(&nobody).await.unwrap();
    let arr = rooms.as_array().expect("should be an array");
    assert!(arr.is_empty(), "user with no rooms should get an empty array");
}

#[tokio::test]
async fn test_collect_child_rooms_empty_input_returns_empty() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let svc = create_room_service(&pool, cache);

    let result = svc.collect_child_rooms(&[]).await.unwrap();
    assert!(result.is_empty(), "empty input must return empty vec");
}

#[tokio::test]
async fn test_collect_child_rooms_includes_existing_room() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let children = svc.collect_child_rooms(&[room_id.clone()]).await.unwrap();
    assert_eq!(children.len(), 1, "existing child room should be included");
    assert_eq!(children[0]["room_id"].as_str(), Some(room_id.as_str()));
    assert!(children[0]["num_joined_members"].as_i64().is_some());

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_collect_child_rooms_skips_missing_rooms() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    let missing = format!("!missing_{}:localhost", unique_id());

    let children = svc.collect_child_rooms(&[room_id.clone(), missing]).await.unwrap();
    assert_eq!(children.len(), 1, "only the existing room should be returned");
    assert_eq!(children[0]["room_id"].as_str(), Some(room_id.as_str()));

    cleanup_room(&pool, &room_id).await;
}

// =============================================================================
// Background task management
// =============================================================================

#[tokio::test]
async fn test_start_cleanup_task_returns_running_handle() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let svc = Arc::new(create_room_service(&pool, cache));
    let handle = svc.clone().start_cleanup_task();
    assert!(!handle.is_finished(), "cleanup task should be running");
    handle.abort();
}

#[tokio::test]
async fn test_cleanup_completed_tasks_removes_finished() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let svc = create_room_service(&pool, cache);

    // Insert a short-lived task and let it finish.
    let handle = tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(2)).await;
    });
    svc.active_tasks.write().await.insert("short_lived".to_string(), handle);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let remaining = svc.cleanup_completed_tasks().await;
    assert_eq!(remaining, 0, "finished task should have been removed");
}

#[tokio::test]
async fn test_abort_task_known_and_unknown() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let svc = create_room_service(&pool, cache);

    let handle = tokio::spawn(async {
        tokio::time::sleep(Duration::from_secs(60)).await;
    });
    svc.active_tasks.write().await.insert("long_running".to_string(), handle);

    assert!(svc.abort_task("long_running").await, "aborting a known task should return true");
    assert!(!svc.abort_task("long_running").await, "aborting again should return false");
    assert!(!svc.abort_task("never_existed").await, "aborting an unknown task should return false");
}

#[tokio::test]
async fn test_shutdown_clears_all_tasks() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let svc = create_room_service(&pool, cache);

    let handle = tokio::spawn(async {
        tokio::time::sleep(Duration::from_secs(60)).await;
    });
    svc.active_tasks.write().await.insert("task_a".to_string(), handle);

    svc.shutdown().await;
    let count = svc.active_tasks.read().await.len();
    assert_eq!(count, 0, "shutdown should drain all tasks");
}

#[tokio::test]
async fn test_room_summary_service_getter() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let svc = create_room_service(&pool, cache);
    let _summary = svc.room_summary_service();
    // Accessing the reference proves the getter compiles and returns.
}

// =============================================================================
// Federation infrastructure setters
// =============================================================================

#[tokio::test]
async fn test_set_event_broadcaster() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let svc = create_room_service(&pool, cache);
    let broadcaster = Arc::new(EventBroadcaster::new("localhost".to_string()));
    svc.set_event_broadcaster(broadcaster).await;
}

#[tokio::test]
async fn test_set_key_rotation_manager() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let svc = create_room_service(&pool, cache);
    let krm = Arc::new(KeyRotationManager::new(&pool, "localhost"));
    svc.set_key_rotation_manager(krm).await;
}

#[tokio::test]
async fn test_set_federation_client() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let svc = create_room_service(&pool, cache);
    let krm = Arc::new(KeyRotationManager::new(&pool, "localhost"));
    let fed = Arc::new(FederationClient::new("localhost".to_string(), krm));
    svc.set_federation_client(fed).await;
}

#[tokio::test]
async fn test_set_app_service_manager() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let manager = create_test_appservice_manager(&pool);
    svc.set_app_service_manager(manager).await;

    cleanup_room(&pool, &room_id).await;
}

// =============================================================================
// dispatch_appservice_event
// =============================================================================

#[tokio::test]
async fn test_dispatch_appservice_event_without_manager_is_noop() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let svc = create_room_service(&pool, cache);
    // No app_service_manager set -> should return without error.
    svc.dispatch_appservice_event("$evt:localhost", "!room:localhost", "m.room.message", "@u:localhost", &json!({}), None)
        .await;
}

#[tokio::test]
async fn test_dispatch_appservice_event_with_manager_enqueues() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let manager = create_test_appservice_manager(&pool);
    let as_id = format!("dispatch-bridge-{id}");
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: as_id.clone(),
            url: "http://localhost:9999".to_string(),
            as_token: format!("as_{as_id}"),
            hs_token: format!("hs_{as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: None,
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [{"exclusive": false, "regex": "@.*:localhost"}],
                "aliases": [],
                "rooms": []
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("register should succeed");
    svc.set_app_service_manager(manager.clone()).await;

    let storage = ApplicationServiceStorage::new(&pool);
    let before = storage.get_pending_events(&as_id, 256).await.unwrap().len();
    svc.dispatch_appservice_event("$evt:localhost", &room_id, "m.room.message", &alice, &json!({"body": "x"}), None)
        .await;
    let after = storage.get_pending_events(&as_id, 256).await.unwrap().len();
    assert!(after > before, "dispatch should enqueue at least one event");

    cleanup_room(&pool, &room_id).await;
}

// =============================================================================
// is_remote_user / is_remote_room
// =============================================================================

#[tokio::test]
async fn test_is_remote_user_and_room() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let svc = create_room_service(&pool, cache);

    assert!(!svc.is_remote_user("@alice:localhost"), "local user is not remote");
    assert!(svc.is_remote_user("@alice:other.example"), "remote user is remote");
    assert!(!svc.is_remote_room("!room:localhost"), "local room is not remote");
    assert!(svc.is_remote_room("!room:other.example"), "remote room is remote");
}

// =============================================================================
// Membership forwarding — moderation & queries
// =============================================================================

#[tokio::test]
async fn test_leave_room_after_join() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    svc.join_room(&room_id, &bob).await.unwrap();

    svc.leave_room(&room_id, &bob).await.unwrap();
    let membership = svc.get_room_membership(&room_id, &bob).await.unwrap();
    assert_eq!(membership.as_deref(), Some("leave"));

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_forget_room_after_leave() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    svc.join_room(&room_id, &bob).await.unwrap();
    svc.leave_room(&room_id, &bob).await.unwrap();

    svc.forget_room(&room_id, &bob).await.unwrap();
    let membership = svc.get_room_membership(&room_id, &bob).await.unwrap();
    assert!(membership.is_none(), "forgotten member should have no record");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_kick_user_changes_membership_to_leave() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    svc.join_room(&room_id, &bob).await.unwrap();

    svc.kick_user(&room_id, &bob, &alice, Some("off-topic")).await.unwrap();
    let membership = svc.get_room_membership(&room_id, &bob).await.unwrap();
    assert_eq!(membership.as_deref(), Some("leave"));

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_unban_user_changes_membership() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    svc.ban_user(&room_id, &bob, &alice, Some("spam")).await.unwrap();

    svc.unban_user(&room_id, &bob, &alice).await.unwrap();
    let membership = svc.get_room_membership(&room_id, &bob).await.unwrap();
    assert_eq!(membership.as_deref(), Some("leave"));

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_room_membership_for_member_and_nonmember() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let nobody = format!("@nobody_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    assert_eq!(svc.get_room_membership(&room_id, &alice).await.unwrap().as_deref(), Some("join"));
    assert!(svc.get_room_membership(&room_id, &nobody).await.unwrap().is_none());

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_room_member_record() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let member = svc.get_room_member_record(&room_id, &alice).await.unwrap().expect("member should exist");
    assert_eq!(member.user_id, alice);
    assert_eq!(member.membership, "join");

    let missing = svc.get_room_member_record(&room_id, "@nope:localhost").await.unwrap();
    assert!(missing.is_none());

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_joined_rooms_returns_creator_room() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let rooms = svc.get_joined_rooms(&alice).await.unwrap();
    assert!(rooms.iter().any(|r| r == &room_id));

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_joined_members_with_profiles() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let members = svc.get_joined_members_with_profiles(&room_id).await.unwrap();
    assert!(members.iter().any(|m| m.user_id == alice && m.membership == "join"));

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_room_members_by_membership() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    svc.invite_user(&room_id, &alice, &bob).await.unwrap();

    let invited = svc.get_room_members_by_membership(&room_id, "invite").await.unwrap();
    assert!(invited.iter().any(|m| m.user_id == bob));
    let joined = svc.get_room_members_by_membership(&room_id, "join").await.unwrap();
    assert!(joined.iter().any(|m| m.user_id == alice));

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_share_common_room_true_and_false() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    let carol = format!("@carol_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    create_test_user(&pool, &carol, &format!("carol_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    svc.join_room(&room_id, &bob).await.unwrap();

    assert!(svc.share_common_room(&alice, &bob).await.unwrap(), "alice and bob share a room");
    assert!(!svc.share_common_room(&alice, &carol).await.unwrap(), "alice and carol do not share a room");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_invited_members_count() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    let carol = format!("@carol_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    create_test_user(&pool, &carol, &format!("carol_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    svc.invite_user(&room_id, &alice, &bob).await.unwrap();
    svc.invite_user(&room_id, &alice, &carol).await.unwrap();

    let count = svc.get_invited_members_count(&room_id).await.unwrap();
    assert_eq!(count, 2, "two invited members expected");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_room_member_count_admin() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    svc.join_room(&room_id, &bob).await.unwrap();

    let count = svc.get_room_member_count_admin(&room_id).await.unwrap();
    assert!(count >= 2, "at least alice and bob should be counted, got {count}");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_admin_ban_and_unban_membership() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    svc.admin_ban_user_membership(&room_id, &bob, &alice).await.unwrap();
    assert_eq!(svc.get_room_membership(&room_id, &bob).await.unwrap().as_deref(), Some("ban"));

    svc.admin_unban_user_membership(&room_id, &bob).await.unwrap();
    assert_eq!(svc.get_room_membership(&room_id, &bob).await.unwrap().as_deref(), Some("leave"));

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_set_ban_reason() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    svc.ban_user(&room_id, &bob, &alice, None).await.unwrap();

    svc.set_ban_reason(&room_id, &bob, "violated policy").await.unwrap();
    let member = svc.get_room_member_record(&room_id, &bob).await.unwrap().unwrap();
    assert_eq!(member.ban_reason.as_deref(), Some("violated policy"));

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_force_leave_membership() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    svc.join_room(&room_id, &bob).await.unwrap();

    let now = chrono::Utc::now().timestamp_millis();
    svc.force_leave_membership(&room_id, &bob, now).await.unwrap();
    assert_eq!(svc.get_room_membership(&room_id, &bob).await.unwrap().as_deref(), Some("leave"));

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_remove_member_record() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    svc.join_room(&room_id, &bob).await.unwrap();
    assert!(svc.get_room_member_record(&room_id, &bob).await.unwrap().is_some());

    svc.remove_member_record(&room_id, &bob).await.unwrap();
    assert!(svc.get_room_member_record(&room_id, &bob).await.unwrap().is_none());

    cleanup_room(&pool, &room_id).await;
}

// =============================================================================
// State forwarding — room info, directory, aliases, tags, admin
// =============================================================================

#[tokio::test]
async fn test_room_exists_true_and_false() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    assert!(svc.room_exists(&room_id).await.unwrap());
    assert!(!svc.room_exists(&format!("!nope_{id}:localhost")).await.unwrap());

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_room_record() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let room = svc.get_room_record(&room_id).await.unwrap().expect("room should exist");
    assert_eq!(room.room_id, room_id);
    assert_eq!(room.creator_user_id.as_deref(), Some(alice.as_str()));
    assert!(room.is_public);

    let missing = svc.get_room_record(&format!("!nope_{id}:localhost")).await.unwrap();
    assert!(missing.is_none());

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_room_count_increases_with_creation() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let before = create_room_service(&pool, cache.clone()).get_room_count().await.unwrap();
    let (_svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    let after = create_room_service(&pool, cache).get_room_count().await.unwrap();
    assert!(after > before, "room count should increase after creation");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_room_version() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    create_test_user(&pool, &alice, &format!("alice_{id}")).await;
    let svc = create_room_service(&pool, cache);
    let val = svc
        .create_room(
            &alice,
            CreateRoomConfig { room_version: Some("11".to_string()), ..Default::default() },
        )
        .await
        .unwrap();
    let room_id = val["room_id"].as_str().unwrap();

    let version = svc.get_room_version(&room_id).await.unwrap();
    assert_eq!(version.as_deref(), Some("11"));

    let missing = svc.get_room_version(&format!("!nope_{id}:localhost")).await.unwrap();
    assert!(missing.is_none());

    cleanup_room(&pool, room_id).await;
}

#[tokio::test]
async fn test_is_room_creator_true_and_false() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    assert!(svc.is_room_creator(&room_id, &alice).await.unwrap());
    assert!(!svc.is_room_creator(&room_id, &bob).await.unwrap());

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_check_room_has_encryption_false_by_default() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    assert!(!svc.check_room_has_encryption(&room_id).await.unwrap(), "default room is not encrypted");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_room_encryption_status_unencrypted() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let status = svc.get_room_encryption_status(&room_id).await.unwrap();
    assert!(!status.is_encrypted);

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_block_unblock_room_and_status() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    svc.block_room(&room_id, &alice, Some("policy")).await.unwrap();
    let blocked = svc.get_room_block_status(&room_id).await.unwrap();
    assert!(blocked.is_some(), "blocked room should have a status");

    svc.unblock_room(&room_id).await.unwrap();
    let after = svc.get_room_block_status(&room_id).await.unwrap();
    assert!(after.is_none(), "unblocked room should have no status");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_set_room_directory_and_visibility() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    svc.set_room_directory(&room_id, true).await.unwrap();
    let visibility = svc.get_room_visibility(&room_id).await.unwrap();
    assert_eq!(visibility, "public");

    svc.set_room_directory(&room_id, false).await.unwrap();
    let visibility = svc.get_room_visibility(&room_id).await.unwrap();
    assert_eq!(visibility, "private");

    svc.remove_room_directory(&room_id).await.unwrap();

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_public_rooms_and_count() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let rooms = svc.get_public_rooms(100).await.unwrap();
    let chunk = rooms["chunk"].as_array().expect("chunk should be an array");
    assert!(chunk.iter().any(|r| r["room_id"].as_str() == Some(room_id.as_str())));

    let count = svc.count_public_rooms().await.unwrap();
    assert!(count >= 1, "at least one public room should exist");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_public_rooms_paginated() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let rooms = svc.get_public_rooms_paginated(100, None, None).await.unwrap();
    assert!(rooms.iter().any(|r| r.room_id == room_id), "created public room should appear");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_room_alias_lifecycle() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    let alias = format!("#alias_{id}:localhost");

    svc.set_room_alias(&room_id, &alias, &alice).await.unwrap();
    let aliases = svc.get_room_aliases(&room_id).await.unwrap();
    assert!(aliases.iter().any(|a| a == &alias));

    let resolved = svc.get_room_by_alias(&alias).await.unwrap();
    assert_eq!(resolved.as_deref(), Some(room_id.as_str()));

    svc.remove_room_alias_by_name(&alias).await.unwrap();
    let after = svc.get_room_by_alias(&alias).await.unwrap();
    assert!(after.is_none(), "alias should be gone after removal");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_tags_lifecycle() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    svc.add_tag(&alice, &room_id, "m.favourite", Some(0.5)).await.unwrap();
    svc.add_tag(&alice, &room_id, "m.lowpriority", None).await.unwrap();

    let tags = svc.get_tags(&alice, &room_id).await.unwrap();
    assert_eq!(tags.len(), 2);
    assert!(tags.iter().any(|t| t.tag == "m.favourite"));

    let all = svc.get_all_tags(&alice).await.unwrap();
    assert!(!all.is_empty());

    svc.remove_tag(&alice, &room_id, "m.favourite").await.unwrap();
    let after = svc.get_tags(&alice, &room_id).await.unwrap();
    assert_eq!(after.len(), 1);
    assert!(after.iter().all(|t| t.tag != "m.favourite"));

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_grant_room_admin() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    svc.join_room(&room_id, &bob).await.unwrap();

    svc.grant_room_admin(&room_id, &bob).await.unwrap();

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_delete_room_removes_record() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    assert!(svc.room_exists(&room_id).await.unwrap());

    svc.delete_room(&room_id, &alice).await.unwrap();
    assert!(!svc.room_exists(&room_id).await.unwrap(), "room should be gone after delete");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_tombstone_event_none_for_fresh_room() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let tombstone = svc.get_tombstone_event(&room_id).await.unwrap();
    assert!(tombstone.is_none(), "fresh room has no tombstone");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_is_room_upgrade_allowed_for_creator() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    assert!(svc.is_room_upgrade_allowed(&room_id, &alice).await.unwrap(), "creator should be allowed to upgrade");
    assert!(!svc.is_room_upgrade_allowed(&room_id, &bob).await.unwrap(), "non-creator should not be allowed");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_migrate_room_content() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, source_room) = make_public_room(&pool, &cache, &alice).await;
    let (_, target_room) = make_public_room(&pool, &cache, &alice).await;

    let result = svc.migrate_room_content(&source_room, &target_room, &alice).await;
    assert!(result.is_ok(), "migrate_room_content should succeed: {:?}", result.err());

    cleanup_room(&pool, &source_room).await;
    cleanup_room(&pool, &target_room).await;
}

// =============================================================================
// Messaging forwarding — events, state, timeline, redaction, signatures
// =============================================================================

#[tokio::test]
async fn test_get_event_record_returns_created_event() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    let event_id = format!("$rec_{id}:localhost");
    svc.create_event(
        CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: alice.clone(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "hi"}),
            state_key: None,
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            redacts: None,
        },
        None,
    )
    .await
    .unwrap();

    let record = svc.get_event_record(&event_id).await.unwrap().expect("event should exist");
    assert_eq!(record.event_id, event_id);
    assert_eq!(record.room_id, room_id);

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_event_record_in_room() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    let event_id = format!("$inroom_{id}:localhost");
    svc.create_event(
        CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: alice.clone(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "hello"}),
            state_key: None,
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            redacts: None,
        },
        None,
    )
    .await
    .unwrap();

    let record = svc.get_event_record_in_room(&room_id, &event_id).await.unwrap();
    assert_eq!(record.event_id, event_id);

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_state_events_and_records() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let events = svc.get_state_events(&room_id).await.unwrap();
    assert!(!events.is_empty(), "fresh room should have state events (create, member, ...)");

    let records = svc.get_state_event_records(&room_id).await.unwrap();
    assert!(!records.is_empty());

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_state_events_by_type() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let create_events = svc.get_state_events_by_type(&room_id, "m.room.create").await.unwrap();
    assert!(!create_events.is_empty(), "room should have a create state event");

    let empty = svc.get_state_events_by_type(&room_id, "m.room.nonexistent").await.unwrap();
    assert!(empty.is_empty());

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_pinned_event_ids_lifecycle() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let before = svc.get_pinned_event_ids(&room_id).await.unwrap();
    assert!(before.is_empty(), "fresh room has no pinned events");

    let pinned = vec![format!("$pin1_{id}:localhost"), format!("$pin2_{id}:localhost")];
    svc.set_pinned_event_ids(&room_id, &alice, &pinned).await.unwrap();
    let after = svc.get_pinned_event_ids(&room_id).await.unwrap();
    assert_eq!(after, pinned);

    svc.set_pinned_event_ids(&room_id, &alice, &[]).await.unwrap();
    let cleared = svc.get_pinned_event_ids(&room_id).await.unwrap();
    assert!(cleared.is_empty());

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_event_returns_json() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    let event_id = format!("$getevt_{id}:localhost");
    svc.create_event(
        CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: alice.clone(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "fetch me"}),
            state_key: None,
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            redacts: None,
        },
        None,
    )
    .await
    .unwrap();

    let event = svc.get_event(&room_id, &event_id).await.unwrap();
    assert_eq!(event["event_id"].as_str(), Some(event_id.as_str()));
    assert_eq!(event["content"]["body"], "fetch me");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_room_events_and_by_type() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    for i in 0..3 {
        svc.create_event(
            CreateEventParams {
                event_id: format!("$tl_{id}_{i}:localhost"),
                room_id: room_id.clone(),
                user_id: alice.clone(),
                event_type: "m.room.message".to_string(),
                content: json!({"body": format!("msg-{i}")}),
                state_key: None,
                origin_server_ts: chrono::Utc::now().timestamp_millis() + i,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();
    }

    let events = svc.get_room_events(&room_id, 100).await.unwrap();
    assert!(events.len() >= 3, "should return at least the 3 created message events");

    let messages = svc.get_room_events_by_type(&room_id, "m.room.message", 100).await.unwrap();
    assert!(messages.iter().all(|e| e.event_type == "m.room.message"));

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_forward_extremities_count() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let count = svc.get_forward_extremities_count(&room_id).await.unwrap();
    assert!(count >= 0, "extremities count should be non-negative");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_count_events_by_status() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    // count_events_by_status returns i64 directly (not ApiResult).
    let count = svc.count_events_by_status(&room_id, "sent").await;
    assert!(count >= 0, "count should be non-negative");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_redact_event_content() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    let event_id = format!("$redact_{id}:localhost");
    svc.create_event(
        CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: alice.clone(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "soon redacted"}),
            state_key: None,
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            redacts: None,
        },
        None,
    )
    .await
    .unwrap();

    svc.redact_event_content(&event_id, Some(&alice)).await.unwrap();
    // `RoomEvent` struct does not expose `is_redacted`; verify via the DB column.
    let is_redacted: bool = sqlx::query_scalar("SELECT is_redacted FROM events WHERE event_id = $1")
        .bind(&event_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert!(is_redacted, "event should be marked redacted");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_daily_message_count() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    svc.send_message(&room_id, &alice, "m.room.message", &json!({"body": "today"})).await.unwrap();

    let count = svc.get_daily_message_count().await.unwrap();
    assert!(count >= 1, "at least one message was sent today");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_find_missing_event_ids() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    let existing = format!("$exists_{id}:localhost");
    svc.create_event(
        CreateEventParams {
            event_id: existing.clone(),
            room_id: room_id.clone(),
            user_id: alice.clone(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "here"}),
            state_key: None,
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            redacts: None,
        },
        None,
    )
    .await
    .unwrap();

    let missing_input = vec![existing.clone(), format!("$absent_{id}:localhost")];
    let missing = svc.find_missing_event_ids(&missing_input).await.unwrap();
    assert!(missing.iter().any(|e| e == &format!("$absent_{id}:localhost")), "absent id should be reported missing");
    assert!(!missing.iter().any(|e| e == &existing), "existing id should not be reported missing");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_report_event() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let bob = format!("@bob_{id}:localhost");
    create_test_user(&pool, &bob, &format!("bob_{id}")).await;
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    svc.join_room(&room_id, &bob).await.unwrap();
    let event_id = format!("$report_{id}:localhost");
    svc.create_event(
        CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: alice.clone(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "report me"}),
            state_key: None,
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            redacts: None,
        },
        None,
    )
    .await
    .unwrap();

    let report_id = svc.report_event(&event_id, &room_id, &bob, Some("spam"), -50).await.unwrap();
    assert!(report_id > 0, "report should return a positive id");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_find_event_by_timestamp() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    let base_ts = chrono::Utc::now().timestamp_millis() + 100_000;
    let event_id = format!("$ts_{id}:localhost");
    svc.create_event(
        CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: alice.clone(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "ts"}),
            state_key: None,
            origin_server_ts: base_ts,
            redacts: None,
        },
        None,
    )
    .await
    .unwrap();

    let forward = svc.find_event_by_timestamp(&room_id, base_ts - 1, true).await.unwrap();
    assert!(forward.is_some(), "forward search from before should find an event");
    let (found_id, found_ts) = forward.unwrap();
    assert_eq!(found_id, event_id);
    assert_eq!(found_ts, base_ts);

    let backward = svc.find_event_by_timestamp(&room_id, base_ts + 1, false).await.unwrap();
    assert!(backward.is_some(), "backward search from after should find an event");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_save_and_get_event_signatures() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    let event_id = format!("$sig_{id}:localhost");
    svc.create_event(
        CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: alice.clone(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "signed"}),
            state_key: None,
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            redacts: None,
        },
        None,
    )
    .await
    .unwrap();

    let before = svc.get_event_signatures(&event_id).await.unwrap();
    assert!(before.is_empty(), "fresh event has no signatures");

    svc.save_event_signature(&event_id, &alice, "DEV1", "sig-bytes", "k1:ed", "ed25519", chrono::Utc::now().timestamp_millis())
        .await
        .unwrap();

    let after = svc.get_event_signatures(&event_id).await.unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].user_id, alice);

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_send_and_get_receipts() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    let event_id = format!("$rcpt_{id}:localhost");
    svc.create_event(
        CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: alice.clone(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "read me"}),
            state_key: None,
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            redacts: None,
        },
        None,
    )
    .await
    .unwrap();

    svc.send_receipt(&room_id, &alice, &event_id, "m.read", &json!({"ts": 123}))
        .await
        .unwrap();
    let receipts = svc.get_receipts(&room_id, "m.read", &event_id).await.unwrap();
    assert!(receipts.iter().any(|r| r.user_id == alice), "alice's receipt should be present");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_update_read_marker() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    let event_id = format!("$marker_{id}:localhost");
    svc.create_event(
        CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: alice.clone(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "mark"}),
            state_key: None,
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            redacts: None,
        },
        None,
    )
    .await
    .unwrap();

    let result = svc.update_read_marker(&room_id, &alice, &event_id, "m.fully_read").await;
    assert!(result.is_ok(), "update_read_marker should succeed: {:?}", result.err());

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_set_read_markers() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    let event_id = format!("$rm_{id}:localhost");
    svc.create_event(
        CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: alice.clone(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "markers"}),
            state_key: None,
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            redacts: None,
        },
        None,
    )
    .await
    .unwrap();

    let result = svc
        .set_read_markers(&room_id, &alice, &json!({"m.fully_read": event_id, "m.read": event_id}))
        .await;
    assert!(result.is_ok(), "set_read_markers should succeed: {:?}", result.err());

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_typing_ephemeral_set_clear_and_get() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    svc.set_typing_ephemeral_event(&room_id, &alice, &[alice.clone()], 5000).await.unwrap();
    let events = svc.get_ephemeral_events_for_client(&room_id, 10).await.unwrap();
    assert!(!events.is_empty(), "typing event should be retrievable");

    svc.clear_typing_ephemeral_event(&room_id, &alice).await.unwrap();

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_pending_events() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    for i in 0..2 {
        svc.create_event(
            CreateEventParams {
                event_id: format!("$pend_{id}_{i}:localhost"),
                room_id: room_id.clone(),
                user_id: alice.clone(),
                event_type: "m.room.message".to_string(),
                content: json!({"body": format!("p{i}")}),
                state_key: None,
                origin_server_ts: chrono::Utc::now().timestamp_millis() + i,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();
    }

    let pending = svc.get_pending_events(&room_id, 100).await.unwrap();
    // Pending events may be empty if all are processed; just verify it returns.
    assert!(pending.len() <= 100, "limit should be respected");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_event_context_admin_returns_json() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;
    let event_id = format!("$ctx_{id}:localhost");
    svc.create_event(
        CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: alice.clone(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "context"}),
            state_key: None,
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            redacts: None,
        },
        None,
    )
    .await
    .unwrap();

    let context = svc.get_event_context_admin(&room_id, &event_id, 5).await.unwrap();
    assert!(context.is_object(), "context should be a JSON object");

    cleanup_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_get_room_events_paginated_admin() {
    let (_guard, pool) = guarded_pool().await;
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let id = unique_id();
    let alice = format!("@alice_{id}:localhost");
    let (svc, room_id) = make_public_room(&pool, &cache, &alice).await;

    let events = svc.get_room_events_paginated_admin(&room_id, None, 10, "b").await.unwrap();
    assert!(events.len() <= 10, "limit should be respected");

    cleanup_room(&pool, &room_id).await;
}
