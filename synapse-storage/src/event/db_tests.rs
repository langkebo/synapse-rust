use super::*;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

/// 每个测试一个从迁移 baseline 克隆出来的独立 schema（返回 guard 与 pool）。
///
/// 2026-09-21：原先用共享 `public` 池。共享池的问题：测试结果取决于环境里 `public` 的
/// 状态（本地 `public` 落后于迁移 baseline 时会直接 42P01），且并行测试互相影响。
/// 按铁律 7 消除状态共享：per-test schema 由模板克隆，表一定存在、行数从 0 开始。
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

fn test_server_name() -> String {
    "example.com".to_string()
}

// --- Core CRUD ---

#[tokio::test]
async fn test_create_event_returns_valid_record() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!evt_create_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$evt_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@sender:example.com";

    // Cleanup from previous runs
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "hello", "msgtype": "m.text"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };

    let event = storage.create_event(params, None).await.expect("create_event should succeed");
    assert_eq!(event.event_id, event_id);
    assert_eq!(event.room_id, room_id);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_event_found() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!evt_get_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$evt_get_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@getter:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "test"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    let found = storage.get_event(&event_id).await.expect("get_event should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().event_id, event_id);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_event_not_found() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let result = storage.get_event("$nonexistent:example.com").await.expect("get_event should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_room_events_returns_list() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!evt_list_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@lister:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    for i in 0..3 {
        let params = CreateEventParams {
            event_id: format!("$list_{}_{}:example.com", i, uuid::Uuid::new_v4()),
            room_id: room_id.clone(),
            user_id: user_id.to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": format!("msg {}", i)}),
            state_key: None,
            origin_server_ts: current_timestamp_millis(),
            redacts: None,
        };
        storage.create_event(params, None).await.unwrap();
    }

    let events = storage.get_room_events(&room_id, 10).await.expect("get_room_events should succeed");
    assert!(events.len() >= 3);

    let _ = storage.delete_room_events(&room_id).await;
}

// --- Count, Pagination, Delete ---

#[tokio::test]
async fn test_count_room_events() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!evt_count_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@counter:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let before = storage.count_room_events(&room_id).await.expect("count should succeed");

    let params = CreateEventParams {
        event_id: format!("$count_{}:example.com", uuid::Uuid::new_v4()),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "count me"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    let after = storage.count_room_events(&room_id).await.expect("count should succeed");
    assert!(after > before);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_room_events_paginated() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!evt_page_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@pager:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let events = storage.get_room_events_paginated(&room_id, None, 5, "b").await.expect("paginated should succeed");
    assert!(events.len() <= 5);

    let _ = storage.delete_room_events(&room_id).await;
}

/// ISSUE-03: txn 去重的 DB 持久化 —— 同一 (user, room, txn) 只能记录一次，
/// 缓存失效后仍能查到原始 event_id。
#[tokio::test]
async fn test_record_event_txn_dedups_and_lookups() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!txn_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@txnuser:example.com";

    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    // 未知三元组 → None
    let miss = storage.get_event_id_by_txn(user_id, &room_id, "txn-unknown").await.expect("lookup should succeed");
    assert!(miss.is_none());

    // 首次记录成功
    let inserted = storage
        .record_event_txn(user_id, &room_id, "txn-1", "$evt_txn_1:example.com")
        .await
        .expect("record should succeed");
    assert!(inserted, "first record_event_txn must insert");

    // 重复记录 → false（ON CONFLICT DO NOTHING），且不覆盖原 event_id
    let dup = storage
        .record_event_txn(user_id, &room_id, "txn-1", "$evt_txn_1_dup:example.com")
        .await
        .expect("dup record should succeed");
    assert!(!dup, "duplicate record_event_txn must not insert");

    let found = storage.get_event_id_by_txn(user_id, &room_id, "txn-1").await.expect("lookup should succeed");
    assert_eq!(found.as_deref(), Some("$evt_txn_1:example.com"), "lookup must return the original event_id");

    // 不同 room / user / txn 互不影响
    let other_txn = storage
        .record_event_txn(user_id, &room_id, "txn-2", "$evt_txn_2:example.com")
        .await
        .expect("other txn should succeed");
    assert!(other_txn);

    let _ = sqlx::query("DELETE FROM room_event_txn_dedup WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
}

#[tokio::test]
async fn test_delete_room_events() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!evt_del_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@deleter:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let params = CreateEventParams {
        event_id: format!("$del_{}:example.com", uuid::Uuid::new_v4()),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "delete me"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    storage.delete_room_events(&room_id).await.expect("delete_room_events should succeed");
    let count = storage.count_room_events(&room_id).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_get_room_message_count() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let count =
        storage.get_room_message_count("!any:example.com").await.expect("get_room_message_count should succeed");
    assert!(count >= 0);
}

// --- Ephemeral events, reporting, redaction, signatures ---

#[tokio::test]
async fn test_ephemeral_event_crud() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!eph_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@epher:example.com";

    // Cleanup from previous runs
    let _ = sqlx::query("DELETE FROM room_ephemeral WHERE room_id = $1 AND user_id = $2")
        .bind(&room_id)
        .bind(user_id)
        .execute(&*pool)
        .await;
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    storage
        .add_ephemeral_event(&room_id, user_id, "m.typing", &serde_json::json!({"typing": true}), 1)
        .await
        .expect("add_ephemeral_event should succeed");

    let now = current_timestamp_millis();
    let events = storage.get_ephemeral_events(&room_id, now, 10).await.expect("get_ephemeral_events should succeed");
    assert!(!events.is_empty());

    storage.delete_ephemeral_event(&room_id, "m.typing", user_id).await.expect("delete_ephemeral_event should succeed");

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_report_event() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!report_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$report_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@reporter:example.com";

    // Cleanup from previous runs
    let _ = sqlx::query("DELETE FROM event_reports WHERE event_id = $1").bind(&event_id).execute(&*pool).await;
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "bad content"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    storage
        .report_event(&event_id, &room_id, user_id, user_id, Some("spam"), -50)
        .await
        .expect("report_event should succeed");

    let reports = storage.get_event_report(&event_id).await.expect("get_event_report should succeed");
    assert!(!reports.is_empty());

    // Cleanup: delete reports first (FK constraint with events), then events
    let _ = sqlx::query("DELETE FROM event_reports WHERE event_id = $1").bind(&event_id).execute(&*pool).await;
    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_redact_event_content() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!redact_{}:example.com", suffix);
    let event_id = format!("$redact_{}:example.com", suffix);
    let redaction_event_id = format!("$redact_evt_{}:example.com", suffix);
    let user_id = "@redactor:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "to be redacted"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    // `redacted_by` is a foreign key to events.event_id, so the redaction
    // event itself must exist before we can record who performed the redact.
    let redaction_params = CreateEventParams {
        event_id: redaction_event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.redaction".to_string(),
        content: serde_json::json!({}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: Some(event_id.clone()),
    };
    storage.create_event(redaction_params, None).await.unwrap();

    storage
        .redact_event_content(&event_id, Some(&redaction_event_id))
        .await
        .expect("redact_event_content should succeed");

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_save_and_get_event_signatures() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!sig_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$sig_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@signer:example.com";

    // Cleanup from previous runs
    let _ = sqlx::query("DELETE FROM event_signatures WHERE event_id = $1").bind(&event_id).execute(&*pool).await;
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "signed"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    let now = current_timestamp_millis();
    storage
        .save_event_signature(&event_id, user_id, "DEVICE1", "sig_data", "ed25519:1", "ed25519", now)
        .await
        .expect("save_event_signature should succeed");

    let sigs = storage.get_event_signatures(&event_id).await.expect("get_event_signatures should succeed");
    assert!(!sigs.is_empty());
    assert_eq!(sigs[0].user_id, user_id);

    // Cleanup: delete signatures first, then events
    let _ = sqlx::query("DELETE FROM event_signatures WHERE event_id = $1").bind(&event_id).execute(&*pool).await;
    let _ = storage.delete_room_events(&room_id).await;
}

