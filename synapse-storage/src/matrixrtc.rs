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
mod db_tests {
    use super::*;
    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
    use std::env;

    async fn test_pool() -> Arc<Pool<Postgres>> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    /// Clean up test data in all three matrixRTC tables for a given room_id suffix.
    async fn cleanup_matrixrtc_data(pool: &Pool<Postgres>, suffix: &str) {
        let _ = sqlx::query(
            "DELETE FROM matrixrtc_encryption_keys WHERE room_id LIKE $1",
        )
        .bind(format!("%{suffix}"))
        .execute(pool)
        .await;
        let _ = sqlx::query(
            "DELETE FROM matrixrtc_memberships WHERE room_id LIKE $1",
        )
        .bind(format!("%{suffix}"))
        .execute(pool)
        .await;
        let _ = sqlx::query(
            "DELETE FROM matrixrtc_sessions WHERE room_id LIKE $1",
        )
        .bind(format!("%{suffix}"))
        .execute(pool)
        .await;
    }

    // --- Session tests ---

    #[tokio::test]
    async fn test_create_session_returns_valid_record() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_matrixrtc_data(&pool, &suffix).await;

        let storage = MatrixRTCStorage::new(pool.clone());
        let room_id = format!("!room_{suffix}:example.com");
        let session_id = format!("session_{suffix}");

        let params = CreateSessionParams {
            room_id: room_id.clone(),
            session_id: session_id.clone(),
            application: "m.call".to_string(),
            call_id: Some(format!("call_{suffix}")),
            creator: format!("@user_{suffix}:example.com"),
            config: json!({"offer": "sdp", "type": "offer"}),
        };

        let session = storage
            .create_session(params)
            .await
            .expect("create_session should succeed");

        assert!(session.id > 0);
        assert_eq!(session.room_id, room_id);
        assert_eq!(session.session_id, session_id);
        assert_eq!(session.application, "m.call");
        assert_eq!(session.call_id, Some(format!("call_{suffix}")));
        assert_eq!(session.creator, format!("@user_{suffix}:example.com"));
        assert!(session.is_active);
        assert!(session.created_ts > 0);
        assert_eq!(session.updated_ts, session.created_ts);

        cleanup_matrixrtc_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_create_session_upsert_updates_existing() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_matrixrtc_data(&pool, &suffix).await;

        let storage = MatrixRTCStorage::new(pool.clone());
        let room_id = format!("!room_{suffix}:example.com");
        let session_id = format!("session_{suffix}");

        // First create
        let params1 = CreateSessionParams {
            room_id: room_id.clone(),
            session_id: session_id.clone(),
            application: "m.call".to_string(),
            call_id: Some(format!("call1_{suffix}")),
            creator: format!("@user_{suffix}:example.com"),
            config: json!({"type": "offer", "sdp": "v1"}),
        };
        let session1 = storage.create_session(params1).await.expect("first create");
        let id1 = session1.id;
        assert_eq!(session1.config, json!({"type": "offer", "sdp": "v1"}));

        // Second create with same room_id + session_id should UPSERT
        let params2 = CreateSessionParams {
            room_id: room_id.clone(),
            session_id: session_id.clone(),
            application: "m.call".to_string(),
            call_id: Some(format!("call2_{suffix}")),
            creator: format!("@user2_{suffix}:example.com"),
            config: json!({"type": "answer", "sdp": "v2"}),
        };
        let session2 = storage.create_session(params2).await.expect("upsert create");

        // Should be same row (same id), but updated config
        assert_eq!(session2.id, id1, "upsert should update the same row");
        assert_eq!(session2.config, json!({"type": "answer", "sdp": "v2"}));
        assert!(session2.is_active);
        assert!(session2.updated_ts >= session2.created_ts);

        cleanup_matrixrtc_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_session_found() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_matrixrtc_data(&pool, &suffix).await;

        let storage = MatrixRTCStorage::new(pool.clone());
        let room_id = format!("!room_{suffix}:example.com");
        let session_id = format!("session_{suffix}");

