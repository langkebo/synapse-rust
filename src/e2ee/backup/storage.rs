use super::models::*;
use crate::error::ApiError;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct BackupKeyInsertParams {
    pub user_id: String,
    pub backup_id: String,
    pub room_id: String,
    pub session_id: String,
    pub first_message_index: i64,
    pub forwarded_count: i64,
    pub is_verified: bool,
    pub backup_data: serde_json::Value,
}

#[derive(Clone)]
pub struct KeyBackupStorage {
    pub pool: Arc<PgPool>,
}

impl KeyBackupStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_backup(&self, backup: &KeyBackup) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO key_backups (user_id, backup_id, version, algorithm, auth_key, mgmt_key, backup_data, etag)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, backup_id) DO UPDATE
            SET algorithm = EXCLUDED.algorithm,
                auth_key = EXCLUDED.auth_key,
                mgmt_key = EXCLUDED.mgmt_key,
                backup_data = EXCLUDED.backup_data,
                etag = EXCLUDED.etag
            "#
        )
        .bind(&backup.user_id)
        .bind(&backup.backup_id)
        .bind(backup.version)
        .bind(&backup.algorithm)
        .bind(&backup.auth_key)
        .bind(&backup.mgmt_key)
        .bind(&backup.backup_data)
        .bind(&backup.etag)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_backup(&self, user_id: &str) -> Result<Option<KeyBackup>, ApiError> {
        let row = sqlx::query_as::<_, KeyBackup>(
            r#"
            SELECT user_id, backup_id, version, algorithm, auth_key, mgmt_key, backup_data, etag
            FROM key_backups
            WHERE user_id = $1
            ORDER BY version DESC
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_backup_version(
        &self,
        user_id: &str,
        version: &str,
    ) -> Result<Option<KeyBackup>, ApiError> {
        let version_int: i64 = version.parse().unwrap_or(0);
        let row = sqlx::query_as::<_, KeyBackup>(
            r#"
            SELECT user_id, backup_id, version, algorithm, auth_key, mgmt_key, backup_data, etag
            FROM key_backups
            WHERE user_id = $1 AND version = $2
            "#,
        )
        .bind(user_id)
        .bind(version_int)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_backup(&self, user_id: &str, version: &str) -> Result<(), ApiError> {
        let version_int: i64 = version.parse().unwrap_or(0);
        sqlx::query(
            r#"
            DELETE FROM key_backups
            WHERE user_id = $1 AND version = $2
            "#,
        )
        .bind(user_id)
        .bind(version_int)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct BackupKeyStorage {
    pool: Arc<PgPool>,
}

impl BackupKeyStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn upload_backup_key(&self, params: BackupKeyInsertParams) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO backup_keys (user_id, backup_id, room_id, session_id, first_message_index, forwarded_count, is_verified, backup_data)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, backup_id, room_id, session_id, first_message_index) DO UPDATE SET
                forwarded_count = EXCLUDED.forwarded_count,
                is_verified = EXCLUDED.is_verified,
                backup_data = EXCLUDED.backup_data
            "#
        )
        .bind(&params.user_id)
        .bind(&params.backup_id)
        .bind(&params.room_id)
        .bind(&params.session_id)
        .bind(params.first_message_index)
        .bind(params.forwarded_count)
        .bind(params.is_verified)
        .bind(&params.backup_data)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_room_backup_keys(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> Result<Vec<BackupKeyInfo>, ApiError> {
        let rows = sqlx::query_as::<_, BackupKeyInfo>(
            r#"
            SELECT user_id, backup_id, room_id, session_id, first_message_index,
                   forwarded_count, is_verified, backup_data
            FROM backup_keys
            WHERE user_id = $1 AND room_id = $2
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_room_backup_keys_by_backup_id(
        &self,
        user_id: &str,
        backup_id: &str,
        room_id: &str,
    ) -> Result<Vec<BackupKeyInfo>, ApiError> {
        let rows = sqlx::query_as::<_, BackupKeyInfo>(
            r#"
            SELECT user_id, backup_id, room_id, session_id, first_message_index,
                   forwarded_count, is_verified, backup_data
            FROM backup_keys
            WHERE user_id = $1 AND backup_id = $2 AND room_id = $3
            "#,
        )
        .bind(user_id)
        .bind(backup_id)
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_backup_key(
        &self,
        user_id: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<Option<BackupKeyInfo>, ApiError> {
        let row = sqlx::query_as::<_, BackupKeyInfo>(
            r#"
            SELECT user_id, backup_id, room_id, session_id, first_message_index,
                   forwarded_count, is_verified, backup_data
            FROM backup_keys
            WHERE user_id = $1 AND room_id = $2 AND session_id = $3
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .bind(session_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_backup_key_by_backup_id(
        &self,
        user_id: &str,
        backup_id: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<Option<BackupKeyInfo>, ApiError> {
        let row = sqlx::query_as::<_, BackupKeyInfo>(
            r#"
            SELECT user_id, backup_id, room_id, session_id, first_message_index,
                   forwarded_count, is_verified, backup_data
            FROM backup_keys
            WHERE user_id = $1 AND backup_id = $2 AND room_id = $3 AND session_id = $4
            "#,
        )
        .bind(user_id)
        .bind(backup_id)
        .bind(room_id)
        .bind(session_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_backup_key(
        &self,
        user_id: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM backup_keys
            WHERE user_id = $1 AND room_id = $2 AND session_id = $3
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .bind(session_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}
