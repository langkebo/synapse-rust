use async_trait::async_trait;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

use super::repository::EventRepository;

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
    #[sqlx(rename = "processed_at")]
    pub processed_ts: i64,
    pub not_before: i64,
    pub status: Option<String>,
    pub reference_image: Option<String>,
    pub origin: String,
    pub stream_ordering: Option<i64>,
    /// Target event_id for `m.room.redaction` events (P0-05).  `None` for
    /// non-redaction events or redaction events that do not specify a target.
    pub redacts: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StateEvent {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: Option<String>,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
    pub unsigned: Option<serde_json::Value>,
    pub is_redacted: Option<bool>,
    pub origin_server_ts: i64,
    pub depth: Option<i64>,
    #[sqlx(rename = "processed_at")]
    pub processed_ts: Option<i64>,
    pub not_before: Option<i64>,
    pub status: Option<String>,
    pub reference_image: Option<String>,
    pub origin: Option<String>,
    pub user_id: Option<String>,
    pub stream_ordering: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoomEphemeralEvent {
    pub event_type: String,
    pub user_id: String,
    pub content: serde_json::Value,
    pub stream_id: i64,
    pub created_ts: i64,
}

#[derive(Clone)]
pub struct EventStorage {
    pub pool: Arc<Pool<Postgres>>,
    pub server_name: String,
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
    /// Target event_id for `m.room.redaction` events (P0-05).  Set to `None`
    /// for non-redaction events.  For v1-v10 this is populated from the
    /// top-level `redacts` PDU field; for v11+ from `content.redacts`.
    pub redacts: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EventQueryFilter {
    pub types: Option<Vec<String>>,
    pub not_types: Option<Vec<String>>,
    pub senders: Option<Vec<String>>,
    pub not_senders: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Event signature model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventSignature {
    pub id: uuid::Uuid,
    pub event_id: String,
    pub user_id: String,
    pub device_id: String,
    pub signature: String,
    pub key_id: String,
    pub created_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventReportId {
    pub id: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventReport {
    pub id: i64,
    pub event_id: String,
    pub room_id: String,
    pub reporter_user_id: String,
    pub reason: Option<String>,
    pub score: i32,
    pub received_ts: i64,
    #[sqlx(rename = "resolved_at")]
    pub resolved_ts: Option<i64>,
    pub resolved_by: Option<String>,
}

#[async_trait]
impl EventRepository for EventStorage {
    async fn get_event(&self, event_id: &str) -> Result<Option<RoomEvent>, sqlx::Error> {
        self.get_event(event_id).await
    }

    async fn create_event(&self, params: &CreateEventParams) -> Result<RoomEvent, sqlx::Error> {
        EventStorage::create_event(self, params.clone(), None).await
    }

    async fn get_room_events_paginated(
        &self,
        room_id: &str,
        limit: i64,
        from: Option<i64>,
        to: Option<i64>,
        dir: Option<&str>,
        filter: Option<&EventQueryFilter>,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        if to.is_some() {
            tracing::warn!("EventRepository::get_room_events_paginated: 'to' parameter not yet supported");
        }
        if filter.is_some() {
            tracing::warn!("EventRepository::get_room_events_paginated: 'filter' parameter not yet supported");
        }
        let direction = dir.unwrap_or("b");
        EventStorage::get_room_events_paginated(self, room_id, from, limit, direction).await
    }

    async fn get_events_batch(
        &self,
        event_ids: &[String],
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        self.get_events_batch(event_ids).await
    }

    async fn get_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<StateEvent>, sqlx::Error> {
        self.get_state_event(room_id, event_type, state_key).await
    }

    async fn get_state_events(
        &self,
        room_id: &str,
    ) -> Result<Vec<StateEvent>, sqlx::Error> {
        self.get_state_events(room_id).await
    }

    async fn get_state_events_batch(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error> {
        self.get_state_events_batch(room_ids).await
    }

    async fn get_room_events_paginated_with_filter(
        &self,
        room_id: &str,
        from: Option<&str>,
        to: Option<&str>,
        limit: i64,
        filter: Option<&EventQueryFilter>,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        if to.is_some() {
            tracing::warn!("EventRepository::get_room_events_paginated_with_filter: 'to' parameter not yet supported");
        }
        if filter.is_some() {
            tracing::warn!("EventRepository::get_room_events_paginated_with_filter: 'filter' parameter not yet supported");
        }
        self.get_room_events_paginated_with_filter(room_id, from, to, limit, filter).await
    }

    async fn get_room_create_event(
        &self,
        room_id: &str,
    ) -> Result<Option<RoomEvent>, sqlx::Error> {
        self.get_room_create_event(room_id).await
    }

    async fn count_room_events(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        self.count_room_events(room_id).await
    }

    async fn search_postgres_messages(
        &self,
        room_id: &str,
        search_term: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        self.search_room_postgres_messages(room_id, search_term, limit).await
    }
}
