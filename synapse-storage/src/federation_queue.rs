use serde::{Deserialize, Serialize};
use sqlx::postgres::PgQueryResult;
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FederationQueueEntry {
    pub id: i64,
    pub destination: String,
    pub event_id: String,
    pub event_type: String,
    pub room_id: Option<String>,
    pub content: serde_json::Value,
    pub created_ts: i64,
    pub sent_at: Option<i64>,
    pub retry_count: i32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertFederationQueueRequest {
    pub destination: String,
    pub event_id: String,
    pub event_type: String,
    pub room_id: Option<String>,
    pub content: serde_json::Value,
    pub created_ts: i64,
}

pub struct FederationQueueStorage {
    pool: PgPool,
}

impl FederationQueueStorage {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, req: &InsertFederationQueueRequest) -> Result<i64, sqlx::Error> {
        let row = sqlx::query_as::<_, (i64,)>(
            r"
            INSERT INTO federation_queue (destination, event_id, event_type, room_id, content, created_ts, status)
            VALUES ($1, $2, $3, $4, $5, $6, 'pending')
            RETURNING id
            ",
        )
        .bind(&req.destination)
        .bind(&req.event_id)
        .bind(&req.event_type)
        .bind(&req.room_id)
        .bind(&req.content)
        .bind(req.created_ts)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0)
    }

    pub async fn mark_sent(&self, id: i64, sent_at: i64) -> Result<PgQueryResult, sqlx::Error> {
        sqlx::query(
            r"
            UPDATE federation_queue
            SET status = 'sent', sent_at = $2
            WHERE id = $1
            ",
        )
        .bind(id)
        .bind(sent_at)
        .execute(&self.pool)
        .await
    }

    pub async fn increment_retry(&self, id: i64) -> Result<PgQueryResult, sqlx::Error> {
        sqlx::query(
            r"
            UPDATE federation_queue
            SET retry_count = retry_count + 1, status = 'pending'
            WHERE id = $1
            ",
        )
        .bind(id)
        .execute(&self.pool)
        .await
    }

    pub async fn mark_failed(&self, id: i64) -> Result<PgQueryResult, sqlx::Error> {
        sqlx::query(
            r"
            UPDATE federation_queue
            SET status = 'failed'
            WHERE id = $1
            ",
        )
        .bind(id)
        .execute(&self.pool)
        .await
    }

    pub async fn get_pending_by_destination(
        &self,
        destination: &str,
        limit: i64,
    ) -> Result<Vec<FederationQueueEntry>, sqlx::Error> {
        sqlx::query_as::<_, FederationQueueEntry>(
            r"
            SELECT id, destination, event_id, event_type, room_id, content, created_ts, sent_at, retry_count, status
            FROM federation_queue
            WHERE destination = $1 AND status = 'pending'
            ORDER BY created_ts ASC
            LIMIT $2
            ",
        )
        .bind(destination)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_all_pending(&self) -> Result<Vec<FederationQueueEntry>, sqlx::Error> {
        sqlx::query_as::<_, FederationQueueEntry>(
            r"
            SELECT id, destination, event_id, event_type, room_id, content, created_ts, sent_at, retry_count, status
            FROM federation_queue
            WHERE status = 'pending'
            ORDER BY created_ts ASC
            LIMIT 1000
            ",
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn delete_completed(&self, older_than_ts: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r"
            DELETE FROM federation_queue
            WHERE status IN ('sent', 'failed') AND created_ts < $1
            ",
        )
        .bind(older_than_ts)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn count_pending(&self) -> Result<i64, sqlx::Error> {
        let row =
            sqlx::query_as::<_, (Option<i64>,)>(r"SELECT COUNT(*) FROM federation_queue WHERE status = 'pending'")
                .fetch_one(&self.pool)
                .await?;

        Ok(row.0.unwrap_or(0))
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn now_millis() -> i64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
    }

    async fn test_pool() -> sqlx::PgPool {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database")
    }

    async fn cleanup_queue(pool: &PgPool, suffix: &str) {
        let _ = sqlx::query("DELETE FROM federation_queue WHERE destination LIKE $1")
            .bind(format!("%{suffix}%"))
            .execute(pool)
            .await;
    }

    fn make_entry(suffix: &str, idx: i32, created_ts: i64) -> InsertFederationQueueRequest {
        InsertFederationQueueRequest {
            destination: format!("server-{suffix}.example.com"),
            event_id: format!("$event_{idx}_{suffix}"),
            event_type: "m.room.message".to_string(),
            room_id: Some(format!("!room_{suffix}:example.com")),
            content: serde_json::json!({"body": format!("test {idx}"), "msgtype": "m.text"}),
            created_ts,
        }
    }

    // --- insert ---

    #[tokio::test]
    async fn test_insert_returns_id() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_queue(&pool, &suffix).await;

        let storage = FederationQueueStorage::new(pool.clone());
        let id = storage.insert(&make_entry(&suffix, 1, now_millis())).await.expect("insert should succeed");

        assert!(id > 0, "insert should return a positive id");

        cleanup_queue(&pool, &suffix).await;
    }

    // --- mark_sent ---

    #[tokio::test]
    async fn test_mark_sent_updates_status_and_sent_at() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_queue(&pool, &suffix).await;

        let storage = FederationQueueStorage::new(pool.clone());
        let now = now_millis();
        let id = storage.insert(&make_entry(&suffix, 1, now)).await.expect("insert should succeed");

        let sent_at = now + 100;
        storage.mark_sent(id, sent_at).await.expect("mark_sent should succeed");

        let (row_id, status, actual_sent_at): (i64, String, Option<i64>) =
            sqlx::query_as("SELECT id, status, sent_at FROM federation_queue WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("query should succeed");

        assert_eq!(row_id, id);
        assert_eq!(status, "sent", "status should be 'sent' after mark_sent");
        assert_eq!(actual_sent_at, Some(sent_at), "sent_at should be set to the provided timestamp");

        cleanup_queue(&pool, &suffix).await;
    }

    // --- increment_retry ---

    #[tokio::test]
    async fn test_increment_retry_increases_count() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_queue(&pool, &suffix).await;

        let storage = FederationQueueStorage::new(pool.clone());
        let now = now_millis();
        let id = storage.insert(&make_entry(&suffix, 1, now)).await.expect("insert should succeed");

        // First increment
        storage.increment_retry(id).await.expect("increment_retry should succeed");

        let (_, retry_count): (i64, i32) = sqlx::query_as("SELECT id, retry_count FROM federation_queue WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("query should succeed");

        assert_eq!(retry_count, 1, "retry_count should be 1 after one increment");

        // Second increment to verify accumulation
        storage.increment_retry(id).await.expect("second increment_retry should succeed");

        let (_, retry_count2): (i64, i32) =
            sqlx::query_as("SELECT id, retry_count FROM federation_queue WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("query should succeed");

        assert_eq!(retry_count2, 2, "retry_count should be 2 after two increments");

        cleanup_queue(&pool, &suffix).await;
    }

    // --- mark_failed ---

    #[tokio::test]
    async fn test_mark_failed_updates_status() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_queue(&pool, &suffix).await;

        let storage = FederationQueueStorage::new(pool.clone());
        let now = now_millis();
        let id = storage.insert(&make_entry(&suffix, 1, now)).await.expect("insert should succeed");

        storage.mark_failed(id).await.expect("mark_failed should succeed");

        let (_, status): (i64, String) = sqlx::query_as("SELECT id, status FROM federation_queue WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("query should succeed");

        assert_eq!(status, "failed", "status should be 'failed' after mark_failed");

        cleanup_queue(&pool, &suffix).await;
    }

    // --- get_pending_by_destination ---

    #[tokio::test]
    async fn test_get_pending_by_destination_filters_by_destination() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let dest = format!("server-{suffix}.example.com");
        cleanup_queue(&pool, &suffix).await;

        let storage = FederationQueueStorage::new(pool.clone());
        let now = now_millis();

        // Insert 2 entries for our destination
        storage.insert(&make_entry(&suffix, 1, now)).await.expect("insert 1 should succeed");
        storage.insert(&make_entry(&suffix, 2, now + 1)).await.expect("insert 2 should succeed");

        // Insert an entry for a different destination
        let mut entry3 = make_entry(&suffix, 3, now + 2);
        entry3.destination = format!("other-{suffix}.example.com");
        storage.insert(&entry3).await.expect("insert 3 should succeed");

        let results =
            storage.get_pending_by_destination(&dest, 100).await.expect("get_pending_by_destination should succeed");

        assert_eq!(results.len(), 2, "should return only entries for the matching destination");
        for entry in &results {
            assert_eq!(entry.destination, dest, "all returned entries should match the requested destination");
        }

        cleanup_queue(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_pending_by_destination_orders_by_created_ts_asc() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let dest = format!("server-{suffix}.example.com");
        cleanup_queue(&pool, &suffix).await;

        let storage = FederationQueueStorage::new(pool.clone());
        let base_ts = now_millis();

        // Insert 3 entries with different timestamps (intentionally out of order)
        storage.insert(&make_entry(&suffix, 1, base_ts + 200)).await.expect("insert 1 should succeed");
        storage.insert(&make_entry(&suffix, 2, base_ts)).await.expect("insert 2 should succeed");
        storage.insert(&make_entry(&suffix, 3, base_ts + 100)).await.expect("insert 3 should succeed");

        let results =
            storage.get_pending_by_destination(&dest, 100).await.expect("get_pending_by_destination should succeed");

        assert_eq!(results.len(), 3, "should return all 3 entries");
        for i in 1..results.len() {
            assert!(results[i - 1].created_ts <= results[i].created_ts, "entries should be ordered by created_ts ASC");
        }

        cleanup_queue(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_pending_by_destination_respects_limit() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let dest = format!("server-{suffix}.example.com");
        cleanup_queue(&pool, &suffix).await;

        let storage = FederationQueueStorage::new(pool.clone());
        let now = now_millis();

        // Insert 3 entries
        for i in 0..3 {
            storage.insert(&make_entry(&suffix, i, now + i as i64)).await.expect("insert should succeed");
        }

        // Request with limit of 2
        let results =
            storage.get_pending_by_destination(&dest, 2).await.expect("get_pending_by_destination should succeed");

        assert_eq!(results.len(), 2, "should respect the limit parameter");

        cleanup_queue(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_pending_by_destination_omits_non_pending() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let dest = format!("server-{suffix}.example.com");
        cleanup_queue(&pool, &suffix).await;

        let storage = FederationQueueStorage::new(pool.clone());
        let now = now_millis();

        // Insert a pending entry
        let pending_id = storage.insert(&make_entry(&suffix, 1, now)).await.expect("insert pending should succeed");

        // Insert and mark as sent (same destination)
        let sent_id = storage.insert(&make_entry(&suffix, 2, now + 1)).await.expect("insert sent-about should succeed");
        storage.mark_sent(sent_id, now + 2).await.expect("mark_sent should succeed");

        let results =
            storage.get_pending_by_destination(&dest, 100).await.expect("get_pending_by_destination should succeed");

        assert_eq!(results.len(), 1, "should return only pending entries");
        assert_eq!(results[0].id, pending_id, "should be the pending entry");
        assert_eq!(results[0].status, "pending", "status should be 'pending'");

        cleanup_queue(&pool, &suffix).await;
    }

    // --- get_all_pending ---

    #[tokio::test]
    async fn test_get_all_pending_returns_only_pending() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_queue(&pool, &suffix).await;

        let storage = FederationQueueStorage::new(pool.clone());
        let now = now_millis();

        // Insert a pending entry
        let pending_id = storage.insert(&make_entry(&suffix, 1, now)).await.expect("insert pending should succeed");

        // Insert and mark as sent
        let sent_id = storage.insert(&make_entry(&suffix, 2, now + 1)).await.expect("insert sent-about should succeed");
        storage.mark_sent(sent_id, now + 2).await.expect("mark_sent should succeed");

        // Insert and mark as failed
        let failed_id =
            storage.insert(&make_entry(&suffix, 3, now + 3)).await.expect("insert failed-about should succeed");
        storage.mark_failed(failed_id).await.expect("mark_failed should succeed");

        let results = storage.get_all_pending().await.expect("get_all_pending should succeed");

        // Filter results to find our test entries
        let our_results: Vec<_> = results.iter().filter(|e| e.destination.contains(&suffix)).collect();
        assert_eq!(our_results.len(), 1, "should return only the pending entry from our test data");
        assert_eq!(our_results[0].id, pending_id, "should be the pending entry");
        assert_eq!(our_results[0].status, "pending", "status should be 'pending'");

        cleanup_queue(&pool, &suffix).await;
    }

    // --- delete_completed ---

    #[tokio::test]
    async fn test_delete_completed_removes_old_completed_and_keeps_pending() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_queue(&pool, &suffix).await;

        let storage = FederationQueueStorage::new(pool.clone());
        let now = now_millis();

        // Old sent entry (should be deleted)
        let old_sent_id = storage
            .insert(&InsertFederationQueueRequest {
                event_id: format!("$old_sent_{suffix}"),
                room_id: None,
                ..make_entry(&suffix, 1, now - 100_000)
            })
            .await
            .expect("insert old sent should succeed");
        storage.mark_sent(old_sent_id, now - 90_000).await.expect("mark_sent should succeed");

        // Old failed entry (should be deleted)
        let old_failed_id = storage
            .insert(&InsertFederationQueueRequest {
                event_id: format!("$old_failed_{suffix}"),
                room_id: None,
                ..make_entry(&suffix, 2, now - 100_000)
            })
            .await
            .expect("insert old failed should succeed");
        storage.mark_failed(old_failed_id).await.expect("mark_failed should succeed");

        // Old pending entry (should NOT be deleted)
        let old_pending_id = storage
            .insert(&InsertFederationQueueRequest {
                event_id: format!("$old_pending_{suffix}"),
                room_id: None,
                ..make_entry(&suffix, 3, now - 100_000)
            })
            .await
            .expect("insert old pending should succeed");

        // Delete completed entries older than threshold
        let threshold = now - 50_000;
        let deleted = storage.delete_completed(threshold).await.expect("delete_completed should succeed");

        assert!(deleted >= 2, "should delete at least 2 completed entries");

        // Verify old sent entry is gone
        let check = sqlx::query_as::<_, (i64,)>("SELECT id FROM federation_queue WHERE id = $1")
            .bind(old_sent_id)
            .fetch_optional(&pool)
            .await
            .expect("query should succeed");
        assert!(check.is_none(), "old sent entry should be deleted");

        // Verify old failed entry is gone
        let check = sqlx::query_as::<_, (i64,)>("SELECT id FROM federation_queue WHERE id = $1")
            .bind(old_failed_id)
            .fetch_optional(&pool)
            .await
            .expect("query should succeed");
        assert!(check.is_none(), "old failed entry should be deleted");

        // Verify old pending entry is still present
        let check = sqlx::query_as::<_, (i64, String)>("SELECT id, status FROM federation_queue WHERE id = $1")
            .bind(old_pending_id)
            .fetch_optional(&pool)
            .await
            .expect("query should succeed");
        assert!(check.is_some(), "old pending entry should NOT be deleted");
        assert_eq!(check.unwrap().1, "pending", "pending entry should still have 'pending' status");

        cleanup_queue(&pool, &suffix).await;
    }

    // --- count_pending ---

    #[tokio::test]
    async fn test_count_pending_returns_correct_count() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_queue(&pool, &suffix).await;

        let storage = FederationQueueStorage::new(pool.clone());
        let now = now_millis();

        // Insert 3 pending entries
        for i in 0..3 {
            storage.insert(&make_entry(&suffix, i, now + i as i64)).await.expect("insert pending should succeed");
        }

        // Insert a sent entry (should NOT be counted)
        let sent_id = storage.insert(&make_entry(&suffix, 10, now + 10)).await.expect("insert should succeed");
        storage.mark_sent(sent_id, now + 11).await.expect("mark_sent should succeed");

        // Insert a failed entry (should NOT be counted)
        let failed_id = storage.insert(&make_entry(&suffix, 11, now + 12)).await.expect("insert should succeed");
        storage.mark_failed(failed_id).await.expect("mark_failed should succeed");

        let total = storage.count_pending().await.expect("count_pending should succeed");

        // Verify our specific pending entries via direct query
        let our_pending: (Option<i64>,) =
            sqlx::query_as("SELECT COUNT(*) FROM federation_queue WHERE destination LIKE $1 AND status = 'pending'")
                .bind(format!("%{suffix}%"))
                .fetch_one(&pool)
                .await
                .expect("query should succeed");

        assert_eq!(our_pending.0, Some(3), "should have exactly 3 pending entries with our suffix");

        // The global count should include our 3 entries
        assert!(total >= 3, "global count should be at least 3, got {total}");

        cleanup_queue(&pool, &suffix).await;
    }
}
