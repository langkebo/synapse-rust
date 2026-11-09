use super::*;
use serde_json::json;
use std::sync::Arc;

/// 每个测试一个从迁移 baseline 克隆出来的独立 schema（返回 guard 与 pool）。
///
/// 2026-09-21：原先用共享 `public` 池。共享池的两个问题：测试结果取决于环境里
/// `public` 的残渣（本地 `public` 落后于迁移 baseline 时会直接 42P01），且并行测试
/// 互相影响。按铁律 7 消除状态共享：per-test schema 由模板克隆，表一定存在、行数从 0 开始。
async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<sqlx::PgPool>) {
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated test pool");
    let pool = isolated.pool();
    (isolated, pool)
}

fn unique_user_id(prefix: &str) -> String {
    format!("{}_pushtest_{}:test.com", prefix, uuid::Uuid::new_v4())
}

// ── helpers ─────────────────────────────────────────────────────────

async fn cleanup_pushers(pool: &sqlx::PgPool, user_id: &str) {
    let _ = sqlx::query("DELETE FROM pushers WHERE user_id = $1").bind(user_id).execute(pool).await;
}

async fn cleanup_push_rules(pool: &sqlx::PgPool, user_id: &str) {
    let _ = sqlx::query("DELETE FROM push_rules WHERE user_id = $1").bind(user_id).execute(pool).await;
}

async fn cleanup_notifications(pool: &sqlx::PgPool, user_id: &str) {
    let _ = sqlx::query("DELETE FROM notifications WHERE user_id = $1").bind(user_id).execute(pool).await;
}

// reason: `insert_notification` 是协议/存储字段的直传签名：参数逐个来自调用点，打包成结构体只会把同一份字段列表抄两遍
#[allow(clippy::too_many_arguments)]
async fn insert_notification(
    pool: &sqlx::PgPool,
    user_id: &str,
    event_id: &str,
    room_id: &str,
    ts: i64,
    notification_type: &str,
    is_read: bool,
    created_ts: i64,
) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO notifications (user_id, event_id, room_id, ts, notification_type, is_read, created_ts) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(user_id)
    .bind(event_id)
    .bind(room_id)
    .bind(ts)
    .bind(notification_type)
    .bind(is_read)
    .bind(created_ts)
    .fetch_one(pool)
    .await
    .expect("failed to insert test notification")
}

// ── pushers tests ────────────────────────────────────────────────────

#[tokio::test]
async fn test_upsert_and_get_pushers_with_device_id() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let device_id = "device1";
    let pushkey = "pushkey1";
    let now = current_timestamp_millis();

    cleanup_pushers(&pool, &user_id).await;

    storage
        .upsert_pusher(&user_id, device_id, pushkey, "http", "app.id", "My App", "My Device", &None, "en", &None, now)
        .await
        .expect("upsert_pusher should succeed");

    let rows = storage.get_pushers(&user_id, Some(device_id)).await.expect("get_pushers should succeed");
    assert!(!rows.is_empty(), "should return at least one pusher");

    let row = &rows[0];
    assert_eq!(row.pushkey, "pushkey1");
    assert_eq!(row.device_id, device_id);

    cleanup_pushers(&pool, &user_id).await;
}

#[tokio::test]
async fn test_get_pushers_filters_by_specific_device_id() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");

    cleanup_pushers(&pool, &user_id).await;

    let now = current_timestamp_millis();
    storage
        .upsert_pusher(&user_id, "d1", "pk1", "http", "app.id", "App", "Dev", &None, "en", &None, now)
        .await
        .expect("upsert d1 should succeed");
    storage
        .upsert_pusher(&user_id, "d2", "pk2", "http", "app.id", "App", "Dev", &None, "en", &None, now)
        .await
        .expect("upsert d2 should succeed");

    // Filtering by a specific device_id should return only that pusher
    let rows_d1 = storage.get_pushers(&user_id, Some("d1")).await.expect("get_pushers with Some(d1) should succeed");
    assert_eq!(rows_d1.len(), 1, "should return exactly one pusher for d1");
    assert_eq!(rows_d1[0].device_id, "d1");

    let rows_d2 = storage.get_pushers(&user_id, Some("d2")).await.expect("get_pushers with Some(d2) should succeed");
    assert_eq!(rows_d2.len(), 1, "should return exactly one pusher for d2");
    assert_eq!(rows_d2[0].device_id, "d2");

    cleanup_pushers(&pool, &user_id).await;
}

