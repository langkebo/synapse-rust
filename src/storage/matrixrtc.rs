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
    pub expires_at: Option<i64>,
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
    pub expires_at: Option<i64>,
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

    pub async fn create_session(&self, params: CreateSessionParams) -> Result<RTCSession, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as!(
            RTCSession,
            r##"INSERT INTO matrixrtc_sessions
                (room_id, session_id, application, call_id, creator, created_ts, updated_ts, is_active, config)
            VALUES ($1, $2, $3, $4, $5, $6, $7, true, $8)
            ON CONFLICT (room_id, session_id) DO UPDATE SET
                updated_ts = EXCLUDED.updated_ts,
                is_active = true,
                config = EXCLUDED.config
            RETURNING id AS "id!", room_id AS "room_id!", session_id AS "session_id!",
                application AS "application!", call_id AS "call_id?", creator AS "creator!",
                created_ts AS "created_ts!", updated_ts AS "updated_ts!",
                is_active AS "is_active!", config AS "config!"
            "##,
            &params.room_id,
            &params.session_id,
            &params.application,
            params.call_id.as_deref(),
            &params.creator,
            now,
            now,
            &params.config
        )
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_session(&self, room_id: &str, session_id: &str) -> Result<Option<RTCSession>, sqlx::Error> {
        sqlx::query_as!(
            RTCSession,
            r##"SELECT id AS "id!", room_id AS "room_id!", session_id AS "session_id!",
                application AS "application!", call_id AS "call_id?", creator AS "creator!",
                created_ts AS "created_ts!", updated_ts AS "updated_ts!",
                is_active AS "is_active!", config AS "config!"
            FROM matrixrtc_sessions
            WHERE room_id = $1 AND session_id = $2 AND is_active = true
            "##,
            room_id,
            session_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_active_sessions_for_room(&self, room_id: &str) -> Result<Vec<RTCSession>, sqlx::Error> {
        sqlx::query_as!(
            RTCSession,
            r##"SELECT id AS "id!", room_id AS "room_id!", session_id AS "session_id!",
                application AS "application!", call_id AS "call_id?", creator AS "creator!",
                created_ts AS "created_ts!", updated_ts AS "updated_ts!",
                is_active AS "is_active!", config AS "config!"
            FROM matrixrtc_sessions
            WHERE room_id = $1 AND is_active = true
            ORDER BY created_ts DESC
            "##,
            room_id
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn end_session(&self, room_id: &str, session_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query!(
            r#"
            UPDATE matrixrtc_sessions
            SET is_active = false, updated_ts = $3
            WHERE room_id = $1 AND session_id = $2
            "#,
            room_id,
            session_id,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn create_membership(&self, params: CreateMembershipParams) -> Result<RTCMembership, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = now + 3600 * 1000;

        sqlx::query_as!(
            RTCMembership,
            r##"INSERT INTO matrixrtc_memberships
                (room_id, session_id, user_id, device_id, membership_id, application, call_id,
                 created_ts, updated_ts, expires_at, foci_active, foci_preferred, application_data, is_active)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, true)
            ON CONFLICT (room_id, session_id, user_id, device_id) DO UPDATE SET
                membership_id = EXCLUDED.membership_id,
                updated_ts = EXCLUDED.updated_ts,
                expires_at = EXCLUDED.expires_at,
                foci_active = EXCLUDED.foci_active,
                foci_preferred = EXCLUDED.foci_preferred,
                application_data = EXCLUDED.application_data,
                is_active = true
            RETURNING id AS "id!", room_id AS "room_id!", session_id AS "session_id!",
                user_id AS "user_id!", device_id AS "device_id!", membership_id AS "membership_id!",
                application AS "application!", call_id AS "call_id?",
                created_ts AS "created_ts!", updated_ts AS "updated_ts!", expires_at AS "expires_at?",
                foci_active AS "foci_active?", foci_preferred AS "foci_preferred?",
                application_data AS "application_data?", is_active AS "is_active!"
            "##,
            &params.room_id,
            &params.session_id,
            &params.user_id,
            &params.device_id,
            &params.membership_id,
            &params.application,
            params.call_id.as_deref(),
            now,
            now,
            expires_at,
            params.foci_active.as_deref(),
            params.foci_preferred.as_ref(),
            params.application_data.as_ref()
        )
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_memberships_for_session(
        &self,
        room_id: &str,
        session_id: &str,
    ) -> Result<Vec<RTCMembership>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as!(
            RTCMembership,
            r##"SELECT id AS "id!", room_id AS "room_id!", session_id AS "session_id!",
                user_id AS "user_id!", device_id AS "device_id!", membership_id AS "membership_id!",
                application AS "application!", call_id AS "call_id?",
                created_ts AS "created_ts!", updated_ts AS "updated_ts!", expires_at AS "expires_at?",
                foci_active AS "foci_active?", foci_preferred AS "foci_preferred?",
                application_data AS "application_data?", is_active AS "is_active!"
            FROM matrixrtc_memberships
            WHERE room_id = $1 AND session_id = $2 AND is_active = true
              AND (expires_at IS NULL OR expires_at > $3)
            ORDER BY created_ts ASC
            "##,
            room_id,
            session_id,
            now
        )
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
        sqlx::query_as!(
            RTCMembership,
            r##"SELECT id AS "id!", room_id AS "room_id!", session_id AS "session_id!",
                user_id AS "user_id!", device_id AS "device_id!", membership_id AS "membership_id!",
                application AS "application!", call_id AS "call_id?",
                created_ts AS "created_ts!", updated_ts AS "updated_ts!", expires_at AS "expires_at?",
                foci_active AS "foci_active?", foci_preferred AS "foci_preferred?",
                application_data AS "application_data?", is_active AS "is_active!"
            FROM matrixrtc_memberships
            WHERE room_id = $1 AND session_id = $2 AND user_id = $3 AND device_id = $4
            "##,
            room_id,
            session_id,
            user_id,
            device_id
        )
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

        sqlx::query!(
            r#"
            UPDATE matrixrtc_memberships
            SET is_active = false, updated_ts = $5
            WHERE room_id = $1 AND session_id = $2 AND user_id = $3 AND device_id = $4
            "#,
            room_id,
            session_id,
            user_id,
            device_id,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn cleanup_expired_memberships(&self) -> Result<u64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query!(
            r#"
            UPDATE matrixrtc_memberships
            SET is_active = false
            WHERE is_active = true AND expires_at IS NOT NULL AND expires_at < $1
            "#,
            now
        )
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
        let expires_at = now + 24 * 3600 * 1000;

        sqlx::query_as!(
            RTCEncryptionKey,
            r##"INSERT INTO matrixrtc_encryption_keys
                (room_id, session_id, key_index, key, created_ts, expires_at, sender_user_id, sender_device_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (room_id, session_id, key_index) DO UPDATE SET
                key = EXCLUDED.key,
                created_ts = EXCLUDED.created_ts,
                expires_at = EXCLUDED.expires_at,
                sender_user_id = EXCLUDED.sender_user_id,
                sender_device_id = EXCLUDED.sender_device_id
            RETURNING id AS "id!", room_id AS "room_id!", session_id AS "session_id!",
                key_index AS "key_index!", key AS "key!", created_ts AS "created_ts!",
                expires_at AS "expires_at?", sender_user_id AS "sender_user_id!",
                sender_device_id AS "sender_device_id!"
            "##,
            room_id,
            session_id,
            key_index,
            key,
            now,
            expires_at,
            sender_user_id,
            sender_device_id
        )
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_encryption_keys(
        &self,
        room_id: &str,
        session_id: &str,
    ) -> Result<Vec<RTCEncryptionKey>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as!(
            RTCEncryptionKey,
            r##"SELECT id AS "id!", room_id AS "room_id!", session_id AS "session_id!",
                key_index AS "key_index!", key AS "key!", created_ts AS "created_ts!",
                expires_at AS "expires_at?", sender_user_id AS "sender_user_id!",
                sender_device_id AS "sender_device_id!"
            FROM matrixrtc_encryption_keys
            WHERE room_id = $1 AND session_id = $2
              AND (expires_at IS NULL OR expires_at > $3)
            ORDER BY key_index ASC
            "##,
            room_id,
            session_id,
            now
        )
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
            Ok(Some(SessionWithMemberships { session, memberships }))
        } else {
            Ok(None)
        }
    }
}
