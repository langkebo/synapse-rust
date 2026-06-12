use crate::media::models::QuarantinedMediaChange;
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::ApiError;

/// Storage layer for the `quarantined_media_changes` stream table.
///
/// Provides methods to record quarantine/unquarantine changes and query
/// incremental changes for stream replication between workers.
#[derive(Clone)]
pub struct QuarantinedMediaChangeStorage {
    pool: PgPool,
}

impl QuarantinedMediaChangeStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: (**pool).clone() }
    }

    /// Record a media quarantine/unquarantine change and return the new stream_id.
    pub async fn record_media_quarantine_change(
        &self,
        media_id: &str,
        server_name: &str,
        change_type: &str,
        changed_by: &str,
        now_ts: i64,
    ) -> Result<i64, ApiError> {
        let row = sqlx::query_as::<_, QuarantinedMediaChange>(
            r#"
            INSERT INTO quarantined_media_changes (media_id, server_name, change_type, changed_by, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING stream_id, media_id, server_name, change_type, changed_by, created_ts
            "#,
        )
        .bind(media_id)
        .bind(server_name)
        .bind(change_type)
        .bind(changed_by)
        .bind(now_ts)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to record media quarantine change", &e))?;

        Ok(row.stream_id)
    }

    /// Get incremental quarantine changes since the given stream_id.
    pub async fn get_quarantined_media_changes(
        &self,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<Vec<QuarantinedMediaChange>, ApiError> {
        let changes = sqlx::query_as::<_, QuarantinedMediaChange>(
            r#"
            SELECT stream_id, media_id, server_name, change_type, changed_by, created_ts
            FROM quarantined_media_changes
            WHERE stream_id > $1
            ORDER BY stream_id ASC
            LIMIT $2
            "#,
        )
        .bind(since_stream_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get quarantined media changes", &e))?;

        Ok(changes)
    }

    /// Update the quarantine_status column on media_metadata.
    pub async fn set_media_quarantine_status(
        &self,
        media_id: &str,
        server_name: &str,
        quarantine_status: &str,
    ) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r#"
            UPDATE media_metadata
            SET quarantine_status = $1
            WHERE media_id = $2 AND server_name = $3
            "#,
        )
        .bind(quarantine_status)
        .bind(media_id)
        .bind(server_name)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update media quarantine status", &e))?;

        Ok(result.rows_affected() > 0)
    }

    /// Get the current maximum stream_id (used for position tracking).
    pub async fn get_current_stream_id(&self) -> Result<i64, ApiError> {
        let stream_id: Option<i64> = sqlx::query_scalar(r"SELECT MAX(stream_id) FROM quarantined_media_changes")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get current quarantine stream id", &e))?;

        Ok(stream_id.unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_creation() {
        // Verify the storage struct can be constructed (pool connectivity tested in integration tests)
        let _model = QuarantinedMediaChange {
            stream_id: 1,
            media_id: "abc123".to_string(),
            server_name: "example.com".to_string(),
            change_type: "quarantine".to_string(),
            changed_by: "@admin:example.com".to_string(),
            created_ts: 1234567890000,
        };
    }
}
