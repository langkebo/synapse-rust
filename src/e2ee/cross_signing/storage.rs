use super::models::*;
use crate::error::ApiError;
use sqlx::{PgPool, Row};
use std::sync::Arc;

#[derive(Clone)]
pub struct CrossSigningStorage {
    pub pool: Arc<PgPool>,
}

impl CrossSigningStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
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
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_cross_signing_key(
        &self,
        user_id: &str,
        key_type: &str,
    ) -> Result<Option<CrossSigningKey>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT id, user_id, key_type, public_key, usage, signatures, created_at, updated_at
            FROM cross_signing_keys
            WHERE user_id = $1 AND key_type = $2
            "#,
        )
        .bind(user_id)
        .bind(key_type)
        .fetch_optional(&*self.pool)
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

    pub async fn get_cross_signing_keys(
        &self,
        user_id: &str,
    ) -> Result<Vec<CrossSigningKey>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT id, user_id, key_type, public_key, usage, signatures, created_at, updated_at
            FROM cross_signing_keys
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| CrossSigningKey {
                id: row.get("id"),
                user_id: row.get("user_id"),
                key_type: row.get("key_type"),
                public_key: row.get("public_key"),
                usage: row.get("usage"),
                signatures: row.get("signatures"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect())
    }

    pub async fn update_cross_signing_key(&self, key: &CrossSigningKey) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            UPDATE cross_signing_keys SET public_key = $1, usage = $2, signatures = $3, updated_at = $4
            WHERE user_id = $5 AND key_type = $6
            "#
        )
        .bind(&key.public_key)
        .bind(&key.usage)
        .bind(&key.signatures)
        .bind(key.updated_at)
        .bind(&key.user_id)
        .bind(&key.key_type)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn save_device_key(&self, key: &DeviceKeyInfo) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO device_keys (user_id, device_id, key_type, algorithm, public_key, signatures, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (user_id, device_id) DO UPDATE SET
                key_type = EXCLUDED.key_type,
                algorithm = EXCLUDED.algorithm,
                public_key = EXCLUDED.public_key,
                signatures = EXCLUDED.signatures
            "#
        )
        .bind(&key.user_id)
        .bind(&key.device_id)
        .bind(&key.key_type)
        .bind(&key.algorithm)
        .bind(&key.public_key)
        .bind(&key.signatures)
        .bind(key.created_at)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn save_device_signature(&self, sig: &DeviceSignature) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO device_signatures 
            (user_id, device_id, signing_key_id, target_user_id, target_device_id, target_key_id, signature, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, signing_key_id, target_user_id, target_key_id) DO UPDATE SET
                signature = EXCLUDED.signature,
                created_at = EXCLUDED.created_at
            "#
        )
        .bind(&sig.user_id)
        .bind(&sig.device_id)
        .bind(&sig.signing_key_id)
        .bind(&sig.target_user_id)
        .bind(&sig.target_device_id)
        .bind(&sig.target_key_id)
        .bind(&sig.signature)
        .bind(sig.created_at)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_user_signatures(
        &self,
        user_id: &str,
    ) -> Result<Vec<DeviceSignature>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT user_id, device_id, signing_key_id, target_user_id, target_device_id, 
                   target_key_id, signature, created_at
            FROM device_signatures
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| DeviceSignature {
                user_id: row.get("user_id"),
                device_id: row.get("device_id"),
                signing_key_id: row.get("signing_key_id"),
                target_user_id: row.get("target_user_id"),
                target_device_id: row.get("target_device_id"),
                target_key_id: row.get("target_key_id"),
                signature: row.get("signature"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    pub async fn get_device_signatures(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<DeviceSignature>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT user_id, device_id, signing_key_id, target_user_id, target_device_id, 
                   target_key_id, signature, created_at
            FROM device_signatures
            WHERE user_id = $1 AND target_device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| DeviceSignature {
                user_id: row.get("user_id"),
                device_id: row.get("device_id"),
                signing_key_id: row.get("signing_key_id"),
                target_user_id: row.get("target_user_id"),
                target_device_id: row.get("target_device_id"),
                target_key_id: row.get("target_key_id"),
                signature: row.get("signature"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    pub async fn get_signature(
        &self,
        user_id: &str,
        key_id: &str,
        signing_key_id: &str,
    ) -> Result<Option<DeviceSignature>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT user_id, device_id, signing_key_id, target_user_id, target_device_id, 
                   target_key_id, signature, created_at
            FROM device_signatures
            WHERE user_id = $1 AND target_key_id = $2 AND signing_key_id = $3
            "#,
        )
        .bind(user_id)
        .bind(key_id)
        .bind(signing_key_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|row| DeviceSignature {
            user_id: row.get("user_id"),
            device_id: row.get("device_id"),
            signing_key_id: row.get("signing_key_id"),
            target_user_id: row.get("target_user_id"),
            target_device_id: row.get("target_device_id"),
            target_key_id: row.get("target_key_id"),
            signature: row.get("signature"),
            created_at: row.get("created_at"),
        }))
    }

    pub async fn delete_cross_signing_keys(&self, user_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM cross_signing_keys WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            DELETE FROM device_signatures WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}
