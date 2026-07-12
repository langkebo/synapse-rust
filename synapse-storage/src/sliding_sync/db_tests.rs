use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};

use super::*;

async fn test_pool() -> Arc<Pool<Postgres>> {
    let db_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
    let pool =
        PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
    Arc::new(pool)
}

fn unique_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

#[tokio::test]
async fn test_create_or_update_token_insert_then_update() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");

    let token =
        storage.create_or_update_token(&user_id, &device_id, None).await.expect("create_or_update_token should insert");
    assert_eq!(token.user_id, user_id);
    assert_eq!(token.device_id, device_id);
    assert!(token.conn_id.is_none());
    let first_pos = token.pos;

    // Calling again with the same (user, device, conn) should update the existing row
    let updated =
        storage.create_or_update_token(&user_id, &device_id, None).await.expect("create_or_update_token should update");
    assert!(updated.pos > first_pos, "pos should advance on update, got {} -> {}", first_pos, updated.pos);

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_create_or_update_token_with_conn_id() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");
    let conn_id = unique_id("conn");

    let token = storage
        .create_or_update_token(&user_id, &device_id, Some(&conn_id))
        .await
        .expect("create_or_update_token with conn_id should succeed");
    assert_eq!(token.conn_id.as_deref(), Some(conn_id.as_str()));

    storage.delete_connection_data(&user_id, &device_id, Some(&conn_id)).await.expect("cleanup");
}

#[tokio::test]
async fn test_get_token_returns_none_when_absent() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@missing");
    let device_id = unique_id("DEV");

    let result = storage.get_token(&user_id, &device_id, None).await.expect("get_token should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_token_returns_inserted_token() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");

    storage.create_or_update_token(&user_id, &device_id, None).await.unwrap();
    let fetched = storage.get_token(&user_id, &device_id, None).await.expect("get_token should succeed");
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().user_id, user_id);

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_validate_pos_valid_and_invalid() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");

    let token = storage.create_or_update_token(&user_id, &device_id, None).await.unwrap();
    let valid_pos = token.pos.to_string();
    assert!(storage.validate_pos(&user_id, &device_id, None, &valid_pos).await.expect("validate_pos valid"));
    // A wrong pos should not validate
    assert!(!storage.validate_pos(&user_id, &device_id, None, "999999999").await.expect("validate_pos invalid"));
    // Nonexistent token should not validate
    assert!(!storage.validate_pos("@nobody:x", "NOPE", None, "1").await.expect("validate_pos nonexistent"));

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_save_list_insert_and_update() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");
    let list_key = "main";

    let sort = vec!["by_recency".to_string()];
    let ranges = vec![(0u32, 10u32)];

    let list = storage
        .save_list(&user_id, &device_id, None, list_key, &sort, None, None, &ranges)
        .await
        .expect("save_list should insert");
    assert_eq!(list.list_key, list_key);

    // Update with new sort/ranges
    let new_sort = vec!["by_name".to_string()];
    let new_ranges = vec![(0u32, 20u32)];
    let updated = storage
        .save_list(&user_id, &device_id, None, list_key, &new_sort, None, None, &new_ranges)
        .await
        .expect("save_list should update");
    assert_eq!(updated.id, list.id, "upsert should keep the same id");

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_get_lists_returns_saved_lists() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");

    let sort = vec!["by_recency".to_string()];
    let ranges = vec![(0u32, 10u32)];
    storage.save_list(&user_id, &device_id, None, "main", &sort, None, None, &ranges).await.unwrap();
    storage.save_list(&user_id, &device_id, None, "archive", &sort, None, None, &ranges).await.unwrap();

    let lists = storage.get_lists(&user_id, &device_id, None).await.expect("get_lists should succeed");
    assert_eq!(lists.len(), 2);

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_get_lists_empty() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");

    let lists = storage.get_lists(&user_id, &device_id, None).await.expect("get_lists should succeed");
    assert!(lists.is_empty());
}