// --- Other queries ---

#[tokio::test]
async fn test_find_missing_event_ids() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let input = vec![format!("$missing_{}:example.com", uuid::Uuid::new_v4())];
    let missing = storage.find_missing_event_ids(&input).await.expect("find_missing_event_ids should succeed");
    assert_eq!(missing.len(), 1);
}

#[tokio::test]
async fn test_get_total_message_count() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let count = storage.get_total_message_count().await.expect("get_total_message_count should succeed");
    assert!(count >= 0);
}

#[tokio::test]
async fn test_get_daily_message_count() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let count = storage.get_daily_message_count().await.expect("get_daily_message_count should succeed");
    assert!(count >= 0);
}

#[tokio::test]
async fn test_delete_remote_events_before() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!evt_old_{}:example.com", uuid::Uuid::new_v4());

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;

    // Delete events before a far-future timestamp — should succeed even if 0 rows
    let _deleted = storage
        .delete_remote_events_before(&room_id, current_timestamp_millis() + 86400000, false)
        .await
        .expect("delete_remote_events_before should succeed");

    let _ = storage.delete_room_events(&room_id).await;
}

/// Security regression test (P0): purge history must preserve local events
/// (origin = 'self' or NULL) and only delete remote/federated events.
///
/// Mirrors the Element Synapse v1.156 fix that prevents accidental deletion
/// of locally-originated outbound events during history purge operations.
#[tokio::test]
async fn test_purge_history_preserves_local_events() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!purge_sec_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@purger:example.com";
    let local_event_id = format!("$local_{}:example.com", uuid::Uuid::new_v4());
    let remote_event_id = format!("$remote_{}:remote.example.com", uuid::Uuid::new_v4());

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    // Insert a LOCAL event via create_event (origin = 'self')
    let past_ts = current_timestamp_millis() - 60_000;
    let local_params = CreateEventParams {
        event_id: local_event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "local outbound"}),
        state_key: None,
        origin_server_ts: past_ts,
        redacts: None,
    };
    storage.create_event(local_params, None).await.expect("local event insert should succeed");

    // Insert a REMOTE event via direct SQL (origin = 'remote.example.com')
    sqlx::query(
        r#"INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, state_key, origin_server_ts, is_redacted, origin)
           VALUES ($1, $2, $3, $3, 'm.room.message', $4, NULL, $5, false, 'remote.example.com')"#,
    )
    .bind(&remote_event_id)
    .bind(&room_id)
    .bind(user_id)
    .bind(serde_json::json!({"body": "remote inbound"}))
    .bind(past_ts)
    .execute(&*pool)
    .await
    .expect("remote event insert should succeed");

    // Purge all events before now+1s — both events satisfy the timestamp filter
    let purge_cutoff = current_timestamp_millis() + 1_000;
    let deleted = storage
        .delete_remote_events_before(&room_id, purge_cutoff, false)
        .await
        .expect("delete_remote_events_before should succeed");

    // Exactly one remote event should have been deleted
    assert_eq!(deleted, 1, "purge should delete exactly 1 remote event, got {}", deleted);

    // Local event MUST still exist
    let local_still_exists = storage.get_event(&local_event_id).await.expect("get_event for local should succeed");
    assert!(local_still_exists.is_some(), "LOCAL event must be preserved during purge history (security regression)");

    // Remote event MUST be deleted
    let remote_still_exists = storage.get_event(&remote_event_id).await.expect("get_event for remote should succeed");
    assert!(remote_still_exists.is_none(), "REMOTE event should have been purged");

    let _ = storage.delete_room_events(&room_id).await;
}

/// Verify `count_events_before` returns the number of remote events that
/// would be purged, and that `delete_remote_events_before` with `dry_run=true`
/// returns the same count without deleting anything.
#[tokio::test]
async fn test_count_events_before_and_dry_run() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!dryrun_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@dryrunner:example.com";
    let remote_event_id = format!("$remote_dry_{}:remote.example.com", uuid::Uuid::new_v4());

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let past_ts = current_timestamp_millis() - 60_000;
    sqlx::query(
        r#"INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, state_key, origin_server_ts, is_redacted, origin)
           VALUES ($1, $2, $3, $3, 'm.room.message', $4, NULL, $5, false, 'remote.example.com')"#,
    )
    .bind(&remote_event_id)
    .bind(&room_id)
    .bind(user_id)
    .bind(serde_json::json!({"body": "remote"}))
    .bind(past_ts)
    .execute(&*pool)
    .await
    .expect("remote event insert should succeed");

    let purge_cutoff = current_timestamp_millis() + 1_000;

    // count_events_before should report exactly 1 deletable remote event
    let count = storage.count_events_before(&room_id, purge_cutoff).await.expect("count_events_before should succeed");
    assert_eq!(count, 1, "count_events_before should report 1 remote event");

    // dry_run=true should return the same count but NOT delete
    let dry_count = storage
        .delete_remote_events_before(&room_id, purge_cutoff, true)
        .await
        .expect("dry-run delete_remote_events_before should succeed");
    assert_eq!(dry_count, 1, "dry-run should report 1 deletable event");

    // Event must still exist after dry-run
    let still_exists = storage.get_event(&remote_event_id).await.expect("get_event should succeed");
    assert!(still_exists.is_some(), "dry-run must not delete the event");

    // Now actually delete with dry_run=false
    let deleted = storage
        .delete_remote_events_before(&room_id, purge_cutoff, false)
        .await
        .expect("delete_remote_events_before should succeed");
    assert_eq!(deleted, 1, "actual delete should remove 1 event");

    let gone = storage.get_event(&remote_event_id).await.expect("get_event should succeed");
    assert!(gone.is_none(), "event should be deleted after non-dry-run purge");

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_room_create_event_none_for_non_existent() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let result =
        storage.get_room_create_event("!nonexistent:example.com").await.expect("get_room_create_event should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_events_batch_empty_input() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let results = storage.get_events_batch(&[]).await.expect("get_events_batch should succeed");
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_get_forward_extremities_count() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let count = storage
        .get_forward_extremities_count("!any:example.com")
        .await
        .expect("get_forward_extremities_count should succeed");
    assert!(count >= 0);
}

// --- create_event_with_graph / signatures_and_hashes ---

