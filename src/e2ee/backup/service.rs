use super::models::*;
use super::storage::BackupKeyStorage;
use crate::e2ee::crypto::aes::{Aes256GcmCipher, Aes256GcmKey};
use std::sync::Arc;
use crate::error::ApiError;

pub struct BackupKeyService {
    storage: BackupKeyStorage<'static>,
    encryption_key: [u8; 32],
}

impl BackupKeyService {
    pub fn new(storage: BackupKeyStorage<'static>, encryption_key: [u8; 32]) -> Self {
        Self { storage, encryption_key }
    }
    
    pub async fn create_backup(&self, user_id: &str, algorithm: &str) -> Result<BackupVersion, ApiError> {
        let version = uuid::Uuid::new_v4().to_string();
        
        let backup = KeyBackup {
            id: uuid::Uuid::new_v4(),
            user_id: user_id.to_string(),
            version: version.clone(),
            algorithm: algorithm.to_string(),
            auth_data: serde_json::json!({}),
            encrypted_data: serde_json::json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        self.storage.create_backup(&backup).await?;
        
        Ok(BackupVersion {
            version: version.clone(),
            algorithm: algorithm.to_string(),
            auth_data: serde_json::json!({}),
            count: 0,
            etag: uuid::Uuid::new_v4().to_string(),
        })
    }
    
    pub async fn get_backup(&self, user_id: &str) -> Result<Option<BackupVersion>, ApiError> {
        let backup = self.storage.get_backup(user_id).await?;
        
        Ok(backup.map(|b| BackupVersion {
            version: b.version.clone(),
            algorithm: b.algorithm.clone(),
            auth_data: b.auth_data.clone(),
            count: 0,
            etag: uuid::Uuid::new_v4().to_string(),
        }))
    }
    
    pub async fn upload_backup(&self, user_id: &str, room_id: &str, session_id: &str, request: BackupKeyUploadRequest) -> Result<BackupUploadResponse, ApiError> {
        let backup = self.storage.get_backup(user_id).await?
            .ok_or_else(|| ApiError::NotFound("Backup not found".to_string()))?;
        
        let encrypted_data = serde_json::json!({
            room_id: {
                session_id: request
            }
        });
        
        let mut updated_backup = backup.clone();
        updated_backup.encrypted_data = encrypted_data;
        updated_backup.updated_at = Utc::now();
        
        self.storage.update_backup(&updated_backup).await?;
        
        Ok(BackupUploadResponse {
            etag: uuid::Uuid::new_v4().to_string(),
            count: 1,
        })
    }
    
    pub async fn download_backup(&self, user_id: &str, room_id: &str, session_id: &str) -> Result<Option<BackupKeyUploadRequest>, ApiError> {
        let backup = self.storage.get_backup(user_id).await?;
        
        Ok(backup.and_then(|b| {
            b.encrypted_data.get(room_id)
                .and_then(|room| room.get(session_id))
                .and_then(|data| serde_json::from_value(data.clone()).ok())
        }))
    }
    
    pub async fn delete_backup(&self, user_id: &str, version: &str) -> Result<(), ApiError> {
        self.storage.delete_backup(user_id, version).await?;
        Ok(())
    }
}