#[tokio::test]
async fn test_delete_list_removes_single_list() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");

    let sort = vec!["by_recency".to_string()];
    let ranges = vec![(0u32, 10u32)];
    storage.save_list(&user_id, &device_id, None, "main", &sort, None, None, &ranges).await.unwrap();
    storage.save_list(&user_id, &device_id, None, "archive", &sort, None, None, &ranges).await.unwrap();

    storage.delete_list(&user_id, &device_id, None, "main").await.expect("delete_list should succeed");

    let lists = storage.get_lists(&user_id, &device_id, None).await.unwrap();
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0].list_key, "archive");

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_upsert_room_insert_and_update() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");
    let room_id = unique_id("!room");

    let room = storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_id,
            None,
            Some("main"),
            1000,
            1,
            5,
            false,
            false,
            false,
            false,
            Some("First"),
            None,
            1000,
        )
        .await
        .expect("upsert_room should insert");
    assert_eq!(room.room_id, room_id);
    assert_eq!(room.bump_stamp, 1000);
    assert_eq!(room.highlight_count, 1);

    // Update with higher bump stamp; bump_stamp uses GREATEST so it should not decrease
    let updated = storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_id,
            None,
            Some("main"),
            500,
            2,
            8,
            true,
            true,
            false,
            false,
            Some("Second"),
            Some("avatar"),
            2000,
        )
        .await
        .expect("upsert_room should update");
    assert_eq!(updated.id, room.id, "upsert should keep the same id");
    assert_eq!(updated.bump_stamp, 1000, "bump_stamp uses GREATEST so should remain 1000");
    assert_eq!(updated.highlight_count, 2);
    assert_eq!(updated.name.as_deref(), Some("Second"));
    assert_eq!(updated.avatar.as_deref(), Some("avatar"));
    assert!(updated.is_dm);
    assert!(updated.is_encrypted);

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_get_room_returns_none_when_absent() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");
    let room_id = unique_id("!room");

    let result = storage.get_room(&user_id, &device_id, &room_id, None).await.expect("get_room should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_room_returns_inserted_room() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");
    let room_id = unique_id("!room");

    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_id,
            None,
            Some("main"),
            1000,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("My Room"),
            None,
            1000,
        )
        .await
        .unwrap();

    let fetched = storage.get_room(&user_id, &device_id, &room_id, None).await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().name.as_deref(), Some("My Room"));

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_get_rooms_for_list_pagination() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");
    let list_key = "main";

    // Insert 3 rooms with descending bump stamps
    for i in 0..3 {
        let room_id = unique_id("!room");
        storage
            .upsert_room(
                &user_id,
                &device_id,
                &room_id,
                None,
                Some(list_key),
                1000 - i as i64,
                0,
                0,
                false,
                false,
                false,
                false,
                None,
                None,
                1000,
            )
            .await
            .unwrap();
    }

    let query = SlidingSyncListQuery {
        user_id: &user_id,
        device_id: &device_id,
        conn_id: None,
        list_key,
        start: 0,
        end: 1,
        filters: None,
    };
    let rooms = storage.get_rooms_for_list(query).await.expect("get_rooms_for_list should succeed");
    assert_eq!(rooms.len(), 2, "should return start..end inclusive (2 rooms)");

    // With offset
    let query = SlidingSyncListQuery {
        user_id: &user_id,
        device_id: &device_id,
        conn_id: None,
        list_key,
        start: 2,
        end: 2,
        filters: None,
    };
    let rooms = storage.get_rooms_for_list(query).await.expect("get_rooms_for_list with offset should succeed");
    assert_eq!(rooms.len(), 1, "should return only the 3rd room");

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_count_rooms_for_list() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");
    let list_key = "main";

    let count_before = storage.count_rooms_for_list(&user_id, &device_id, None, list_key, None).await.unwrap();
    assert_eq!(count_before, 0);

    for i in 0..3 {
        let room_id = unique_id("!room");
        storage
            .upsert_room(
                &user_id,
                &device_id,
                &room_id,
                None,
                Some(list_key),
                1000 - i,
                0,
                0,
                false,
                false,
                false,
                false,
                None,
                None,
                1000,
            )
            .await
            .unwrap();
    }

    let count_after = storage.count_rooms_for_list(&user_id, &device_id, None, list_key, None).await.unwrap();
    assert_eq!(count_after, 3);

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_count_rooms_for_list_with_filters() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");
    let list_key = "main";

    // Two DM rooms, one non-DM
    for (is_dm, bump) in [(true, 1000), (true, 900), (false, 800)] {
        let room_id = unique_id("!room");
        storage
            .upsert_room(
                &user_id,
                &device_id,
                &room_id,
                None,
                Some(list_key),
                bump,
                0,
                0,
                is_dm,
                false,
                false,
                false,
                None,
                None,
                1000,
            )
            .await
            .unwrap();
    }

    let filters = SlidingSyncFilters { is_dm: Some(true), ..Default::default() };
    let count = storage.count_rooms_for_list(&user_id, &device_id, None, list_key, Some(&filters)).await.unwrap();
    assert_eq!(count, 2);

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_delete_room_removes_room() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");
    let room_id = unique_id("!room");

    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_id,
            None,
            Some("main"),
            1000,
            0,
            0,
            false,
            false,
            false,
            false,
            None,
            None,
            1000,
        )
        .await
        .unwrap();
    assert!(storage.get_room(&user_id, &device_id, &room_id, None).await.unwrap().is_some());

    storage.delete_room(&user_id, &device_id, &room_id, None).await.expect("delete_room should succeed");
    assert!(storage.get_room(&user_id, &device_id, &room_id, None).await.unwrap().is_none());
}

