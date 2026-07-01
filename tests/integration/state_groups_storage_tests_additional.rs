//! Additional integration tests for `StateGroupStorage` covering advanced DAG
//! scenarios not exercised by `state_groups_storage_tests_migrated.rs`:
//!   - Multi-level state chains (depth >= 3)
//!   - Multi-branch merges (a group with 2+ prevs)
//!   - Diamond dependencies (transitive ancestor resolution)
//!   - Cycle handling (visited-set termination)
//!   - Cross-room state isolation
//!   - Empty-slice no-op contracts for batch APIs
//!   - Large batch `set_state_entries` round-trip
//!   - `bind_event_to_state_group` rebind across groups
//!   - `get_room_state_groups` DESC ordering
//!
//! These tests complement the existing 20 tests, which already cover basic
//! create/get/upsert, single-edge traversal, single-entry state, and
//! single-level state resolution.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_storage::state_groups::{StateGroupStateEntry, StateGroupStorage};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Warm up the shared pool on the current tokio runtime.
///
/// Each `#[tokio::test]` spawns its own runtime. When the shared `OnceCell`
/// pool is first initialized on test N's runtime, sqlx creates connections
/// bound to that runtime. After test N returns, its runtime shuts down and
/// those connections are orphaned. The next test reuses the orphaned pool
/// and hits `PoolTimedOut` on its first DB operation.
///
/// This helper forces the pool to establish fresh connections on the *current*
/// runtime by issuing a trivial `SELECT 1` with a short timeout, retrying a
/// few times so transient orphans are replaced before the test body runs.
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