#[tokio::test]
async fn test_get_pushers_with_none_device_id() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");

    cleanup_pushers(&pool, &user_id).await;

    let now = current_timestamp_millis();
    storage
        .upsert_pusher(&user_id, "d1", "pk1", "http", "app.id", "App", "Dev", &None, "en", &None, now)
        .await
        .expect("upsert should succeed");

    // When device_id is None, it binds as SQL NULL, and
    // `device_id IS NOT DISTINCT FROM NULL` only matches rows
    // where device_id IS NULL. Since the column is NOT NULL,
    // this currently returns zero rows.
    let rows = storage.get_pushers(&user_id, None).await.expect("get_pushers with None should succeed");
    assert!(rows.is_empty(), "None device_id currently returns empty result set");

    cleanup_pushers(&pool, &user_id).await;
}

#[tokio::test]
async fn test_get_pushers_returns_empty_for_unknown_user() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@unknown");

    let rows = storage.get_pushers(&user_id, None).await.expect("get_pushers should succeed");
    assert!(rows.is_empty(), "unknown user should have no pushers");
}

#[tokio::test]
async fn test_upsert_pusher_updates_existing() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let device_id = "dev_update";
    let pushkey = "pk_update";
    let now = current_timestamp_millis();

    cleanup_pushers(&pool, &user_id).await;

    // Insert initial
    storage
        .upsert_pusher(&user_id, device_id, pushkey, "http", "app.id", "App1", "Dev1", &None, "en", &None, now)
        .await
        .expect("first upsert should succeed");

    // Update with new app_display_name
    let now2 = now + 1000;
    storage
        .upsert_pusher(&user_id, device_id, pushkey, "http", "app.id", "App2", "Dev2", &None, "en", &None, now2)
        .await
        .expect("second upsert should succeed");

    let rows = storage.get_pushers(&user_id, Some(device_id)).await.expect("get_pushers should succeed");
    assert_eq!(rows.len(), 1, "should still have exactly one pusher after upsert");
    assert_eq!(rows[0].app_display_name, "App2");

    cleanup_pushers(&pool, &user_id).await;
}

#[tokio::test]
async fn test_delete_pusher() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let device_id = "dev_del";
    let pushkey = "pk_del";
    let now = current_timestamp_millis();

    cleanup_pushers(&pool, &user_id).await;

    storage
        .upsert_pusher(&user_id, device_id, pushkey, "http", "app.id", "App", "Dev", &None, "en", &None, now)
        .await
        .expect("upsert should succeed");

    storage.delete_pusher(&user_id, device_id, pushkey).await.expect("delete_pusher should succeed");

    let rows = storage.get_pushers(&user_id, Some(device_id)).await.expect("get_pushers should succeed");
    assert!(rows.is_empty(), "pusher should be deleted");

    cleanup_pushers(&pool, &user_id).await;
}

#[tokio::test]
async fn test_delete_pusher_nonexistent_does_not_error() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");

    storage
        .delete_pusher(&user_id, "no_dev", "no_pk")
        .await
        .expect("delete_pusher on non-existent row should not error");
}

// ── push_rules tests ─────────────────────────────────────────────────

#[tokio::test]
async fn test_upsert_and_get_push_rule() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let now = current_timestamp_millis();

    cleanup_push_rules(&pool, &user_id).await;

    storage
        .upsert_push_rule(&user_id, "global", "override", "rule1", &None, &None, &json!(["notify"]), None, None, now)
        .await
        .expect("upsert_push_rule should succeed");

    let rows =
        storage.get_user_push_rules(&user_id, "global", "override").await.expect("get_user_push_rules should succeed");
    assert!(!rows.is_empty(), "should return at least one rule");

    let enabled = storage
        .get_push_rule_enabled(&user_id, "global", "override", "rule1")
        .await
        .expect("get_push_rule_enabled should succeed");
    assert_eq!(enabled, Some(true), "new rule should be enabled by default");

    cleanup_push_rules(&pool, &user_id).await;
}

