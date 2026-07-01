//! Additional integration tests for `RoomStorage` covering methods not
//! exercised by any existing migrated test file:
//!   - `create_room` + `get_room` (round-trip)
//!   - `get_room_creator`
//!   - `room_exists`
//!   - `get_rooms_batch` (including empty input)
//!   - `get_public_rooms` + `count_public_rooms`
//!   - `update_room_name` + `update_room_topic` + `update_room_avatar`
//!   - `set_canonical_alias` / `update_canonical_alias`
//!   - `get_room_count`
//!   - `set_room_visibility`
//!   - `set_room_alias` + `get_room_alias` + `get_room_aliases` + `get_room_by_alias`
//!   - `remove_room_alias` / `remove_room_alias_by_name`
//!   - `delete_room`
//!   - `set_room_version`
//!   - `set_room_directory` + `is_room_in_directory` + `remove_room_directory`
//!   - `shutdown_room`
//!   - `copy_room_state`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use synapse_storage::room::{Room, RoomStorage};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn room_storage_test_guard() -> &'static Mutex<()> {
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

/// Delete child tables first to respect FK constraints. `room_state_events`
/// has no FK to `rooms`, so it is explicitly cleared.
async fn setup(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
    sqlx::query("DELETE FROM room_aliases").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM room_directory").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM room_state_events").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM rooms").execute(pool.as_ref()).await.ok();
}

async fn teardown(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM room_aliases").execute(pool).await.ok();
    sqlx::query("DELETE FROM room_directory").execute(pool).await.ok();
    sqlx::query("DELETE FROM room_state_events").execute(pool).await.ok();
    sqlx::query("DELETE FROM rooms").execute(pool).await.ok();
}

fn new_storage(pool: &Arc<sqlx::PgPool>) -> RoomStorage {
    RoomStorage::new(pool)
}

fn unique_room_id() -> String {
    format!("!rst_{}:localhost", unique_id())
}

