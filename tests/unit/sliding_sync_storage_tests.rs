#![cfg(test)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::storage::sliding_sync::{
    decode_room_token_sync_cursor, encode_room_token_sync_cursor, RoomTokenSyncCursor,
    SlidingSyncFilters, SlidingSyncListQuery, SlidingSyncStorage,
};
use tokio::runtime::Runtime;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database() -> Option<Arc<sqlx::PgPool>> {
    let pool = match synapse_rust::test_utils::prepare_empty_isolated_test_pool().await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!(
                "Skipping sliding sync storage tests because test database is unavailable: {error}"
            );
            return None;
        }
    };

    sqlx::query("CREATE SEQUENCE IF NOT EXISTS sliding_sync_pos_seq")
        .execute(&*pool)
        .await
        .expect("Failed to create sliding_sync_pos_seq");

    sqlx::query(
        r#"
        CREATE TABLE sliding_sync_tokens (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            conn_id TEXT,
            token TEXT NOT NULL,
            pos BIGINT NOT NULL,
            created_ts BIGINT NOT NULL,
            expires_at BIGINT
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create sliding_sync_tokens table");

    sqlx::query(
        r#"
        CREATE UNIQUE INDEX idx_sliding_sync_tokens_unique ON sliding_sync_tokens(user_id, device_id, COALESCE(conn_id, ''))
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create sliding_sync_tokens unique index");

    sqlx::query(
        r#"
        CREATE TABLE sliding_sync_lists (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            conn_id TEXT,
            list_key TEXT NOT NULL,
            sort JSONB DEFAULT '[]',
            filters JSONB DEFAULT '{}',
            room_subscription JSONB DEFAULT '{}',
            ranges JSONB DEFAULT '[]',
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create sliding_sync_lists table");

    sqlx::query(
        r#"
        CREATE UNIQUE INDEX idx_sliding_sync_lists_unique ON sliding_sync_lists(user_id, device_id, COALESCE(conn_id, ''), list_key)
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create sliding_sync_lists unique index");

    sqlx::query(
        r#"
        CREATE TABLE sliding_sync_rooms (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            conn_id TEXT,
            list_key TEXT,
            bump_stamp BIGINT DEFAULT 0,
            highlight_count INTEGER DEFAULT 0,
            notification_count INTEGER DEFAULT 0,
            is_dm BOOLEAN DEFAULT FALSE,
            is_encrypted BOOLEAN DEFAULT FALSE,
            is_tombstoned BOOLEAN DEFAULT FALSE,
            invited BOOLEAN DEFAULT FALSE,
            name TEXT,
            avatar TEXT,
            timestamp BIGINT DEFAULT 0,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create sliding_sync_rooms table");

    sqlx::query(
        r#"
        CREATE UNIQUE INDEX idx_sliding_sync_rooms_unique ON sliding_sync_rooms(user_id, device_id, room_id, COALESCE(conn_id, ''))
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create sliding_sync_rooms unique index");

    sqlx::query(
        r#"
        CREATE INDEX idx_sliding_sync_rooms_room_id ON sliding_sync_rooms(room_id, updated_ts DESC)
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create sliding_sync_rooms room_id index");

    Some(pool)
}

#[test]
fn test_create_or_update_token_creates_new() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());

        let token = storage
            .create_or_update_token("@alice:localhost", "DEVICE1", None)
            .await
            .unwrap();

        assert_eq!(token.user_id, "@alice:localhost");
        assert_eq!(token.device_id, "DEVICE1");
        assert!(token.conn_id.is_none());
        assert!(!token.token.is_empty());
        assert!(token.pos > 0);
        assert!(token.created_ts > 0);
        assert!(token.expires_at.is_some());
    });
}

#[test]
fn test_create_or_update_token_with_conn_id() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());

        let token = storage
            .create_or_update_token("@bob:localhost", "DEVICE2", Some("conn1"))
            .await
            .unwrap();

        assert_eq!(token.conn_id, Some("conn1".to_string()));
    });
}

#[test]
fn test_create_or_update_token_upserts_existing() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@upsert_user_{suffix}:localhost");

        let token1 = storage
            .create_or_update_token(&user_id, "DEV1", None)
            .await
            .unwrap();

        let token2 = storage
            .create_or_update_token(&user_id, "DEV1", None)
            .await
            .unwrap();

        assert_eq!(token1.id, token2.id);
        assert!(token2.pos > token1.pos);
    });
}