#[tokio::test]
async fn test_get_user_push_rules_returns_empty_for_no_rules() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");

    let rows =
        storage.get_user_push_rules(&user_id, "global", "override").await.expect("get_user_push_rules should succeed");
    assert!(rows.is_empty(), "unknown user should have no push rules");
}

#[tokio::test]
async fn test_upsert_push_rule_updates_existing() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let now = current_timestamp_millis();

    cleanup_push_rules(&pool, &user_id).await;

    storage
        .upsert_push_rule(
            &user_id,
            "global",
            "override",
            "rule2",
            &Some("original".to_string()),
            &None,
            &json!(["notify"]),
            None,
            None,
            now,
        )
        .await
        .expect("first upsert should succeed");

    storage
        .upsert_push_rule(
            &user_id,
            "global",
            "override",
            "rule2",
            &Some("updated".to_string()),
            &None,
            &json!(["dont_notify"]),
            None,
            None,
            now,
        )
        .await
        .expect("second upsert should succeed");

    let rows =
        storage.get_user_push_rules(&user_id, "global", "override").await.expect("get_user_push_rules should succeed");
    assert_eq!(rows.len(), 1, "should still have exactly one rule after upsert");
    assert_eq!(rows[0].pattern.as_deref(), Some("updated"));

    cleanup_push_rules(&pool, &user_id).await;
}

#[tokio::test]
async fn test_update_push_rule_actions() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let now = current_timestamp_millis();

    cleanup_push_rules(&pool, &user_id).await;

    storage
        .upsert_push_rule(
            &user_id,
            "global",
            "override",
            "rule_actions",
            &None,
            &None,
            &json!(["notify"]),
            None,
            None,
            now,
        )
        .await
        .expect("upsert should succeed");

    storage
        .update_push_rule_actions(
            &user_id,
            "global",
            "override",
            "rule_actions",
            &json!(["dont_notify", {"set_tweak": "highlight"}]),
        )
        .await
        .expect("update_push_rule_actions should succeed");

    let rows =
        storage.get_user_push_rules(&user_id, "global", "override").await.expect("get_user_push_rules should succeed");
    assert!(!rows.is_empty());
    let actions = rows[0].actions.as_ref().expect("actions are stored as JSONB");
    assert!(actions.as_array().is_some_and(|a| a.len() >= 2));

    cleanup_push_rules(&pool, &user_id).await;
}

#[tokio::test]
async fn test_delete_push_rule() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let now = current_timestamp_millis();

    cleanup_push_rules(&pool, &user_id).await;

    storage
        .upsert_push_rule(&user_id, "global", "override", "rule_del", &None, &None, &json!(["notify"]), None, None, now)
        .await
        .expect("upsert should succeed");

    let affected = storage
        .delete_push_rule(&user_id, "global", "override", "rule_del")
        .await
        .expect("delete_push_rule should succeed");
    assert_eq!(affected, 1, "should delete exactly one row");

    let affected2 = storage
        .delete_push_rule(&user_id, "global", "override", "rule_del")
        .await
        .expect("second delete should succeed");
    assert_eq!(affected2, 0, "second delete should affect zero rows");

    cleanup_push_rules(&pool, &user_id).await;
}

#[tokio::test]
async fn test_get_push_rule_enabled_returns_none_for_nonexistent() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");

    let enabled = storage
        .get_push_rule_enabled(&user_id, "global", "override", "no_such_rule")
        .await
        .expect("get_push_rule_enabled should succeed");
    assert_eq!(enabled, None, "non-existent rule should return None");
}

