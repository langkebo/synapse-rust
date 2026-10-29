// MSC4354 联邦补发游标（UG-7 catch-up）的存储层用例。
//
// 这里只钉住"存储层自己的契约"：游标种子幂等、只补本机 owner 的行、墓碑也补、
// 推进单调、重置即全量。真正的"发出去了才推进"在 `RoomService` 里，见
// `synapse-services/src/room/sticky_catch_up.rs`。

use super::db_tests::{ensure_test_room, ensure_test_user, test_pool};
use super::*;

/// Seed a `join` membership so the room has a member on `user_id`'s origin.
async fn join_member(pool: &PgPool, room_id: &str, user_id: &str) {
    sqlx::query(
        r#"INSERT INTO room_memberships (room_id, user_id, membership, joined_ts)
           VALUES ($1, $2, 'join', $3)
           ON CONFLICT (room_id, user_id) DO UPDATE SET membership = 'join'"#,
    )
    .bind(room_id)
    .bind(user_id)
    .bind(current_timestamp_millis())
    .execute(pool)
    .await
    .expect("failed to seed membership");
}

/// The three things every case needs: a room, a local pin, and a remote peer.
struct Fixture {
    room_id: String,
    local_user: String,
    remote_user: String,
}

async fn fixture(pool: &PgPool, storage: &StickyEventStorage, tag: &str) -> Fixture {
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_{tag}_{suffix}:test.com");
    // `users.username` 是唯一的，所以 localpart 也带上 uuid。
    let local_user = format!("@local_{tag}_{suffix}:test.com");
    let remote_user = format!("@remote_{tag}_{suffix}:remote.example");
    ensure_test_room(pool, &room_id).await;
    ensure_test_user(pool, &local_user).await;
    ensure_test_user(pool, &remote_user).await;
    join_member(pool, &room_id, &remote_user).await;
    storage
        .set_is_sticky_event(
            &room_id,
            &local_user,
            &format!("$ev_{tag}_{suffix}:test.com"),
            "m.room.message",
            true,
            None,
        )
        .await
        .expect("seed local pin");
    Fixture { room_id, local_user, remote_user }
}

#[tokio::test]
async fn test_seed_destination_cursors_is_idempotent_and_skips_self() {
    let (_isolated, pool) = test_pool().await;
    let storage = StickyEventStorage::new(pool.clone());
    let f = fixture(&pool, &storage, "seed").await;
    // 另一 origin 的房级行：它不是本机该补的内容。
    storage
        .set_is_sticky_event(&f.room_id, "other.example", "$ev_other:other.example", "m.room.topic", true, None)
        .await
        .expect("seed room-level pin");

    let created = storage.seed_destination_cursors("test.com").await.expect("seed should succeed");
    assert_eq!(created, 1, "只应为远端 peer 建一条游标");

    let again = storage.seed_destination_cursors("test.com").await.expect("seed should be re-runnable");
    assert_eq!(again, 0, "游标已存在时不得重复创建（ON CONFLICT DO NOTHING）");

    // 本机用户自己也加进房间：不得为本地服务器建游标（自己不发给自己）。
    join_member(&pool, &f.room_id, &f.local_user).await;
    let after_self_join = storage.seed_destination_cursors("test.com").await.expect("seed should succeed");
    assert_eq!(after_self_join, 0, "本机服务器不得成为补发目标");

    let pending = storage.pending_sticky_destinations("test.com", 100).await.expect("pending should succeed");
    let mine: Vec<_> = pending.iter().filter(|p| p.room_id == f.room_id).collect();
    assert_eq!(mine.len(), 1, "只应有 remote.example 一条待补发");
    assert_eq!(mine[0].destination, f.remote_user.split(':').next_back().unwrap());
    assert_eq!(mine[0].last_revision, 0, "新游标必须从全量补发开始");

    let rows = storage.pending_sticky_rows(&f.room_id, "test.com", 0, 10).await.expect("rows should succeed");
    assert_eq!(rows.len(), 1, "只补本机 owner 的行，另一 origin 的房级行不归本机发");
    assert_eq!(rows[0].event_type, "m.room.message");
    assert!(rows[0].is_sticky);
}