#[test]
fn test_get_token_returns_created() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@get_token_{suffix}:localhost");

        storage
            .create_or_update_token(&user_id, "DEV1", Some("c1"))
            .await
            .unwrap();

        let fetched = storage
            .get_token(&user_id, "DEV1", Some("c1"))
            .await
            .unwrap();

        assert!(fetched.is_some());
        let t = fetched.unwrap();
        assert_eq!(t.user_id, user_id);
        assert_eq!(t.device_id, "DEV1");
        assert_eq!(t.conn_id, Some("c1".to_string()));
    });
}

#[test]
fn test_get_token_returns_none_for_missing() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());

        let fetched = storage
            .get_token("@nonexistent:localhost", "DEV1", None)
            .await
            .unwrap();

        assert!(fetched.is_none());
    });
}

#[test]
fn test_get_token_null_conn_id_distinction() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@null_conn_{suffix}:localhost");

        storage
            .create_or_update_token(&user_id, "DEV1", None)
            .await
            .unwrap();

        storage
            .create_or_update_token(&user_id, "DEV1", Some("conn_x"))
            .await
            .unwrap();

        let null_token = storage
            .get_token(&user_id, "DEV1", None)
            .await
            .unwrap()
            .unwrap();
        assert!(null_token.conn_id.is_none());

        let conn_token = storage
            .get_token(&user_id, "DEV1", Some("conn_x"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(conn_token.conn_id, Some("conn_x".to_string()));
    });
}

#[test]
fn test_validate_pos_valid() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@valid_pos_{suffix}:localhost");

        let token = storage
            .create_or_update_token(&user_id, "DEV1", None)
            .await
            .unwrap();

        let is_valid = storage
            .validate_pos(&user_id, "DEV1", None, &token.pos.to_string())
            .await
            .unwrap();

        assert!(is_valid);
    });
}

#[test]
fn test_validate_pos_invalid() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@invalid_pos_{suffix}:localhost");

        storage
            .create_or_update_token(&user_id, "DEV1", None)
            .await
            .unwrap();

        let is_valid = storage
            .validate_pos(&user_id, "DEV1", None, "999999")
            .await
            .unwrap();

        assert!(!is_valid);
    });
}

#[test]
fn test_validate_pos_missing_user() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());

        let is_valid = storage
            .validate_pos("@missing:localhost", "DEV1", None, "1")
            .await
            .unwrap();

        assert!(!is_valid);
    });
}

#[test]
fn test_save_list_creates_new() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@save_list_{suffix}:localhost");

        let sort = vec!["by_recency".to_string()];
        let list = storage
            .save_list(
                &user_id,
                "DEV1",
                None,
                "main",
                &sort,
                None,
                None,
                &[(0u32, 20u32)],
            )
            .await
            .unwrap();

        assert_eq!(list.user_id, user_id);
        assert_eq!(list.device_id, "DEV1");
        assert_eq!(list.list_key, "main");
        assert!(list.created_ts > 0);
        assert_eq!(list.updated_ts, list.created_ts);
    });
}

#[test]
fn test_save_list_upserts_existing() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@upsert_list_{suffix}:localhost");

        let sort1 = vec!["by_recency".to_string()];
        storage
            .save_list(
                &user_id,
                "DEV1",
                None,
                "main",
                &sort1,
                None,
                None,
                &[(0, 10)],
            )
            .await
            .unwrap();

        let sort2 = vec!["by_name".to_string()];
        let updated = storage
            .save_list(
                &user_id,
                "DEV1",
                None,
                "main",
                &sort2,
                None,
                None,
                &[(0, 20)],
            )
            .await
            .unwrap();

        assert_eq!(updated.list_key, "main");
        assert!(updated.updated_ts >= updated.created_ts);
    });
}

