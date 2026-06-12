// Secure Backup Service
// E2EE Phase 3: Secure key backup with passphrase

use crate::secure_backup::models::*;
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::{Argon2, Params, Version};
use base64::Engine;
use rand::RngCore;
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::ApiError;

#[derive(Clone)]
pub struct SecureBackupService {
    pool: Arc<PgPool>,
}

impl SecureBackupService {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// Create a secure backup with passphrase
    pub async fn create_backup(&self, user_id: &str, passphrase: &str) -> Result<SecureBackupResponse, ApiError> {
        // 1. Generate salt
        let mut salt_bytes = [0u8; 16];
        OsRng.fill_bytes(&mut salt_bytes);
        let salt = base64::engine::general_purpose::STANDARD.encode(salt_bytes);

        // 2. Derive key using Argon2
        let _key = Self::derive_key(passphrase, &salt_bytes, 500000)?;

        // 3. Generate backup ID and version
        let backup_id = uuid::Uuid::new_v4().to_string();
        let version = chrono::Utc::now().timestamp().to_string();

        // 4. Create auth data
        let auth_data = SecureBackupAuthData {
            salt: salt.clone(),
            iterations: 500000,
            backup_id: backup_id.clone(),
            public_key: None,
        };

        // 5. Store backup metadata
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
        .bind("m.megolm_backup.v1.secure")
        .bind(serde_json::to_string(&auth_data).map_err(|e| ApiError::internal(e.to_string()))?)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(SecureBackupResponse {
            backup_id,
            version,
            algorithm: "m.megolm_backup.v1.secure".to_string(),
            auth_data,
            key_count: 0,
        })
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
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(SecureBackupResponse { backup_id, version, algorithm: algorithm.to_string(), auth_data, key_count: 0 })
    }

