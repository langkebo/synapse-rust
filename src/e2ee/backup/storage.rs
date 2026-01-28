use super::models::*;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use chrono::Utc;
use crate::error::ApiError;

pub struct BackupKeyStorage<'a> {
    pool: &'a PgPool,
}

impl<'a> BackupKeyStorage<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_backup(&self, backup: &KeyBackup) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO key_backups (id, user_id, version, algorithm, auth_data, encrypted_data, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, version) DO UPDATE
            SET algorithm = EXCLUDED.algorithm,
                auth_data = EXCLUDED.auth_data,
                encrypted_data = EXCLUDED.encrypted_data,
                updated_at = EXCLUDED.updated_at
            "#
        )
        .bind(backup.id)
        .bind(&backup.user_id)
        .bind(&backup.version)
        .bind(&backup.algorithm)
        .bind(&backup.auth_data)
        .bind(&backup.encrypted_data)
        .bind(backup.created_at)
        .bind(backup.updated_at)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_backup(&self, user_id: &str) -> Result<Option<KeyBackup>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT id, user_id, version, algorithm, auth_data, encrypted_data, created_at, updated_at
            FROM key_backups
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#
        )
        .bind(user_id)
        .fetch_optional(self.pool)
        .await?;
        
        Ok(row.map(|row| KeyBackup {
            id: row.get("id"),
            user_id: row.get("user_id"),
            version: row.get("version"),
            algorithm: row.get("algorithm"),
            auth_data: row.get("auth_data"),
            encrypted_data: row.get("encrypted_data"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }))
    }
    
    pub async fn get_backup_version(&self, user_id: &str, version: &str) -> Result<Option<KeyBackup>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT id, user_id, version, algorithm, auth_data, encrypted_data, created_at, updated_at
            FROM key_backups
            WHERE user_id = $1 AND version = $2
            "#
        )
        .bind(user_id)
        .bind(version)
        .fetch_optional(self.pool)
        .await?;
        
        Ok(row.map(|row| KeyBackup {
            id: row.get("id"),
            user_id: row.get("user_id"),
            version: row.get("version"),
            algorithm: row.get("algorithm"),
            auth_data: row.get("auth_data"),
            encrypted_data: row.get("encrypted_data"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }))
    }
    
    pub async fn delete_backup(&self, user_id: &str, version: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM key_backups
            WHERE user_id = $1 AND version = $2
            "#
        )
        .bind(user_id)
        .bind(version)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn update_backup(&self, backup: &KeyBackup) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            UPDATE key_backups
            SET encrypted_data = $2, updated_at = $3
            WHERE user_id = $1 AND version = $4
            "#
        )
        .bind(&backup.user_id)
        .bind(&backup.encrypted_data)
        .bind(backup.updated_at)
        .bind(&backup.version)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
}