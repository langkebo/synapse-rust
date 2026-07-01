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

    pub async fn get_session(&self, room_id: &str, session_id: &str) -> Result<Option<RTCSession>, sqlx::Error> {
        sqlx::query_as::<_, RTCSession>(
            r#"
            SELECT id, room_id, session_id, application, call_id, creator, created_ts, updated_ts, is_active, config FROM matrixrtc_sessions
            WHERE room_id = $1 AND session_id = $2 AND is_active = true
            "#,
        )
        .bind(room_id)
        .bind(session_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_active_sessions_for_room(&self, room_id: &str) -> Result<Vec<RTCSession>, sqlx::Error> {
        sqlx::query_as::<_, RTCSession>(
            r#"
            SELECT id, room_id, session_id, application, call_id, creator, created_ts, updated_ts, is_active, config FROM matrixrtc_sessions
            WHERE room_id = $1 AND is_active = true
            ORDER BY created_ts DESC
            "#,
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn end_session(&self, room_id: &str, session_id: &str) -> Result<(), sqlx::Error> {
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

    pub async fn create_membership(&self, params: CreateMembershipParams) -> Result<RTCMembership, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = now + 3600 * 1000;

        sqlx::query_as::<_, RTCMembership>(
            r#"
            INSERT INTO matrixrtc_memberships
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
        .bind(expires_at)
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
            SELECT id, room_id, session_id, user_id, device_id, membership_id, application, call_id, created_ts, updated_ts, expires_at, foci_active, foci_preferred, application_data, is_active FROM matrixrtc_memberships
            WHERE room_id = $1 AND session_id = $2 AND is_active = true
              AND (expires_at IS NULL OR expires_at > $3)
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
            SELECT id, room_id, session_id, user_id, device_id, membership_id, application, call_id, created_ts, updated_ts, expires_at, foci_active, foci_preferred, application_data, is_active FROM matrixrtc_memberships
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
            WHERE is_active = true AND expires_at IS NOT NULL AND expires_at < $1
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
        let expires_at = now + 24 * 3600 * 1000;

        sqlx::query_as::<_, RTCEncryptionKey>(
            r#"
            INSERT INTO matrixrtc_encryption_keys
                (room_id, session_id, key_index, key, created_ts, expires_at, sender_user_id, sender_device_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (room_id, session_id, key_index) DO UPDATE SET
                key = EXCLUDED.key,
                created_ts = EXCLUDED.created_ts,
                expires_at = EXCLUDED.expires_at,
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
        .bind(expires_at)
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
            SELECT id, room_id, session_id, key_index, key, created_ts, expires_at, sender_user_id, sender_device_id FROM matrixrtc_encryption_keys
            WHERE room_id = $1 AND session_id = $2
              AND (expires_at IS NULL OR expires_at > $3)
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
            Ok(Some(SessionWithMemberships { session, memberships }))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    fn sample_config() -> serde_json::Value {
        serde_json::json!({
            "focus": "audio",
            "features": ["hd_audio", "noise_suppression"]
        })
    }

    // -- RTCSession --

    #[test]
    fn test_rtc_session_construction() {
        let session = RTCSession {
            id: 1,
            room_id: "!room:example.com".to_string(),
            session_id: "session-123".to_string(),
            application: "m.call".to_string(),
            call_id: Some("call-abc".to_string()),
            creator: "@alice:example.com".to_string(),
            created_ts: 1_700_000_000_000,
            updated_ts: 1_700_000_000_500,
            is_active: true,
            config: sample_config(),
        };
        assert_eq!(session.id, 1);
        assert_eq!(session.room_id, "!room:example.com");
        assert_eq!(session.session_id, "session-123");
        assert_eq!(session.application, "m.call");
        assert_eq!(session.call_id.as_deref(), Some("call-abc"));
        assert_eq!(session.creator, "@alice:example.com");
        assert!(session.is_active);
        assert_eq!(session.config["focus"], "audio");
    }

    #[test]
    fn test_rtc_session_serde_roundtrip() {
        let session = RTCSession {
            id: 42,
            room_id: "!test:srv".to_string(),
            session_id: "sess-xyz".to_string(),
            application: "m.call".to_string(),
            call_id: None,
            creator: "@bob:srv".to_string(),
            created_ts: 1,
            updated_ts: 2,
            is_active: false,
            config: serde_json::json!({}),
        };
        let json = serde_json::to_string(&session).expect("serialize RTCSession");
        let restored: RTCSession = serde_json::from_str(&json).expect("deserialize RTCSession");
        assert_eq!(restored.id, 42);
        assert_eq!(restored.room_id, session.room_id);
        assert_eq!(restored.session_id, session.session_id);
        assert!(restored.call_id.is_none());
        assert!(!restored.is_active);
    }

    #[test]
    fn test_rtc_session_optional_call_id() {
        let mut session = RTCSession {
            id: 1,
            room_id: "!r:s".to_string(),
            session_id: "s".to_string(),
            application: "m.call".to_string(),
            call_id: None,
            creator: "@a:s".to_string(),
            created_ts: 0,
            updated_ts: 0,
            is_active: true,
            config: serde_json::json!({}),
        };
        assert!(session.call_id.is_none());
        session.call_id = Some("call-1".to_string());
        assert_eq!(session.call_id.as_deref(), Some("call-1"));
    }

    // -- RTCMembership --

    #[test]
    fn test_rtc_membership_construction() {
        let membership = RTCMembership {
            id: 10,
            room_id: "!room:example.com".to_string(),
            session_id: "session-123".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE_A".to_string(),
            membership_id: "m-1".to_string(),
            application: "m.call".to_string(),
            call_id: Some("call-abc".to_string()),
            created_ts: 1_700_000_000_000,
            updated_ts: 1_700_000_000_000,
            expires_at: Some(1_700_000_003_600),
            foci_active: Some("livekit".to_string()),
            foci_preferred: Some(serde_json::json!(["livekit"])),
            application_data: Some(serde_json::json!({"muted": false})),
            is_active: true,
        };
        assert_eq!(membership.id, 10);
        assert_eq!(membership.membership_id, "m-1");
        assert_eq!(membership.device_id, "DEVICE_A");
        assert_eq!(membership.foci_active.as_deref(), Some("livekit"));
        assert!(membership.expires_at.is_some());
        assert!(membership.is_active);
    }

    #[test]
    fn test_rtc_membership_serde_roundtrip() {
        let membership = RTCMembership {
            id: 1,
            room_id: "!r:s".to_string(),
            session_id: "s".to_string(),
            user_id: "@u:s".to_string(),
            device_id: "d".to_string(),
            membership_id: "m".to_string(),
            application: "m.call".to_string(),
            call_id: None,
            created_ts: 1,
            updated_ts: 2,
            expires_at: None,
            foci_active: None,
            foci_preferred: None,
            application_data: None,
            is_active: true,
        };
        let json = serde_json::to_string(&membership).expect("serialize RTCMembership");
        let restored: RTCMembership = serde_json::from_str(&json).expect("deserialize RTCMembership");
        assert_eq!(restored.id, 1);
        assert_eq!(restored.membership_id, membership.membership_id);
        assert!(restored.expires_at.is_none());
        assert!(restored.foci_preferred.is_none());
        assert!(restored.application_data.is_none());
    }

    // -- RTCEncryptionKey --

    #[test]
    fn test_rtc_encryption_key_construction() {
        let key = RTCEncryptionKey {
            id: 1,
            room_id: "!room:example.com".to_string(),
            session_id: "session-123".to_string(),
            key_index: 0,
            key: "base64keydata".to_string(),
            created_ts: now_ms(),
            expires_at: Some(now_ms() + 86_400_000),
            sender_user_id: "@alice:example.com".to_string(),
            sender_device_id: "DEVICE_A".to_string(),
        };
        assert_eq!(key.key_index, 0);
        assert_eq!(key.key, "base64keydata");
        assert_eq!(key.sender_device_id, "DEVICE_A");
        assert!(key.expires_at.unwrap() > key.created_ts);
    }

    #[test]
    fn test_rtc_encryption_key_serde_roundtrip() {
        let key = RTCEncryptionKey {
            id: 7,
            room_id: "!r:s".to_string(),
            session_id: "s".to_string(),
            key_index: 3,
            key: "k".to_string(),
            created_ts: 100,
            expires_at: None,
            sender_user_id: "@u:s".to_string(),
            sender_device_id: "d".to_string(),
        };
        let json = serde_json::to_string(&key).expect("serialize RTCEncryptionKey");
        let restored: RTCEncryptionKey = serde_json::from_str(&json).expect("deserialize RTCEncryptionKey");
        assert_eq!(restored.id, 7);
        assert_eq!(restored.key_index, 3);
        assert_eq!(restored.key, "k");
        assert!(restored.expires_at.is_none());
    }

    // -- CreateSessionParams --

    #[test]
    fn test_create_session_params_construction() {
        let params = CreateSessionParams {
            room_id: "!room:example.com".to_string(),
            session_id: "session-123".to_string(),
            application: "m.call".to_string(),
            call_id: Some("call-abc".to_string()),
            creator: "@alice:example.com".to_string(),
            config: sample_config(),
        };
        assert_eq!(params.room_id, "!room:example.com");
        assert_eq!(params.session_id, "session-123");
        assert_eq!(params.call_id.as_deref(), Some("call-abc"));
        assert_eq!(params.config["focus"], "audio");
    }

    #[test]
    fn test_create_session_params_serde_roundtrip() {
        let params = CreateSessionParams {
            room_id: "!r:s".to_string(),
            session_id: "s".to_string(),
            application: "m.call".to_string(),
            call_id: None,
            creator: "@u:s".to_string(),
            config: serde_json::json!({}),
        };
        let json = serde_json::to_string(&params).expect("serialize CreateSessionParams");
        let restored: CreateSessionParams = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.room_id, params.room_id);
        assert_eq!(restored.application, params.application);
        assert!(restored.call_id.is_none());
    }

    // -- CreateMembershipParams --

    #[test]
    fn test_create_membership_params_construction() {
        let params = CreateMembershipParams {
            room_id: "!room:example.com".to_string(),
            session_id: "session-123".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE_A".to_string(),
            membership_id: "m-1".to_string(),
            application: "m.call".to_string(),
            call_id: Some("call-abc".to_string()),
            foci_active: Some("livekit".to_string()),
            foci_preferred: Some(serde_json::json!(["livekit"])),
            application_data: Some(serde_json::json!({"muted": false})),
        };
        assert_eq!(params.membership_id, "m-1");
        assert_eq!(params.device_id, "DEVICE_A");
        assert_eq!(params.foci_active.as_deref(), Some("livekit"));
        assert!(params.application_data.is_some());
    }

    #[test]
    fn test_create_membership_params_serde_roundtrip() {
        let params = CreateMembershipParams {
            room_id: "!r:s".to_string(),
            session_id: "s".to_string(),
            user_id: "@u:s".to_string(),
            device_id: "d".to_string(),
            membership_id: "m".to_string(),
            application: "m.call".to_string(),
            call_id: None,
            foci_active: None,
            foci_preferred: None,
            application_data: None,
        };
        let json = serde_json::to_string(&params).expect("serialize CreateMembershipParams");
        let restored: CreateMembershipParams = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.membership_id, params.membership_id);
        assert!(restored.call_id.is_none());
        assert!(restored.foci_active.is_none());
    }

    // -- SessionWithMemberships aggregate --

    #[test]
    fn test_session_with_memberships_construction() {
        let session = RTCSession {
            id: 1,
            room_id: "!r:s".to_string(),
            session_id: "s".to_string(),
            application: "m.call".to_string(),
            call_id: None,
            creator: "@u:s".to_string(),
            created_ts: 0,
            updated_ts: 0,
            is_active: true,
            config: serde_json::json!({}),
        };
        let membership = RTCMembership {
            id: 1,
            room_id: "!r:s".to_string(),
            session_id: "s".to_string(),
            user_id: "@u:s".to_string(),
            device_id: "d".to_string(),
            membership_id: "m".to_string(),
            application: "m.call".to_string(),
            call_id: None,
            created_ts: 0,
            updated_ts: 0,
            expires_at: None,
            foci_active: None,
            foci_preferred: None,
            application_data: None,
            is_active: true,
        };
        let aggregate = SessionWithMemberships { session: session.clone(), memberships: vec![membership.clone()] };
        assert_eq!(aggregate.session.id, 1);
        assert_eq!(aggregate.memberships.len(), 1);
        assert_eq!(aggregate.memberships[0].device_id, "d");
    }

    #[test]
    fn test_session_with_memberships_serde_roundtrip() {
        let session = RTCSession {
            id: 99,
            room_id: "!r:s".to_string(),
            session_id: "s".to_string(),
            application: "m.call".to_string(),
            call_id: None,
            creator: "@u:s".to_string(),
            created_ts: 0,
            updated_ts: 0,
            is_active: true,
            config: serde_json::json!({}),
        };
        let aggregate = SessionWithMemberships { session, memberships: vec![] };
        let json = serde_json::to_string(&aggregate).expect("serialize SessionWithMemberships");
        let restored: SessionWithMemberships = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.session.id, 99);
        assert!(restored.memberships.is_empty());
    }

    // -- MatrixRTCStorage construction (no DB) --

    #[test]
    fn test_storage_clone_preserves_pool_pointer() {
        // We can't construct a real pool without DB, but we can assert the
        // type is Clone (compile-time check via trait bound already present).
        fn assert_clone<T: Clone>() {}
        assert_clone::<MatrixRTCStorage>();
    }
}
