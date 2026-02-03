use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoomEvent {
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub event_type: String,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
    pub depth: i64,
    pub origin_server_ts: i64,
    pub processed_ts: i64,
    pub not_before: i64,
    pub status: Option<String>,
    pub reference_image: Option<String>,
    pub origin: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StateEvent {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
    pub unsigned: serde_json::Value,
    pub redacted: Option<bool>,
    pub origin_server_ts: i64,
    pub depth: Option<i64>,
    pub processed_ts: Option<i64>,
    pub not_before: Option<i64>,
    pub status: Option<String>,
    pub reference_image: Option<String>,
    pub origin: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Clone)]
pub struct EventStorage {
    pub pool: Arc<Pool<Postgres>>,
}

#[derive(Debug, Clone)]
pub struct CreateEventParams {
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub event_type: String,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
    pub origin_server_ts: i64,
}

impl EventStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_event(&self, params: CreateEventParams) -> Result<RoomEvent, sqlx::Error> {
        let processed_ts = chrono::Utc::now().timestamp_millis();
        let event = sqlx::query_as(
            r#"
            INSERT INTO events (event_id, room_id, user_id, sender, event_type, content, state_key, origin_server_ts, processed_ts)
            VALUES ($1, $2, $3, $3, $4, $5, $6, $7, $8)
            RETURNING event_id, room_id, user_id, event_type, content, state_key, 
                      COALESCE(depth, 0) as depth, origin_server_ts, processed_ts, 
                      COALESCE(not_before, 0) as not_before, status, reference_image, 
                      COALESCE(origin, 'self') as origin
            "#,
        )
        .bind(&params.event_id)
        .bind(&params.room_id)
        .bind(&params.user_id)
        .bind(&params.event_type)
        .bind(&params.content)
        .bind(params.state_key.as_deref())
        .bind(params.origin_server_ts)
        .bind(processed_ts)
        .fetch_one(&*self.pool)
        .await?;
        Ok(event)
    }

    pub async fn get_event(&self, event_id: &str) -> Result<Option<RoomEvent>, sqlx::Error> {
        let event = sqlx::query_as::<_, RoomEvent>(
            r#"
            SELECT event_id, room_id, user_id, event_type, content, state_key, 
                   COALESCE(depth, 0) as depth, origin_server_ts, processed_ts, 
                   COALESCE(not_before, 0) as not_before, status, reference_image, origin
            FROM events WHERE event_id = $1
            "#,
        )
        .bind(event_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(event)
    }

    pub async fn delete_events_before(
        &self,
        room_id: &str,
        timestamp: i64,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM events WHERE room_id = $1 AND origin_server_ts < $2 AND event_type != 'm.room.create'",
        )
        .bind(room_id)
        .bind(timestamp)
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn get_room_events(
        &self,
        room_id: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let events = sqlx::query_as(
            r#"
            SELECT event_id, room_id, user_id, event_type, content, state_key, 
                   COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(processed_ts, 0) as processed_ts, 
                   COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(origin, 'self') as origin
            FROM events WHERE room_id = $1
            ORDER BY origin_server_ts DESC
            LIMIT $2
            "#,
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(events)
    }

    pub async fn get_room_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let events = sqlx::query_as(
            r#"
            SELECT event_id, room_id, user_id, event_type, content, state_key, 
                   COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(processed_ts, 0) as processed_ts, 
                   COALESCE(not_before, 0) as not_before, status, reference_image, origin
            FROM events WHERE room_id = $1 AND event_type = $2
            ORDER BY origin_server_ts DESC
            LIMIT $3
            "#,
        )
        .bind(room_id)
        .bind(event_type)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(events)
    }

    pub async fn get_sender_events(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let events = sqlx::query_as(
            r#"
            SELECT event_id, room_id, user_id, event_type, content, state_key, 
                   COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(processed_ts, 0) as processed_ts, 
                   COALESCE(not_before, 0) as not_before, status, reference_image, origin
            FROM events WHERE user_id = $1
            ORDER BY origin_server_ts DESC
            LIMIT $2
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(events)
    }

    pub async fn get_room_message_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(COUNT(*), 0) FROM events WHERE room_id = $1 AND event_type = 'm.room.message'
            "#,
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    pub async fn get_total_message_count(&self) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(COUNT(*), 0) FROM events WHERE event_type = 'm.room.message'
            "#,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    pub async fn delete_room_events(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM events WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_state_events(&self, room_id: &str) -> Result<Vec<StateEvent>, sqlx::Error> {
        sqlx::query_as::<_, StateEvent>(
            r#"
            SELECT * FROM events WHERE room_id = $1 AND state_key IS NOT NULL ORDER BY origin_server_ts DESC
            "#,
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_state_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
    ) -> Result<Vec<StateEvent>, sqlx::Error> {
        sqlx::query_as::<_, StateEvent>(
            r#"
            SELECT * FROM events WHERE room_id = $1 AND event_type = $2 AND state_key IS NOT NULL ORDER BY origin_server_ts DESC
            "#,
        )
        .bind(room_id)
        .bind(event_type)
        .fetch_all(&*self.pool)
        .await
    }
}