    /// Store encrypted session keys
    pub async fn store_session_keys(
        &self,
        user_id: &str,
        backup_id: &str,
        passphrase: &str,
        session_keys: Vec<SessionKeyData>,
    ) -> Result<i64, ApiError> {
        // 1. Get backup auth data
        let auth_data_str: Option<String> = sqlx::query_scalar::<_, String>(
            r"
            SELECT auth_data
            FROM secure_key_backups
            WHERE user_id = $1 AND backup_id = $2
            ",
        )
        .bind(user_id)
        .bind(backup_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let auth_data_str = auth_data_str.ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        let auth_data: SecureBackupAuthData = serde_json::from_str(&auth_data_str).map_err(|e| {
            tracing::error!("Invalid auth data: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        // 2. Derive key
        let salt_bytes = base64::engine::general_purpose::STANDARD.decode(&auth_data.salt).map_err(|e| {
            tracing::error!("Invalid salt: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let key = Self::derive_key(passphrase, &salt_bytes, auth_data.iterations)?;

        // 3. Encrypt and store each session key
        let mut key_count = 0i64;

        for session_key in session_keys {
            // Encrypt session key
            let encrypted = Self::encrypt_aes_gcm(&key, session_key.session_key.as_bytes())?;
            let encrypted_b64 = base64::engine::general_purpose::STANDARD.encode(&encrypted);

            // Store encrypted key
            sqlx::query(
                r"
                INSERT INTO secure_backup_session_keys (user_id, backup_id, room_id, session_id, encrypted_key)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (user_id, backup_id, room_id, session_id) DO UPDATE SET
                    encrypted_key = EXCLUDED.encrypted_key
                ",
            )
            .bind(user_id)
            .bind(backup_id)
            .bind(&session_key.room_id)
            .bind(&session_key.session_id)
            .bind(&encrypted_b64)
            .execute(&*self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Database error: {e}");
                ApiError::database("A database error occurred".to_string())
            })?;

            key_count += 1;
        }

        // 4. Update backup key count
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
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(key_count)
    }

    /// Restore session keys from backup
    pub async fn restore_backup(
        &self,
        user_id: &str,
        backup_id: &str,
        passphrase: &str,
        rooms: Option<Vec<String>>,
    ) -> Result<RestoreResponse, ApiError> {
        // 1. Get backup auth data
        let row: (String, i64) =
            sqlx::query_as("SELECT auth_data, key_count FROM secure_key_backups WHERE user_id = $1 AND backup_id = $2")
                .bind(user_id)
                .bind(backup_id)
                .fetch_one(&*self.pool)
                .await
                .map_err(|_| ApiError::not_found("Backup not found".to_string()))?;

        let auth_data_str = row.0;
        let total_keys = row.1;

        let auth_data: SecureBackupAuthData = serde_json::from_str(&auth_data_str).map_err(|e| {
            tracing::error!("Invalid auth data: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        // 2. Derive key
        let salt_bytes = base64::engine::general_purpose::STANDARD.decode(&auth_data.salt).map_err(|e| {
            tracing::error!("Invalid salt: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let key = Self::derive_key(passphrase, &salt_bytes, auth_data.iterations)?;

        // 3. Get all encrypted session keys
        let encrypted_keys: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT room_id, session_id, encrypted_key FROM secure_backup_session_keys
             WHERE user_id = $1 AND backup_id = $2",
        )
        .bind(user_id)
        .bind(backup_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?
        .into_iter()
        .collect();

        let allowed_rooms = rooms.map(|room_ids| room_ids.into_iter().collect::<std::collections::HashSet<_>>());

        // 4. Decrypt session keys
        let mut restored_count = 0i64;
        for (room_id, _session_id, encrypted_b64) in encrypted_keys {
            if let Some(allowed_rooms) = &allowed_rooms {
                if !allowed_rooms.contains(&room_id) {
                    continue;
                }
            }

            match base64::engine::general_purpose::STANDARD.decode(&encrypted_b64) {
                Ok(encrypted) => {
                    if Self::decrypt_aes_gcm(&key, &encrypted).is_ok() {
                        restored_count += 1;
                    }
                }
                Err(_) => continue,
            }
        }

        Ok(RestoreResponse { recovered_keys: restored_count, total_keys })
    }

    /// Verify passphrase
    pub async fn verify_passphrase(&self, user_id: &str, backup_id: &str, passphrase: &str) -> Result<bool, ApiError> {
        // Try to restore - if successful, passphrase is valid
        let result = self.restore_backup(user_id, backup_id, passphrase, None).await?;
        Ok(result.recovered_keys > 0)
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
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        match result {
            Some(row) => {
                let auth_data: SecureBackupAuthData = serde_json::from_str(&row.auth_data).map_err(|e| {
                    tracing::error!("Invalid auth data: {e}");
                    ApiError::database("A database error occurred".to_string())
                })?;

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
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let mut backups = Vec::new();
        for row in results {
            let auth_data: SecureBackupAuthData = serde_json::from_str(&row.auth_data).map_err(|e| {
                tracing::error!("Invalid auth data: {e}");
                ApiError::database("A database error occurred".to_string())
            })?;

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
        // Delete session keys first
        sqlx::query("DELETE FROM secure_backup_session_keys WHERE user_id = $1 AND backup_id = $2")
            .bind(user_id)
            .bind(backup_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Database error: {e}");
                ApiError::database("A database error occurred".to_string())
            })?;

        // Delete backup
        sqlx::query("DELETE FROM secure_key_backups WHERE user_id = $1 AND backup_id = $2")
            .bind(user_id)
            .bind(backup_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Database error: {e}");
                ApiError::database("A database error occurred".to_string())
            })?;

        Ok(())
    }

    // =====================================================
    // Private helper methods
    // =====================================================

    /// Derive encryption key from passphrase using Argon2
    fn derive_key(passphrase: &str, salt: &[u8], _iterations: i64) -> Result<[u8; 32], ApiError> {
        let params = Params::new(65536, 3, 4, Some(32)).map_err(|e| {
            tracing::error!("Argon2 params error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, Version::V0x13, params);

        let mut key = [0u8; 32];
        argon2.hash_password_into(passphrase.as_bytes(), salt, &mut key).map_err(|e| {
            tracing::error!("Key derivation error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(key)
    }

    /// Encrypt data using AES-256-GCM
    fn encrypt_aes_gcm(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, ApiError> {
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| {
            tracing::error!("Cipher error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt
        let ciphertext = cipher.encrypt(nonce, plaintext).map_err(|e| {
            tracing::error!("Encryption error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);

        Ok(result)
    }

    /// Decrypt data using AES-256-GCM
    fn decrypt_aes_gcm(key: &[u8; 32], ciphertext: &[u8]) -> Result<Vec<u8>, ApiError> {
        if ciphertext.len() < 12 {
            return Err(ApiError::internal("Ciphertext too short".to_string()));
        }

        let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| {
            tracing::error!("Cipher error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        let nonce = Nonce::from_slice(&ciphertext[..12]);
        let encrypted = &ciphertext[12..];

        cipher
            .decrypt(nonce, encrypted)
            .map_err(|_| ApiError::unauthorized("Decryption failed - invalid passphrase".to_string()))
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
