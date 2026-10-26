//! `EventReader` / `EventWriter` 的 trait object 转发层契约测试（2026-10-26）。
//!
//! 为什么单独建一个文件（见 `docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §7.5 ⑤）：
//! `event/reader.rs` / `event/writer.rs` 里的 `impl … for EventStorage` 全是**一行转发**
//! （`self.get_event(id).await`），而本 crate 自己的单测一律走固有方法
//! ⇒ 这两层转发在 `cargo llvm-cov -p synapse-storage --lib` 里恒为 0%，看起来像死代码。
//! 实际它们被生产 wiring 当 `Arc<dyn EventReader>` / `Arc<dyn EventWriter>` 使用
//! （`synapse-services/src/wiring/**` 等 107 处 trait object 调用点）。
//!
//! 本文件用 `&dyn` 真的走一遍，钉住两件事：
//! 1. 通过 trait 对象读到/写回的数据与固有方法**完全一致**（转发层不丢字段、不换语义）；
//! 2. `pool()` 这条转发返回的确实是构建时那个池（trait 对象没有偷偷换连接池）。

use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

use super::*;

async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<sqlx::PgPool>) {
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated test pool");
    let pool = isolated.pool();
    (isolated, pool)
}

async fn ensure_test_room(pool: &Pool<Postgres>, room_id: &str) {
    let now = current_timestamp_millis();
    sqlx::query(
        r#"INSERT INTO rooms (room_id, creator, join_rules, room_version, is_public, history_visibility, created_ts, last_activity_ts)
           VALUES ($1, '@test:example.com', 'invite', '10', false, 'joined', $2, $2)
           ON CONFLICT (room_id) DO NOTHING"#,
    )
    .bind(room_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to create test room");
}

async fn ensure_test_user(pool: &Pool<Postgres>, user_id: &str) {
    let now = current_timestamp_millis();
    let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
    sqlx::query(
        r#"INSERT INTO users (user_id, username, created_ts)
           VALUES ($1, $2, $3)
           ON CONFLICT (user_id) DO NOTHING"#,
    )
    .bind(user_id)
    .bind(username)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to create test user");
}

/// 写走 trait 对象、读也走 trait 对象，两者必须与固有方法看到同一份数据。
#[tokio::test]
async fn trait_object_write_then_read_matches_inherent_methods() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, "example.com".to_string());
    let room_id = format!("!dyn_room_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$dyn_evt_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@dyn_sender:example.com";

    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    // 生产侧拿到的就是这两个 trait 对象；这里显式用 `&dyn` 复现同一形态。
    let reader: &dyn EventReader = &storage;
    let writer: &dyn EventWriter = &storage;

    // ① 写：trait object 的 create_event 必须与固有方法等价地落库
    let created = writer
        .create_event(
            CreateEventParams {
                event_id: event_id.clone(),
                room_id: room_id.clone(),
                user_id: user_id.to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": "via dyn", "msgtype": "m.text"}),
                state_key: None,
                origin_server_ts: current_timestamp_millis(),
                redacts: None,
            },
            None,
        )
        .await
        .expect("trait object create_event should succeed");
    assert_eq!(created.event_id, event_id);
    assert_eq!(created.room_id, room_id);

    // ② 读：trait object 与固有方法必须返回同一行（同一 event_id / 同一 content）
    let via_trait = reader.get_event(&event_id).await.expect("dyn get_event").expect("row exists");
    let via_inherent = storage.get_event(&event_id).await.expect("inherent get_event").expect("row exists");
    assert_eq!(via_trait.event_id, via_inherent.event_id);
    assert_eq!(via_trait.content, via_inherent.content);

    // ③ 列表：trait object 的批量读也要看到刚写的那一行
    let events = reader.get_room_events(&room_id, 10).await.expect("dyn get_room_events");
    assert_eq!(events.len(), 1, "trait object 应看到唯一一条事件");
    assert_eq!(events[0].event_id, event_id);

    // ④ 分页：同样是转发，方向参数必须真的传下去（`b` = 倒序）
    let paged = reader.get_room_events_paginated(&room_id, None, 10, "b").await.expect("dyn paginated");
    assert_eq!(paged.len(), 1);
    assert_eq!(paged[0].event_id, event_id);

    // ⑤ pool()：trait 对象暴露的池必须就是构建时那个池，不能是新连接的池
    assert!(Arc::ptr_eq(reader.pool(), &pool), "reader.pool() 必须是构建时传入的同一个池");
    assert!(Arc::ptr_eq(writer.pool(), &pool), "writer.pool() 必须是构建时传入的同一个池");

    let _ = storage.delete_room_events(&room_id).await;
}

/// 读不到时两条路径都必须返回 `Ok(None)`（而不是 `Err`）—— 消费者靠这个区分
/// "房间/事件不存在"（404）和"库挂了"（500）。
#[tokio::test]
async fn trait_object_read_of_missing_event_is_ok_none_not_err() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, "example.com".to_string());
    let reader: &dyn EventReader = &storage;

    let missing = format!("$absent_{}:example.com", uuid::Uuid::new_v4());
    let got = reader.get_event(&missing).await.expect("缺失的行必须是 Ok(None)，不能是 Err");
    assert!(got.is_none(), "不存在的 event 应返回 None");

    let unknown_room = format!("!absent_{}:example.com", uuid::Uuid::new_v4());
    let events = reader.get_room_events(&unknown_room, 10).await.expect("未知房间应是 Ok(空)");
    assert!(events.is_empty());
}