async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
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
    .expect("Failed to create rooms table");

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
            stream_ordering BIGSERIAL,
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create events table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS state_groups (
            id BIGSERIAL PRIMARY KEY,
            room_id TEXT NOT NULL,
            event_id TEXT NOT NULL,
            state_hash TEXT NOT NULL UNIQUE,
            created_ts BIGINT NOT NULL,
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
            FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create state_groups table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS state_group_edges (
            state_group_id BIGINT NOT NULL,
            prev_state_group_id BIGINT NOT NULL,
            PRIMARY KEY (state_group_id, prev_state_group_id),
            FOREIGN KEY (state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE,
            FOREIGN KEY (prev_state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create state_group_edges table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS event_to_state_groups (
            event_id TEXT NOT NULL PRIMARY KEY,
            state_group_id BIGINT NOT NULL,
            FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE,
            FOREIGN KEY (state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create event_to_state_groups table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS state_group_state (
            state_group_id BIGINT NOT NULL,
            event_type TEXT NOT NULL,
            state_key TEXT NOT NULL,
            event_id TEXT NOT NULL,
            PRIMARY KEY (state_group_id, event_type, state_key),
            FOREIGN KEY (state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE,
            FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create state_group_state table");
}

async fn insert_room(pool: &sqlx::PgPool, room_id: &str) {
    sqlx::query(r#"INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3)"#)
        .bind(room_id)
        .bind("@creator:test")
        .bind(1000_i64)
        .execute(pool)
        .await
        .expect("Failed to insert room");
}

async fn insert_event(pool: &sqlx::PgPool, event_id: &str, room_id: &str) {
    sqlx::query(
        r#"INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(event_id)
    .bind(room_id)
    .bind("@sender:test")
    .bind("m.room.message")
    .bind(serde_json::json!({}))
    .bind(1000_i64)
    .execute(pool)
    .await
    .expect("Failed to insert event");
}

/// Helper: create a state group with its underlying room + event.
async fn make_state_group(
    storage: &StateGroupStorage,
    pool: &sqlx::PgPool,
    room_id: &str,
    suffix: u64,
    index: usize,
) -> i64 {
    let event_id = format!("$sg_{suffix}_{index}:test");
    let state_hash = format!("hash_{suffix}_{index}");
    insert_event(pool, &event_id, room_id).await;
    storage
        .create_state_group(room_id, &event_id, &state_hash, 1000 + index as i64)
        .await
        .unwrap()
}

/// Helper: set a state entry with its underlying event.
///
/// `label` is combined with an internal `unique_id()` to derive a globally
/// unique `event_id`, so callers do not need to thread a suffix through.
async fn set_entry(
    storage: &StateGroupStorage,
    pool: &sqlx::PgPool,
    room_id: &str,
    sg_id: i64,
    event_type: &str,
    state_key: &str,
    label: &str,
) -> String {
    let uid = unique_id();
    let event_id = format!("$st_{uid}_{label}:test");
    insert_event(pool, &event_id, room_id).await;
    storage
        .set_state_entry(sg_id, event_type, state_key, &event_id)
        .await
        .unwrap();
    event_id
}

// =============================================================================
// Multi-level chain (depth >= 3)
// =============================================================================

#[tokio::test]
async fn test_resolve_state_multi_level_chain() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!chain_{suffix}:test");
    insert_room(&pool, &room_id).await;

    // Build a 4-level chain: sg0 -> sg1 -> sg2 -> sg3 (each prev links upward)
    let sg0 = make_state_group(&storage, &pool, &room_id, suffix, 0).await;
    let sg1 = make_state_group(&storage, &pool, &room_id, suffix, 1).await;
    let sg2 = make_state_group(&storage, &pool, &room_id, suffix, 2).await;
    let sg3 = make_state_group(&storage, &pool, &room_id, suffix, 3).await;

    storage.add_state_group_edge(sg1, sg0).await.unwrap();
    storage.add_state_group_edge(sg2, sg1).await.unwrap();
    storage.add_state_group_edge(sg3, sg2).await.unwrap();

    // Each level owns a distinct (event_type, state_key) so the merged result
    // should contain all 4 entries.
    let e0 = set_entry(&storage, &pool, &room_id, sg0, "m.room.create", "", "c0").await;
    let e1 = set_entry(&storage, &pool, &room_id, sg1, "m.room.power_levels", "", "c1").await;
    let e2 = set_entry(&storage, &pool, &room_id, sg2, "m.room.join_rules", "", "c2").await;
    let e3 = set_entry(&storage, &pool, &room_id, sg3, "m.room.name", "", "c3").await;

    let resolved = storage.resolve_state_for_group(sg3).await.unwrap();
    assert_eq!(resolved.len(), 4, "all four levels should contribute state");
    assert_eq!(resolved.get(&("m.room.create".to_string(), "".to_string())), Some(&e0));
    assert_eq!(resolved.get(&("m.room.power_levels".to_string(), "".to_string())), Some(&e1));
    assert_eq!(resolved.get(&("m.room.join_rules".to_string(), "".to_string())), Some(&e2));
    assert_eq!(resolved.get(&("m.room.name".to_string(), "".to_string())), Some(&e3));
}

#[tokio::test]
async fn test_resolve_state_chain_child_overrides_deep_ancestor() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!chainovr_{suffix}:test");
    insert_room(&pool, &room_id).await;

    let sg0 = make_state_group(&storage, &pool, &room_id, suffix, 0).await;
    let sg1 = make_state_group(&storage, &pool, &room_id, suffix, 1).await;
    let sg2 = make_state_group(&storage, &pool, &room_id, suffix, 2).await;

    storage.add_state_group_edge(sg1, sg0).await.unwrap();
    storage.add_state_group_edge(sg2, sg1).await.unwrap();

    // Same (type, key) at every level; the BFS visits sg2 first so its entry
    // wins, and `or_insert` keeps the first-wins semantics.
    let old_event = format!("$old_{suffix}:test");
    let mid_event = format!("$mid_{suffix}:test");
    let new_event = format!("$new_{suffix}:test");
    insert_event(&pool, &old_event, &room_id).await;
    insert_event(&pool, &mid_event, &room_id).await;
    insert_event(&pool, &new_event, &room_id).await;

    storage
        .set_state_entry(sg0, "m.room.name", "", &old_event)
        .await
        .unwrap();
    storage
        .set_state_entry(sg1, "m.room.name", "", &mid_event)
        .await
        .unwrap();
    storage
        .set_state_entry(sg2, "m.room.name", "", &new_event)
        .await
        .unwrap();

    let resolved = storage.resolve_state_for_group(sg2).await.unwrap();
    assert_eq!(resolved.len(), 1);
    // BFS starts at sg2, so the child's entry is recorded first.
    assert_eq!(
        resolved.get(&("m.room.name".to_string(), "".to_string())),
        Some(&new_event),
        "descendant (sg2) should win over deep ancestors"
    );
}

// =============================================================================
// Multi-branch merge
// =============================================================================

#[tokio::test]
async fn test_resolve_state_multi_branch_merge() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!branch_{suffix}:test");
    insert_room(&pool, &room_id).await;

    // sg_head has two prevs (sg_left, sg_right), each contributing distinct state.
    let sg_left = make_state_group(&storage, &pool, &room_id, suffix, 10).await;
    let sg_right = make_state_group(&storage, &pool, &room_id, suffix, 11).await;
    let sg_head = make_state_group(&storage, &pool, &room_id, suffix, 12).await;

    storage.add_state_group_edge(sg_head, sg_left).await.unwrap();
    storage.add_state_group_edge(sg_head, sg_right).await.unwrap();

    let e_left = set_entry(
        &storage,
        &pool,
        &room_id,
        sg_left,
        "m.room.member",
        "@alice:test",
        "ml",
    )
    .await;
    let e_right = set_entry(
        &storage,
        &pool,
        &room_id,
        sg_right,
        "m.room.member",
        "@bob:test",
        "mr",
    )
    .await;
    let e_head = set_entry(
        &storage,
        &pool,
        &room_id,
        sg_head,
        "m.room.name",
        "",
        "mh",
    )
    .await;

    let resolved = storage.resolve_state_for_group(sg_head).await.unwrap();
    assert_eq!(resolved.len(), 3, "head + both branches should merge");
    assert_eq!(
        resolved.get(&("m.room.member".to_string(), "@alice:test".to_string())),
        Some(&e_left)
    );
    assert_eq!(
        resolved.get(&("m.room.member".to_string(), "@bob:test".to_string())),
        Some(&e_right)
    );
    assert_eq!(resolved.get(&("m.room.name".to_string(), "".to_string())), Some(&e_head));
}

#[tokio::test]
async fn test_resolve_state_diamond_dependency() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!diamond_{suffix}:test");
    insert_room(&pool, &room_id).await;

    // Diamond: root -> {left, right} -> head
    //   root (sg_root)
    //     |
    //     +-> sg_left  -+
    //     +-> sg_right -+-> sg_head
    let sg_root = make_state_group(&storage, &pool, &room_id, suffix, 20).await;
    let sg_left = make_state_group(&storage, &pool, &room_id, suffix, 21).await;
    let sg_right = make_state_group(&storage, &pool, &room_id, suffix, 22).await;
    let sg_head = make_state_group(&storage, &pool, &room_id, suffix, 23).await;

    storage.add_state_group_edge(sg_left, sg_root).await.unwrap();
    storage.add_state_group_edge(sg_right, sg_root).await.unwrap();
    storage.add_state_group_edge(sg_head, sg_left).await.unwrap();
    storage.add_state_group_edge(sg_head, sg_right).await.unwrap();

    let e_root = set_entry(&storage, &pool, &room_id, sg_root, "m.room.create", "", "dr").await;
    let e_left = set_entry(
        &storage,
        &pool,
        &room_id,
        sg_left,
        "m.room.member",
        "@alice:test",
        "dl",
    )
    .await;
    let e_right = set_entry(
        &storage,
        &pool,
        &room_id,
        sg_right,
        "m.room.member",
        "@bob:test",
        "drr",
    )
    .await;
    let e_head = set_entry(&storage, &pool, &room_id, sg_head, "m.room.name", "", "dh").await;

    let resolved = storage.resolve_state_for_group(sg_head).await.unwrap();
    // root's create + left's alice + right's bob + head's name = 4 distinct keys.
    assert_eq!(resolved.len(), 4, "diamond should resolve root + both branches + head");
    assert_eq!(resolved.get(&("m.room.create".to_string(), "".to_string())), Some(&e_root));
    assert_eq!(
        resolved.get(&("m.room.member".to_string(), "@alice:test".to_string())),
        Some(&e_left)
    );
    assert_eq!(
        resolved.get(&("m.room.member".to_string(), "@bob:test".to_string())),
        Some(&e_right)
    );
    assert_eq!(resolved.get(&("m.room.name".to_string(), "".to_string())), Some(&e_head));
}

// =============================================================================
// Cycle handling — resolve_state_for_group must terminate via visited set
// =============================================================================

#[tokio::test]
async fn test_resolve_state_terminates_on_cycle() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!cycle_{suffix}:test");
    insert_room(&pool, &room_id).await;

    let sg_a = make_state_group(&storage, &pool, &room_id, suffix, 30).await;
    let sg_b = make_state_group(&storage, &pool, &room_id, suffix, 31).await;

    // A -> B and B -> A forms a cycle.
    storage.add_state_group_edge(sg_a, sg_b).await.unwrap();
    storage.add_state_group_edge(sg_b, sg_a).await.unwrap();

    let e_a = set_entry(&storage, &pool, &room_id, sg_a, "m.room.name", "", "ca").await;
    let e_b = set_entry(&storage, &pool, &room_id, sg_b, "m.room.topic", "", "cb").await;

    // Must terminate (visited set prevents infinite loop) and still merge state.
    let resolved = storage
        .resolve_state_for_group(sg_a)
        .await
        .expect("cycle should not cause error");
    assert_eq!(resolved.len(), 2, "both groups' state should be merged despite the cycle");
    assert_eq!(resolved.get(&("m.room.name".to_string(), "".to_string())), Some(&e_a));
    assert_eq!(resolved.get(&("m.room.topic".to_string(), "".to_string())), Some(&e_b));
}

#[tokio::test]
async fn test_resolve_state_self_loop_terminates() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!selfloop_{suffix}:test");
    insert_room(&pool, &room_id).await;

    let sg = make_state_group(&storage, &pool, &room_id, suffix, 40).await;
    // Self-loop: sg -> sg.
    storage.add_state_group_edge(sg, sg).await.unwrap();

    let e = set_entry(&storage, &pool, &room_id, sg, "m.room.name", "", "sl").await;

    let resolved = storage.resolve_state_for_group(sg).await.unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved.get(&("m.room.name".to_string(), "".to_string())), Some(&e));
}

// =============================================================================
// Cross-room isolation
// =============================================================================

#[tokio::test]
async fn test_state_groups_isolated_across_rooms() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();

    let room_a = format!("!iso_a_{suffix}:test");
    let room_b = format!("!iso_b_{suffix}:test");
    insert_room(&pool, &room_a).await;
    insert_room(&pool, &room_b).await;

    let sg_a = make_state_group(&storage, &pool, &room_a, suffix, 50).await;
    let sg_b = make_state_group(&storage, &pool, &room_b, suffix, 51).await;

    let e_a = set_entry(&storage, &pool, &room_a, sg_a, "m.room.name", "", "ia").await;
    let e_b = set_entry(&storage, &pool, &room_b, sg_b, "m.room.name", "", "ib").await;

    // Resolving room A's group must NOT see room B's state.
    let resolved_a = storage.resolve_state_for_group(sg_a).await.unwrap();
    assert_eq!(resolved_a.len(), 1);
    assert_eq!(resolved_a.get(&("m.room.name".to_string(), "".to_string())), Some(&e_a));
    assert_ne!(
        resolved_a.get(&("m.room.name".to_string(), "".to_string())),
        Some(&e_b),
        "room B's state must not leak into room A"
    );

    // get_room_state_groups must only return groups for the requested room.
    let groups_a = storage.get_room_state_groups(&room_a, 100).await.unwrap();
    let groups_b = storage.get_room_state_groups(&room_b, 100).await.unwrap();
    assert!(groups_a.iter().all(|g| g.room_id == room_a));
    assert!(groups_b.iter().all(|g| g.room_id == room_b));
    assert!(!groups_a.iter().any(|g| g.id == sg_b));
    assert!(!groups_b.iter().any(|g| g.id == sg_a));
}

// =============================================================================
// Empty-slice no-op contracts for batch APIs
// =============================================================================

#[tokio::test]
async fn test_add_state_group_edges_empty_slice_is_noop() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!empty_{suffix}:test");
    insert_room(&pool, &room_id).await;

    let sg = make_state_group(&storage, &pool, &room_id, suffix, 60).await;

    // Empty slice: must return Ok(()) without touching the DB.
    let result = storage.add_state_group_edges(sg, &[]).await;
    assert!(result.is_ok(), "empty slice should be a no-op success");

    let prev = storage.get_prev_state_groups(sg).await.unwrap();
    assert!(prev.is_empty(), "no edges should have been inserted");
}

