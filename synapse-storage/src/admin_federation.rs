use async_trait::async_trait;
use sqlx::{PgPool, Row};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FederationDestinationRecord {
    pub server_name: Option<String>,
    pub last_failed_connect_at: Option<i64>,
    pub last_successful_connect_at: Option<i64>,
    pub failure_count: Option<i32>,
    pub status: Option<String>,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PendingFederationRecord {
    pub server_name: String,
    pub failure_count: Option<i32>,
    pub last_failed_connect_at: Option<i64>,
    pub last_successful_connect_at: Option<i64>,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct FederationCacheRecord {
    pub key: String,
    pub value: Option<String>,
    pub expiry_ts: Option<i64>,
}

#[async_trait]
pub trait AdminFederationStoreApi: Send + Sync {
    async fn count_destinations(&self) -> Result<i64, sqlx::Error>;
    async fn list_destinations(
        &self,
        after_server_name: Option<&str>,
        limit: i64,
    ) -> Result<Vec<FederationDestinationRecord>, sqlx::Error>;
    async fn get_destination(&self, server_name: &str) -> Result<Option<FederationDestinationRecord>, sqlx::Error>;
    async fn reset_connection(&self, server_name: &str) -> Result<u64, sqlx::Error>;
    async fn delete_destination(&self, server_name: &str) -> Result<u64, sqlx::Error>;
    async fn destination_exists(&self, server_name: &str) -> Result<bool, sqlx::Error>;
    async fn get_destination_rooms(&self, server_name: &str) -> Result<Vec<String>, sqlx::Error>;
    async fn count_distinct_rooms_by_sender_server(&self, server_name: &str) -> Result<i64, sqlx::Error>;
    async fn get_destination_status(&self, server_name: &str) -> Result<Option<String>, sqlx::Error>;
    async fn get_server_admission_status(&self, server_name: &str) -> Result<Option<Option<String>>, sqlx::Error>;
    async fn insert_pending_server(&self, server_name: &str, now_ts: i64) -> Result<u64, sqlx::Error>;
    async fn update_destination_status(
        &self,
        server_name: &str,
        status: &str,
        updated_ts: i64,
    ) -> Result<u64, sqlx::Error>;
    async fn list_pending_federation(
        &self,
        updated_ts: Option<i64>,
        server_name: Option<&str>,
        limit: i64,
    ) -> Result<Vec<PendingFederationRecord>, sqlx::Error>;
    async fn count_pending_federation(&self) -> Result<i64, sqlx::Error>;
    async fn get_federation_cache(&self) -> Result<Vec<FederationCacheRecord>, sqlx::Error>;
    async fn delete_federation_cache_entry(&self, key: &str) -> Result<u64, sqlx::Error>;
    async fn clear_federation_cache(&self) -> Result<u64, sqlx::Error>;
}

#[derive(Debug, Clone)]
pub struct AdminFederationStorage {
    pool: Arc<PgPool>,
}

impl AdminFederationStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn count_destinations(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM federation_servers").fetch_one(&*self.pool).await
    }

    pub async fn list_destinations(
        &self,
        after_server_name: Option<&str>,
        limit: i64,
    ) -> Result<Vec<FederationDestinationRecord>, sqlx::Error> {
        if let Some(after_server_name) = after_server_name {
            sqlx::query_as::<_, FederationDestinationRecord>(
                r"
                SELECT server_name, last_failed_connect_at, last_successful_connect_at, failure_count, status, updated_ts
                FROM federation_servers
                WHERE server_name > $1
                ORDER BY server_name ASC
                LIMIT $2
                ",
            )
            .bind(after_server_name)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as::<_, FederationDestinationRecord>(
                r"
                SELECT server_name, last_failed_connect_at, last_successful_connect_at, failure_count, status, updated_ts
                FROM federation_servers
                ORDER BY server_name ASC
                LIMIT $1
                ",
            )
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        }
    }

    pub async fn get_destination(&self, server_name: &str) -> Result<Option<FederationDestinationRecord>, sqlx::Error> {
        sqlx::query_as::<_, FederationDestinationRecord>(
            r"
            SELECT server_name, last_failed_connect_at, last_successful_connect_at, failure_count, status, updated_ts
            FROM federation_servers
            WHERE server_name = $1
            ",
        )
        .bind(server_name)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn reset_connection(&self, server_name: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE federation_servers SET last_failed_connect_at = NULL, failure_count = 0 WHERE server_name = $1",
        )
        .bind(server_name)
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn delete_destination(&self, server_name: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM federation_servers WHERE server_name = $1")
            .bind(server_name)
            .execute(&*self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn destination_exists(&self, server_name: &str) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM federation_servers WHERE server_name = $1)")
            .bind(server_name)
            .fetch_one(&*self.pool)
            .await
    }

    pub async fn get_destination_rooms(&self, server_name: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<Option<String>> = sqlx::query_scalar(
            "SELECT DISTINCT room_id FROM federation_queue WHERE destination = $1 AND room_id IS NOT NULL ORDER BY room_id",
        )
        .bind(server_name)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().flatten().collect())
    }

    pub async fn count_distinct_rooms_by_sender_server(&self, server_name: &str) -> Result<i64, sqlx::Error> {
        let suffix = format!("%:{server_name}");
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(DISTINCT room_id) FROM events WHERE sender LIKE $1 AND state_key IS NOT NULL",
        )
        .bind(suffix)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_destination_status(&self, server_name: &str) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            "SELECT COALESCE(status, 'active') FROM federation_servers WHERE server_name = $1",
        )
        .bind(server_name)
        .fetch_optional(&*self.pool)
        .await
    }

    /// Returns the raw `status` column for a federation server (no COALESCE).
    ///
    /// Returns `None` when the server is not present in `federation_servers`.
    /// Returns `Some(None)` when the row exists but `status` is NULL.
    /// Used by the federation admission middleware to distinguish "unknown
    /// server" from "known server with explicit status".
    pub async fn get_server_admission_status(&self, server_name: &str) -> Result<Option<Option<String>>, sqlx::Error> {
        sqlx::query_scalar::<_, Option<String>>("SELECT status FROM federation_servers WHERE server_name = $1")
            .bind(server_name)
            .fetch_optional(&*self.pool)
            .await
    }

    /// Inserts a new federation server row with `status = 'pending'`.
    ///
    /// Uses `ON CONFLICT DO NOTHING` so concurrent admission probes for the
    /// same server do not clobber an existing row. Returns the number of
    /// rows actually inserted (0 if the server already existed).
    pub async fn insert_pending_server(&self, server_name: &str, now_ts: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "INSERT INTO federation_servers (server_name, status, updated_ts) \
             VALUES ($1, 'pending', $2) \
             ON CONFLICT (server_name) DO NOTHING",
        )
        .bind(server_name)
        .bind(now_ts)
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn update_destination_status(
        &self,
        server_name: &str,
        status: &str,
        updated_ts: i64,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("UPDATE federation_servers SET status = $1, updated_ts = $2 WHERE server_name = $3")
            .bind(status)
            .bind(updated_ts)
            .bind(server_name)
            .execute(&*self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn list_pending_federation(
        &self,
        updated_ts: Option<i64>,
        server_name: Option<&str>,
        limit: i64,
    ) -> Result<Vec<PendingFederationRecord>, sqlx::Error> {
        sqlx::query_as::<_, PendingFederationRecord>(
            "SELECT server_name, failure_count, last_failed_connect_at, last_successful_connect_at, updated_ts \
             FROM federation_servers WHERE status = 'pending' \
               AND (($1::BIGINT IS NULL AND $2::TEXT IS NULL)
               OR COALESCE(updated_ts, 0) < $1
               OR (COALESCE(updated_ts, 0) = $1 AND server_name < $2)) \
             ORDER BY COALESCE(updated_ts, 0) DESC, server_name DESC \
             LIMIT $3",
        )
        .bind(updated_ts)
        .bind(server_name)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn count_pending_federation(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM federation_servers WHERE status = 'pending'")
            .fetch_one(&*self.pool)
            .await
    }

    pub async fn get_federation_cache(&self) -> Result<Vec<FederationCacheRecord>, sqlx::Error> {
        let rows = sqlx::query("SELECT key, value, expiry_ts FROM federation_cache ORDER BY key")
            .fetch_all(&*self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| FederationCacheRecord {
                key: row.get("key"),
                value: row.try_get::<Option<String>, _>("value").ok().flatten(),
                expiry_ts: row.try_get::<Option<i64>, _>("expiry_ts").ok().flatten(),
            })
            .collect())
    }

    pub async fn delete_federation_cache_entry(&self, key: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM federation_cache WHERE key = $1").bind(key).execute(&*self.pool).await?;
        Ok(result.rows_affected())
    }

    pub async fn clear_federation_cache(&self) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM federation_cache").execute(&*self.pool).await?;
        Ok(result.rows_affected())
    }
}

