use super::models::KeyRequestInfo;
use crate::error::ApiError;
use sqlx::PgPool;

#[derive(Clone)]
pub struct KeyRequestStorage {
    pool: PgPool,
}

impl KeyRequestStorage {
    pub fn new(pool: &PgPool) -> Self {
        Self {
            pool: pool.clone(),
        }
    }

    pub async fn create_request(&self, request: &KeyRequestInfo) -> Result<(), ApiError> {
        sqlx::query!(
            r#"
            INSERT INTO e2ee_key_requests 
                (request_id, user_id, device_id, room_id, session_id, algorithm, action, created_ts, fulfilled)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (request_id) DO UPDATE SET
                action = EXCLUDED.action,
                fulfilled = EXCLUDED.fulfilled
            "#,
            request.request_id,
            request.user_id,
            request.device_id,
            request.room_id,
            request.session_id,
            request.algorithm,
            request.action,
            request.created_ts,
            request.fulfilled
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_request(&self, request_id: &str) -> Result<Option<KeyRequestInfo>, ApiError> {
        let row = sqlx::query!(
            r#"
            SELECT request_id, user_id, device_id, room_id, session_id, algorithm, 
                   action, created_ts, fulfilled, fulfilled_by_device, fulfilled_ts
            FROM e2ee_key_requests
            WHERE request_id = $1
            "#,
            request_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| KeyRequestInfo {
            request_id: r.request_id,
            user_id: r.user_id,
            device_id: r.device_id,
            room_id: r.room_id,
            session_id: r.session_id,
            algorithm: r.algorithm,
            action: r.action,
            created_ts: r.created_ts,
            fulfilled: r.fulfilled.unwrap_or(false),
            fulfilled_by_device: r.fulfilled_by_device,
            fulfilled_ts: r.fulfilled_ts,
        }))
    }

    pub async fn get_requests_for_user(&self, user_id: &str) -> Result<Vec<KeyRequestInfo>, ApiError> {
        let rows = sqlx::query!(
            r#"
            SELECT request_id, user_id, device_id, room_id, session_id, algorithm, 
                   action, created_ts, fulfilled, fulfilled_by_device, fulfilled_ts
            FROM e2ee_key_requests
            WHERE user_id = $1 AND fulfilled = FALSE
            ORDER BY created_ts DESC
            LIMIT 100
            "#,
            user_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| KeyRequestInfo {
                request_id: r.request_id,
                user_id: r.user_id,
                device_id: r.device_id,
                room_id: r.room_id,
                session_id: r.session_id,
                algorithm: r.algorithm,
                action: r.action,
                created_ts: r.created_ts,
                fulfilled: r.fulfilled.unwrap_or(false),
                fulfilled_by_device: r.fulfilled_by_device,
                fulfilled_ts: r.fulfilled_ts,
            })
            .collect())
    }

    pub async fn get_all_pending_requests(&self) -> Result<Vec<KeyRequestInfo>, ApiError> {
        let rows = sqlx::query!(
            r#"
            SELECT request_id, user_id, device_id, room_id, session_id, algorithm, 
                   action, created_ts, fulfilled, fulfilled_by_device, fulfilled_ts
            FROM e2ee_key_requests
            WHERE fulfilled = FALSE
            ORDER BY created_ts DESC
            LIMIT 100
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| KeyRequestInfo {
                request_id: r.request_id,
                user_id: r.user_id,
                device_id: r.device_id,
                room_id: r.room_id,
                session_id: r.session_id,
                algorithm: r.algorithm,
                action: r.action,
                created_ts: r.created_ts,
                fulfilled: r.fulfilled.unwrap_or(false),
                fulfilled_by_device: r.fulfilled_by_device,
                fulfilled_ts: r.fulfilled_ts,
            })
            .collect())
    }

    pub async fn fulfill_request(
        &self,
        request_id: &str,
        device_id: &str,
    ) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query!(
            r#"
            UPDATE e2ee_key_requests 
            SET fulfilled = TRUE, fulfilled_by_device = $2, fulfilled_ts = $3
            WHERE request_id = $1
            "#,
            request_id,
            device_id,
            now
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn cancel_request(&self, request_id: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r#"
            UPDATE e2ee_key_requests 
            SET action = 'cancellation', fulfilled = TRUE
            WHERE request_id = $1
            "#,
            request_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_request(&self, request_id: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r#"
            DELETE FROM e2ee_key_requests WHERE request_id = $1
            "#,
            request_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_old_requests(&self, older_than_ts: i64) -> Result<u64, ApiError> {
        let result = sqlx::query!(
            r#"
            DELETE FROM e2ee_key_requests 
            WHERE fulfilled = TRUE AND fulfilled_ts < $1
            "#,
            older_than_ts
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}
