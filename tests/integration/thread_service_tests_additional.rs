//! Integration tests for the thread service layer (`synapse-services/src/thread_service.rs`).
//!
//! Covers all 23 public methods of `ThreadService`, exercising the service-layer
//! business logic directly against the shared integration Postgres pool:
//! thread creation (UUID generation + self-relation), reply validation
//! (missing/frozen thread rejection), subscription validation, read receipts,
//! thread listing with summary fallback, search, freeze/unfreeze, redaction
//! and edit marking.
//!
//! Follows the warm_up_pool + Mutex guard + unique_id pattern. Only compilation
//! is verified in CI without a live database; the tests themselves are not run
//! here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::await_holding_lock)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use synapse_common::error::ApiError;
use synapse_services::thread_service::{
    CreateReplyRequest, CreateThreadRequest, GetThreadRequest, ListThreadsRequest, MarkReadRequest,
    SubscribeRequest, ThreadService,
};
use synapse_storage::thread::{CreateThreadRootParams, ThreadStorage};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn thread_service_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Warm up the shared pool on the current tokio runtime.
/// SELECT 1 with 8 retries and 400ms backoff fixes cross-runtime sqlx pool isolation.
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

/// Ensure all thread-related tables exist. These are no-ops when migrations
/// have already created the canonical schema; the local definitions mirror the
/// migration shapes so the file is self-contained for fresh databases.
async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            user_id TEXT NOT NULL PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT,
            is_admin BOOLEAN DEFAULT FALSE,
            is_guest BOOLEAN DEFAULT FALSE,
            creation_ts BIGINT NOT NULL,
            deactivated BOOLEAN DEFAULT FALSE,
            displayname TEXT,
            avatar_url TEXT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS rooms (
            room_id TEXT NOT NULL PRIMARY KEY,
            creator TEXT,
            is_public BOOLEAN DEFAULT FALSE,
            room_version TEXT DEFAULT '6',
            created_ts BIGINT NOT NULL,
            last_activity_ts BIGINT,
            is_federated BOOLEAN DEFAULT TRUE,
            has_guest_access BOOLEAN DEFAULT FALSE,
            join_rules TEXT DEFAULT 'invite',
            history_visibility TEXT DEFAULT 'shared',
            name TEXT,
            topic TEXT,
            avatar_url TEXT,
            canonical_alias TEXT,
            visibility TEXT DEFAULT 'private'
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS events (
            event_id TEXT NOT NULL PRIMARY KEY,
            room_id TEXT NOT NULL,
            sender TEXT NOT NULL,
            event_type TEXT NOT NULL,
            content JSONB NOT NULL,
            origin_server_ts BIGINT NOT NULL,
            state_key TEXT,
            is_redacted BOOLEAN DEFAULT FALSE,
            redacted_at BIGINT,
            redacted_by TEXT,
            transaction_id TEXT,
            depth BIGINT,
            prev_events JSONB,
            auth_events JSONB,
            signatures JSONB,
            hashes JSONB,
            unsigned JSONB DEFAULT '{}',
            processed_at BIGINT,
            not_before BIGINT DEFAULT 0,
            status TEXT,
            reference_image TEXT,
            origin TEXT,
            user_id TEXT,
            redacts TEXT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS thread_roots (
            id BIGSERIAL PRIMARY KEY,
            room_id TEXT NOT NULL,
            root_event_id TEXT NOT NULL,
            sender TEXT NOT NULL,
            thread_id TEXT,
            reply_count BIGINT DEFAULT 0,
            last_reply_event_id TEXT,
            last_reply_sender TEXT,
            last_reply_ts BIGINT,
            participants JSONB DEFAULT '[]',
            is_fetched BOOLEAN DEFAULT FALSE,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS thread_replies (
            id BIGSERIAL PRIMARY KEY,
            room_id TEXT NOT NULL,
            thread_id TEXT NOT NULL,
            event_id TEXT NOT NULL,
            root_event_id TEXT NOT NULL,
            sender TEXT NOT NULL,
            in_reply_to_event_id TEXT,
            content JSONB NOT NULL DEFAULT '{}',
            origin_server_ts BIGINT NOT NULL,
            is_edited BOOLEAN NOT NULL DEFAULT FALSE,
            is_redacted BOOLEAN NOT NULL DEFAULT FALSE,
            created_ts BIGINT NOT NULL
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS thread_relations (
            id BIGSERIAL PRIMARY KEY,
            room_id TEXT NOT NULL,
            event_id TEXT NOT NULL,
            relates_to_event_id TEXT NOT NULL,
            relation_type TEXT NOT NULL,
            thread_id TEXT,
            is_falling_back BOOLEAN NOT NULL DEFAULT FALSE,
            created_ts BIGINT NOT NULL
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS thread_subscriptions (
            id BIGSERIAL PRIMARY KEY,
            room_id TEXT NOT NULL,
            thread_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            notification_level TEXT DEFAULT 'all',
            is_muted BOOLEAN DEFAULT FALSE,
            is_pinned BOOLEAN DEFAULT FALSE,
            subscribed_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL,
            CONSTRAINT uq_thread_subscriptions UNIQUE (room_id, thread_id, user_id)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS thread_read_receipts (
            id BIGSERIAL PRIMARY KEY,
            room_id TEXT NOT NULL,
            thread_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            last_read_event_id TEXT,
            last_read_ts BIGINT NOT NULL DEFAULT 0,
            unread_count INTEGER NOT NULL DEFAULT 0,
            updated_ts BIGINT NOT NULL,
            CONSTRAINT uq_thread_read_receipts UNIQUE (room_id, thread_id, user_id)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();
}

/// Clean up all thread-related rows owned by this test file (prefix `tsts_`).
/// Child tables first to respect FK constraints.
async fn cleanup_owned(pool: &Arc<sqlx::PgPool>) {
    sqlx::query("DELETE FROM thread_read_receipts WHERE user_id LIKE 'tsts_%' OR room_id LIKE 'tsts_%'")
        .execute(pool.as_ref())
        .await
        .ok();
    sqlx::query("DELETE FROM thread_subscriptions WHERE user_id LIKE 'tsts_%' OR room_id LIKE 'tsts_%'")
        .execute(pool.as_ref())
        .await
        .ok();
    sqlx::query("DELETE FROM thread_relations WHERE room_id LIKE 'tsts_%'")
        .execute(pool.as_ref())
        .await
        .ok();
    sqlx::query("DELETE FROM thread_replies WHERE room_id LIKE 'tsts_%'")
        .execute(pool.as_ref())
        .await
        .ok();
    sqlx::query("DELETE FROM thread_roots WHERE room_id LIKE 'tsts_%'")
        .execute(pool.as_ref())
        .await
        .ok();
    sqlx::query("DELETE FROM events WHERE room_id LIKE 'tsts_%' OR sender LIKE 'tsts_%'")
        .execute(pool.as_ref())
        .await
        .ok();
    sqlx::query("DELETE FROM rooms WHERE room_id LIKE 'tsts_%'")
        .execute(pool.as_ref())
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE user_id LIKE 'tsts_%'")
        .execute(pool.as_ref())
        .await
        .ok();
}

/// Build a `ThreadService` backed by the shared pool.
fn create_service(pool: &Arc<sqlx::PgPool>) -> ThreadService {
    let storage = Arc::new(ThreadStorage::new(pool));
    ThreadService::new(storage)
}

/// Seed a unique room + creator + replier + reader for a test.
/// Returns `(creator, replier, reader, room_id)`.
async fn seed_room(
    pool: &Arc<sqlx::PgPool>,
    uid: u64,
) -> (String, String, String, String) {
    let creator = format!("tsts_creator_{uid}:localhost");
    let replier = format!("tsts_replier_{uid}:localhost");
    let reader = format!("tsts_reader_{uid}:localhost");
    let room_id = format!("!tsts_room_{uid}:localhost");

    for (user_id, username) in
        [(&creator, "creator"), (&replier, "replier"), (&reader, "reader")]
            .into_iter()
            .map(|(u, role)| (u, format!("tsts_{role}_{uid}")))
    {
        sqlx::query(
            "INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, 0) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .execute(pool.as_ref())
        .await
        .ok();
    }

    sqlx::query("INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, 0) ON CONFLICT (room_id) DO NOTHING")
        .bind(&room_id)
        .bind(&creator)
        .execute(pool.as_ref())
        .await
        .ok();

    (creator, replier, reader, room_id)
}

/// Insert an event row so that `get_thread_summary` / `search_threads` joins resolve.
async fn insert_event(
    pool: &Arc<sqlx::PgPool>,
    room_id: &str,
    sender: &str,
    event_id: &str,
    body: &str,
    origin_server_ts: i64,
) {
    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts)
        VALUES ($1, $2, $3, 'm.room.message', $4, $5)
        ON CONFLICT (event_id) DO UPDATE SET content = EXCLUDED.content
        "#,
    )
    .bind(event_id)
    .bind(room_id)
    .bind(sender)
    .bind(serde_json::json!({ "msgtype": "m.text", "body": body }))
    .bind(origin_server_ts)
    .execute(pool.as_ref())
    .await
    .ok();
}

/// Acquire the guard, set up DB, and return a freshly-built service. Cleans
/// prior rows owned by this file so tests start from a known state.
async fn prepare() -> (std::sync::MutexGuard<'static, ()>, ThreadService, Arc<sqlx::PgPool>) {
    let guard = thread_service_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    cleanup_owned(&pool).await;
    let service = create_service(&pool);
    (guard, service, pool)
}

/// Create a thread via the service and return `(thread_id, root_event_id, room_id, creator)`.
async fn make_thread(
    service: &ThreadService,
    pool: &Arc<sqlx::PgPool>,
    uid: u64,
) -> (String, String, String, String) {
    let (creator, _replier, _reader, room_id) = seed_room(pool, uid).await;
    let root_event_id = format!("$tsts_root_{uid}:localhost");
    let root = service
        .create_thread(
            &creator,
            CreateThreadRequest {
                room_id: room_id.clone(),
                root_event_id: root_event_id.clone(),
            },
        )
        .await
        .expect("create_thread should succeed");
    let thread_id = root.thread_id.clone().expect("service should assign thread_id");
    (thread_id, root_event_id, room_id, creator)
}

// =============================================================================
// new
// =============================================================================

#[tokio::test]
async fn test_new_constructs_service() {
    let (_guard, service, _pool) = prepare().await;
    // A trivial delegation proves the service was constructed with a usable storage.
    let _ = service.get_thread_root("!tsts_none:localhost", "$none").await;
}

// =============================================================================
// create_thread
// =============================================================================

#[tokio::test]
async fn test_create_thread_generates_uuid_thread_id() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (creator, _replier, _reader, room_id) = seed_room(&pool, uid).await;
    let root_event_id = format!("$tsts_root_{uid}:localhost");

    let root = service
        .create_thread(
            &creator,
            CreateThreadRequest { room_id: room_id.clone(), root_event_id: root_event_id.clone() },
        )
        .await
        .expect("create_thread should succeed");

    // Service generates a thread_id of the form "$<uuid>".
    let thread_id = root.thread_id.as_ref().expect("thread_id should be set");
    assert!(thread_id.starts_with('$'), "thread_id should start with '$', got {thread_id}");
    assert!(thread_id.len() > 1, "thread_id should have a non-empty body");
    assert_eq!(root.room_id, room_id);
    assert_eq!(root.root_event_id, root_event_id);
    assert_eq!(root.sender, creator);
    assert_eq!(root.reply_count, 0);
    assert!(!root.is_fetched);
    assert!(root.last_reply_event_id.is_none());
}

#[tokio::test]
async fn test_create_thread_creates_self_relation() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;

    // The service creates a relation where the root event relates to itself.
    let relation: Option<(String, String, String)> = sqlx::query_as(
        "SELECT event_id, relates_to_event_id, relation_type FROM thread_relations WHERE room_id = $1 AND event_id = $2",
    )
    .bind(&room_id)
    .bind(&root_event_id)
    .fetch_optional(pool.as_ref())
    .await
    .expect("query thread_relations");
    let (event_id, relates_to, rel_type) = relation.expect("self-relation should exist");
    assert_eq!(event_id, root_event_id);
    assert_eq!(relates_to, root_event_id, "root should relate to itself");
    assert_eq!(rel_type, "m.thread");

    // No thread_id column mismatch.
    let stored_thread_id: Option<String> =
        sqlx::query_scalar("SELECT thread_id FROM thread_relations WHERE room_id = $1 AND event_id = $2")
            .bind(&room_id)
            .bind(&root_event_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("fetch thread_id from thread_relations");
    assert_eq!(stored_thread_id.as_deref(), Some(thread_id.as_str()));
}

// =============================================================================
// get_thread_root
// =============================================================================

#[tokio::test]
async fn test_get_thread_root_existing() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, _root_event_id, _room_id, _creator) = make_thread(&service, &pool, uid).await;

    let loaded = service
        .get_thread_root(&_format_room(uid), &thread_id)
        .await
        .expect("get_thread_root should succeed");
    assert!(loaded.is_some(), "thread root should be retrievable by (room_id, thread_id)");
    assert_eq!(loaded.unwrap().thread_id.as_deref(), Some(thread_id.as_str()));
}

fn _format_room(uid: u64) -> String {
    format!("!tsts_room_{uid}:localhost")
}

#[tokio::test]
async fn test_get_thread_root_nonexistent_returns_none() {
    let (_guard, service, _pool) = prepare().await;
    let result = service
        .get_thread_root("!tsts_missing:localhost", "$missing_thread")
        .await
        .expect("get_thread_root should not error on missing thread");
    assert!(result.is_none());
}

// =============================================================================
// get_thread_root_by_event
// =============================================================================

#[tokio::test]
async fn test_get_thread_root_by_event_existing() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (_thread_id, root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;

    let loaded = service
        .get_thread_root_by_event(&room_id, &root_event_id)
        .await
        .expect("get_thread_root_by_event should succeed")
        .expect("thread root should exist");
    assert_eq!(loaded.root_event_id, root_event_id);
    assert_eq!(loaded.room_id, room_id);
}

#[tokio::test]
async fn test_get_thread_root_by_event_nonexistent_returns_none() {
    let (_guard, service, _pool) = prepare().await;
    let result = service
        .get_thread_root_by_event("!tsts_missing:localhost", "$missing_event")
        .await
        .expect("get_thread_root_by_event should not error on missing event");
    assert!(result.is_none());
}

// =============================================================================
// get_thread_replies
// =============================================================================

#[tokio::test]
async fn test_get_thread_replies_empty() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, _root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;

    let replies = service
        .get_thread_replies(&room_id, &thread_id, None, None)
        .await
        .expect("get_thread_replies should succeed on empty thread");
    assert!(replies.is_empty(), "freshly created thread should have no replies");
}

#[tokio::test]
async fn test_get_thread_replies_returns_inserted_replies() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, root_event_id, room_id, creator) = make_thread(&service, &pool, uid).await;
    let replier = format!("tsts_replier_{uid}:localhost");
    // Ensure replier user exists (seed_room already created it).
    let _ = &replier;

    // Insert two replies directly via the service.
    for i in 0..2 {
        let event_id = format!("$tsts_reply_{uid}_{i}:localhost");
        service
            .add_reply(
                &replier,
                CreateReplyRequest {
                    room_id: room_id.clone(),
                    thread_id: thread_id.clone(),
                    event_id: event_id.clone(),
                    root_event_id: root_event_id.clone(),
                    content: serde_json::json!({ "body": format!("reply {i}") }),
                    in_reply_to_event_id: Some(root_event_id.clone()),
                    origin_server_ts: 1000 + i,
                },
            )
            .await
            .expect("add_reply should succeed");
    }

    let replies = service
        .get_thread_replies(&room_id, &thread_id, None, None)
        .await
        .expect("get_thread_replies should succeed");
    assert_eq!(replies.len(), 2);
    // Ordered by origin_server_ts ASC.
    assert_eq!(replies[0].origin_server_ts, 1000);
    assert_eq!(replies[1].origin_server_ts, 1001);
    assert_eq!(replies[0].root_event_id, root_event_id);
    // creator is unused in this branch beyond make_thread.
    let _ = creator;
}