#[tokio::test]
async fn test_update_notification_counts() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");
    let room_id = unique_id("!room");

    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_id,
            None,
            Some("main"),
            1000,
            0,
            0,
            false,
            false,
            false,
            false,
            None,
            None,
            1000,
        )
        .await
        .unwrap();

    storage
        .update_notification_counts(&user_id, &device_id, &room_id, None, 7, 42)
        .await
        .expect("update_notification_counts should succeed");

    let room = storage.get_room(&user_id, &device_id, &room_id, None).await.unwrap().unwrap();
    assert_eq!(room.highlight_count, 7);
    assert_eq!(room.notification_count, 42);

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_bump_room_does_not_decrease() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");
    let room_id = unique_id("!room");

    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_id,
            None,
            Some("main"),
            1000,
            0,
            0,
            false,
            false,
            false,
            false,
            None,
            None,
            1000,
        )
        .await
        .unwrap();

    // Lower bump should not decrease (uses GREATEST)
    storage.bump_room(&user_id, &device_id, &room_id, None, 500).await.expect("bump_room lower");
    let room = storage.get_room(&user_id, &device_id, &room_id, None).await.unwrap().unwrap();
    assert_eq!(room.bump_stamp, 1000, "bump_room should not decrease bump_stamp");

    // Higher bump should increase
    storage.bump_room(&user_id, &device_id, &room_id, None, 2000).await.expect("bump_room higher");
    let room = storage.get_room(&user_id, &device_id, &room_id, None).await.unwrap().unwrap();
    assert_eq!(room.bump_stamp, 2000, "bump_room should increase bump_stamp");

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_cleanup_expired_tokens_removes_only_expired() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");

    // Create a valid (non-expired) token
    storage.create_or_update_token(&user_id, &device_id, None).await.unwrap();

    // Manually expire one token row
    let past_ts = chrono::Utc::now().timestamp_millis() - 1000;
    sqlx::query("UPDATE sliding_sync_tokens SET expires_at = $1 WHERE user_id = $2 AND device_id = $3")
        .bind(past_ts)
        .bind(&user_id)
        .bind(&device_id)
        .execute(&*pool)
        .await
        .expect("should expire token");

    let removed = storage.cleanup_expired_tokens().await.expect("cleanup_expired_tokens should succeed");
    assert!(removed >= 1, "should have removed at least the expired token");

    // The expired token should no longer be retrievable
    let fetched = storage.get_token(&user_id, &device_id, None).await.unwrap();
    assert!(fetched.is_none());
}

