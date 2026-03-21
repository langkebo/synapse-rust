// Secure Backup Service
// E2EE Phase 3: Secure key backup with passphrase

use crate::e2ee::secure_backup::models::*;
use crate::error::ApiError;
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::{Argon2, Params, Version};
use base64::Engine;
use rand::RngCore;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct SecureBackupService {
    pool: Arc<PgPool>,
}

impl SecureBackupService {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// Create a secure backup with passphrase
    pub async fn create_backup(
        &self,
        user_id: &str,
        passphrase: &str,
    ) -> Result<SecureBackupResponse, ApiError> {
        // 1. Generate salt
        let mut salt_bytes = [0u8; 16];
        OsRng.fill_bytes(&mut salt_bytes);
        let salt = base64::engine::general_purpose::STANDARD.encode(salt_bytes);

        // 2. Derive key using Argon2
        let _key = self.derive_key(passphrase, &salt_bytes, 500000)?;

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
            r#"
            INSERT INTO secure_key_backups (user_id, backup_id, version, algorithm, auth_data, key_count)
            VALUES ($1, $2, $3, $4, $5, 0)
            ON CONFLICT (user_id, backup_id) DO UPDATE SET
                version = EXCLUDED.version,
                auth_data = EXCLUDED.auth_data,
                updated_at = CURRENT_TIMESTAMP
            "#
        )
        .bind(user_id)
        .bind(&backup_id)
        .bind(&version)
        .bind("m.megolm_backup.v1.secure")
        .bind(serde_json::to_string(&auth_data).map_err(|e| ApiError::internal(e.to_string()))?)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(SecureBackupResponse {
            backup_id,
            version,
            algorithm: "m.megolm_backup.v1.secure".to_string(),
            auth_data,
            key_count: 0,
        })
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
        let auth_data_str: Option<String> = sqlx::query_scalar(
            "SELECT auth_data FROM secure_key_backups WHERE user_id = $1 AND backup_id = $2",
        )
        .bind(user_id)
        .bind(backup_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        let auth_data_str =
            auth_data_str.ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;
        let auth_data: SecureBackupAuthData = serde_json::from_str(&auth_data_str)
            .map_err(|e| ApiError::internal(format!("Invalid auth data: {}", e)))?;

        // 2. Derive key
        let salt_bytes = base64::engine::general_purpose::STANDARD
            .decode(&auth_data.salt)
            .map_err(|e| ApiError::internal(format!("Invalid salt: {}", e)))?;

        let key = self.derive_key(passphrase, &salt_bytes, auth_data.iterations)?;

        // 3. Encrypt and store each session key
        let mut key_count = 0i64;

        for session_key in session_keys {
            // Encrypt session key
            let encrypted = self.encrypt_aes_gcm(&key, session_key.session_key.as_bytes())?;
            let encrypted_b64 = base64::engine::general_purpose::STANDARD.encode(&encrypted);

            // Store encrypted key
            sqlx::query(
                r#"
                INSERT INTO secure_backup_session_keys (user_id, backup_id, room_id, session_id, encrypted_key)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (user_id, backup_id, room_id, session_id) DO UPDATE SET
                    encrypted_key = EXCLUDED.encrypted_key
                "#
            )
            .bind(user_id)
            .bind(backup_id)
            .bind(&session_key.room_id)
            .bind(&session_key.session_id)
            .bind(&encrypted_b64)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

            key_count += 1;
        }

        // 4. Update backup key count
        sqlx::query(
            "UPDATE secure_key_backups SET key_count = key_count + $1, updated_at = CURRENT_TIMESTAMP 
             WHERE user_id = $2 AND backup_id = $3"
        )
        .bind(key_count)
        .bind(user_id)
        .bind(backup_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(key_count)
    }

    /// Restore session keys from backup
    pub async fn restore_backup(
        &self,
        user_id: &str,
        backup_id: &str,
        passphrase: &str,
    ) -> Result<RestoreResponse, ApiError> {
        // 1. Get backup auth data
        let row: (String, i64) = sqlx::query_as(
            "SELECT auth_data, key_count FROM secure_key_backups WHERE user_id = $1 AND backup_id = $2"
        )
        .bind(user_id)
        .bind(backup_id)
        .fetch_one(&*self.pool)
        .await
        .map_err(|_| ApiError::not_found("Backup not found".to_string()))?;

        let auth_data_str = row.0;

        let auth_data: SecureBackupAuthData = serde_json::from_str(&auth_data_str)
            .map_err(|e| ApiError::internal(format!("Invalid auth data: {}", e)))?;

        // 2. Derive key
        let salt_bytes = base64::engine::general_purpose::STANDARD
            .decode(&auth_data.salt)
            .map_err(|e| ApiError::internal(format!("Invalid salt: {}", e)))?;

        let key = self.derive_key(passphrase, &salt_bytes, auth_data.iterations)?;

        // 3. Get all encrypted session keys
        let encrypted_keys: Vec<(String, String)> = sqlx::query_as(
            "SELECT session_id, encrypted_key FROM secure_backup_session_keys 
             WHERE user_id = $1 AND backup_id = $2",
        )
        .bind(user_id)
        .bind(backup_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .into_iter()
        .collect();

        // 4. Decrypt session keys
        let mut restored_count = 0i64;
        for (_session_id, encrypted_b64) in encrypted_keys {
            match base64::engine::general_purpose::STANDARD.decode(&encrypted_b64) {
                Ok(encrypted) => {
                    if self.decrypt_aes_gcm(&key, &encrypted).is_ok() {
                        restored_count += 1;
                    }
                }
                Err(_) => continue,
            }
        }

        Ok(RestoreResponse {
            success: restored_count > 0,
            key_count: restored_count,
            message: if restored_count > 0 {
                format!("Successfully restored {} session keys", restored_count)
            } else {
                "Failed to restore any session keys. Check your passphrase.".to_string()
            },
        })
    }

    /// Verify passphrase
    pub async fn verify_passphrase(
        &self,
        user_id: &str,
        backup_id: &str,
        passphrase: &str,
    ) -> Result<bool, ApiError> {
        // Try to restore - if successful, passphrase is valid
        let result = self.restore_backup(user_id, backup_id, passphrase).await?;
        Ok(result.success)
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
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        match result {
            Some(row) => {
                let auth_data: SecureBackupAuthData = serde_json::from_str(&row.auth_data)
                    .map_err(|e| ApiError::internal(format!("Invalid auth data: {}", e)))?;

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
             FROM secure_key_backups WHERE user_id = $1 ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let mut backups = Vec::new();
        for row in results {
            let auth_data: SecureBackupAuthData = serde_json::from_str(&row.auth_data)
                .map_err(|e| ApiError::internal(format!("Invalid auth data: {}", e)))?;

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
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        // Delete backup
        sqlx::query("DELETE FROM secure_key_backups WHERE user_id = $1 AND backup_id = $2")
            .bind(user_id)
            .bind(backup_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    // =====================================================
    // Private helper methods
    // =====================================================

    /// Derive encryption key from passphrase using Argon2
    fn derive_key(
        &self,
        passphrase: &str,
        salt: &[u8],
        _iterations: i64,
    ) -> Result<[u8; 32], ApiError> {
        let params = Params::new(65536, 3, 4, Some(32))
            .map_err(|e| ApiError::internal(format!("Argon2 params error: {}", e)))?;

        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, Version::V0x13, params);

        let mut key = [0u8; 32];
        argon2
            .hash_password_into(passphrase.as_bytes(), salt, &mut key)
            .map_err(|e| ApiError::internal(format!("Key derivation error: {}", e)))?;

        Ok(key)
    }

    /// Encrypt data using AES-256-GCM
    fn encrypt_aes_gcm(&self, key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, ApiError> {
        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|e| ApiError::internal(format!("Cipher error: {}", e)))?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| ApiError::internal(format!("Encryption error: {}", e)))?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);

        Ok(result)
    }

    /// Decrypt data using AES-256-GCM
    fn decrypt_aes_gcm(&self, key: &[u8; 32], ciphertext: &[u8]) -> Result<Vec<u8>, ApiError> {
        if ciphertext.len() < 12 {
            return Err(ApiError::internal("Ciphertext too short".to_string()));
        }

        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|e| ApiError::internal(format!("Cipher error: {}", e)))?;

        let nonce = Nonce::from_slice(&ciphertext[..12]);
        let encrypted = &ciphertext[12..];

        cipher.decrypt(nonce, encrypted).map_err(|_| {
            ApiError::unauthorized("Decryption failed - invalid passphrase".to_string())
        })
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
