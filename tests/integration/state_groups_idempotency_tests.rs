#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! A3: Per-event state group idempotency and replay tests
//!
//! These tests verify:
//! 1. Duplicate bindings are no-ops (idempotency)
//! 2. Duplicate event writes don't create duplicate bindings  
//! 3. Messages after fork resolution are properly bound to state groups

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_storage::state_groups::{StateGroupStateEntry, StateGroupStorage};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1000);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn insert_room(pool: &Arc<sqlx::PgPool>, room_id: &str) {
    sqlx::query("INSERT INTO rooms (room_id, creator, created_ts, room_version) VALUES ($1, $2, $3, $4)")
        .bind(room_id)
        .bind("@creator:test")
        .bind(1000_i64)
        .bind(12_i32)
        .execute(pool.as_ref())
        .await
        .expect("Failed to insert room");
}

async fn insert_event(pool: &Arc<sqlx::PgPool>, event_id: &str, room_id: &str) {
    sqlx::query(
        "INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(event_id)
    .bind(room_id)
    .bind("@sender:test")
    .bind("m.room.message")
    .bind(serde_json::json!({}))
    .bind(1000_i64)
    .execute(pool.as_ref())
    .await
    .expect("Failed to insert event");
}

#[tokio::test]
async fn test_duplicate_bind_is_noop() {
    let pool = crate::require_test_pool().await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();

    let room_id = format!("!room_idempotent_{}:test", suffix);
    let event_id = format!("$event_idempotent_{}:test", suffix);

    insert_room(&pool, &room_id).await;
    insert_event(&pool, &event_id, &room_id).await;

    // Create initial state group
    let state_hash = format!("hash_{}", suffix);
    let group_id = storage.create_state_group(&room_id, &event_id, &state_hash, 1000).await.unwrap();

    // First bind - should INSERT
    storage.bind_event_to_state_group(&event_id, group_id).await.unwrap();

    // Count records after first bind
    let count_after_first =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM event_to_state_groups WHERE event_id = $1")
            .bind(&event_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();

    assert_eq!(count_after_first, 1, "Should have exactly 1 binding after first bind");

    // Second bind - should be a no-op (ON CONFLICT DO UPDATE with same value)
    storage.bind_event_to_state_group(&event_id, group_id).await.unwrap();

    // Count records after second bind
    let count_after_second =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM event_to_state_groups WHERE event_id = $1")
            .bind(&event_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();

    assert_eq!(count_after_second, 1, "Should still have exactly 1 binding after duplicate bind (idempotent)");

    // Verify the event is bound to the correct group
    let bound_group = storage.get_state_group_for_event(&event_id).await.unwrap().unwrap();

    assert_eq!(bound_group, group_id, "Event should be bound to the correct group");
}

#[tokio::test]
async fn test_duplicate_bind_different_group_updates() {
    let pool = crate::require_test_pool().await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();

    let room_id = format!("!room_rebind_{}:test", suffix);
    let event_id1 = format!("$event1_rebind_{}:test", suffix);
    let event_id = format!("$event_rebind_{}:test", suffix);

    insert_room(&pool, &room_id).await;
    insert_event(&pool, &event_id1, &room_id).await;
    insert_event(&pool, &event_id, &room_id).await;

    // Create two state groups
    let state_hash1 = format!("hash1_{}", suffix);
    let state_hash2 = format!("hash2_{}", suffix);
    let group_id1 = storage.create_state_group(&room_id, &event_id1, &state_hash1, 1000).await.unwrap();
    let group_id2 = storage.create_state_group(&room_id, &event_id, &state_hash2, 2000).await.unwrap();

    // Bind event to first group
    storage.bind_event_to_state_group(&event_id, group_id1).await.unwrap();

    let initial_group = storage.get_state_group_for_event(&event_id).await.unwrap().unwrap();

    assert_eq!(initial_group, group_id1, "Initially bound to group 1");

    // Re-bind to second group (simulating re-resolution)
    storage.bind_event_to_state_group(&event_id, group_id2).await.unwrap();

    let updated_group = storage.get_state_group_for_event(&event_id).await.unwrap().unwrap();

    assert_eq!(updated_group, group_id2, "Event should be rebound to group 2");
}

#[tokio::test]
async fn test_batch_bind_idempotency() {
    let pool = crate::require_test_pool().await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();

    let room_id = format!("!room_batch_{}:test", suffix);
    insert_room(&pool, &room_id).await;

    // Create initial state group
    let event_id = format!("$init_batch_{}:test", suffix);
    insert_event(&pool, &event_id, &room_id).await;
    let state_hash = format!("hash_batch_{}", suffix);
    let group_id = storage.create_state_group(&room_id, &event_id, &state_hash, 1000).await.unwrap();

    // Create multiple events
    let event_ids: Vec<String> = (0..5).map(|i| format!("$batch_event_{}_{}:test", i, suffix)).collect();

    // Insert events into events table (required for foreign key constraint)
    for event_id in &event_ids {
        insert_event(&pool, event_id, &room_id).await;
    }

    // First batch bind
    storage.batch_bind_events_to_state_group(&event_ids, group_id).await.unwrap();

    let count_after_first =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM event_to_state_groups WHERE state_group_id = $1")
            .bind(group_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();

    assert_eq!(count_after_first, 5, "Should have exactly 5 bindings after first batch bind");

    // Second batch bind (same events, same group)
    storage.batch_bind_events_to_state_group(&event_ids, group_id).await.unwrap();

    let count_after_second =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM event_to_state_groups WHERE state_group_id = $1")
            .bind(group_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();

    assert_eq!(count_after_second, 5, "Should still have exactly 5 bindings after duplicate batch bind");
}

#[tokio::test]
async fn test_message_after_fork_is_bound_to_group() {
    let pool = crate::require_test_pool().await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();

    // Setup room
    let room_id = format!("!room_fork_bound_{}:test", suffix);
    let initial_event_id = format!("$initial_fork_{}:test", suffix);

    insert_room(&pool, &room_id).await;

    // A3 FIX: Insert the initial event before creating state group
    // (state_groups has FK constraint on event_id)
    insert_event(&pool, &initial_event_id, &room_id).await;

    // Create initial state group (simulating fork resolution)
    let state_hash = format!("fork_resolved_{}", suffix);
    let group_id = storage.create_state_group(&room_id, &initial_event_id, &state_hash, 1000).await.unwrap();

    // Add state entries (all events must exist in events table)
    let member_event_id = format!("$member_{}:test", suffix);
    insert_event(&pool, &member_event_id, &room_id).await;

    let state_entries = vec![
        StateGroupStateEntry {
            event_type: "m.room.create".to_string(),
            state_key: "".to_string(),
            event_id: initial_event_id.clone(),
        },
        StateGroupStateEntry {
            event_type: "m.room.member".to_string(),
            state_key: "@creator:test".to_string(),
            event_id: member_event_id.clone(),
        },
    ];
    storage.set_state_entries(group_id, &state_entries).await.unwrap();

    // Simulate message event being sent AFTER fork resolution
    let message_event_id = format!("$message_after_fork_{}:test", suffix);
    insert_event(&pool, &message_event_id, &room_id).await;

    // A3 Implementation: Bind message to current state group
    // This is what copy_forward or after_state_event should do
    storage.bind_event_to_state_group(&message_event_id, group_id).await.unwrap();

    // Verify: message is bound to the state group
    let bound_group = storage.get_state_group_for_event(&message_event_id).await.unwrap();

    assert!(bound_group.is_some(), "Message after fork should be bound to a state group");
    assert_eq!(bound_group.unwrap(), group_id, "Message should be bound to the resolved state group");

    // Verify: we can retrieve the message via state group
    let all_bindings =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM event_to_state_groups WHERE state_group_id = $1")
            .bind(group_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();

    assert!(all_bindings >= 1, "State group should contain at least the message event");
}

#[tokio::test]
async fn test_concurrent_bind_serializes_correctly() {
    let pool = crate::require_test_pool().await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();

    let room_id = format!("!room_concurrent_{}:test", suffix);
    let event_id = format!("$event_concurrent_{}:test", suffix);

    insert_room(&pool, &room_id).await;
    insert_event(&pool, &event_id, &room_id).await;

    // Create state group
    let state_hash = format!("hash_concurrent_{}", suffix);
    let group_id = storage.create_state_group(&room_id, &event_id, &state_hash, 1000).await.unwrap();

    // Simulate concurrent binds (using tokio::join!)
    let bind1 = storage.bind_event_to_state_group(&event_id, group_id);
    let bind2 = storage.bind_event_to_state_group(&event_id, group_id);

    let (result1, result2) = tokio::join!(bind1, bind2);

    // Both should succeed
    result1.unwrap();
    result2.unwrap();

    // Should still have exactly one binding
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM event_to_state_groups WHERE event_id = $1")
        .bind(&event_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();

    assert_eq!(count, 1, "Concurrent duplicate binds should result in exactly one binding");
}