#[tokio::test]
async fn test_count_room_token_sync() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let room_id = unique_id("!room");
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");

    let count_before = storage.count_room_token_sync(&room_id).await.expect("count_room_token_sync should succeed");
    assert_eq!(count_before, 0);

    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_id,
            None,
            Some("main"),
            1000,
            0,
            0,
            false,
            false,
            false,
            false,
            None,
            None,
            1000,
        )
        .await
        .unwrap();

    let count_after = storage.count_room_token_sync(&room_id).await.unwrap();
    assert_eq!(count_after, 1);

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_list_room_token_sync_without_cursor() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let room_id = unique_id("!room");
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");

    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_id,
            None,
            Some("main"),
            1000,
            1,
            2,
            false,
            false,
            false,
            false,
            Some("Room"),
            None,
            1000,
        )
        .await
        .unwrap();

    let entries = storage.list_room_token_sync(&room_id, 10, None).await.expect("list_room_token_sync should succeed");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].user_id, user_id);
    assert_eq!(entries[0].device_id, device_id);
    assert_eq!(entries[0].name.as_deref(), Some("Room"));
    assert!(!entries[0].is_expired);

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_list_room_token_sync_limit_truncates() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let room_id = unique_id("!room");

    // Insert 3 entries for the same room
    for i in 0..3 {
        let user_id = unique_id(&format!("@user{i}"));
        let device_id = unique_id("DEV");
        storage
            .upsert_room(
                &user_id,
                &device_id,
                &room_id,
                None,
                Some("main"),
                1000 - i as i64,
                0,
                0,
                false,
                false,
                false,
                false,
                None,
                None,
                1000,
            )
            .await
            .unwrap();
    }

    let entries = storage.list_room_token_sync(&room_id, 2, None).await.unwrap();
    assert_eq!(entries.len(), 2, "limit should truncate to 2");

    // Cleanup all inserted rows for this room
    sqlx::query("DELETE FROM sliding_sync_rooms WHERE room_id = $1").bind(&room_id).execute(&*pool).await.unwrap();
}

#[tokio::test]
async fn test_get_global_account_data_empty() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");

    let data = storage.get_global_account_data(&user_id).await.expect("get_global_account_data should succeed");
    assert!(data.as_object().map_or(true, |m| m.is_empty()));
}

#[tokio::test]
async fn test_get_global_account_data_with_rows() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"
        INSERT INTO account_data (user_id, data_type, content, created_ts, updated_ts)
        VALUES ($1, $2, $3, $4, $4)
        ON CONFLICT (user_id, data_type) DO UPDATE SET content = EXCLUDED.content, updated_ts = EXCLUDED.updated_ts
        "#,
    )
    .bind(&user_id)
    .bind("m.direct")
    .bind(serde_json::json!({"@bob:example.com": ["!room:example.com"]}))
    .bind(now)
    .execute(&*pool)
    .await
    .expect("should insert account_data");

    let data = storage.get_global_account_data(&user_id).await.expect("get_global_account_data should succeed");
    let obj = data.as_object().expect("should be object");
    assert!(obj.contains_key("m.direct"));

    sqlx::query("DELETE FROM account_data WHERE user_id = $1").bind(&user_id).execute(&*pool).await.unwrap();
}

#[tokio::test]
async fn test_get_room_account_data_empty_input() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());

    let data = storage.get_room_account_data("@nobody:x", &[]).await.expect("get_room_account_data empty");
    assert_eq!(data, serde_json::json!({}));
}

#[tokio::test]
async fn test_get_room_account_data_with_rows() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let room_id = unique_id("!room");
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        r#"
        INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts)
        VALUES ($1, $2, $3, $4, $5, $5)
        ON CONFLICT (user_id, room_id, data_type) DO UPDATE SET data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
        "#,
    )
    .bind(&user_id)
    .bind(&room_id)
    .bind("m.fully_read")
    .bind(serde_json::json!({"event_id": "$event:example.com"}))
    .bind(now)
    .execute(&*pool)
    .await
    .expect("should insert room_account_data");

    let data = storage
        .get_room_account_data(&user_id, &[room_id.clone()])
        .await
        .expect("get_room_account_data should succeed");
    let obj = data.as_object().expect("should be object");
    assert!(obj.contains_key(&room_id), "should contain the room_id key");

    sqlx::query("DELETE FROM room_account_data WHERE user_id = $1").bind(&user_id).execute(&*pool).await.unwrap();
}

#[tokio::test]
async fn test_get_receipts_for_rooms_empty_input() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());

    let data = storage.get_receipts_for_rooms(&[]).await.expect("get_receipts_for_rooms empty");
    assert_eq!(data, serde_json::json!({}));
}

#[tokio::test]
async fn test_get_receipts_for_rooms_with_rows() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let room_id = unique_id("!room");
    let user_id = unique_id("@user");
    let event_id = unique_id("$event");
    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"
        INSERT INTO event_receipts (event_id, room_id, user_id, receipt_type, ts, data, created_ts, updated_ts)
        VALUES ($1, $2, $3, $4, $5, $6, $5, $5)
        ON CONFLICT (event_id, room_id, user_id, receipt_type) DO UPDATE SET ts = EXCLUDED.ts, data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
        "#,
    )
    .bind(&event_id)
    .bind(&room_id)
    .bind(&user_id)
    .bind("m.read")
    .bind(now)
    .bind(serde_json::json!({}))
    .execute(&*pool)
    .await
    .expect("should insert event_receipt");

    let data = storage.get_receipts_for_rooms(&[room_id.clone()]).await.expect("get_receipts_for_rooms should succeed");
    let obj = data.as_object().expect("should be object");
    assert!(obj.contains_key(&room_id), "should contain the room_id key");

    sqlx::query("DELETE FROM event_receipts WHERE room_id = $1").bind(&room_id).execute(&*pool).await.unwrap();
}

