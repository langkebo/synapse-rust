use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use std::sync::Arc;

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
    pub offset: i64,
}

#[derive(Clone)]
pub struct AuditEventStorage {
    pool: Arc<PgPool>,
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
            r#"
            SELECT event_id, actor_id, action, resource_type, resource_id, result, request_id, details, created_ts
            FROM audit_events
            WHERE event_id = $1
            "#,
        )
        .bind(event_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn list_events(
        &self,
        filters: &AuditEventFilters,
    ) -> Result<(Vec<AuditEvent>, i64), sqlx::Error> {
        let actor_id = filters.actor_id.clone();
        let action = filters.action.clone();
        let resource_type = filters.resource_type.clone();
        let resource_id = filters.resource_id.clone();
        let result = filters.result.clone();

        let mut count_query =
            QueryBuilder::<Postgres>::new("SELECT COUNT(*)::BIGINT FROM audit_events WHERE 1=1");
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
        let total = count_query
            .build_query_scalar::<i64>()
            .fetch_one(&*self.pool)
            .await?;

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
        query.push(" ORDER BY created_ts DESC, event_id DESC LIMIT ");
        query.push_bind(filters.limit);
        query.push(" OFFSET ");
        query.push_bind(filters.offset);

        let events = query
            .build_query_as::<AuditEvent>()
            .fetch_all(&*self.pool)
            .await?;

        Ok((events, total))
    }

    pub async fn delete_events_before(&self, cutoff_ts: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM audit_events
            WHERE created_ts < $1
            "#,
        )
        .bind(cutoff_ts)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
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
        r#"
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
        "#,
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
