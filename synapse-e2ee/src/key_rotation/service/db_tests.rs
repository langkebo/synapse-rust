//! DB round trip for `key_rotation_log` — the table the E2EE rotation path writes
//! and `GET /_matrix/vendor/v1/keys/rotation/history/{device_id}` reads.
//!
//! Why this file exists: `log_rotation` bound `None::<String>` into
//! `key_rotation_log.device_id`, which the baseline declares `NOT NULL`, so every
//! call aborted with `null value in column "device_id" violates not-null
//! constraint`. Nothing caught it — the only caller
//! (`KeyRotationService::rotate_megolm_session`) is not wired into production, and
//! this module had no DB coverage at all (the sibling `tests` module is pure
//! config/constructor checks).
//!
//! The guard below is a real write→read round trip rather than a mock, so it pins
//! three things at once: the column's `NOT NULL`, the argument order of
//! `log_rotation`, and the reader's `(user_id, device_id)` filter.

use super::*;
use synapse_common::test_isolation::IsolatedTestPool;

/// The workspace baseline migration. The bytes are load-bearing (the shared
/// template name is a content fingerprint of this string), so it must stay
/// byte-identical to the copies in `synapse-storage/src/test_isolation.rs`,
/// `synapse-federation/src/test_isolation.rs`, `synapse-services/src/test_utils.rs`
/// and `synapse-e2ee/src/megolm/storage/db_tests.rs`.
const BASELINE_SQL: &str = include_str!("../../../../migrations/00000000_unified_schema_v12.sql");

#[tokio::test]
async fn test_log_rotation_round_trips_through_the_device_scoped_reader() {
    let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
    let storage = KeyRotationStorage::new(isolated.pool());

    let user = "@alice:example.com";
    let device = "DEVICEALICE";
    let room = "!kr:example.com";

    // Aborted with a NOT NULL violation while `device_id` was bound as SQL NULL.
    storage.log_rotation(user, device, room, "megolm").await.expect("log_rotation must insert");

    let history = storage.get_device_rotation_history(user, device).await.expect("history read");
    assert_eq!(history.len(), 1, "the row must be readable back through the device-scoped reader: {history:?}");
    let (key_id, rotated_ts) = &history[0];
    assert!(key_id.is_some(), "`log_rotation` writes `new_key_id`, so it must come back non-NULL: {history:?}");
    assert!(rotated_ts.is_some_and(|ts| ts > 0), "`rotated_at` must be a real millisecond stamp, not 0: {history:?}");

    // The row carries the device that performed the rotation, not merely *some*
    // non-NULL value: another device must not see it.
    let other_device = storage.get_device_rotation_history(user, "DEVICEBOB").await.expect("history read");
    assert!(
        other_device.is_empty(),
        "history is device-scoped — a different device must see nothing: {other_device:?}"
    );

    // ...and the same device id under another user must not either.
    let other_user = storage.get_device_rotation_history("@bob:example.com", device).await.expect("history read");
    assert!(other_user.is_empty(), "history is user-scoped too: {other_user:?}");
}

// ---------------------------------------------------------------------------
// M-05「接写者」：服务端 megolm 轮换执行器（审计 §11.16）
//
// 这些用例是"那个恒为空的端点"终于有写入者的**端到端**证据：种一条到期的服务端会话 →
// 跑一轮扫描 → 审计行能被 `get_device_rotation_history`（就是
// `GET /_matrix/vendor/v1/keys/rotation/history/{device_id}` 的读取器）读出来。
// ---------------------------------------------------------------------------

use crate::crypto::key_at_rest::KeyAtRest;
use crate::megolm::MegolmSessionStorage;
use synapse_cache::{CacheConfig, CacheManager};

/// 执行器在审计日志里用的"服务器即设备"身份 —— 生产由 `server.megolm_device_id` 提供。
const DEVICE: &str = "SERVER_MEGOLM";

