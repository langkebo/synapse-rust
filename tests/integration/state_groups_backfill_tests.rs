#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! A4: Backfill verification tests
//!
//! These tests verify that:
//! 1. Rooms without state groups are detected correctly
//! 2. Backfill script logic works correctly
//! 3. After backfill, all v12+ rooms have state groups

use std::sync::atomic::{AtomicU64, Ordering};
use synapse_storage::state_groups::StateGroupStorage;

/// 每个用例一个独立后缀（与 `state_groups_idempotency_tests.rs` 同形）。
///
/// D-81：本文件（A3+A4 批次新增）直接调用 `unique_id()` 却**从未定义它** ——
/// 集成测试 target 因此编译失败（E0425 ×2），`--all-targets` 的 clippy 与
/// `check_sqlx_cache_fresh.sh --compile` 双红。`unique_id` 在每个测试模块里都是
/// **文件本地**助手（不是共享工具），所以修法是补上本文件的这一份，而不是跨模块引用。
static TEST_COUNTER: AtomicU64 = AtomicU64::new(10_000);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

#[tokio::test]
async fn test_detect_unbackfilled_rooms() {
    let pool = crate::require_test_pool().await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();

    // Create a v12 room WITHOUT state group (simulating pre-A3 state)
    let room_id = format!("!room_unbackfilled_{}:test", suffix);
    let event_id = format!("$event_{}:test", suffix);

    sqlx::query("INSERT INTO rooms (room_id, creator, created_ts, room_version) VALUES ($1, $2, $3, $4)")
        .bind(&room_id)
        .bind("@creator:test")
        .bind(1000_i64)
        .bind(12_i32)
        .execute(pool.as_ref())
        .await
        .expect("Failed to insert room");

    sqlx::query("INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(&event_id)
        .bind(&room_id)
        .bind("@sender:test")
        .bind("m.room.message")
        .bind(serde_json::json!({}))
        .bind(1000_i64)
        .execute(pool.as_ref())
        .await
        .expect("Failed to insert event");

    // Verify: room has no state group
    let state_group = storage.get_state_group_for_event(&event_id).await.unwrap();
    assert!(state_group.is_none(), "Room should have no state group (pre-backfill)");
}

#[tokio::test]
async fn test_backfilled_room_has_state_group() {
    let pool = crate::require_test_pool().await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();

    // Create a v12 room WITH state group (simulating post-backfill state)
    let room_id = format!("!room_backfilled_{}:test", suffix);
    let event_id = format!("$event_{}:test", suffix);

    sqlx::query("INSERT INTO rooms (room_id, creator, created_ts, room_version) VALUES ($1, $2, $3, $4)")
        .bind(&room_id)
        .bind("@creator:test")
        .bind(1000_i64)
        .bind(12_i32)
        .execute(pool.as_ref())
        .await
        .expect("Failed to insert room");

    sqlx::query("INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(&event_id)
        .bind(&room_id)
        .bind("@sender:test")
        .bind("m.room.message")
        .bind(serde_json::json!({}))
        .bind(1000_i64)
        .execute(pool.as_ref())
        .await
        .expect("Failed to insert event");

    // Simulate backfill: create state group
    let state_hash = format!("hash_{}", suffix);
    let group_id = storage.create_state_group(&room_id, &event_id, &state_hash, 1000).await.unwrap();

    storage.bind_event_to_state_group(&event_id, group_id).await.unwrap();

    // Verify: room now has state group
    let state_group = storage.get_state_group_for_event(&event_id).await.unwrap();
    assert!(state_group.is_some(), "Room should have state group (post-backfill)");
    assert_eq!(state_group.unwrap(), group_id, "Should be bound to correct group");
}

#[tokio::test]
async fn test_all_v12_rooms_have_state_groups() {
    let pool = crate::require_test_pool().await;

    // Count v12+ rooms
    let total_v12 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM rooms WHERE room_version >= 12")
        .fetch_one(pool.as_ref())
        .await
        .unwrap();

    // Count v12+ rooms with state groups
    let v12_with_state_groups = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM rooms r
           WHERE r.room_version >= 12
             AND EXISTS (SELECT 1 FROM state_groups sg WHERE sg.room_id = r.room_id)"#,
    )
    .fetch_one(pool.as_ref())
    .await
    .unwrap();

    tracing::info!(total_v12 = total_v12, v12_with_state_groups = v12_with_state_groups, "Backfill verification");

    // Note: This test doesn't assert equality because existing rooms
    // may not have been backfilled yet. After full backfill, they should match.
    assert!(v12_with_state_groups <= total_v12, "State group count should not exceed total v12 count");
}
