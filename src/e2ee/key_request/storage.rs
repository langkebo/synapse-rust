use super::models::KeyRequestInfo;
use crate::error::ApiError;
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct KeyRequestStorage {
    pool: PgPool,
}

impl KeyRequestStorage {
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_request(&self, request: &KeyRequestInfo) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO e2ee_key_requests 
                (request_id, user_id, device_id, room_id, session_id, algorithm, action, created_ts, is_fulfilled)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (request_id) DO UPDATE SET
                action = EXCLUDED.action,
                is_fulfilled = EXCLUDED.is_fulfilled
            "#
        )
        .bind(&request.request_id)
        .bind(&request.user_id)
        .bind(&request.device_id)
        .bind(&request.room_id)
        .bind(&request.session_id)
        .bind(&request.algorithm)
        .bind(&request.action)
        .bind(request.created_ts)
        .bind(request.is_fulfilled)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    pub async fn get_request(&self, request_id: &str) -> Result<Option<KeyRequestInfo>, ApiError> {
        let row: Option<sqlx::postgres::PgRow> = sqlx::query(
            r#"
            SELECT request_id, user_id, device_id, room_id, session_id, algorithm, 
                   action, created_ts, is_fulfilled, fulfilled_by_device, fulfilled_ts
            FROM e2ee_key_requests
            WHERE request_id = $1
            "#,
        )
        .bind(request_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(row.map(|r| KeyRequestInfo {
            request_id: r.get("request_id"),
            user_id: r.get("user_id"),
            device_id: r.get("device_id"),
            room_id: r.get("room_id"),
            session_id: r.get("session_id"),
            algorithm: r.get("algorithm"),
            action: r.get("action"),
            created_ts: r.get("created_ts"),
            is_fulfilled: r.get::<Option<bool>, _>("is_fulfilled").unwrap_or(false),
            fulfilled_by_device: r.get("fulfilled_by_device"),
            fulfilled_ts: r.get("fulfilled_ts"),
        }))
    }

    pub async fn get_requests_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<KeyRequestInfo>, ApiError> {
        let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
            r#"
            SELECT request_id, user_id, device_id, room_id, session_id, algorithm, 
                   action, created_ts, is_fulfilled, fulfilled_by_device, fulfilled_ts
            FROM e2ee_key_requests
            WHERE user_id = $1
            ORDER BY created_ts DESC
            LIMIT 100
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(rows.into_iter().map(map_request_row).collect())
    }

    pub async fn get_all_pending_requests(&self) -> Result<Vec<KeyRequestInfo>, ApiError> {
        let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
            r#"
            SELECT request_id, user_id, device_id, room_id, session_id, algorithm, 
                   action, created_ts, is_fulfilled, fulfilled_by_device, fulfilled_ts
            FROM e2ee_key_requests
            WHERE is_fulfilled = FALSE
            ORDER BY created_ts DESC
            LIMIT 100
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(rows.into_iter().map(map_request_row).collect())
    }

    pub async fn fulfill_request(&self, request_id: &str, device_id: &str) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE e2ee_key_requests 
            SET is_fulfilled = TRUE, fulfilled_by_device = $2, fulfilled_ts = $3
            WHERE request_id = $1
            "#,
        )
        .bind(request_id)
        .bind(device_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    pub async fn cancel_request(&self, request_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            UPDATE e2ee_key_requests 
            SET action = 'cancellation', is_fulfilled = TRUE
            WHERE request_id = $1
            "#,
        )
        .bind(request_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    pub async fn update_request_status(
        &self,
        request_id: &str,
        status: &str,
    ) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE e2ee_key_requests 
            SET action = $2, updated_ts = $3
            WHERE request_id = $1
            "#,
        )
        .bind(request_id)
        .bind(status)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    pub async fn delete_request(&self, request_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM e2ee_key_requests WHERE request_id = $1
            "#,
        )
        .bind(request_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    pub async fn delete_old_requests(&self, older_than_ts: i64) -> Result<u64, ApiError> {
        let result = sqlx::query(
            r#"
            DELETE FROM e2ee_key_requests 
            WHERE is_fulfilled = TRUE AND fulfilled_ts < $1
            "#,
        )
        .bind(older_than_ts)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(result.rows_affected())
    }
}

fn map_request_row(r: sqlx::postgres::PgRow) -> KeyRequestInfo {
    KeyRequestInfo {
        request_id: r.get("request_id"),
        user_id: r.get("user_id"),
        device_id: r.get("device_id"),
        room_id: r.get("room_id"),
        session_id: r.get("session_id"),
        algorithm: r.get("algorithm"),
        action: r.get("action"),
        created_ts: r.get("created_ts"),
        is_fulfilled: r.get::<Option<bool>, _>("is_fulfilled").unwrap_or(false),
        fulfilled_by_device: r.get("fulfilled_by_device"),
        fulfilled_ts: r.get("fulfilled_ts"),
    }
}