#[test]
fn test_save_list_with_filters() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@filter_list_{suffix}:localhost");

        let filters = SlidingSyncFilters {
            is_dm: Some(true),
            is_encrypted: Some(false),
            ..Default::default()
        };
        let sort = vec!["by_recency".to_string()];

        let list = storage
            .save_list(
                &user_id,
                "DEV1",
                None,
                "dm_list",
                &sort,
                Some(&filters),
                None,
                &[(0, 50)],
            )
            .await
            .unwrap();

        assert_eq!(list.list_key, "dm_list");
        assert!(list.filters.is_some());
    });
}

#[test]
fn test_get_lists_returns_all_for_user_device() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@get_lists_{suffix}:localhost");

        let sort = vec!["by_recency".to_string()];
        storage
            .save_list(
                &user_id,
                "DEV1",
                None,
                "main",
                &sort,
                None,
                None,
                &[(0, 10)],
            )
            .await
            .unwrap();
        storage
            .save_list(&user_id, "DEV1", None, "dm", &sort, None, None, &[(0, 5)])
            .await
            .unwrap();

        let lists = storage.get_lists(&user_id, "DEV1", None).await.unwrap();

        assert_eq!(lists.len(), 2);
    });
}

#[test]
fn test_get_lists_empty() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());

        let lists = storage
            .get_lists("@nolists:localhost", "DEV1", None)
            .await
            .unwrap();

        assert!(lists.is_empty());
    });
}

#[test]
fn test_delete_list() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@del_list_{suffix}:localhost");

        let sort = vec!["by_recency".to_string()];
        storage
            .save_list(
                &user_id,
                "DEV1",
                None,
                "main",
                &sort,
                None,
                None,
                &[(0, 10)],
            )
            .await
            .unwrap();
        storage
            .save_list(&user_id, "DEV1", None, "dm", &sort, None, None, &[(0, 5)])
            .await
            .unwrap();

        storage
            .delete_list(&user_id, "DEV1", None, "main")
            .await
            .unwrap();

        let lists = storage.get_lists(&user_id, "DEV1", None).await.unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].list_key, "dm");
    });
}

#[test]
fn test_upsert_room_creates_new() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@upsert_room_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        let room = storage
            .upsert_room(
                &user_id,
                "DEV1",
                &room_id,
                None,
                Some("main"),
                1000,
                2,
                5,
                true,
                false,
                false,
                false,
                Some("Test Room"),
                Some("mxc://avatar"),
                1700000000000,
            )
            .await
            .unwrap();

        assert_eq!(room.user_id, user_id);
        assert_eq!(room.room_id, room_id);
        assert_eq!(room.bump_stamp, 1000);
        assert_eq!(room.highlight_count, 2);
        assert_eq!(room.notification_count, 5);
        assert!(room.is_dm);
        assert!(!room.is_encrypted);
        assert_eq!(room.name, Some("Test Room".to_string()));
        assert_eq!(room.avatar, Some("mxc://avatar".to_string()));
    });
}

#[test]
fn test_upsert_room_updates_existing() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@update_room_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        storage
            .upsert_room(
                &user_id,
                "DEV1",
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
                Some("Old Name"),
                None,
                1700000000000,
            )
            .await
            .unwrap();

        let updated = storage
            .upsert_room(
                &user_id,
                "DEV1",
                &room_id,
                None,
                Some("main"),
                2000,
                3,
                7,
                true,
                true,
                false,
                false,
                Some("New Name"),
                Some("mxc://new"),
                1700000001000,
            )
            .await
            .unwrap();

        assert_eq!(updated.bump_stamp, 2000);
        assert_eq!(updated.highlight_count, 3);
        assert_eq!(updated.notification_count, 7);
        assert!(updated.is_dm);
        assert!(updated.is_encrypted);
        assert_eq!(updated.name, Some("New Name".to_string()));
    });
}

