use super::models::*;
use super::storage::{BackupKeyStorage, KeyBackupStorage};
use crate::error::ApiError;
use serde_json::json;
use uuid::Uuid;

#[derive(Clone)]
pub struct KeyBackupService {
    storage: KeyBackupStorage,
    key_storage: BackupKeyStorage,
}

impl KeyBackupService {
    pub fn new(storage: KeyBackupStorage) -> Self {
        Self {
            storage: storage.clone(),
            key_storage: BackupKeyStorage::new(&storage.pool),
        }
    }

    pub async fn create_backup(
        &self,
        user_id: &str,
        algorithm: &str,
        auth_data: Option<serde_json::Value>,
    ) -> Result<String, ApiError> {
        let version = format!("{}", chrono::Utc::now().timestamp());
        let backup = KeyBackup {
            id: Uuid::new_v4(),
            user_id: user_id.to_string(),
            version: version.clone(),
            algorithm: algorithm.to_string(),
            auth_data: auth_data.unwrap_or(serde_json::json!({})),
            encrypted_data: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        self.storage.create_backup(&backup).await?;

        Ok(version)
    }

    pub async fn get_backup(
        &self,
        user_id: &str,
        version: &str,
    ) -> Result<Option<KeyBackup>, ApiError> {
        self.storage.get_backup_version(user_id, version).await
    }

    pub async fn update_backup_auth_data(
        &self,
        user_id: &str,
        version: &str,
        auth_data: Option<serde_json::Value>,
    ) -> Result<(), ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, version)
            .await?
            .ok_or_else(|| {
                ApiError::not_found(format!(
                    "Backup version '{}' not found for user '{}'",
                    version, user_id
                ))
            })?;

        let mut updated_backup = backup;
        if let Some(data) = auth_data {
            updated_backup.auth_data = data;
        }
        updated_backup.updated_at = chrono::Utc::now();

        self.storage.update_backup(&updated_backup).await?;