#[async_trait]
impl AdminFederationStoreApi for AdminFederationStorage {
    async fn count_destinations(&self) -> Result<i64, sqlx::Error> {
        self.count_destinations().await
    }

    async fn list_destinations(
        &self,
        after_server_name: Option<&str>,
        limit: i64,
    ) -> Result<Vec<FederationDestinationRecord>, sqlx::Error> {
        self.list_destinations(after_server_name, limit).await
    }

    async fn get_destination(&self, server_name: &str) -> Result<Option<FederationDestinationRecord>, sqlx::Error> {
        self.get_destination(server_name).await
    }

    async fn reset_connection(&self, server_name: &str) -> Result<u64, sqlx::Error> {
        self.reset_connection(server_name).await
    }

    async fn delete_destination(&self, server_name: &str) -> Result<u64, sqlx::Error> {
        self.delete_destination(server_name).await
    }

    async fn destination_exists(&self, server_name: &str) -> Result<bool, sqlx::Error> {
        self.destination_exists(server_name).await
    }

    async fn get_destination_rooms(&self, server_name: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_destination_rooms(server_name).await
    }

    async fn count_distinct_rooms_by_sender_server(&self, server_name: &str) -> Result<i64, sqlx::Error> {
        self.count_distinct_rooms_by_sender_server(server_name).await
    }