#[tokio::test]
async fn test_create_event_with_graph_no_prev_events() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!graph_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$graph_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@grapher:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "graph"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    let event = storage
        .create_event_with_graph(params, &[], &[], 1, None)
        .await
        .expect("create_event_with_graph should succeed");
    assert_eq!(event.event_id, event_id);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_create_event_with_graph_with_prev_events() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!graphp_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@grapherp:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    // Create a parent event first.
    let parent_id = format!("$parent_{}:example.com", uuid::Uuid::new_v4());
    let parent_params = CreateEventParams {
        event_id: parent_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "parent"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(parent_params, None).await.unwrap();

    let child_id = format!("$child_{}:example.com", uuid::Uuid::new_v4());
    let child_params = CreateEventParams {
        event_id: child_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "child"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage
        .create_event_with_graph(child_params, std::slice::from_ref(&parent_id), &[], 2, None)
        .await
        .expect("create_event_with_graph with prev should succeed");

    // event_edges should have a row linking child -> parent.
    let edge_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM event_edges WHERE event_id = $1 AND prev_event_id = $2")
            .bind(&child_id)
            .bind(&parent_id)
            .fetch_one(&*pool)
            .await
            .expect("edge count query should succeed");
    assert!(edge_count >= 1);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_create_event_with_graph_in_transaction() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!graphtx_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@graphtx:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let event_id = format!("$graphtx_{}:example.com", uuid::Uuid::new_v4());
    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "tx"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };

    let mut tx = pool.begin().await.expect("begin tx should succeed");
    let event = storage
        .create_event_with_graph(params, &[], &[], 1, Some(&mut tx))
        .await
        .expect("create_event_with_graph in tx should succeed");
    tx.commit().await.expect("commit should succeed");
    assert_eq!(event.event_id, event_id);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_update_event_signatures_and_hashes() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!sighash_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$sighash_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@sighasher:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "sign me"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    let signatures = serde_json::json!({"example.com": {"ed25519:1": "abc"}});
    let hashes = serde_json::json!({"sha256": "def"});
    storage
        .update_event_signatures_and_hashes(&event_id, &signatures, &hashes)
        .await
        .expect("update_event_signatures_and_hashes should succeed");

    let _ = storage.delete_room_events(&room_id).await;
}

// --- find_missing_event_ids / get_missing_events_between ---

#[tokio::test]
async fn test_find_missing_event_ids_empty_input() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let missing =
        storage.find_missing_event_ids(&[]).await.expect("find_missing_event_ids with empty input should succeed");
    assert!(missing.is_empty());
}

#[tokio::test]
async fn test_find_missing_event_ids_partial_existing() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!miss_{}:example.com", uuid::Uuid::new_v4());
    let existing_id = format!("$exists_{}:example.com", uuid::Uuid::new_v4());
    let missing_id = format!("$nope_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@missr:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let params = CreateEventParams {
        event_id: existing_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "exists"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    let missing = storage
        .find_missing_event_ids(&[existing_id, missing_id.clone()])
        .await
        .expect("find_missing_event_ids should succeed");
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0], missing_id);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_missing_events_between_empty_latest() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let result = storage
        .get_missing_events_between("!any:example.com", &["$a:example.com".to_string()], &[], 10)
        .await
        .expect("get_missing_events_between with empty latest should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_missing_events_between_walks_dag() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!dag_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@dagger:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    // Build a small DAG: root -> middle -> leaf
    let root_id = format!("$root_{}:example.com", uuid::Uuid::new_v4());
    let middle_id = format!("$middle_{}:example.com", uuid::Uuid::new_v4());
    let leaf_id = format!("$leaf_{}:example.com", uuid::Uuid::new_v4());

    let mk = |eid: String, ts: i64| CreateEventParams {
        event_id: eid,
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "node"}),
        state_key: None,
        origin_server_ts: ts,
        redacts: None,
    };
    storage.create_event(mk(root_id.clone(), 1000), None).await.unwrap();
    storage
        .create_event_with_graph(mk(middle_id.clone(), 2000), std::slice::from_ref(&root_id), &[], 1, None)
        .await
        .unwrap();
    storage
        .create_event_with_graph(mk(leaf_id.clone(), 3000), std::slice::from_ref(&middle_id), &[], 2, None)
        .await
        .unwrap();

    // Walk back from leaf, with root as earliest — should collect middle.
    let missing = storage
        .get_missing_events_between(&room_id, &[root_id], &[leaf_id], 10)
        .await
        .expect("get_missing_events_between should succeed");
    let ids: Vec<&str> = missing.iter().filter_map(|v| v["event_id"].as_str()).collect();
    assert!(ids.contains(&middle_id.as_str()));

    let _ = storage.delete_room_events(&room_id).await;
}

// --- upsert_ephemeral_event / get_ephemeral_events_batch ---

