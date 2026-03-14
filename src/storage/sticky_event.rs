// Sticky Event Storage - MSC4354
// Stores sticky event metadata for rooms
// Following project field naming standards

use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct StickyEventStorage {
    pool: Arc<PgPool>,
}

impl StickyEventStorage {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Set sticky event metadata for a room
    pub async fn set_sticky_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        event_type: &str,
        sticky: bool,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO room_sticky_events (room_id, user_id, event_id, event_type, sticky, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (room_id, user_id, event_type) 
            DO UPDATE SET event_id = EXCLUDED.event_id, sticky = EXCLUDED.sticky, updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(event_id)
        .bind(event_type)
        .bind(sticky)
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Get sticky event for a room and user by event type
    pub async fn get_sticky_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
    ) -> Result<Option<StickyEvent>, sqlx::Error> {
        let result = sqlx::query_as::<_, (String, String, String, String, bool, i64, i64)>(
            r#"
            SELECT room_id, user_id, event_id, event_type, sticky, created_ts, updated_ts
            FROM room_sticky_events 
            WHERE room_id = $1 AND user_id = $2 AND event_type = $3 AND sticky = true
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(event_type)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(
            |(room_id, user_id, event_id, event_type, sticky, created_ts, updated_ts)| StickyEvent {
                room_id,
                user_id,
                event_id,
                event_type,
                sticky,
                created_ts,
                updated_ts,
            },
        ))
    }

    /// Get all sticky events for a room and user
    pub async fn get_all_sticky_events(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Vec<StickyEvent>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, String, String, bool, i64, i64)>(
            r#"
            SELECT room_id, user_id, event_id, event_type, sticky, created_ts, updated_ts
            FROM room_sticky_events 
            WHERE room_id = $1 AND user_id = $2 AND sticky = true
            ORDER BY event_type
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(room_id, user_id, event_id, event_type, sticky, created_ts, updated_ts)| StickyEvent {
                    room_id,
                    user_id,
                    event_id,
                    event_type,
                    sticky,
                    created_ts,
                    updated_ts,
                },
            )
            .collect())
    }

    /// Clear sticky event for a room, user, and event type
    pub async fn clear_sticky_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"
            UPDATE room_sticky_events 
            SET sticky = false, updated_ts = $4
            WHERE room_id = $1 AND user_id = $2 AND event_type = $3
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(event_type)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Get rooms with sticky events for a user (for sync)
    pub async fn get_rooms_with_sticky_events(
        &self,
        user_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT DISTINCT room_id FROM room_sticky_events 
            WHERE user_id = $1 AND sticky = true
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }
}

/// Sticky Event - Following project field naming standards
/// - created_ts: NOT NULL, milliseconds timestamp
/// - updated_ts: NOT NULL (or NULLABLE if appropriate), milliseconds timestamp
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StickyEvent {
    pub room_id: String,
    pub user_id: String,
    pub event_id: String,
    pub event_type: String,
    pub sticky: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sticky_event_struct() {
        let event = StickyEvent {
            room_id: "!room:localhost".to_string(),
            user_id: "@user:localhost".to_string(),
            event_id: "$event:localhost".to_string(),
            event_type: "m.room.message".to_string(),
            sticky: true,
            created_ts: 1700000000000i64,
            updated_ts: 1700000000000i64,
        };

        assert_eq!(event.room_id, "!room:localhost");
        assert!(event.sticky);
    }

    #[test]
    fn test_event_type_validation() {
        let valid_types = vec![
            "m.room.message",
            "m.room.topic",
            "m.room.avatar",
        ];

        for event_type in valid_types {
            assert!(event_type.starts_with("m.") || event_type.starts_with("com."));
        }
    }
}