    async fn get_destination_status(&self, server_name: &str) -> Result<Option<String>, sqlx::Error> {
        self.get_destination_status(server_name).await
    }

    async fn get_server_admission_status(&self, server_name: &str) -> Result<Option<Option<String>>, sqlx::Error> {
        self.get_server_admission_status(server_name).await
    }

    async fn insert_pending_server(&self, server_name: &str, now_ts: i64) -> Result<u64, sqlx::Error> {
        self.insert_pending_server(server_name, now_ts).await
    }

    async fn update_destination_status(
        &self,
        server_name: &str,
        status: &str,
        updated_ts: i64,
    ) -> Result<u64, sqlx::Error> {
        self.update_destination_status(server_name, status, updated_ts).await
    }

    async fn list_pending_federation(
        &self,
        updated_ts: Option<i64>,
        server_name: Option<&str>,
        limit: i64,
    ) -> Result<Vec<PendingFederationRecord>, sqlx::Error> {
        self.list_pending_federation(updated_ts, server_name, limit).await
    }

    async fn count_pending_federation(&self) -> Result<i64, sqlx::Error> {
        self.count_pending_federation().await
    }

    async fn get_federation_cache(&self) -> Result<Vec<FederationCacheRecord>, sqlx::Error> {
        self.get_federation_cache().await
    }

    async fn delete_federation_cache_entry(&self, key: &str) -> Result<u64, sqlx::Error> {
        self.delete_federation_cache_entry(key).await
    }

    async fn clear_federation_cache(&self) -> Result<u64, sqlx::Error> {
        self.clear_federation_cache().await
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use std::env;
    use uuid::Uuid;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn cleanup_server_prefix(pool: &PgPool, prefix: &str) {
        sqlx::query("DELETE FROM federation_servers WHERE server_name LIKE $1").bind(prefix).execute(pool).await.ok();
    }

    async fn cleanup_queue_prefix(pool: &PgPool, prefix: &str) {
        sqlx::query("DELETE FROM federation_queue WHERE destination LIKE $1").bind(prefix).execute(pool).await.ok();
    }

    async fn cleanup_cache_prefix(pool: &PgPool, prefix: &str) {
        sqlx::query("DELETE FROM federation_cache WHERE key LIKE $1").bind(prefix).execute(pool).await.ok();
    }

    async fn insert_test_server(pool: &PgPool, server_name: &str, status: &str, updated_ts: i64) {
        sqlx::query("INSERT INTO federation_servers (server_name, status, updated_ts) VALUES ($1, $2, $3)")
            .bind(server_name)
            .bind(status)
            .bind(updated_ts)
            .execute(pool)
            .await
            .expect("insert_test_server should succeed");
    }

    // 1. count_destinations returns total count of federation_servers.
    #[tokio::test]
    async fn test_count_destinations() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let prefix = format!("test-count-{}%", suffix);

        cleanup_server_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        for i in 0..3 {
            insert_test_server(&pool, &format!("test-count-{}-{}.com", i, suffix), "active", now).await;
        }

        let count = storage.count_destinations().await.expect("count_destinations should succeed");
        assert!(count >= 3, "should count at least 3 servers, got {count}");

        cleanup_server_prefix(&pool, &prefix).await;
    }