#[test]
fn test_upsert_room_bump_stamp_uses_greatest() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@bump_greatest_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &room_id,
                None,
                Some("main"),
                5000,
                0,
                0,
                false,
                false,
                false,
                false,
                None,
                None,
                1700000000000,
            )
            .await
            .unwrap();

        let updated = storage
            .upsert_room(
                &user_id,
                "DEV1",
                &room_id,
                None,
                Some("main"),
                3000,
                0,
                0,
                false,
                false,
                false,
                false,
                None,
                None,
                1700000001000,
            )
            .await
            .unwrap();

        assert_eq!(updated.bump_stamp, 5000);
    });
}

#[test]
fn test_upsert_room_null_name_keeps_existing() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@keep_name_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        storage
            .upsert_room(
                &user_id,
                "DEV1",
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
                Some("Original Name"),
                Some("mxc://orig"),
                1700000000000,
            )
            .await
            .unwrap();

        let updated = storage
            .upsert_room(
                &user_id,
                "DEV1",
                &room_id,
                None,
                Some("main"),
                2000,
                1,
                1,
                false,
                false,
                false,
                false,
                None,
                None,
                1700000001000,
            )
            .await
            .unwrap();

        assert_eq!(updated.name, Some("Original Name".to_string()));
        assert_eq!(updated.avatar, Some("mxc://orig".to_string()));
    });
}

#[test]
fn test_get_room_returns_existing() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@get_room_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        storage
            .upsert_room(
                &user_id,
                "DEV1",
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
                1700000000000,
            )
            .await
            .unwrap();

        let fetched = storage
            .get_room(&user_id, "DEV1", &room_id, None)
            .await
            .unwrap();

        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().name, Some("My Room".to_string()));
    });
}

#[test]
fn test_get_room_returns_none_for_missing() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());

        let fetched = storage
            .get_room("@nobody:localhost", "DEV1", "!nonexistent:localhost", None)
            .await
            .unwrap();

        assert!(fetched.is_none());
    });
}

#[test]
fn test_delete_room() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@del_room_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        storage
            .upsert_room(
                &user_id,
                "DEV1",
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
                1700000000000,
            )
            .await
            .unwrap();

        storage
            .delete_room(&user_id, "DEV1", &room_id, None)
            .await
            .unwrap();

        let fetched = storage
            .get_room(&user_id, "DEV1", &room_id, None)
            .await
            .unwrap();
        assert!(fetched.is_none());
    });
}

#[test]
fn test_update_notification_counts() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@notif_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        storage
            .upsert_room(
                &user_id,
                "DEV1",
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
                1700000000000,
            )
            .await
            .unwrap();

        storage
            .update_notification_counts(&user_id, "DEV1", &room_id, None, 5, 12)
            .await
            .unwrap();

        let room = storage
            .get_room(&user_id, "DEV1", &room_id, None)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(room.highlight_count, 5);
        assert_eq!(room.notification_count, 12);
    });
}

#[test]
fn test_bump_room_increases_stamp() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@bump_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        storage
            .upsert_room(
                &user_id,
                "DEV1",
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
                1700000000000,
            )
            .await
            .unwrap();

        storage
            .bump_room(&user_id, "DEV1", &room_id, None, 3000)
            .await
            .unwrap();

        let room = storage
            .get_room(&user_id, "DEV1", &room_id, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(room.bump_stamp, 3000);

        storage
            .bump_room(&user_id, "DEV1", &room_id, None, 2000)
            .await
            .unwrap();

        let room = storage
            .get_room(&user_id, "DEV1", &room_id, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(room.bump_stamp, 3000);
    });
}

