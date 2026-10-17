#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_storage::room::RoomStorage;
static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn setup_test_database(_pool: &Arc<sqlx::PgPool>) {
    // Tables are created by the shared test pool (crate::require_test_pool).
}

#[tokio::test]
async fn test_add_receipt_preserves_receipt_data_payload() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool);

    let room_storage = RoomStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!receipt_data_{suffix}:localhost");
    let user_id = format!("@receipt_user_{suffix}:localhost");
    let event_id = format!("$receipt_event_{suffix}:localhost");
    let data = json!({
        "thread_id": "main"
    });

    room_storage.add_receipt(&user_id, &user_id, &room_id, &event_id, "m.read", &data).await.unwrap();

    let receipts = room_storage.get_receipts(&room_id, "m.read", &event_id).await.unwrap();

    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].data, data);
}

#[tokio::test]
async fn test_add_receipt_replaces_previous_event_for_same_user_and_type() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool);

    let room_storage = RoomStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!receipt_replace_{suffix}:localhost");
    let user_id = format!("@receipt_user_{suffix}:localhost");
    let first_event_id = format!("$receipt_first_{suffix}:localhost");
    let second_event_id = format!("$receipt_second_{suffix}:localhost");

    room_storage
        .add_receipt(&user_id, &user_id, &room_id, &first_event_id, "m.read", &json!({"thread_id": "main"}))
        .await
        .unwrap();

    room_storage
        .add_receipt(&user_id, &user_id, &room_id, &second_event_id, "m.read", &json!({"thread_id": "main"}))
        .await
        .unwrap();

    let old_receipts = room_storage.get_receipts(&room_id, "m.read", &first_event_id).await.unwrap();
    let new_receipts = room_storage.get_receipts(&room_id, "m.read", &second_event_id).await.unwrap();

    assert!(old_receipts.is_empty());
    assert_eq!(new_receipts.len(), 1);
    assert_eq!(new_receipts[0].user_id, user_id);
    assert_eq!(new_receipts[0].event_id, second_event_id);
}

/// MSC2409: appservice ephemeral delivery pages a per-service position over the
/// `event_receipts` stream, so the stream must expose a monotonic head plus a
/// bounded, ordered, half-open paging window.
#[tokio::test]
async fn test_receipt_stream_position_advances_and_pages_in_order() {
    let pool = crate::require_test_pool().await;
    let room_storage = RoomStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!receipt_stream_{suffix}:localhost");
    let user_id = format!("@receipt_stream_user_{suffix}:localhost");
    let first_event_id = format!("$receipt_stream_first_{suffix}:localhost");
    let second_event_id = format!("$receipt_stream_second_{suffix}:localhost");

    let before = room_storage.get_latest_receipt_position().await.unwrap();

    room_storage.add_receipt(&user_id, &user_id, &room_id, &first_event_id, "m.read", &json!({})).await.unwrap();
    room_storage.add_receipt(&user_id, &user_id, &room_id, &second_event_id, "m.read", &json!({})).await.unwrap();

    let after = room_storage.get_latest_receipt_position().await.unwrap();
    assert!(after > before, "stream position must advance after writing receipts");

    // Rows must be paged back in ascending stream order.
    let rows = room_storage.get_receipts_since(before, after, 1000).await.unwrap();
    assert!(rows.windows(2).all(|w| w[0].stream_id < w[1].stream_id), "rows must come back in ascending stream order");
    let ours: Vec<&str> = rows.iter().filter(|r| r.room_id == room_id).map(|r| r.event_id.as_str()).collect();
    assert_eq!(ours, vec![first_event_id.as_str(), second_event_id.as_str()]);

    // `limit` caps a single page.
    let one = room_storage.get_receipts_since(before, after, 1).await.unwrap();
    assert_eq!(one.len(), 1);
    assert!(one[0].stream_id > before && one[0].stream_id <= after);

    // An exhausted window yields nothing (the loop's terminal condition).
    let exhausted = room_storage.get_receipts_since(after, after, 1000).await.unwrap();
    assert!(exhausted.is_empty());
}

/// Regression (main 上的既存缺陷): the cursor used to be `event_receipts.id`,
/// which `ON CONFLICT DO UPDATE` rewrites **in place** without moving. A second
/// receipt for the same `(event_id, room_id, user_id, receipt_type)` therefore
/// left the head unchanged and was invisible to appservice delivery. The
/// append-only `event_receipts_stream` log must advance on every write.
#[tokio::test]
async fn test_receipt_stream_advances_on_same_key_update() {
    let pool = crate::require_test_pool().await;
    let room_storage = RoomStorage::new(&pool);
    let suffix = unique_id();
    let room_id = format!("!receipt_update_{suffix}:localhost");
    let user_id = format!("@receipt_update_user_{suffix}:localhost");
    let event_id = format!("$receipt_update_event_{suffix}:localhost");

    room_storage
        .add_receipt(&user_id, &user_id, &room_id, &event_id, "m.read", &json!({"thread_id": "main"}))
        .await
        .unwrap();
    let before = room_storage.get_latest_receipt_position().await.unwrap();

    // Same key ⇒ `event_receipts` UPSERT path (its `id` does not move).
    room_storage
        .add_receipt(&user_id, &user_id, &room_id, &event_id, "m.read", &json!({"thread_id": "side"}))
        .await
        .unwrap();
    let after = room_storage.get_latest_receipt_position().await.unwrap();

    assert!(after > before, "an in-place update must still advance the receipt stream position");

    let rows = room_storage.get_receipts_since(before, after, 1000).await.unwrap();
    let ours: Vec<_> = rows.iter().filter(|r| r.room_id == room_id).collect();
    assert_eq!(ours.len(), 1, "the same-key update must be visible as exactly one new log row");
    assert_eq!(ours[0].event_id, event_id);
    assert_eq!(ours[0].data, json!({"thread_id": "side"}), "the log row must carry the updated payload");

    // The current-state table still holds exactly one row for the key.
    let current = room_storage.get_receipts(&room_id, "m.read", &event_id).await.unwrap();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].data, json!({"thread_id": "side"}));
}
