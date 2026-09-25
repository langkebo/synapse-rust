//! Depth calculation methods for v12+ event creation.
//!
//! Implements the logic to compute `depth` for new events based on their
//! `prev_events`. The depth is calculated as `max(prev_events.depth) + 1`.

use super::EventStorage;

impl EventStorage {
    /// Calculate the depth for a new event based on its `prev_events`.
    ///
    /// The depth is computed as `max(prev_events.depth) + 1`. If `prev_events`
    /// is empty (room creation), returns `1`.
    ///
    /// This method is used by the v12+ event creation path to populate the
    /// `depth` field before writing to the database.
    ///
    /// # Arguments
    ///
    /// * `room_id` - The room where the event will be created
    /// * `prev_events` - The list of prev_event IDs (forward extremities)
    ///
    /// # Returns
    ///
    /// The calculated depth value, or an error if the database query fails.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let prev_events = storage.get_forward_extremities_in_room(&room_id, 10).await?;
    /// let depth = storage.calculate_event_depth(&room_id, &prev_events).await?;
    /// // depth = max(prev_events.depths) + 1
    /// ```
    pub async fn calculate_event_depth(&self, room_id: &str, prev_events: &[String]) -> Result<i64, sqlx::Error> {
        if prev_events.is_empty() {
            // Room creation: first event has depth 1
            return Ok(1);
        }

        // Query the maximum depth among prev_events
        // COALESCE ensures we get 0 if no prev_events have depth set
        let max_depth: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(MAX(depth), 0) FROM events
            WHERE room_id = $1
              AND event_id = ANY($2)
            "#,
        )
        .bind(room_id)
        .bind(prev_events)
        .fetch_one(&*self.pool)
        .await?;

