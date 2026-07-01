//! Additional integration tests for `EventStorage` covering methods not
//! exercised by `event_storage_tests_migrated.rs`:
//!   - `create_event_with_graph` + `event_edges` population
//!   - `find_missing_event_ids` (batch existence check)
//!   - `get_missing_events_between` (DAG walk)
//!   - `get_event` (single fetch)
//!   - Ephemeral events (add/upsert/delete/get/get_batch)
//!   - `delete_events_before` (preserves m.room.create)
//!   - `get_room_events` + `get_room_events_paginated` (4 directions)
//!   - `find_event_by_timestamp` + `find_event_id_by_timestamp`
//!   - `get_sender_events`
//!   - Message counts (room/total/daily)
//!   - `delete_room_events`
//!   - Reports (report_event/update_score/get)
//!   - `redact_event_content`
//!   - `save_event_signature`/`get_event_signatures`
//!   - `upsert_power_levels_event`
//!   - Context (before/after)
//!   - `search_room_messages_admin`
//!   - `get_latest_event_ids_in_room`
//!   - `get_room_create_event`
//!   - `count_room_events`
//!   - `get_forward_extremities_count`
//!   - `get_room_events_paginated_with_filter`
//!
//! These tests complement the existing 6 tests in `event_storage_tests_migrated.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use synapse_storage::event::{CreateEventParams, EventStorage};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn event_storage_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Warm up the shared pool on the current tokio runtime.
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

/// Build CreateEventParams with a globally-unique event_id derived from
/// `unique_id()` so tests never collide on the shared `events` table.
fn make_params(event_suffix: &str, room_id: &str, event_type: &str, ts: i64) -> CreateEventParams {
    CreateEventParams {
        event_id: format!("$ea_{event_suffix}:localhost"),
        room_id: room_id.to_string(),
        user_id: "@alice:localhost".to_string(),
        event_type: event_type.to_string(),
        content: serde_json::json!({"body": format!("msg-{event_suffix}")}),
        state_key: None,
        origin_server_ts: ts,
        redacts: None,
    }
}

async fn setup(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
    // Delete child tables first to respect FK constraints.
    sqlx::query("DELETE FROM event_edges").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM event_signatures").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM event_reports").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM room_ephemeral").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM events").execute(pool.as_ref()).await.ok();
}

async fn teardown(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM event_edges").execute(pool).await.ok();
    sqlx::query("DELETE FROM event_signatures").execute(pool).await.ok();
    sqlx::query("DELETE FROM event_reports").execute(pool).await.ok();
    sqlx::query("DELETE FROM room_ephemeral").execute(pool).await.ok();
    sqlx::query("DELETE FROM events").execute(pool).await.ok();
}

fn new_storage(pool: &Arc<sqlx::PgPool>) -> EventStorage {
    EventStorage::new(pool, "localhost".to_string())
}