    // 2. list_destinations returns cursor-paginated results ordered by server_name.
    #[tokio::test]
    async fn test_list_destinations_pagination() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let prefix = format!("test-list-{}%", suffix);

        cleanup_server_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        let servers: Vec<String> = (0..5).map(|i| format!("test-list-{:02}-{}.com", i, suffix)).collect();
        for s in &servers {
            insert_test_server(&pool, s, "active", now).await;
        }

        // First page: no cursor, limit=3.
        let page1 = storage.list_destinations(None, 3).await.expect("list_destinations page 1 should succeed");
        assert!(!page1.is_empty(), "first page should not be empty");

        // Collect all our test servers across pages by paginating until done.
        let mut all_seen: Vec<String> = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let page = storage.list_destinations(cursor.as_deref(), 3).await.expect("list_destinations should succeed");
            if page.is_empty() {
                break;
            }
            for row in &page {
                if let Some(name) = &row.server_name {
                    all_seen.push(name.clone());
                }
            }
            cursor = page.last().and_then(|r| r.server_name.clone());
        }

        // All our test servers should appear in the full result set.
        for s in &servers {
            assert!(all_seen.contains(s), "server {} should appear in paginated results", s);
        }

        // Verify ascending order across results.
        for w in all_seen.windows(2) {
            assert!(w[0] < w[1], "server_name should be ascending: {} < {}", w[0], w[1]);
        }

        cleanup_server_prefix(&pool, &prefix).await;
    }

    // 3. get_destination retrieves a specific server record.
    #[tokio::test]
    async fn test_get_destination_found() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-get-{}.com", suffix);
        let prefix = format!("test-get-{}%", suffix);

        cleanup_server_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        insert_test_server(&pool, &server_name, "active", now).await;

        let dest = storage
            .get_destination(&server_name)
            .await
            .expect("get_destination should succeed")
            .expect("server should be found");
        assert_eq!(dest.server_name.as_deref(), Some(server_name.as_str()));
        assert_eq!(dest.status.as_deref(), Some("active"));

        cleanup_server_prefix(&pool, &prefix).await;
    }

    // 4. get_destination returns None for unknown server.
    #[tokio::test]
    async fn test_get_destination_not_found() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-get-nonexist-{}.com", suffix);

        let result = storage.get_destination(&server_name).await.expect("get_destination should succeed");
        assert!(result.is_none(), "unknown server should return None");
    }

    // 5. reset_connection clears last_failed_connect_at and failure_count.
    #[tokio::test]
    async fn test_reset_connection() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-reset-{}.com", suffix);
        let prefix = format!("test-reset-{}%", suffix);

        cleanup_server_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        // Insert with failure data.
        sqlx::query(
            "INSERT INTO federation_servers (server_name, status, updated_ts, last_failed_connect_at, failure_count) \
             VALUES ($1, 'active', $2, $3, $4)",
        )
        .bind(&server_name)
        .bind(now)
        .bind(now - 1000)
        .bind(5)
        .execute(&*pool)
        .await
        .expect("insert with failure data should succeed");

        let affected = storage.reset_connection(&server_name).await.expect("reset_connection should succeed");
        assert_eq!(affected, 1);

        // Verify fields were cleared.
        let dest = storage
            .get_destination(&server_name)
            .await
            .expect("get_destination should succeed")
            .expect("server should exist");
        assert_eq!(dest.last_failed_connect_at, None);
        assert_eq!(dest.failure_count, Some(0));

        cleanup_server_prefix(&pool, &prefix).await;
    }

    // 6. reset_connection returns 0 when server does not exist.
    #[tokio::test]
    async fn test_reset_connection_no_match() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-reset-nonexist-{}.com", suffix);

        let affected = storage.reset_connection(&server_name).await.expect("reset_connection should succeed");
        assert_eq!(affected, 0);
    }

    // 7. delete_destination removes a server row and returns rows_affected.
    #[tokio::test]
    async fn test_delete_destination() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-del-{}.com", suffix);
        let prefix = format!("test-del-{}%", suffix);

        cleanup_server_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        insert_test_server(&pool, &server_name, "active", now).await;

        let affected = storage.delete_destination(&server_name).await.expect("delete_destination should succeed");
        assert_eq!(affected, 1);

        // Verify it is gone.
        let dest = storage.get_destination(&server_name).await.expect("get_destination should succeed");
        assert!(dest.is_none());

        cleanup_server_prefix(&pool, &prefix).await;
    }

    // 8. delete_destination returns 0 when server does not exist.
    #[tokio::test]
    async fn test_delete_destination_no_match() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-del-nonexist-{}.com", suffix);

        let affected = storage.delete_destination(&server_name).await.expect("delete_destination should succeed");
        assert_eq!(affected, 0);
    }

    // 9. destination_exists returns true for a known server.
    #[tokio::test]
    async fn test_destination_exists_true() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-exists-{}.com", suffix);
        let prefix = format!("test-exists-{}%", suffix);

        cleanup_server_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        insert_test_server(&pool, &server_name, "active", now).await;

        let exists = storage.destination_exists(&server_name).await.expect("destination_exists should succeed");
        assert!(exists);

        cleanup_server_prefix(&pool, &prefix).await;
    }

    // 10. destination_exists returns false for an unknown server.
    #[tokio::test]
    async fn test_destination_exists_false() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-exists-nonexist-{}.com", suffix);

        let exists = storage.destination_exists(&server_name).await.expect("destination_exists should succeed");
        assert!(!exists);
    }

    // 11. get_destination_rooms returns distinct room_ids from federation_queue.
    #[tokio::test]
    async fn test_get_destination_rooms() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();

        let server_name = format!("test-rooms-{}.com", suffix);
        let prefix = format!("test-rooms-{}%", suffix);

        cleanup_queue_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        // Insert rows into federation_queue for this destination.
        sqlx::query(
            "INSERT INTO federation_queue (destination, event_id, event_type, room_id, content, created_ts) \
             VALUES ($1, $2, 'm.room.message', $3, '{}'::jsonb, $4)",
        )
        .bind(&server_name)
        .bind(format!("event-a-{}", suffix))
        .bind(format!("!room-a:{}", suffix))
        .bind(now)
        .execute(&*pool)
        .await
        .expect("insert queue entry a should succeed");

        sqlx::query(
            "INSERT INTO federation_queue (destination, event_id, event_type, room_id, content, created_ts) \
             VALUES ($1, $2, 'm.room.message', $3, '{}'::jsonb, $4)",
        )
        .bind(&server_name)
        .bind(format!("event-b-{}", suffix))
        .bind(format!("!room-b:{}", suffix))
        .bind(now)
        .execute(&*pool)
        .await
        .expect("insert queue entry b should succeed");

        // Duplicate room_id to verify DISTINCT.
        sqlx::query(
            "INSERT INTO federation_queue (destination, event_id, event_type, room_id, content, created_ts) \
             VALUES ($1, $2, 'm.room.message', $3, '{}'::jsonb, $4)",
        )
        .bind(&server_name)
        .bind(format!("event-b2-{}", suffix))
        .bind(format!("!room-b:{}", suffix))
        .bind(now)
        .execute(&*pool)
        .await
        .expect("insert queue entry b2 should succeed");

        let rooms = storage.get_destination_rooms(&server_name).await.expect("get_destination_rooms should succeed");
        assert_eq!(rooms.len(), 2, "should return 2 distinct rooms");
        assert!(rooms.contains(&format!("!room-a:{}", suffix)));
        assert!(rooms.contains(&format!("!room-b:{}", suffix)));
        // Verify ascending order.
        assert!(rooms[0] < rooms[1], "rooms should be ordered ascending");

        cleanup_queue_prefix(&pool, &prefix).await;
    }

    // 12. get_destination_rooms returns empty vector when no rows match.
    #[tokio::test]
    async fn test_get_destination_rooms_empty() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-rooms-empty-{}.com", suffix);

        let rooms = storage.get_destination_rooms(&server_name).await.expect("get_destination_rooms should succeed");
        assert!(rooms.is_empty());
    }

    // 13. get_destination_status returns COALESCE(status, 'active').
    #[tokio::test]
    async fn test_get_destination_status_found() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-status-{}.com", suffix);
        let prefix = format!("test-status-{}%", suffix);

        cleanup_server_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        insert_test_server(&pool, &server_name, "rejected", now).await;

        let status = storage.get_destination_status(&server_name).await.expect("get_destination_status should succeed");
        assert_eq!(status.as_deref(), Some("rejected"));

        cleanup_server_prefix(&pool, &prefix).await;
    }

    // 14. get_destination_status returns None for unknown server.
    #[tokio::test]
    async fn test_get_destination_status_not_found() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-status-nonexist-{}.com", suffix);

        let status = storage.get_destination_status(&server_name).await.expect("get_destination_status should succeed");
        assert!(status.is_none());
    }

    // 15. get_server_admission_status: unknown server returns None.
    #[tokio::test]
    async fn test_get_server_admission_status_unknown() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-admission-unknown-{}.com", suffix);

        let status = storage
            .get_server_admission_status(&server_name)
            .await
            .expect("get_server_admission_status should succeed");
        assert!(status.is_none(), "unknown server should return None");
    }

    // 16. get_server_admission_status: known server with explicit status returns Some(Some(...)).
    #[tokio::test]
    async fn test_get_server_admission_status_known() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-admission-known-{}.com", suffix);
        let prefix = format!("test-admission-known-{}%", suffix);

        cleanup_server_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        insert_test_server(&pool, &server_name, "pending", now).await;

        let status = storage
            .get_server_admission_status(&server_name)
            .await
            .expect("get_server_admission_status should succeed");
        assert_eq!(status, Some(Some("pending".to_string())));

        cleanup_server_prefix(&pool, &prefix).await;
    }

    // 17. insert_pending_server inserts a new row with status='pending' and returns rows_affected=1.
    #[tokio::test]
    async fn test_insert_pending_server_new() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-insert-pending-{}.com", suffix);
        let prefix = format!("test-insert-pending-{}%", suffix);

        cleanup_server_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        let affected =
            storage.insert_pending_server(&server_name, now).await.expect("insert_pending_server should succeed");
        assert_eq!(affected, 1, "should insert 1 row");

        // Verify the row was inserted with status='pending'.
        let dest = storage
            .get_destination(&server_name)
            .await
            .expect("get_destination should succeed")
            .expect("server should exist");
        assert_eq!(dest.status.as_deref(), Some("pending"));
        assert_eq!(dest.updated_ts, Some(now));

        cleanup_server_prefix(&pool, &prefix).await;
    }

    // 18. insert_pending_server with ON CONFLICT DO NOTHING returns rows_affected=0 on duplicate.
    #[tokio::test]
    async fn test_insert_pending_server_duplicate() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-insert-dup-{}.com", suffix);
        let prefix = format!("test-insert-dup-{}%", suffix);

        cleanup_server_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        let first =
            storage.insert_pending_server(&server_name, now).await.expect("first insert_pending_server should succeed");
        assert_eq!(first, 1);

        // Second insert with same server_name should do nothing.
        let second = storage
            .insert_pending_server(&server_name, now + 1000)
            .await
            .expect("second insert_pending_server should succeed");
        assert_eq!(second, 0, "ON CONFLICT DO NOTHING should return 0");

        // Verify the updated_ts was NOT overwritten (first value preserved).
        let dest = storage
            .get_destination(&server_name)
            .await
            .expect("get_destination should succeed")
            .expect("server should exist");
        assert_eq!(dest.updated_ts, Some(now));

        cleanup_server_prefix(&pool, &prefix).await;
    }

    // 19. update_destination_status sets status and updated_ts for an existing server.
    #[tokio::test]
    async fn test_update_destination_status() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-update-status-{}.com", suffix);
        let prefix = format!("test-update-status-{}%", suffix);

        cleanup_server_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        insert_test_server(&pool, &server_name, "pending", now).await;

        let new_ts = now + 5000;
        let affected = storage
            .update_destination_status(&server_name, "active", new_ts)
            .await
            .expect("update_destination_status should succeed");
        assert_eq!(affected, 1);

        let dest = storage
            .get_destination(&server_name)
            .await
            .expect("get_destination should succeed")
            .expect("server should exist");
        assert_eq!(dest.status.as_deref(), Some("active"));
        assert_eq!(dest.updated_ts, Some(new_ts));

        cleanup_server_prefix(&pool, &prefix).await;
    }

    // 20. update_destination_status returns 0 when server does not exist.
    #[tokio::test]
    async fn test_update_destination_status_no_match() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let server_name = format!("test-update-nonexist-{}.com", suffix);

        let now = chrono::Utc::now().timestamp_millis();
        let affected = storage
            .update_destination_status(&server_name, "active", now)
            .await
            .expect("update_destination_status should succeed");
        assert_eq!(affected, 0);
    }

    // 21. list_pending_federation returns servers with status='pending' using cursor pagination.
    #[tokio::test]
    async fn test_list_pending_federation_pagination() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let prefix = format!("test-pending-list-{}%", suffix);

        cleanup_server_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        // Insert pending servers with staggered updated_ts.
        let servers: Vec<String> = (0..5).map(|i| format!("test-pending-list-{:02}-{}.com", i, suffix)).collect();
        for (i, s) in servers.iter().enumerate() {
            sqlx::query("INSERT INTO federation_servers (server_name, status, updated_ts) VALUES ($1, 'pending', $2)")
                .bind(s)
                .bind(now + i as i64)
                .execute(&*pool)
                .await
                .expect("insert pending server should succeed");
        }

        // Collect all our test servers across pages by paginating until done.
        let mut all_seen: Vec<String> = Vec::new();
        let mut cursor_ts: Option<i64> = None;
        let mut cursor_name: Option<String> = None;
        loop {
            let page = storage
                .list_pending_federation(cursor_ts, cursor_name.as_deref(), 3)
                .await
                .expect("list_pending_federation should succeed");
            if page.is_empty() {
                break;
            }
            for row in &page {
                if servers.contains(&row.server_name) {
                    all_seen.push(row.server_name.clone());
                }
            }
            // Advance cursor to the last row of this page.
            let last = page.last().unwrap();
            cursor_ts = last.updated_ts;
            cursor_name = Some(last.server_name.clone());
        }

        // All our test servers should appear.
        for s in &servers {
            assert!(all_seen.contains(s), "server {} should appear in paginated results", s);
        }

        cleanup_server_prefix(&pool, &prefix).await;
    }

    // 22. list_pending_federation returns empty when no pending servers exist.
    #[tokio::test]
    async fn test_list_pending_federation_empty() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);

        let results =
            storage.list_pending_federation(None, None, 10).await.expect("list_pending_federation should succeed");

        // Verify no results for pending status (there may be none, validate each is pending).
        // We can't assert length is zero since other tests may leave pending rows.
        for r in &results {
            // All returned rows should correspond to pending servers.
            let raw_status: Option<String> =
                sqlx::query_scalar("SELECT status FROM federation_servers WHERE server_name = $1")
                    .bind(&r.server_name)
                    .fetch_optional(&*pool)
                    .await
                    .expect("status lookup should succeed")
                    .flatten();
            assert!(
                raw_status.as_deref().map_or(true, |s| s == "pending"),
                "list_pending_federation should only return pending servers, got status={raw_status:?}"
            );
        }
    }

    // 23. count_pending_federation returns count of servers with status='pending'.
    #[tokio::test]
    async fn test_count_pending_federation() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let prefix = format!("test-pending-count-{}%", suffix);

        cleanup_server_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        // Insert some pending servers.
        for i in 0..2 {
            sqlx::query("INSERT INTO federation_servers (server_name, status, updated_ts) VALUES ($1, 'pending', $2)")
                .bind(format!("test-pending-count-{}-{}.com", i, suffix))
                .bind(now)
                .execute(&*pool)
                .await
                .expect("insert pending server should succeed");
        }

        // Also insert an active server to ensure it is not counted.
        insert_test_server(&pool, &format!("test-pending-count-active-{}.com", suffix), "active", now).await;

        let count = storage.count_pending_federation().await.expect("count_pending_federation should succeed");
        assert!(count >= 2, "should count at least 2 pending servers, got {count}");

        cleanup_server_prefix(&pool, &prefix).await;
    }

    // 24. get_federation_cache returns all cache entries ordered by key.
    #[tokio::test]
    async fn test_get_federation_cache() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let prefix = format!("test-cache-{}%", suffix);

        cleanup_cache_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        // Insert two cache entries.
        sqlx::query("INSERT INTO federation_cache (key, value, expiry_ts, created_ts) VALUES ($1, $2, $3, $4)")
            .bind(format!("test-cache-a-{}", suffix))
            .bind("value-a")
            .bind(now + 3600_000)
            .bind(now)
            .execute(&*pool)
            .await
            .expect("insert cache entry a should succeed");

        sqlx::query("INSERT INTO federation_cache (key, value, expiry_ts, created_ts) VALUES ($1, $2, $3, $4)")
            .bind(format!("test-cache-b-{}", suffix))
            .bind("value-b")
            .bind(now + 7200_000)
            .bind(now)
            .execute(&*pool)
            .await
            .expect("insert cache entry b should succeed");

        let entries = storage.get_federation_cache().await.expect("get_federation_cache should succeed");

        // Find our test entries.
        let ours: Vec<&FederationCacheRecord> = entries
            .iter()
            .filter(|e| e.key.starts_with("test-cache-") && e.key.ends_with(&suffix.to_string()))
            .collect();
        assert_eq!(ours.len(), 2, "should find 2 test cache entries");

        // Verify ordering by key.
        for w in ours.windows(2) {
            assert!(w[0].key < w[1].key, "cache entries should be ordered by key ASC");
        }

        cleanup_cache_prefix(&pool, &prefix).await;
    }

    // 25. get_federation_cache returns empty when no entries exist.
    #[tokio::test]
    async fn test_get_federation_cache_empty() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let prefix = format!("test-cache-empty-{}%", suffix);

        cleanup_cache_prefix(&pool, &prefix).await;

        // With no matching entries, the function should return an empty vec.
        let entries = storage.get_federation_cache().await.expect("get_federation_cache should succeed");

        let ours: Vec<&FederationCacheRecord> = entries
            .iter()
            .filter(|e| e.key.starts_with("test-cache-empty-") && e.key.ends_with(&suffix.to_string()))
            .collect();
        assert!(ours.is_empty());

        cleanup_cache_prefix(&pool, &prefix).await;
    }

    // 26. delete_federation_cache_entry removes a single entry by key.
    #[tokio::test]
    async fn test_delete_federation_cache_entry() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let prefix = format!("test-cache-del-{}%", suffix);

        cleanup_cache_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        let key = format!("test-cache-del-{}", suffix);
        sqlx::query("INSERT INTO federation_cache (key, value, expiry_ts, created_ts) VALUES ($1, $2, $3, $4)")
            .bind(&key)
            .bind("temp-value")
            .bind(now + 3600_000)
            .bind(now)
            .execute(&*pool)
            .await
            .expect("insert cache entry should succeed");

        let affected =
            storage.delete_federation_cache_entry(&key).await.expect("delete_federation_cache_entry should succeed");
        assert_eq!(affected, 1);

        // Verify entry is gone.
        let remaining = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM federation_cache WHERE key = $1")
            .bind(&key)
            .fetch_one(&*pool)
            .await
            .expect("count should succeed");
        assert_eq!(remaining, 0);

        cleanup_cache_prefix(&pool, &prefix).await;
    }

    // 27. delete_federation_cache_entry returns 0 when key does not exist.
    #[tokio::test]
    async fn test_delete_federation_cache_entry_no_match() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let key = format!("test-cache-del-nonexist-{}", suffix);

        let affected =
            storage.delete_federation_cache_entry(&key).await.expect("delete_federation_cache_entry should succeed");
        assert_eq!(affected, 0);
    }

    // 28. clear_federation_cache deletes all entries and returns rows_affected.
    #[tokio::test]
    async fn test_clear_federation_cache() {
        let pool = test_pool().await;
        let storage = AdminFederationStorage::new(&pool);
        let suffix = Uuid::new_v4();
        let prefix = format!("test-cache-clear-{}%", suffix);

        cleanup_cache_prefix(&pool, &prefix).await;

        let now = chrono::Utc::now().timestamp_millis();
        for i in 0..2 {
            sqlx::query("INSERT INTO federation_cache (key, value, expiry_ts, created_ts) VALUES ($1, $2, $3, $4)")
                .bind(format!("test-cache-clear-{}-{}", i, suffix))
                .bind(format!("value-{}", i))
                .bind(now + (i + 1) as i64 * 3600_000)
                .bind(now)
                .execute(&*pool)
                .await
                .expect("insert cache entry should succeed");
        }

        let affected = storage.clear_federation_cache().await.expect("clear_federation_cache should succeed");
        // Global DELETE — just assert it didn't error (rows affected >= our 2 entries).
        assert!(affected >= 2, "should delete at least 2 entries, got {affected}");

        // Verify cache has been fully cleared.
        let entries = storage.get_federation_cache().await.expect("get_federation_cache should succeed");
        assert_eq!(entries.len(), 0, "federation_cache should be empty after clear");

        cleanup_cache_prefix(&pool, &prefix).await;
    }
}
