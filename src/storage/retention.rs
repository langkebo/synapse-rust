use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomRetentionPolicy {
    pub id: i64,
    pub room_id: String,
    pub max_lifetime: Option<i64>,
    pub min_lifetime: i64,
    pub expire_on_clients: bool,
    pub is_server_default: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServerRetentionPolicy {
    pub id: i64,
    pub max_lifetime: Option<i64>,
    pub min_lifetime: i64,
    pub expire_on_clients: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RetentionCleanupQueueItem {
    pub id: i64,
    pub room_id: String,
    pub event_id: Option<String>,
    pub event_type: Option<String>,
    pub origin_server_ts: i64,
    pub scheduled_ts: i64,
    pub status: String,
    pub created_ts: i64,
    pub processed_ts: Option<i64>,
    pub error_message: Option<String>,
    pub retry_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RetentionCleanupLog {
    pub id: i64,
    pub room_id: String,
    pub events_deleted: i64,
    pub state_events_deleted: i64,
    pub media_deleted: i64,
    pub bytes_freed: i64,
    pub started_ts: i64,
    pub completed_ts: Option<i64>,
    pub status: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DeletedEventIndex {
    pub id: i64,
    pub room_id: String,
    pub event_id: String,
    pub deletion_ts: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RetentionStats {
    pub id: i64,
    pub room_id: String,
    pub total_events: i64,
    pub events_in_retention: i64,
    pub events_expired: i64,
    pub last_cleanup_ts: Option<i64>,
    pub next_cleanup_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomRetentionPolicyRequest {
    pub room_id: String,
    pub max_lifetime: Option<i64>,
    pub min_lifetime: Option<i64>,
    pub expire_on_clients: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateRoomRetentionPolicyRequest {
    pub max_lifetime: Option<i64>,
    pub min_lifetime: Option<i64>,
    pub expire_on_clients: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateServerRetentionPolicyRequest {
    pub max_lifetime: Option<i64>,
    pub min_lifetime: Option<i64>,
    pub expire_on_clients: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveRetentionPolicy {
    pub max_lifetime: Option<i64>,
    pub min_lifetime: i64,
    pub expire_on_clients: bool,
}

#[derive(Clone)]
pub struct RetentionStorage {
    pool: Arc<PgPool>,
}

impl RetentionStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_room_policy(
        &self,
        request: CreateRoomRetentionPolicyRequest,
    ) -> Result<RoomRetentionPolicy, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, RoomRetentionPolicy>(
            r#"
            INSERT INTO room_retention_policies (
                room_id, max_lifetime, min_lifetime, expire_on_clients, is_server_default, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, FALSE, $5, $5)
            ON CONFLICT (room_id) DO UPDATE SET
                max_lifetime = EXCLUDED.max_lifetime,
                min_lifetime = EXCLUDED.min_lifetime,
                expire_on_clients = EXCLUDED.expire_on_clients,
                updated_ts = EXCLUDED.updated_ts
            RETURNING id, room_id, max_lifetime, min_lifetime, expire_on_clients, is_server_default, created_ts, updated_ts
            "#,
        )
        .bind(&request.room_id)
        .bind(request.max_lifetime)
        .bind(request.min_lifetime.unwrap_or(0))
        .bind(request.expire_on_clients.unwrap_or(false))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_room_policy(
        &self,
        room_id: &str,
    ) -> Result<Option<RoomRetentionPolicy>, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomRetentionPolicy>(
            "SELECT id, room_id, max_lifetime, min_lifetime, expire_on_clients, is_server_default, created_ts, updated_ts FROM room_retention_policies WHERE room_id = $1",
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn update_room_policy(
        &self,
        room_id: &str,
        request: UpdateRoomRetentionPolicyRequest,
    ) -> Result<RoomRetentionPolicy, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomRetentionPolicy>(
            r#"
            UPDATE room_retention_policies SET
                max_lifetime = COALESCE($2, max_lifetime),
                min_lifetime = COALESCE($3, min_lifetime),
                expire_on_clients = COALESCE($4, expire_on_clients)
            WHERE room_id = $1
            RETURNING *
            "#,
        )
        .bind(room_id)
        .bind(request.max_lifetime)
        .bind(request.min_lifetime)
        .bind(request.expire_on_clients)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_room_policy(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM room_retention_policies WHERE room_id = $1")
            .bind(room_id)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn get_server_policy(&self) -> Result<ServerRetentionPolicy, sqlx::Error> {
        let row = sqlx::query_as::<_, ServerRetentionPolicy>(
            "SELECT id, max_lifetime, min_lifetime, expire_on_clients, created_ts, updated_ts FROM server_retention_policy ORDER BY id LIMIT 1",
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn update_server_policy(
        &self,
        request: UpdateServerRetentionPolicyRequest,
    ) -> Result<ServerRetentionPolicy, sqlx::Error> {
        let row = sqlx::query_as::<_, ServerRetentionPolicy>(
            r#"
            UPDATE server_retention_policy SET
                max_lifetime = COALESCE($1, max_lifetime),
                min_lifetime = COALESCE($2, min_lifetime),
                expire_on_clients = COALESCE($3, expire_on_clients)
            WHERE id = (SELECT MIN(id) FROM server_retention_policy)
            RETURNING id, max_lifetime, min_lifetime, expire_on_clients, created_ts, updated_ts
            "#,
        )
        .bind(request.max_lifetime)
        .bind(request.min_lifetime)
        .bind(request.expire_on_clients)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_effective_policy(
        &self,
        room_id: &str,
    ) -> Result<EffectiveRetentionPolicy, sqlx::Error> {
        let room_policy = self.get_room_policy(room_id).await?;
        let server_policy = self.get_server_policy().await?;

        Ok(EffectiveRetentionPolicy {
            max_lifetime: room_policy
                .as_ref()
                .and_then(|p| p.max_lifetime)
                .or(server_policy.max_lifetime),
            min_lifetime: room_policy
                .as_ref()
                .map(|p| p.min_lifetime)
                .unwrap_or(server_policy.min_lifetime),
            expire_on_clients: room_policy
                .as_ref()
                .map(|p| p.expire_on_clients)
                .unwrap_or(server_policy.expire_on_clients),
        })
    }

    pub async fn delete_events_before(
        &self,
        room_id: &str,
        cutoff_ts: i64,
    ) -> Result<i64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM events 
            WHERE room_id = $1 
            AND origin_server_ts < $2
            AND event_type NOT IN ('m.room.create', 'm.room.power_levels', 'm.room.join_rules', 'm.room.history_visibility')
            AND state_key IS NULL
            "#,
        )
        .bind(room_id)
        .bind(cutoff_ts)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn get_rooms_with_policies(&self) -> Result<Vec<RoomRetentionPolicy>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoomRetentionPolicy>(
            "SELECT id, room_id, max_lifetime, min_lifetime, expire_on_clients, is_server_default, created_ts, updated_ts FROM room_retention_policies ORDER BY room_id",
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_retention_policy_creation() {
        let policy = RoomRetentionPolicy {
            id: 1,
            room_id: "!room:example.com".to_string(),
            max_lifetime: Some(86400000),
            min_lifetime: 0,
            expire_on_clients: true,
            is_server_default: false,
            created_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert_eq!(policy.room_id, "!room:example.com");
        assert!(policy.max_lifetime.is_some());
    }

    #[test]
    fn test_room_retention_policy_max_lifetime_none() {
        let policy = RoomRetentionPolicy {
            id: 2,
            room_id: "!room2:example.com".to_string(),
            max_lifetime: None,
            min_lifetime: 0,
            expire_on_clients: false,
            is_server_default: false,
            created_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert!(policy.max_lifetime.is_none());
    }

    #[test]
    fn test_server_retention_policy_defaults() {
        let policy = ServerRetentionPolicy {
            id: 1,
            max_lifetime: Some(2592000000),
            min_lifetime: 0,
            expire_on_clients: true,
            created_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert!(policy.max_lifetime.is_some());
    }

    #[test]
    fn test_retention_cleanup_log() {
        let log = RetentionCleanupLog {
            id: 1,
            room_id: "!room:example.com".to_string(),
            events_deleted: 100,
            state_events_deleted: 10,
            media_deleted: 50,
            bytes_freed: 1048576,
            started_ts: 1234567890,
            completed_ts: Some(1234567990),
            status: "completed".to_string(),
            error_message: None,
        };
        assert_eq!(log.events_deleted, 100);
        assert!(log.completed_ts.is_some());
    }

    #[test]
    fn test_create_room_retention_policy_request() {
        let request = CreateRoomRetentionPolicyRequest {
            room_id: "!room:example.com".to_string(),
            max_lifetime: Some(86400000),
            min_lifetime: Some(0),
            expire_on_clients: Some(true),
        };
        assert_eq!(request.room_id, "!room:example.com");
    }

    #[test]
    fn test_effective_retention_policy() {
        let policy = EffectiveRetentionPolicy {
            max_lifetime: Some(86400000),
            min_lifetime: 0,
            expire_on_clients: true,
        };
        assert_eq!(policy.max_lifetime, Some(86400000));
        assert!(policy.expire_on_clients);
    }

    #[test]
    fn test_effective_retention_policy_no_max_lifetime() {
        let policy = EffectiveRetentionPolicy {
            max_lifetime: None,
            min_lifetime: 0,
            expire_on_clients: false,
        };
        assert!(policy.max_lifetime.is_none());
    }
}
