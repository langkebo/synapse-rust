use super::models::{SecretStorageKey, StoredSecret};
use crate::error::ApiError;
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct SecretStorage {
    pool: PgPool,
}

impl SecretStorage {
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_key(&self, key: &SecretStorageKey) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO e2ee_secret_storage_keys 
                (key_id, user_id, algorithm, encrypted_key, public_key, signatures, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (key_id, user_id) DO UPDATE SET
                algorithm = EXCLUDED.algorithm,
                encrypted_key = EXCLUDED.encrypted_key,
                public_key = EXCLUDED.public_key,
                signatures = EXCLUDED.signatures
            "#,
        )
        .bind(&key.key_id)
        .bind(&key.user_id)
        .bind(&key.algorithm)
        .bind(&key.encrypted_key)
        .bind(&key.public_key)
        .bind(&key.signatures)
        .bind(key.created_ts)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    pub async fn get_key(
        &self,
        user_id: &str,
        key_id: &str,
    ) -> Result<Option<SecretStorageKey>, ApiError> {
        let row: Option<sqlx::postgres::PgRow> = sqlx::query(
            r#"
            SELECT key_id, user_id, algorithm, encrypted_key, public_key, signatures, created_ts
            FROM e2ee_secret_storage_keys
            WHERE user_id = $1 AND key_id = $2
            "#,
        )
        .bind(user_id)
        .bind(key_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(row.map(|r| SecretStorageKey {
            key_id: r.get("key_id"),
            user_id: r.get("user_id"),
            algorithm: r.get("algorithm"),
            encrypted_key: r.get("encrypted_key"),
            public_key: r.get("public_key"),
            signatures: r
                .get::<Option<serde_json::Value>, _>("signatures")
                .unwrap_or(serde_json::json!({})),
            created_ts: r.get("created_ts"),
        }))
    }

    pub async fn get_all_keys(&self, user_id: &str) -> Result<Vec<SecretStorageKey>, ApiError> {
        let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
            r#"
            SELECT key_id, user_id, algorithm, encrypted_key, public_key, signatures, created_ts
            FROM e2ee_secret_storage_keys
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r| SecretStorageKey {
                key_id: r.get("key_id"),
                user_id: r.get("user_id"),
                algorithm: r.get("algorithm"),
                encrypted_key: r.get("encrypted_key"),
                public_key: r.get("public_key"),
                signatures: r
                    .get::<Option<serde_json::Value>, _>("signatures")
                    .unwrap_or(serde_json::json!({})),
                created_ts: r.get("created_ts"),
            })
            .collect())
    }

    pub async fn delete_key(&self, user_id: &str, key_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM e2ee_secret_storage_keys
            WHERE user_id = $1 AND key_id = $2
            "#,
        )
        .bind(user_id)
        .bind(key_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    pub async fn store_secret(&self, user_id: &str, secret: &StoredSecret) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO e2ee_stored_secrets 
                (user_id, secret_name, encrypted_secret, key_id)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, secret_name) DO UPDATE SET
                encrypted_secret = EXCLUDED.encrypted_secret,
                key_id = EXCLUDED.key_id
            "#,
        )
        .bind(user_id)
        .bind(&secret.secret_name)
        .bind(&secret.encrypted_secret)
        .bind(&secret.key_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    pub async fn get_secret(
        &self,
        user_id: &str,
        secret_name: &str,
    ) -> Result<Option<StoredSecret>, ApiError> {
        let row: Option<sqlx::postgres::PgRow> = sqlx::query(
            r#"
            SELECT user_id, secret_name, encrypted_secret, key_id
            FROM e2ee_stored_secrets
            WHERE user_id = $1 AND secret_name = $2
            "#,
        )
        .bind(user_id)
        .bind(secret_name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(row.map(|r| StoredSecret {
            secret_name: r.get("secret_name"),
            encrypted_secret: r.get("encrypted_secret"),
            key_id: r.get("key_id"),
        }))
    }

    pub async fn get_secrets(
        &self,
        user_id: &str,
        secret_names: &[String],
    ) -> Result<Vec<StoredSecret>, ApiError> {
        if secret_names.is_empty() {
            return Ok(Vec::new());
        }

        let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
            r#"
            SELECT user_id, secret_name, encrypted_secret, key_id
            FROM e2ee_stored_secrets
            WHERE user_id = $1 AND secret_name = ANY($2)
            "#,
        )
        .bind(user_id)
        .bind(secret_names)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r| StoredSecret {
                secret_name: r.get("secret_name"),
                encrypted_secret: r.get("encrypted_secret"),
                key_id: r.get("key_id"),
            })
            .collect())
    }

    pub async fn delete_secret(&self, user_id: &str, secret_name: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM e2ee_stored_secrets
            WHERE user_id = $1 AND secret_name = $2
            "#,
        )
        .bind(user_id)
        .bind(secret_name)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    pub async fn delete_secrets(
        &self,
        user_id: &str,
        secret_names: &[String],
    ) -> Result<(), ApiError> {
        if secret_names.is_empty() {
            return Ok(());
        }

        sqlx::query(
            r#"
            DELETE FROM e2ee_stored_secrets
            WHERE user_id = $1 AND secret_name = ANY($2)
            "#,
        )
        .bind(user_id)
        .bind(secret_names)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    pub async fn has_secrets(&self, user_id: &str) -> Result<bool, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count FROM e2ee_secret_storage_keys WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let count: i64 = row.get("count");
        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    fn create_test_storage(pool: &PgPool) -> SecretStorage {
        SecretStorage::new(pool)
    }
}