#[tokio::test]
async fn test_upsert_ephemeral_event_updates_existing() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!ephup_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@ephup:example.com";

    let _ = sqlx::query("DELETE FROM room_ephemeral WHERE room_id = $1 AND user_id = $2")
        .bind(&room_id)
        .bind(user_id)
        .execute(&*pool)
        .await;
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let now = current_timestamp_millis();
    storage
        .upsert_ephemeral_event(&room_id, user_id, "m.typing", &serde_json::json!({"typing": false}), 1, now, None)
        .await
        .expect("first upsert should succeed");
    storage
        .upsert_ephemeral_event(&room_id, user_id, "m.typing", &serde_json::json!({"typing": true}), 2, now, None)
        .await
        .expect("second upsert should succeed");

    let events = storage.get_ephemeral_events(&room_id, now, 10).await.unwrap();
    // Should only have one row (upserted), with stream_id == 2.
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].stream_id, 2);

    storage.delete_ephemeral_event(&room_id, "m.typing", user_id).await.unwrap();
    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_ephemeral_events_filters_expired() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!ephexp_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@ephexp:example.com";

    let _ = sqlx::query("DELETE FROM room_ephemeral WHERE room_id = $1 AND user_id = $2")
        .bind(&room_id)
        .bind(user_id)
        .execute(&*pool)
        .await;
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let now = current_timestamp_millis();
    let past_expiry = now - 1000;
    // Insert an expired ephemeral event directly via upsert (expires_at in the past).
    storage
        .upsert_ephemeral_event(
            &room_id,
            user_id,
            "m.typing",
            &serde_json::json!({"typing": true}),
            1,
            now,
            Some(past_expiry),
        )
        .await
        .expect("upsert expired ephemeral should succeed");

    // Querying at `now` should exclude the expired row.
    let events = storage.get_ephemeral_events(&room_id, now, 10).await.unwrap();
    assert!(events.is_empty());

    storage.delete_ephemeral_event(&room_id, "m.typing", user_id).await.unwrap();
    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_ephemeral_events_batch_empty_rooms() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let now = current_timestamp_millis();
    let result = storage
        .get_ephemeral_events_batch(&[], now, 10)
        .await
        .expect("get_ephemeral_events_batch with empty rooms should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_ephemeral_events_batch_multiple_rooms() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room1 = format!("!ephb1_{}:example.com", uuid::Uuid::new_v4());
    let room2 = format!("!ephb2_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@ephbatch:example.com";

    for r in [&room1, &room2] {
        let _ = sqlx::query("DELETE FROM room_ephemeral WHERE room_id = $1 AND user_id = $2")
            .bind(r)
            .bind(user_id)
            .execute(&*pool)
            .await;
        let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(r).execute(&*pool).await;
        ensure_test_room(&pool, r).await;
    }
    ensure_test_user(&pool, user_id).await;

    let now = current_timestamp_millis();
    storage.add_ephemeral_event(&room1, user_id, "m.typing", &serde_json::json!({"typing": true}), 1).await.unwrap();
    storage.add_ephemeral_event(&room2, user_id, "m.typing", &serde_json::json!({"typing": false}), 2).await.unwrap();

    let result = storage
        .get_ephemeral_events_batch(&[room1.clone(), room2.clone()], now, 10)
        .await
        .expect("get_ephemeral_events_batch should succeed");
    assert_eq!(result.len(), 2);
    assert!(result.contains_key(&room1));
    assert!(result.contains_key(&room2));
    assert!(!result[&room1].is_empty());
    assert!(!result[&room2].is_empty());

    for r in [&room1, &room2] {
        storage.delete_ephemeral_event(r, "m.typing", user_id).await.unwrap();
        let _ = storage.delete_room_events(r).await;
    }
}

// --- pagination (forward / backward) ---

#[tokio::test]
async fn test_get_room_events_paginated_forward_with_from() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!pagef_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@pagef:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let base = 1_000_000_i64;
    for i in 0..3 {
        let params = CreateEventParams {
            event_id: format!("$pf_{}_{}:example.com", i, uuid::Uuid::new_v4()),
            room_id: room_id.clone(),
            user_id: user_id.to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": format!("msg {i}")}),
            state_key: None,
            origin_server_ts: base + i,
            redacts: None,
        };
        storage.create_event(params, None).await.unwrap();
    }

    // Forward from base+1 should return events with ts > base+1.
    let events = storage
        .get_room_events_paginated(&room_id, Some(base + 1), 10, "f")
        .await
        .expect("forward paginated should succeed");
    assert!(!events.is_empty());
    for e in &events {
        assert!(e.origin_server_ts > base + 1);
    }

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_room_events_paginated_forward_no_from() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!pagefn_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@pagefn:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    for i in 0..2 {
        let params = CreateEventParams {
            event_id: format!("$pfn_{}_{}:example.com", i, uuid::Uuid::new_v4()),
            room_id: room_id.clone(),
            user_id: user_id.to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": format!("m {i}")}),
            state_key: None,
            origin_server_ts: current_timestamp_millis(),
            redacts: None,
        };
        storage.create_event(params, None).await.unwrap();
    }

    let events = storage
        .get_room_events_paginated(&room_id, None, 10, "f")
        .await
        .expect("forward paginated no from should succeed");
    assert!(events.len() >= 2);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_room_events_paginated_backward_with_from() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!pageb_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@pageb:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let base = 5_000_000_i64;
    for i in 0..3 {
        let params = CreateEventParams {
            event_id: format!("$pb_{}_{}:example.com", i, uuid::Uuid::new_v4()),
            room_id: room_id.clone(),
            user_id: user_id.to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": format!("m {i}")}),
            state_key: None,
            origin_server_ts: base + i,
            redacts: None,
        };
        storage.create_event(params, None).await.unwrap();
    }

    // Backward from base+2 should return events with ts < base+2.
    let events = storage
        .get_room_events_paginated(&room_id, Some(base + 2), 10, "b")
        .await
        .expect("backward paginated should succeed");
    assert!(!events.is_empty());
    for e in &events {
        assert!(e.origin_server_ts < base + 2);
    }

    let _ = storage.delete_room_events(&room_id).await;
}

/// ISSUE-06: 同一毫秒内的多条事件在 /messages 翻页边界不得丢失或重复。
/// 旧实现 token 只有 `t{ts}` 且用严格不等号，同毫秒事件会被跳过；
/// 新的游标 API 以 (origin_server_ts, stream_ordering) 复合游标精确翻页。
#[tokio::test]
async fn test_paginated_cursor_same_millisecond_no_loss_no_dup() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!pagems_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@pagems:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    // 三条事件共享完全相同的 origin_server_ts
    let same_ts = current_timestamp_millis();
    let mut inserted_ids = Vec::new();
    for i in 0..3 {
        let params = CreateEventParams {
            event_id: format!("$pagems_{}_{}:example.com", i, uuid::Uuid::new_v4()),
            room_id: room_id.clone(),
            user_id: user_id.to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": format!("same-ms {i}")}),
            state_key: None,
            origin_server_ts: same_ts,
            redacts: None,
        };
        let event = storage.create_event(params, None).await.unwrap();
        assert!(event.stream_ordering.is_some(), "stream_ordering must be assigned by the DB");
        inserted_ids.push(event.event_id.clone());
    }

    // 第一页：取最新 2 条
    let page1 =
        storage.get_room_events_paginated_cursor(&room_id, None, 2, "b").await.expect("cursor page1 should succeed");
    assert_eq!(page1.len(), 2, "page1 must contain 2 events");

    // 用页尾（最旧一条）的复合游标翻第二页
    let boundary = page1.last().expect("page1 non-empty");
    let cursor = (boundary.origin_server_ts, boundary.stream_ordering);
    let page2 = storage
        .get_room_events_paginated_cursor(&room_id, Some(cursor), 2, "b")
        .await
        .expect("cursor page2 should succeed");

    let mut seen: Vec<String> = page1.iter().chain(page2.iter()).map(|e| e.event_id.clone()).collect();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 3, "two pages must cover all 3 same-ms events without duplicates, got {seen:?}");
    for id in &inserted_ids {
        assert!(seen.contains(id), "event {id} must appear exactly once across pages");
    }

    // 前向翻页同样不得丢失
    let fwd1 = storage
        .get_room_events_paginated_cursor(&room_id, None, 2, "f")
        .await
        .expect("cursor fwd page1 should succeed");
    assert_eq!(fwd1.len(), 2);
    let fwd_boundary = fwd1.last().expect("fwd page1 non-empty");
    let fwd_cursor = (fwd_boundary.origin_server_ts, fwd_boundary.stream_ordering);
    let fwd2 = storage
        .get_room_events_paginated_cursor(&room_id, Some(fwd_cursor), 2, "f")
        .await
        .expect("cursor fwd page2 should succeed");
    let mut fwd_seen: Vec<String> = fwd1.iter().chain(fwd2.iter()).map(|e| e.event_id.clone()).collect();
    fwd_seen.sort();
    fwd_seen.dedup();
    assert_eq!(fwd_seen.len(), 3, "forward pages must cover all 3 same-ms events without duplicates");

    // legacy 语义保持：stream 为 None 时退化为旧的严格时间戳比较
    let legacy = storage
        .get_room_events_paginated_cursor(&room_id, Some((same_ts, None)), 10, "b")
        .await
        .expect("legacy cursor should succeed");
    assert!(legacy.is_empty(), "legacy (ts-only) backward cursor keeps strict < ts semantics");

    let _ = storage.delete_room_events(&room_id).await;
}

// --- timestamp lookups ---

