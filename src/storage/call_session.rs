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
    pub async fn create_session(
        &self,
        params: CreateCallSessionParams,
    ) -> Result<CallSession, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let lifetime = params.lifetime.unwrap_or(60000); // Default 60 seconds

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
    pub async fn get_session(
        &self,
        call_id: &str,
        room_id: &str,
    ) -> Result<Option<CallSession>, sqlx::Error> {
        let session = sqlx::query_as::<_, CallSession>(
            r#"
            SELECT * FROM call_sessions 
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
    pub async fn update_state(
        &self,
        call_id: &str,
        room_id: &str,
        state: &str,
    ) -> Result<(), sqlx::Error> {
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
    pub async fn set_answer(
        &self,
        call_id: &str,
        room_id: &str,
        answer_sdp: &str,
    ) -> Result<(), sqlx::Error> {
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
    pub async fn get_candidates(
        &self,
        call_id: &str,
        room_id: &str,
    ) -> Result<Vec<CallCandidate>, sqlx::Error> {
        let candidates = sqlx::query_as::<_, CallCandidate>(
            r#"
            SELECT * FROM call_candidates 
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
    pub async fn end_session(
        &self,
        call_id: &str,
        room_id: &str,
    ) -> Result<(), sqlx::Error> {
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
