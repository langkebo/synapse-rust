use super::models::{KeyRequestInfo, KeyRequestPagination};
use crate::error::ApiError;
use sqlx::PgPool;

#[derive(Clone)]
pub struct KeyRequestStorage {
    pool: PgPool,
}

impl KeyRequestStorage {
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_request(&self, request: &KeyRequestInfo) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            INSERT INTO e2ee_key_requests
                (request_id, user_id, device_id, room_id, session_id, algorithm, action, created_ts, is_fulfilled)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (request_id) DO UPDATE SET
                action = EXCLUDED.action,
                is_fulfilled = EXCLUDED.is_fulfilled
            ",
            request.request_id,
            request.user_id,
            request.device_id,
            request.room_id,
            request.session_id,
            request.algorithm,
            request.action,
            request.created_ts,
            request.is_fulfilled,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_request(&self, request_id: &str) -> Result<Option<KeyRequestInfo>, ApiError> {
        sqlx::query_as!(
            KeyRequestInfo,
            r#"
            SELECT
                request_id,
                user_id,
                device_id,
                room_id,
                session_id,
                algorithm,
                action,
                created_ts,
                COALESCE(is_fulfilled, FALSE) AS "is_fulfilled!",
                fulfilled_by_device AS "fulfilled_by_device?",
                fulfilled_ts AS "fulfilled_ts?"
            FROM e2ee_key_requests
            WHERE request_id = $1
            "#,
            request_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })
    }

    pub async fn get_requests_for_user(&self, user_id: &str) -> Result<Vec<KeyRequestInfo>, ApiError> {
        sqlx::query_as!(
            KeyRequestInfo,
            r#"
            SELECT
                request_id,
                user_id,
                device_id,
                room_id,
                session_id,
                algorithm,
                action,
                created_ts,
                COALESCE(is_fulfilled, FALSE) AS "is_fulfilled!",
                fulfilled_by_device AS "fulfilled_by_device?",
                fulfilled_ts AS "fulfilled_ts?"
            FROM e2ee_key_requests
            WHERE user_id = $1
            ORDER BY created_ts DESC
            LIMIT 100
            "#,
            user_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })
    }

    pub async fn get_requests_paginated(
        &self,
        pagination: KeyRequestPagination<'_>,
    ) -> Result<Vec<KeyRequestInfo>, ApiError> {
        let KeyRequestPagination { user_id, limit, from_ts, from_id, status, room_id, session_id } = pagination;
        let mut query = sqlx::QueryBuilder::new(
            r#"
            SELECT
                request_id,
                user_id,
                device_id,
                room_id,
                session_id,
                algorithm,
                action,
                created_ts,
                COALESCE(is_fulfilled, FALSE) AS is_fulfilled,
                fulfilled_by_device,
                fulfilled_ts
            FROM e2ee_key_requests
            WHERE user_id = "#,
        );
        query.push_bind(user_id);

        if let Some(room) = room_id {
            query.push(" AND room_id = ");
            query.push_bind(room);
        }

        if let Some(session) = session_id {
            query.push(" AND session_id = ");
            query.push_bind(session);
        }

        if let Some(status) = status {
            match status {
                "pending" => {
                    query.push(" AND is_fulfilled = FALSE");
                }
                "fulfilled" => {
                    query.push(" AND is_fulfilled = TRUE");
                }
                "cancelled" => {
                    query.push(" AND (action = 'cancelled' OR action = 'cancellation')");
                }
                _ => {}
            }
        }

        if let (Some(ts), Some(id)) = (from_ts, from_id) {
            query.push(" AND (created_ts < ");
            query.push_bind(ts);
            query.push(" OR (created_ts = ");
            query.push_bind(ts);
            query.push(" AND request_id < ");
            query.push_bind(id);
            query.push("))");
        }

        query.push(" ORDER BY created_ts DESC, request_id DESC LIMIT ");
        query.push_bind(limit);

        query.build_query_as::<KeyRequestInfo>().fetch_all(&self.pool).await.map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })
    }

    pub async fn get_all_pending_requests(&self) -> Result<Vec<KeyRequestInfo>, ApiError> {
        sqlx::query_as!(
            KeyRequestInfo,
            r#"
            SELECT
                request_id,
                user_id,
                device_id,
                room_id,
                session_id,
                algorithm,
                action,
                created_ts,
                COALESCE(is_fulfilled, FALSE) AS "is_fulfilled!",
                fulfilled_by_device AS "fulfilled_by_device?",
                fulfilled_ts AS "fulfilled_ts?"
            FROM e2ee_key_requests
            WHERE is_fulfilled = FALSE
            ORDER BY created_ts DESC
            LIMIT 100
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })
    }

    pub async fn fulfill_request(&self, request_id: &str, device_id: &str) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query!(
            r"
            UPDATE e2ee_key_requests
            SET is_fulfilled = TRUE, fulfilled_by_device = $2, fulfilled_ts = $3
            WHERE request_id = $1
            ",
            request_id,
            device_id,
            now,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn cancel_request(&self, request_id: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            UPDATE e2ee_key_requests
            SET action = 'cancellation', is_fulfilled = TRUE
            WHERE request_id = $1
            ",
            request_id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn update_request_status(&self, request_id: &str, status: &str) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query!(
            r"
            UPDATE e2ee_key_requests
            SET action = $2, updated_ts = $3
            WHERE request_id = $1
            ",
            request_id,
            status,
            now,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn delete_request(&self, request_id: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            DELETE FROM e2ee_key_requests WHERE request_id = $1
            ",
            request_id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn delete_old_requests(&self, older_than_ts: i64) -> Result<u64, ApiError> {
        let result = sqlx::query!(
            r"
            DELETE FROM e2ee_key_requests
            WHERE is_fulfilled = TRUE AND fulfilled_ts < $1
            ",
            older_than_ts,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(result.rows_affected())
    }
}