#[tokio::test]
async fn test_find_event_by_timestamp_found() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!tsfind_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$tsfind_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@tsfinder:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let ts = 2_000_000_i64;
    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "ts"}),
        state_key: None,
        origin_server_ts: ts,
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    let found = storage.find_event_by_timestamp(&room_id, ts).await.expect("find_event_by_timestamp should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap()["event_id"], event_id);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_find_event_by_timestamp_none() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let result = storage
        .find_event_by_timestamp("!nonexistent:example.com", 1_000_000)
        .await
        .expect("find_event_by_timestamp should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn test_find_event_id_by_timestamp_forward() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!tsfwd_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$tsfwd_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@tsfwd:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let ts = 3_000_000_i64;
    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "fwd"}),
        state_key: None,
        origin_server_ts: ts,
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    // Forward from ts-1 should find the event at ts.
    let found = storage
        .find_event_id_by_timestamp(&room_id, ts - 1, true)
        .await
        .expect("find_event_id_by_timestamp forward should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().0, event_id);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_find_event_id_by_timestamp_backward() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!tsbwd_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$tsbwd_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@tsbwd:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let ts = 4_000_000_i64;
    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "bwd"}),
        state_key: None,
        origin_server_ts: ts,
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    // Backward from ts+1 should find the event at ts.
    let found = storage
        .find_event_id_by_timestamp(&room_id, ts + 1, false)
        .await
        .expect("find_event_id_by_timestamp backward should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().0, event_id);

    let _ = storage.delete_room_events(&room_id).await;
}

// --- type / sender filters ---

