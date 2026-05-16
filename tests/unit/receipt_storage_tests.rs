#![cfg(test)]

use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::storage::room::RoomStorage;
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
                "Skipping receipt storage tests because test database is unavailable: {error}"
            );
            None
        }
    }
}

#[test]
fn test_add_receipt_preserves_receipt_data_payload() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let room_storage = RoomStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!receipt_data_{suffix}:localhost");
        let user_id = format!("@receipt_user_{suffix}:localhost");
        let event_id = format!("$receipt_event_{suffix}:localhost");
        let data = json!({
            "thread_id": "main"
        });

        room_storage
            .add_receipt(&user_id, &user_id, &room_id, &event_id, "m.read", &data)
            .await
            .unwrap();

        let receipts = room_storage
            .get_receipts(&room_id, "m.read", &event_id)
            .await
            .unwrap();

        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].data, data);
    });
}

#[test]
fn test_add_receipt_replaces_previous_event_for_same_user_and_type() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };

        let room_storage = RoomStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!receipt_replace_{suffix}:localhost");
        let user_id = format!("@receipt_user_{suffix}:localhost");
        let first_event_id = format!("$receipt_first_{suffix}:localhost");
        let second_event_id = format!("$receipt_second_{suffix}:localhost");

        room_storage
            .add_receipt(
                &user_id,
                &user_id,
                &room_id,
                &first_event_id,
                "m.read",
                &json!({"thread_id": "main"}),
            )
            .await
            .unwrap();

        room_storage
            .add_receipt(
                &user_id,
                &user_id,
                &room_id,
                &second_event_id,
                "m.read",
                &json!({"thread_id": "main"}),
            )
            .await
            .unwrap();

        let old_receipts = room_storage
            .get_receipts(&room_id, "m.read", &first_event_id)
            .await
            .unwrap();
        let new_receipts = room_storage
            .get_receipts(&room_id, "m.read", &second_event_id)
            .await
            .unwrap();

        assert!(old_receipts.is_empty());
        assert_eq!(new_receipts.len(), 1);
        assert_eq!(new_receipts[0].user_id, user_id);
        assert_eq!(new_receipts[0].event_id, second_event_id);
    });
}