#[tokio::test]
async fn test_pending_rows_include_tombstones_and_cursor_only_moves_forward() {
    let (_isolated, pool) = test_pool().await;
    let storage = StickyEventStorage::new(pool.clone());
    let f = fixture(&pool, &storage, "tomb").await;
    storage.seed_destination_cursors("test.com").await.expect("seed should succeed");
    let destination = f.remote_user.split(':').next_back().unwrap().to_string();

    storage.clear_is_sticky_event(&f.room_id, &f.local_user, "m.room.message").await.expect("clear should succeed");

    let rows = storage.pending_sticky_rows(&f.room_id, "test.com", 0, 10).await.expect("rows should succeed");
    assert_eq!(rows.len(), 1, "唯一键决定同一 (user, type) 只留最新状态，不是逐次追加的历史");
    assert!(!rows[0].is_sticky, "取消也必须补：否则远端会永远留着这条置顶");
    assert!(rows[0].revision > 0);

    storage
        .advance_destination_cursor(&f.room_id, &destination, rows[0].revision)
        .await
        .expect("advance should succeed");
    let pending = storage.pending_sticky_destinations("test.com", 100).await.expect("pending should succeed");
    assert!(pending.iter().all(|p| p.room_id != f.room_id), "补齐后不应再出现在待补发列表");

    // 并发/stale 写入不得把游标拉回去。
    storage
        .advance_destination_cursor(&f.room_id, &destination, rows[0].revision - 1)
        .await
        .expect("advance should succeed");
    let pending = storage.pending_sticky_destinations("test.com", 100).await.expect("pending should succeed");
    assert!(pending.iter().all(|p| p.room_id != f.room_id), "GREATEST 必须挡住回拨");
}

#[tokio::test]
async fn test_reset_destination_cursor_forces_full_replay() {
    let (_isolated, pool) = test_pool().await;
    let storage = StickyEventStorage::new(pool.clone());
    let f = fixture(&pool, &storage, "reset").await;
    storage.seed_destination_cursors("test.com").await.expect("seed should succeed");
    let destination = f.remote_user.split(':').next_back().unwrap().to_string();

    let rows = storage.pending_sticky_rows(&f.room_id, "test.com", 0, 10).await.expect("rows should succeed");
    storage
        .advance_destination_cursor(&f.room_id, &destination, rows[0].revision)
        .await
        .expect("advance should succeed");

    // 新成员入房（#20233）：游标归零 ⇒ 下一轮全量补发。
    storage.reset_destination_cursor(&f.room_id, &destination).await.expect("reset should succeed");

    let pending = storage.pending_sticky_destinations("test.com", 100).await.expect("pending should succeed");
    let mine: Vec<_> = pending.iter().filter(|p| p.room_id == f.room_id).collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].last_revision, 0, "重置后必须是全量补发");

    let replay = storage.pending_sticky_rows(&f.room_id, "test.com", 0, 10).await.expect("rows should succeed");
    assert_eq!(replay.len(), 1, "重置后重放房间已有的置顶");
}

#[tokio::test]
async fn test_departed_member_is_not_a_catch_up_target() {
    let (_isolated, pool) = test_pool().await;
    let storage = StickyEventStorage::new(pool.clone());
    let f = fixture(&pool, &storage, "left").await;
    storage.seed_destination_cursors("test.com").await.expect("seed should succeed");

    sqlx::query("UPDATE room_memberships SET membership = 'leave' WHERE room_id = $1 AND user_id = $2")
        .bind(&f.room_id)
        .bind(&f.remote_user)
        .execute(pool.as_ref())
        .await
        .expect("failed to flip membership");

    let pending = storage.pending_sticky_destinations("test.com", 100).await.expect("pending should succeed");
    assert!(pending.iter().all(|p| p.room_id != f.room_id), "已离开的服务器不得持有补发积压");
}