#[test]
fn test_get_rooms_for_list_ordered_by_bump_stamp() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@rooms_list_{suffix}:localhost");

        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &format!("!low_{suffix}:localhost"),
                None,
                Some("main"),
                100,
                0,
                0,
                false,
                false,
                false,
                false,
                Some("Low"),
                None,
                1700000000000,
            )
            .await
            .unwrap();
        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &format!("!high_{suffix}:localhost"),
                None,
                Some("main"),
                500,
                0,
                0,
                false,
                false,
                false,
                false,
                Some("High"),
                None,
                1700000000000,
            )
            .await
            .unwrap();
        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &format!("!mid_{suffix}:localhost"),
                None,
                Some("main"),
                300,
                0,
                0,
                false,
                false,
                false,
                false,
                Some("Mid"),
                None,
                1700000000000,
            )
            .await
            .unwrap();

        let query = SlidingSyncListQuery {
            user_id: &user_id,
            device_id: "DEV1",
            conn_id: None,
            list_key: "main",
            start: 0,
            end: 10,
            filters: None,
        };

        let rooms = storage.get_rooms_for_list(query).await.unwrap();

        assert_eq!(rooms.len(), 3);
        assert_eq!(rooms[0].name, Some("High".to_string()));
        assert_eq!(rooms[1].name, Some("Mid".to_string()));
        assert_eq!(rooms[2].name, Some("Low".to_string()));
    });
}

#[test]
fn test_get_rooms_for_list_with_filters() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@filter_rooms_{suffix}:localhost");

        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &format!("!dm_{suffix}:localhost"),
                None,
                Some("main"),
                100,
                0,
                0,
                true,
                false,
                false,
                false,
                Some("DM Room"),
                None,
                1700000000000,
            )
            .await
            .unwrap();
        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &format!("!group_{suffix}:localhost"),
                None,
                Some("main"),
                200,
                0,
                0,
                false,
                false,
                false,
                false,
                Some("Group Room"),
                None,
                1700000000000,
            )
            .await
            .unwrap();

        let filters = SlidingSyncFilters {
            is_dm: Some(true),
            ..Default::default()
        };
        let query = SlidingSyncListQuery {
            user_id: &user_id,
            device_id: "DEV1",
            conn_id: None,
            list_key: "main",
            start: 0,
            end: 10,
            filters: Some(&filters),
        };

        let rooms = storage.get_rooms_for_list(query).await.unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].name, Some("DM Room".to_string()));
    });
}

#[test]
fn test_get_rooms_for_list_pagination() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@paginate_{suffix}:localhost");

        for i in 0..5 {
            storage
                .upsert_room(
                    &user_id,
                    "DEV1",
                    &format!("!room{i}_{suffix}:localhost"),
                    None,
                    Some("main"),
                    (i * 100) as i64,
                    0,
                    0,
                    false,
                    false,
                    false,
                    false,
                    None,
                    None,
                    1700000000000,
                )
                .await
                .unwrap();
        }

        let query = SlidingSyncListQuery {
            user_id: &user_id,
            device_id: "DEV1",
            conn_id: None,
            list_key: "main",
            start: 1,
            end: 3,
            filters: None,
        };

        let rooms = storage.get_rooms_for_list(query).await.unwrap();
        assert_eq!(rooms.len(), 3);
    });
}

#[test]
fn test_count_rooms_for_list() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@count_rooms_{suffix}:localhost");

        for i in 0..3 {
            storage
                .upsert_room(
                    &user_id,
                    "DEV1",
                    &format!("!room{i}_{suffix}:localhost"),
                    None,
                    Some("main"),
                    (i * 100) as i64,
                    0,
                    0,
                    false,
                    false,
                    false,
                    false,
                    None,
                    None,
                    1700000000000,
                )
                .await
                .unwrap();
        }

        let count = storage
            .count_rooms_for_list(&user_id, "DEV1", None, "main", None)
            .await
            .unwrap();

        assert_eq!(count, 3);
    });
}

#[test]
fn test_count_rooms_for_list_with_filters() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@count_filter_{suffix}:localhost");

        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &format!("!dm_{suffix}:localhost"),
                None,
                Some("main"),
                100,
                0,
                0,
                true,
                true,
                false,
                false,
                None,
                None,
                1700000000000,
            )
            .await
            .unwrap();
        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &format!("!group_{suffix}:localhost"),
                None,
                Some("main"),
                200,
                0,
                0,
                false,
                false,
                false,
                false,
                None,
                None,
                1700000000000,
            )
            .await
            .unwrap();

        let filters = SlidingSyncFilters {
            is_encrypted: Some(true),
            ..Default::default()
        };
        let count = storage
            .count_rooms_for_list(&user_id, "DEV1", None, "main", Some(&filters))
            .await
            .unwrap();

        assert_eq!(count, 1);
    });
}