#[tokio::test]
async fn test_batch_bind_events_empty_slice_is_noop() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!emptybind_{suffix}:test");
    insert_room(&pool, &room_id).await;

    let sg = make_state_group(&storage, &pool, &room_id, suffix, 70).await;

    let result = storage.batch_bind_events_to_state_group(&[], sg).await;
    assert!(result.is_ok(), "empty slice should be a no-op success");

    // No events should be bound to this group.
    let probe = format!("$probe_{suffix}:test");
    insert_event(&pool, &probe, &room_id).await;
    let lookup = storage.get_state_group_for_event(&probe).await.unwrap();
    assert!(lookup.is_none(), "probe event should not be bound to any group");
}

#[tokio::test]
async fn test_set_state_entries_empty_slice_is_noop() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!emptyentries_{suffix}:test");
    insert_room(&pool, &room_id).await;

    let sg = make_state_group(&storage, &pool, &room_id, suffix, 80).await;

    let result = storage.set_state_entries(sg, &[]).await;
    assert!(result.is_ok(), "empty slice should be a no-op success");

    let state = storage.get_state_at_group(sg).await.unwrap();
    assert!(state.is_empty(), "no state entries should have been inserted");
}

// =============================================================================
// Large batch round-trip
// =============================================================================

#[tokio::test]
async fn test_set_state_entries_large_batch_round_trip() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!largebatch_{suffix}:test");
    insert_room(&pool, &room_id).await;

    let sg = make_state_group(&storage, &pool, &room_id, suffix, 90).await;

    // 50 distinct (type, key) entries across two event types.
    let count = 50usize;
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let event_id = format!("$lb_{suffix}_{i}:test");
        insert_event(&pool, &event_id, &room_id).await;
        entries.push(StateGroupStateEntry {
            event_type: if i % 2 == 0 { "m.room.member".to_string() } else { "m.room.custom".to_string() },
            state_key: format!("@user{i}:test"),
            event_id,
        });
    }

    storage.set_state_entries(sg, &entries).await.unwrap();

    let state = storage.get_state_at_group(sg).await.unwrap();
    assert_eq!(state.len(), count, "all batched entries should be retrievable");

    // Spot-check a few entries via the point-lookup API.
    for i in [0, 7, 25, 49] {
        let expected = &entries[i];
        let got = storage
            .get_state_entry(sg, &expected.event_type, &expected.state_key)
            .await
            .unwrap();
        assert_eq!(got.as_deref(), Some(expected.event_id.as_str()));
    }
}

