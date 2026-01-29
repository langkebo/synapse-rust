use super::models::*;
use crate::error::ApiError;
use chrono::Utc;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct KeyBackupStorage {
    pub pool: Arc<PgPool>,
}

impl KeyBackupStorage {
    /// Creates a new `KeyBackupStorage` instance.
    ///
    /// # Arguments
    ///
    /// * `pool` - A reference to the PostgreSQL connection pool
    ///
    /// # Returns
    ///
    /// A new `KeyBackupStorage` instance with a cloned reference to the pool
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// Creates or updates a key backup in the database.
    ///
    /// This method performs an upsert operation - if a backup with the same
    /// user_id and version exists, it will be updated; otherwise, a new backup
    /// will be created.
    ///
    /// # Arguments
    ///
    /// * `backup` - A reference to the `KeyBackup` to create or update
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an `ApiError` if the operation fails
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
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Retrieves the most recent backup for a given user.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The Matrix user ID (e.g., "@user:example.com")
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(KeyBackup))` if a backup exists, `Ok(None)` if not,
    /// or an `ApiError` if the operation fails
    pub async fn get_backup(&self, user_id: &str) -> Result<Option<KeyBackup>, ApiError> {
        let row = sqlx::query_as::<_, KeyBackup>(
            r#"
            SELECT id, user_id, version, algorithm, auth_data, encrypted_data, created_at, updated_at
            FROM key_backups
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#
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
        let row = sqlx::query_as::<_, KeyBackup>(
            r#"
            SELECT id, user_id, version, algorithm, auth_data, encrypted_data, created_at, updated_at
            FROM key_backups
            WHERE user_id = $1 AND version = $2
            "#
        )
        .bind(user_id)
        .bind(version)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_backup(&self, user_id: &str, version: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM key_backups
            WHERE user_id = $1 AND version = $2
            "#,
        )
        .bind(user_id)
        .bind(version)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_backup(&self, backup: &KeyBackup) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            UPDATE key_backups
            SET encrypted_data = $2, updated_at = $3
            WHERE user_id = $1 AND version = $4
            "#,
        )
        .bind(&backup.user_id)
        .bind(&backup.encrypted_data)
        .bind(backup.updated_at)
        .bind(&backup.version)
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

    pub async fn upload_backup_key(
        &self,
        backup_id: &Uuid,
        room_id: &str,
        session_id: &str,
        first_message_index: i64,
        forwarded_count: i64,
        is_verified: bool,
        session_data: &str,
    ) -> Result<(), ApiError> {
        let key_id = uuid::Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO backup_keys (id, backup_id, room_id, session_id, first_message_index, forwarded_count, is_verified, session_data, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (backup_id, room_id, session_id) DO UPDATE SET
                first_message_index = EXCLUDED.first_message_index,
                forwarded_count = EXCLUDED.forwarded_count,
                is_verified = EXCLUDED.is_verified,
                session_data = EXCLUDED.session_data
            "#
        )
        .bind(key_id)
        .bind(backup_id)
        .bind(room_id)
        .bind(session_id)
        .bind(first_message_index)
        .bind(forwarded_count)
        .bind(is_verified)
        .bind(session_data)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn upload_backup_keys_batch(
        &self,
        backup_id: &Uuid,
        room_id: &str,
        keys: Vec<BackupKeyUpload>,
    ) -> Result<(), ApiError> {
        let now = Utc::now();

        for key in keys {
            let key_id = uuid::Uuid::new_v4();
            sqlx::query(
                r#"
                INSERT INTO backup_keys (id, backup_id, room_id, session_id, first_message_index, forwarded_count, is_verified, session_data, created_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                ON CONFLICT (backup_id, room_id, session_id) DO UPDATE SET
                    first_message_index = EXCLUDED.first_message_index,
                    forwarded_count = EXCLUDED.forwarded_count,
                    is_verified = EXCLUDED.is_verified,
                    session_data = EXCLUDED.session_data
                "#
            )
            .bind(key_id)
            .bind(backup_id)
            .bind(room_id)
            .bind(&key.session_id)
            .bind(key.first_message_index)
            .bind(key.forwarded_count)
            .bind(key.is_verified)
            .bind(&key.session_data)
            .bind(now)
            .execute(&*self.pool)
            .await?;
        }

        Ok(())
    }

    pub async fn get_room_backup_keys(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> Result<Vec<BackupKeyInfo>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT bk.id, bk.backup_id, bk.room_id, bk.session_id, bk.first_message_index,
                   bk.forwarded_count, bk.is_verified, bk.session_data, bk.created_at
            FROM backup_keys bk
            INNER JOIN key_backups kb ON bk.backup_id = kb.id
            WHERE kb.user_id = $1 AND bk.room_id = $2
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| BackupKeyInfo {
                id: row.get("id"),
                backup_id: row.get("backup_id"),
                room_id: row.get("room_id"),
                session_id: row.get("session_id"),
                first_message_index: row.get("first_message_index"),
                forwarded_count: row.get("forwarded_count"),
                is_verified: row.get("is_verified"),
                session_data: row.get("session_data"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    pub async fn get_backup_key(
        &self,
        user_id: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<Option<BackupKeyInfo>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT bk.id, bk.backup_id, bk.room_id, bk.session_id, bk.first_message_index,
                   bk.forwarded_count, bk.is_verified, bk.session_data, bk.created_at
            FROM backup_keys bk
            INNER JOIN key_backups kb ON bk.backup_id = kb.id
            WHERE kb.user_id = $1 AND bk.room_id = $2 AND bk.session_id = $3
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .bind(session_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|row| BackupKeyInfo {
            id: row.get("id"),
            backup_id: row.get("backup_id"),
            room_id: row.get("room_id"),
            session_id: row.get("session_id"),
            first_message_index: row.get("first_message_index"),
            forwarded_count: row.get("forwarded_count"),
            is_verified: row.get("is_verified"),
            session_data: row.get("session_data"),
            created_at: row.get("created_at"),
        }))
    }

    pub async fn delete_backup_key(
        &self,
        user_id: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM backup_keys bk
            USING key_backups kb
            WHERE bk.backup_id = kb.id AND kb.user_id = $1 AND bk.room_id = $2 AND bk.session_id = $3
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