#[tokio::test]
async fn test_get_thread_replies_respects_limit() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let replier = format!("tsts_replier_{uid}:localhost");

    for i in 0..5 {
        let event_id = format!("$tsts_lim_{uid}_{i}:localhost");
        service
            .add_reply(
                &replier,
                CreateReplyRequest {
                    room_id: room_id.clone(),
                    thread_id: thread_id.clone(),
                    event_id,
                    root_event_id: root_event_id.clone(),
                    content: serde_json::json!({ "body": i.to_string() }),
                    in_reply_to_event_id: Some(root_event_id.clone()),
                    origin_server_ts: 2000 + i,
                },
            )
            .await
            .expect("add_reply should succeed");
    }

    let limited = service
        .get_thread_replies(&room_id, &thread_id, Some(2), None)
        .await
        .expect("get_thread_replies with limit should succeed");
    assert_eq!(limited.len(), 2, "limit should be respected");
}

// =============================================================================
// get_thread_participants
// =============================================================================

#[tokio::test]
async fn test_get_thread_participants_includes_root_and_repliers() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, root_event_id, room_id, creator) = make_thread(&service, &pool, uid).await;
    let replier = format!("tsts_replier_{uid}:localhost");

    service
        .add_reply(
            &replier,
            CreateReplyRequest {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: format!("$tsts_part_{uid}:localhost"),
                root_event_id: root_event_id.clone(),
                content: serde_json::json!({ "body": "hi" }),
                in_reply_to_event_id: Some(root_event_id.clone()),
                origin_server_ts: 5000,
            },
        )
        .await
        .expect("add_reply should succeed");

    let participants = service
        .get_thread_participants(&room_id, &thread_id)
        .await
        .expect("get_thread_participants should succeed");
    assert!(participants.contains(&creator), "creator should be a participant");
    assert!(participants.contains(&replier), "replier should be a participant");
}

