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

#[derive(Debug, Clone)]
pub struct AdminFederationStorage {
    pool: Arc<PgPool>,
}

impl AdminFederationStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn count_destinations(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM federation_servers")
            .fetch_one(&*self.pool)
            .await
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

    pub async fn get_destination(
        &self,
        server_name: &str,
    ) -> Result<Option<FederationDestinationRecord>, sqlx::Error> {
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

    pub async fn update_destination_status(
        &self,
        server_name: &str,
        status: &str,
        updated_ts: i64,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE federation_servers SET status = $1, updated_ts = $2 WHERE server_name = $3",
        )
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
        let result = sqlx::query("DELETE FROM federation_cache WHERE key = $1")
            .bind(key)
            .execute(&*self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn clear_federation_cache(&self) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM federation_cache").execute(&*self.pool).await?;
        Ok(result.rows_affected())
    }
}
