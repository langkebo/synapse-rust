#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::storage::room_tag::RoomTagStorage;
static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database(_pool: &Arc<sqlx::PgPool>) {
    // Tables are created by the shared test pool (crate::require_test_pool).
}

#[tokio::test]
async fn test_room_tag_storage_round_trip() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let suffix = unique_id();
    let user_id = format!("@room_tag_user_{suffix}:localhost");
    let room_id = format!("!room_tag_room_{suffix}:localhost");

    RoomTagStorage::add_tag(&pool, &user_id, &room_id, "m.favourite", Some(0.25)).await.unwrap();

    let tags = RoomTagStorage::get_tags(&pool, &user_id, &room_id).await.unwrap();

    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].user_id, user_id);
    assert_eq!(tags[0].room_id, room_id);
    assert_eq!(tags[0].tag, "m.favourite");
    assert_eq!(tags[0].order, Some(0.25));
    assert!(tags[0].created_ts > 0);
}

#[tokio::test]
async fn test_room_tag_storage_updates_existing_tag_order() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let suffix = unique_id();
    let user_id = format!("@room_tag_user_{suffix}:localhost");
    let room_id = format!("!room_tag_room_{suffix}:localhost");

    RoomTagStorage::add_tag(&pool, &user_id, &room_id, "m.lowpriority", Some(0.9)).await.unwrap();
    RoomTagStorage::add_tag(&pool, &user_id, &room_id, "m.lowpriority", Some(0.1)).await.unwrap();

    let tags = RoomTagStorage::get_tags(&pool, &user_id, &room_id).await.unwrap();

    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].tag, "m.lowpriority");
    assert_eq!(tags[0].order, Some(0.1));
}
