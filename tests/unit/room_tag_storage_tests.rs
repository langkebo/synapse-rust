#![cfg(test)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::storage::room_tag::RoomTagStorage;
use tokio::runtime::Runtime;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database() -> Option<Arc<sqlx::PgPool>> {
    match synapse_rust::test_utils::prepare_isolated_test_pool().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping room tag storage tests because test database is unavailable: {}",
                error
            );
            None
        }
    }
}

#[test]
fn test_room_tag_storage_round_trip() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let suffix = unique_id();
        let user_id = format!("@room_tag_user_{suffix}:localhost");
        let room_id = format!("!room_tag_room_{suffix}:localhost");

        RoomTagStorage::add_tag(&pool, &user_id, &room_id, "m.favourite", Some(0.25))
            .await
            .unwrap();

        let tags = RoomTagStorage::get_tags(&pool, &user_id, &room_id)
            .await
            .unwrap();

        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].user_id, user_id);
        assert_eq!(tags[0].room_id, room_id);
        assert_eq!(tags[0].tag, "m.favourite");
        assert_eq!(tags[0].order, Some(0.25));
        assert!(tags[0].created_ts > 0);
    });
}

#[test]
fn test_room_tag_storage_updates_existing_tag_order() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let suffix = unique_id();
        let user_id = format!("@room_tag_user_{suffix}:localhost");
        let room_id = format!("!room_tag_room_{suffix}:localhost");

        RoomTagStorage::add_tag(&pool, &user_id, &room_id, "m.lowpriority", Some(0.9))
            .await
            .unwrap();
        RoomTagStorage::add_tag(&pool, &user_id, &room_id, "m.lowpriority", Some(0.1))
            .await
            .unwrap();

        let tags = RoomTagStorage::get_tags(&pool, &user_id, &room_id)
            .await
            .unwrap();

        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag, "m.lowpriority");
        assert_eq!(tags[0].order, Some(0.1));
    });
}
