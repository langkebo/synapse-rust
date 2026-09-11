use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

use super::*;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

async fn test_pool() -> Arc<Pool<Postgres>> {
    let db_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string());
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(30))
        .connect(&db_url)
        .await
        .expect("Failed to connect to test database");
    Arc::new(pool)
}

async fn ensure_test_user(pool: &Pool<Postgres>, user_id: &str) {
    let now = current_timestamp_millis();
    let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
    sqlx::query(
        r#"INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING"#,
    )
    .bind(user_id)
    .bind(username)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to create test user");
}

async fn ensure_test_room(pool: &Pool<Postgres>, room_id: &str) {
    sqlx::query(
            r#"INSERT INTO rooms (room_id, room_version, is_public, creator, created_ts) VALUES ($1, '10', false, $2, $3) ON CONFLICT (room_id) DO NOTHING"#,
        )
        .bind(room_id)
        .bind("@test:localhost")
        .bind(current_timestamp_millis())
        .execute(pool)
        .await
        .expect("failed to create test room");
}

async fn cleanup_thread_data(pool: &Pool<Postgres>, room_id: &str, thread_id: &str) {
    sqlx::query("DELETE FROM thread_read_receipts WHERE room_id = $1 AND thread_id = $2")
        .bind(room_id)
        .bind(thread_id)
        .execute(pool)
        .await
        .expect("test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure");
    sqlx::query("DELETE FROM thread_subscriptions WHERE room_id = $1 AND thread_id = $2")
        .bind(room_id)
        .bind(thread_id)
        .execute(pool)
        .await
        .expect("test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure");
    sqlx::query("DELETE FROM thread_relations WHERE room_id = $1 AND thread_id = $2")
        .bind(room_id)
        .bind(thread_id)
        .execute(pool)
        .await
        .expect("test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure");
    sqlx::query("DELETE FROM thread_replies WHERE room_id = $1 AND thread_id = $2")
        .bind(room_id)
        .bind(thread_id)
        .execute(pool)
        .await
        .expect("test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure");
    sqlx::query("DELETE FROM thread_roots WHERE room_id = $1 AND thread_id = $2")
        .bind(room_id)
        .bind(thread_id)
        .execute(pool)
        .await
        .expect("test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure");
}

// 1. test_create_thread_root
#[tokio::test]
async fn test_create_thread_root() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_cr_{suffix}:localhost");
    let thread_id = format!("thread-cr-{suffix}:localhost");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    let root = storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create thread root");

    assert!(root.id > 0);
    assert_eq!(root.room_id, room_id);
    assert_eq!(root.thread_id, Some(thread_id.clone()));
    assert_eq!(root.reply_count, Some(0));
    assert!(!root.is_fetched);

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 2. test_get_thread_root_found
#[tokio::test]
async fn test_get_thread_root_found() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_gt_{suffix}:localhost");
    let thread_id = format!("thread-gt-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create thread root");

    let found = storage.get_thread_root(&room_id, &thread_id).await.expect("query should succeed");

    assert!(found.is_some());
    let root = found.unwrap();
    assert_eq!(root.room_id, room_id);
    assert_eq!(root.thread_id, Some(thread_id.clone()));

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 3. test_get_thread_root_not_found
#[tokio::test]
async fn test_get_thread_root_not_found() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);

    let result =
        storage.get_thread_root("!nonexistent:localhost", "nonexistent-thread").await.expect("query should succeed");

    assert!(result.is_none(), "nonexistent thread should return None");
}

// 4. test_get_thread_root_by_event
#[tokio::test]
async fn test_get_thread_root_by_event() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_gte_{suffix}:localhost");
    let thread_id = format!("thread-gte-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create thread root");

    let found = storage
        .get_thread_root_by_event(&room_id, &format!("$root_{suffix}:localhost"))
        .await
        .expect("query should succeed");

    assert!(found.is_some());
    let root = found.unwrap();
    assert_eq!(root.root_event_id, format!("$root_{suffix}:localhost"));

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 5. test_list_thread_roots
#[tokio::test]
async fn test_list_thread_roots() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_lt_{suffix}:localhost");
    let t1 = format!("thread-lt-a-{suffix}");
    let t2 = format!("thread-lt-b-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &t1).await;
    cleanup_thread_data(&pool, &room_id, &t2).await;

    for tid in &[&t1, &t2] {
        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$ev_{}", tid),
                sender: "@sender:localhost".to_string(),
                thread_id: Some((*tid).clone()),
            })
            .await
            .expect("should create thread root");
    }

    let roots = storage
        .list_thread_roots(ThreadListParams {
            room_id: room_id.clone(),
            limit: Some(10),
            from: None,
            include_all: false,
        })
        .await
        .expect("should list roots");

    assert!(roots.len() >= 2);

    cleanup_thread_data(&pool, &room_id, &t1).await;
    cleanup_thread_data(&pool, &room_id, &t2).await;
}