// =============================================================================
// bind_event rebind across groups
// =============================================================================

#[tokio::test]
async fn test_bind_event_rebind_across_groups() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!rebind_{suffix}:test");
    insert_room(&pool, &room_id).await;

    let sg1 = make_state_group(&storage, &pool, &room_id, suffix, 100).await;
    let sg2 = make_state_group(&storage, &pool, &room_id, suffix, 101).await;

    let target_event = format!("$target_{suffix}:test");
    insert_event(&pool, &target_event, &room_id).await;

    // Bind to sg1 first.
    storage.bind_event_to_state_group(&target_event, sg1).await.unwrap();
    assert_eq!(
        storage.get_state_group_for_event(&target_event).await.unwrap(),
        Some(sg1)
    );

    // Rebind to sg2 — ON CONFLICT UPDATE should switch the binding.
    storage.bind_event_to_state_group(&target_event, sg2).await.unwrap();
    assert_eq!(
        storage.get_state_group_for_event(&target_event).await.unwrap(),
        Some(sg2),
        "rebind should switch the event's state group"
    );
}

// =============================================================================
// get_room_state_groups DESC ordering
// =============================================================================

#[tokio::test]
async fn test_get_room_state_groups_desc_ordering() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!order_{suffix}:test");
    insert_room(&pool, &room_id).await;

    // Insert 5 groups; BIGSERIAL assigns increasing ids.
    let mut ids = Vec::new();
    for i in 0..5 {
        ids.push(make_state_group(&storage, &pool, &room_id, suffix, 200 + i).await);
    }

    let groups = storage.get_room_state_groups(&room_id, 100).await.unwrap();
    assert_eq!(groups.len(), 5);

    // Verify DESC by id.
    let returned_ids: Vec<i64> = groups.iter().map(|g| g.id).collect();
    let mut expected = ids.clone();
    expected.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(returned_ids, expected, "groups should be returned in DESC id order");

    // Limit respected.
    let limited = storage.get_room_state_groups(&room_id, 3).await.unwrap();
    assert_eq!(limited.len(), 3);
    // First 3 of DESC order == top 3 ids.
    assert_eq!(limited[0].id, expected[0]);
    assert_eq!(limited[1].id, expected[1]);
    assert_eq!(limited[2].id, expected[2]);
}

