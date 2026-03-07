use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RTCSession {
    pub id: i64,
    pub room_id: String,
    pub session_id: String,
    pub application: String,
    pub call_id: Option<String>,
    pub creator: String,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub is_active: bool,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RTCMembership {
    pub id: i64,
    pub room_id: String,
    pub session_id: String,
    pub user_id: String,
    pub device_id: String,
    pub membership_id: String,
    pub application: String,
    pub call_id: Option<String>,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub expires_ts: Option<i64>,
    pub foci_active: Option<String>,
    pub foci_preferred: Option<serde_json::Value>,
    pub application_data: Option<serde_json::Value>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RTCEncryptionKey {
    pub id: i64,
    pub room_id: String,
    pub session_id: String,
    pub key_index: i32,
    pub key: String,
    pub created_ts: i64,
    pub expires_ts: Option<i64>,
    pub sender_user_id: String,
    pub sender_device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionParams {
    pub room_id: String,
    pub session_id: String,
    pub application: String,
    pub call_id: Option<String>,
    pub creator: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMembershipParams {
    pub room_id: String,
    pub session_id: String,
    pub user_id: String,
    pub device_id: String,
    pub membership_id: String,
    pub application: String,
    pub call_id: Option<String>,
    pub foci_active: Option<String>,
    pub foci_preferred: Option<serde_json::Value>,
    pub application_data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionWithMemberships {
    pub session: RTCSession,
    pub memberships: Vec<RTCMembership>,
}

#[derive(Clone)]
pub struct MatrixRTCStorage {
    pool: Arc<Pool<Postgres>>,
}

impl MatrixRTCStorage {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    pub async fn create_session(
        &self,
        params: CreateSessionParams,
    ) -> Result<RTCSession, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as::<_, RTCSession>(
            r#"
            INSERT INTO matrixrtc_sessions 
                (room_id, session_id, application, call_id, creator, created_ts, updated_ts, is_active, config)
            VALUES ($1, $2, $3, $4, $5, $6, $7, true, $8)
            ON CONFLICT (room_id, session_id) DO UPDATE SET
                updated_ts = EXCLUDED.updated_ts,
                is_active = true,
                config = EXCLUDED.config
            RETURNING *
            "#,
        )
        .bind(&params.room_id)
        .bind(&params.session_id)
        .bind(&params.application)
        .bind(&params.call_id)
        .bind(&params.creator)
        .bind(now)
        .bind(now)
        .bind(&params.config)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_session(
        &self,
        room_id: &str,
        session_id: &str,
    ) -> Result<Option<RTCSession>, sqlx::Error> {
        sqlx::query_as::<_, RTCSession>(
            r#"
            SELECT * FROM matrixrtc_sessions 
            WHERE room_id = $1 AND session_id = $2 AND is_active = true
            "#,
        )
        .bind(room_id)
        .bind(session_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_active_sessions_for_room(
        &self,
        room_id: &str,
    ) -> Result<Vec<RTCSession>, sqlx::Error> {
        sqlx::query_as::<_, RTCSession>(
            r#"
            SELECT * FROM matrixrtc_sessions 
            WHERE room_id = $1 AND is_active = true
            ORDER BY created_ts DESC
            "#,
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn end_session(
        &self,
        room_id: &str,
        session_id: &str,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE matrixrtc_sessions 
            SET is_active = false, updated_ts = $3
            WHERE room_id = $1 AND session_id = $2
            "#,
        )
        .bind(room_id)
        .bind(session_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn create_membership(
        &self,
        params: CreateMembershipParams,
    ) -> Result<RTCMembership, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let expires_ts = now + 3600 * 1000;

        sqlx::query_as::<_, RTCMembership>(
            r#"
            INSERT INTO matrixrtc_memberships 
                (room_id, session_id, user_id, device_id, membership_id, application, call_id,
                 created_ts, updated_ts, expires_ts, foci_active, foci_preferred, application_data, is_active)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, true)
            ON CONFLICT (room_id, session_id, user_id, device_id) DO UPDATE SET
                membership_id = EXCLUDED.membership_id,
                updated_ts = EXCLUDED.updated_ts,
                expires_ts = EXCLUDED.expires_ts,
                foci_active = EXCLUDED.foci_active,
                foci_preferred = EXCLUDED.foci_preferred,
                application_data = EXCLUDED.application_data,
                is_active = true
            RETURNING *
            "#,
        )
        .bind(&params.room_id)
        .bind(&params.session_id)
        .bind(&params.user_id)
        .bind(&params.device_id)
        .bind(&params.membership_id)
        .bind(&params.application)
        .bind(&params.call_id)
        .bind(now)
        .bind(now)
        .bind(expires_ts)
        .bind(&params.foci_active)
        .bind(&params.foci_preferred)
        .bind(&params.application_data)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_memberships_for_session(
        &self,
        room_id: &str,
        session_id: &str,
    ) -> Result<Vec<RTCMembership>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as::<_, RTCMembership>(
            r#"
            SELECT * FROM matrixrtc_memberships 
            WHERE room_id = $1 AND session_id = $2 AND is_active = true 
              AND (expires_ts IS NULL OR expires_ts > $3)
            ORDER BY created_ts ASC
            "#,
        )
        .bind(room_id)
        .bind(session_id)
        .bind(now)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_user_membership(
        &self,
        room_id: &str,
        session_id: &str,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<RTCMembership>, sqlx::Error> {
        sqlx::query_as::<_, RTCMembership>(
            r#"
            SELECT * FROM matrixrtc_memberships 
            WHERE room_id = $1 AND session_id = $2 AND user_id = $3 AND device_id = $4
            "#,
        )
        .bind(room_id)
        .bind(session_id)
        .bind(user_id)
        .bind(device_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn end_membership(
        &self,
        room_id: &str,
        session_id: &str,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE matrixrtc_memberships 
            SET is_active = false, updated_ts = $5
            WHERE room_id = $1 AND session_id = $2 AND user_id = $3 AND device_id = $4
            "#,
        )
        .bind(room_id)
        .bind(session_id)
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn cleanup_expired_memberships(&self) -> Result<u64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r#"
            UPDATE matrixrtc_memberships 
            SET is_active = false
            WHERE is_active = true AND expires_ts IS NOT NULL AND expires_ts < $1
            "#,
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn store_encryption_key(
        &self,
        room_id: &str,
        session_id: &str,
        key_index: i32,
        key: &str,
        sender_user_id: &str,
        sender_device_id: &str,
    ) -> Result<RTCEncryptionKey, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let expires_ts = now + 24 * 3600 * 1000;

        sqlx::query_as::<_, RTCEncryptionKey>(
            r#"
            INSERT INTO matrixrtc_encryption_keys 
                (room_id, session_id, key_index, key, created_ts, expires_ts, sender_user_id, sender_device_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (room_id, session_id, key_index) DO UPDATE SET
                key = EXCLUDED.key,
                created_ts = EXCLUDED.created_ts,
                expires_ts = EXCLUDED.expires_ts,
                sender_user_id = EXCLUDED.sender_user_id,
                sender_device_id = EXCLUDED.sender_device_id
            RETURNING *
            "#,
        )
        .bind(room_id)
        .bind(session_id)
        .bind(key_index)
        .bind(key)
        .bind(now)
        .bind(expires_ts)
        .bind(sender_user_id)
        .bind(sender_device_id)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_encryption_keys(
        &self,
        room_id: &str,
        session_id: &str,
    ) -> Result<Vec<RTCEncryptionKey>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as::<_, RTCEncryptionKey>(
            r#"
            SELECT * FROM matrixrtc_encryption_keys 
            WHERE room_id = $1 AND session_id = $2 
              AND (expires_ts IS NULL OR expires_ts > $3)
            ORDER BY key_index ASC
            "#,
        )
        .bind(room_id)
        .bind(session_id)
        .bind(now)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_session_with_memberships(
        &self,
        room_id: &str,
        session_id: &str,
    ) -> Result<Option<SessionWithMemberships>, sqlx::Error> {
        let session = self.get_session(room_id, session_id).await?;

        if let Some(session) = session {
            let memberships = self.get_memberships_for_session(room_id, session_id).await?;
            Ok(Some(SessionWithMemberships {
                session,
                memberships,
            }))
        } else {
            Ok(None)
        }
    }
}