// ---------------------------------------------------------------------------
// create_room + get_room (round-trip)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_room_and_get_room_round_trip() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let room_id = unique_room_id();
    let creator = "@alice:localhost";
    let room = storage
        .create_room(&room_id, creator, "invite", "9", false)
        .await
        .unwrap();
    assert_eq!(room.room_id, room_id);
    assert_eq!(room.creator_user_id, Some(creator.to_string()));
    assert_eq!(room.join_rule, "invite");
    assert_eq!(room.room_version, "9");
    assert!(!room.is_public);

    // Round-trip via get_room.
    let fetched = storage.get_room(&room_id).await.unwrap().expect("room should exist");
    assert_eq!(fetched.room_id, room_id);
    assert_eq!(fetched.creator_user_id, Some(creator.to_string()));
    assert_eq!(fetched.join_rule, "invite");
    assert_eq!(fetched.room_version, "9");
    assert!(!fetched.is_public);
    // member_count is 0 because no room_summaries/memberships rows exist.
    assert_eq!(fetched.member_count, 0);

    // Non-existent room → None.
    let missing = storage.get_room("!nonexistent:localhost").await.unwrap();
    assert!(missing.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_room_creator
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_room_creator_found_and_not_found() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let room_id = unique_room_id();
    storage
        .create_room(&room_id, "@bob:localhost", "invite", "9", false)
        .await
        .unwrap();

    let creator = storage.get_room_creator(&room_id).await.unwrap();
    assert_eq!(creator, Some("@bob:localhost".to_string()));

    let missing = storage.get_room_creator("!nope:localhost").await.unwrap();
    assert!(missing.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// room_exists
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_room_exists_true_and_false() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let room_id = unique_room_id();
    storage
        .create_room(&room_id, "@alice:localhost", "invite", "9", false)
        .await
        .unwrap();

    assert!(storage.room_exists(&room_id).await.unwrap());
    assert!(!storage.room_exists("!missing:localhost").await.unwrap());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_rooms_batch (including empty input)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_rooms_batch_empty_and_populated() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Empty input → empty output.
    let empty = storage.get_rooms_batch(&[]).await.unwrap();
    assert!(empty.is_empty());

    let r1 = unique_room_id();
    let r2 = unique_room_id();
    storage
        .create_room(&r1, "@a:localhost", "invite", "9", false)
        .await
        .unwrap();
    storage
        .create_room(&r2, "@b:localhost", "invite", "9", false)
        .await
        .unwrap();

    let batch = storage.get_rooms_batch(&[r1.clone(), r2.clone()]).await.unwrap();
    assert_eq!(batch.len(), 2);
    let ids: Vec<&str> = batch.iter().map(|r: &Room| r.room_id.as_str()).collect();
    assert!(ids.contains(&r1.as_str()));
    assert!(ids.contains(&r2.as_str()));

    // Batch with a non-existent id → only existing rooms returned.
    let batch = storage
        .get_rooms_batch(&[r1.clone(), "!missing:localhost".to_string()])
        .await
        .unwrap();
    assert_eq!(batch.len(), 1);
    assert_eq!(batch[0].room_id, r1);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_public_rooms + count_public_rooms
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_public_rooms_and_count() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // No public rooms initially.
    assert_eq!(storage.count_public_rooms().await.unwrap(), 0);

    let pub1 = unique_room_id();
    let pub2 = unique_room_id();
    let priv1 = unique_room_id();
    storage
        .create_room(&pub1, "@a:localhost", "invite", "9", true)
        .await
        .unwrap();
    storage
        .create_room(&pub2, "@b:localhost", "invite", "9", true)
        .await
        .unwrap();
    storage
        .create_room(&priv1, "@c:localhost", "invite", "9", false)
        .await
        .unwrap();

    assert_eq!(storage.count_public_rooms().await.unwrap(), 2);

    let public = storage.get_public_rooms(10).await.unwrap();
    assert_eq!(public.len(), 2);
    let ids: Vec<&str> = public.iter().map(|r| r.room_id.as_str()).collect();
    assert!(ids.contains(&pub1.as_str()));
    assert!(ids.contains(&pub2.as_str()));
    assert!(!ids.contains(&priv1.as_str()));

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// update_room_name + update_room_topic + update_room_avatar
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_room_name_topic_avatar() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let room_id = unique_room_id();
    storage
        .create_room(&room_id, "@a:localhost", "invite", "9", false)
        .await
        .unwrap();

    storage.update_room_name(&room_id, "Test Room").await.unwrap();
    storage.update_room_topic(&room_id, "A topic").await.unwrap();
    storage.update_room_avatar(&room_id, "mxc://localhost/abc").await.unwrap();

    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert_eq!(room.name, Some("Test Room".to_string()));
    assert_eq!(room.topic, Some("A topic".to_string()));
    assert_eq!(room.avatar_url, Some("mxc://localhost/abc".to_string()));

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// set_canonical_alias / update_canonical_alias
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_and_update_canonical_alias() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let room_id = unique_room_id();
    storage
        .create_room(&room_id, "@a:localhost", "invite", "9", false)
        .await
        .unwrap();

    // Set alias.
    storage
        .set_canonical_alias(&room_id, Some("#alias1:localhost"))
        .await
        .unwrap();
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert_eq!(room.canonical_alias, Some("#alias1:localhost".to_string()));

    // update_canonical_alias delegates to set_canonical_alias(Some(...)).
    storage.update_canonical_alias(&room_id, "#alias2:localhost").await.unwrap();
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert_eq!(room.canonical_alias, Some("#alias2:localhost".to_string()));

    // Clear alias.
    storage.set_canonical_alias(&room_id, None).await.unwrap();
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert_eq!(room.canonical_alias, None);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_room_count
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_room_count() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let before = storage.get_room_count().await.unwrap();
    assert_eq!(before, 0, "setup should have cleared rooms");

    storage
        .create_room(&unique_room_id(), "@a:localhost", "invite", "9", false)
        .await
        .unwrap();
    storage
        .create_room(&unique_room_id(), "@b:localhost", "invite", "9", false)
        .await
        .unwrap();

    let after = storage.get_room_count().await.unwrap();
    assert_eq!(after, 2);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// set_room_visibility
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_room_visibility() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let room_id = unique_room_id();
    storage
        .create_room(&room_id, "@a:localhost", "invite", "9", false)
        .await
        .unwrap();

    // Default visibility is 'private' per schema.
    let initial: String = sqlx::query_scalar("SELECT visibility FROM rooms WHERE room_id = $1")
        .bind(&room_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(initial, "private");

    // Set to public.
    storage.set_room_visibility(&room_id, "public").await.unwrap();
    let vis: String = sqlx::query_scalar("SELECT visibility FROM rooms WHERE room_id = $1")
        .bind(&room_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(vis, "public");

    // Set to private.
    storage.set_room_visibility(&room_id, "private").await.unwrap();
    let vis: String = sqlx::query_scalar("SELECT visibility FROM rooms WHERE room_id = $1")
        .bind(&room_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(vis, "private");

    // Invalid value defaults to private.
    storage.set_room_visibility(&room_id, "invalid").await.unwrap();
    let vis: String = sqlx::query_scalar("SELECT visibility FROM rooms WHERE room_id = $1")
        .bind(&room_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(vis, "private");

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// set_room_alias + get_room_alias + get_room_aliases + get_room_by_alias
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_get_room_alias_round_trip() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let room_id = unique_room_id();
    storage
        .create_room(&room_id, "@a:localhost", "invite", "9", false)
        .await
        .unwrap();

    let alias = format!("#alias_{}:localhost", unique_id());
    storage.set_room_alias(&room_id, &alias, "@a:localhost").await.unwrap();

    // get_room_alias returns one alias.
    let one = storage.get_room_alias(&room_id).await.unwrap();
    assert_eq!(one, Some(alias.clone()));

    // get_room_aliases returns all aliases for the room.
    let all = storage.get_room_aliases(&room_id).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0], alias);

    // get_room_by_alias resolves alias → room_id.
    let resolved = storage.get_room_by_alias(&alias).await.unwrap();
    assert_eq!(resolved, Some(room_id.clone()));

    // Non-existent alias → None.
    let missing = storage.get_room_by_alias("#nope:localhost").await.unwrap();
    assert!(missing.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// remove_room_alias / remove_room_alias_by_name
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_remove_room_alias_by_room() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let room_id = unique_room_id();
    storage
        .create_room(&room_id, "@a:localhost", "invite", "9", false)
        .await
        .unwrap();
    let alias = format!("#rm_{}:localhost", unique_id());
    storage.set_room_alias(&room_id, &alias, "@a:localhost").await.unwrap();
    assert_eq!(storage.get_room_alias(&room_id).await.unwrap(), Some(alias.clone()));

    // remove_room_alias deletes all aliases for the room.
    storage.remove_room_alias(&room_id).await.unwrap();
    assert!(storage.get_room_alias(&room_id).await.unwrap().is_none());
    assert!(storage.get_room_by_alias(&alias).await.unwrap().is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_remove_room_alias_by_name() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let room_id = unique_room_id();
    storage
        .create_room(&room_id, "@a:localhost", "invite", "9", false)
        .await
        .unwrap();
    let alias = format!("#rmn_{}:localhost", unique_id());
    storage.set_room_alias(&room_id, &alias, "@a:localhost").await.unwrap();
    assert_eq!(
        storage.get_room_by_alias(&alias).await.unwrap(),
        Some(room_id.clone())
    );

    // remove_room_alias_by_name deletes a single alias by name.
    storage.remove_room_alias_by_name(&alias).await.unwrap();
    assert!(storage.get_room_by_alias(&alias).await.unwrap().is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// delete_room
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_room() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let room_id = unique_room_id();
    storage
        .create_room(&room_id, "@a:localhost", "invite", "9", false)
        .await
        .unwrap();
    assert!(storage.room_exists(&room_id).await.unwrap());

    storage.delete_room(&room_id).await.unwrap();

    assert!(!storage.room_exists(&room_id).await.unwrap());
    assert!(storage.get_room(&room_id).await.unwrap().is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// set_room_version
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_room_version() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let room_id = unique_room_id();
    storage
        .create_room(&room_id, "@a:localhost", "invite", "9", false)
        .await
        .unwrap();

    storage.set_room_version(&room_id, "11").await.unwrap();
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert_eq!(room.room_version, "11");

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// set_room_directory + is_room_in_directory + remove_room_directory
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_room_directory_is_and_remove() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let room_id = unique_room_id();
    storage
        .create_room(&room_id, "@a:localhost", "invite", "9", false)
        .await
        .unwrap();

    // Not in directory initially.
    assert!(!storage.is_room_in_directory(&room_id).await.unwrap());

    // Add to directory as public.
    storage.set_room_directory(&room_id, true).await.unwrap();
    assert!(storage.is_room_in_directory(&room_id).await.unwrap());

    // set_room_directory also updates rooms.is_public.
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert!(room.is_public);

    // Update to private in directory: is_room_in_directory returns true only
    // when the directory row's is_public column is true.
    storage.set_room_directory(&room_id, false).await.unwrap();
    assert!(!storage.is_room_in_directory(&room_id).await.unwrap());

    // Remove from directory entirely.
    storage.remove_room_directory(&room_id).await.unwrap();
    assert!(!storage.is_room_in_directory(&room_id).await.unwrap());

    // Verify the directory row was actually deleted (not just is_public=false).
    let row: Option<bool> =
        sqlx::query_scalar("SELECT is_public FROM room_directory WHERE room_id = $1")
            .bind(&room_id)
            .fetch_optional(pool.as_ref())
            .await
            .unwrap();
    assert!(row.is_none(), "row should be deleted from room_directory");

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// shutdown_room
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_shutdown_room() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let room_id = unique_room_id();
    storage
        .create_room(&room_id, "@a:localhost", "invite", "9", true)
        .await
        .unwrap();
    storage.update_room_name(&room_id, "Original Name").await.unwrap();

    storage.shutdown_room(&room_id).await.unwrap();

    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert!(!room.is_public, "shutdown should make room private");
    assert_eq!(
        room.name,
        Some("Original Name (SHUTDOWN)".to_string()),
        "shutdown appends (SHUTDOWN) to the name"
    );

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// copy_room_state
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_copy_room_state() {
    let _guard = room_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let source_room = format!("!src_{uid}:localhost");
    let target_room = format!("!tgt_{uid}:localhost");
    storage
        .create_room(&source_room, "@a:localhost", "invite", "9", false)
        .await
        .unwrap();
    storage
        .create_room(&target_room, "@a:localhost", "invite", "9", false)
        .await
        .unwrap();

    // Insert source state events directly into the events table.
    // The events table has both user_id and sender columns; both are provided.
    for (etype, skey, ts) in [
        ("m.room.name", "", 1_000_i64),
        ("m.room.topic", "", 2_000),
        ("m.room.member", "@bob:localhost", 3_000),
    ] {
        sqlx::query(
            r"
            INSERT INTO events (event_id, room_id, user_id, event_type, content, origin_server_ts, sender, state_key)
            VALUES ($1, $2, $3, $4, $5, $6, $3, $7)
            ",
        )
        .bind(format!("$rst_src_{uid}_{etype}_{ts}:localhost"))
        .bind(&source_room)
        .bind("@a:localhost")
        .bind(etype)
        .bind(serde_json::json!({}))
        .bind(ts)
        .bind(skey)
        .execute(pool.as_ref())
        .await
        .unwrap();
    }

    // Insert a non-state event (state_key IS NULL) that should be ignored.
    sqlx::query(
        r"
        INSERT INTO events (event_id, room_id, user_id, event_type, content, origin_server_ts, sender, state_key)
        VALUES ($1, $2, $3, $4, $5, $6, $3, NULL)
        ",
    )
    .bind(format!("$rst_src_{uid}_msg:localhost"))
    .bind(&source_room)
    .bind("@a:localhost")
    .bind("m.room.message")
    .bind(serde_json::json!({"body": "ignored"}))
    .bind(4_000_i64)
    .execute(pool.as_ref())
    .await
    .unwrap();

    // Copy state from source → target.
    storage.copy_room_state(&source_room, &target_room).await.unwrap();

    // Verify target room_state_events has 3 rows (the 3 state events).
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM room_state_events WHERE room_id = $1")
        .bind(&target_room)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(count, 3, "should copy 3 state events (ignoring non-state)");

    // Verify the types were copied (ordered by type for determinism).
    let types: Vec<String> =
        sqlx::query_scalar("SELECT type FROM room_state_events WHERE room_id = $1 ORDER BY type")
            .bind(&target_room)
            .fetch_all(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(types, vec!["m.room.member", "m.room.name", "m.room.topic"]);

    // Source room_state_events should still be empty (copy is one-way).
    let src_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM room_state_events WHERE room_id = $1")
            .bind(&source_room)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(src_count, 0);

    // Cleanup: remove the events we inserted (room_state_events is wiped by teardown).
    sqlx::query("DELETE FROM events WHERE room_id = $1")
        .bind(&source_room)
        .execute(pool.as_ref())
        .await
        .ok();

    teardown(pool.as_ref()).await;
}