#[tokio::test]
async fn test_get_thread_participants_empty_for_nonexistent() {
    let (_guard, service, _pool) = prepare().await;
    let participants = service
        .get_thread_participants("!tsts_missing:localhost", "$missing")
        .await
        .expect("get_thread_participants should not error on missing thread");
    assert!(participants.is_empty());
}

// =============================================================================
// add_reply
// =============================================================================

#[tokio::test]
async fn test_add_reply_success_creates_relation() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let replier = format!("tsts_replier_{uid}:localhost");
    let reply_event_id = format!("$tsts_reply_ok_{uid}:localhost");

    let reply = service
        .add_reply(
            &replier,
            CreateReplyRequest {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: reply_event_id.clone(),
                root_event_id: root_event_id.clone(),
                content: serde_json::json!({ "body": "first reply" }),
                in_reply_to_event_id: Some(root_event_id.clone()),
                origin_server_ts: 9000,
            },
        )
        .await
        .expect("add_reply should succeed");

    assert_eq!(reply.event_id, reply_event_id);
    assert_eq!(reply.thread_id, thread_id);
    assert_eq!(reply.root_event_id, root_event_id);
    assert_eq!(reply.sender, replier);
    assert!(!reply.is_redacted);
    assert!(!reply.is_edited);

    // A relation linking the reply to the root should have been created.
    let relation: Option<(String, String)> = sqlx::query_as(
        "SELECT relates_to_event_id, relation_type FROM thread_relations WHERE room_id = $1 AND event_id = $2",
    )
    .bind(&room_id)
    .bind(&reply_event_id)
    .fetch_optional(pool.as_ref())
    .await
    .expect("query thread_relations");
    let (relates_to, rel_type) = relation.expect("reply relation should exist");
    assert_eq!(relates_to, root_event_id);
    assert_eq!(rel_type, "m.thread");
}

