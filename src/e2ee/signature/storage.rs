use super::models::*;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use chrono::Utc;
use crate::error::ApiError;

pub struct SignatureStorage<'a> {
    pool: &'a PgPool,
}

impl<'a> SignatureStorage<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_signature(&self, signature: &EventSignature) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO event_signatures (id, event_id, user_id, device_id, signature, key_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (event_id, user_id, device_id, key_id) DO UPDATE
            SET signature = EXCLUDED.signature
            "#
        )
        .bind(signature.id)
        .bind(&signature.event_id)
        .bind(&signature.user_id)
        .bind(&signature.device_id)
        .bind(&signature.signature)
        .bind(&signature.key_id)
        .bind(signature.created_at)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_signature(&self, event_id: &str, user_id: &str, device_id: &str, key_id: &str) -> Result<Option<EventSignature>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT id, event_id, user_id, device_id, signature, key_id, created_at
            FROM event_signatures
            WHERE event_id = $1 AND user_id = $2 AND device_id = $3 AND key_id = $4
            "#
        )
        .bind(event_id)
        .bind(user_id)
        .bind(device_id)
        .bind(key_id)
        .fetch_optional(self.pool)
        .await?;
        
        Ok(row.map(|row| EventSignature {
            id: row.get("id"),
            event_id: row.get("event_id"),
            user_id: row.get("user_id"),
            device_id: row.get("device_id"),
            signature: row.get("signature"),
            key_id: row.get("key_id"),
            created_at: row.get("created_at"),
        }))
    }
    
    pub async fn get_event_signatures(&self, event_id: &str) -> Result<Vec<EventSignature>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT id, event_id, user_id, device_id, signature, key_id, created_at
            FROM event_signatures
            WHERE event_id = $1
            "#
        )
        .bind(event_id)
        .fetch_all(self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|row| EventSignature {
            id: row.get("id"),
            event_id: row.get("event_id"),
            user_id: row.get("user_id"),
            device_id: row.get("device_id"),
            signature: row.get("signature"),
            key_id: row.get("key_id"),
            created_at: row.get("created_at"),
        }).collect())
    }
}