#[test]
fn test_count_rooms_for_list_empty() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());

        let count = storage
            .count_rooms_for_list("@empty:localhost", "DEV1", None, "main", None)
            .await
            .unwrap();

        assert_eq!(count, 0);
    });
}

#[test]
fn test_cleanup_expired_tokens() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@cleanup_{suffix}:localhost");

        let token = storage
            .create_or_update_token(&user_id, "DEV1", None)
            .await
            .unwrap();

        let past_expiry = chrono::Utc::now().timestamp_millis() - 1000;
        sqlx::query("UPDATE sliding_sync_tokens SET expires_at = $1 WHERE id = $2")
            .bind(past_expiry)
            .bind(token.id)
            .execute(&*pool)
            .await
            .unwrap();

        let deleted = storage.cleanup_expired_tokens().await.unwrap();
        assert_eq!(deleted, 1);

        let fetched = storage.get_token(&user_id, "DEV1", None).await.unwrap();
        assert!(fetched.is_none());
    });
}

#[test]
fn test_cleanup_expired_tokens_preserves_valid() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@preserve_{suffix}:localhost");

        storage
            .create_or_update_token(&user_id, "DEV1", None)
            .await
            .unwrap();

        let deleted = storage.cleanup_expired_tokens().await.unwrap();
        assert_eq!(deleted, 0);

        let fetched = storage.get_token(&user_id, "DEV1", None).await.unwrap();
        assert!(fetched.is_some());
    });
}

#[test]
fn test_list_room_token_sync_basic() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@token_sync_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        storage
            .create_or_update_token(&user_id, "DEV1", None)
            .await
            .unwrap();

        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &room_id,
                None,
                Some("main"),
                1000,
                1,
                3,
                false,
                false,
                false,
                false,
                Some("Sync Room"),
                None,
                1700000000000,
            )
            .await
            .unwrap();

        let entries = storage
            .list_room_token_sync(&room_id, 10, None)
            .await
            .unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].user_id, user_id);
        assert_eq!(entries[0].device_id, "DEV1");
        assert_eq!(entries[0].name, Some("Sync Room".to_string()));
        assert!(entries[0].pos.is_some());
        assert!(!entries[0].is_expired);
    });
}

#[test]
fn test_list_room_token_sync_with_cursor() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let room_id = format!("!cursor_room_{suffix}:localhost");

        storage
            .create_or_update_token(&format!("@user1_{suffix}:localhost"), "DEV1", None)
            .await
            .unwrap();
        storage
            .create_or_update_token(&format!("@user2_{suffix}:localhost"), "DEV1", None)
            .await
            .unwrap();

        storage
            .upsert_room(
                &format!("@user1_{suffix}:localhost"),
                "DEV1",
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
                1700000000000,
            )
            .await
            .unwrap();
        storage
            .upsert_room(
                &format!("@user2_{suffix}:localhost"),
                "DEV1",
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
                1700000000000,
            )
            .await
            .unwrap();

        let first_page = storage
            .list_room_token_sync(&room_id, 10, None)
            .await
            .unwrap();
        assert_eq!(first_page.len(), 2);

        let cursor = RoomTokenSyncCursor {
            room_updated_ts: first_page[0].room_updated_ts,
            user_id: first_page[0].user_id.clone(),
            device_id: first_page[0].device_id.clone(),
            conn_id: first_page[0].conn_id.clone(),
        };

        let second_page = storage
            .list_room_token_sync(&room_id, 10, Some(&cursor))
            .await
            .unwrap();
        assert_eq!(second_page.len(), 1);
        assert_ne!(second_page[0].user_id, first_page[0].user_id);
    });
}