#[tokio::test]
async fn test_add_reply_to_nonexistent_thread_returns_not_found() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (_creator, _replier, _reader, room_id) = seed_room(&pool, uid).await;

    let err = service
        .add_reply(
            "@tsts_replier_{uid}:localhost",
            CreateReplyRequest {
                room_id,
                thread_id: "$nonexistent_thread".to_string(),
                event_id: "$tsts_reply_missing:localhost".to_string(),
                root_event_id: "$root:localhost".to_string(),
                content: serde_json::json!({ "body": "hi" }),
                in_reply_to_event_id: None,
                origin_server_ts: 1,
            },
        )
        .await
        .expect_err("add_reply to missing thread should error");
    assert!(err.is_not_found(), "expected NotFound, got {:?}", err.kind);
    assert!(err.message.contains("Thread not found"));
}

#[tokio::test]
async fn test_add_reply_to_frozen_thread_returns_bad_request() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let replier = format!("tsts_replier_{uid}:localhost");

    // Freeze the thread via the service.
    service
        .freeze_thread(&room_id, &thread_id)
        .await
        .expect("freeze_thread should succeed");

    let err = service
        .add_reply(
            &replier,
            CreateReplyRequest {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: format!("$tsts_frozen_{uid}:localhost"),
                root_event_id: root_event_id.clone(),
                content: serde_json::json!({ "body": "frozen" }),
                in_reply_to_event_id: Some(root_event_id),
                origin_server_ts: 1,
            },
        )
        .await
        .expect_err("add_reply to frozen thread should error");
    assert!(err.is_bad_request(), "expected BadRequest, got {:?}", err.kind);
    assert!(err.message.contains("frozen"));
}

// =============================================================================
// get_thread
// =============================================================================

#[tokio::test]
async fn test_get_thread_returns_not_found_for_missing() {
    let (_guard, service, _pool) = prepare().await;
    let err = service
        .get_thread(
            GetThreadRequest {
                room_id: "!tsts_missing:localhost".to_string(),
                thread_id: "$missing".to_string(),
                include_replies: true,
                reply_limit: None,
            },
            None,
        )
        .await
        .expect_err("get_thread on missing thread should error");
    assert!(err.is_not_found());
    assert!(err.message.contains("Thread not found"));
}

#[tokio::test]
async fn test_get_thread_without_replies() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, _root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;

    let detail = service
        .get_thread(
            GetThreadRequest {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                include_replies: false,
                reply_limit: None,
            },
            None,
        )
        .await
        .expect("get_thread should succeed");
    assert_eq!(detail.root.thread_id.as_deref(), Some(thread_id.as_str()));
    assert!(detail.replies.is_empty(), "include_replies=false should yield no replies");
    assert_eq!(detail.reply_count, 0);
    // No user provided -> receipt and subscription must be None.
    assert!(detail.user_receipt.is_none());
    assert!(detail.user_subscription.is_none());
    assert!(detail.summary.is_some(), "summary should resolve even without an events row");
}

#[tokio::test]
async fn test_get_thread_with_replies_and_user_context() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, root_event_id, room_id, creator) = make_thread(&service, &pool, uid).await;
    let replier = format!("tsts_replier_{uid}:localhost");
    let reader = format!("tsts_reader_{uid}:localhost");

    // Add a reply.
    let reply_event_id = format!("$tsts_gtr_{uid}:localhost");
    service
        .add_reply(
            &replier,
            CreateReplyRequest {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: reply_event_id.clone(),
                root_event_id: root_event_id.clone(),
                content: serde_json::json!({ "body": "hi" }),
                in_reply_to_event_id: Some(root_event_id.clone()),
                origin_server_ts: 7000,
            },
        )
        .await
        .expect("add_reply should succeed");

    // Subscribe the reader and mark the reply read.
    service
        .subscribe(SubscribeRequest {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            user_id: reader.clone(),
            notification_level: "all".to_string(),
        })
        .await
        .expect("subscribe should succeed");

    service
        .mark_read(MarkReadRequest {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            user_id: reader.clone(),
            event_id: reply_event_id.clone(),
            origin_server_ts: 7000,
        })
        .await
        .expect("mark_read should succeed");

    let detail = service
        .get_thread(
            GetThreadRequest {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                include_replies: true,
                reply_limit: None,
            },
            Some(&reader),
        )
        .await
        .expect("get_thread with user should succeed");

    assert_eq!(detail.replies.len(), 1);
    assert_eq!(detail.reply_count, 1);
    assert!(detail.participants.contains(&creator));
    assert!(detail.participants.contains(&replier));
    let receipt = detail.user_receipt.expect("receipt should be present for the reader");
    assert_eq!(receipt.last_read_event_id.as_deref(), Some(reply_event_id.as_str()));
    assert_eq!(receipt.unread_count, 0, "mark_read should reset unread_count to 0");
    let subscription = detail.user_subscription.expect("subscription should be present");
    assert_eq!(subscription.notification_level, "all");
}

