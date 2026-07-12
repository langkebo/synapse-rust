use super::*;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

async fn test_pool() -> Arc<Pool<Postgres>> {
    let db_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
    let pool =
        PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
    Arc::new(pool)
}

async fn ensure_test_room(pool: &Pool<Postgres>, room_id: &str) {
    let now = chrono::Utc::now().timestamp_millis();
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
    let now = chrono::Utc::now().timestamp_millis();
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
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
        redacts: None,
    };

    let event = storage.create_event(params, None).await.expect("create_event should succeed");
    assert_eq!(event.event_id, event_id);
    assert_eq!(event.room_id, room_id);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_event_found() {
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let result = storage.get_event("$nonexistent:example.com").await.expect("get_event should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_room_events_returns_list() {
    let pool = test_pool().await;
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
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    let after = storage.count_room_events(&room_id).await.expect("count should succeed");
    assert!(after > before);

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_room_events_paginated() {
    let pool = test_pool().await;
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

#[tokio::test]
async fn test_delete_room_events() {
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    storage.delete_room_events(&room_id).await.expect("delete_room_events should succeed");
    let count = storage.count_room_events(&room_id).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_get_room_message_count() {
    let pool = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let count =
        storage.get_room_message_count("!any:example.com").await.expect("get_room_message_count should succeed");
    assert!(count >= 0);
}

// --- Ephemeral events, reporting, redaction, signatures ---

#[tokio::test]
async fn test_ephemeral_event_crud() {
    let pool = test_pool().await;
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

    let now = chrono::Utc::now().timestamp_millis();
    let events = storage.get_ephemeral_events(&room_id, now, 10).await.expect("get_ephemeral_events should succeed");
    assert!(!events.is_empty());

    storage.delete_ephemeral_event(&room_id, "m.typing", user_id).await.expect("delete_ephemeral_event should succeed");

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_report_event() {
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!redact_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$redact_{}:example.com", uuid::Uuid::new_v4());
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    storage.redact_event_content(&event_id, Some(user_id)).await.expect("redact_event_content should succeed");

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_save_and_get_event_signatures() {
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    let now = chrono::Utc::now().timestamp_millis();
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
    let pool = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let input = vec![format!("$missing_{}:example.com", uuid::Uuid::new_v4())];
    let missing = storage.find_missing_event_ids(&input).await.expect("find_missing_event_ids should succeed");
    assert_eq!(missing.len(), 1);
}

#[tokio::test]
async fn test_get_total_message_count() {
    let pool = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let count = storage.get_total_message_count().await.expect("get_total_message_count should succeed");
    assert!(count >= 0);
}

#[tokio::test]
async fn test_get_daily_message_count() {
    let pool = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let count = storage.get_daily_message_count().await.expect("get_daily_message_count should succeed");
    assert!(count >= 0);
}

#[tokio::test]
async fn test_delete_events_before() {
    let pool = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let room_id = format!("!evt_old_{}:example.com", uuid::Uuid::new_v4());

    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id).await;

    // Delete events before a far-future timestamp — should succeed even if 0 rows
    let _deleted = storage
        .delete_events_before(&room_id, chrono::Utc::now().timestamp_millis() + 86400000)
        .await
        .expect("delete_events_before should succeed");

    let _ = storage.delete_room_events(&room_id).await;
}

#[tokio::test]
async fn test_get_room_create_event_none_for_non_existent() {
    let pool = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let result =
        storage.get_room_create_event("!nonexistent:example.com").await.expect("get_room_create_event should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_events_batch_empty_input() {
    let pool = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let results = storage.get_events_batch(&[]).await.expect("get_events_batch should succeed");
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_get_forward_extremities_count() {
    let pool = test_pool().await;
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
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
        redacts: None,
    };
    storage
        .create_event_with_graph(child_params, &[parent_id.clone()], &[], 2, None)
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
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let missing =
        storage.find_missing_event_ids(&[]).await.expect("find_missing_event_ids with empty input should succeed");
    assert!(missing.is_empty());
}

#[tokio::test]
async fn test_find_missing_event_ids_partial_existing() {
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let result = storage
        .get_missing_events_between("!any:example.com", &["$a:example.com".to_string()], &[], 10)
        .await
        .expect("get_missing_events_between with empty latest should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_missing_events_between_walks_dag() {
    let pool = test_pool().await;
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
    storage.create_event_with_graph(mk(middle_id.clone(), 2000), &[root_id.clone()], &[], 1, None).await.unwrap();
    storage.create_event_with_graph(mk(leaf_id.clone(), 3000), &[middle_id.clone()], &[], 2, None).await.unwrap();

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
    let pool = test_pool().await;
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

    let now = chrono::Utc::now().timestamp_millis();
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
    let pool = test_pool().await;
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

    let now = chrono::Utc::now().timestamp_millis();
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
    let pool = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let now = chrono::Utc::now().timestamp_millis();
    let result = storage
        .get_ephemeral_events_batch(&[], now, 10)
        .await
        .expect("get_ephemeral_events_batch with empty rooms should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_ephemeral_events_batch_multiple_rooms() {
    let pool = test_pool().await;
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

    let now = chrono::Utc::now().timestamp_millis();
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
    let pool = test_pool().await;
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
    let pool = test_pool().await;
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
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
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

// --- timestamp lookups ---

#[tokio::test]
async fn test_find_event_by_timestamp_found() {
    let pool = test_pool().await;
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
    let pool = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let result = storage
        .find_event_by_timestamp("!nonexistent:example.com", 1_000_000)
        .await
        .expect("find_event_by_timestamp should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn test_find_event_id_by_timestamp_forward() {
    let pool = test_pool().await;
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
    let pool = test_pool().await;
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
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
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
    let pool = test_pool().await;
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
    let pool = test_pool().await;
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
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
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
    let pool = test_pool().await;
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
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
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
    let pool = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    // Creating the FTS index twice should not error (idempotent).
    storage.create_postgres_fts_index().await.expect("create_postgres_fts_index first call should succeed");
    storage.create_postgres_fts_index().await.expect("create_postgres_fts_index second call should succeed");
}

#[tokio::test]
async fn test_search_joined_room_events_empty_joined() {
    let pool = test_pool().await;
    let storage = EventStorage::new(&pool, test_server_name());
    let results = storage
        .search_joined_room_events(&[], "%anything%", None, None, None, None, None, 10)
        .await
        .expect("search_joined_room_events with empty joined should succeed");
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_search_joined_room_events_matches() {
    let pool = test_pool().await;
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
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
        redacts: None,
    };
    storage.create_event(params, None).await.unwrap();

    let pattern = format!("%{}%", needle.to_lowercase());
    let results = storage
        .search_joined_room_events(&[room_id.clone()], &pattern, None, None, None, None, None, 10)
        .await
        .expect("search_joined_room_events should succeed");
    assert!(!results.is_empty());

    let _ = storage.delete_room_events(&room_id).await;
}
