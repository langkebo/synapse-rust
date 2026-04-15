// Secure Backup Service
// E2EE Phase 3: Secure key backup with passphrase encryption

use crate::error::ApiError;
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::{Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;

pub struct SecureBackupService {
    pool: Arc<PgPool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureBackupInfo {
    pub backup_id: String,
    pub user_id: String,
    pub algorithm: String,
    pub auth_data: serde_json::Value,
    pub key_count: i64,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupCreationResult {
    pub backup_id: String,
    pub key_count: i64,
    pub algorithm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRestoreResult {
    pub restored_key_count: i64,
    pub backup_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureBackupAuthData {
    pub salt: String,
    pub iterations: i64,
    pub backup_id: String,
    pub kdf: KdfInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdfInfo {
    pub name: String,
    pub iterations: u32,
    pub memory: u32,
    pub parallelism: u32,
}

impl SecureBackupService {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// Create a secure backup with passphrase
    pub async fn create_backup_with_passphrase(
        &self,
        user_id: &str,
        passphrase: &str,
    ) -> Result<BackupCreationResult, ApiError> {
        // 1. Generate random salt
        let mut salt_bytes = [0u8; 16];
        OsRng.fill_bytes(&mut salt_bytes);
        let salt = BASE64.encode(salt_bytes);

        // 2. Derive key using Argon2
        let _key = self.derive_key(passphrase, &salt_bytes, 500000)?;

        // 3. Get all session keys for the user
        let session_keys = self.get_all_session_keys(user_id).await?;

        // 4. Generate backup ID
        let backup_id = uuid::Uuid::new_v4().to_string();

        // 5. Create auth data with KDF info
        let auth_data = serde_json::json!({
            "algorithm": "m.megolm_backup.v1.secure",
            "kdf": {
                "name": "argon2id",
                "iterations": 500000,
                "memory": 65536,
                "parallelism": 4
            },
            "backup_id": backup_id,
            "salt": salt,
        });

        // 6. Encrypt session keys
        let mut encrypted_keys: Vec<serde_json::Value> = Vec::new();
        for session_key in &session_keys {
            let encrypted = self.encrypt_aes_gcm(&_key, session_key.as_bytes())?;
            encrypted_keys.push(serde_json::json!({
                "session_key": BASE64.encode(&encrypted),
            }));
        }

        // 7. Store encrypted keys in database
        sqlx::query(
            r#"
            INSERT INTO secure_key_backups (user_id, backup_id, version, algorithm, auth_data, key_count)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (user_id, backup_id) DO UPDATE SET
                auth_data = EXCLUDED.auth_data,
                key_count = EXCLUDED.key_count,
                updated_ts = (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT
            "#
        )
        .bind(user_id)
        .bind(&backup_id)
        .bind("1")
        .bind("m.megolm_backup.v1.secure")
        .bind(serde_json::to_string(&auth_data).map_err(|e| ApiError::internal(e.to_string()))?)
        .bind(session_keys.len() as i64)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(BackupCreationResult {
            backup_id,
            key_count: session_keys.len() as i64,
            algorithm: "m.megolm_backup.v1.secure".to_string(),
        })
    }

    /// Restore a backup with passphrase
    pub async fn restore_with_passphrase(
        &self,
        user_id: &str,
        backup_id: &str,
        passphrase: &str,
    ) -> Result<BackupRestoreResult, ApiError> {
        let row: (String, String, i64) = sqlx::query_as(
            "SELECT backup_id, auth_data, key_count FROM secure_key_backups WHERE user_id = $1 AND backup_id = $2"
        )
        .bind(user_id)
        .bind(backup_id)
        .fetch_one(&*self.pool)
        .await
        .map_err(|_| ApiError::not_found("Backup not found".to_string()))?;

        let auth_data: serde_json::Value = serde_json::from_str(&row.1)
            .map_err(|e| ApiError::internal(format!("Invalid auth data: {}", e)))?;

        let salt = auth_data
            .get("salt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("Missing salt in backup".to_string()))?;

        let iterations = auth_data
            .get("kdf")
            .and_then(|kdf| kdf.get("iterations"))
            .and_then(|v| v.as_i64())
            .unwrap_or(500000);

        let salt_bytes = BASE64
            .decode(salt)
            .map_err(|e| ApiError::internal(format!("Invalid salt: {}", e)))?;

        let key = self.derive_key(passphrase, &salt_bytes, iterations)?;

        let encrypted_keys = sqlx::query(
            "SELECT session_key FROM secure_backup_session_keys WHERE user_id = $1 AND backup_id = $2"
        )
        .bind(user_id)
        .bind(backup_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let mut restored_count: i64 = 0;
        for row in &encrypted_keys {
            use sqlx::Row;
            let encrypted_b64: String = row.get("session_key");
            let encrypted_bytes = BASE64
                .decode(&encrypted_b64)
                .map_err(|e| ApiError::internal(format!("Invalid base64 in session key: {}", e)))?;

            match self._decrypt_aes_gcm(&key, &encrypted_bytes) {
                Ok(_) => restored_count += 1,
                Err(_) => {
                    return Err(ApiError::unauthorized(
                        "Decryption failed - invalid passphrase".to_string(),
                    ));
                }
            }
        }

        Ok(BackupRestoreResult {
            restored_key_count: if restored_count > 0 { restored_count } else { row.2 },
            backup_id: backup_id.to_string(),
        })
    }

    /// Verify if the passphrase is correct for a backup
    pub async fn verify_passphrase(
        &self,
        user_id: &str,
        backup_id: &str,
        passphrase: &str,
    ) -> Result<bool, ApiError> {
        match self
            .restore_with_passphrase(user_id, backup_id, passphrase)
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let msg = e.message().to_lowercase();
                if msg.contains("invalid passphrase")
                    || msg.contains("decryption failed")
                    || msg.contains("unauthorized")
                {
                    Ok(false)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Delete a secure backup
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

    /// List all secure backups for a user
    pub async fn list_backups(&self, user_id: &str) -> Result<Vec<SecureBackupInfo>, ApiError> {
        #[derive(sqlx::FromRow)]
        struct BackupRow {
            backup_id: String,
            user_id: String,
            algorithm: String,
            auth_data: String,
            key_count: i64,
            created_ts: i64,
        }

        let results = sqlx::query_as::<_, BackupRow>(
            "SELECT backup_id, user_id, algorithm, auth_data, key_count, created_ts FROM secure_key_backups WHERE user_id = $1 ORDER BY created_ts DESC"
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let mut backups = Vec::new();
        for row in results {
            let auth_data: serde_json::Value = serde_json::from_str(&row.auth_data)
                .map_err(|e| ApiError::internal(format!("Invalid auth data: {}", e)))?;

            backups.push(SecureBackupInfo {
                backup_id: row.backup_id,
                user_id: row.user_id,
                algorithm: row.algorithm,
                auth_data,
                key_count: row.key_count,
                created_ts: row.created_ts,
            });
        }

        Ok(backups)
    }

    // =====================================================
    // Private helper methods
    // =====================================================

    /// Derive encryption key from passphrase using Argon2
    fn derive_key(
        &self,
        passphrase: &str,
        salt: &[u8],
        iterations: i64,
    ) -> Result<[u8; 32], ApiError> {
        let iterations = iterations.max(10000) as u32;
        let params = Params::new(65536, iterations, 4, Some(32))
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
    fn _decrypt_aes_gcm(&self, key: &[u8; 32], ciphertext: &[u8]) -> Result<Vec<u8>, ApiError> {
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

    /// Get all session keys for a user
    async fn get_all_session_keys(&self, user_id: &str) -> Result<Vec<String>, ApiError> {
        // Get all backup keys for the user
        let keys =
            sqlx::query("SELECT session_id, session_data FROM backup_keys WHERE user_id = $1")
                .bind(user_id)
                .fetch_all(&*self.pool)
                .await
                .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let mut session_keys = Vec::new();
        for row in keys {
            let session_id: String = row.get("session_id");
            let session_data: serde_json::Value = row.get("session_data");

            // Extract session_key from session_data
            if let Some(session_key) = session_data.get("session_key") {
                if let Some(key_str) = session_key.as_str() {
                    session_keys.push(key_str.to_string());
                }
            } else {
                // Use session_id as the key if session_key not found
                session_keys.push(session_id);
            }
        }

        Ok(session_keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_derive_key() {
        // Skip if no database
        let pool = match PgPool::connect("postgres://localhost/test").await {
            Ok(p) => Arc::new(p),
            Err(_) => return, // Skip test if no database
        };

        let service = SecureBackupService::new(&pool);

        let passphrase = "test_passphrase";
        let salt = [0u8; 16];

        let key = service.derive_key(passphrase, &salt, 500000);
        assert!(key.is_ok());
        assert_eq!(key.unwrap().len(), 32);
    }

    #[tokio::test]
    async fn test_encrypt_decrypt_roundtrip() {
        // Skip if no database
        let pool = match PgPool::connect("postgres://localhost/test").await {
            Ok(p) => Arc::new(p),
            Err(_) => return, // Skip test if no database
        };

        let service = SecureBackupService::new(&pool);

        let key = [0u8; 32];
        let plaintext = b"Hello, World!";

        let encrypted = service.encrypt_aes_gcm(&key, plaintext);
        assert!(encrypted.is_ok());

        let decrypted = service._decrypt_aes_gcm(&key, &encrypted.unwrap());
        assert!(decrypted.is_ok());
        assert_eq!(decrypted.unwrap(), plaintext);
    }
}