// =============================================================================
// list_threads
// =============================================================================

#[tokio::test]
async fn test_list_threads_empty_room() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (_creator, _replier, _reader, room_id) = seed_room(&pool, uid).await;

    let response = service
        .list_threads(ListThreadsRequest {
            room_id: room_id.clone(),
            limit: None,
            from: None,
            include_all: false,
        })
        .await
        .expect("list_threads on empty room should succeed");
    assert!(response.threads.is_empty());
    assert_eq!(response.total, 0);
    assert!(response.next_batch.is_none());
}

#[tokio::test]
async fn test_list_threads_returns_summaries_and_next_batch() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (creator, _replier, _reader, room_id) = seed_room(&pool, uid).await;

    // Create two threads in the same room.
    let mut thread_ids = Vec::new();
    for i in 0..2 {
        let root_event_id = format!("$tsts_list_{uid}_{i}:localhost");
        let root = service
            .create_thread(
                &creator,
                CreateThreadRequest { room_id: room_id.clone(), root_event_id },
            )
            .await
            .expect("create_thread should succeed");
        thread_ids.push(root.thread_id.clone().expect("thread_id should be set"));
    }

    let response = service
        .list_threads(ListThreadsRequest {
            room_id: room_id.clone(),
            limit: Some(1),
            from: None,
            include_all: false,
        })
        .await
        .expect("list_threads should succeed");
    assert_eq!(response.threads.len(), 1, "limit=1 should return one thread");
    assert!(response.next_batch.is_some(), "next_batch should be present when page is full");
    // The next_batch is the thread_id of the last returned root.
    let next = response.next_batch.unwrap();
    assert!(thread_ids.contains(&next));

    // Page 2 from the cursor.
    let page2 = service
        .list_threads(ListThreadsRequest {
            room_id: room_id.clone(),
            limit: Some(1),
            from: Some(next),
            include_all: false,
        })
        .await
        .expect("list_threads page 2 should succeed");
    assert_eq!(page2.threads.len(), 1, "one remaining thread on page 2");
    assert!(page2.next_batch.is_none(), "no further pages");
    assert_ne!(page2.threads[0].thread_id, response.threads[0].thread_id);
}

#[tokio::test]
async fn test_list_threads_summary_fallback_when_no_summary() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (creator, _replier, _reader, room_id) = seed_room(&pool, uid).await;

    // Insert a thread root directly via storage with a thread_id that has no
    // matching replies / events, so the summary CTE still resolves to a row
    // (the service fallback only triggers when get_thread_summary returns None).
    let storage = ThreadStorage::new(&pool);
    let direct_thread_id = format!("tsts_direct_{uid}");
    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$tsts_direct_root_{uid}:localhost"),
            sender: creator.clone(),
            thread_id: Some(direct_thread_id.clone()),
        })
        .await
        .expect("create_thread_root should succeed");

    let response = service
        .list_threads(ListThreadsRequest {
            room_id: room_id.clone(),
            limit: None,
            from: None,
            include_all: false,
        })
        .await
        .expect("list_threads should succeed");
    // At least the direct thread should appear; its summary may come from the
    // storage CTE or the service fallback — both paths are acceptable.
    assert!(response.threads.iter().any(|s| s.thread_id == direct_thread_id));
}

// =============================================================================
// list_all_threads
// =============================================================================

#[tokio::test]
async fn test_list_all_threads_across_rooms() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    // Two rooms, one thread each.
    let (_c1, _r1, _rd1, room1) = seed_room(&pool, uid).await;
    let (_c2, _r2, _rd2, room2) = seed_room(&pool, uid + 1_000_000).await;

    let root1 = service
        .create_thread(
            &_c1,
            CreateThreadRequest { room_id: room1.clone(), root_event_id: format!("$tsts_all_1_{uid}:localhost") },
        )
        .await
        .expect("create_thread should succeed");
    let root2 = service
        .create_thread(
            &_c2,
            CreateThreadRequest { room_id: room2.clone(), root_event_id: format!("$tsts_all_2_{uid}:localhost") },
        )
        .await
        .expect("create_thread should succeed");

    let response = service
        .list_all_threads(None, None)
        .await
        .expect("list_all_threads should succeed");
    let returned_ids: Vec<String> = response.threads.iter().map(|s| s.thread_id.clone()).collect();
    assert!(
        returned_ids.contains(root1.thread_id.as_ref().unwrap()),
        "thread from room1 should be in global list"
    );
    assert!(
        returned_ids.contains(root2.thread_id.as_ref().unwrap()),
        "thread from room2 should be in global list"
    );
    assert_eq!(response.total as usize, response.threads.len());
}

// =============================================================================
// subscribe
// =============================================================================

#[tokio::test]
async fn test_subscribe_success() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, _root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let reader = format!("tsts_reader_{uid}:localhost");

    let sub = service
        .subscribe(SubscribeRequest {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            user_id: reader.clone(),
            notification_level: "mentions".to_string(),
        })
        .await
        .expect("subscribe should succeed");
    assert_eq!(sub.room_id, room_id);
    assert_eq!(sub.thread_id, thread_id);
    assert_eq!(sub.user_id, reader);
    assert_eq!(sub.notification_level, "mentions");
    assert!(!sub.is_muted);
}

#[tokio::test]
async fn test_subscribe_to_nonexistent_returns_not_found() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (_creator, _replier, reader, room_id) = seed_room(&pool, uid).await;

    let err = service
        .subscribe(SubscribeRequest {
            room_id,
            thread_id: "$nope".to_string(),
            user_id: reader,
            notification_level: "all".to_string(),
        })
        .await
        .expect_err("subscribe to missing thread should error");
    assert!(err.is_not_found());
    assert!(err.message.contains("Thread not found"));
}

#[tokio::test]
async fn test_subscribe_to_frozen_returns_bad_request() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, _root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let reader = format!("tsts_reader_{uid}:localhost");

    service.freeze_thread(&room_id, &thread_id).await.expect("freeze_thread should succeed");

    let err = service
        .subscribe(SubscribeRequest {
            room_id,
            thread_id,
            user_id: reader,
            notification_level: "all".to_string(),
        })
        .await
        .expect_err("subscribe to frozen thread should error");
    assert!(err.is_bad_request());
    assert!(err.message.contains("frozen"));
}