// 6. test_list_all_thread_roots
#[tokio::test]
async fn test_list_all_thread_roots() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_la_{suffix}:localhost");
    let thread_id = format!("thread-la-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create thread root");

    let roots = storage.list_all_thread_roots(Some(10), None).await.expect("should list all roots");

    assert!(!roots.is_empty());

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 7. test_create_thread_reply
#[tokio::test]
async fn test_create_thread_reply() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_rp_{suffix}:localhost");
    let thread_id = format!("thread-rp-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create thread root");

    let ts = current_timestamp_millis();
    let reply = storage
        .create_thread_reply(CreateThreadReplyParams {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            event_id: format!("$reply_{suffix}:localhost"),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@replier:localhost".to_string(),
            in_reply_to_event_id: Some(format!("$root_{suffix}:localhost")),
            content: serde_json::json!({"body": "test reply"}),
            origin_server_ts: ts,
        })
        .await
        .expect("should create reply");

    assert!(reply.id > 0);
    assert_eq!(reply.thread_id, thread_id);
    assert_eq!(reply.sender, "@replier:localhost");
    assert!(!reply.is_edited);
    assert!(!reply.is_redacted);

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 8. test_get_thread_replies
#[tokio::test]
async fn test_get_thread_replies() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_gtr_{suffix}:localhost");
    let thread_id = format!("thread-gtr-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create thread root");

    let ts1 = current_timestamp_millis();
    let ts2 = ts1 + 1;
    storage
        .create_thread_reply(CreateThreadReplyParams {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            event_id: format!("$r1_{suffix}:localhost"),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@r1:localhost".to_string(),
            in_reply_to_event_id: None,
            content: serde_json::json!({"body": "first reply"}),
            origin_server_ts: ts1,
        })
        .await
        .expect("should create reply1");
    storage
        .create_thread_reply(CreateThreadReplyParams {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            event_id: format!("$r2_{suffix}:localhost"),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@r2:localhost".to_string(),
            in_reply_to_event_id: None,
            content: serde_json::json!({"body": "second reply"}),
            origin_server_ts: ts2,
        })
        .await
        .expect("should create reply2");

    let replies = storage.get_thread_replies(&room_id, &thread_id, Some(10), None).await.expect("should get replies");

    assert!(replies.len() >= 2);

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 9. test_get_reply_count
#[tokio::test]
async fn test_get_reply_count() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_rc_{suffix}:localhost");
    let thread_id = format!("thread-rc-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    // Zero when no thread exists yet
    let count0 = storage.get_reply_count(&room_id, &thread_id).await.expect("should get count");
    assert_eq!(count0, 0);

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    storage
        .create_thread_reply(CreateThreadReplyParams {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            event_id: format!("$r1_{suffix}:localhost"),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@r1:localhost".to_string(),
            in_reply_to_event_id: None,
            content: serde_json::json!({"body": "reply1"}),
            origin_server_ts: current_timestamp_millis(),
        })
        .await
        .expect("should create reply1");

    let count1 = storage.get_reply_count(&room_id, &thread_id).await.expect("should get count");
    assert_eq!(count1, 1);

    storage
        .create_thread_reply(CreateThreadReplyParams {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            event_id: format!("$r2_{suffix}:localhost"),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@r2:localhost".to_string(),
            in_reply_to_event_id: None,
            content: serde_json::json!({"body": "reply2"}),
            origin_server_ts: current_timestamp_millis(),
        })
        .await
        .expect("should create reply2");

    let count2 = storage.get_reply_count(&room_id, &thread_id).await.expect("should get count");
    assert_eq!(count2, 2);

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 10. test_subscribe_to_thread
#[tokio::test]
async fn test_subscribe_to_thread() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_sub_{suffix}:localhost");
    let thread_id = format!("thread-sub-{suffix}");
    let user_id = format!("@user_sub_{suffix}:localhost");

    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: user_id.clone(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create thread root");

    let sub = storage.subscribe_to_thread(&room_id, &thread_id, &user_id, "all").await.expect("should subscribe");

    assert!(sub.id > 0);
    assert_eq!(sub.room_id, room_id);
    assert_eq!(sub.thread_id, thread_id);
    assert_eq!(sub.user_id, user_id);
    assert_eq!(sub.notification_level, "all");
    assert!(!sub.is_muted);

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 11. test_unsubscribe_from_thread
#[tokio::test]
async fn test_unsubscribe_from_thread() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_unsub_{suffix}:localhost");
    let thread_id = format!("thread-unsub-{suffix}");
    let user_id = format!("@user_unsub_{suffix}:localhost");

    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: user_id.clone(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    storage.subscribe_to_thread(&room_id, &thread_id, &user_id, "all").await.expect("should subscribe");

    storage.unsubscribe_from_thread(&room_id, &thread_id, &user_id).await.expect("should unsubscribe");

    let sub = storage.get_thread_subscription(&room_id, &thread_id, &user_id).await.expect("query should succeed");

    assert!(sub.is_none(), "subscription should be removed after unsubscribe");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 12. test_get_thread_subscription — found / not found
#[tokio::test]
async fn test_get_thread_subscription() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_gsub_{suffix}:localhost");
    let thread_id = format!("thread-gsub-{suffix}");
    let user_id = format!("@user_gsub_{suffix}:localhost");

    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    // Not found initially
    let sub = storage.get_thread_subscription(&room_id, &thread_id, &user_id).await.expect("query should succeed");
    assert!(sub.is_none(), "should not exist before subscribe");

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: user_id.clone(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    storage.subscribe_to_thread(&room_id, &thread_id, &user_id, "all").await.expect("should subscribe");

    // Found after subscription
    let sub = storage.get_thread_subscription(&room_id, &thread_id, &user_id).await.expect("query should succeed");
    assert!(sub.is_some(), "should exist after subscribe");
    let sub = sub.unwrap();
    assert_eq!(sub.notification_level, "all");
    assert!(!sub.is_muted);

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 13. test_get_user_thread_subscriptions
#[tokio::test]
async fn test_get_user_thread_subscriptions() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_uts_{suffix}:localhost");
    let user_id = format!("@user_uts_{suffix}:localhost");
    let t1 = format!("thread-uts-a-{suffix}");
    let t2 = format!("thread-uts-b-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;
    cleanup_thread_data(&pool, &room_id, &t1).await;
    cleanup_thread_data(&pool, &room_id, &t2).await;

    for tid in &[&t1, &t2] {
        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$ev_{}", tid),
                sender: user_id.clone(),
                thread_id: Some((*tid).clone()),
            })
            .await
            .expect("should create root");
        storage.subscribe_to_thread(&room_id, tid, &user_id, "all").await.expect("should subscribe");
    }

    let subs = storage.get_user_thread_subscriptions(&user_id, Some(10), None).await.expect("should get subscriptions");

    assert!(subs.len() >= 2, "expected at least 2 subscriptions, got {}", subs.len());

    cleanup_thread_data(&pool, &room_id, &t1).await;
    cleanup_thread_data(&pool, &room_id, &t2).await;
}

// 14. test_update_read_receipt
#[tokio::test]
async fn test_update_read_receipt() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_urr_{suffix}:localhost");
    let thread_id = format!("thread-urr-{suffix}");
    let user_id = format!("@user_urr_{suffix}:localhost");

    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: user_id.clone(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create thread root");

    let ts = current_timestamp_millis();
    let receipt = storage
        .update_read_receipt(&room_id, &thread_id, &user_id, "$event_last", ts)
        .await
        .expect("should update receipt");

    assert!(receipt.id > 0);
    assert_eq!(receipt.room_id, room_id);
    assert_eq!(receipt.thread_id, thread_id);
    assert_eq!(receipt.user_id, user_id);
    assert_eq!(receipt.last_read_event_id, Some("$event_last".to_string()));
    assert_eq!(receipt.unread_count, 0);

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 15. test_get_read_receipt
#[tokio::test]
async fn test_get_read_receipt() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_grr_{suffix}:localhost");
    let thread_id = format!("thread-grr-{suffix}");
    let user_id = format!("@user_grr_{suffix}:localhost");

    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    // Not found initially
    let rr = storage.get_read_receipt(&room_id, &thread_id, &user_id).await.expect("query should succeed");
    assert!(rr.is_none(), "read receipt should not exist initially");

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: user_id.clone(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    storage
        .update_read_receipt(&room_id, &thread_id, &user_id, "$event_123", current_timestamp_millis())
        .await
        .expect("should update receipt");

    let rr = storage.get_read_receipt(&room_id, &thread_id, &user_id).await.expect("query should succeed");

    assert!(rr.is_some(), "read receipt should exist after update");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 16. test_delete_thread
#[tokio::test]
async fn test_delete_thread() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_del_{suffix}:localhost");
    let thread_id = format!("thread-del-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create thread root");

    // Create a reply too, to verify cascading delete
    storage
        .create_thread_reply(CreateThreadReplyParams {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            event_id: format!("$reply_{suffix}:localhost"),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@replier:localhost".to_string(),
            in_reply_to_event_id: None,
            content: serde_json::json!({"body": "reply"}),
            origin_server_ts: current_timestamp_millis(),
        })
        .await
        .expect("should create reply");

    // Verify thread and reply exist before delete
    assert!(storage.get_thread_root(&room_id, &thread_id).await.unwrap().is_some());
    assert_eq!(storage.get_reply_count(&room_id, &thread_id).await.unwrap(), 1);

    storage.delete_thread(&room_id, &thread_id).await.expect("should delete");

    // Root should be gone
    let root = storage.get_thread_root(&room_id, &thread_id).await.expect("query should succeed");
    assert!(root.is_none(), "thread root should be deleted");

    // Reply should be gone
    let count = storage.get_reply_count(&room_id, &thread_id).await.expect("query should succeed");
    assert_eq!(count, 0, "replies should be deleted");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// === Helper: insert a minimal event row for search_threads tests ===
async fn insert_test_event(
    pool: &Pool<Postgres>,
    event_id: &str,
    room_id: &str,
    sender: &str,
    body: &str,
    origin_server_ts: i64,
) {
    sqlx::query(
        r#"INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts)
               VALUES ($1, $2, $3, 'm.room.message', $4, $5)
               ON CONFLICT (event_id) DO UPDATE SET content = EXCLUDED.content"#,
    )
    .bind(event_id)
    .bind(room_id)
    .bind(sender)
    .bind(serde_json::json!({"body": body}))
    .bind(origin_server_ts)
    .execute(pool)
    .await
    .expect("failed to insert test event");
}

// 17. test_get_thread_participants
#[tokio::test]
async fn test_get_thread_participants() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_gp_{suffix}:localhost");
    let thread_id = format!("thread-gp-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    // Create root with sender A
    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@senderA:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    // Create reply from sender B
    storage
        .create_thread_reply(CreateThreadReplyParams {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            event_id: format!("$reply_{suffix}:localhost"),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@senderB:localhost".to_string(),
            in_reply_to_event_id: None,
            content: serde_json::json!({"body": "reply"}),
            origin_server_ts: current_timestamp_millis(),
        })
        .await
        .expect("should create reply");

    let participants = storage.get_thread_participants(&room_id, &thread_id).await.expect("should get participants");
    assert!(participants.contains(&"@senderA:localhost".to_string()), "should include root sender");
    assert!(participants.contains(&"@senderB:localhost".to_string()), "should include reply sender");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 18. test_mute_thread (creates a new muted subscription)
#[tokio::test]
async fn test_mute_thread() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_mt_{suffix}:localhost");
    let thread_id = format!("thread-mt-{suffix}");
    let user_id = format!("@user_mt_{suffix}:localhost");

    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: user_id.clone(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    let muted = storage.mute_thread(&room_id, &thread_id, &user_id).await.expect("should mute thread");
    assert!(muted.is_muted, "subscription should be muted");
    assert_eq!(muted.notification_level, "none");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 19. test_mute_thread_updates_existing_subscription
#[tokio::test]
async fn test_mute_thread_updates_existing_subscription() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_mt2_{suffix}:localhost");
    let thread_id = format!("thread-mt2-{suffix}");
    let user_id = format!("@user_mt2_{suffix}:localhost");

    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: user_id.clone(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    // Subscribe first (not muted)
    let sub = storage.subscribe_to_thread(&room_id, &thread_id, &user_id, "all").await.expect("should subscribe");
    assert!(!sub.is_muted);

    // Now mute — should update the existing subscription
    let muted = storage.mute_thread(&room_id, &thread_id, &user_id).await.expect("should mute");
    assert!(muted.is_muted, "subscription should now be muted");

    // Verify via get_thread_subscription
    let fetched = storage.get_thread_subscription(&room_id, &thread_id, &user_id).await.unwrap().unwrap();
    assert!(fetched.is_muted, "fetched subscription should be muted");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 20. test_increment_unread_count (from 0 to 1)
#[tokio::test]
async fn test_increment_unread_count() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_iu_{suffix}:localhost");
    let thread_id = format!("thread-iu-{suffix}");
    let user_id = format!("@user_iu_{suffix}:localhost");

    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    // No receipt initially
    let before = storage.get_read_receipt(&room_id, &thread_id, &user_id).await.unwrap();
    assert!(before.is_none());

    // Increment once
    storage.increment_unread_count(&room_id, &thread_id, &user_id).await.expect("should increment");

    let after = storage.get_read_receipt(&room_id, &thread_id, &user_id).await.unwrap().unwrap();
    assert_eq!(after.unread_count, 1, "unread count should be 1 after first increment");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 21. test_increment_unread_count_accumulates
#[tokio::test]
async fn test_increment_unread_count_accumulates() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_iu2_{suffix}:localhost");
    let thread_id = format!("thread-iu2-{suffix}");
    let user_id = format!("@user_iu2_{suffix}:localhost");

    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    // Increment 3 times
    for _ in 0..3 {
        storage.increment_unread_count(&room_id, &thread_id, &user_id).await.expect("should increment");
    }

    let after = storage.get_read_receipt(&room_id, &thread_id, &user_id).await.unwrap().unwrap();
    assert_eq!(after.unread_count, 3, "unread count should accumulate to 3");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 22. test_create_thread_relation
#[tokio::test]
async fn test_create_thread_relation() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_tr_{suffix}:localhost");
    let thread_id = format!("thread-tr-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    let event_id = format!("$evt_{suffix}:localhost");
    let relates_to = format!("$root_{suffix}:localhost");

    let relation = storage
        .create_thread_relation(&room_id, &event_id, &relates_to, "m.thread", Some(&thread_id), false)
        .await
        .expect("should create relation");

    assert!(relation.id > 0);
    assert_eq!(relation.room_id, room_id);
    assert_eq!(relation.event_id, event_id);
    assert_eq!(relation.relates_to_event_id, relates_to);
    assert_eq!(relation.relation_type, "m.thread");
    assert_eq!(relation.thread_id, Some(thread_id.clone()));
    assert!(!relation.is_falling_back);

    // Cleanup the relation (cleanup_thread_data doesn't delete relations by thread_id when thread_id is None in the row)
    let _ = sqlx::query("DELETE FROM thread_relations WHERE room_id = $1 AND event_id = $2")
        .bind(&room_id)
        .bind(&event_id)
        .execute(&*pool)
        .await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 23. test_create_thread_relation_with_fallback
#[tokio::test]
async fn test_create_thread_relation_with_fallback() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_trfb_{suffix}:localhost");
    let thread_id = format!("thread-trfb-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    let event_id = format!("$evt_fb_{suffix}:localhost");
    let relates_to = format!("$root_fb_{suffix}:localhost");

    let relation = storage
        .create_thread_relation(&room_id, &event_id, &relates_to, "m.thread", Some(&thread_id), true)
        .await
        .expect("should create relation with fallback");

    assert!(relation.is_falling_back, "relation should have is_falling_back=true");

    let _ = sqlx::query("DELETE FROM thread_relations WHERE room_id = $1 AND event_id = $2")
        .bind(&room_id)
        .bind(&event_id)
        .execute(&*pool)
        .await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 24. test_mark_reply_edited
#[tokio::test]
async fn test_mark_reply_edited() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_me_{suffix}:localhost");
    let thread_id = format!("thread-me-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    let event_id = format!("$reply_{suffix}:localhost");
    storage
        .create_thread_reply(CreateThreadReplyParams {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            event_id: event_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@replier:localhost".to_string(),
            in_reply_to_event_id: None,
            content: serde_json::json!({"body": "original"}),
            origin_server_ts: current_timestamp_millis(),
        })
        .await
        .expect("should create reply");

    // Verify not edited initially
    let replies = storage.get_thread_replies(&room_id, &thread_id, Some(10), None).await.unwrap();
    let reply = replies.iter().find(|r| r.event_id == event_id).unwrap();
    assert!(!reply.is_edited);

    // Mark as edited
    storage.mark_reply_edited(&room_id, &event_id).await.expect("should mark edited");

    // Verify edited
    let replies = storage.get_thread_replies(&room_id, &thread_id, Some(10), None).await.unwrap();
    let reply = replies.iter().find(|r| r.event_id == event_id).unwrap();
    assert!(reply.is_edited, "reply should be marked as edited");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 25. test_mark_reply_redacted
#[tokio::test]
async fn test_mark_reply_redacted() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_mr_{suffix}:localhost");
    let thread_id = format!("thread-mr-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    let event_id = format!("$reply_{suffix}:localhost");
    storage
        .create_thread_reply(CreateThreadReplyParams {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            event_id: event_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@replier:localhost".to_string(),
            in_reply_to_event_id: None,
            content: serde_json::json!({"body": "original content"}),
            origin_server_ts: current_timestamp_millis(),
        })
        .await
        .expect("should create reply");

    // Verify not redacted initially
    let replies = storage.get_thread_replies(&room_id, &thread_id, Some(10), None).await.unwrap();
    let reply = replies.iter().find(|r| r.event_id == event_id).unwrap();
    assert!(!reply.is_redacted);

    // Mark as redacted
    storage.mark_reply_redacted(&room_id, &event_id).await.expect("should mark redacted");

    // Verify redacted and content cleared
    let replies = storage.get_thread_replies(&room_id, &thread_id, Some(10), None).await.unwrap();
    let reply = replies.iter().find(|r| r.event_id == event_id).unwrap();
    assert!(reply.is_redacted, "reply should be marked as redacted");
    assert_eq!(reply.content, serde_json::json!({}), "content should be cleared to empty object");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 26. test_get_threads_with_unread_with_room_id
#[tokio::test]
async fn test_get_threads_with_unread_with_room_id() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_twu_{suffix}:localhost");
    let thread_id = format!("thread-twu-{suffix}");
    let user_id = format!("@user_twu_{suffix}:localhost");

    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    // No unread threads initially
    let unread = storage.get_threads_with_unread(&user_id, Some(&room_id)).await.expect("should query");
    assert!(
        unread.iter().all(|u| u.room_id != room_id || u.thread_id != thread_id),
        "should not contain our thread yet"
    );

    // Increment unread
    storage.increment_unread_count(&room_id, &thread_id, &user_id).await.expect("should increment");

    // Now should appear
    let unread = storage.get_threads_with_unread(&user_id, Some(&room_id)).await.expect("should query");
    assert!(unread.iter().any(|u| u.room_id == room_id && u.thread_id == thread_id), "should contain our thread");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 27. test_get_threads_with_unread_without_room_id
#[tokio::test]
async fn test_get_threads_with_unread_without_room_id() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_twu2_{suffix}:localhost");
    let thread_id = format!("thread-twu2-{suffix}");
    let user_id = format!("@user_twu2_{suffix}:localhost");

    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    // Increment unread
    storage.increment_unread_count(&room_id, &thread_id, &user_id).await.expect("should increment");

    // Query across all rooms
    let unread = storage.get_threads_with_unread(&user_id, None).await.expect("should query");
    assert!(unread.iter().any(|u| u.room_id == room_id && u.thread_id == thread_id), "should contain our thread");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 28. test_get_thread_summary
#[tokio::test]
async fn test_get_thread_summary() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_ts_{suffix}:localhost");
    let thread_id = format!("thread-ts-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@root_sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    // Add a reply
    storage
        .create_thread_reply(CreateThreadReplyParams {
            room_id: room_id.clone(),
            thread_id: thread_id.clone(),
            event_id: format!("$reply_{suffix}:localhost"),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@reply_sender:localhost".to_string(),
            in_reply_to_event_id: None,
            content: serde_json::json!({"body": "a reply"}),
            origin_server_ts: current_timestamp_millis(),
        })
        .await
        .expect("should create reply");

    let summary = storage.get_thread_summary(&room_id, &thread_id).await.expect("should get summary");
    assert!(summary.is_some(), "summary should exist for existing thread");
    let s = summary.unwrap();
    assert_eq!(s.room_id, room_id);
    assert_eq!(s.thread_id, thread_id);
    assert_eq!(s.root_sender, "@root_sender:localhost");
    assert_eq!(s.reply_count, 1, "should have 1 reply");
    assert!(s.latest_event_id.is_some(), "should have a latest reply event_id");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 29. test_get_thread_summary_not_found
#[tokio::test]
async fn test_get_thread_summary_not_found() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);

    let summary =
        storage.get_thread_summary("!nonexistent:localhost", "nonexistent-thread").await.expect("query should succeed");
    assert!(summary.is_none(), "nonexistent thread should return None");
}

// 30. test_get_thread_statistics
#[tokio::test]
async fn test_get_thread_statistics() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_tstat_{suffix}:localhost");
    let thread_id = format!("thread-tstat-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    let ts = current_timestamp_millis();
    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@root_sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    // Add 2 replies
    for i in 0..2 {
        storage
            .create_thread_reply(CreateThreadReplyParams {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: format!("$reply_{i}_{suffix}:localhost"),
                root_event_id: format!("$root_{suffix}:localhost"),
                sender: format!("@replyer{i}:localhost"),
                in_reply_to_event_id: None,
                content: serde_json::json!({"body": "reply"}),
                origin_server_ts: ts + i,
            })
            .await
            .expect("should create reply");
    }

    let stats = storage.get_thread_statistics(&room_id, &thread_id).await.expect("should get statistics");
    assert!(stats.is_some(), "statistics should exist for existing thread");
    let s = stats.unwrap();
    assert_eq!(s.room_id, room_id);
    assert_eq!(s.total_replies, 2, "should have 2 replies");
    assert!(s.total_participants >= 2, "should have at least 2 participants (root + at least 1 replyer)");
    assert!(s.last_reply_ts.is_some(), "should have a last_reply_ts");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 31. test_get_thread_statistics_not_found
#[tokio::test]
async fn test_get_thread_statistics_not_found() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);

    let stats = storage
        .get_thread_statistics("!nonexistent:localhost", "nonexistent-thread")
        .await
        .expect("query should succeed");
    assert!(stats.is_none(), "nonexistent thread should return None");
}

// 32. test_search_threads_finds_match
#[tokio::test]
async fn test_search_threads_finds_match() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_st_{suffix}:localhost");
    let thread_id = format!("thread-st-{suffix}");
    let root_event_id = format!("$root_st_{suffix}:localhost");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    // Insert an event with searchable body content
    let ts = current_timestamp_millis();
    insert_test_event(&pool, &root_event_id, &room_id, "@sender:localhost", "UniqueSearchableKeyword content", ts)
        .await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: root_event_id.clone(),
            sender: "@sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    // Search for the unique keyword
    let results =
        storage.search_threads(&room_id, "UniqueSearchableKeyword", Some(10)).await.expect("search should succeed");
    assert!(results.iter().any(|s| s.thread_id == thread_id), "should find our thread by keyword");

    // Cleanup the event
    let _ = sqlx::query("DELETE FROM events WHERE event_id = $1").bind(&root_event_id).execute(&*pool).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 33. test_search_threads_no_match
#[tokio::test]
async fn test_search_threads_no_match() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_stnm_{suffix}:localhost");
    let thread_id = format!("thread-stnm-{suffix}");
    let root_event_id = format!("$root_stnm_{suffix}:localhost");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    let ts = current_timestamp_millis();
    insert_test_event(&pool, &root_event_id, &room_id, "@sender:localhost", "some body text", ts).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: root_event_id.clone(),
            sender: "@sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    // Search for a keyword that doesn't match
    let results =
        storage.search_threads(&room_id, "ZZZNoMatchAtAllZZZ", Some(10)).await.expect("search should succeed");
    assert!(results.iter().all(|s| s.thread_id != thread_id), "should not find our thread with non-matching query");

    let _ = sqlx::query("DELETE FROM events WHERE event_id = $1").bind(&root_event_id).execute(&*pool).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 34. test_freeze_thread
#[tokio::test]
async fn test_freeze_thread() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_fr_{suffix}:localhost");
    let thread_id = format!("thread-fr-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    // Verify not frozen initially
    let root = storage.get_thread_root(&room_id, &thread_id).await.unwrap().unwrap();
    assert!(!root.is_fetched, "thread should not be frozen initially");

    // Freeze
    storage.freeze_thread(&room_id, &thread_id).await.expect("should freeze");

    // Verify frozen (is_fetched maps to is_frozen in summaries)
    let root = storage.get_thread_root(&room_id, &thread_id).await.unwrap().unwrap();
    assert!(root.is_fetched, "thread should be frozen (is_fetched=true) after freeze_thread");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 35. test_unfreeze_thread
#[tokio::test]
async fn test_unfreeze_thread() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_uf_{suffix}:localhost");
    let thread_id = format!("thread-uf-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    // Freeze first
    storage.freeze_thread(&room_id, &thread_id).await.expect("should freeze");
    let root = storage.get_thread_root(&room_id, &thread_id).await.unwrap().unwrap();
    assert!(root.is_fetched, "should be frozen after freeze");

    // Unfreeze
    storage.unfreeze_thread(&room_id, &thread_id).await.expect("should unfreeze");

    // Verify unfrozen
    let root = storage.get_thread_root(&room_id, &thread_id).await.unwrap().unwrap();
    assert!(!root.is_fetched, "thread should not be frozen after unfreeze_thread");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}

// 36. test_list_thread_roots_with_from_cursor
#[tokio::test]
async fn test_list_thread_roots_with_from_cursor() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_ltf_{suffix}:localhost");
    // Use sortable thread IDs so the `from` cursor (thread_id > $2) works deterministically
    let t1 = format!("aaa-thread-ltf-{suffix}");
    let t2 = format!("bbb-thread-ltf-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &t1).await;
    cleanup_thread_data(&pool, &room_id, &t2).await;

    for tid in [&t1, &t2] {
        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$ev_{}", tid),
                sender: "@sender:localhost".to_string(),
                thread_id: Some((*tid).to_string()),
            })
            .await
            .expect("should create root");
    }

    // First page: get all roots ordered by thread_id ASC, take the first one
    let first_page = storage
        .list_thread_roots(ThreadListParams {
            room_id: room_id.clone(),
            limit: Some(1),
            from: None,
            include_all: false,
        })
        .await
        .expect("first page should succeed");
    assert_eq!(first_page.len(), 1, "first page should have 1 root");
    let first_tid = first_page[0].thread_id.clone().unwrap();

    // Second page: use the first thread_id as the `from` cursor
    let second_page = storage
        .list_thread_roots(ThreadListParams {
            room_id: room_id.clone(),
            limit: Some(10),
            from: Some(first_tid.clone()),
            include_all: false,
        })
        .await
        .expect("second page should succeed");
    assert!(
        second_page.iter().all(|r| r.thread_id.as_deref().unwrap_or("") > first_tid.as_str()),
        "all second-page roots should have thread_id > from cursor"
    );
    assert!(second_page.iter().any(|r| r.thread_id.as_deref() == Some(t2.as_str())), "second page should contain t2");

    cleanup_thread_data(&pool, &room_id, &t1).await;
    cleanup_thread_data(&pool, &room_id, &t2).await;
}

// 37. test_list_all_thread_roots_with_from_cursor
#[tokio::test]
async fn test_list_all_thread_roots_with_from_cursor() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_latf_{suffix}:localhost");
    let t1 = format!("aaa-thread-latf-{suffix}");
    let t2 = format!("bbb-thread-latf-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &t1).await;
    cleanup_thread_data(&pool, &room_id, &t2).await;

    for tid in [&t1, &t2] {
        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$ev_{}", tid),
                sender: "@sender:localhost".to_string(),
                thread_id: Some((*tid).to_string()),
            })
            .await
            .expect("should create root");
    }

    // First page: limit=1, no cursor
    let first_page = storage.list_all_thread_roots(Some(1), None).await.expect("first page should succeed");
    assert_eq!(first_page.len(), 1, "first page should have 1 root");
    let first_tid = first_page[0].thread_id.clone().unwrap();

    // Second page: use first thread_id as from cursor
    let second_page =
        storage.list_all_thread_roots(Some(10), Some(first_tid.clone())).await.expect("second page should succeed");
    assert!(
        second_page.iter().all(|r| r.thread_id.as_deref().unwrap_or("") > first_tid.as_str()),
        "all second-page roots should have thread_id > from cursor"
    );
    assert!(second_page.iter().any(|r| r.thread_id.as_deref() == Some(t2.as_str())), "second page should contain t2");

    cleanup_thread_data(&pool, &room_id, &t1).await;
    cleanup_thread_data(&pool, &room_id, &t2).await;
}

// 38. test_get_thread_replies_with_from_cursor
#[tokio::test]
async fn test_get_thread_replies_with_from_cursor() {
    let pool = test_pool().await;
    let storage = ThreadStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!room_gtrf_{suffix}:localhost");
    let thread_id = format!("thread-gtrf-{suffix}");

    ensure_test_room(&pool, &room_id).await;
    cleanup_thread_data(&pool, &room_id, &thread_id).await;

    storage
        .create_thread_root(CreateThreadRootParams {
            room_id: room_id.clone(),
            root_event_id: format!("$root_{suffix}:localhost"),
            sender: "@sender:localhost".to_string(),
            thread_id: Some(thread_id.clone()),
        })
        .await
        .expect("should create root");

    // Create two replies with deterministic event_ids for cursor pagination
    // The `from` cursor filters by event_id > $3, so use sortable IDs
    let event_a = format!("aaa_reply_{suffix}");
    let event_b = format!("bbb_reply_{suffix}");
    let ts = current_timestamp_millis();
    for (i, eid) in [event_a.clone(), event_b.clone()].iter().enumerate() {
        storage
            .create_thread_reply(CreateThreadReplyParams {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: eid.clone(),
                root_event_id: format!("$root_{suffix}:localhost"),
                sender: "@replier:localhost".to_string(),
                in_reply_to_event_id: None,
                content: serde_json::json!({"body": "reply"}),
                origin_server_ts: ts + i as i64,
            })
            .await
            .expect("should create reply");
    }

    // First page: get all replies, take the first one (ordered by origin_server_ts ASC)
    let first_page =
        storage.get_thread_replies(&room_id, &thread_id, Some(1), None).await.expect("first page should succeed");
    assert_eq!(first_page.len(), 1, "first page should have 1 reply");
    let first_eid = first_page[0].event_id.clone();

    // Second page: use first event_id as from cursor
    let second_page = storage
        .get_thread_replies(&room_id, &thread_id, Some(10), Some(first_eid.clone()))
        .await
        .expect("second page should succeed");
    assert!(
        second_page.iter().all(|r| r.event_id > first_eid),
        "all second-page replies should have event_id > from cursor"
    );
    assert_eq!(second_page.len(), 1, "second page should have the remaining 1 reply");

    cleanup_thread_data(&pool, &room_id, &thread_id).await;
}