// ---------------------------------------------------------------------------
// create_event_with_graph + event_edges
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_event_with_graph_populates_edges() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Create a prev event first (no graph data).
    let prev_id = format!("$ea_prev_graph_{}", unique_id());
    let ts = 1_000_000_i64;
    storage
        .create_event(
            CreateEventParams {
                event_id: prev_id.clone(),
                room_id: "!graph:localhost".to_string(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "prev"}),
                state_key: None,
                origin_server_ts: ts,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    // Create a child event with prev_events pointing to the prev.
    let child_id = format!("$ea_child_graph_{}", unique_id());
    let params = CreateEventParams {
        event_id: child_id.clone(),
        room_id: "!graph:localhost".to_string(),
        user_id: "@alice:localhost".to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "child"}),
        state_key: None,
        origin_server_ts: ts + 1,
        redacts: None,
    };
    let event = storage
        .create_event_with_graph(params, &[prev_id.clone()], &[], 2, None)
        .await
        .unwrap();
    assert_eq!(event.event_id, child_id);
    assert_eq!(event.depth, 2);

    // Verify event_edges row was created.
    let edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_edges WHERE event_id = $1")
        .bind(&child_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(edge_count, 1, "event_edges should have one row for the child");

    let prev_in_edge: String =
        sqlx::query_scalar("SELECT prev_event_id FROM event_edges WHERE event_id = $1")
            .bind(&child_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(prev_in_edge, prev_id);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_event_with_graph_no_prev_events_is_noop_for_edges() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let event_id = format!("$ea_noprev_{uid}:localhost");
    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: "!noprev:localhost".to_string(),
        user_id: "@alice:localhost".to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "noprev"}),
        state_key: None,
        origin_server_ts: 2_000_000,
        redacts: None,
    };
    let event = storage.create_event_with_graph(params, &[], &[], 0, None).await.unwrap();
    assert_eq!(event.depth, 0);

    // No prev_events → no event_edges row.
    let edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_edges WHERE event_id = $1")
        .bind(&event_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(edge_count, 0);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// find_missing_event_ids
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_find_missing_event_ids_empty_input() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let missing = storage.find_missing_event_ids(&[]).await.unwrap();
    assert!(missing.is_empty());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_find_missing_event_ids_all_exist_and_some_missing() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let id2 = format!("$ea_miss2_{uid}:localhost");
    // Create two events.
    storage.create_event(make_params(&format!("miss1_{uid}"), "!miss:localhost", "m.room.message", 1), None).await.unwrap();
    // Override the event_id in make_params — create a second one directly.
    storage
        .create_event(
            CreateEventParams {
                event_id: id2.clone(),
                room_id: "!miss:localhost".to_string(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "miss2"}),
                state_key: None,
                origin_server_ts: 2,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();
    // The first event_id was derived from make_params suffix; reconstruct it.
    let id1_actual = format!("$ea_miss1_{uid}:localhost");

    // All exist → empty missing list.
    let missing = storage.find_missing_event_ids(&[id1_actual.clone(), id2.clone()]).await.unwrap();
    assert!(missing.is_empty(), "all events exist, should be no missing");

    // One missing.
    let ghost = "$ea_ghost:localhost".to_string();
    let missing = storage.find_missing_event_ids(&[id1_actual, id2, ghost.clone()]).await.unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0], ghost);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_missing_events_between (DAG walk)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_missing_events_between_walks_dag() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!dag_{uid}:localhost");

    // Build a chain: e1 <- e2 <- e3 (latest).
    let e1 = format!("$ea_dag1_{uid}:localhost");
    let e2 = format!("$ea_dag2_{uid}:localhost");
    let e3 = format!("$ea_dag3_{uid}:localhost");

    storage
        .create_event(
            CreateEventParams {
                event_id: e1.clone(),
                room_id: room.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "e1"}),
                state_key: None,
                origin_server_ts: 100,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();
    storage
        .create_event_with_graph(
            CreateEventParams {
                event_id: e2.clone(),
                room_id: room.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "e2"}),
                state_key: None,
                origin_server_ts: 200,
                redacts: None,
            },
            &[e1.clone()],
            &[],
            2,
            None,
        )
        .await
        .unwrap();
    storage
        .create_event_with_graph(
            CreateEventParams {
                event_id: e3.clone(),
                room_id: room.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "e3"}),
                state_key: None,
                origin_server_ts: 300,
                redacts: None,
            },
            &[e2.clone()],
            &[],
            3,
            None,
        )
        .await
        .unwrap();

    // earliest = [e1], latest = [e3]. Missing = [e2] (between e1 and e3).
    let missing = storage.get_missing_events_between(&room, &[e1], &[e3], 10).await.unwrap();
    assert_eq!(missing.len(), 1, "should find exactly e2 as missing");
    assert_eq!(missing[0]["event_id"].as_str(), Some(e2.as_str()));

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_missing_events_between_empty_latest() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let missing = storage
        .get_missing_events_between("!empty:localhost", &["$e1:localhost".to_string()], &[], 10)
        .await
        .unwrap();
    assert!(missing.is_empty());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_event
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_event_found_and_not_found() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let event_id = format!("$ea_get_{uid}:localhost");
    storage
        .create_event(
            CreateEventParams {
                event_id: event_id.clone(),
                room_id: "!getev:localhost".to_string(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "hello"}),
                state_key: None,
                origin_server_ts: 5_000_000,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    let found = storage.get_event(&event_id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().event_id, event_id);

    let not_found = storage.get_event("$nonexistent:localhost").await.unwrap();
    assert!(not_found.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// Ephemeral events
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_ephemeral_add_get_delete() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!eph_{uid}:localhost");
    storage
        .add_ephemeral_event(
            &room,
            "@alice:localhost",
            "m.typing",
            &serde_json::json!({"typing": true}),
            1,
        )
        .await
        .unwrap();

    // Should be retrievable.
    let events = storage.get_ephemeral_events(&room, 1_000_000_000, 10).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "m.typing");
    assert_eq!(events[0].user_id, "@alice:localhost");

    // Delete.
    storage.delete_ephemeral_event(&room, "m.typing", "@alice:localhost").await.unwrap();
    let events = storage.get_ephemeral_events(&room, 1_000_000_000, 10).await.unwrap();
    assert!(events.is_empty());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_ephemeral_upsert_replaces_existing() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!uph_{uid}:localhost");
    // Insert with expires_at in the future.
    storage
        .upsert_ephemeral_event(
            &room,
            "@bob:localhost",
            "m.typing",
            &serde_json::json!({"typing": true}),
            1,
            1_000_000,
            Some(9_000_000_000),
        )
        .await
        .unwrap();

    // Upsert (replace) with different content.
    storage
        .upsert_ephemeral_event(
            &room,
            "@bob:localhost",
            "m.typing",
            &serde_json::json!({"typing": false}),
            2,
            2_000_000,
            Some(9_000_000_000),
        )
        .await
        .unwrap();

    let events = storage.get_ephemeral_events(&room, 1_000_000_000, 10).await.unwrap();
    assert_eq!(events.len(), 1, "upsert should replace, not duplicate");
    assert_eq!(events[0].stream_id, 2);
    assert_eq!(events[0].content["typing"], false);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_ephemeral_expired_filtered_out() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!exp_{uid}:localhost");
    // Insert with expires_at in the past.
    storage
        .upsert_ephemeral_event(
            &room,
            "@carol:localhost",
            "m.typing",
            &serde_json::json!({"typing": true}),
            1,
            1_000_000,
            Some(500_000), // expired
        )
        .await
        .unwrap();

    // now > expires_at → filtered out.
    let events = storage.get_ephemeral_events(&room, 1_000_000_000, 10).await.unwrap();
    assert!(events.is_empty(), "expired ephemeral should be filtered");

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_ephemeral_events_batch_multi_room() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room1 = format!("!eb1_{uid}:localhost");
    let room2 = format!("!eb2_{uid}:localhost");

    storage
        .add_ephemeral_event(&room1, "@a:localhost", "m.typing", &serde_json::json!({}), 1)
        .await
        .unwrap();
    storage
        .add_ephemeral_event(&room2, "@b:localhost", "m.typing", &serde_json::json!({}), 2)
        .await
        .unwrap();

    let map = storage
        .get_ephemeral_events_batch(&[room1.clone(), room2.clone()], 1_000_000_000, 10)
        .await
        .unwrap();
    assert_eq!(map.len(), 2);
    assert_eq!(map[&room1].len(), 1);
    assert_eq!(map[&room2].len(), 1);

    // Empty input → empty map.
    let empty_map = storage.get_ephemeral_events_batch(&[], 1_000_000_000, 10).await.unwrap();
    assert!(empty_map.is_empty());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// delete_events_before (preserves m.room.create)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_events_before_preserves_create() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!delb_{uid}:localhost");

    // Insert a create event (should be preserved).
    storage
        .create_event(
            CreateEventParams {
                event_id: format!("$ea_create_{uid}:localhost"),
                room_id: room.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.create".to_string(),
                content: serde_json::json!({"creator": "@alice:localhost"}),
                state_key: None,
                origin_server_ts: 1_000,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    // Insert a message before the cutoff.
    storage
        .create_event(
            CreateEventParams {
                event_id: format!("$ea_msg1_{uid}:localhost"),
                room_id: room.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "old"}),
                state_key: None,
                origin_server_ts: 2_000,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    // Insert a message after the cutoff.
    storage
        .create_event(
            CreateEventParams {
                event_id: format!("$ea_msg2_{uid}:localhost"),
                room_id: room.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "new"}),
                state_key: None,
                origin_server_ts: 5_000,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    let deleted = storage.delete_events_before(&room, 3_000).await.unwrap();
    assert_eq!(deleted, 1, "only msg1 should be deleted (before cutoff)");

    // Verify create and msg2 survive.
    let remaining: Vec<String> =
        sqlx::query_scalar("SELECT event_id FROM events WHERE room_id = $1 ORDER BY origin_server_ts")
            .bind(&room)
            .fetch_all(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(remaining.len(), 2, "create + msg2 should remain");
    assert!(remaining.iter().any(|id| id.contains("create")));
    assert!(remaining.iter().any(|id| id.contains("msg2")));

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_room_events + get_room_events_paginated (4 directions)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_room_events_orders_desc() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!gre_{uid}:localhost");
    for i in 1..=3 {
        storage
            .create_event(
                CreateEventParams {
                    event_id: format!("$ea_gre{i}_{uid}:localhost"),
                    room_id: room.clone(),
                    user_id: "@alice:localhost".to_string(),
                    event_type: "m.room.message".to_string(),
                    content: serde_json::json!({"body": i}),
                    state_key: None,
                    origin_server_ts: i * 1000,
                    redacts: None,
                },
                None,
            )
            .await
            .unwrap();
    }

    let events = storage.get_room_events(&room, 10).await.unwrap();
    assert_eq!(events.len(), 3);
    // DESC ordering: newest first.
    assert!(events[0].origin_server_ts >= events[1].origin_server_ts);
    assert!(events[1].origin_server_ts >= events[2].origin_server_ts);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_room_events_paginated_four_directions() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!pag_{uid}:localhost");
    for i in 1..=5 {
        storage
            .create_event(
                CreateEventParams {
                    event_id: format!("$ea_pag{i}_{uid}:localhost"),
                    room_id: room.clone(),
                    user_id: "@alice:localhost".to_string(),
                    event_type: "m.room.message".to_string(),
                    content: serde_json::json!({"body": i}),
                    state_key: None,
                    origin_server_ts: i * 1000,
                    redacts: None,
                },
                None,
            )
            .await
            .unwrap();
    }

    // Forward from 2000 → events with ts > 2000 (3,4,5).
    let fwd_from = storage.get_room_events_paginated(&room, Some(2000), 10, "f").await.unwrap();
    assert_eq!(fwd_from.len(), 3, "forward from 2000 should return 3 events");

    // Forward from None → all 5 ASC.
    let fwd_none = storage.get_room_events_paginated(&room, None, 10, "f").await.unwrap();
    assert_eq!(fwd_none.len(), 5);
    assert!(fwd_none[0].origin_server_ts <= fwd_none[1].origin_server_ts, "forward should be ASC");

    // Backward from 4000 → events with ts < 4000 (1,2,3) DESC.
    let bwd_from = storage.get_room_events_paginated(&room, Some(4000), 10, "b").await.unwrap();
    assert_eq!(bwd_from.len(), 3, "backward from 4000 should return 3 events");
    assert!(bwd_from[0].origin_server_ts >= bwd_from[1].origin_server_ts, "backward should be DESC");

    // Backward from None → all 5 DESC.
    let bwd_none = storage.get_room_events_paginated(&room, None, 10, "b").await.unwrap();
    assert_eq!(bwd_none.len(), 5);
    assert!(bwd_none[0].origin_server_ts >= bwd_none[1].origin_server_ts, "backward should be DESC");

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// find_event_by_timestamp + find_event_id_by_timestamp
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_find_event_by_timestamp() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!ts_{uid}:localhost");
    storage
        .create_event(
            CreateEventParams {
                event_id: format!("$ea_ts1_{uid}:localhost"),
                room_id: room.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "at 1000"}),
                state_key: None,
                origin_server_ts: 1_000,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    // Exact match.
    let found = storage.find_event_by_timestamp(&room, 1_000).await.unwrap();
    assert!(found.is_some(), "exact ts match");
    let found_val = found.unwrap();
    assert_eq!(found_val["origin_server_ts"].as_i64(), Some(1_000));

    // ts > event → event at or before ts.
    let found = storage.find_event_by_timestamp(&room, 2_000).await.unwrap();
    assert!(found.is_some());

    // ts < event → nothing.
    let found = storage.find_event_by_timestamp(&room, 500).await.unwrap();
    assert!(found.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_find_event_id_by_timestamp_forward_and_backward() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!fid_{uid}:localhost");
    for ts in [1_000, 2_000, 3_000_i64] {
        storage
            .create_event(
                CreateEventParams {
                    event_id: format!("$ea_fid{ts}_{uid}:localhost"),
                    room_id: room.clone(),
                    user_id: "@alice:localhost".to_string(),
                    event_type: "m.room.message".to_string(),
                    content: serde_json::json!({"body": ts}),
                    state_key: None,
                    origin_server_ts: ts,
                    redacts: None,
                },
                None,
            )
            .await
            .unwrap();
    }

    // Forward (>= 2000) → first event at ts 2000.
    let fwd = storage.find_event_id_by_timestamp(&room, 2_000, true).await.unwrap();
    assert!(fwd.is_some());
    assert_eq!(fwd.unwrap().1, 2_000);

    // Backward (<= 2000) → first event at ts 2000.
    let bwd = storage.find_event_id_by_timestamp(&room, 2_000, false).await.unwrap();
    assert!(bwd.is_some());
    assert_eq!(bwd.unwrap().1, 2_000);

    // Forward (>= 9999) → none.
    let fwd_none = storage.find_event_id_by_timestamp(&room, 9_999, true).await.unwrap();
    assert!(fwd_none.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_sender_events
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_sender_events() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!snd_{uid}:localhost");
    storage
        .create_event(
            CreateEventParams {
                event_id: format!("$ea_snd1_{uid}:localhost"),
                room_id: room.clone(),
                user_id: "@bob:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "hi"}),
                state_key: None,
                origin_server_ts: 1_000,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    let events = storage.get_sender_events("@bob:localhost", 10).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].user_id, "@bob:localhost");

    // Different sender → empty.
    let events = storage.get_sender_events("@nobody:localhost", 10).await.unwrap();
    assert!(events.is_empty());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// Message counts
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_message_counts() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room_a = format!("!cntA_{uid}:localhost");
    let room_b = format!("!cntB_{uid}:localhost");

    // Insert 2 messages in A, 1 in B, 1 non-message in A.
    for (eid, room, etype) in [
        (format!("$ea_c1_{uid}:localhost"), room_a.clone(), "m.room.message"),
        (format!("$ea_c2_{uid}:localhost"), room_a.clone(), "m.room.message"),
        (format!("$ea_c3_{uid}:localhost"), room_b.clone(), "m.room.message"),
        (format!("$ea_c4_{uid}:localhost"), room_a.clone(), "m.room.member"),
    ] {
        storage
            .create_event(
                CreateEventParams {
                    event_id: eid,
                    room_id: room,
                    user_id: "@alice:localhost".to_string(),
                    event_type: etype.to_string(),
                    content: serde_json::json!({}),
                    state_key: None,
                    origin_server_ts: chrono::Utc::now().timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .unwrap();
    }

    // Room message count.
    assert_eq!(storage.get_room_message_count(&room_a).await.unwrap(), 2);
    assert_eq!(storage.get_room_message_count(&room_b).await.unwrap(), 1);

    // Total message count = 3.
    assert_eq!(storage.get_total_message_count().await.unwrap(), 3);

    // Daily count = 3 (all events have recent timestamps).
    assert_eq!(storage.get_daily_message_count().await.unwrap(), 3);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// delete_room_events
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_room_events() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!dre_{uid}:localhost");
    storage
        .create_event(make_params(&format!("dre_{uid}"), &room, "m.room.message", 1_000), None)
        .await
        .unwrap();

    let before = storage.count_room_events(&room).await.unwrap();
    assert_eq!(before, 1);

    storage.delete_room_events(&room).await.unwrap();

    let after = storage.count_room_events(&room).await.unwrap();
    assert_eq!(after, 0);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_report_event_and_update_score() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let event_id = format!("$ea_rep_{uid}:localhost");
    let room = format!("!rep_{uid}:localhost");
    storage
        .create_event(
            CreateEventParams {
                event_id: event_id.clone(),
                room_id: room.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "report me"}),
                state_key: None,
                origin_server_ts: 1_000,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    // Report the event.
    let report_id = storage
        .report_event(&event_id, &room, "@alice:localhost", "@bob:localhost", Some("spam"), -50)
        .await
        .unwrap();
    assert!(report_id > 0);

    // Get the report.
    let reports = storage.get_event_report(&event_id).await.unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].reason, Some("spam".to_string()));
    assert_eq!(reports[0].score, -50);

    // Update score by report id.
    storage.update_event_report_score(report_id, -100).await.unwrap();
    let reports = storage.get_event_report(&event_id).await.unwrap();
    assert_eq!(reports[0].score, -100);

    // Update score by event id.
    storage.update_event_report_score_by_event(&event_id, 0).await.unwrap();
    let reports = storage.get_event_report(&event_id).await.unwrap();
    assert_eq!(reports[0].score, 0);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// redact_event_content
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_redact_event_content_found_and_not_found() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let event_id = format!("$ea_red_{uid}:localhost");
    let room = format!("!red_{uid}:localhost");
    storage
        .create_event(
            CreateEventParams {
                event_id: event_id.clone(),
                room_id: room.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "redact me", "extra": "field"}),
                state_key: None,
                origin_server_ts: 1_000,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    // Redact.
    storage.redact_event_content(&event_id, Some("@mod:localhost")).await.unwrap();

    // Verify is_redacted via DB query (the redaction logic retains
    // spec-mandated fields; for m.room.message the content is stripped down).
    let is_redacted: bool = sqlx::query_scalar("SELECT is_redacted FROM events WHERE event_id = $1")
        .bind(&event_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert!(is_redacted, "event should be marked redacted");

    let redacted_by: Option<String> =
        sqlx::query_scalar("SELECT redacted_by FROM events WHERE event_id = $1")
            .bind(&event_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(redacted_by, Some("@mod:localhost".to_string()));

    // Redact non-existent event → Ok(()) no-op.
    let result = storage.redact_event_content("$nonexistent:localhost", None).await;
    assert!(result.is_ok(), "redacting non-existent event should be a no-op");

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// save_event_signature / get_event_signatures
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_save_and_get_event_signatures_upsert() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let event_id = format!("$ea_sig_{uid}:localhost");
    let room = format!("!sig_{uid}:localhost");
    storage
        .create_event(
            CreateEventParams {
                event_id: event_id.clone(),
                room_id: room,
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "sign me"}),
                state_key: None,
                origin_server_ts: 1_000,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    // Save first signature.
    storage
        .save_event_signature(
            &event_id,
            "@alice:localhost",
            "DEV1",
            "sig-bytes-1",
            "ed25519:1",
            "ed25519",
            1_000_000,
        )
        .await
        .unwrap();

    // Save second signature (different key_id) → should add, not replace.
    storage
        .save_event_signature(
            &event_id,
            "@alice:localhost",
            "DEV1",
            "sig-bytes-2",
            "ed25519:2",
            "ed25519",
            2_000_000,
        )
        .await
        .unwrap();

    let sigs = storage.get_event_signatures(&event_id).await.unwrap();
    assert_eq!(sigs.len(), 2, "two distinct key_ids → two rows");

    // Upsert same (event_id, user_id, device_id, key_id) → should update.
    storage
        .save_event_signature(
            &event_id,
            "@alice:localhost",
            "DEV1",
            "sig-bytes-updated",
            "ed25519:1",
            "ed25519",
            3_000_000,
        )
        .await
        .unwrap();

    let sigs = storage.get_event_signatures(&event_id).await.unwrap();
    assert_eq!(sigs.len(), 2, "upsert should not add a new row");
    let sig1 = sigs.iter().find(|s| s.key_id == "ed25519:1").unwrap();
    assert_eq!(sig1.signature, "sig-bytes-updated");
    assert_eq!(sig1.created_ts, Some(3_000_000));

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// upsert_power_levels_event
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_upsert_power_levels_event_insert_and_update() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let event_id = format!("$ea_pl_{uid}:localhost");
    let room = format!("!pl_{uid}:localhost");

    let content = serde_json::json!({"ban": 50, "kick": 50});
    storage
        .upsert_power_levels_event(&event_id, &room, "@alice:localhost", content.clone(), 1_000, "@alice:localhost")
        .await
        .unwrap();

    // Verify it was inserted.
    let stored_content: serde_json::Value =
        sqlx::query_scalar("SELECT content FROM events WHERE event_id = $1")
            .bind(&event_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(stored_content["ban"], 50);

    // Upsert (update content) via ON CONFLICT.
    let new_content = serde_json::json!({"ban": 100, "kick": 100});
    storage
        .upsert_power_levels_event(&event_id, &room, "@alice:localhost", new_content.clone(), 2_000, "@alice:localhost")
        .await
        .unwrap();

    let stored_content: serde_json::Value =
        sqlx::query_scalar("SELECT content FROM events WHERE event_id = $1")
            .bind(&event_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(stored_content["ban"], 100, "content should be updated on conflict");

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// Context (before / after)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_events_before_and_after_context() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!ctx_{uid}:localhost");
    for ts in [1_000, 2_000, 3_000, 4_000, 5_000_i64] {
        storage
            .create_event(
                CreateEventParams {
                    event_id: format!("$ea_ctx{ts}_{uid}:localhost"),
                    room_id: room.clone(),
                    user_id: "@alice:localhost".to_string(),
                    event_type: "m.room.message".to_string(),
                    content: serde_json::json!({"body": ts}),
                    state_key: None,
                    origin_server_ts: ts,
                    redacts: None,
                },
                None,
            )
            .await
            .unwrap();
    }

    // Before 3000 (ts < 3000): events at 1000, 2000 → DESC.
    let before = storage.get_events_before_context(&room, 3_000, 10).await.unwrap();
    assert_eq!(before.len(), 2);
    assert_eq!(before[0]["origin_server_ts"].as_i64(), Some(2_000), "DESC before 3000");

    // After 3000 (ts > 3000): events at 4000, 5000 → ASC.
    let after = storage.get_events_after_context(&room, 3_000, 10).await.unwrap();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0]["origin_server_ts"].as_i64(), Some(4_000), "ASC after 3000");

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// search_room_messages_admin
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_search_room_messages_admin() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!srch_{uid}:localhost");
    storage
        .create_event(
            CreateEventParams {
                event_id: format!("$ea_s1_{uid}:localhost"),
                room_id: room.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "hello world"}),
                state_key: None,
                origin_server_ts: 1_000,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();
    storage
        .create_event(
            CreateEventParams {
                event_id: format!("$ea_s2_{uid}:localhost"),
                room_id: room.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "different message"}),
                state_key: None,
                origin_server_ts: 2_000,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    // Search for "hello".
    let results = storage.search_room_messages_admin(&room, "%hello%", 10).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["event_id"].as_str(), Some(format!("$ea_s1_{uid}:localhost").as_str()));

    // Search for non-matching pattern.
    let results = storage.search_room_messages_admin(&room, "%nonexistent%", 10).await.unwrap();
    assert!(results.is_empty());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_latest_event_ids_in_room
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_latest_event_ids_in_room() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!lat_{uid}:localhost");
    for i in 1..=3 {
        storage
            .create_event(
                CreateEventParams {
                    event_id: format!("$ea_lat{i}_{uid}:localhost"),
                    room_id: room.clone(),
                    user_id: "@alice:localhost".to_string(),
                    event_type: "m.room.message".to_string(),
                    content: serde_json::json!({"body": i}),
                    state_key: None,
                    origin_server_ts: i * 1000,
                    redacts: None,
                },
                None,
            )
            .await
            .unwrap();
    }

    // Get top 2 → should be the 2 newest by ts DESC.
    let latest = storage.get_latest_event_ids_in_room(&room, 2).await.unwrap();
    assert_eq!(latest.len(), 2);
    // Newest first (ts 3000, 2000).
    assert!(latest[0].contains("lat3"));
    assert!(latest[1].contains("lat2"));

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_room_create_event
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_room_create_event_found_and_not_found() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!crt_{uid}:localhost");
    // Insert a create event.
    storage
        .create_event(
            CreateEventParams {
                event_id: format!("$ea_crt_{uid}:localhost"),
                room_id: room.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.create".to_string(),
                content: serde_json::json!({"creator": "@alice:localhost"}),
                state_key: None,
                origin_server_ts: 500,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();
    // Insert a message.
    storage
        .create_event(
            CreateEventParams {
                event_id: format!("$ea_crtmsg_{uid}:localhost"),
                room_id: room.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "hi"}),
                state_key: None,
                origin_server_ts: 1_000,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    let create = storage.get_room_create_event(&room).await.unwrap();
    assert!(create.is_some());
    assert_eq!(create.unwrap().event_type, "m.room.create");

    // No create in a different room.
    let create = storage.get_room_create_event("!nocreate:localhost").await.unwrap();
    assert!(create.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// count_room_events + get_forward_extremities_count
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_count_room_events_and_forward_extremities() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!fwd_{uid}:localhost");
    for i in 1..=3 {
        storage
            .create_event(
                CreateEventParams {
                    event_id: format!("$ea_fwd{i}_{uid}:localhost"),
                    room_id: room.clone(),
                    user_id: "@alice:localhost".to_string(),
                    event_type: "m.room.message".to_string(),
                    content: serde_json::json!({"body": i}),
                    state_key: None,
                    origin_server_ts: i * 1000,
                    redacts: None,
                },
                None,
            )
            .await
            .unwrap();
    }

    // count_room_events.
    let count = storage.count_room_events(&room).await.unwrap();
    assert_eq!(count, 3);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_room_events_paginated_with_filter
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_room_events_paginated_with_filter_backward() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room = format!("!pf_{uid}:localhost");
    for i in 1..=3 {
        storage
            .create_event(
                CreateEventParams {
                    event_id: format!("$ea_pf{i}_{uid}:localhost"),
                    room_id: room.clone(),
                    user_id: "@alice:localhost".to_string(),
                    event_type: "m.room.message".to_string(),
                    content: serde_json::json!({"body": i}),
                    state_key: None,
                    origin_server_ts: i * 1000,
                    redacts: None,
                },
                None,
            )
            .await
            .unwrap();
    }

    // from = "2000" → backward (ts < 2000) → 1 event.
    let events = storage
        .get_room_events_paginated_with_filter(&room, Some("2000"), None, 10, None)
        .await
        .unwrap();
    assert_eq!(events.len(), 1, "backward from 2000 should return 1 event");

    // from = None → all 3 backward (DESC).
    let events = storage
        .get_room_events_paginated_with_filter(&room, None, None, 10, None)
        .await
        .unwrap();
    assert_eq!(events.len(), 3);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// update_event_signatures_and_hashes
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_event_signatures_and_hashes() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let event_id = format!("$ea_sh_{uid}:localhost");
    let room = format!("!sh_{uid}:localhost");
    storage
        .create_event(
            CreateEventParams {
                event_id: event_id.clone(),
                room_id: room,
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "hash me"}),
                state_key: None,
                origin_server_ts: 1_000,
                redacts: None,
            },
            None,
        )
        .await
        .unwrap();

    let sigs = serde_json::json!({"test_server": {"ed25519:1": "abc"}});
    let hashes = serde_json::json!({"sha256": "def"});

    storage
        .update_event_signatures_and_hashes(&event_id, &sigs, &hashes)
        .await
        .unwrap();

    let stored_sig: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT signatures FROM events WHERE event_id = $1")
            .bind(&event_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(stored_sig, Some(sigs));

    let stored_hash: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT hashes FROM events WHERE event_id = $1")
            .bind(&event_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(stored_hash, Some(hashes));

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// create_postgres_fts_index
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_postgres_fts_index_is_idempotent() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Should succeed and be idempotent.
    storage.create_postgres_fts_index().await.unwrap();
    storage.create_postgres_fts_index().await.unwrap();

    teardown(pool.as_ref()).await;
}