// =============================================================================
// Batch bind then resolve — end-to-end
// =============================================================================

#[tokio::test]
async fn test_batch_bind_then_resolve_end_to_end() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!e2e_{suffix}:test");
    insert_room(&pool, &room_id).await;

    // Two-level chain: sg_root -> sg_head, each with batched state.
    let sg_root = make_state_group(&storage, &pool, &room_id, suffix, 300).await;
    let sg_head = make_state_group(&storage, &pool, &room_id, suffix, 301).await;
    storage.add_state_group_edge(sg_head, sg_root).await.unwrap();

    // Batch-set 3 entries on root and 2 on head (distinct keys).
    let mut root_entries = Vec::new();
    for i in 0..3 {
        let event_id = format!("$e2e_root_{suffix}_{i}:test");
        insert_event(&pool, &event_id, &room_id).await;
        root_entries.push(StateGroupStateEntry {
            event_type: "m.room.member".to_string(),
            state_key: format!("@root_user{i}:test"),
            event_id,
        });
    }
    storage.set_state_entries(sg_root, &root_entries).await.unwrap();

    let mut head_entries = Vec::new();
    for i in 0..2 {
        let event_id = format!("$e2e_head_{suffix}_{i}:test");
        insert_event(&pool, &event_id, &room_id).await;
        head_entries.push(StateGroupStateEntry {
            event_type: "m.room.name".to_string(),
            state_key: format!("head{i}"),
            event_id,
        });
    }
    storage.set_state_entries(sg_head, &head_entries).await.unwrap();

    // Batch-bind several events to sg_head.
    let mut bound_events = Vec::new();
    for i in 0..4 {
        let event_id = format!("$e2e_bound_{suffix}_{i}:test");
        insert_event(&pool, &event_id, &room_id).await;
        bound_events.push(event_id);
    }
    storage
        .batch_bind_events_to_state_group(&bound_events, sg_head)
        .await
        .unwrap();

    // All bound events should resolve back to sg_head.
    for eid in &bound_events {
        assert_eq!(
            storage.get_state_group_for_event(eid).await.unwrap(),
            Some(sg_head)
        );
    }

    // Resolving sg_head must merge root + head state (5 distinct keys total).
    let resolved = storage.resolve_state_for_group(sg_head).await.unwrap();
    assert_eq!(resolved.len(), 5, "root(3) + head(2) entries should merge");
    for entry in &root_entries {
        assert_eq!(
            resolved.get(&(entry.event_type.clone(), entry.state_key.clone())),
            Some(&entry.event_id)
        );
    }
    for entry in &head_entries {
        assert_eq!(
            resolved.get(&(entry.event_type.clone(), entry.state_key.clone())),
            Some(&entry.event_id)
        );
    }
}