#[tokio::test]
async fn test_set_push_rule_enabled() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let now = current_timestamp_millis();

    cleanup_push_rules(&pool, &user_id).await;

    storage
        .upsert_push_rule(
            &user_id,
            "global",
            "override",
            "rule_toggle",
            &None,
            &None,
            &json!(["notify"]),
            None,
            None,
            now,
        )
        .await
        .expect("upsert should succeed");

    storage
        .set_push_rule_enabled(&user_id, "global", "override", "rule_toggle", false)
        .await
        .expect("set_push_rule_enabled should succeed");

    let enabled = storage
        .get_push_rule_enabled(&user_id, "global", "override", "rule_toggle")
        .await
        .expect("get_push_rule_enabled should succeed");
    assert_eq!(enabled, Some(false), "rule should be disabled");

    storage
        .set_push_rule_enabled(&user_id, "global", "override", "rule_toggle", true)
        .await
        .expect("re-enable should succeed");

    let enabled = storage
        .get_push_rule_enabled(&user_id, "global", "override", "rule_toggle")
        .await
        .expect("get_push_rule_enabled should succeed");
    assert_eq!(enabled, Some(true), "rule should be re-enabled");

    cleanup_push_rules(&pool, &user_id).await;
}

#[tokio::test]
async fn test_get_user_push_rules_scoped_by_scope_and_kind() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let now = current_timestamp_millis();

    cleanup_push_rules(&pool, &user_id).await;

    // Create rules in different scope/kind combinations
    storage
        .upsert_push_rule(&user_id, "global", "override", "r1", &None, &None, &json!(["notify"]), None, None, now)
        .await
        .expect("upsert r1");
    storage
        .upsert_push_rule(&user_id, "global", "content", "r2", &None, &None, &json!(["notify"]), None, None, now)
        .await
        .expect("upsert r2");
    storage
        .upsert_push_rule(&user_id, "devices/DEV1", "override", "r3", &None, &None, &json!(["notify"]), None, None, now)
        .await
        .expect("upsert r3");

    let global_override =
        storage.get_user_push_rules(&user_id, "global", "override").await.expect("get global override");
    assert_eq!(global_override.len(), 1, "should return only global/override rules");
    assert_eq!(global_override[0].rule_id, "r1");

    let global_content = storage.get_user_push_rules(&user_id, "global", "content").await.expect("get global content");
    assert_eq!(global_content.len(), 1);
    assert_eq!(global_content[0].rule_id, "r2");

    let device_override =
        storage.get_user_push_rules(&user_id, "devices/DEV1", "override").await.expect("get device override");
    assert_eq!(device_override.len(), 1);
    assert_eq!(device_override[0].rule_id, "r3");

    cleanup_push_rules(&pool, &user_id).await;
}

// ── notifications tests ──────────────────────────────────────────────

#[tokio::test]
async fn test_get_notifications_returns_rows() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let now = current_timestamp_millis();

    cleanup_notifications(&pool, &user_id).await;

    insert_notification(&pool, &user_id, "$ev1", "!room1:test.com", now, "message", false, now).await;
    insert_notification(&pool, &user_id, "$ev2", "!room1:test.com", now + 1, "invite", false, now).await;

    let rows = storage
        .get_notifications(&user_id, &NotificationPageQuery::first(10))
        .await
        .expect("get_notifications should succeed");
    assert_eq!(rows.len(), 2, "should return both notifications");
    // Results ordered by ts DESC, so newest (ev2) first
    assert_eq!(rows[0].event_id.as_deref(), Some("$ev2"));

    cleanup_notifications(&pool, &user_id).await;
}

#[tokio::test]
async fn test_get_notifications_respects_limit() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let now = current_timestamp_millis();

    cleanup_notifications(&pool, &user_id).await;

    for i in 0..5 {
        insert_notification(&pool, &user_id, &format!("$ev{i}"), "!room1:test.com", now + i, "message", false, now)
            .await;
    }

    let rows = storage
        .get_notifications(&user_id, &NotificationPageQuery::first(3))
        .await
        .expect("get_notifications should succeed");
    assert_eq!(rows.len(), 3, "should respect limit");

    cleanup_notifications(&pool, &user_id).await;
}

