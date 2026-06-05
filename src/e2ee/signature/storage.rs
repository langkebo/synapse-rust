use super::models::*;
use crate::error::ApiError;
use sqlx::PgPool;

pub struct SignatureStorage<'a> {
    pool: &'a PgPool,
}

impl<'a> SignatureStorage<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_signature(&self, signature: &EventSignature) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            INSERT INTO event_signatures (id, event_id, user_id, device_id, signature, key_id, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (event_id, user_id, device_id, key_id) DO UPDATE
            SET signature = EXCLUDED.signature
            ",
            signature.id,
            signature.event_id,
            signature.user_id,
            signature.device_id,
            signature.signature,
            signature.key_id,
            signature.created_ts,
        )
        .execute(self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_signature(
        &self,
        event_id: &str,
        user_id: &str,
        device_id: &str,
        key_id: &str,
    ) -> Result<Option<EventSignature>, ApiError> {
        let row = sqlx::query_as!(
            EventSignature,
            r#"
            SELECT
                id,
                event_id,
                user_id,
                device_id,
                signature,
                key_id,
                created_ts
            FROM event_signatures
            WHERE event_id = $1 AND user_id = $2 AND device_id = $3 AND key_id = $4
            "#,
            event_id,
            user_id,
            device_id,
            key_id,
        )
        .fetch_optional(self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_event_signatures(&self, event_id: &str) -> Result<Vec<EventSignature>, ApiError> {
        let rows = sqlx::query_as!(
            EventSignature,
            r#"
            SELECT
                id,
                event_id,
                user_id,
                device_id,
                signature,
                key_id,
                created_ts
            FROM event_signatures
            WHERE event_id = $1
            "#,
            event_id,
        )
        .fetch_all(self.pool)
        .await?;

        Ok(rows)
    }
}
