use super::models::*;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use chrono::Utc;
use crate::error::ApiError;

pub struct CrossSigningStorage<'a> {
    pool: &'a PgPool,
}

impl<'a> CrossSigningStorage<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_cross_signing_key(&self, key: &CrossSigningKey) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO cross_signing_keys (id, user_id, key_type, public_key, usage, signatures, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, key_type) DO UPDATE
            SET public_key = EXCLUDED.public_key,
                usage = EXCLUDED.usage,
                signatures = EXCLUDED.signatures,
                updated_at = EXCLUDED.updated_at
            "#
        )
        .bind(key.id)
        .bind(&key.user_id)
        .bind(&key.key_type)
        .bind(&key.public_key)
        .bind(&key.usage)
        .bind(&key.signatures)
        .bind(key.created_at)
        .bind(key.updated_at)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_cross_signing_key(&self, user_id: &str, key_type: &str) -> Result<Option<CrossSigningKey>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT id, user_id, key_type, public_key, usage, signatures, created_at, updated_at
            FROM cross_signing_keys
            WHERE user_id = $1 AND key_type = $2
            "#
        )
        .bind(user_id)
        .bind(key_type)
        .fetch_optional(self.pool)
        .await?;
        
        Ok(row.map(|row| CrossSigningKey {
            id: row.get("id"),
            user_id: row.get("user_id"),
            key_type: row.get("key_type"),
            public_key: row.get("public_key"),
            usage: row.get("usage"),
            signatures: row.get("signatures"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }))
    }
    
    pub async fn get_cross_signing_keys(&self, user_id: &str) -> Result<Vec<CrossSigningKey>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT id, user_id, key_type, public_key, usage, signatures, created_at, updated_at
            FROM cross_signing_keys
            WHERE user_id = $1
            "#
        )
        .bind(user_id)
        .fetch_all(self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|row| CrossSigningKey {
            id: row.get("id"),
            user_id: row.get("user_id"),
            key_type: row.get("key_type"),
            public_key: row.get("public_key"),
            usage: row.get("usage"),
            signatures: row.get("signatures"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }).collect())
    }
}