#[tokio::test]
async fn test_get_room_events_by_type_filters() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!type_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@typer:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let mk = |eid: &str, et: &str| CreateEventParams {
        event_id: eid.to_string(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: et.to_string(),
        content: serde_json::json!({}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage
        .create_event(mk(&format!("$t1_{}:example.com", uuid::Uuid::new_v4()), "m.room.message"), None)
        .await
        .unwrap();
    storage
        .create_event(mk(&format!("$t2_{}:example.com", uuid::Uuid::new_v4()), "m.room.member"), None)
        .await
        .unwrap();

    let messages = storage
        .get_room_events_by_type(&room_id, "m.room.message", 10)
        .await
        .expect("get_room_events_by_type should succeed");
    assert!(messages.iter().all(|e| e.event_type == "m.room.message"));

    let members = storage
        .get_room_events_by_type(&room_id, "m.room.member", 10)
        .await
        .expect("get_room_events_by_type member should succeed");
    assert!(members.iter().all(|e| e.event_type == "m.room.member"));

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_sender_events_filters() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!sender_{}:example.com", uuid::Uuid::new_v4());
    let user_id = format!("@sender_{}:example.com", uuid::Uuid::new_v4());

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    let params = CreateEventParams {
        event_id: format!("$send_{}:example.com", uuid::Uuid::new_v4()),
        room_id: room_id.clone(),
        user_id: user_id.clone(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "from sender"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    let events = storage.get_sender_events(&user_id, 10).await.expect("get_sender_events should succeed");
    assert!(events.iter().all(|e| e.user_id == user_id));

    let _ = storage.delete_room_events(&room_id).await;
}

// --- report score updates ---

#[tokio::test]
async fn test_update_event_report_score_by_id() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!rscore_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$rscore_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@reporter2:example.com";

    let _ = sqlx::query("DELETE FROM event_reports WHERE event_id = $1").bind(&event_id).execute(&*pool).await;
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "report me"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    let report_id = storage
        .report_event(&event_id, &room_id, user_id, user_id, Some("bad"), -10)
        .await
        .expect("report_event should succeed");

    storage.update_event_report_score(report_id, -100).await.expect("update_event_report_score should succeed");

    let reports = storage.get_event_report(&event_id).await.unwrap();
    let updated = reports.iter().find(|r| r.id == report_id).expect("report should exist");
    assert_eq!(updated.score, -100);

    let _ = sqlx::query("DELETE FROM event_reports WHERE event_id = $1").bind(&event_id).execute(&*pool).await;
    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_update_event_report_score_by_event() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!rscoreev_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$rscoreev_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@reporter3:example.com";

    let _ = sqlx::query("DELETE FROM event_reports WHERE event_id = $1").bind(&event_id).execute(&*pool).await;
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "report me by event"}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    storage.report_event(&event_id, &room_id, user_id, user_id, Some("bad"), 0).await.unwrap();
    storage
        .update_event_report_score_by_event(&event_id, -42)
        .await
        .expect("update_event_report_score_by_event should succeed");

    let reports = storage.get_event_report(&event_id).await.unwrap();
    assert!(reports.iter().all(|r| r.score == -42));

    let _ = sqlx::query("DELETE FROM event_reports WHERE event_id = $1").bind(&event_id).execute(&*pool).await;
    let _ = storage.delete_room_events(&room_id).await;
}

// --- power levels / context / search / latest ---

#[tokio::test]
async fn test_upsert_power_levels_event_insert_and_update() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!pl_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$pl_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@power:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let content = serde_json::json!({"ban": 50, "kick": 50});
    storage
        .upsert_power_levels_event(&event_id, &room_id, user_id, content.clone(), 1_000, user_id)
        .await
        .expect("upsert_power_levels_event insert should succeed");

    // Upsert again to update content.
    let new_content = serde_json::json!({"ban": 75});
    storage
        .upsert_power_levels_event(&event_id, &room_id, user_id, new_content.clone(), 2_000, user_id)
        .await
        .expect("upsert_power_levels_event update should succeed");

    let event = storage.get_event(&event_id).await.unwrap().expect("power_levels event should exist");
    assert_eq!(event.event_type, "m.room.power_levels");
    assert_eq!(event.content, new_content);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_events_before_context() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!ctxb_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@ctxb:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let base = 6_000_000_i64;
    for i in 0..3 {
        let params = CreateEventParams {
            event_id: format!("$ctxb_{}_{}:example.com", i, uuid::Uuid::new_v4()),
            room_id: room_id.clone(),
            user_id: user_id.to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": format!("ctx {i}")}),
            state_key: None,
            origin_server_ts: base + i,
            redacts: None,
        };
        storage.create_event(params, None).await.unwrap();
    }

    let before = storage
        .get_events_before_context(&room_id, base + 2, 10)
        .await
        .expect("get_events_before_context should succeed");
    assert!(!before.is_empty());
    assert!(before.iter().all(|e| e["origin_server_ts"].as_i64().unwrap_or(0) < base + 2));

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_events_after_context() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!ctxa_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@ctxa:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let base = 7_000_000_i64;
    for i in 0..3 {
        let params = CreateEventParams {
            event_id: format!("$ctxa_{}_{}:example.com", i, uuid::Uuid::new_v4()),
            room_id: room_id.clone(),
            user_id: user_id.to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": format!("ctxa {i}")}),
            state_key: None,
            origin_server_ts: base + i,
            redacts: None,
        };
        storage.create_event(params, None).await.unwrap();
    }

    let after =
        storage.get_events_after_context(&room_id, base, 10).await.expect("get_events_after_context should succeed");
    assert!(!after.is_empty());
    assert!(after.iter().all(|e| e["origin_server_ts"].as_i64().unwrap_or(0) > base));

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_search_room_messages_admin_matches() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!adminsearch_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@adminsearch:example.com";
    let needle = format!("uniqueneedle_{}", uuid::Uuid::new_v4());

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let params = CreateEventParams {
        event_id: format!("$as_{}:example.com", uuid::Uuid::new_v4()),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": needle.clone()}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    let pattern = format!("%{}%", needle.to_lowercase());
    let results = storage
        .search_room_messages_admin(&room_id, &pattern, 10)
        .await
        .expect("search_room_messages_admin should succeed");
    assert!(!results.is_empty());

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_latest_event_ids_in_room() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!latest_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@latest:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let base = 8_000_000_i64;
    let last_id = format!("$last_{}:example.com", uuid::Uuid::new_v4());
    for i in 0..3 {
        let eid = if i == 2 { last_id.clone() } else { format!("$l_{}_{}:example.com", i, uuid::Uuid::new_v4()) };
        let params = CreateEventParams {
            event_id: eid,
            room_id: room_id.clone(),
            user_id: user_id.to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": "latest"}),
            state_key: None,
            origin_server_ts: base + i,
            redacts: None,
        };
        storage.create_event(params, None).await.unwrap();
    }

    let ids =
        storage.get_latest_event_ids_in_room(&room_id, 1).await.expect("get_latest_event_ids_in_room should succeed");
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0], last_id);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_room_events_paginated_with_filter_no_filter() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!filt_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@filt:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    for i in 0..2 {
        let params = CreateEventParams {
            event_id: format!("$filt_{}_{}:example.com", i, uuid::Uuid::new_v4()),
            room_id: room_id.clone(),
            user_id: user_id.to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": format!("f {i}")}),
            state_key: None,
            origin_server_ts: current_timestamp_millis(),
            redacts: None,
        };
        storage.create_event(params, None).await.unwrap();
    }

    let events = storage
        .get_room_events_paginated_with_filter(&room_id, None, None, 10, None)
        .await
        .expect("get_room_events_paginated_with_filter should succeed");
    assert!(events.len() >= 2);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_room_create_event_found() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!createev_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$createev_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@creator:example.com";

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.create".to_string(),
        content: serde_json::json!({"creator": user_id}),
        state_key: Some("".to_string()),
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    let found = storage.get_room_create_event(&room_id).await.expect("get_room_create_event should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().event_id, event_id);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_search_room_postgres_messages_matches() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!pgfts_{}:example.com", uuid::Uuid::new_v4());
    let user_id = "@pgfts:example.com";
    let term = format!("zxcvunique_{}", uuid::Uuid::new_v4());

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    let params = CreateEventParams {
        event_id: format!("$pgfts_{}:example.com", uuid::Uuid::new_v4()),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": term.clone()}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    let results = storage
        .search_room_postgres_messages(&room_id, &term, 10)
        .await
        .expect("search_room_postgres_messages should succeed");
    assert!(results.iter().any(|e| e.event_type == "m.room.message"));

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_create_postgres_fts_index_idempotent() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    // Creating the FTS index twice should not error (idempotent).
    storage.create_postgres_fts_index().await.expect("create_postgres_fts_index first call should succeed");
    storage.create_postgres_fts_index().await.expect("create_postgres_fts_index second call should succeed");
}

/// A `CONCURRENTLY` build that fails leaves an **INVALID** index behind, and
/// `IF NOT EXISTS` then skips that name forever — so `create_postgres_fts_index`
/// must not report success in that state.
///
/// The INVALID state is reproduced the way PostgreSQL actually produces it: a
/// concurrent UNIQUE build over a table with duplicate rows fails and keeps the
/// index. This runs on a **per-test isolated schema** (not the shared
/// `test_pool()`), because the leftover index would otherwise outlive the test
/// and break `test_create_postgres_fts_index_idempotent`, which shares that
/// schema.
#[tokio::test]
async fn test_create_postgres_fts_index_reports_invalid_leftover() {
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
    let pool = isolated.pool();
    let storage = EventStorage::new(&pool, test_server_name());

    // Two duplicate rows in one statement, via CTAS: an `INSERT INTO <scratch>`
    // would also register the throwaway table name with
    // `scripts/check_schema_table_coverage.py`, which scans SQL literals for
    // table references and (correctly) expects them to exist in migrations.
    // `CREATE TABLE … AS SELECT` references nothing, so the gate stays quiet
    // about a table that only exists inside this test's isolated schema.
    sqlx::query("CREATE TABLE fts_invalid_probe AS SELECT 1 AS a UNION ALL SELECT 1")
        .execute(&*pool)
        .await
        .expect("scratch table with duplicate rows");

    // Fails with a unique violation and leaves `events_fts_idx` INVALID.
    let build =
        sqlx::query("CREATE UNIQUE INDEX CONCURRENTLY events_fts_idx ON fts_invalid_probe (a)").execute(&*pool).await;
    assert!(build.is_err(), "a concurrent UNIQUE build over duplicate rows must fail");

    let invalid: bool = sqlx::query_scalar(
        "SELECT NOT i.indisvalid FROM pg_index i WHERE i.indexrelid = to_regclass('events_fts_idx')",
    )
    .fetch_one(&*pool)
    .await
    .expect("the failed build must leave the index present");
    assert!(invalid, "precondition: the leftover index must be INVALID");

    let err = storage
        .create_postgres_fts_index()
        .await
        .expect_err("an INVALID leftover index must not be reported as success");
    assert!(err.to_string().contains("INVALID"), "the error must name the INVALID state and the remedy, got: {err}");
}

#[tokio::test]
async fn test_search_joined_room_events_empty_joined() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let results = storage
        .search_joined_room_events(&[], "%anything%", None, None, None, None, None, 10)
        .await
        .expect("search_joined_room_events with empty joined should succeed");
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_search_joined_room_events_matches() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!jrsearch_{}:example.com", uuid::Uuid::new_v4());
    let user_id = format!("@jrsearch_{}:example.com", uuid::Uuid::new_v4());
    let needle = format!("jrneedle_{}", uuid::Uuid::new_v4());

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    let params = CreateEventParams {
        event_id: format!("$jr_{}:example.com", uuid::Uuid::new_v4()),
        room_id: room_id.clone(),
        user_id: user_id.clone(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": needle.clone()}),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    let pattern = format!("%{}%", needle.to_lowercase());
    let results = storage
        .search_joined_room_events(std::slice::from_ref(&room_id), &pattern, None, None, None, None, None, 10)
        .await
        .expect("search_joined_room_events should succeed");
    assert!(!results.is_empty());

    let _ = storage.delete_room_events(&room_id).await;
}

// --- P1-7: notification count bloat after purge_history ---

/// Helper: insert a remote-origin event from `sender` at `ts`.
async fn insert_remote_event(
    pool: &Pool<Postgres>,
    event_id: &str,
    room_id: &str,
    sender: &str,
    ts: i64,
    origin: &str,
) {
    sqlx::query(
        r#"INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, state_key, origin_server_ts, is_redacted, origin)
           VALUES ($1, $2, $3, $3, 'm.room.message', $4, NULL, $5, false, $6)"#,
    )
    .bind(event_id)
    .bind(room_id)
    .bind(sender)
    .bind(serde_json::json!({"body": "msg"}))
    .bind(ts)
    .bind(origin)
    .execute(pool)
    .await
    .expect("insert event should succeed");
}

/// P1-7 RED test: After `purge_history` deletes the event referenced by
/// `read_markers.event_id`, `get_unread_counts` must NOT bloat.
///
/// Scenario:
/// - e1_local  @ ts=1000  (origin='self', sender=@other) — survives purge
/// - e2_remote @ ts=2000  (origin='remote', sender=@other) — MARKER event, purged
/// - e3_remote @ ts=3000  (origin='remote', sender=@other) — unread, survives purge
/// - Reader @me sets read marker to e2 → e3 is the only unread (count=1)
/// - Purge before ts=2500 → deletes e2_remote (marker). e1_local + e3_remote remain.
/// - Expected: count still 1 (only e3; e1_local was already read before marker e2)
/// - BUG (current): last_read_ts=0 because e2 is gone, so e1_local (ts=1000 > 0)
///   is counted as unread → count=2 (bloat)
#[tokio::test]
async fn test_p1_7_unread_count_not_bloated_after_purge_history() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_storage = crate::room::RoomStorage::new(&pool);

    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!p17_{}:example.com", suffix);
    let reader = format!("@p17me_{}:example.com", suffix);
    let other = format!("@p17other_{}:example.com", suffix);
    let e1_id = format!("$p17_e1_{}:example.com", suffix);
    let e2_id = format!("$p17_e2_{}:remote.example.com", suffix);
    let e3_id = format!("$p17_e3_{}:remote.example.com", suffix);

    // Cleanup
    let _ = sqlx::query("DELETE FROM read_markers WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &reader).await;
    ensure_test_user(&pool, &other).await;

    // e1: LOCAL event from @other (origin='self', survives purge)
    insert_remote_event(&pool, &e1_id, &room_id, &other, 1_000_000, "self").await;
    // e2: REMOTE event from @other (origin='remote', will be purged) — MARKER
    insert_remote_event(&pool, &e2_id, &room_id, &other, 1_000_001, "remote.example.com").await;
    // e3: REMOTE event from @other (origin='remote', after cutoff, survives)
    insert_remote_event(&pool, &e3_id, &room_id, &other, 1_000_002, "remote.example.com").await;

    // Reader @me sets read marker to e2 (read up to e2; e3 is unread)
    room_storage.update_read_marker(&room_id, &reader, &e2_id).await.expect("update_read_marker should succeed");

    // Baseline: only e3 is unread (e1 is before marker e2)
    let baseline =
        storage.get_unread_counts(&room_id, &reader).await.expect("baseline get_unread_counts should succeed");
    assert_eq!(baseline.notification_count, 1, "baseline: only e3 should be unread (e1 is before marker e2)");

    // Purge history before ts=1_000_002 → deletes e2 (ts=1_000_001 < cutoff, origin=remote)
    // e1 survives (origin='self'), e3 survives (ts=1_000_002 is NOT < cutoff)
    let purge_cutoff = 1_000_002;
    let deleted = storage
        .delete_remote_events_before(&room_id, purge_cutoff, false)
        .await
        .expect("delete_remote_events_before should succeed");
    assert_eq!(deleted, 1, "purge should delete exactly 1 remote event (e2, the marker)");

    // Verify e2 is gone and e1/e3 remain
    assert!(storage.get_event(&e2_id).await.unwrap().is_none(), "e2 (marker) should be purged");
    assert!(storage.get_event(&e1_id).await.unwrap().is_some(), "e1 (local) should survive purge");
    assert!(storage.get_event(&e3_id).await.unwrap().is_some(), "e3 (after cutoff) should survive purge");

    // P1-7 assertion: count must still be 1 (only e3), NOT 2 (e1+e3)
    let after_purge =
        storage.get_unread_counts(&room_id, &reader).await.expect("post-purge get_unread_counts should succeed");
    assert_eq!(
        after_purge.notification_count, 1,
        "P1-7: after purge_history deletes the marker event, unread count must NOT bloat. \
         Expected 1 (only e3), got {}. e1_local was already read before marker e2 and must not be recounted.",
        after_purge.notification_count
    );

    // Cleanup
    let _ = sqlx::query("DELETE FROM read_markers WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    let _ = storage.delete_room_events(&room_id).await;
}

// =============================================================================
// P2-14: MSC4242 State DAG — prev_state_events storage and query
// =============================================================================
//
// MSC4242 adds `prev_state_events` to state events, forming a state DAG
// distinct from the room DAG (`prev_events`). These tests verify the storage
// layer can persist and retrieve `prev_state_events`, which is the tracer
// bullet for MSC4242 support.

/// P2-14 RED: A state event created with `prev_state_events` must persist
/// them to the `events.prev_state_events` column and return them via
/// `get_prev_state_events`.
#[tokio::test]
async fn test_p2_14_state_event_stores_prev_state_events() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());

    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!p214a_{}:example.com", suffix);
    let event_id = format!("$p214a_state_{}:example.com", suffix);
    let prev_state_1 = format!("$p214a_prev1_{}:example.com", suffix);
    let prev_state_2 = format!("$p214a_prev2_{}:example.com", suffix);
    let user_id = "@p214sender:example.com";

    // Cleanup
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    // event_edges.prev_event_id has an FK to events.event_id, so the two
    // prev_state rows must already exist before the state event is created.
    for prev_state_id in [&prev_state_1, &prev_state_2] {
        let prev_params = CreateEventParams {
            event_id: prev_state_id.clone(),
            room_id: room_id.clone(),
            user_id: user_id.to_string(),
            event_type: "m.room.member".to_string(),
            content: serde_json::json!({"membership": "join"}),
            state_key: Some(user_id.to_string()),
            origin_server_ts: current_timestamp_millis(),
            redacts: None,
        };
        storage.create_event(prev_params, None).await.expect("prev_state event should be created");
    }

    let params = CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.member".to_string(),
        content: serde_json::json!({"membership": "join"}),
        state_key: Some("@p214sender:example.com".to_string()),
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };

    // Create a state event with prev_state_events (MSC4242).
    let prev_state_events = vec![prev_state_1.clone(), prev_state_2.clone()];
    storage
        .create_state_event_with_dag(params, &[], &[], &prev_state_events, 1, None)
        .await
        .expect("create_state_event_with_dag should succeed");

    // Query back the prev_state_events.
    let result = storage.get_prev_state_events(&event_id).await.expect("get_prev_state_events should succeed");
    assert!(result.is_some(), "get_prev_state_events must return Some for event with prev_state_events");
    let retrieved = result.unwrap();
    assert_eq!(retrieved.len(), 2, "must retrieve exactly 2 prev_state_events");
    assert!(
        retrieved.contains(&prev_state_1),
        "retrieved prev_state_events must contain {prev_state_1}, got {retrieved:?}"
    );
    assert!(
        retrieved.contains(&prev_state_2),
        "retrieved prev_state_events must contain {prev_state_2}, got {retrieved:?}"
    );

    // Cleanup
    let _ = storage.delete_room_events(&room_id).await;
}

