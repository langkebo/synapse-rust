use async_trait::async_trait;

use super::models::{CreateEventParams, EventQueryFilter, RoomEvent, StateEvent};

#[async_trait]
pub trait EventRepository: Send + Sync {
    async fn get_event(&self, event_id: &str) -> Result<Option<RoomEvent>, sqlx::Error>;

    async fn create_event(&self, params: &CreateEventParams) -> Result<RoomEvent, sqlx::Error>;

    async fn get_room_events_paginated(
        &self,
        room_id: &str,
        limit: i64,
        from: Option<i64>,
        to: Option<i64>,
        dir: Option<&str>,
        filter: Option<&EventQueryFilter>,
    ) -> Result<Vec<RoomEvent>, sqlx::Error>;

    async fn get_events_batch(
        &self,
        event_ids: &[String],
    ) -> Result<Vec<RoomEvent>, sqlx::Error>;

    async fn get_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<StateEvent>, sqlx::Error>;

    async fn get_state_events(
        &self,
        room_id: &str,
    ) -> Result<Vec<StateEvent>, sqlx::Error>;

    async fn get_state_events_batch(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error>;

    async fn get_room_events_paginated_with_filter(
        &self,
        room_id: &str,
        from: Option<&str>,
        to: Option<&str>,
        limit: i64,
        filter: Option<&EventQueryFilter>,
    ) -> Result<Vec<RoomEvent>, sqlx::Error>;

    async fn get_room_create_event(
        &self,
        room_id: &str,
    ) -> Result<Option<RoomEvent>, sqlx::Error>;

    async fn count_room_events(&self, room_id: &str) -> Result<i64, sqlx::Error>;

    async fn search_postgres_messages(
        &self,
        room_id: &str,
        search_term: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error>;
}
