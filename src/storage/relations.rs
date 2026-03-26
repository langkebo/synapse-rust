use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EventRelation {
    pub id: i64,
    pub room_id: String,
    pub event_id: String,
    pub relates_to_event_id: String,
    pub relation_type: String,
    pub sender: String,
    pub origin_server_ts: i64,
    pub content: serde_json::Value,
    pub is_redacted: bool,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRelationParams {
    pub room_id: String,
    pub event_id: String,
    pub relates_to_event_id: String,
    pub relation_type: String,
    pub sender: String,
    pub origin_server_ts: i64,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationQueryParams {
    pub room_id: String,
    pub relates_to_event_id: String,
    pub relation_type: Option<String>,
    pub limit: Option<i32>,
    pub from: Option<String>,
    pub direction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AggregationResult {
    pub relation_type: String,
    pub key: Option<String>,
    pub count: i64,
    pub sender: Option<String>,
}

#[derive(Clone)]
pub struct RelationsStorage {
    pub pool: Arc<Pool<Postgres>>,
}

impl RelationsStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_relation(
        &self,
        params: CreateRelationParams,
    ) -> Result<EventRelation, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as::<_, EventRelation>(
            r#"
            INSERT INTO event_relations (
                room_id, event_id, relates_to_event_id, relation_type,
                sender, origin_server_ts, content, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (event_id, relation_type, sender) DO UPDATE SET
                content = EXCLUDED.content,
                origin_server_ts = EXCLUDED.origin_server_ts,
                is_redacted = FALSE
            RETURNING id, room_id, event_id, relates_to_event_id, relation_type,
                      sender, origin_server_ts, content, is_redacted, created_ts
            "#,
        )
        .bind(&params.room_id)
        .bind(&params.event_id)
        .bind(&params.relates_to_event_id)
        .bind(&params.relation_type)
        .bind(&params.sender)
        .bind(params.origin_server_ts)
        .bind(&params.content)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_relation(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> Result<Option<EventRelation>, sqlx::Error> {
        sqlx::query_as::<_, EventRelation>(
            r#"
            SELECT id, room_id, event_id, relates_to_event_id, relation_type,
                   sender, origin_server_ts, content, is_redacted, created_ts
            FROM event_relations
            WHERE room_id = $1 AND event_id = $2 AND is_redacted = FALSE
            "#,
        )
        .bind(room_id)
        .bind(event_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_relations(
        &self,
        params: RelationQueryParams,
    ) -> Result<Vec<EventRelation>, sqlx::Error> {
        let limit = params.limit.unwrap_or(50).min(100);
        let direction = params.direction.as_deref().unwrap_or("f");

        let query = match direction {
            "b" => {
                let from = params.from.unwrap_or_default();
                sqlx::query_as::<_, EventRelation>(
                    r#"
                    SELECT id, room_id, event_id, relates_to_event_id, relation_type,
                           sender, origin_server_ts, content, is_redacted, created_ts
                    FROM event_relations
                    WHERE room_id = $1 AND relates_to_event_id = $2
                      AND ($3::text IS NULL OR relation_type = $3)
                      AND ($4::text = '' OR event_id < $4)
                      AND is_redacted = FALSE
                    ORDER BY origin_server_ts DESC, event_id DESC
                    LIMIT $5
                    "#,
                )
                .bind(&params.room_id)
                .bind(&params.relates_to_event_id)
                .bind(&params.relation_type)
                .bind(&from)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await
            }
            _ => {
                let from = params.from.unwrap_or_default();
                sqlx::query_as::<_, EventRelation>(
                    r#"
                    SELECT id, room_id, event_id, relates_to_event_id, relation_type,
                           sender, origin_server_ts, content, is_redacted, created_ts
                    FROM event_relations
                    WHERE room_id = $1 AND relates_to_event_id = $2
                      AND ($3::text IS NULL OR relation_type = $3)
                      AND ($4::text = '' OR event_id > $4)
                      AND is_redacted = FALSE
                    ORDER BY origin_server_ts ASC, event_id ASC
                    LIMIT $5
                    "#,
                )
                .bind(&params.room_id)
                .bind(&params.relates_to_event_id)
                .bind(&params.relation_type)
                .bind(&from)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await
            }
        };

        query
    }

    pub async fn get_annotations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        limit: Option<i32>,
    ) -> Result<Vec<EventRelation>, sqlx::Error> {
        let limit = limit.unwrap_or(50).min(100);

        sqlx::query_as::<_, EventRelation>(
            r#"
            SELECT id, room_id, event_id, relates_to_event_id, relation_type,
                   sender, origin_server_ts, content, is_redacted, created_ts
            FROM event_relations
            WHERE room_id = $1 AND relates_to_event_id = $2
              AND relation_type = 'm.annotation'
              AND is_redacted = FALSE
            ORDER BY origin_server_ts DESC
            LIMIT $3
            "#,
        )
        .bind(room_id)
        .bind(relates_to_event_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_references(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        limit: Option<i32>,
    ) -> Result<Vec<EventRelation>, sqlx::Error> {
        let limit = limit.unwrap_or(50).min(100);

        sqlx::query_as::<_, EventRelation>(
            r#"
            SELECT id, room_id, event_id, relates_to_event_id, relation_type,
                   sender, origin_server_ts, content, is_redacted, created_ts
            FROM event_relations
            WHERE room_id = $1 AND relates_to_event_id = $2
              AND relation_type = 'm.reference'
              AND is_redacted = FALSE
            ORDER BY origin_server_ts DESC
            LIMIT $3
            "#,
        )
        .bind(room_id)
        .bind(relates_to_event_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_replacement(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        sender: &str,
    ) -> Result<Option<EventRelation>, sqlx::Error> {
        sqlx::query_as::<_, EventRelation>(
            r#"
            SELECT id, room_id, event_id, relates_to_event_id, relation_type,
                   sender, origin_server_ts, content, is_redacted, created_ts
            FROM event_relations
            WHERE room_id = $1 AND relates_to_event_id = $2
              AND relation_type = 'm.replace'
              AND sender = $3
              AND is_redacted = FALSE
            ORDER BY origin_server_ts DESC
            LIMIT 1
            "#,
        )
        .bind(room_id)
        .bind(relates_to_event_id)
        .bind(sender)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn aggregate_annotations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
    ) -> Result<Vec<AggregationResult>, sqlx::Error> {
        sqlx::query_as::<_, AggregationResult>(
            r#"
            SELECT
                relation_type,
                content->>'body' as key,
                COUNT(*) as count,
                NULL::text as sender
            FROM event_relations
            WHERE room_id = $1 AND relates_to_event_id = $2
              AND relation_type = 'm.annotation'
              AND is_redacted = FALSE
            GROUP BY relation_type, content->>'body'
            ORDER BY count DESC
            "#,
        )
        .bind(room_id)
        .bind(relates_to_event_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn redact_relation(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE event_relations
            SET is_redacted = TRUE, content = '{}'
            WHERE room_id = $1 AND event_id = $2
            "#,
        )
        .bind(room_id)
        .bind(event_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_relation(
        &self,
        room_id: &str,
        event_id: &str,
        sender: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM event_relations
            WHERE room_id = $1 AND event_id = $2 AND sender = $3
            "#,
        )
        .bind(room_id)
        .bind(event_id)
        .bind(sender)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn relation_exists(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: &str,
        sender: &str,
    ) -> Result<bool, sqlx::Error> {
        let result: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT 1 FROM event_relations
            WHERE room_id = $1 AND relates_to_event_id = $2
              AND relation_type = $3 AND sender = $4
              AND is_redacted = FALSE
            LIMIT 1
            "#,
        )
        .bind(room_id)
        .bind(relates_to_event_id)
        .bind(relation_type)
        .bind(sender)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_relation() -> EventRelation {
        EventRelation {
            id: 1,
            room_id: "!test:example.com".to_string(),
            event_id: "$reaction1".to_string(),
            relates_to_event_id: "$original:example.com".to_string(),
            relation_type: "m.annotation".to_string(),
            sender: "@user:example.com".to_string(),
            origin_server_ts: 1234567890,
            content: serde_json::json!({"body": "👍"}),
            is_redacted: false,
            created_ts: 1234567890,
        }
    }

    #[test]
    fn test_relation_creation() {
        let relation = create_test_relation();
        assert_eq!(relation.id, 1);
        assert_eq!(relation.room_id, "!test:example.com");
        assert_eq!(relation.relation_type, "m.annotation");
        assert!(!relation.is_redacted);
    }

    #[test]
    fn test_relation_query_params() {
        let params = RelationQueryParams {
            room_id: "!test:example.com".to_string(),
            relates_to_event_id: "$original:example.com".to_string(),
            relation_type: Some("m.annotation".to_string()),
            limit: Some(50),
            from: None,
            direction: Some("f".to_string()),
        };
        assert_eq!(params.room_id, "!test:example.com");
        assert!(params.limit.is_some());
    }

    #[test]
    fn test_aggregation_result() {
        let agg = AggregationResult {
            relation_type: "m.annotation".to_string(),
            key: Some("👍".to_string()),
            count: 5,
            sender: None,
        };
        assert_eq!(agg.count, 5);
        assert_eq!(agg.key.as_deref(), Some("👍"));
    }
}