#[tokio::test]
async fn test_get_notifications_empty_for_unknown_user() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@unknown");

    let rows = storage
        .get_notifications(&user_id, &NotificationPageQuery::first(10))
        .await
        .expect("get_notifications should succeed");
    assert!(rows.is_empty(), "unknown user should have no notifications");
}

/// D-68：`record_notification` 必须真的写进 `notifications`，并可从
/// `get_notifications` 读回（此前该表在生产路径上**没有任何写入者**，
/// 三个已注册端点恒为空）。
#[tokio::test]
async fn test_record_notification_inserts_a_readable_row() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let now = current_timestamp_millis();

    storage
        .record_notification(&user_id, Some("$ev_record"), Some("!room1:test.com"), "message", now)
        .await
        .expect("record_notification must succeed");

    let rows = storage.get_notifications(&user_id, &NotificationPageQuery::first(10)).await.expect("get_notifications");
    assert_eq!(rows.len(), 1, "the recorded notification must be readable");
    assert_eq!(rows[0].event_id.as_deref(), Some("$ev_record"));
    assert_eq!(rows[0].room_id.as_deref(), Some("!room1:test.com"));
    assert_eq!(rows[0].notification_type.as_deref(), Some("message"));
    assert_eq!(rows[0].is_read, Some(false), "a freshly recorded notification is unread");
    assert_eq!(rows[0].ts, now);

    cleanup_notifications(&pool, &user_id).await;
}

/// 同一 event 在被 ack 之前重复触发（同一次推送的多次尝试）**不得**累积重复行；
/// ack 之后允许重新成为一条新通知。
#[tokio::test]
async fn test_record_notification_is_idempotent_until_acked() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let now = current_timestamp_millis();

    for attempt in 0..3 {
        storage
            .record_notification(&user_id, Some("$ev_dup"), Some("!room1:test.com"), "message", now + attempt)
            .await
            .expect("record_notification must succeed");
    }
    let rows = storage.get_notifications(&user_id, &NotificationPageQuery::first(10)).await.expect("get_notifications");
    assert_eq!(rows.len(), 1, "repeated attempts for one unread event must not duplicate");

    storage.ack_notification(rows[0].id, &user_id, now + 10).await.expect("ack_notification");
    storage
        .record_notification(&user_id, Some("$ev_dup"), Some("!room1:test.com"), "message", now + 11)
        .await
        .expect("record_notification after ack must succeed");
    let rows = storage.get_notifications(&user_id, &NotificationPageQuery::first(10)).await.expect("get_notifications");
    assert_eq!(rows.len(), 2, "after ack the event may notify again");

    cleanup_notifications(&pool, &user_id).await;
}

/// `event_id` 缺失（服务端主动推送）时**不做**去重：两次通知各自成行，
/// 否则所有无 event 的推送都会被第一条吞掉。
#[tokio::test]
async fn test_record_notification_without_event_id_never_dedups() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let now = current_timestamp_millis();

    storage.record_notification(&user_id, None, None, "message", now).await.expect("record_notification must succeed");
    storage
        .record_notification(&user_id, None, None, "message", now + 1)
        .await
        .expect("record_notification must succeed");

    let rows = storage.get_notifications(&user_id, &NotificationPageQuery::first(10)).await.expect("get_notifications");
    assert_eq!(rows.len(), 2, "notifications without an event_id must never be merged");

    cleanup_notifications(&pool, &user_id).await;
}