#[tokio::test]
async fn test_subscribe_with_invalid_notification_level_returns_bad_request() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, _root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let reader = format!("tsts_reader_{uid}:localhost");

    let err = service
        .subscribe(SubscribeRequest {
            room_id,
            thread_id,
            user_id: reader,
            notification_level: "invalid_level".to_string(),
        })
        .await
        .expect_err("subscribe with invalid level should error");
    assert!(err.is_bad_request());
    assert!(err.message.contains("Invalid notification level"));
}

#[tokio::test]
async fn test_subscribe_upserts_existing_subscription() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, _root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let reader = format!("tsts_reader_{uid}:localhost");

    let first = service
        .subscribe(SubscribeRequest {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            user_id: reader.clone(),
            notification_level: "all".to_string(),
        })
        .await
        .expect("first subscribe should succeed");

    let second = service
        .subscribe(SubscribeRequest {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            user_id: reader.clone(),
            notification_level: "none".to_string(),
        })
        .await
        .expect("upsert subscribe should succeed");

    assert_eq!(first.id, second.id, "upsert should keep the same row id");
    assert_eq!(second.notification_level, "none", "notification_level should be updated");
    assert!(!second.is_muted, "upsert clears is_muted");
}

// =============================================================================
// unsubscribe
// =============================================================================

#[tokio::test]
async fn test_unsubscribe_removes_subscription() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, _root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let reader = format!("tsts_reader_{uid}:localhost");

    service
        .subscribe(SubscribeRequest {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            user_id: reader.clone(),
            notification_level: "all".to_string(),
        })
        .await
        .expect("subscribe should succeed");

    service
        .unsubscribe(&room_id, &thread_id, &reader)
        .await
        .expect("unsubscribe should succeed");

    // Confirm via get_thread with the user — subscription should be None.
    let detail = service
        .get_thread(
            GetThreadRequest {
                room_id,
                thread_id,
                include_replies: false,
                reply_limit: None,
            },
            Some(&reader),
        )
        .await
        .expect("get_thread should succeed");
    assert!(detail.user_subscription.is_none(), "subscription should be gone after unsubscribe");
}

// =============================================================================
// mute_thread
// =============================================================================

#[tokio::test]
async fn test_mute_thread_returns_muted_subscription() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, _root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let reader = format!("tsts_reader_{uid}:localhost");

    // Mute without a prior subscription — mute_thread upserts.
    let sub = service
        .mute_thread(&room_id, &thread_id, &reader)
        .await
        .expect("mute_thread should succeed");
    assert!(sub.is_muted, "subscription should be muted");
    assert_eq!(sub.notification_level, "none");

    // Verify persistence via get_thread.
    let detail = service
        .get_thread(
            GetThreadRequest {
                room_id,
                thread_id,
                include_replies: false,
                reply_limit: None,
            },
            Some(&reader),
        )
        .await
        .expect("get_thread should succeed");
    let persisted = detail.user_subscription.expect("subscription should exist after mute");
    assert!(persisted.is_muted);
}

// =============================================================================
// mark_read
// =============================================================================

#[tokio::test]
async fn test_mark_read_returns_receipt() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let reader = format!("tsts_reader_{uid}:localhost");

    let receipt = service
        .mark_read(MarkReadRequest {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            user_id: reader.clone(),
            event_id: root_event_id.clone(),
            origin_server_ts: 4242,
        })
        .await
        .expect("mark_read should succeed");
    assert_eq!(receipt.room_id, room_id);
    assert_eq!(receipt.thread_id, thread_id);
    assert_eq!(receipt.user_id, reader);
    assert_eq!(receipt.last_read_event_id.as_deref(), Some(root_event_id.as_str()));
    assert_eq!(receipt.unread_count, 0, "mark_read resets unread_count to 0");
}

#[tokio::test]
async fn test_mark_read_is_idempotent_upsert() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let reader = format!("tsts_reader_{uid}:localhost");

    let first = service
        .mark_read(MarkReadRequest {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            user_id: reader.clone(),
            event_id: root_event_id.clone(),
            origin_server_ts: 1,
        })
        .await
        .expect("first mark_read should succeed");

    let second_event = format!("$tsts_second_{uid}:localhost");
    let second = service
        .mark_read(MarkReadRequest {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            user_id: reader.clone(),
            event_id: second_event.clone(),
            origin_server_ts: 2,
        })
        .await
        .expect("second mark_read should upsert");

    assert_eq!(first.id, second.id, "upsert should reuse the row");
    assert_eq!(second.last_read_event_id.as_deref(), Some(second_event.as_str()));
}

// =============================================================================
// get_unread_threads
// =============================================================================

#[tokio::test]
async fn test_get_unread_threads_empty() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let reader = format!("tsts_reader_{uid}:localhost");
    // Ensure the reader user exists.
    sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, 0) ON CONFLICT DO NOTHING")
        .bind(&reader)
        .bind(format!("tsts_reader_{uid}"))
        .execute(pool.as_ref())
        .await
        .ok();

    let response = service
        .get_unread_threads(&reader, None)
        .await
        .expect("get_unread_threads should succeed on empty");
    assert!(response.threads.is_empty());
    assert_eq!(response.total_unread, 0);
    assert_eq!(response.total_threads, 0);
}

#[tokio::test]
async fn test_get_unread_threads_with_unread_count() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, _root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let reader = format!("tsts_reader_{uid}:localhost");

    // Mark read first (creates a receipt with unread_count=0), then increment unread.
    service
        .mark_read(MarkReadRequest {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            user_id: reader.clone(),
            event_id: format!("$tsts_root_{uid}:localhost"),
            origin_server_ts: 1,
        })
        .await
        .expect("mark_read should succeed");

    // Increment unread count via the underlying storage to simulate new replies.
    let storage = ThreadStorage::new(&pool);
    storage
        .increment_unread_count(&room_id, &thread_id, &reader)
        .await
        .expect("increment_unread_count should succeed");
    storage
        .increment_unread_count(&room_id, &thread_id, &reader)
        .await
        .expect("increment_unread_count should succeed");

    let response = service
        .get_unread_threads(&reader, Some(&room_id))
        .await
        .expect("get_unread_threads should succeed");
    assert_eq!(response.threads.len(), 1, "the thread should now be unread");
    assert_eq!(response.threads[0].thread_id, thread_id);
    assert_eq!(response.threads[0].unread_count, 2);
    assert_eq!(response.total_unread, 1);
    assert_eq!(response.total_threads, 1);
}