// =============================================================================
// get_state_at_group returns all rows for a group with multiple entries
// =============================================================================

#[tokio::test]
async fn test_get_state_at_group_returns_all_entries() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!allentries_{suffix}:test");
    insert_room(&pool, &room_id).await;

    let sg = make_state_group(&storage, &pool, &room_id, suffix, 400).await;

    // Mix of types and keys via the batch API.
    let entries = vec![
        StateGroupStateEntry {
            event_type: "m.room.member".to_string(),
            state_key: "@alice:test".to_string(),
            event_id: format!("$ae1_{suffix}:test"),
        },
        StateGroupStateEntry {
            event_type: "m.room.member".to_string(),
            state_key: "@bob:test".to_string(),
            event_id: format!("$ae2_{suffix}:test"),
        },
        StateGroupStateEntry {
            event_type: "m.room.name".to_string(),
            state_key: "".to_string(),
            event_id: format!("$ae3_{suffix}:test"),
        },
        StateGroupStateEntry {
            event_type: "m.room.topic".to_string(),
            state_key: "".to_string(),
            event_id: format!("$ae4_{suffix}:test"),
        },
        StateGroupStateEntry {
            event_type: "m.room.power_levels".to_string(),
            state_key: "".to_string(),
            event_id: format!("$ae5_{suffix}:test"),
        },
    ];
    for entry in &entries {
        insert_event(&pool, &entry.event_id, &room_id).await;
    }
    storage.set_state_entries(sg, &entries).await.unwrap();

    let state = storage.get_state_at_group(sg).await.unwrap();
    assert_eq!(state.len(), entries.len());
    // Every entry should be present (order not guaranteed, so check as a set).
    let mut got_keys = state
        .iter()
        .map(|s| (s.event_type.clone(), s.state_key.clone()))
        .collect::<Vec<_>>();
    got_keys.sort();
    let mut want_keys = entries
        .iter()
        .map(|e| (e.event_type.clone(), e.state_key.clone()))
        .collect::<Vec<_>>();
    want_keys.sort();
    assert_eq!(got_keys, want_keys);
}
