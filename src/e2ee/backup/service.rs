use super::models::*;
use super::storage::{BackupKeyInsertParams, BackupKeyStorage, KeyBackupStorage};
use crate::error::ApiError;
use sqlx::Row;

#[derive(Debug, Clone)]
pub struct BackupKeyUploadParams {
    pub user_id: String,
    pub version: String,
    pub room_id: String,
    pub session_id: String,
    pub first_message_index: i64,
    pub forwarded_count: i64,
    pub is_verified: bool,
    pub session_data: String,
}

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
        let version_i64 = chrono::Utc::now().timestamp();
        let version = version_i64.to_string();
        let auth_key = auth_data
            .as_ref()
            .and_then(|v| v.get("auth_key"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mgmt_key = auth_data
            .as_ref()
            .and_then(|v| v.get("mgmt_key"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let backup = KeyBackup {
            user_id: user_id.to_string(),
            backup_id: version.clone(),
            version: version_i64,
            algorithm: algorithm.to_string(),
            auth_key,
            mgmt_key,
            backup_data: auth_data.unwrap_or(serde_json::json!({})),
            etag: Some(format!("{:x}", version_i64)),
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
            updated_backup.auth_key = data
                .get("auth_key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            updated_backup.mgmt_key = data
                .get("mgmt_key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            updated_backup.backup_data = data;
        }
        updated_backup.etag = Some(format!("{:x}", chrono::Utc::now().timestamp()));

        self.storage.create_backup(&updated_backup).await?;

        Ok(())
    }

    pub async fn delete_backup(&self, user_id: &str, version: &str) -> Result<(), ApiError> {
        self.storage.delete_backup(user_id, version).await?;

        Ok(())
    }

    pub async fn upload_backup_key(&self, params: BackupKeyUploadParams) -> Result<(), ApiError> {
        let backup = self
            .storage
            .get_backup_version(&params.user_id, &params.version)
            .await?
            .ok_or_else(|| {
                ApiError::not_found(format!(
                    "Backup version '{}' not found for user '{}'",
                    params.version, params.user_id
                ))
            })?;

        self.key_storage
            .upload_backup_key(BackupKeyInsertParams {
                user_id: params.user_id,
                backup_id: backup.backup_id,
                room_id: params.room_id,
                session_id: params.session_id,
                first_message_index: params.first_message_index,
                forwarded_count: params.forwarded_count,
                is_verified: params.is_verified,
                backup_data: serde_json::json!({ "session_data": params.session_data }),
            })
            .await?;

        Ok(())
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

        self.key_storage
            .upload_backup_key(BackupKeyInsertParams {
                user_id: user_id.to_string(),
                backup_id: backup.backup_id.clone(),
                room_id: room_id.to_string(),
                session_id: session_id.to_string(),
                first_message_index: 0,
                forwarded_count: 0,
                is_verified: false,
                backup_data: session_data.clone(),
            })
            .await?;

        Ok(())
    }

    pub async fn upload_room_keys_for_room(
        &self,
        user_id: &str,
        room_id: &str,
        version: &str,
        keys: Vec<serde_json::Value>,
    ) -> Result<(), ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, version)
            .await?
            .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        for key in keys {
            let session_id = key["session_id"].as_str().unwrap_or_default().to_string();
            let session_data = key["session_data"].clone();
            let first_message_index = key["first_message_index"].as_i64().unwrap_or(0);
            let forwarded_count = key["forwarded_count"].as_i64().unwrap_or(0);
            let is_verified = key["is_verified"].as_bool().unwrap_or(false);

            self.key_storage
                .upload_backup_key(BackupKeyInsertParams {
                    user_id: user_id.to_string(),
                    backup_id: backup.backup_id.clone(),
                    room_id: room_id.to_string(),
                    session_id,
                    first_message_index,
                    forwarded_count,
                    is_verified,
                    backup_data: session_data,
                })
                .await?;
        }

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

        self.key_storage
            .upload_backup_key(BackupKeyInsertParams {
                user_id: user_id.to_string(),
                backup_id: backup.backup_id.clone(),
                room_id: room_id.to_string(),
                session_id: session_id.to_string(),
                first_message_index: 0,
                forwarded_count: 0,
                is_verified: false,
                backup_data: key_data.clone(),
            })
            .await?;

        Ok(())
    }

    pub async fn get_backup_version(&self, user_id: &str) -> Result<Option<KeyBackup>, ApiError> {
        self.storage.get_backup(user_id).await
    }

    pub async fn get_all_backups(&self, user_id: &str) -> Result<Vec<KeyBackup>, ApiError> {
        self.storage.get_all_backup_versions(user_id).await
    }

    pub async fn get_backup_key_count(&self, user_id: &str) -> Result<i64, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT COALESCE(COUNT(*), 0) as count
            FROM backup_keys
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(&*self.storage.pool)
        .await?;

        Ok(row.try_get::<i64, _>("count")?)
    }

    pub async fn get_all_backup_keys(&self, user_id: &str) -> Result<Vec<BackupKeyInfo>, ApiError> {
        let rows = sqlx::query_as::<_, BackupKeyInfo>(
            r#"
            SELECT user_id, backup_id, room_id, session_id, first_message_index,
                   forwarded_count, is_verified, backup_data
            FROM backup_keys
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.storage.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get_backup_count_per_room(
        &self,
        user_id: &str,
        version: &str,
    ) -> Result<serde_json::Value, ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, version)
            .await?
            .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        let rows = sqlx::query(
            r#"
            SELECT room_id, COALESCE(COUNT(*), 0) as count
            FROM backup_keys
            WHERE user_id = $1 AND backup_id = $2
            GROUP BY room_id
            "#,
        )
        .bind(user_id)
        .bind(&backup.backup_id)
        .fetch_all(&*self.storage.pool)
        .await?;

        let mut rooms: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        for row in rows {
            let room_id: String = row.try_get("room_id")?;
            let count: i64 = row.try_get("count")?;
            rooms.insert(room_id, serde_json::Value::from(count));
        }

        Ok(serde_json::Value::Object(rooms))
    }

    pub async fn get_room_backup_keys(
        &self,
        user_id: &str,
        room_id: &str,
        version: &str,
    ) -> Result<Vec<BackupKeyInfo>, ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, version)
            .await?
            .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        self.key_storage
            .get_room_backup_keys_by_backup_id(user_id, &backup.backup_id, room_id)
            .await
    }

    pub async fn get_backup_key(
        &self,
        user_id: &str,
        room_id: &str,
        session_id: &str,
        version: &str,
    ) -> Result<Option<BackupKeyInfo>, ApiError> {
        let backup = match self.storage.get_backup_version(user_id, version).await? {
            Some(b) => b,
            None => return Ok(None),
        };

        self.key_storage
            .get_backup_key_by_backup_id(user_id, &backup.backup_id, room_id, session_id)
            .await
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