// =============================================================================
// get_subscribed_threads
// =============================================================================

#[tokio::test]
async fn test_get_subscribed_threads_returns_summaries() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, _root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let reader = format!("tsts_reader_{uid}:localhost");

    service
        .subscribe(SubscribeRequest {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            user_id: reader.clone(),
            notification_level: "all".to_string(),
        })
        .await
        .expect("subscribe should succeed");

    let response = service
        .get_subscribed_threads(&reader, None)
        .await
        .expect("get_subscribed_threads should succeed");
    assert_eq!(response.subscribed.len(), 1, "one subscription");
    assert_eq!(response.subscribed[0].thread_id, thread_id);
    assert_eq!(response.threads.len(), 1, "one summary resolved");
    assert_eq!(response.threads[0].thread_id, thread_id);
}

#[tokio::test]
async fn test_get_subscribed_threads_empty_for_user_without_subs() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let reader = format!("tsts_reader_{uid}:localhost");
    sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, 0) ON CONFLICT DO NOTHING")
        .bind(&reader)
        .bind(format!("tsts_reader_{uid}"))
        .execute(pool.as_ref())
        .await
        .ok();

    let response = service
        .get_subscribed_threads(&reader, None)
        .await
        .expect("get_subscribed_threads should succeed");
    assert!(response.subscribed.is_empty());
    assert!(response.threads.is_empty());
}

// =============================================================================
// delete_thread
// =============================================================================

#[tokio::test]
async fn test_delete_thread_removes_thread_and_relations() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let replier = format!("tsts_replier_{uid}:localhost");

    // Add a reply so we have replies + relations to cascade-delete.
    service
        .add_reply(
            &replier,
            CreateReplyRequest {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: format!("$tsts_del_{uid}:localhost"),
                root_event_id: root_event_id.clone(),
                content: serde_json::json!({ "body": "x" }),
                in_reply_to_event_id: Some(root_event_id),
                origin_server_ts: 1,
            },
        )
        .await
        .expect("add_reply should succeed");

    service
        .delete_thread(&room_id, &thread_id)
        .await
        .expect("delete_thread should succeed");

    // Root gone.
    let root = service
        .get_thread_root(&room_id, &thread_id)
        .await
        .expect("get_thread_root should succeed");
    assert!(root.is_none(), "thread root should be gone after delete_thread");

    // Replies gone.
    let replies = service
        .get_thread_replies(&room_id, &thread_id, None, None)
        .await
        .expect("get_thread_replies should succeed");
    assert!(replies.is_empty());

    // Relations gone.
    let relation_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM thread_relations WHERE room_id = $1")
            .bind(&room_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("count thread_relations");
    assert_eq!(relation_count, 0, "all relations for the room's deleted thread should be gone");
}

// =============================================================================
// get_thread_statistics
// =============================================================================

#[tokio::test]
async fn test_get_thread_statistics_returns_stats() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let replier = format!("tsts_replier_{uid}:localhost");

    for i in 0..3 {
        service
            .add_reply(
                &replier,
                CreateReplyRequest {
                    room_id: room_id.clone(),
                    thread_id: thread_id.clone(),
                    event_id: format!("$tsts_stat_{uid}_{i}:localhost"),
                    root_event_id: root_event_id.clone(),
                    content: serde_json::json!({ "body": i.to_string() }),
                    in_reply_to_event_id: Some(root_event_id.clone()),
                    origin_server_ts: 3000 + i,
                },
            )
            .await
            .expect("add_reply should succeed");
    }

    let stats = service
        .get_thread_statistics(&room_id, &thread_id)
        .await
        .expect("get_thread_statistics should succeed")
        .expect("statistics should exist");
    assert_eq!(stats.total_replies, 3);
    assert_eq!(stats.total_participants, 2, "creator + replier");
    assert_eq!(stats.total_edits, 0);
    assert_eq!(stats.total_redactions, 0);
    assert_eq!(stats.first_reply_ts, Some(3000));
    assert_eq!(stats.last_reply_ts, Some(3002));
}

#[tokio::test]
async fn test_get_thread_statistics_none_for_missing() {
    let (_guard, service, _pool) = prepare().await;
    let result = service
        .get_thread_statistics("!tsts_missing:localhost", "$missing")
        .await
        .expect("get_thread_statistics should not error on missing thread");
    assert!(result.is_none());
}

// =============================================================================
// search_threads
// =============================================================================

#[tokio::test]
async fn test_search_threads_finds_by_root_content() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (creator, _replier, _reader, room_id) = seed_room(&pool, uid).await;
    let root_event_id = format!("$tsts_search_{uid}:localhost");

    // Insert the backing event so the search ILIKE on content->>'body' matches.
    insert_event(&pool, &room_id, &creator, &root_event_id, "hello searchable world", 1).await;

    let root = service
        .create_thread(
            &creator,
            CreateThreadRequest { room_id: room_id.clone(), root_event_id: root_event_id.clone() },
        )
        .await
        .expect("create_thread should succeed");
    let thread_id = root.thread_id.clone().unwrap();

    let results = service
        .search_threads(&room_id, "searchable", Some(10))
        .await
        .expect("search_threads should succeed");
    assert_eq!(results.len(), 1, "search should find the thread by root content");
    assert_eq!(results[0].thread_id, thread_id);
}

#[tokio::test]
async fn test_search_threads_empty_when_no_match() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (creator, _replier, _reader, room_id) = seed_room(&pool, uid).await;
    let root_event_id = format!("$tsts_nomatch_{uid}:localhost");
    insert_event(&pool, &room_id, &creator, &root_event_id, "alpha beta", 1).await;
    service
        .create_thread(
            &creator,
            CreateThreadRequest { room_id: room_id.clone(), root_event_id },
        )
        .await
        .expect("create_thread should succeed");

    let results = service
        .search_threads(&room_id, "nonexistent_term_zzz", Some(10))
        .await
        .expect("search_threads should succeed");
    assert!(results.is_empty(), "no thread should match the query");
}