        storage
            .create_session(CreateSessionParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                application: "m.call".to_string(),
                call_id: None,
                creator: format!("@alice_{suffix}:example.com"),
                config: json!({}),
            })
            .await
            .unwrap();

        let found = storage
            .get_session(&room_id, &session_id)
            .await
            .expect("get_session query should succeed")
            .expect("session should be found");

        assert_eq!(found.room_id, room_id);
        assert_eq!(found.session_id, session_id);
        assert!(found.is_active);

        cleanup_matrixrtc_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_session_not_found() {
        let pool = test_pool().await;
        let storage = MatrixRTCStorage::new(pool.clone());

        let result = storage
            .get_session("!nonexistent:example.com", "no_session")
            .await
            .expect("get_session query should succeed");

        assert!(result.is_none(), "nonexistent session should return None");
    }

    #[tokio::test]
    async fn test_get_active_sessions_for_room() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_matrixrtc_data(&pool, &suffix).await;

        let storage = MatrixRTCStorage::new(pool.clone());
        let room_id = format!("!room_{suffix}:example.com");

        // Create two active sessions
        let session_id1 = format!("session1_{suffix}");
        let session_id2 = format!("session2_{suffix}");

        storage
            .create_session(CreateSessionParams {
                room_id: room_id.clone(),
                session_id: session_id1.clone(),
                application: "m.call".to_string(),
                call_id: Some(format!("call1_{suffix}")),
                creator: format!("@alice_{suffix}:example.com"),
                config: json!({}),
            })
            .await
            .unwrap();

        storage
            .create_session(CreateSessionParams {
                room_id: room_id.clone(),
                session_id: session_id2.clone(),
                application: "m.call".to_string(),
                call_id: Some(format!("call2_{suffix}")),
                creator: format!("@bob_{suffix}:example.com"),
                config: json!({}),
            })
            .await
            .unwrap();

        let sessions = storage
            .get_active_sessions_for_room(&room_id)
            .await
            .expect("get_active_sessions_for_room should succeed");

        assert_eq!(sessions.len(), 2);
        // Second created should come first (ORDER BY created_ts DESC)
        assert_eq!(sessions[0].session_id, session_id2);
        assert_eq!(sessions[1].session_id, session_id1);
        assert!(sessions.iter().all(|s| s.is_active));

        // End one session and verify it is excluded
        storage
            .end_session(&room_id, &session_id1)
            .await
            .expect("end_session should succeed");

        let sessions_after = storage
            .get_active_sessions_for_room(&room_id)
            .await
            .expect("get_active_sessions_for_room should succeed");

        assert_eq!(sessions_after.len(), 1);
        assert_eq!(sessions_after[0].session_id, session_id2);

        cleanup_matrixrtc_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_end_session() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_matrixrtc_data(&pool, &suffix).await;

        let storage = MatrixRTCStorage::new(pool.clone());
        let room_id = format!("!room_{suffix}:example.com");
        let session_id = format!("session_{suffix}");

        storage
            .create_session(CreateSessionParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                application: "m.call".to_string(),
                call_id: None,
                creator: format!("@alice_{suffix}:example.com"),
                config: json!({}),
            })
            .await
            .unwrap();

        storage
            .end_session(&room_id, &session_id)
            .await
            .expect("end_session should succeed");

        // After ending, get_session should return None (filters on is_active=true)
        let result = storage
            .get_session(&room_id, &session_id)
            .await
            .expect("get_session query should succeed");

        assert!(result.is_none(), "ended session should not be returned by get_session");

        cleanup_matrixrtc_data(&pool, &suffix).await;
    }

    // --- Membership tests ---

    #[tokio::test]
    async fn test_create_membership_returns_valid_record() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_matrixrtc_data(&pool, &suffix).await;

        let storage = MatrixRTCStorage::new(pool.clone());
        let room_id = format!("!room_{suffix}:example.com");
        let session_id = format!("session_{suffix}");
        let user_id = format!("@user_{suffix}:example.com");
        let device_id = format!("device_{suffix}");

        // Must create session first (FK constraint)
        storage
            .create_session(CreateSessionParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                application: "m.call".to_string(),
                call_id: None,
                creator: user_id.clone(),
                config: json!({}),
            })
            .await
            .unwrap();

        let params = CreateMembershipParams {
            room_id: room_id.clone(),
            session_id: session_id.clone(),
            user_id: user_id.clone(),
            device_id: device_id.clone(),
            membership_id: format!("membership_{suffix}"),
            application: "m.call".to_string(),
            call_id: Some(format!("call_{suffix}")),
            foci_active: Some("focus1".to_string()),
            foci_preferred: Some(json!({"type": "focus", "priority": 1})),
            application_data: Some(json!({"audio": true})),
        };

        let membership = storage
            .create_membership(params)
            .await
            .expect("create_membership should succeed");

        assert!(membership.id > 0);
        assert_eq!(membership.room_id, room_id);
        assert_eq!(membership.session_id, session_id);
        assert_eq!(membership.user_id, user_id);
        assert_eq!(membership.device_id, device_id);
        assert_eq!(membership.membership_id, format!("membership_{suffix}"));
        assert_eq!(membership.application, "m.call");
        assert_eq!(membership.call_id, Some(format!("call_{suffix}")));
        assert_eq!(membership.foci_active, Some("focus1".to_string()));
        assert_eq!(membership.foci_preferred, Some(json!({"type": "focus", "priority": 1})));
        assert_eq!(membership.application_data, Some(json!({"audio": true})));
        assert!(membership.is_active);
        assert!(membership.created_ts > 0);
        assert_eq!(membership.updated_ts, membership.created_ts);
        // expires_at should be set to created_ts + 1 hour (in ms)
        assert!(membership.expires_at.is_some());
        let expires_at = membership.expires_at.unwrap();
        assert!(expires_at > membership.created_ts);
        assert!(expires_at <= membership.created_ts + 3600 * 1000 + 1000); // allow 1s clock drift

        cleanup_matrixrtc_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_create_membership_upsert_updates_existing() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_matrixrtc_data(&pool, &suffix).await;

        let storage = MatrixRTCStorage::new(pool.clone());
        let room_id = format!("!room_{suffix}:example.com");
        let session_id = format!("session_{suffix}");
        let user_id = format!("@user_{suffix}:example.com");
        let device_id = format!("device_{suffix}");

        // Create session
        storage
            .create_session(CreateSessionParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                application: "m.call".to_string(),
                call_id: None,
                creator: user_id.clone(),
                config: json!({}),
            })
            .await
            .unwrap();

        // First membership
        let membership1 = storage
            .create_membership(CreateMembershipParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                user_id: user_id.clone(),
                device_id: device_id.clone(),
                membership_id: format!("membership1_{suffix}"),
                application: "m.call".to_string(),
                call_id: Some(format!("call1_{suffix}")),
                foci_active: Some("focus1".to_string()),
                foci_preferred: None,
                application_data: None,
            })
            .await
            .expect("first create_membership");
        let id1 = membership1.id;
        let created_ts1 = membership1.created_ts;

        // Second membership with same keys should UPSERT
        let membership2 = storage
            .create_membership(CreateMembershipParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                user_id: user_id.clone(),
                device_id: device_id.clone(),
                membership_id: format!("membership2_{suffix}"),
                application: "m.call".to_string(),
                call_id: Some(format!("call2_{suffix}")),
                foci_active: Some("focus2".to_string()),
                foci_preferred: Some(json!({"priority": 2})),
                application_data: Some(json!({"video": true})),
            })
            .await
            .expect("upsert create_membership");

        assert_eq!(membership2.id, id1, "upsert should update the same row");
        assert_eq!(membership2.membership_id, format!("membership2_{suffix}"));
        assert_eq!(membership2.foci_active, Some("focus2".to_string()));
        assert_eq!(membership2.foci_preferred, Some(json!({"priority": 2})));
        assert_eq!(membership2.application_data, Some(json!({"video": true})));
        assert!(membership2.is_active);
        assert!(membership2.updated_ts >= created_ts1);

        cleanup_matrixrtc_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_memberships_for_session() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_matrixrtc_data(&pool, &suffix).await;

        let storage = MatrixRTCStorage::new(pool.clone());
        let room_id = format!("!room_{suffix}:example.com");
        let session_id = format!("session_{suffix}");

        // Create session
        storage
            .create_session(CreateSessionParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                application: "m.call".to_string(),
                call_id: None,
                creator: format!("@creator_{suffix}:example.com"),
                config: json!({}),
            })
            .await
            .unwrap();

        // Create two memberships
        storage
            .create_membership(CreateMembershipParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                user_id: format!("@alice_{suffix}:example.com"),
                device_id: format!("device_a_{suffix}"),
                membership_id: format!("m1_{suffix}"),
                application: "m.call".to_string(),
                call_id: None,
                foci_active: None,
                foci_preferred: None,
                application_data: None,
            })
            .await
            .unwrap();

        storage
            .create_membership(CreateMembershipParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                user_id: format!("@bob_{suffix}:example.com"),
                device_id: format!("device_b_{suffix}"),
                membership_id: format!("m2_{suffix}"),
                application: "m.call".to_string(),
                call_id: None,
                foci_active: None,
                foci_preferred: None,
                application_data: None,
            })
            .await
            .unwrap();

        let memberships = storage
            .get_memberships_for_session(&room_id, &session_id)
            .await
            .expect("get_memberships_for_session should succeed");

        assert_eq!(memberships.len(), 2);
        // ORDER BY created_ts ASC, so alice should come first
        assert_eq!(memberships[0].user_id, format!("@alice_{suffix}:example.com"));
        assert_eq!(memberships[1].user_id, format!("@bob_{suffix}:example.com"));
        assert!(memberships.iter().all(|m| m.is_active));

        cleanup_matrixrtc_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_user_membership_found() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_matrixrtc_data(&pool, &suffix).await;

        let storage = MatrixRTCStorage::new(pool.clone());
        let room_id = format!("!room_{suffix}:example.com");
        let session_id = format!("session_{suffix}");
        let user_id = format!("@user_{suffix}:example.com");
        let device_id = format!("device_{suffix}");

        // Create session and membership
        storage
            .create_session(CreateSessionParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                application: "m.call".to_string(),
                call_id: None,
                creator: user_id.clone(),
                config: json!({}),
            })
            .await
            .unwrap();

        storage
            .create_membership(CreateMembershipParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                user_id: user_id.clone(),
                device_id: device_id.clone(),
                membership_id: format!("m_{suffix}"),
                application: "m.call".to_string(),
                call_id: None,
                foci_active: None,
                foci_preferred: None,
                application_data: None,
            })
            .await
            .unwrap();

        let found = storage
            .get_user_membership(&room_id, &session_id, &user_id, &device_id)
            .await
            .expect("get_user_membership query should succeed")
            .expect("membership should be found");

        assert_eq!(found.user_id, user_id);
        assert_eq!(found.device_id, device_id);
        assert_eq!(found.membership_id, format!("m_{suffix}"));

        cleanup_matrixrtc_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_user_membership_not_found() {
        let pool = test_pool().await;
        let storage = MatrixRTCStorage::new(pool.clone());

        let result = storage
            .get_user_membership("!nonexistent:example.com", "no_session", "@fake:example.com", "fake_device")
            .await
            .expect("get_user_membership query should succeed");

        assert!(result.is_none(), "nonexistent membership should return None");
    }

    #[tokio::test]
    async fn test_end_membership() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_matrixrtc_data(&pool, &suffix).await;

        let storage = MatrixRTCStorage::new(pool.clone());
        let room_id = format!("!room_{suffix}:example.com");
        let session_id = format!("session_{suffix}");
        let user_id = format!("@user_{suffix}:example.com");
        let device_id = format!("device_{suffix}");

        // Create session and membership
        storage
            .create_session(CreateSessionParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                application: "m.call".to_string(),
                call_id: None,
                creator: user_id.clone(),
                config: json!({}),
            })
            .await
            .unwrap();

        storage
            .create_membership(CreateMembershipParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                user_id: user_id.clone(),
                device_id: device_id.clone(),
                membership_id: format!("m_{suffix}"),
                application: "m.call".to_string(),
                call_id: None,
                foci_active: None,
                foci_preferred: None,
                application_data: None,
            })
            .await
            .unwrap();

        storage
            .end_membership(&room_id, &session_id, &user_id, &device_id)
            .await
            .expect("end_membership should succeed");

        // After ending, should still be found by get_user_membership (no is_active filter),
        // but should be excluded from get_memberships_for_session (filters is_active=true)
        let memberships = storage
            .get_memberships_for_session(&room_id, &session_id)
            .await
            .expect("get_memberships_for_session should succeed");

        assert!(memberships.is_empty(), "ended membership should not appear in get_memberships_for_session");

        cleanup_matrixrtc_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_cleanup_expired_memberships() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_matrixrtc_data(&pool, &suffix).await;

        let storage = MatrixRTCStorage::new(pool.clone());
        let room_id = format!("!room_{suffix}:example.com");
        let session_id = format!("session_{suffix}");
        let user_id = format!("@user_{suffix}:example.com");

        // Create session
        storage
            .create_session(CreateSessionParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                application: "m.call".to_string(),
                call_id: None,
                creator: user_id.clone(),
                config: json!({}),
            })
            .await
            .unwrap();

        // Create a membership normally (expires_at = now + 1hr, which is in future)
        storage
            .create_membership(CreateMembershipParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                user_id: format!("@active_{suffix}:example.com"),
                device_id: format!("device_active_{suffix}"),
                membership_id: format!("m_active_{suffix}"),
                application: "m.call".to_string(),
                call_id: None,
                foci_active: None,
                foci_preferred: None,
                application_data: None,
            })
            .await
            .unwrap();

        // Directly insert a membership with an already-expired expires_at
        let now = chrono::Utc::now().timestamp_millis();
        let past_expiry = now - 10_000; // 10 seconds in the past
        sqlx::query(
            r#"
            INSERT INTO matrixrtc_memberships
                (room_id, session_id, user_id, device_id, membership_id, application, call_id,
                 created_ts, updated_ts, expires_at, foci_active, foci_preferred, application_data, is_active)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, true)
            "#,
        )
        .bind(&room_id)
        .bind(&session_id)
        .bind(format!("@expired_{suffix}:example.com"))
        .bind(format!("device_expired_{suffix}"))
        .bind(format!("m_expired_{suffix}"))
        .bind("m.call")
        .bind::<Option<String>>(None)
        .bind(now)
        .bind(now)
        .bind(past_expiry)
        .bind::<Option<String>>(None)
        .bind::<Option<serde_json::Value>>(None)
        .bind::<Option<serde_json::Value>>(None)
        .execute(&*pool)
        .await
        .expect("direct insert of expired membership should succeed");

        // Call cleanup — the expired membership should be deactivated
        let affected = storage
            .cleanup_expired_memberships()
            .await
            .expect("cleanup_expired_memberships should succeed");

        assert!(affected >= 1, "should have cleaned up at least 1 expired membership");

        // Only the non-expired membership should remain in get_memberships_for_session
        let remaining = storage
            .get_memberships_for_session(&room_id, &session_id)
            .await
            .expect("get_memberships_for_session should succeed");

        assert_eq!(remaining.len(), 1, "only the non-expired membership should remain");
        assert_eq!(remaining[0].user_id, format!("@active_{suffix}:example.com"));

        cleanup_matrixrtc_data(&pool, &suffix).await;
    }

    // --- Encryption key tests ---

    #[tokio::test]
    async fn test_store_and_get_encryption_keys() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_matrixrtc_data(&pool, &suffix).await;

        let storage = MatrixRTCStorage::new(pool.clone());
        let room_id = format!("!room_{suffix}:example.com");
        let session_id = format!("session_{suffix}");

        // Create session (for consistency, though not required by FK for encryption keys)
        storage
            .create_session(CreateSessionParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                application: "m.call".to_string(),
                call_id: None,
                creator: format!("@creator_{suffix}:example.com"),
                config: json!({}),
            })
            .await
            .unwrap();

        // Store two encryption keys
        let key1 = storage
            .store_encryption_key(
                &room_id,
                &session_id,
                0,
                "base64key0",
                &format!("@sender_{suffix}:example.com"),
                &format!("device_{suffix}"),
            )
            .await
            .expect("store_encryption_key should succeed");

        assert!(key1.id > 0);
        assert_eq!(key1.room_id, room_id);
        assert_eq!(key1.session_id, session_id);
        assert_eq!(key1.key_index, 0);
        assert_eq!(key1.key, "base64key0");
        assert_eq!(key1.sender_user_id, format!("@sender_{suffix}:example.com"));
        assert_eq!(key1.sender_device_id, format!("device_{suffix}"));
        assert!(key1.created_ts > 0);
        assert!(key1.expires_at.is_some());
        let expires_at = key1.expires_at.unwrap();
        assert!(expires_at > key1.created_ts);
        assert!(expires_at <= key1.created_ts + 24 * 3600 * 1000 + 1000); // 24hrs + 1s drift

        // Store a second key with different index
        storage
            .store_encryption_key(
                &room_id,
                &session_id,
                1,
                "base64key1",
                &format!("@sender_{suffix}:example.com"),
                &format!("device_{suffix}"),
            )
            .await
            .expect("store second key");

        // Retrieve all keys
        let keys = storage
            .get_encryption_keys(&room_id, &session_id)
            .await
            .expect("get_encryption_keys should succeed");

        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].key_index, 0);
        assert_eq!(keys[0].key, "base64key0");
        assert_eq!(keys[1].key_index, 1);
        assert_eq!(keys[1].key, "base64key1");

        cleanup_matrixrtc_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_store_encryption_key_upsert() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_matrixrtc_data(&pool, &suffix).await;

        let storage = MatrixRTCStorage::new(pool.clone());
        let room_id = format!("!room_{suffix}:example.com");
        let session_id = format!("session_{suffix}");

        // Create session first
        storage
            .create_session(CreateSessionParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                application: "m.call".to_string(),
                call_id: None,
                creator: format!("@creator_{suffix}:example.com"),
                config: json!({}),
            })
            .await
            .unwrap();

        // First store
        let key1 = storage
            .store_encryption_key(
                &room_id,
                &session_id,
                0,
                "old_key",
                &format!("@sender_{suffix}:example.com"),
                &format!("device_{suffix}"),
            )
            .await
            .expect("first store_encryption_key");

        assert_eq!(key1.key, "old_key");

        // Second store with same (room_id, session_id, key_index) — ON CONFLICT DO UPDATE
        let key2 = storage
            .store_encryption_key(
                &room_id,
                &session_id,
                0,
                "new_key",
                &format!("@sender2_{suffix}:example.com"),
                &format!("device2_{suffix}"),
            )
            .await
            .expect("upsert store_encryption_key");

        // Should be same row but updated key
        assert_eq!(key2.id, key1.id, "upsert should update the same row");
        assert_eq!(key2.key, "new_key");
        assert_eq!(key2.sender_user_id, format!("@sender2_{suffix}:example.com"));
        assert_eq!(key2.sender_device_id, format!("device2_{suffix}"));

        cleanup_matrixrtc_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_encryption_keys_empty_for_nonexistent() {
        let pool = test_pool().await;
        let storage = MatrixRTCStorage::new(pool.clone());

        let keys = storage
            .get_encryption_keys("!nonexistent:example.com", "no_session")
            .await
            .expect("get_encryption_keys query should succeed");

        assert!(keys.is_empty());
    }

    // --- Combined query tests ---

    #[tokio::test]
    async fn test_get_session_with_memberships() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_matrixrtc_data(&pool, &suffix).await;

        let storage = MatrixRTCStorage::new(pool.clone());
        let room_id = format!("!room_{suffix}:example.com");
        let session_id = format!("session_{suffix}");

        // Create session
        storage
            .create_session(CreateSessionParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                application: "m.call".to_string(),
                call_id: None,
                creator: format!("@creator_{suffix}:example.com"),
                config: json!({"type": "offer"}),
            })
            .await
            .unwrap();

        // Create two memberships
        storage
            .create_membership(CreateMembershipParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                user_id: format!("@alice_{suffix}:example.com"),
                device_id: format!("device_a_{suffix}"),
                membership_id: format!("m_a_{suffix}"),
                application: "m.call".to_string(),
                call_id: None,
                foci_active: None,
                foci_preferred: None,
                application_data: None,
            })
            .await
            .unwrap();

        storage
            .create_membership(CreateMembershipParams {
                room_id: room_id.clone(),
                session_id: session_id.clone(),
                user_id: format!("@bob_{suffix}:example.com"),
                device_id: format!("device_b_{suffix}"),
                membership_id: format!("m_b_{suffix}"),
                application: "m.call".to_string(),
                call_id: None,
                foci_active: None,
                foci_preferred: None,
                application_data: None,
            })
            .await
            .unwrap();

        let result = storage
            .get_session_with_memberships(&room_id, &session_id)
            .await
            .expect("get_session_with_memberships query should succeed")
            .expect("session with memberships should be found");

        assert_eq!(result.session.room_id, room_id);
        assert_eq!(result.session.session_id, session_id);
        assert_eq!(result.session.config, json!({"type": "offer"}));
        assert_eq!(result.memberships.len(), 2);
        assert_eq!(result.memberships[0].user_id, format!("@alice_{suffix}:example.com"));
        assert_eq!(result.memberships[1].user_id, format!("@bob_{suffix}:example.com"));

        cleanup_matrixrtc_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_session_with_memberships_not_found() {
        let pool = test_pool().await;
        let storage = MatrixRTCStorage::new(pool.clone());

        let result = storage
            .get_session_with_memberships("!nonexistent:example.com", "no_session")
            .await
            .expect("get_session_with_memberships query should succeed");

        assert!(result.is_none(), "nonexistent session should return None");
    }
}
