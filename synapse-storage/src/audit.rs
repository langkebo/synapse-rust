use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEventCursor {
    pub created_ts: i64,
    pub event_id: String,
}

pub fn encode_audit_event_cursor(cursor: &AuditEventCursor) -> String {
    format!("{}|{}", cursor.created_ts, cursor.event_id)
}

pub fn decode_audit_event_cursor(cursor: Option<&str>) -> Option<AuditEventCursor> {
    let cursor = cursor?;
    let (created_ts, event_id) = cursor.split_once('|')?;
    let created_ts = created_ts.parse::<i64>().ok()?;
    if event_id.is_empty() {
        return None;
    }
    Some(AuditEventCursor { created_ts, event_id: event_id.to_string() })
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuditEvent {
    pub event_id: String,
    pub actor_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub result: String,
    pub request_id: String,
    pub details: Value,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAuditEventRequest {
    pub actor_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub result: String,
    pub request_id: String,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct AuditEventFilters {
    pub actor_id: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub result: Option<String>,
    pub limit: i64,
    pub from: Option<AuditEventCursor>,
}

#[derive(Clone)]
pub struct AuditEventStorage {
    pool: Arc<PgPool>,
}

#[async_trait]
pub trait AuditEventStoreApi: Send + Sync {
    async fn create_event(
        &self,
        event_id: &str,
        created_ts: i64,
        request: &CreateAuditEventRequest,
    ) -> Result<AuditEvent, sqlx::Error>;

    async fn get_event(&self, event_id: &str) -> Result<Option<AuditEvent>, sqlx::Error>;

    async fn list_events(
        &self,
        filters: &AuditEventFilters,
    ) -> Result<(Vec<AuditEvent>, i64, Option<String>), sqlx::Error>;

    async fn delete_events_before(&self, cutoff_ts: i64) -> Result<u64, sqlx::Error>;
}

impl AuditEventStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_event(
        &self,
        event_id: &str,
        created_ts: i64,
        request: &CreateAuditEventRequest,
    ) -> Result<AuditEvent, sqlx::Error> {
        insert_audit_event(&*self.pool, event_id, created_ts, request).await
    }

    pub async fn get_event(&self, event_id: &str) -> Result<Option<AuditEvent>, sqlx::Error> {
        sqlx::query_as::<_, AuditEvent>(
            r"
            SELECT event_id, actor_id, action, resource_type, resource_id, result, request_id, details, created_ts
            FROM audit_events
            WHERE event_id = $1
            ",
        )
        .bind(event_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn list_events(
        &self,
        filters: &AuditEventFilters,
    ) -> Result<(Vec<AuditEvent>, i64, Option<String>), sqlx::Error> {
        let actor_id = filters.actor_id.clone();
        let action = filters.action.clone();
        let resource_type = filters.resource_type.clone();
        let resource_id = filters.resource_id.clone();
        let result = filters.result.clone();

        let mut count_query = QueryBuilder::<Postgres>::new("SELECT COUNT(*)::BIGINT FROM audit_events WHERE 1=1");
        if let Some(ref v) = actor_id {
            count_query.push(" AND actor_id = ");
            count_query.push_bind(v.clone());
        }
        if let Some(ref v) = action {
            count_query.push(" AND action = ");
            count_query.push_bind(v.clone());
        }
        if let Some(ref v) = resource_type {
            count_query.push(" AND resource_type = ");
            count_query.push_bind(v.clone());
        }
        if let Some(ref v) = resource_id {
            count_query.push(" AND resource_id = ");
            count_query.push_bind(v.clone());
        }
        if let Some(ref v) = result {
            count_query.push(" AND result = ");
            count_query.push_bind(v.clone());
        }
        let total = count_query.build_query_scalar::<i64>().fetch_one(&*self.pool).await?;

        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT event_id, actor_id, action, resource_type, resource_id, result, request_id, details, created_ts FROM audit_events WHERE 1=1",
        );
        if let Some(ref v) = actor_id {
            query.push(" AND actor_id = ");
            query.push_bind(v.clone());
        }
        if let Some(ref v) = action {
            query.push(" AND action = ");
            query.push_bind(v.clone());
        }
        if let Some(ref v) = resource_type {
            query.push(" AND resource_type = ");
            query.push_bind(v.clone());
        }
        if let Some(ref v) = resource_id {
            query.push(" AND resource_id = ");
            query.push_bind(v.clone());
        }
        if let Some(ref v) = result {
            query.push(" AND result = ");
            query.push_bind(v.clone());
        }
        if let Some(ref cursor) = filters.from {
            query.push(" AND (created_ts, event_id) < (");
            query.push_bind(cursor.created_ts);
            query.push(", ");
            query.push_bind(cursor.event_id.clone());
            query.push(")");
        }
        query.push(" ORDER BY created_ts DESC, event_id DESC LIMIT ");
        query.push_bind(filters.limit + 1);

        let events = query.build_query_as::<AuditEvent>().fetch_all(&*self.pool).await?;

        let next_batch = if events.len() > filters.limit as usize {
            events.get(filters.limit as usize).map(|event| {
                encode_audit_event_cursor(&AuditEventCursor {
                    created_ts: event.created_ts,
                    event_id: event.event_id.clone(),
                })
            })
        } else {
            None
        };

        let events = events.into_iter().take(filters.limit as usize).collect();

        Ok((events, total, next_batch))
    }

    pub async fn delete_events_before(&self, cutoff_ts: i64) -> Result<u64, sqlx::Error> {
        // Wrap in a transaction so that set_config (is_local=true) applies to
        // the DELETE statement and bypasses the append-only trigger guard.
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT set_config('synapse.allow_audit_delete', 'true', true)")
            .execute(&mut *tx)
            .await?;
        let result = sqlx::query("DELETE FROM audit_events WHERE created_ts < $1")
            .bind(cutoff_ts)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result.rows_affected())
    }
}

#[async_trait]
impl AuditEventStoreApi for AuditEventStorage {
    async fn create_event(
        &self,
        event_id: &str,
        created_ts: i64,
        request: &CreateAuditEventRequest,
    ) -> Result<AuditEvent, sqlx::Error> {
        self.create_event(event_id, created_ts, request).await
    }

    async fn get_event(&self, event_id: &str) -> Result<Option<AuditEvent>, sqlx::Error> {
        self.get_event(event_id).await
    }

    async fn list_events(
        &self,
        filters: &AuditEventFilters,
    ) -> Result<(Vec<AuditEvent>, i64, Option<String>), sqlx::Error> {
        self.list_events(filters).await
    }

    async fn delete_events_before(&self, cutoff_ts: i64) -> Result<u64, sqlx::Error> {
        self.delete_events_before(cutoff_ts).await
    }
}

async fn insert_audit_event<'e, E>(
    executor: E,
    event_id: &str,
    created_ts: i64,
    request: &CreateAuditEventRequest,
) -> Result<AuditEvent, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, AuditEvent>(
        r"
        INSERT INTO audit_events (
            event_id,
            actor_id,
            action,
            resource_type,
            resource_id,
            result,
            request_id,
            details,
            created_ts
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING event_id, actor_id, action, resource_type, resource_id, result, request_id, details, created_ts
        ",
    )
    .bind(event_id)
    .bind(&request.actor_id)
    .bind(&request.action)
    .bind(&request.resource_type)
    .bind(&request.resource_id)
    .bind(&request.result)
    .bind(&request.request_id)
    .bind(request.details.clone().unwrap_or_else(|| serde_json::json!({})))
    .bind(created_ts)
    .fetch_one(executor)
    .await
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_audit_event_cursor, encode_audit_event_cursor, AuditEventCursor};

    #[test]
    fn audit_event_cursor_round_trip() {
        let cursor = AuditEventCursor { created_ts: 1_746_700_000_000, event_id: "evt-123".to_string() };

        let encoded = encode_audit_event_cursor(&cursor);
        assert_eq!(decode_audit_event_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn audit_event_cursor_rejects_invalid_values() {
        assert_eq!(decode_audit_event_cursor(None), None);
        assert_eq!(decode_audit_event_cursor(Some("bad")), None);
        assert_eq!(decode_audit_event_cursor(Some("123|")), None);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    fn sample_request(event_id: &str) -> CreateAuditEventRequest {
        CreateAuditEventRequest {
            actor_id: format!("@user:test_{}", Uuid::new_v4()),
            action: "delete_test".to_string(),
            resource_type: "event".to_string(),
            resource_id: event_id.to_string(),
            result: "success".to_string(),
            request_id: Uuid::new_v4().to_string(),
            details: None,
        }
    }

    #[tokio::test]
    async fn test_delete_events_before_bypasses_append_only_guard() {
        let pool = test_pool().await;
        let storage = AuditEventStorage::new(&pool);

        let event_id = Uuid::new_v4().to_string();
        let ts = chrono::Utc::now().timestamp_millis();

        // Insert a test event
        let event = storage
            .create_event(&event_id, ts - 10_000, &sample_request(&event_id))
            .await
            .expect("create_event should succeed");

        assert_eq!(event.event_id, event_id);

        // delete_events_before must bypass the append-only trigger by setting
        // synapse.allow_audit_delete within its transaction.
        let deleted = storage
            .delete_events_before(ts)
            .await
            .expect("delete_events_before should succeed (bypasses trigger guard)");

        // At least the one event we just inserted should be deleted.
        assert!(deleted >= 1, "should have deleted at least the test event");

        // Verify the event is gone
        let found = storage.get_event(&event_id).await.expect("get_event should succeed");
        assert!(found.is_none(), "deleted event should not be found");
    }
}