#[test]
fn test_count_room_token_sync() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let room_id = format!("!count_sync_{suffix}:localhost");

        storage
            .upsert_room(
                &format!("@user1_{suffix}:localhost"),
                "DEV1",
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
                1700000000000,
            )
            .await
            .unwrap();
        storage
            .upsert_room(
                &format!("@user2_{suffix}:localhost"),
                "DEV1",
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
                1700000000000,
            )
            .await
            .unwrap();

        let count = storage.count_room_token_sync(&room_id).await.unwrap();
        assert_eq!(count, 2);
    });
}

#[test]
fn test_list_room_token_sync_empty() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());

        let entries = storage
            .list_room_token_sync("!nonexistent:localhost", 10, None)
            .await
            .unwrap();

        assert!(entries.is_empty());
    });
}

#[test]
fn test_cursor_round_trip_with_conn_id() {
    let cursor = RoomTokenSyncCursor {
        room_updated_ts: 1_700_000_000_000,
        user_id: "@alice:example.com".to_string(),
        device_id: "DEVICE".to_string(),
        conn_id: Some("main|conn".to_string()),
    };

    let encoded = encode_room_token_sync_cursor(&cursor);
    let decoded = decode_room_token_sync_cursor(Some(&encoded));
    assert_eq!(decoded, Some(cursor));
}

#[test]
fn test_cursor_round_trip_without_conn_id() {
    let cursor = RoomTokenSyncCursor {
        room_updated_ts: 1_700_000_000_000,
        user_id: "@bob:example.com".to_string(),
        device_id: "PHONE".to_string(),
        conn_id: None,
    };

    let encoded = encode_room_token_sync_cursor(&cursor);
    let decoded = decode_room_token_sync_cursor(Some(&encoded));
    assert_eq!(decoded, Some(cursor));
}

#[test]
fn test_cursor_decode_invalid_input() {
    assert_eq!(decode_room_token_sync_cursor(None), None);
    assert_eq!(decode_room_token_sync_cursor(Some("")), None);
    assert_eq!(decode_room_token_sync_cursor(Some("bad")), None);
    assert_eq!(decode_room_token_sync_cursor(Some("123|||")), None);
    assert_eq!(decode_room_token_sync_cursor(Some("1|a|b|0|c|extra")), None);
}

#[test]
fn test_conn_id_isolation_between_tokens() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@conn_iso_{suffix}:localhost");

        let token_none = storage
            .create_or_update_token(&user_id, "DEV1", None)
            .await
            .unwrap();
        let token_conn = storage
            .create_or_update_token(&user_id, "DEV1", Some("conn1"))
            .await
            .unwrap();

        assert_ne!(token_none.id, token_conn.id);
        assert!(token_none.conn_id.is_none());
        assert_eq!(token_conn.conn_id, Some("conn1".to_string()));
    });
}

#[test]
fn test_conn_id_isolation_between_rooms() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@room_conn_iso_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        storage
            .upsert_room(
                &user_id,
                "DEV1",
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
                Some("No Conn"),
                None,
                1700000000000,
            )
            .await
            .unwrap();
        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &room_id,
                Some("conn1"),
                Some("main"),
                1000,
                3,
                4,
                false,
                false,
                false,
                false,
                Some("With Conn"),
                None,
                1700000000000,
            )
            .await
            .unwrap();

        let room_none = storage
            .get_room(&user_id, "DEV1", &room_id, None)
            .await
            .unwrap()
            .unwrap();
        let room_conn = storage
            .get_room(&user_id, "DEV1", &room_id, Some("conn1"))
            .await
            .unwrap()
            .unwrap();

        assert_ne!(room_none.id, room_conn.id);
        assert_eq!(room_none.name, Some("No Conn".to_string()));
        assert_eq!(room_conn.name, Some("With Conn".to_string()));
        assert_eq!(room_none.highlight_count, 1);
        assert_eq!(room_conn.highlight_count, 3);
    });
}