#[tokio::test]
async fn test_delete_connection_data_removes_all() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");

    // Seed token, list, and room
    storage.create_or_update_token(&user_id, &device_id, None).await.unwrap();
    let sort = vec!["by_recency".to_string()];
    let ranges = vec![(0u32, 10u32)];
    storage.save_list(&user_id, &device_id, None, "main", &sort, None, None, &ranges).await.unwrap();
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &unique_id("!room"),
            None,
            Some("main"),
            1000,
            0,
            0,
            false,
            false,
            false,
            false,
            None,
            None,
            1000,
        )
        .await
        .unwrap();

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("delete_connection_data should succeed");

    // All three should be gone
    assert!(storage.get_token(&user_id, &device_id, None).await.unwrap().is_none());
    assert!(storage.get_lists(&user_id, &device_id, None).await.unwrap().is_empty());
}

#[tokio::test]
async fn test_get_rooms_for_list_with_filters() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");
    let list_key = "main";

    // One encrypted room, one unencrypted
    for (is_encrypted, bump) in [(true, 1000), (false, 900)] {
        let room_id = unique_id("!room");
        storage
            .upsert_room(
                &user_id,
                &device_id,
                &room_id,
                None,
                Some(list_key),
                bump,
                0,
                0,
                false,
                is_encrypted,
                false,
                false,
                None,
                None,
                1000,
            )
            .await
            .unwrap();
    }

    let filters = SlidingSyncFilters { is_encrypted: Some(true), ..Default::default() };
    let query = SlidingSyncListQuery {
        user_id: &user_id,
        device_id: &device_id,
        conn_id: None,
        list_key,
        start: 0,
        end: 9,
        filters: Some(&filters),
    };
    let rooms = storage.get_rooms_for_list(query).await.expect("get_rooms_for_list with filters");
    assert_eq!(rooms.len(), 1);
    assert!(rooms[0].is_encrypted);

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_get_rooms_for_list_with_room_name_like_filter() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");
    let list_key = "main";

    for (name, bump) in [("Project Alpha", 1000), ("Project Beta", 900), ("Other", 800)] {
        let room_id = unique_id("!room");
        storage
            .upsert_room(
                &user_id,
                &device_id,
                &room_id,
                None,
                Some(list_key),
                bump,
                0,
                0,
                false,
                false,
                false,
                false,
                Some(name),
                None,
                1000,
            )
            .await
            .unwrap();
    }

    let filters = SlidingSyncFilters { room_name_like: Some("project".to_string()), ..Default::default() };
    let query = SlidingSyncListQuery {
        user_id: &user_id,
        device_id: &device_id,
        conn_id: None,
        list_key,
        start: 0,
        end: 9,
        filters: Some(&filters),
    };
    let rooms = storage.get_rooms_for_list(query).await.expect("get_rooms_for_list with name filter");
    assert_eq!(rooms.len(), 2, "should match both 'Project' rooms");

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}

#[tokio::test]
async fn test_sliding_sync_room_is_invited_column() {
    let pool = test_pool().await;
    let storage = SlidingSyncStorage::new(pool.clone());
    let user_id = unique_id("@user");
    let device_id = unique_id("DEV");
    let room_id = unique_id("!room");
    let now = chrono::Utc::now().timestamp_millis();

    // Insert a room with is_invited = true via upsert (uses the renamed column)
    storage
        .upsert_room(
            &user_id,
            &device_id,
            &room_id,
            None,
            Some("main"),
            now,
            0,
            0,
            false,
            false,
            false,
            true, // invited = true
            Some("Invited Room"),
            None,
            now,
        )
        .await
        .expect("upsert_room with is_invited=true");

    let row = storage
        .get_room(&user_id, &device_id, &room_id, None)
        .await
        .expect("get_room should succeed")
        .expect("room should exist");

    assert!(row.is_invited, "is_invited should be true");

    storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
}