// =============================================================================
// freeze_thread / unfreeze_thread
// =============================================================================

#[tokio::test]
async fn test_freeze_thread_marks_fetched() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, _root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;

    service.freeze_thread(&room_id, &thread_id).await.expect("freeze_thread should succeed");

    let root = service
        .get_thread_root(&room_id, &thread_id)
        .await
        .expect("get_thread_root should succeed")
        .expect("thread root should exist");
    assert!(root.is_fetched, "is_fetched should be true after freeze");

    let is_fetched: bool =
        sqlx::query_scalar("SELECT is_fetched FROM thread_roots WHERE room_id = $1 AND thread_id = $2")
            .bind(&room_id)
            .bind(&thread_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("query is_fetched");
    assert!(is_fetched);
}

#[tokio::test]
async fn test_unfreeze_thread_clears_fetched() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, _root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;

    service.freeze_thread(&room_id, &thread_id).await.expect("freeze should succeed");
    service.unfreeze_thread(&room_id, &thread_id).await.expect("unfreeze should succeed");

    let root = service
        .get_thread_root(&room_id, &thread_id)
        .await
        .expect("get_thread_root should succeed")
        .expect("thread root should exist");
    assert!(!root.is_fetched, "is_fetched should be false after unfreeze");

    // And add_reply should now succeed again (covered separately, but verify the flag is clear).
    let is_fetched: bool =
        sqlx::query_scalar("SELECT is_fetched FROM thread_roots WHERE room_id = $1 AND thread_id = $2")
            .bind(&room_id)
            .bind(&thread_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("query is_fetched");
    assert!(!is_fetched);
}

#[tokio::test]
async fn test_unfreeze_allows_add_reply_again() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let replier = format!("tsts_replier_{uid}:localhost");

    service.freeze_thread(&room_id, &thread_id).await.expect("freeze should succeed");
    service.unfreeze_thread(&room_id, &thread_id).await.expect("unfreeze should succeed");

    let reply = service
        .add_reply(
            &replier,
            CreateReplyRequest {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: format!("$tsts_unfreeze_reply_{uid}:localhost"),
                root_event_id: root_event_id.clone(),
                content: serde_json::json!({ "body": "after unfreeze" }),
                in_reply_to_event_id: Some(root_event_id),
                origin_server_ts: 1,
            },
        )
        .await
        .expect("add_reply should succeed after unfreeze");
    assert_eq!(reply.sender, replier);
}

// =============================================================================
// redact_reply
// =============================================================================

#[tokio::test]
async fn test_redact_reply_marks_redacted_and_clears_content() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let replier = format!("tsts_replier_{uid}:localhost");
    let reply_event_id = format!("$tsts_redact_{uid}:localhost");

    service
        .add_reply(
            &replier,
            CreateReplyRequest {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: reply_event_id.clone(),
                root_event_id: root_event_id.clone(),
                content: serde_json::json!({ "body": "to be redacted" }),
                in_reply_to_event_id: Some(root_event_id),
                origin_server_ts: 1,
            },
        )
        .await
        .expect("add_reply should succeed");

    service
        .redact_reply(&room_id, &reply_event_id)
        .await
        .expect("redact_reply should succeed");

    let replies = service
        .get_thread_replies(&room_id, &thread_id, None, None)
        .await
        .expect("get_thread_replies should succeed");
    let reply = replies.iter().find(|r| r.event_id == reply_event_id).expect("reply should exist");
    assert!(reply.is_redacted, "reply should be marked redacted");
    assert_eq!(reply.content, serde_json::json!({}), "redacted content should be cleared");

    // Persisted in DB.
    let is_redacted: bool =
        sqlx::query_scalar("SELECT is_redacted FROM thread_replies WHERE room_id = $1 AND event_id = $2")
            .bind(&room_id)
            .bind(&reply_event_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("query is_redacted");
    assert!(is_redacted);
}

// =============================================================================
// edit_reply
// =============================================================================

#[tokio::test]
async fn test_edit_reply_marks_edited() {
    let (_guard, service, pool) = prepare().await;
    let uid = unique_id();
    let (thread_id, root_event_id, room_id, _creator) = make_thread(&service, &pool, uid).await;
    let replier = format!("tsts_replier_{uid}:localhost");
    let reply_event_id = format!("$tsts_edit_{uid}:localhost");

    service
        .add_reply(
            &replier,
            CreateReplyRequest {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: reply_event_id.clone(),
                root_event_id: root_event_id.clone(),
                content: serde_json::json!({ "body": "original" }),
                in_reply_to_event_id: Some(root_event_id),
                origin_server_ts: 1,
            },
        )
        .await
        .expect("add_reply should succeed");

    service.edit_reply(&room_id, &reply_event_id).await.expect("edit_reply should succeed");

    let replies = service
        .get_thread_replies(&room_id, &thread_id, None, None)
        .await
        .expect("get_thread_replies should succeed");
    let reply = replies.iter().find(|r| r.event_id == reply_event_id).expect("reply should exist");
    assert!(reply.is_edited, "reply should be marked edited");
    assert!(!reply.is_redacted, "edit should not redact");

    let is_edited: bool =
        sqlx::query_scalar("SELECT is_edited FROM thread_replies WHERE room_id = $1 AND event_id = $2")
            .bind(&room_id)
            .bind(&reply_event_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("query is_edited");
    assert!(is_edited);
}

// =============================================================================
// ApiError propagation (delegation error mapping)
// =============================================================================

#[tokio::test]
async fn test_storage_errors_are_mapped_to_api_error() {
    let (_guard, service, _pool) = prepare().await;
    // Force an internal-error path is hard without breaking the DB, but we can
    // at least confirm that a NotFound path returns an ApiError (not a sqlx::Error).
    let result: Result<_, ApiError> = service
        .get_thread(
            GetThreadRequest {
                room_id: "!tsts_err:localhost".to_string(),
                thread_id: "$err".to_string(),
                include_replies: false,
                reply_limit: None,
            },
            None,
        )
        .await;
    let err = result.expect_err("should error on missing thread");
    assert!(err.is_not_found());
}