/// P2-14: `get_state_dag_edges` must return all (event_id, prev_state_event_id)
/// pairs for a room, forming the complete state DAG edge list.
#[tokio::test]
async fn test_p2_14_get_state_dag_edges_returns_all_edges() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());

    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!p214b_{}:example.com", suffix);
    let user_id = "@p214bsender:example.com";

    // Cleanup
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    // Create 3 state events forming a chain: e3 -> e2 -> e1
    let e1 = format!("$p214b_e1_{}:example.com", suffix);
    let e2 = format!("$p214b_e2_{}:example.com", suffix);
    let e3 = format!("$p214b_e3_{}:example.com", suffix);

    // e1: no prev_state_events (genesis state event)
    storage
        .create_state_event_with_dag(
            CreateEventParams {
                event_id: e1.clone(),
                room_id: room_id.clone(),
                user_id: user_id.to_string(),
                event_type: "m.room.create".to_string(),
                content: serde_json::json!({"creator": user_id}),
                state_key: Some("".to_string()),
                origin_server_ts: 1_000_000,
                redacts: None,
            },
            &[],
            &[],
            &[],
            0,
            None,
        )
        .await
        .unwrap();

    // e2: prev_state_events = [e1]
    storage
        .create_state_event_with_dag(
            CreateEventParams {
                event_id: e2.clone(),
                room_id: room_id.clone(),
                user_id: user_id.to_string(),
                event_type: "m.room.member".to_string(),
                content: serde_json::json!({"membership": "join"}),
                state_key: Some(user_id.to_string()),
                origin_server_ts: 1_000_001,
                redacts: None,
            },
            &[],
            &[],
            std::slice::from_ref(&e1),
            1,
            None,
        )
        .await
        .unwrap();

    // e3: prev_state_events = [e2]
    storage
        .create_state_event_with_dag(
            CreateEventParams {
                event_id: e3.clone(),
                room_id: room_id.clone(),
                user_id: user_id.to_string(),
                event_type: "m.room.power_levels".to_string(),
                content: serde_json::json!({"ban": 50}),
                state_key: Some("".to_string()),
                origin_server_ts: 1_000_002,
                redacts: None,
            },
            &[],
            &[],
            std::slice::from_ref(&e2),
            2,
            None,
        )
        .await
        .unwrap();

    // Query state DAG edges for the room.
    let edges = storage.get_state_dag_edges(&room_id).await.expect("get_state_dag_edges should succeed");

    // e1 has empty prev_state_events (not stored as edges), so only 2 edges:
    // (e2 -> e1) and (e3 -> e2)
    assert_eq!(edges.len(), 2, "expected 2 state DAG edges (e2->e1, e3->e2), got {edges:?}");

    // Verify edge (e2 -> e1)
    assert!(edges.contains(&(e2.clone(), e1.clone())), "edges must contain (e2, e1), got {edges:?}");
    // Verify edge (e3 -> e2)
    assert!(edges.contains(&(e3.clone(), e2.clone())), "edges must contain (e3, e2), got {edges:?}");

    // Cleanup
    let _ = storage.delete_room_events(&room_id).await;
}

