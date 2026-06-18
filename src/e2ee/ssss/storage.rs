use super::models::{SecretStorageKey, StoredSecret};
use crate::error::ApiError;
use sqlx::PgPool;

/// Internal row representation for `e2ee_secret_storage_keys`.
///
/// The DB schema (v7+) requires `key_name TEXT NOT NULL` and
/// `key_data BYTEA NOT NULL`, but the public [`SecretStorageKey`] model
/// surfaces the cryptographic material through `encrypted_key` instead.
/// `key_name` mirrors `key_id`; `key_data` is a placeholder empty bytea
/// because the real ciphertext lives in `encrypted_key` (TEXT) for this
/// homeserver's SSSS layout.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SecretStorageKeyRow {
    pub key_id: String,
    pub user_id: String,
    pub algorithm: String,
    pub encrypted_key: String,
    pub public_key: Option<String>,
    pub signatures: Option<serde_json::Value>,
    pub created_ts: i64,
}

impl SecretStorageKeyRow {
    fn into_storage_key(self) -> SecretStorageKey {
        SecretStorageKey {
            key_id: self.key_id,
            user_id: self.user_id,
            algorithm: self.algorithm,
            encrypted_key: self.encrypted_key,
            public_key: self.public_key,
            signatures: self.signatures.unwrap_or(serde_json::json!({})),
            created_ts: self.created_ts,
        }
    }
}

/// Internal row representation for `e2ee_stored_secrets`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StoredSecretRow {
    pub secret_name: String,
    pub encrypted_secret: Option<String>,
    pub key_id: Option<String>,
}

impl StoredSecretRow {
    fn into_stored_secret(self) -> StoredSecret {
        StoredSecret {
            secret_name: self.secret_name,
            encrypted_secret: self.encrypted_secret.unwrap_or_default(),
            key_id: self.key_id.unwrap_or_default(),
        }
    }
}

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
            r"
            INSERT INTO e2ee_secret_storage_keys
                (key_id, key_name, user_id, algorithm, key_data,
                 encrypted_key, public_key, signatures, created_ts, updated_ts, is_active)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9, TRUE)
            ON CONFLICT (key_id, user_id) DO UPDATE SET
                algorithm = EXCLUDED.algorithm,
                key_data = EXCLUDED.key_data,
                encrypted_key = EXCLUDED.encrypted_key,
                public_key = EXCLUDED.public_key,
                signatures = EXCLUDED.signatures,
                updated_ts = EXCLUDED.updated_ts,
                is_active = TRUE
            ",
        )
        .bind(&key.key_id)
        .bind(&key.key_id)
        .bind(&key.user_id)
        .bind(&key.algorithm)
        .bind(Vec::<u8>::new())
        .bind(&key.encrypted_key)
        .bind(&key.public_key)
        .bind(&key.signatures)
        .bind(key.created_ts)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_key(&self, user_id: &str, key_id: &str) -> Result<Option<SecretStorageKey>, ApiError> {
        let row: Option<SecretStorageKeyRow> = sqlx::query_as::<_, SecretStorageKeyRow>(
            r#"
            SELECT
                key_id,
                user_id,
                algorithm,
                encrypted_key,
                public_key,
                signatures,
                created_ts
            FROM e2ee_secret_storage_keys
            WHERE user_id = $1 AND key_id = $2 AND is_active = TRUE
            "#,
        )
        .bind(user_id)
        .bind(key_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(row.map(SecretStorageKeyRow::into_storage_key))
    }

    pub async fn get_all_keys(&self, user_id: &str) -> Result<Vec<SecretStorageKey>, ApiError> {
        let rows: Vec<SecretStorageKeyRow> = sqlx::query_as::<_, SecretStorageKeyRow>(
            r#"
            SELECT
                key_id,
                user_id,
                algorithm,
                encrypted_key,
                public_key,
                signatures,
                created_ts
            FROM e2ee_secret_storage_keys
            WHERE user_id = $1 AND is_active = TRUE
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows.into_iter().map(SecretStorageKeyRow::into_storage_key).collect())
    }

    pub async fn delete_key(&self, user_id: &str, key_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r"
            UPDATE e2ee_secret_storage_keys
            SET is_active = FALSE, updated_ts = (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT
            WHERE user_id = $1 AND key_id = $2 AND is_active = TRUE
            ",
        )
        .bind(user_id)
        .bind(key_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn store_secret(&self, user_id: &str, secret: &StoredSecret) -> Result<(), ApiError> {
        let now_ts = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r"
            INSERT INTO e2ee_stored_secrets
                (user_id, secret_name, secret_data, key_key_id,
                 encrypted_secret, key_id, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
            ON CONFLICT (user_id, secret_name) DO UPDATE SET
                secret_data = EXCLUDED.secret_data,
                key_key_id = EXCLUDED.key_key_id,
                encrypted_secret = EXCLUDED.encrypted_secret,
                key_id = EXCLUDED.key_id,
                updated_ts = EXCLUDED.updated_ts
            ",
        )
        .bind(user_id)
        .bind(&secret.secret_name)
        .bind(Vec::<u8>::new())
        .bind(&secret.key_id)
        .bind(&secret.encrypted_secret)
        .bind(&secret.key_id)
        .bind(now_ts)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_secret(&self, user_id: &str, secret_name: &str) -> Result<Option<StoredSecret>, ApiError> {
        let row: Option<StoredSecretRow> = sqlx::query_as::<_, StoredSecretRow>(
            r#"
            SELECT
                secret_name,
                encrypted_secret,
                key_id
            FROM e2ee_stored_secrets
            WHERE user_id = $1 AND secret_name = $2
            "#,
        )
        .bind(user_id)
        .bind(secret_name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(row.map(StoredSecretRow::into_stored_secret))
    }

    pub async fn get_secrets(&self, user_id: &str, secret_names: &[String]) -> Result<Vec<StoredSecret>, ApiError> {
        if secret_names.is_empty() {
            return Ok(Vec::new());
        }

        let rows: Vec<StoredSecretRow> = sqlx::query_as::<_, StoredSecretRow>(
            r#"
            SELECT
                secret_name,
                encrypted_secret,
                key_id
            FROM e2ee_stored_secrets
            WHERE user_id = $1 AND secret_name = ANY($2)
            "#,
        )
        .bind(user_id)
        .bind(secret_names)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows.into_iter().map(StoredSecretRow::into_stored_secret).collect())
    }

    pub async fn delete_secret(&self, user_id: &str, secret_name: &str) -> Result<(), ApiError> {
        sqlx::query(
            r"
            DELETE FROM e2ee_stored_secrets
            WHERE user_id = $1 AND secret_name = $2
            ",
        )
        .bind(user_id)
        .bind(secret_name)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn delete_secrets(&self, user_id: &str, secret_names: &[String]) -> Result<(), ApiError> {
        if secret_names.is_empty() {
            return Ok(());
        }

        sqlx::query(
            r"
            DELETE FROM e2ee_stored_secrets
            WHERE user_id = $1 AND secret_name = ANY($2)
            ",
        )
        .bind(user_id)
        .bind(secret_names)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn has_secrets(&self, user_id: &str) -> Result<bool, ApiError> {
        let count: i64 = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM e2ee_secret_storage_keys
            WHERE user_id = $1 AND is_active = TRUE
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {}
