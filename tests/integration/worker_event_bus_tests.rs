//! The **write** side of the worker event bus (`worker_events`) — P-5.
//!
//! `worker_events` is the replication stream that `GET .../worker/events` serves
//! (`WorkerManager::get_events_since` → `WorkerStoreApi::get_events_since`). It
//! shipped with a read side and **no** write side: nothing ever inserted a row, so
//! the endpoint returned an empty list forever, silently
//! (`docs/synapse-rust-vs-synapse-comparison.md` §18.7 P-5). The write side is
//! `WorkerManagerEventSink` → `WorkerStoreApi::add_event`, published from the
//! `EventWriter` decorator when `worker.enabled` is set.
//!
//! Two invariants need a real database, so they live here:
//!
//! * publishing the same event twice yields **one** row and keeps the original
//!   `stream_id` — a worker must never be told to replay an event at a new
//!   position (the table has `UNIQUE (event_id)`, and republishing is legitimate:
//!   federation backfill, retries);
//! * retention is bounded — stale rows are pruned, fresh ones survive. Without it
//!   the bus is one unbounded row per room event in worker deployments.
//!
//! The decorator's side (publish on commit, stay silent inside a transaction) is
//! unit-tested in `synapse-services::notifying_event_writer`.

use std::sync::Arc;

use synapse_storage::worker::{WorkerStorage, WorkerStoreApi};

async fn worker_storage() -> (Arc<sqlx::PgPool>, Arc<dyn WorkerStoreApi>) {
    let pool = crate::require_test_pool().await;
    let storage: Arc<dyn WorkerStoreApi> = Arc::new(WorkerStorage::new(&pool));
    (pool, storage)
}

async fn count_for_event(pool: &sqlx::PgPool, event_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM worker_events WHERE event_id = $1")
        .bind(event_id)
        .fetch_one(pool)
        .await
        .expect("worker_events readable")
}

#[tokio::test]
async fn publishing_the_same_event_twice_keeps_the_original_stream_id() {
    let (pool, storage) = worker_storage().await;
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let event_id = format!("$workerbus_{suffix}:localhost");
    let room_id = format!("!workerbus_{suffix}:localhost");
    let sender = format!("@workerbus_{suffix}:localhost");

    let first = storage
        .add_event(&event_id, "m.room.message", Some(&room_id), Some(&sender), serde_json::json!({ "body": "one" }))
        .await
        .expect("first publish");
    let second = storage
        .add_event(&event_id, "m.room.message", Some(&room_id), Some(&sender), serde_json::json!({ "body": "two" }))
        .await
        .expect("a republish must be tolerated, not raise a unique violation");

    assert_eq!(first.stream_id, second.stream_id, "a republish must keep the original position");
    assert_eq!(first.id, second.id, "a republish must not insert a second row");
    assert_eq!(count_for_event(&pool, &event_id).await, 1, "worker_events holds exactly one row per event_id");

    // The point of the bus: a worker polling from just before the position sees it.
    let served = storage
        .get_events_since(first.stream_id - 1, 10)
        .await
        .expect("get_events_since must serve the published event");
    assert!(
        served.iter().any(|event| event.event_id == event_id),
        "the published event must be visible to a worker polling get_events_since"
    );

    sqlx::query("DELETE FROM worker_events WHERE event_id = $1")
        .bind(&event_id)
        .execute(&*pool)
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn retention_prunes_stale_events_and_keeps_fresh_ones() {
    let (pool, storage) = worker_storage().await;
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let stale = format!("$workerbus_stale_{suffix}:localhost");
    let fresh = format!("$workerbus_fresh_{suffix}:localhost");

    storage.add_event(&stale, "m.room.message", None, None, serde_json::json!({})).await.expect("stale publish");
    storage.add_event(&fresh, "m.room.message", None, None, serde_json::json!({})).await.expect("fresh publish");

    // Backdate only the stale row: retention keys on `created_ts`.
    let old_ts = synapse_common::current_timestamp_millis()
        - ((synapse_storage::pruning::WORKER_EVENTS_RETENTION_DAYS + 1) * 86_400_000);
    sqlx::query("UPDATE worker_events SET created_ts = $1 WHERE event_id = $2")
        .bind(old_ts)
        .bind(&stale)
        .execute(&*pool)
        .await
        .expect("backdate the stale row");

    let deleted = synapse_storage::pruning::prune_old_worker_events(&pool).await.expect("prune");
    assert!(deleted >= 1, "the stale row must be pruned, deleted={deleted}");

    assert_eq!(count_for_event(&pool, &stale).await, 0, "a stale event must not survive retention");
    assert_eq!(count_for_event(&pool, &fresh).await, 1, "a fresh event must survive retention");

    sqlx::query("DELETE FROM worker_events WHERE event_id = $1").bind(&fresh).execute(&*pool).await.expect("cleanup");
}