/// D16：事件落库后必须为房间内**本地 joined** 成员各写一条 `notifications`
/// 行，且与推送投递解耦——成员没有注册 pusher 也要有收件箱记录。
///
/// 同时验证三类抑制：发送者自己、远端成员（本服务器没有其收件箱）、
/// 非 joined 成员；以及 `m.mentions.user_ids` 命中时记为 `highlight`。
#[tokio::test]
async fn test_record_room_event_notifications_targets_local_joined_members() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let suffix = uuid::Uuid::new_v4().to_string();
    let room_id = format!("!d16_{suffix}:test.com");
    let sender = format!("@d16_sender_{suffix}:test.com");
    let alice = format!("@d16_alice_{suffix}:test.com");
    let bob = format!("@d16_bob_{suffix}:test.com");
    let remote = format!("@d16_remote_{suffix}:other.com");
    let left = format!("@d16_left_{suffix}:test.com");
    let now = current_timestamp_millis();

    seed_room(&pool, &room_id, now).await;
    for user_id in [&sender, &alice, &bob, &remote, &left] {
        seed_user(&pool, user_id, now).await;
    }
    for (user_id, membership) in
        [(&sender, "join"), (&alice, "join"), (&bob, "join"), (&remote, "join"), (&left, "leave")]
    {
        seed_membership(&pool, &room_id, user_id, membership, now).await;
    }

    let inserted = storage
        .record_room_event_notifications(&room_id, "$d16_event", &sender, &[alice.clone()], false, now, "test.com", "")
        .await
        .expect("recording notifications must succeed");
    assert_eq!(inserted, 2, "only the local joined non-sender members get a row");

    let alice_rows =
        storage.get_notifications(&alice, &NotificationPageQuery::first(10)).await.expect("get_notifications");
    assert_eq!(alice_rows.len(), 1);
    assert_eq!(alice_rows[0].notification_type.as_deref(), Some("highlight"), "a mentioned user gets a highlight");

    let bob_rows = storage.get_notifications(&bob, &NotificationPageQuery::first(10)).await.expect("get_notifications");
    assert_eq!(bob_rows.len(), 1);
    assert_eq!(bob_rows[0].notification_type.as_deref(), Some("message"));

    for skipped in [&sender, &remote, &left] {
        let rows = storage.get_notifications(skipped, &NotificationPageQuery::first(10)).await.expect("get");
        assert!(rows.is_empty(), "{skipped} must not receive a notification row");
    }

    // Re-delivery (a retried federation transaction) must not duplicate an
    // unread inbox entry for the same event.
    let again = storage
        .record_room_event_notifications(&room_id, "$d16_event", &sender, &[], false, now + 1, "test.com", "")
        .await
        .expect("recording notifications must succeed");
    assert_eq!(again, 0, "an unread row for the same event is not duplicated");

    // P2：正文高亮不再只认 `m.mentions`。收件人的 displayname 出现在正文里
    // （`.m.rule.contains_display_name`，默认规则、未持久化）时记 highlight；
    // 用户自定义关键字规则（kind='content' + highlight tweak）命中时同理。
    sqlx::query("UPDATE users SET displayname = 'Alice Wonder' WHERE user_id = $1")
        .bind(&alice)
        .execute(pool.as_ref())
        .await
        .expect("seed displayname");
    sqlx::query(
        "INSERT INTO push_rules (user_id, scope, kind, rule_id, pattern, actions, is_enabled, created_ts) \
         VALUES ($1, 'global', 'content', 'k8s', 'kubernetes', '[{\"set_tweak\":\"highlight\"}]'::jsonb, TRUE, $2)",
    )
    .bind(&bob)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .expect("seed keyword highlight rule");

    let body_inserted = storage
        .record_room_event_notifications(
            &room_id,
            "$d16_event_body",
            &sender,
            &[],
            false,
            now,
            "test.com",
            "let us discuss Kubernetes, Alice Wonder",
        )
        .await
        .expect("recording notifications must succeed");
    assert_eq!(body_inserted, 2);

    let alice_body_rows =
        storage.get_notifications(&alice, &NotificationPageQuery::first(10)).await.expect("get_notifications");
    assert!(
        alice_body_rows.iter().any(|row| row.event_id.as_deref() == Some("$d16_event_body")
            && row.notification_type.as_deref() == Some("highlight")),
        "displayname in the body must highlight"
    );
    let bob_body_rows =
        storage.get_notifications(&bob, &NotificationPageQuery::first(10)).await.expect("get_notifications");
    assert!(
        bob_body_rows.iter().any(|row| row.event_id.as_deref() == Some("$d16_event_body")
            && row.notification_type.as_deref() == Some("highlight")),
        "a keyword content rule must highlight"
    );

    // P1：房间级静音规则在本服务器存为 `kind='room'` + `pattern=<room_id>`，
    // rule_id 由客户端自取。抑制判定必须按 pattern 命中，不能只看 rule_id。
    sqlx::query(
        "INSERT INTO push_rules (user_id, scope, kind, rule_id, pattern, actions, is_enabled, created_ts) \
         VALUES ($1, 'global', 'room', 'mute_this_room', $2, '[\"dont_notify\"]'::jsonb, TRUE, $3)",
    )
    .bind(&bob)
    .bind(&room_id)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .expect("seed room mute rule");

    let muted = storage
        .record_room_event_notifications(&room_id, "$d16_event_muted", &sender, &[], false, now, "test.com", "")
        .await
        .expect("recording notifications must succeed");
    assert_eq!(muted, 1, "the muted member is suppressed; the un-muted member still gets a row");
}