        Ok(max_depth + 1)
    }

    /// Batch version: calculate depths for multiple rooms at once.
    ///
    /// More efficient than calling [`calculate_event_depth`] repeatedly when
    /// creating events in multiple rooms in the same transaction.
    ///
    /// Returns a map from `room_id` to the calculated depth for that room's
    /// next event.
    pub async fn calculate_event_depths_batch(
        &self,
        room_ids: &[String],
        prev_events_map: &std::collections::HashMap<String, Vec<String>>,
    ) -> Result<std::collections::HashMap<String, i64>, sqlx::Error> {
        let mut result = std::collections::HashMap::new();

        for room_id in room_ids {
            let prev_events = prev_events_map.get(room_id).map(|v| v.as_slice()).unwrap_or(&[]);
            let depth = self.calculate_event_depth(room_id, prev_events).await?;
            result.insert(room_id.clone(), depth);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use crate::event::models::{CreateEventParams, PduGraphFields};
    use crate::test_isolation::isolated_test_pool;
    use sqlx::Pool;
    use std::sync::Arc;
    use synapse_common::current_timestamp_millis;
    use uuid::Uuid;

    fn test_server_name() -> String {
        "example.com".to_string()
    }

    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<sqlx::PgPool>) {
        let isolated = isolated_test_pool().await.expect("isolated test pool");
        let pool = isolated.pool();
        (isolated, pool)
    }

    async fn ensure_test_room(pool: &Pool<sqlx::Postgres>, room_id: &str) {
        let now = current_timestamp_millis();
        sqlx::query(
            r#"INSERT INTO rooms (room_id, creator, join_rules, room_version, is_public, history_visibility, created_ts, last_activity_ts)
               VALUES ($1, '@test:example.com', 'invite', '12', false, 'joined', $2, $2)
               ON CONFLICT (room_id) DO NOTHING"#,
        )
        .bind(room_id)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test room");
    }

    #[tokio::test]
    async fn test_calculate_event_depth_for_room_creation() {
        let (_isolated, pool) = test_pool().await;
        let storage = EventStorage::new(&pool, test_server_name());

        // Empty prev_events should return depth 1
        let depth = storage.calculate_event_depth("!test:example.com", &[]).await.expect("calculate_event_depth");

        assert_eq!(depth, 1);
    }

    #[tokio::test]
    async fn test_calculate_event_depth_with_existing_events() {
        let (_isolated, pool) = test_pool().await;
        let storage = EventStorage::new(&pool, test_server_name());

        // Create some events with known depths
        let room_id = format!("!test_{}:example.com", Uuid::new_v4());

        // Ensure the room exists
        ensure_test_room(&pool, &room_id).await;

        let params1 = CreateEventParams {
            event_id: format!("$evt1:{}", Uuid::new_v4()),
            room_id: room_id.clone(),
            user_id: "@creator:test".to_string(),
            event_type: "m.room.create".to_string(),
            content: serde_json::json!({"creator": "@creator:test"}),
            state_key: Some("".to_string()),
            origin_server_ts: 1000,
            redacts: None,
        };

        let pdu_graph1 = PduGraphFields { depth: Some(1), prev_events: Some(vec![]), auth_events: Some(vec![]) };

        storage.create_event_with_pdu(params1.clone(), pdu_graph1, None).await.expect("create_event");

        // Event 2: depth 2
        let params2 = CreateEventParams {
            event_id: format!("$evt2:{}", Uuid::new_v4()),
            room_id: room_id.clone(),
            user_id: "@user1:test".to_string(),
            event_type: "m.room.member".to_string(),
            content: serde_json::json!({"membership": "join"}),
            state_key: Some("@user1:test".to_string()),
            origin_server_ts: 2000,
            redacts: None,
        };

        let pdu_graph2 = PduGraphFields {
            depth: Some(2),
            prev_events: Some(vec![params1.event_id.clone()]),
            auth_events: Some(vec![params1.event_id.clone()]),
        };

        storage.create_event_with_pdu(params2.clone(), pdu_graph2, None).await.expect("create_event");

        // Calculate depth for next event
        let depth = storage.calculate_event_depth(&room_id, &[params2.event_id]).await.expect("calculate_event_depth");

        assert_eq!(depth, 3);
    }

    #[tokio::test]
    async fn test_calculate_event_depth_with_multiple_prev_events() {
        let (_isolated, pool) = test_pool().await;
        let storage = EventStorage::new(&pool, test_server_name());

        let room_id = format!("!test_{}:example.com", Uuid::new_v4());

        // Ensure the room exists
        ensure_test_room(&pool, &room_id).await;

        // Create two events with different depths
        let evt1_id = format!("$evt1:{}", Uuid::new_v4());
        let evt2_id = format!("$evt2:{}", Uuid::new_v4());

        // Event 1: depth 1
        let params1 = CreateEventParams {
            event_id: evt1_id.clone(),
            room_id: room_id.clone(),
            user_id: "@creator:test".to_string(),
            event_type: "m.room.create".to_string(),
            content: serde_json::json!({}),
            state_key: Some("".to_string()),
            origin_server_ts: 1000,
            redacts: None,
        };

        storage
            .create_event_with_pdu(
                params1,
                PduGraphFields { depth: Some(1), prev_events: Some(vec![]), auth_events: Some(vec![]) },
                None,
            )
            .await
            .expect("create_event");

        // Event 2: depth 5
        let params2 = CreateEventParams {
            event_id: evt2_id.clone(),
            room_id: room_id.clone(),
            user_id: "@user1:test".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({}),
            state_key: None,
            origin_server_ts: 2000,
            redacts: None,
        };

        storage
            .create_event_with_pdu(
                params2,
                PduGraphFields {
                    depth: Some(5),
                    prev_events: Some(vec![evt1_id.clone()]),
                    auth_events: Some(vec![evt1_id.clone()]),
                },
                None,
            )
            .await
            .expect("create_event");

        // Next event references both: depth should be max(1, 5) + 1 = 6
        let depth = storage
            .calculate_event_depth(&room_id, &[evt1_id.clone(), evt2_id.clone()])
            .await
            .expect("calculate_event_depth");

        assert_eq!(depth, 6);
    }
}