#[test]
fn test_delete_room_different_conn_id_no_cross_delete() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@cross_del_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");

        storage
            .upsert_room(
                &user_id,
                "DEV1",
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
                1700000000000,
            )
            .await
            .unwrap();
        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &room_id,
                Some("conn1"),
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
                1700000000000,
            )
            .await
            .unwrap();

        storage
            .delete_room(&user_id, "DEV1", &room_id, None)
            .await
            .unwrap();

        let room_none = storage
            .get_room(&user_id, "DEV1", &room_id, None)
            .await
            .unwrap();
        assert!(room_none.is_none());

        let room_conn = storage
            .get_room(&user_id, "DEV1", &room_id, Some("conn1"))
            .await
            .unwrap();
        assert!(room_conn.is_some());
    });
}

#[test]
fn test_invited_room_filter() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@invite_filter_{suffix}:localhost");

        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &format!("!invited_{suffix}:localhost"),
                None,
                Some("main"),
                100,
                0,
                0,
                false,
                false,
                false,
                true,
                Some("Invited"),
                None,
                1700000000000,
            )
            .await
            .unwrap();
        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &format!("!joined_{suffix}:localhost"),
                None,
                Some("main"),
                200,
                0,
                0,
                false,
                false,
                false,
                false,
                Some("Joined"),
                None,
                1700000000000,
            )
            .await
            .unwrap();

        let filters = SlidingSyncFilters {
            is_invite: Some(true),
            ..Default::default()
        };
        let query = SlidingSyncListQuery {
            user_id: &user_id,
            device_id: "DEV1",
            conn_id: None,
            list_key: "main",
            start: 0,
            end: 10,
            filters: Some(&filters),
        };

        let rooms = storage.get_rooms_for_list(query).await.unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].name, Some("Invited".to_string()));
    });
}

#[test]
fn test_room_name_like_filter() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@name_like_{suffix}:localhost");

        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &format!("!proj_{suffix}:localhost"),
                None,
                Some("main"),
                100,
                0,
                0,
                false,
                false,
                false,
                false,
                Some("Project Alpha"),
                None,
                1700000000000,
            )
            .await
            .unwrap();
        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &format!("!random_{suffix}:localhost"),
                None,
                Some("main"),
                200,
                0,
                0,
                false,
                false,
                false,
                false,
                Some("Random Chat"),
                None,
                1700000000000,
            )
            .await
            .unwrap();

        let filters = SlidingSyncFilters {
            room_name_like: Some("project".to_string()),
            ..Default::default()
        };
        let query = SlidingSyncListQuery {
            user_id: &user_id,
            device_id: "DEV1",
            conn_id: None,
            list_key: "main",
            start: 0,
            end: 10,
            filters: Some(&filters),
        };

        let rooms = storage.get_rooms_for_list(query).await.unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].name, Some("Project Alpha".to_string()));
    });
}

#[test]
fn test_tombstoned_room_filter() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = SlidingSyncStorage::new(pool.clone());
        let suffix = unique_id();
        let user_id = format!("@tomb_filter_{suffix}:localhost");

        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &format!("!tomb_{suffix}:localhost"),
                None,
                Some("main"),
                100,
                0,
                0,
                false,
                false,
                true,
                false,
                Some("Tombstoned"),
                None,
                1700000000000,
            )
            .await
            .unwrap();
        storage
            .upsert_room(
                &user_id,
                "DEV1",
                &format!("!alive_{suffix}:localhost"),
                None,
                Some("main"),
                200,
                0,
                0,
                false,
                false,
                false,
                false,
                Some("Alive"),
                None,
                1700000000000,
            )
            .await
            .unwrap();

        let filters = SlidingSyncFilters {
            is_tombstoned: Some(false),
            ..Default::default()
        };
        let query = SlidingSyncListQuery {
            user_id: &user_id,
            device_id: "DEV1",
            conn_id: None,
            list_key: "main",
            start: 0,
            end: 10,
            filters: Some(&filters),
        };

        let rooms = storage.get_rooms_for_list(query).await.unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].name, Some("Alive".to_string()));
    });
}