/// P2-14: `find_events_referencing_missing_state` must return event IDs whose
/// `prev_state_events` contain any of the missing event IDs. This is the
/// query used by `/get_missing_events` to determine which state DAG events
/// need backfilling (MSC4242 mandates servers fill in unknown prev_state_events).
#[tokio::test]
async fn test_p2_14_find_events_referencing_missing_state() {
    let (_isolated, pool) = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());

    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!p214c_{}:example.com", suffix);
    let user_id = "@p214csender:example.com";

    // Cleanup
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, user_id).await;

    // Create a state event that references a "missing" event (never inserted).
    // We cannot use create_state_event_with_dag because event_edges.prev_event_id
    // has an FK to events.event_id; instead we insert the event directly and
    // then patch its prev_state_events JSONB column (which is what
    // find_events_referencing_missing_state actually reads — it does not query
    // event_edges at all).
    let missing_event = format!("$p214c_missing_{}:example.com", suffix);
    let referencing_event = format!("$p214c_ref_{}:example.com", suffix);
    let now = current_timestamp_millis();

    sqlx::query(
        r"
        INSERT INTO events (event_id, room_id, sender, user_id, event_type, content,
                           origin_server_ts, soft_failed)
        VALUES ($1, $2, $3, $4, $5, $6, $7, false)
        ON CONFLICT DO NOTHING
        ",
    )
    .bind(&referencing_event)
    .bind(&room_id)
    .bind(user_id)
    .bind(user_id)
    .bind("m.room.member")
    .bind(serde_json::json!({"membership": "join"}))
    .bind(now)
    .execute(&*pool)
    .await
    .expect("referencing event should be inserted");

    sqlx::query("UPDATE events SET prev_state_events = $1 WHERE event_id = $2")
        .bind(serde_json::json![&missing_event])
        .bind(&referencing_event)
        .execute(&*pool)
        .await
        .expect("prev_state_events should be set");

    // Query: which events reference the missing event in prev_state_events?
    let result = storage
        .find_events_referencing_missing_state(&room_id, std::slice::from_ref(&missing_event))
        .await
        .expect("find_events_referencing_missing_state should succeed");

    assert_eq!(result.len(), 1, "expected 1 event referencing the missing state event, got {result:?}");
    assert_eq!(result[0], referencing_event, "the referencing event must be returned");

    // Negative test: query for a different missing event → empty result.
    let other_missing = format!("$p214c_other_missing_{}:example.com", suffix);
    let empty_result = storage
        .find_events_referencing_missing_state(&room_id, &[other_missing])
        .await
        .expect("find_events_referencing_missing_state should succeed for non-existent missing");
    assert!(empty_result.is_empty(), "no events should reference a non-existent missing event");

    // Cleanup
    let _ = storage.delete_room_events(&room_id).await;
}

/// B8：`create_event_with_graph` 的 `tx=None` 分支必须原子。
///
/// 注入手段：`fk_event_edges_prev` 外键拒绝不存在的 `prev_event_id`
/// （migrations/00000000_unified_schema_v12.sql:5161-5168）。修复前 events 行先以
/// autocommit 落库、随后 edges 插入失败 ⇒ 留下孤立事件行；修复后整笔回滚。
#[tokio::test]
async fn create_event_with_graph_rolls_back_event_when_edges_insert_fails() {
    let (_guard, pool) = test_pool().await;
    let room_id = "!dag_rollback:example.com";
    ensure_test_room(&pool, room_id).await;
    let storage = EventStorage::new(&pool, test_server_name());

    // 对照组：无 prev_events 时该路径必须成功。否则"失败后没有残留"可能只是第一个
    // INSERT 就失败了，测试会假绿。
    let control = CreateEventParams {
        event_id: "$dag_control:example.com".to_string(),
        room_id: room_id.to_string(),
        user_id: "@test:example.com".to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({ "body": "control" }),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    storage.create_event_with_graph(control, &[], &[], 0, None).await.expect("control insert must succeed");

    let params = CreateEventParams {
        event_id: "$dag_rollback:example.com".to_string(),
        room_id: room_id.to_string(),
        user_id: "@test:example.com".to_string(),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({ "body": "hello" }),
        state_key: None,
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };
    let result =
        storage.create_event_with_graph(params, &["$missing_prev:example.com".to_string()], &[], 1, None).await;
    assert!(result.is_err(), "event_edges 外键失败必须让整笔写入失败");

    let persisted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_id = $1")
        .bind("$dag_rollback:example.com")
        .fetch_one(&*pool)
        .await
        .expect("count events");
    assert_eq!(persisted, 0, "events 行不得在 event_edges 失败后残留（半写窗口）");
}
