use super::models::*;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use chrono::Utc;
use crate::error::ApiError;

pub struct DeviceKeyStorage<'a> {
    pool: &'a PgPool,
}

impl<'a> DeviceKeyStorage<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_device_key(&self, key: &DeviceKey) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO device_keys (id, user_id, device_id, display_name, algorithm, key_id, public_key, signatures, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (user_id, device_id, key_id) DO UPDATE
            SET display_name = EXCLUDED.display_name,
                public_key = EXCLUDED.public_key,
                signatures = EXCLUDED.signatures,
                updated_at = EXCLUDED.updated_at
            "#
        )
        .bind(key.id)
        .bind(&key.user_id)
        .bind(&key.device_id)
        .bind(&key.display_name)
        .bind(&key.algorithm)
        .bind(&key.key_id)
        .bind(&key.public_key)
        .bind(&key.signatures)
        .bind(key.created_at)
        .bind(key.updated_at)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_device_key(&self, user_id: &str, device_id: &str, key_id: &str) -> Result<Option<DeviceKey>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT id, user_id, device_id, display_name, algorithm, key_id, public_key, signatures, created_at, updated_at
            FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND key_id = $3
            "#
        )
        .bind(user_id)
        .bind(device_id)
        .bind(key_id)
        .fetch_optional(self.pool)
        .await?;
        
        Ok(row.map(|row| DeviceKey {
            id: row.get("id"),
            user_id: row.get("user_id"),
            device_id: row.get("device_id"),
            display_name: row.get("display_name"),
            algorithm: row.get("algorithm"),
            key_id: row.get("key_id"),
            public_key: row.get("public_key"),
            signatures: row.get("signatures"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }))
    }
    
    pub async fn get_device_keys(&self, user_id: &str, device_ids: &[String]) -> Result<Vec<DeviceKey>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT id, user_id, device_id, display_name, algorithm, key_id, public_key, signatures, created_at, updated_at
            FROM device_keys
            WHERE user_id = $1 AND device_id = ANY($2)
            "#
        )
        .bind(user_id)
        .bind(device_ids)
        .fetch_all(self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|row| DeviceKey {
            id: row.get("id"),
            user_id: row.get("user_id"),
            device_id: row.get("device_id"),
            display_name: row.get("display_name"),
            algorithm: row.get("algorithm"),
            key_id: row.get("key_id"),
            public_key: row.get("public_key"),
            signatures: row.get("signatures"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }).collect())
    }
    
    pub async fn get_all_device_keys(&self, user_id: &str) -> Result<Vec<DeviceKey>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT id, user_id, device_id, display_name, algorithm, key_id, public_key, signatures, created_at, updated_at
            FROM device_keys
            WHERE user_id = $1
            "#
        )
        .bind(user_id)
        .fetch_all(self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|row| DeviceKey {
            id: row.get("id"),
            user_id: row.get("user_id"),
            device_id: row.get("device_id"),
            display_name: row.get("display_name"),
            algorithm: row.get("algorithm"),
            key_id: row.get("key_id"),
            public_key: row.get("public_key"),
            signatures: row.get("signatures"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }).collect())
    }
    
    pub async fn delete_device_key(&self, user_id: &str, device_id: &str, key_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND key_id = $3
            "#
        )
        .bind(user_id)
        .bind(device_id)
        .bind(key_id)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn delete_device_keys(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM device_keys
            WHERE user_id = $1 AND device_id = $2
            "#
        )
        .bind(user_id)
        .bind(device_id)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_one_time_keys_count(&self, user_id: &str, device_id: &str) -> Result<i64, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count
            FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND algorithm LIKE 'signed_curve25519%'
            "#
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_one(self.pool)
        .await?;
        
        Ok(row.get("count"))
    }
    
    pub async fn claim_one_time_key(&self, user_id: &str, device_id: &str, algorithm: &str) -> Result<Option<DeviceKey>, ApiError> {
        let row = sqlx::query(
            r#"
            DELETE FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND algorithm = $3
            RETURNING id, user_id, device_id, display_name, algorithm, key_id, public_key, signatures, created_at, updated_at
            "#
        )
        .bind(user_id)
        .bind(device_id)
        .bind(algorithm)
        .fetch_optional(self.pool)
        .await?;
        
        Ok(row.map(|row| DeviceKey {
            id: row.get("id"),
            user_id: row.get("user_id"),
            device_id: row.get("device_id"),
            display_name: row.get("display_name"),
            algorithm: row.get("algorithm"),
            key_id: row.get("key_id"),
            public_key: row.get("public_key"),
            signatures: row.get("signatures"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }))
    }
}