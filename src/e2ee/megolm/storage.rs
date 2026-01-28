use super::models::*;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use chrono::Utc;
use crate::error::ApiError;

pub struct MegolmSessionStorage<'a> {
    pool: &'a PgPool,
}

impl<'a> MegolmSessionStorage<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_session(&self, session: &MegolmSession) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO megolm_sessions (id, session_id, room_id, sender_key, session_key, algorithm, message_index, created_at, last_used_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#
        )
        .bind(session.id)
        .bind(&session.session_id)
        .bind(&session.room_id)
        .bind(&session.sender_key)
        .bind(&session.session_key)
        .bind(&session.algorithm)
        .bind(session.message_index)
        .bind(session.created_at)
        .bind(session.last_used_at)
        .bind(session.expires_at)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_session(&self, session_id: &str) -> Result<Option<MegolmSession>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT id, session_id, room_id, sender_key, session_key, algorithm, message_index, created_at, last_used_at, expires_at
            FROM megolm_sessions
            WHERE session_id = $1
            "#
        )
        .bind(session_id)
        .fetch_optional(self.pool)
        .await?;
        
        Ok(row.map(|row| MegolmSession {
            id: row.get("id"),
            session_id: row.get("session_id"),
            room_id: row.get("room_id"),
            sender_key: row.get("sender_key"),
            session_key: row.get("session_key"),
            algorithm: row.get("algorithm"),
            message_index: row.get("message_index"),
            created_at: row.get("created_at"),
            last_used_at: row.get("last_used_at"),
            expires_at: row.get("expires_at"),
        }))
    }
    
    pub async fn get_room_sessions(&self, room_id: &str) -> Result<Vec<MegolmSession>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT id, session_id, room_id, sender_key, session_key, algorithm, message_index, created_at, last_used_at, expires_at
            FROM megolm_sessions
            WHERE room_id = $1
            "#
        )
        .bind(room_id)
        .fetch_all(self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|row| MegolmSession {
            id: row.get("id"),
            session_id: row.get("session_id"),
            room_id: row.get("room_id"),
            sender_key: row.get("sender_key"),
            session_key: row.get("session_key"),
            algorithm: row.get("algorithm"),
            message_index: row.get("message_index"),
            created_at: row.get("created_at"),
            last_used_at: row.get("last_used_at"),
            expires_at: row.get("expires_at"),
        }).collect())
    }
    
    pub async fn update_session(&self, session: &MegolmSession) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            UPDATE megolm_sessions
            SET session_key = $2, message_index = $3, last_used_at = $4, expires_at = $5
            WHERE session_id = $1
            "#
        )
        .bind(&session.session_id)
        .bind(&session.session_key)
        .bind(session.message_index)
        .bind(session.last_used_at)
        .bind(session.expires_at)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn delete_session(&self, session_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM megolm_sessions
            WHERE session_id = $1
            "#
        )
        .bind(session_id)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
}