async fn seed_room(pool: &sqlx::PgPool, room_id: &str, now: i64) {
    sqlx::query(
        "INSERT INTO rooms (room_id, room_version, is_public, creator, created_ts) \
         VALUES ($1, '12', false, '@d16_creator:test.com', $2) ON CONFLICT (room_id) DO NOTHING",
    )
    .bind(room_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("seed room");
}

async fn seed_user(pool: &sqlx::PgPool, user_id: &str, now: i64) {
    sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $1, $2) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("seed user");
}

async fn seed_membership(pool: &sqlx::PgPool, room_id: &str, user_id: &str, membership: &str, now: i64) {
    sqlx::query(
        "INSERT INTO room_memberships (room_id, user_id, membership, joined_ts) \
         VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
    )
    .bind(room_id)
    .bind(user_id)
    .bind(membership)
    .bind(now)
    .execute(pool)
    .await
    .expect("seed room_memberships");
}

#[tokio::test]
async fn test_ack_notification_marks_read_and_returns_id() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let now = current_timestamp_millis();

    cleanup_notifications(&pool, &user_id).await;

    let nid = insert_notification(&pool, &user_id, "$ev_ack", "!room1:test.com", now, "message", false, now).await;

    let result = storage.ack_notification(nid, &user_id, now + 1000).await.expect("ack_notification should succeed");
    assert!(result.is_some(), "should return the acknowledged notification id");
    assert_eq!(result.unwrap(), nid);

    cleanup_notifications(&pool, &user_id).await;
}

#[tokio::test]
async fn test_ack_notification_returns_none_for_wrong_user() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");
    let other_user = unique_user_id("@other");
    let now = current_timestamp_millis();

    cleanup_notifications(&pool, &user_id).await;
    cleanup_notifications(&pool, &other_user).await;

    let nid = insert_notification(&pool, &user_id, "$ev_wrong", "!room1:test.com", now, "message", false, now).await;

    let result = storage.ack_notification(nid, &other_user, now + 1000).await.expect("ack_notification should succeed");
    assert!(result.is_none(), "other user should not be able to ack this notification");

    cleanup_notifications(&pool, &user_id).await;
    cleanup_notifications(&pool, &other_user).await;
}

