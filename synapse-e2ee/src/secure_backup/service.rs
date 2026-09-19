// Secure Backup Service
// E2EE Phase 3: Secure key backup (client-side encryption, ciphertext-at-rest only).

use crate::secure_backup::models::*;
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::map_database;
use synapse_common::ApiError;

#[derive(Clone)]
/// The `SecureBackupService` type.
pub struct SecureBackupService {
    pool: Arc<PgPool>,
}

/// Implementation of [`SecureBackupService`] methods.
impl SecureBackupService {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// Create a secure backup with client-provided algorithm and auth_data
    /// This supports the standard Matrix backup creation flow where the client
    /// provides the algorithm and auth_data directly (e.g., m.megolm_backup.v1.curve25519-aes-sha2)
    pub async fn create_backup_with_data(
        &self,
        user_id: &str,
        algorithm: &str,
        auth_data_val: &serde_json::Value,
    ) -> Result<SecureBackupResponse, ApiError> {
        let backup_id = uuid::Uuid::new_v4().to_string();
        let version = chrono::Utc::now().timestamp().to_string();

        // Build SecureBackupAuthData from client-provided auth_data
        let auth_data = SecureBackupAuthData {
            salt: auth_data_val.get("salt").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            iterations: auth_data_val.get("iterations").and_then(|v| v.as_i64()).unwrap_or(0),
            backup_id: backup_id.clone(),
            public_key: auth_data_val.get("public_key").and_then(|v| v.as_str()).map(|s| s.to_string()),
        };

        // Store backup metadata
        sqlx::query(
            r"
            INSERT INTO secure_key_backups (user_id, backup_id, version, algorithm, auth_data, key_count)
            VALUES ($1, $2, $3, $4, $5, 0)
            ON CONFLICT (user_id, backup_id) DO UPDATE SET
                version = EXCLUDED.version,
                auth_data = EXCLUDED.auth_data,
                updated_ts = (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT
            ",
        )
        .bind(user_id)
        .bind(&backup_id)
        .bind(&version)
        .bind(algorithm)
        .bind(serde_json::to_string(&auth_data).map_err(|e| ApiError::internal(e.to_string()))?)
        .execute(&*self.pool)
        .await
        .map_err(map_database!("create_backup_with_data"))?;

        Ok(SecureBackupResponse { backup_id, version, algorithm: algorithm.to_string(), auth_data, key_count: 0 })
    }

    /// Store encrypted session keys (client-side encrypted; server stores ciphertext only).
    ///
    /// ISSUE-6.3: the server no longer derives a key from a passphrase and never
    /// encrypts/decrypts session keys. `SessionKeyData.session_key` is the
    /// client-side ciphertext (m.megolm_backup.v1.curve25519-aes-sha2), stored verbatim.
    pub async fn store_session_keys(
        &self,
        user_id: &str,
        backup_id: &str,
        session_keys: Vec<SessionKeyData>,
    ) -> Result<i64, ApiError> {
        let exists: Option<i64> = sqlx::query_scalar::<_, i64>(
            r"SELECT 1::bigint FROM secure_key_backups WHERE user_id = $1 AND backup_id = $2",
        )
        .bind(user_id)
        .bind(backup_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_database!("store_session_keys"))?;

        if exists.is_none() {
            return Err(ApiError::not_found("Backup not found".to_string()));
        }

        if session_keys.is_empty() {
            return Ok(0);
        }

        let mut room_ids: Vec<&str> = Vec::with_capacity(session_keys.len());
        let mut sids: Vec<&str> = Vec::with_capacity(session_keys.len());
        let mut encrypted_keys: Vec<String> = Vec::with_capacity(session_keys.len());

        // session_key is already client-side ciphertext — store verbatim, no key derivation.
        for session_key in &session_keys {
            room_ids.push(&session_key.room_id);
            sids.push(&session_key.session_id);
            encrypted_keys.push(session_key.session_key.clone());
        }

        let key_count = session_keys.len() as i64;

        sqlx::query(
            r"
            INSERT INTO secure_backup_session_keys (user_id, backup_id, room_id, session_id, encrypted_key)
            SELECT $1, $2, unnest($3::text[]), unnest($4::text[]), unnest($5::text[])
            ON CONFLICT (user_id, backup_id, room_id, session_id) DO UPDATE SET
                encrypted_key = EXCLUDED.encrypted_key
            ",
        )
        .bind(user_id)
        .bind(backup_id)
        .bind(&room_ids)
        .bind(&sids)
        .bind(&encrypted_keys)
        .execute(&*self.pool)
        .await
        .map_err(map_database!("store_session_keys"))?;

        sqlx::query(
            "UPDATE secure_key_backups SET key_count = key_count + $1,
             updated_ts = (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT
             WHERE user_id = $2 AND backup_id = $3",
        )
        .bind(key_count)
        .bind(user_id)
        .bind(backup_id)
        .execute(&*self.pool)
        .await
        .map_err(map_database!("store_session_keys"))?;

        Ok(key_count)
    }

    /// Restore session keys from backup
    pub async fn restore_backup(
        &self,
        user_id: &str,
        backup_id: &str,
        rooms: Option<Vec<String>>,
    ) -> Result<RestoreResponse, ApiError> {
        let total_keys: i64 =
            sqlx::query_scalar("SELECT key_count FROM secure_key_backups WHERE user_id = $1 AND backup_id = $2")
                .bind(user_id)
                .bind(backup_id)
                .fetch_one(&*self.pool)
                .await
                .map_err(|_| ApiError::not_found("Backup not found".to_string()))?;

        // Return ciphertext only; the client decrypts locally with its recovery key.
        // The server never derives a key or decrypts session keys.
        let encrypted_keys: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT room_id, session_id, encrypted_key FROM secure_backup_session_keys
             WHERE user_id = $1 AND backup_id = $2",
        )
        .bind(user_id)
        .bind(backup_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(map_database!("restore_backup"))?;

        let allowed_rooms = rooms.map(|room_ids| room_ids.into_iter().collect::<std::collections::HashSet<_>>());

        let sessions = encrypted_keys
            .into_iter()
            .filter(|(room_id, _session_id, _encrypted)| {
                allowed_rooms.as_ref().map(|a| a.contains(room_id)).unwrap_or(true)
            })
            .map(|(room_id, session_id, session_key)| EncryptedSessionKey { room_id, session_id, session_key })
            .collect();

        Ok(RestoreResponse { total_keys, sessions })
    }

    /// Get backup info
    pub async fn get_backup_info(
        &self,
        user_id: &str,
        backup_id: &str,
    ) -> Result<Option<SecureBackupResponse>, ApiError> {
        let result = sqlx::query_as::<_, SqlxSecureBackup>(
            "SELECT backup_id, version, algorithm, auth_data, key_count
             FROM secure_key_backups WHERE user_id = $1 AND backup_id = $2",
        )
        .bind(user_id)
        .bind(backup_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_database!("get_backup_info"))?;

        match result {
            Some(row) => {
                let auth_data: SecureBackupAuthData =
                    serde_json::from_str(&row.auth_data).map_err(map_database!("Invalid auth data"))?;

                Ok(Some(SecureBackupResponse {
                    backup_id: row.backup_id,
                    version: row.version,
                    algorithm: row.algorithm,
                    auth_data,
                    key_count: row.key_count,
                }))
            }
            None => Ok(None),
        }
    }

    /// List all backups for user
    pub async fn list_backups(&self, user_id: &str) -> Result<Vec<SecureBackupResponse>, ApiError> {
        let results = sqlx::query_as::<_, SqlxSecureBackup>(
            "SELECT backup_id, version, algorithm, auth_data, key_count
             FROM secure_key_backups WHERE user_id = $1 ORDER BY created_ts DESC",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(map_database!("list_backups"))?;

        let mut backups = Vec::new();
        for row in results {
            let auth_data: SecureBackupAuthData =
                serde_json::from_str(&row.auth_data).map_err(map_database!("Invalid auth data"))?;

            backups.push(SecureBackupResponse {
                backup_id: row.backup_id,
                version: row.version,
                algorithm: row.algorithm,
                auth_data,
                key_count: row.key_count,
            });
        }

        Ok(backups)
    }

    /// Delete backup
    pub async fn delete_backup(&self, user_id: &str, backup_id: &str) -> Result<(), ApiError> {
        let mut tx = self.pool.begin().await.map_err(map_database!("Failed to begin transaction for delete_backup"))?;

        // Delete session keys first
        sqlx::query("DELETE FROM secure_backup_session_keys WHERE user_id = $1 AND backup_id = $2")
            .bind(user_id)
            .bind(backup_id)
            .execute(&mut *tx)
            .await
            .map_err(map_database!("delete_backup"))?;

        // Delete backup
        sqlx::query("DELETE FROM secure_key_backups WHERE user_id = $1 AND backup_id = $2")
            .bind(user_id)
            .bind(backup_id)
            .execute(&mut *tx)
            .await
            .map_err(map_database!("delete_backup"))?;

        tx.commit().await.map_err(map_database!("Failed to commit transaction for delete_backup"))?;

        Ok(())
    }
}

// SQLx row type
#[derive(sqlx::FromRow)]
struct SqlxSecureBackup {
    backup_id: String,
    version: String,
    algorithm: String,
    auth_data: String,
    key_count: i64,
}