/// 构造一条**服务端自有**会话（`sender_key` = 归属 user id，与 `rotate_megolm_session` 同约定）。
fn make_session(room: &str, owner: &str, index: i64, idle_days: i64, expires_in_days: i64) -> MegolmSession {
    let now = Utc::now();
    MegolmSession {
        id: uuid::Uuid::new_v4(),
        session_id: uuid::Uuid::new_v4().to_string(),
        room_id: room.to_string(),
        sender_key: owner.to_string(),
        session_key: "pickle-of-a-server-side-session".to_string(),
        algorithm: "m.megolm.v1.aes-sha2".to_string(),
        message_index: index,
        created_ts: now - chrono::Duration::days(idle_days),
        last_used_ts: now - chrono::Duration::days(idle_days),
        expires_at: Some(now + chrono::Duration::days(expires_in_days)),
    }
}

/// 隔离 schema 上的一套执行器依赖。`_isolated` 声明在**最后** ⇒ 最后析构：
/// 它 Drop 时会拆掉 schema，所以必须活到其余字段（持有 pool clone）之后。
struct Harness {
    service: KeyRotationService,
    storage: KeyRotationStorage,
    sessions: MegolmSessionStorage,
    _isolated: IsolatedTestPool,
}

async fn harness(config: KeyRotationConfig) -> Harness {
    let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
    let pool = isolated.pool();
    let at_rest = KeyAtRest::new([0x11; 32]);
    let sessions = MegolmSessionStorage::new(&pool, at_rest.clone());
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let megolm = Arc::new(MegolmProvider::from_env(sessions.clone(), cache, at_rest));
    let storage = KeyRotationStorage::new(pool);
    let service = KeyRotationService::new(megolm, Arc::new(storage.clone()), config);
    Harness { service, storage, sessions, _isolated: isolated }
}

#[tokio::test]
async fn sweep_rotates_a_due_session_and_the_device_scoped_reader_sees_the_audit_row() {
    let h = harness(KeyRotationConfig::default()).await;
    let room = "!rot1:example.com";
    let owner = "@bridge:example.com";
    // 默认 `megolm_rotation_messages` = 100，所以 index == 100 就是"该轮换了"。
    h.sessions.create_session(&make_session(room, owner, 100, 0, 7)).await.expect("seed session");

    let sweep = h.service.sweep_rotations(DEVICE).await.expect("sweep");
    assert_eq!(sweep, RotationSweep { candidates: 1, rotated: 1, failed: 0 });

    // 审计行必须能被那个"曾经恒为空"的读者读出来（端到端）。
    let history = h.storage.get_device_rotation_history(owner, DEVICE).await.expect("history read");
    assert_eq!(history.len(), 1, "a sweep must write exactly one key_rotation_log row: {history:?}");
    assert!(history[0].0.is_some(), "the audit row carries `new_key_id`: {history:?}");
    assert!(history[0].1.is_some_and(|ts| ts > 0), "…and a real millisecond stamp: {history:?}");

    // 记的是**这台**设备，不是别的。
    let other_device = h.storage.get_device_rotation_history(owner, "OTHER_DEVICE").await.expect("history read");
    assert!(other_device.is_empty(), "the audit row is device-scoped: {other_device:?}");

    // 轮换状态确实被翻转（`mark_session_as_rotated`）。
    let needs_rotation = h.storage.check_needs_rotation(owner, room).await.expect("check_needs_rotation");
    assert!(!needs_rotation, "({owner}, {room}) must be marked rotated after the sweep");
}

/// **防无界轮换（本轮最关键的一条守卫）。**
///
/// 轮换只**新增**一行、**不删**被取代的旧行（旧行要留着解历史消息），而旧行永远满足
/// "超出消息上限"。若候选查询不按 `(room_id, sender_key)` 只看**最新**那一行，第二次扫描
/// 会再次判定该房间到期 ⇒ 每轮都轮换一次 ⇒ 会话行随运行时间无界增长。
#[tokio::test]
async fn a_rotated_room_is_not_rotated_again_on_the_next_sweep() {
    let h = harness(KeyRotationConfig::default()).await;
    let room = "!rot2:example.com";
    let owner = "@bridge2:example.com";
    h.sessions.create_session(&make_session(room, owner, 100, 0, 7)).await.expect("seed session");

    assert_eq!(h.service.sweep_rotations(DEVICE).await.expect("first sweep").rotated, 1);

    let second = h.service.sweep_rotations(DEVICE).await.expect("second sweep");
    assert_eq!(
        second,
        RotationSweep::default(),
        "the superseded row must not keep the room due, or every sweep would rotate again"
    );
}