#[tokio::test]
async fn test_ack_notification_returns_none_for_nonexistent_id() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@test");

    let result = storage
        .ack_notification(99999999, &user_id, current_timestamp_millis())
        .await
        .expect("ack_notification should succeed");
    assert!(result.is_none(), "non-existent notification id should return None");
}
/// B-34: `before` / `after` define the order within a kind; with neither, a
/// **new** rule appends to the end of its kind while an **existing** rule keeps
/// its position (a plain body update must not reorder the list).
#[tokio::test]
async fn test_upsert_push_rule_positions_rules_within_kind() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@pushorder");
    cleanup_push_rules(&pool, &user_id).await;

    async fn put(storage: &PushStorage, user_id: &str, rule_id: &str, before: Option<&str>, after: Option<&str>) {
        storage
            .upsert_push_rule(
                user_id,
                "global",
                "override",
                rule_id,
                &None,
                &None,
                &json!(["notify"]),
                before,
                after,
                current_timestamp_millis(),
            )
            .await
            .expect("positioned upsert should succeed");
    }

    async fn order(storage: &PushStorage, user_id: &str) -> Vec<String> {
        storage
            .get_user_push_rules(user_id, "global", "override")
            .await
            .expect("get_user_push_rules should succeed")
            .into_iter()
            .map(|r| r.rule_id)
            .collect()
    }

    // No anchor: each new rule appends, so the order is insertion order — not
    // `rule_id` order (the pre-B-34 behaviour this feature replaces).
    put(&storage, &user_id, "zzz", None, None).await;
    put(&storage, &user_id, "aaa", None, None).await;
    put(&storage, &user_id, "mmm", None, None).await;
    assert_eq!(order(&storage, &user_id).await, vec!["zzz", "aaa", "mmm"]);

    // `after` = next-less important → immediately after the anchor.
    put(&storage, &user_id, "d", None, Some("zzz")).await;
    assert_eq!(order(&storage, &user_id).await, vec!["zzz", "d", "aaa", "mmm"]);

    // `before` = next-most important → immediately before the anchor.
    put(&storage, &user_id, "e", Some("aaa"), None).await;
    assert_eq!(order(&storage, &user_id).await, vec!["zzz", "d", "e", "aaa", "mmm"]);

    // Updating an existing rule without anchors leaves its position untouched.
    put(&storage, &user_id, "aaa", None, None).await;
    assert_eq!(order(&storage, &user_id).await, vec!["zzz", "d", "e", "aaa", "mmm"]);

    // An existing rule may be moved: it is removed before the anchor is resolved.
    put(&storage, &user_id, "zzz", Some("mmm"), None).await;
    assert_eq!(order(&storage, &user_id).await, vec!["d", "e", "aaa", "zzz", "mmm"]);

    cleanup_push_rules(&pool, &user_id).await;
}

/// `before` + `after` together is client input we cannot honour → typed error
/// (the service maps it to `400 M_INVALID_PARAM`), not a silent pick.
#[tokio::test]
async fn test_upsert_push_rule_rejects_conflicting_position() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@pushorder");
    cleanup_push_rules(&pool, &user_id).await;

    let err = storage
        .upsert_push_rule(
            &user_id,
            "global",
            "override",
            "conflict",
            &None,
            &None,
            &json!(["notify"]),
            Some("a"),
            Some("b"),
            current_timestamp_millis(),
        )
        .await
        .expect_err("both anchors must be rejected");
    assert!(matches!(err, PushRuleWriteError::ConflictingPosition), "unexpected error: {err}");
}

/// An unknown anchor — including a server default rule, which is not a row in
/// this table — is a client error, not a database error.
#[tokio::test]
#[allow(clippy::panic, reason = "test-only: an unexpected error variant is reported via panic! in a match arm")]
async fn test_upsert_push_rule_rejects_unknown_anchor() {
    let (_isolated, pool) = test_pool().await;
    let storage = PushStorage::new(Arc::clone(&pool));
    let user_id = unique_user_id("@pushorder");
    cleanup_push_rules(&pool, &user_id).await;

    let err = storage
        .upsert_push_rule(
            &user_id,
            "global",
            "override",
            "orphan",
            &None,
            &None,
            &json!(["notify"]),
            Some(".m.rule.master"),
            None,
            current_timestamp_millis(),
        )
        .await
        .expect_err("unknown anchor must be rejected");
    match err {
        PushRuleWriteError::AnchorNotFound { anchor } => assert_eq!(anchor, ".m.rule.master"),
        other => panic!("expected AnchorNotFound, got {other}"),
    }

    // The rejected write must not leave a partial row behind.
    let rows = storage.get_user_push_rules(&user_id, "global", "override").await.expect("get_user_push_rules");
    assert!(rows.is_empty(), "a failed positioned write must not insert the rule");
}