        Ok(())
    }

    pub async fn delete_backup(&self, user_id: &str, version: &str) -> Result<(), ApiError> {
        self.storage.delete_backup(user_id, version).await?;

        Ok(())
    }

    pub async fn upload_backup_key(
        &self,
        user_id: &str,
        version: &str,
        room_id: &str,
        session_id: &str,
        first_message_index: i64,
        forwarded_count: i64,
        is_verified: bool,
        session_data: &str,
    ) -> Result<(), ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, version)
            .await?
            .ok_or_else(|| {
                ApiError::not_found(format!(
                    "Backup version '{}' not found for user '{}'",
                    version, user_id
                ))
            })?;

        self.key_storage
            .upload_backup_key(
                &backup.id,
                room_id,
                session_id,
                first_message_index,
                forwarded_count,
                is_verified,
                session_data,
            )
            .await?;

        Ok(())
    }

    pub async fn get_room_backup_keys(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> Result<serde_json::Value, ApiError> {
        let keys = self
            .key_storage
            .get_room_backup_keys(user_id, room_id)
            .await?;

        let mut sessions = serde_json::Map::new();
        for key in keys {
            let session_data: serde_json::Value = serde_json::from_str(&key.session_data)
                .map_err(|e| ApiError::internal(format!("Invalid session data: {}", e)))?;

            sessions.insert(
                key.session_id.clone(),
                json!({
                    "first_message_index": key.first_message_index,
                    "forwarded_count": key.forwarded_count,
                    "is_verified": key.is_verified,
                    "session_data": session_data
                }),
            );
        }

        Ok(serde_json::Value::Object(sessions))
    }

    pub async fn get_backup_key(
        &self,
        user_id: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<Option<BackupKeyInfo>, ApiError> {
        self.key_storage
            .get_backup_key(user_id, room_id, session_id)
            .await
    }

    pub async fn delete_backup_key(
        &self,
        user_id: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<(), ApiError> {
        self.key_storage
            .delete_backup_key(user_id, room_id, session_id)
            .await?;

        Ok(())
    }

    pub async fn upload_room_key(
        &self,
        user_id: &str,
        room_id: &str,
        session_id: &str,
        session_data: &serde_json::Value,
    ) -> Result<(), ApiError> {
        let backup = self.storage.get_backup(user_id).await?.ok_or_else(|| {
            ApiError::not_found(format!("No backup found for user '{}'", user_id))
        })?;

        let session_data_str = serde_json::to_string(session_data).map_err(|e| {
            ApiError::internal(format!(
                "Failed to serialize session data for room '{}', session '{}': {}",
                room_id, session_id, e
            ))
        })?;

        self.key_storage
            .upload_backup_key(
                &backup.id,
                room_id,
                session_id,
                0,
                0,
                false,
                &session_data_str,
            )
            .await?;

        Ok(())
    }

    pub async fn upload_room_keys_for_room(
        &self,
        user_id: &str,
        room_id: &str,
        keys: Vec<serde_json::Value>,
    ) -> Result<(), ApiError> {
        let backup = self
            .storage
            .get_backup(user_id)
            .await?
            .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        let backup_keys: Vec<super::models::BackupKeyUpload> = keys
            .into_iter()
            .map(|key| {
                let session_id = key["session_id"].as_str().unwrap_or_default().to_string();
                let session_data = serde_json::to_string(&key["session_data"]).map_err(|e| {
                    ApiError::internal(format!("Failed to serialize session data: {}", e))
                })?;
                let first_message_index = key["first_message_index"].as_i64().unwrap_or(0);
                let forwarded_count = key["forwarded_count"].as_i64().unwrap_or(0);
                let is_verified = key["is_verified"].as_bool().unwrap_or(false);

                Ok(super::models::BackupKeyUpload {
                    session_id,
                    session_data,
                    first_message_index,
                    forwarded_count,
                    is_verified,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?;

        self.key_storage
            .upload_backup_keys_batch(&backup.id, room_id, backup_keys)
            .await?;

        Ok(())
    }

    pub async fn store_backup_key(
        &self,
        user_id: &str,
        version: &str,
        room_id: &str,
        session_id: &str,
        key_data: &serde_json::Value,
    ) -> Result<(), ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, version)
            .await?
            .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        let key_data_str = serde_json::to_string(key_data)
            .map_err(|e| ApiError::internal(format!("Failed to serialize key data: {}", e)))?;

        self.key_storage
            .upload_backup_key(&backup.id, room_id, session_id, 0, 0, false, &key_data_str)
            .await?;

        Ok(())
    }

    pub async fn get_backup_version(&self, user_id: &str) -> Result<Option<KeyBackup>, ApiError> {
        self.storage.get_backup(user_id).await
    }

    pub async fn get_all_backups(&self, user_id: &str) -> Result<Vec<KeyBackup>, ApiError> {
        self.storage
            .get_backup(user_id)
            .await
            .map(|v| v.map(|b| vec![b]).unwrap_or_default())
    }

    pub async fn get_backup_key_count(&self, user_id: &str) -> Result<i64, ApiError> {
        let result = sqlx::query!(
            r#"
            SELECT COUNT(*) as count FROM backup_keys bk
            INNER JOIN key_backups kb ON bk.backup_id = kb.id
            WHERE kb.user_id = $1
            "#,
            user_id
        )
        .fetch_one(&*self.storage.pool)
        .await?;
        Ok(result.count.unwrap_or(0))
    }

    pub async fn get_all_backup_keys(&self, user_id: &str) -> Result<Vec<BackupKeyInfo>, ApiError> {
        let rows = sqlx::query_as!(
            BackupKeyInfo,
            r#"
            SELECT bk.id, bk.backup_id, bk.room_id, bk.session_id, bk.first_message_index,
                   bk.forwarded_count, bk.is_verified, bk.session_data, bk.created_at
            FROM backup_keys bk
            INNER JOIN key_backups kb ON bk.backup_id = kb.id
            WHERE kb.user_id = $1
            "#,
            user_id
        )
        .fetch_all(&*self.storage.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get_room_key(
        &self,
        user_id: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<Option<BackupKeyInfo>, ApiError> {
        self.key_storage
            .get_backup_key(user_id, room_id, session_id)
            .await
    }
}
