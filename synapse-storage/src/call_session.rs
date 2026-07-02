use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CallSession {
    pub id: i64,
    pub call_id: String,
    pub room_id: String,
    pub caller_id: String,
    pub callee_id: Option<String>,
    pub state: String,
    pub offer_sdp: Option<String>,
    pub answer_sdp: Option<String>,
    pub lifetime: i64,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub ended_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CallCandidate {
    pub id: i64,
    pub call_id: String,
    pub room_id: String,
    pub sender_id: String,
    pub candidate: serde_json::Value,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCallSessionParams {
    pub call_id: String,
    pub room_id: String,
    pub caller_id: String,
    pub callee_id: Option<String>,
    pub offer_sdp: Option<String>,
    pub lifetime: Option<i64>,
}

pub struct CallSessionStorage {
    pool: Arc<Pool<Postgres>>,
}

impl CallSessionStorage {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    /// 创建呼叫会话
    pub async fn create_session(&self, params: CreateCallSessionParams) -> Result<CallSession, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let lifetime = params.lifetime.unwrap_or(60_000); // Default 60 seconds

        let session = sqlx::query_as::<_, CallSession>(
            r#"
            INSERT INTO call_sessions
                (call_id, room_id, caller_id, callee_id, state, offer_sdp, lifetime, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, 'ringing', $5, $6, $7, $7)
            RETURNING *
            "#,
        )
        .bind(&params.call_id)
        .bind(&params.room_id)
        .bind(&params.caller_id)
        .bind(&params.callee_id)
        .bind(&params.offer_sdp)
        .bind(lifetime)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(session)
    }

    /// 获取呼叫会话
    pub async fn get_session(&self, call_id: &str, room_id: &str) -> Result<Option<CallSession>, sqlx::Error> {
        let session = sqlx::query_as::<_, CallSession>(
            r#"
            SELECT id, call_id, room_id, caller_id, callee_id, state, offer_sdp, answer_sdp, lifetime, created_ts, updated_ts, ended_ts FROM call_sessions
            WHERE call_id = $1 AND room_id = $2
            "#,
        )
        .bind(call_id)
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(session)
    }

    /// 更新会话状态
    pub async fn update_state(&self, call_id: &str, room_id: &str, state: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE call_sessions
            SET state = $3, updated_ts = $4, ended_ts = CASE WHEN $3 = 'ended' THEN $4 ELSE ended_ts END
            WHERE call_id = $1 AND room_id = $2
            "#,
        )
        .bind(call_id)
        .bind(room_id)
        .bind(state)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// 设置应答SDP
    pub async fn set_answer(&self, call_id: &str, room_id: &str, answer_sdp: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE call_sessions
            SET answer_sdp = $3, state = 'connected', updated_ts = $4
            WHERE call_id = $1 AND room_id = $2
            "#,
        )
        .bind(call_id)
        .bind(room_id)
        .bind(answer_sdp)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// 添加ICE候选人
    pub async fn add_candidate(
        &self,
        call_id: &str,
        room_id: &str,
        sender_id: &str,
        candidate: serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO call_candidates (call_id, room_id, sender_id, candidate, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(call_id)
        .bind(room_id)
        .bind(sender_id)
        .bind(candidate)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// 获取会话的所有候选人
    pub async fn get_candidates(&self, call_id: &str, room_id: &str) -> Result<Vec<CallCandidate>, sqlx::Error> {
        let candidates = sqlx::query_as::<_, CallCandidate>(
            r#"
            SELECT id, call_id, room_id, sender_id, candidate, created_ts FROM call_candidates
            WHERE call_id = $1 AND room_id = $2
            ORDER BY created_ts ASC
            "#,
        )
        .bind(call_id)
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(candidates)
    }

    /// 结束会话
    pub async fn end_session(&self, call_id: &str, room_id: &str) -> Result<(), sqlx::Error> {
        self.update_state(call_id, room_id, "ended").await
    }

    /// 清理过期的呼叫会话
    pub async fn cleanup_expired(&self) -> Result<u64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r#"
            UPDATE call_sessions
            SET state = 'ended', ended_ts = $1
            WHERE state != 'ended'
            AND created_ts + lifetime < $1
            "#,
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn test_pool() -> Arc<Pool<Postgres>> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    /// Clean up test call sessions and candidates matching a UUID suffix.
    async fn cleanup_test_data(pool: &Pool<Postgres>, suffix: &str) {
        let _ = sqlx::query("DELETE FROM call_candidates WHERE call_id LIKE $1")
            .bind(format!("%{suffix}"))
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM call_sessions WHERE call_id LIKE $1")
            .bind(format!("%{suffix}"))
            .execute(pool)
            .await;
    }

    #[tokio::test]
    async fn test_create_session_returns_valid_record() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_test_data(&pool, &suffix).await;

        let storage = CallSessionStorage::new(pool.clone());

        let params = CreateCallSessionParams {
            call_id: format!("create_session_{suffix}"),
            room_id: format!("!room_{suffix}:example.com"),
            caller_id: format!("@caller_{suffix}:example.com"),
            callee_id: Some(format!("@callee_{suffix}:example.com")),
            offer_sdp: Some("v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=test".to_string()),
            lifetime: Some(120_000),
        };

        let session = storage.create_session(params).await.expect("create_session should succeed");

        assert!(session.id > 0);
        assert_eq!(session.call_id, format!("create_session_{suffix}"));
        assert_eq!(session.room_id, format!("!room_{suffix}:example.com"));
        assert_eq!(session.caller_id, format!("@caller_{suffix}:example.com"));
        assert_eq!(session.callee_id, Some(format!("@callee_{suffix}:example.com")));
        assert_eq!(session.state, "ringing");
        assert!(session.offer_sdp.is_some());
        assert_eq!(session.lifetime, 120_000);
        assert!(session.created_ts > 0);
        assert!(session.updated_ts.is_some());
        assert_eq!(session.updated_ts, Some(session.created_ts));
        assert!(session.ended_ts.is_none());

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_create_session_default_lifetime() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_test_data(&pool, &suffix).await;

        let storage = CallSessionStorage::new(pool.clone());

        let params = CreateCallSessionParams {
            call_id: format!("default_lifetime_{suffix}"),
            room_id: format!("!room_{suffix}:example.com"),
            caller_id: format!("@caller_{suffix}:example.com"),
            callee_id: None,
            offer_sdp: None,
            lifetime: None,
        };

        let session = storage.create_session(params).await.expect("create_session should succeed");

        assert_eq!(session.lifetime, 60_000, "default lifetime should be 60000ms");

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_session_found() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_test_data(&pool, &suffix).await;

        let storage = CallSessionStorage::new(pool.clone());
        let call_id = format!("get_found_{suffix}");
        let room_id = format!("!room_{suffix}:example.com");

        let params = CreateCallSessionParams {
            call_id: call_id.clone(),
            room_id: room_id.clone(),
            caller_id: format!("@alice_{suffix}:example.com"),
            callee_id: None,
            offer_sdp: None,
            lifetime: None,
        };
        storage.create_session(params).await.unwrap();

        let found = storage
            .get_session(&call_id, &room_id)
            .await
            .expect("get_session should succeed")
            .expect("session should be found");

        assert_eq!(found.call_id, call_id);
        assert_eq!(found.room_id, room_id);
        assert_eq!(found.state, "ringing");

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_session_not_found() {
        let pool = test_pool().await;
        let storage = CallSessionStorage::new(pool.clone());

        let result = storage
            .get_session("nonexistent_call", "!nonexistent:example.com")
            .await
            .expect("get_session query should succeed");

        assert!(result.is_none(), "nonexistent session should return None");
    }

    #[tokio::test]
    async fn test_update_state() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_test_data(&pool, &suffix).await;

        let storage = CallSessionStorage::new(pool.clone());
        let call_id = format!("update_state_{suffix}");
        let room_id = format!("!room_{suffix}:example.com");

        let params = CreateCallSessionParams {
            call_id: call_id.clone(),
            room_id: room_id.clone(),
            caller_id: format!("@caller_{suffix}:example.com"),
            callee_id: None,
            offer_sdp: None,
            lifetime: None,
        };
        storage.create_session(params).await.unwrap();

        // Update state to "connected"
        storage.update_state(&call_id, &room_id, "connected").await.expect("update_state should succeed");

        let updated = storage.get_session(&call_id, &room_id).await.unwrap().expect("session should exist");

        assert_eq!(updated.state, "connected");
        assert!(updated.updated_ts.unwrap() > updated.created_ts);

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_set_answer_and_verify_connected() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_test_data(&pool, &suffix).await;

        let storage = CallSessionStorage::new(pool.clone());
        let call_id = format!("set_answer_{suffix}");
        let room_id = format!("!room_{suffix}:example.com");

        let params = CreateCallSessionParams {
            call_id: call_id.clone(),
            room_id: room_id.clone(),
            caller_id: format!("@caller_{suffix}:example.com"),
            callee_id: None,
            offer_sdp: None,
            lifetime: None,
        };
        storage.create_session(params).await.unwrap();

        let answer = "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=answer";
        storage.set_answer(&call_id, &room_id, answer).await.expect("set_answer should succeed");

        let updated = storage.get_session(&call_id, &room_id).await.unwrap().expect("session should exist");

        assert_eq!(updated.state, "connected");
        assert_eq!(updated.answer_sdp.unwrap(), answer);

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_end_session() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_test_data(&pool, &suffix).await;

        let storage = CallSessionStorage::new(pool.clone());
        let call_id = format!("end_session_{suffix}");
        let room_id = format!("!room_{suffix}:example.com");

        let params = CreateCallSessionParams {
            call_id: call_id.clone(),
            room_id: room_id.clone(),
            caller_id: format!("@caller_{suffix}:example.com"),
            callee_id: None,
            offer_sdp: None,
            lifetime: None,
        };
        storage.create_session(params).await.unwrap();

        storage.end_session(&call_id, &room_id).await.expect("end_session should succeed");

        let ended = storage.get_session(&call_id, &room_id).await.unwrap().expect("session should exist");

        assert_eq!(ended.state, "ended");
        assert!(ended.ended_ts.is_some(), "ended_ts should be set when session ends");

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_test_data(&pool, &suffix).await;

        let storage = CallSessionStorage::new(pool.clone());
        let call_id = format!("cleanup_expired_{suffix}");
        let room_id = format!("!room_{suffix}:example.com");

        // Create a session with lifetime of 1 millisecond
        let params = CreateCallSessionParams {
            call_id: call_id.clone(),
            room_id: room_id.clone(),
            caller_id: format!("@caller_{suffix}:example.com"),
            callee_id: None,
            offer_sdp: None,
            lifetime: Some(1),
        };
        storage.create_session(params).await.unwrap();

        // Wait for the session to expire
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let affected = storage.cleanup_expired().await.expect("cleanup_expired should succeed");

        assert!(affected >= 1, "should have cleaned up at least 1 expired session");

        let cleaned = storage.get_session(&call_id, &room_id).await.unwrap().expect("session should exist");

        assert_eq!(cleaned.state, "ended");
        assert!(cleaned.ended_ts.is_some());

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_candidates_round_trip() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_test_data(&pool, &suffix).await;

        let storage = CallSessionStorage::new(pool.clone());
        let call_id = format!("candidates_{suffix}");
        let room_id = format!("!room_{suffix}:example.com");

        // Need a session first since candidates are associated with a call
        let params = CreateCallSessionParams {
            call_id: call_id.clone(),
            room_id: room_id.clone(),
            caller_id: format!("@caller_{suffix}:example.com"),
            callee_id: None,
            offer_sdp: None,
            lifetime: None,
        };
        storage.create_session(params).await.unwrap();

        let candidate1 = serde_json::json!({
            "candidate": "candidate:1 1 UDP 2122252543 192.168.1.1 12345 typ host",
            "sdpMid": "0",
            "sdpMLineIndex": 0
        });
        let candidate2 = serde_json::json!({
            "candidate": "candidate:2 1 UDP 2122252543 10.0.0.1 54321 typ host",
            "sdpMid": "0",
            "sdpMLineIndex": 0
        });

        storage
            .add_candidate(&call_id, &room_id, &format!("@alice_{suffix}:example.com"), candidate1.clone())
            .await
            .expect("add_candidate should succeed");

        storage
            .add_candidate(&call_id, &room_id, &format!("@bob_{suffix}:example.com"), candidate2.clone())
            .await
            .expect("add_candidate should succeed");

        let candidates = storage.get_candidates(&call_id, &room_id).await.expect("get_candidates should succeed");

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].sender_id, format!("@alice_{suffix}:example.com"));
        assert_eq!(candidates[0].candidate, candidate1);
        assert_eq!(candidates[1].sender_id, format!("@bob_{suffix}:example.com"));
        assert_eq!(candidates[1].candidate, candidate2);

        cleanup_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_candidates_empty_when_none_added() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_test_data(&pool, &suffix).await;

        let storage = CallSessionStorage::new(pool.clone());

        let candidates = storage
            .get_candidates("no_candidates", "!nonexistent:example.com")
            .await
            .expect("get_candidates should succeed");

        assert!(candidates.is_empty());

        cleanup_test_data(&pool, &suffix).await;
    }
}