#[tokio::test]
async fn sweep_leaves_a_fresh_session_alone() {
    let h = harness(KeyRotationConfig::default()).await;
    let owner = "@bridge3:example.com";
    h.sessions.create_session(&make_session("!rot3:example.com", owner, 0, 0, 30)).await.expect("seed session");

    assert_eq!(h.service.sweep_rotations(DEVICE).await.expect("sweep"), RotationSweep::default());
    assert!(
        h.storage.get_device_rotation_history(owner, DEVICE).await.expect("history read").is_empty(),
        "a fresh session must not be rotated (and therefore must not be logged)"
    );
}

/// `enable_auto_rotation == false` 是本轮之前**从未被读过**的旋钮；它是执行器的总闸。
#[tokio::test]
async fn sweep_is_a_no_op_when_auto_rotation_is_disabled() {
    let config = KeyRotationConfig { enable_auto_rotation: false, ..Default::default() };
    let h = harness(config).await;
    let room = "!rot4:example.com";
    let owner = "@bridge4:example.com";
    h.sessions.create_session(&make_session(room, owner, 100, 0, 7)).await.expect("seed session");

    assert_eq!(h.service.sweep_rotations(DEVICE).await.expect("sweep"), RotationSweep::default());
    assert!(
        h.storage.get_device_rotation_history(owner, DEVICE).await.expect("history read").is_empty(),
        "`enable_auto_rotation = false` must stop the executor before it writes anything"
    );
}

/// **一致性守卫**：SQL 粗筛（`list_rotatable_sessions`）与权威判定（`should_rotate`）必须逐例一致。
///
/// 这是"同一策略、两个站点"的守则 —— 两处分叉只会表现为"执行器轮换的"与"`should_rotate`
/// 认为该轮换的"不是同一批，而**任何一侧都不会报错**。所以用一张用例表把两者钉在一起。
#[tokio::test]
async fn the_sql_candidate_set_agrees_with_should_rotate_case_by_case() {
    let config = KeyRotationConfig::default(); // 7 天 / 100 条
                                               // (message_index, 空闲天数, 距过期天数, 期望是否该轮换)
    let cases: [(i64, i64, i64, bool); 6] = [
        (0, 0, 30, false),  // 新鲜
        (99, 0, 30, false), // 恰好差 1 条
        (100, 0, 30, true), // 到达消息上限
        (0, 6, 30, false),  // 空闲 6 天 < 7
        (0, 8, 30, true),   // 空闲 8 天 >= 7
        (0, 0, -1, true),   // 已经过期
    ];

    let h = harness(config.clone()).await;
    let mut seeded = Vec::new();
    for (i, (index, idle, expires_in, _)) in cases.iter().enumerate() {
        let room = format!("!agree{i}:example.com");
        let owner = format!("@agree{i}:example.com");
        let session = make_session(&room, &owner, *index, *idle, *expires_in);
        h.sessions.create_session(&session).await.expect("seed session");
        seeded.push((session, room, owner));
    }

    let now_ms = current_timestamp_millis();
    let listed = h
        .storage
        .list_rotatable_sessions(
            config.megolm_rotation_messages,
            config.olm_rotation_days.saturating_mul(MS_PER_DAY),
            now_ms,
        )
        .await
        .expect("list_rotatable_sessions");
    let candidates: std::collections::HashSet<(String, String)> =
        listed.iter().map(|c| (c.room_id.clone(), c.sender_key.clone())).collect();

    let expected_due = cases.iter().filter(|case| case.3).count();
    assert_eq!(candidates.len(), expected_due, "候选集合大小与用例表的期望不一致：{listed:?}");

    for (i, (session, room, owner)) in seeded.iter().enumerate() {
        let expected = cases[i].3;
        let by_rule = h.service.should_rotate(session).await.expect("should_rotate");
        let by_sql = candidates.contains(&(room.clone(), owner.clone()));
        assert_eq!(by_rule, expected, "`should_rotate` disagrees with the case table for case {i}: {session:?}");
        assert_eq!(
            by_sql, expected,
            "`list_rotatable_sessions` disagrees with the case table for case {i}: {listed:?}"
        );
    }
}
