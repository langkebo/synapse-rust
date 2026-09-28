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

/// 回填脚本用的"未回填 v12 房间"判定必须真的能区分两种房间。
///
/// ⚠️ D-86：这个用例原先写的是
/// `SELECT COUNT(*) FROM rooms WHERE room_version >= 12` —— 而
/// `rooms.room_version` 是 **TEXT**（`DEFAULT '6'`）⇒ PG 直接把裸字面量 `12` 定成
/// integer，报 `42883 operator does not exist: text >= integer`，用例必红；
/// 而且它那句 `v12_with_state_groups <= total_v12` 是**恒真**断言（左侧是右侧的子集
/// 计数），即便类型修好也永远不可能失败 —— 属"看起来是门禁、实际不会红"的形态。
/// 现在改成：全局计数只作为背景日志，断言限定在**本用例自己造的房间**上，
/// 并且带一条**反向对照**（没有状态组的房间必须被判为 unbackfilled），
/// 这样判定写错时用例会真的红。
#[tokio::test]
async fn backfill_predicate_distinguishes_bound_from_unbound_v12_rooms() {
    let pool = crate::require_test_pool().await;
    let storage = StateGroupStorage::new(&pool);
    let suffix = unique_id();

    let bound_room = format!("!room_backfill_bound_{}:test", suffix);
    let bound_event = format!("$event_bound_{}:test", suffix);
    let unbound_room = format!("!room_backfill_unbound_{}:test", suffix);

    for room_id in [&bound_room, &unbound_room] {
        sqlx::query("INSERT INTO rooms (room_id, creator, created_ts, room_version) VALUES ($1, $2, $3, $4)")
            .bind(room_id)
            .bind("@creator:test")
            .bind(1000_i64)
            .bind(12_i32)
            .execute(pool.as_ref())
            .await
            .expect("Failed to insert room");
    }

    sqlx::query("INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(&bound_event)
        .bind(&bound_room)
        .bind("@sender:test")
        .bind("m.room.message")
        .bind(serde_json::json!({}))
        .bind(1000_i64)
        .execute(pool.as_ref())
        .await
        .expect("Failed to insert event");

    let group_id = storage
        .create_state_group(&bound_room, &bound_event, &format!("hash_bound_{suffix}"), 1000)
        .await
        .expect("create_state_group");
    storage.bind_event_to_state_group(&bound_event, group_id).await.expect("bind_event_to_state_group");

    // 与回填脚本同款的判定。`room_version` 是 TEXT，必须显式转型（并先用正则挡住非数字版本，
    // 避免 `::int` 对 `org.matrix.msc…` 之类的值报错）。
    const UNBACKFILLED_V12: &str = r#"
        SELECT COUNT(*) FROM rooms r
         WHERE r.room_id = $1
           AND r.room_version ~ '^[0-9]+$'
           AND r.room_version::int >= 12
           AND NOT EXISTS (SELECT 1 FROM state_groups sg WHERE sg.room_id = r.room_id)
    "#;

    let report_bound: i64 = sqlx::query_scalar(UNBACKFILLED_V12)
        .bind(&bound_room)
        .fetch_one(pool.as_ref())
        .await
        .expect("unbackfilled count (bound room)");
    assert_eq!(report_bound, 0, "绑定了状态组的 v12 房间不得被判为待回填");

    let report_unbound: i64 = sqlx::query_scalar(UNBACKFILLED_V12)
        .bind(&unbound_room)
        .fetch_one(pool.as_ref())
        .await
        .expect("unbackfilled count (unbound room)");
    assert_eq!(report_unbound, 1, "没有状态组的 v12 房间必须被判为待回填（反向对照）");

    // 背景信息：全库 v12 房间数与其中已有状态组的数量（不做全局等值断言 —— 并发用例
    // 会同时造房间，全局计数不是本用例能控制的不变量）。
    let (total_v12, v12_with_state_groups): (i64, i64) = sqlx::query_as(
        r#"SELECT
             (SELECT COUNT(*) FROM rooms WHERE room_version ~ '^[0-9]+$' AND room_version::int >= 12),
             (SELECT COUNT(*) FROM rooms r
               WHERE r.room_version ~ '^[0-9]+$' AND r.room_version::int >= 12
                 AND EXISTS (SELECT 1 FROM state_groups sg WHERE sg.room_id = r.room_id))"#,
    )
    .fetch_one(pool.as_ref())
    .await
    .expect("v12 state-group inventory");
    tracing::info!(total_v12, v12_with_state_groups, "Backfill verification");
    assert!(v12_with_state_groups <= total_v12);
}
