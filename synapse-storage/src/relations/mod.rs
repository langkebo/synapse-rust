use async_trait::async_trait;
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

// ── Trait ───────────────────────────────────────────────────────────────

#[async_trait]
pub trait RelationsStoreApi: Send + Sync {
    async fn create_relation(&self, params: CreateRelationParams) -> Result<EventRelation, sqlx::Error>;
    async fn get_relation(&self, room_id: &str, event_id: &str) -> Result<Option<EventRelation>, sqlx::Error>;
    async fn get_relations(&self, params: RelationQueryParams) -> Result<Vec<EventRelation>, sqlx::Error>;
    async fn count_relations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: Option<&str>,
    ) -> Result<i64, sqlx::Error>;
    async fn get_replacement(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        sender: &str,
    ) -> Result<Option<EventRelation>, sqlx::Error>;
    async fn aggregate_annotations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
    ) -> Result<Vec<AggregationResult>, sqlx::Error>;
    async fn redact_relation(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error>;
    async fn relation_exists(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: &str,
        sender: &str,
    ) -> Result<bool, sqlx::Error>;
}

#[derive(Clone)]
pub struct RelationsStorage {
    pub pool: Arc<Pool<Postgres>>,
}

impl RelationsStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_relation(&self, params: CreateRelationParams) -> Result<EventRelation, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as::<_, EventRelation>(
            r"
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
            ",
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

    pub async fn get_relation(&self, room_id: &str, event_id: &str) -> Result<Option<EventRelation>, sqlx::Error> {
        sqlx::query_as::<_, EventRelation>(
            r"
            SELECT id, room_id, event_id, relates_to_event_id, relation_type,
                   sender, origin_server_ts, content, is_redacted, created_ts
            FROM event_relations
            WHERE room_id = $1 AND event_id = $2 AND is_redacted = FALSE
            ",
        )
        .bind(room_id)
        .bind(event_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn count_relations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let count: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(*)
            FROM event_relations
            WHERE room_id = $1 AND relates_to_event_id = $2
              AND ($3::text IS NULL OR relation_type = $3)
              AND is_redacted = FALSE
            ",
        )
        .bind(room_id)
        .bind(relates_to_event_id)
        .bind(relation_type)
        .fetch_one(&*self.pool)
        .await?;

        Ok(count.0)
    }

    pub async fn get_relations(&self, params: RelationQueryParams) -> Result<Vec<EventRelation>, sqlx::Error> {
        let limit = params.limit.unwrap_or(50).min(100);
        let direction = params.direction.as_deref().unwrap_or("f");

        let query = match direction {
            "b" => {
                let from = params.from.unwrap_or_default();
                if let Some(ref rel_type) = params.relation_type {
                    sqlx::query_as::<_, EventRelation>(
                        r"
                        SELECT id, room_id, event_id, relates_to_event_id, relation_type,
                               sender, origin_server_ts, content, is_redacted, created_ts
                        FROM event_relations
                        WHERE room_id = $1 AND relates_to_event_id = $2
                          AND relation_type = $3
                          AND ($4::text = '' OR event_id < $4)
                          AND is_redacted = FALSE
                        ORDER BY origin_server_ts DESC, event_id DESC
                        LIMIT $5
                        ",
                    )
                    .bind(&params.room_id)
                    .bind(&params.relates_to_event_id)
                    .bind(rel_type)
                    .bind(&from)
                    .bind(limit)
                    .fetch_all(&*self.pool)
                    .await
                } else {
                    sqlx::query_as::<_, EventRelation>(
                        r"
                        SELECT id, room_id, event_id, relates_to_event_id, relation_type,
                               sender, origin_server_ts, content, is_redacted, created_ts
                        FROM event_relations
                        WHERE room_id = $1 AND relates_to_event_id = $2
                          AND ($3::text = '' OR event_id < $3)
                          AND is_redacted = FALSE
                        ORDER BY origin_server_ts DESC, event_id DESC
                        LIMIT $4
                        ",
                    )
                    .bind(&params.room_id)
                    .bind(&params.relates_to_event_id)
                    .bind(&from)
                    .bind(limit)
                    .fetch_all(&*self.pool)
                    .await
                }
            }
            _ => {
                let from = params.from.unwrap_or_default();
                if let Some(ref rel_type) = params.relation_type {
                    sqlx::query_as::<_, EventRelation>(
                        r"
                        SELECT id, room_id, event_id, relates_to_event_id, relation_type,
                               sender, origin_server_ts, content, is_redacted, created_ts
                        FROM event_relations
                        WHERE room_id = $1 AND relates_to_event_id = $2
                          AND relation_type = $3
                          AND ($4::text = '' OR event_id > $4)
                          AND is_redacted = FALSE
                        ORDER BY origin_server_ts ASC, event_id ASC
                        LIMIT $5
                        ",
                    )
                    .bind(&params.room_id)
                    .bind(&params.relates_to_event_id)
                    .bind(rel_type)
                    .bind(&from)
                    .bind(limit)
                    .fetch_all(&*self.pool)
                    .await
                } else {
                    sqlx::query_as::<_, EventRelation>(
                        r"
                        SELECT id, room_id, event_id, relates_to_event_id, relation_type,
                               sender, origin_server_ts, content, is_redacted, created_ts
                        FROM event_relations
                        WHERE room_id = $1 AND relates_to_event_id = $2
                          AND ($3::text = '' OR event_id > $3)
                          AND is_redacted = FALSE
                        ORDER BY origin_server_ts ASC, event_id ASC
                        LIMIT $4
                        ",
                    )
                    .bind(&params.room_id)
                    .bind(&params.relates_to_event_id)
                    .bind(&from)
                    .bind(limit)
                    .fetch_all(&*self.pool)
                    .await
                }
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
            r"
            SELECT id, room_id, event_id, relates_to_event_id, relation_type,
                   sender, origin_server_ts, content, is_redacted, created_ts
            FROM event_relations
            WHERE room_id = $1 AND relates_to_event_id = $2
              AND relation_type = 'm.annotation'
              AND is_redacted = FALSE
            ORDER BY origin_server_ts DESC
            LIMIT $3
            ",
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
            r"
            SELECT id, room_id, event_id, relates_to_event_id, relation_type,
                   sender, origin_server_ts, content, is_redacted, created_ts
            FROM event_relations
            WHERE room_id = $1 AND relates_to_event_id = $2
              AND relation_type = 'm.reference'
              AND is_redacted = FALSE
            ORDER BY origin_server_ts DESC
            LIMIT $3
            ",
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
            r"
            SELECT id, room_id, event_id, relates_to_event_id, relation_type,
                   sender, origin_server_ts, content, is_redacted, created_ts
            FROM event_relations
            WHERE room_id = $1 AND relates_to_event_id = $2
              AND relation_type = 'm.replace'
              AND sender = $3
              AND is_redacted = FALSE
            ORDER BY origin_server_ts DESC
            LIMIT 1
            ",
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
            r"
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
            ",
        )
        .bind(room_id)
        .bind(relates_to_event_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn redact_relation(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE event_relations
            SET is_redacted = TRUE, content = '{}'
            WHERE room_id = $1 AND event_id = $2
            ",
        )
        .bind(room_id)
        .bind(event_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_relation(&self, room_id: &str, event_id: &str, sender: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r"
            DELETE FROM event_relations
            WHERE room_id = $1 AND event_id = $2 AND sender = $3
            ",
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
        let result: Option<(i32,)> = sqlx::query_as(
            r"
            SELECT 1 FROM event_relations
            WHERE room_id = $1 AND relates_to_event_id = $2
              AND relation_type = $3 AND sender = $4
              AND is_redacted = FALSE
            LIMIT 1
            ",
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

// ── Trait delegation ────────────────────────────────────────────────────

#[async_trait]
impl RelationsStoreApi for RelationsStorage {
    async fn create_relation(&self, params: CreateRelationParams) -> Result<EventRelation, sqlx::Error> {
        self.create_relation(params).await
    }

    async fn get_relation(&self, room_id: &str, event_id: &str) -> Result<Option<EventRelation>, sqlx::Error> {
        self.get_relation(room_id, event_id).await
    }

    async fn get_relations(&self, params: RelationQueryParams) -> Result<Vec<EventRelation>, sqlx::Error> {
        self.get_relations(params).await
    }

    async fn count_relations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        self.count_relations(room_id, relates_to_event_id, relation_type).await
    }

    async fn get_replacement(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        sender: &str,
    ) -> Result<Option<EventRelation>, sqlx::Error> {
        self.get_replacement(room_id, relates_to_event_id, sender).await
    }

    async fn aggregate_annotations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
    ) -> Result<Vec<AggregationResult>, sqlx::Error> {
        self.aggregate_annotations(room_id, relates_to_event_id).await
    }

    async fn redact_relation(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        self.redact_relation(room_id, event_id).await
    }

    async fn relation_exists(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: &str,
        sender: &str,
    ) -> Result<bool, sqlx::Error> {
        self.relation_exists(room_id, relates_to_event_id, relation_type, sender).await
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

#[cfg(test)]
mod db_tests {
    use super::*;
    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
    use std::env;

    async fn test_pool() -> Arc<Pool<Postgres>> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    /// Ensure a room row exists so FK constraints on event_relations are satisfied.
    async fn ensure_test_room(pool: &Pool<Postgres>, room_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("INSERT INTO rooms (room_id, created_ts) VALUES ($1, $2) ON CONFLICT (room_id) DO NOTHING")
            .bind(room_id)
            .bind(now)
            .execute(pool)
            .await
            .expect("failed to create test room");
    }

    /// Clean up test data in event_relations and rooms for a given room_id suffix.
    async fn cleanup_relations(pool: &Pool<Postgres>, suffix: &str) {
        let _ = sqlx::query("DELETE FROM event_relations WHERE room_id LIKE $1")
            .bind(format!("%{suffix}"))
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM rooms WHERE room_id LIKE $1").bind(format!("%{suffix}")).execute(pool).await;
    }

    fn make_params(suffix: &str) -> CreateRelationParams {
        CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$event_{suffix}"),
            relates_to_event_id: format!("$related_{suffix}"),
            relation_type: "m.annotation".to_string(),
            sender: format!("@user_{suffix}:example.com"),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            content: json!({"body": "👍"}),
        }
    }

    // --- create_relation ---

    #[tokio::test]
    async fn test_create_relation_returns_valid_record() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);

        let rel = storage.create_relation(params).await.expect("create_relation should succeed");

        assert!(rel.id > 0);
        assert_eq!(rel.room_id, format!("!room_{suffix}:example.com"));
        assert_eq!(rel.event_id, format!("$event_{suffix}"));
        assert_eq!(rel.relates_to_event_id, format!("$related_{suffix}"));
        assert_eq!(rel.relation_type, "m.annotation");
        assert_eq!(rel.sender, format!("@user_{suffix}:example.com"));
        assert!(!rel.is_redacted);
        assert!(rel.created_ts > 0);

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_create_relation_upsert_updates_existing() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);

        let params1 = make_params(&suffix);
        let rel1 = storage.create_relation(params1).await.expect("first create_relation should succeed");

        // Upsert with same (event_id, relation_type, sender) but different content
        let params2 = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$event_{suffix}"),
            relates_to_event_id: format!("$related_{suffix}"),
            relation_type: "m.annotation".to_string(),
            sender: format!("@user_{suffix}:example.com"),
            origin_server_ts: chrono::Utc::now().timestamp_millis() + 1000,
            content: json!({"body": "👎", "extra": true}),
        };

        let rel2 = storage.create_relation(params2).await.expect("upsert create_relation should succeed");

        // Same row id, but updated content and origin_server_ts
        assert_eq!(rel2.id, rel1.id);
        assert_eq!(rel2.content, json!({"body": "👎", "extra": true}));
        assert!(rel2.origin_server_ts > rel1.origin_server_ts);
        assert!(!rel2.is_redacted);

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    // --- get_relation ---

    #[tokio::test]
    async fn test_get_relation_returns_existing() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);
        let created = storage.create_relation(params).await.expect("create_relation should succeed");

        let found = storage
            .get_relation(&format!("!room_{suffix}:example.com"), &format!("$event_{suffix}"))
            .await
            .expect("get_relation should succeed");

        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.id, created.id);
        assert_eq!(found.event_id, created.event_id);

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_get_relation_returns_none_for_unknown() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);

        let result = storage
            .get_relation(&format!("!room_{suffix}:example.com"), "$nonexistent")
            .await
            .expect("get_relation should succeed");

        assert!(result.is_none());

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_get_relation_skips_redacted() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);
        let _ = storage.create_relation(params).await.expect("create_relation should succeed");

        // Redact it
        storage
            .redact_relation(&format!("!room_{suffix}:example.com"), &format!("$event_{suffix}"))
            .await
            .expect("redact_relation should succeed");

        // get_relation should skip redacted rows
        let result = storage
            .get_relation(&format!("!room_{suffix}:example.com"), &format!("$event_{suffix}"))
            .await
            .expect("get_relation should succeed");

        assert!(result.is_none());

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    // --- count_relations ---

    #[tokio::test]
    async fn test_count_relations_no_filter() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}");

        // Insert 3 annotations for the same target
        for i in 0..3 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$event_{suffix}_{i}"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_{i}_{suffix}:example.com"),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
                content: json!({"body": "👍"}),
            };
            storage.create_relation(params).await.expect("create_relation should succeed");
        }

        let count = storage
            .count_relations(&format!("!room_{suffix}:example.com"), &relates_to, None)
            .await
            .expect("count_relations should succeed");

        assert_eq!(count, 3);

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_count_relations_with_type_filter() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}");

        // Insert 2 annotations
        for i in 0..2 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$annot_{suffix}_{i}"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_{i}_{suffix}:example.com"),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
                content: json!({"body": "👍"}),
            };
            storage.create_relation(params).await.unwrap();
        }

        // Insert 1 reference
        let ref_params = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$ref_{suffix}"),
            relates_to_event_id: relates_to.clone(),
            relation_type: "m.reference".to_string(),
            sender: format!("@user_ref_{suffix}:example.com"),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            content: json!({"body": "ref"}),
        };
        storage.create_relation(ref_params).await.unwrap();

        let annot_count = storage
            .count_relations(&format!("!room_{suffix}:example.com"), &relates_to, Some("m.annotation"))
            .await
            .expect("count_relations with filter should succeed");

        assert_eq!(annot_count, 2);

        let ref_count = storage
            .count_relations(&format!("!room_{suffix}:example.com"), &relates_to, Some("m.reference"))
            .await
            .expect("count_relations with filter should succeed");

        assert_eq!(ref_count, 1);

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    // --- get_relations ---

    #[tokio::test]
    async fn test_get_relations_forward_pagination() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}");

        // Insert 5 relations with staggered timestamps
        let mut event_ids = Vec::new();
        for i in 0..5 {
            let event_id = format!("$event_{suffix}_{i}");
            event_ids.push(event_id.clone());
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id,
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_{i}_{suffix}:example.com"),
                origin_server_ts: 1000 + (i as i64) * 100,
                content: json!({"body": format!("{}", i)}),
            };
            storage.create_relation(params).await.unwrap();
        }

        let params = RelationQueryParams {
            room_id: format!("!room_{suffix}:example.com"),
            relates_to_event_id: relates_to.clone(),
            relation_type: None,
            limit: Some(10),
            from: None,
            direction: Some("f".to_string()),
        };

        let results = storage.get_relations(params).await.expect("get_relations forward should succeed");

        assert_eq!(results.len(), 5);
        // Forward: ORDER BY origin_server_ts ASC, event_id ASC
        assert!(results[0].origin_server_ts <= results[1].origin_server_ts);
        assert!(results[1].origin_server_ts <= results[2].origin_server_ts);

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_get_relations_backward_pagination() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}");

        for i in 0..5 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$event_{suffix}_{i}"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_{i}_{suffix}:example.com"),
                origin_server_ts: 1000 + (i as i64) * 100,
                content: json!({"body": format!("{}", i)}),
            };
            storage.create_relation(params).await.unwrap();
        }

        let params = RelationQueryParams {
            room_id: format!("!room_{suffix}:example.com"),
            relates_to_event_id: relates_to.clone(),
            relation_type: None,
            limit: Some(10),
            from: None,
            direction: Some("b".to_string()),
        };

        let results = storage.get_relations(params).await.expect("get_relations backward should succeed");

        assert_eq!(results.len(), 5);
        // Backward: ORDER BY origin_server_ts DESC, event_id DESC
        assert!(results[0].origin_server_ts >= results[1].origin_server_ts);
        assert!(results[1].origin_server_ts >= results[2].origin_server_ts);

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_get_relations_with_cursor() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}");

        for i in 0..5 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$event_{suffix}_{i}"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_{i}_{suffix}:example.com"),
                origin_server_ts: 1000 + (i as i64) * 100,
                content: json!({"body": format!("{}", i)}),
            };
            storage.create_relation(params).await.unwrap();
        }

        // Get first page (forward, first 2)
        let page1 = storage
            .get_relations(RelationQueryParams {
                room_id: format!("!room_{suffix}:example.com"),
                relates_to_event_id: relates_to.clone(),
                relation_type: None,
                limit: Some(2),
                from: None,
                direction: Some("f".to_string()),
            })
            .await
            .expect("first page should succeed");

        assert_eq!(page1.len(), 2);

        // Get second page using cursor (event_id from the last item of page1)
        let cursor = page1.last().unwrap().event_id.clone();
        let page2 = storage
            .get_relations(RelationQueryParams {
                room_id: format!("!room_{suffix}:example.com"),
                relates_to_event_id: relates_to.clone(),
                relation_type: None,
                limit: Some(10),
                from: Some(cursor),
                direction: Some("f".to_string()),
            })
            .await
            .expect("second page should succeed");

        // Should have the remaining 3 items
        assert_eq!(page2.len(), 3);
        // The first item of page2 should come after the last item of page1 (ASC order)
        assert!(page2[0].origin_server_ts >= page1.last().unwrap().origin_server_ts);

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_get_relations_with_type_filter() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}");

        // Insert 2 annotations and 1 reference
        for i in 0..2 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$annot_{suffix}_{i}"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_{i}_{suffix}:example.com"),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
                content: json!({"body": "👍"}),
            };
            storage.create_relation(params).await.unwrap();
        }
        let ref_params = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$ref_{suffix}"),
            relates_to_event_id: relates_to.clone(),
            relation_type: "m.reference".to_string(),
            sender: format!("@user_ref_{suffix}:example.com"),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            content: json!({"body": "ref"}),
        };
        storage.create_relation(ref_params).await.unwrap();

        let annot_results = storage
            .get_relations(RelationQueryParams {
                room_id: format!("!room_{suffix}:example.com"),
                relates_to_event_id: relates_to.clone(),
                relation_type: Some("m.annotation".to_string()),
                limit: Some(10),
                from: None,
                direction: None,
            })
            .await
            .expect("get_relations with annotation filter should succeed");

        assert_eq!(annot_results.len(), 2);
        for r in &annot_results {
            assert_eq!(r.relation_type, "m.annotation");
        }

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    // --- get_annotations ---

    #[tokio::test]
    async fn test_get_annotations_returns_only_annotations() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}");

        // Insert 2 annotations
        for i in 0..2 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$annot_{suffix}_{i}"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_{i}_{suffix}:example.com"),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
                content: json!({"body": "👍"}),
            };
            storage.create_relation(params).await.unwrap();
        }

        // Insert 1 reference (should not appear in annotations)
        let ref_params = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$ref_{suffix}"),
            relates_to_event_id: relates_to.clone(),
            relation_type: "m.reference".to_string(),
            sender: format!("@user_ref_{suffix}:example.com"),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            content: json!({"body": "ref"}),
        };
        storage.create_relation(ref_params).await.unwrap();

        let annotations = storage
            .get_annotations(&format!("!room_{suffix}:example.com"), &relates_to, None)
            .await
            .expect("get_annotations should succeed");

        assert_eq!(annotations.len(), 2);
        for a in &annotations {
            assert_eq!(a.relation_type, "m.annotation");
        }

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_get_annotations_respects_limit() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}");

        for i in 0..5 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$annot_{suffix}_{i}"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_{i}_{suffix}:example.com"),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
                content: json!({"body": format!("{}", i)}),
            };
            storage.create_relation(params).await.unwrap();
        }

        let annotations = storage
            .get_annotations(&format!("!room_{suffix}:example.com"), &relates_to, Some(3))
            .await
            .expect("get_annotations should succeed");

        assert_eq!(annotations.len(), 3);

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    // --- get_references ---

    #[tokio::test]
    async fn test_get_references_returns_only_references() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}");

        // Insert 2 references
        for i in 0..2 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$ref_{suffix}_{i}"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.reference".to_string(),
                sender: format!("@user_{i}_{suffix}:example.com"),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
                content: json!({"body": format!("ref {}", i)}),
            };
            storage.create_relation(params).await.unwrap();
        }

        // Insert 1 annotation (should not appear in references)
        let annot_params = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$annot_{suffix}"),
            relates_to_event_id: relates_to.clone(),
            relation_type: "m.annotation".to_string(),
            sender: format!("@user_annot_{suffix}:example.com"),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            content: json!({"body": "👍"}),
        };
        storage.create_relation(annot_params).await.unwrap();

        let references = storage
            .get_references(&format!("!room_{suffix}:example.com"), &relates_to, None)
            .await
            .expect("get_references should succeed");

        assert_eq!(references.len(), 2);
        for r in &references {
            assert_eq!(r.relation_type, "m.reference");
        }

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    // --- get_replacement ---

    #[tokio::test]
    async fn test_get_replacement_returns_latest() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}");
        let sender = format!("@user_{suffix}:example.com");

        // Insert two replacements (same sender, same target) with staggered timestamps
        for i in 0..2 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$replace_{suffix}_{i}"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.replace".to_string(),
                sender: sender.clone(),
                origin_server_ts: 1000 + (i as i64) * 500,
                content: json!({"body": format!("v{}", i), "msgtype": "m.text"}),
            };
            storage.create_relation(params).await.unwrap();
        }

        let replacement = storage
            .get_replacement(&format!("!room_{suffix}:example.com"), &relates_to, &sender)
            .await
            .expect("get_replacement should succeed");

        assert!(replacement.is_some());
        let replacement = replacement.unwrap();
        assert_eq!(replacement.relation_type, "m.replace");
        assert_eq!(replacement.sender, sender);
        // Should return the latest one (highest origin_server_ts, due to ORDER BY DESC LIMIT 1)
        assert!(replacement.content["body"].as_str().unwrap().contains("v1"));

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_get_replacement_returns_none_for_different_sender() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}");

        let params = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$replace_{suffix}"),
            relates_to_event_id: relates_to.clone(),
            relation_type: "m.replace".to_string(),
            sender: format!("@alice_{suffix}:example.com"),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            content: json!({"body": "edit", "msgtype": "m.text"}),
        };
        storage.create_relation(params).await.unwrap();

        let replacement = storage
            .get_replacement(&format!("!room_{suffix}:example.com"), &relates_to, &format!("@bob_{suffix}:example.com"))
            .await
            .expect("get_replacement should succeed");

        assert!(replacement.is_none());

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    // --- aggregate_annotations ---

    #[tokio::test]
    async fn test_aggregate_annotations_groups_by_body() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}");

        // Insert 3 thumbs up and 2 thumbs down
        for i in 0..3 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$annot_up_{suffix}_{i}"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_up_{i}_{suffix}:example.com"),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
                content: json!({"body": "👍"}),
            };
            storage.create_relation(params).await.unwrap();
        }
        for i in 0..2 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$annot_down_{suffix}_{i}"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_down_{i}_{suffix}:example.com"),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
                content: json!({"body": "👎"}),
            };
            storage.create_relation(params).await.unwrap();
        }

        let agg = storage
            .aggregate_annotations(&format!("!room_{suffix}:example.com"), &relates_to)
            .await
            .expect("aggregate_annotations should succeed");

        assert_eq!(agg.len(), 2);

        // The one with count 3 should come first (ORDER BY count DESC)
        assert_eq!(agg[0].count, 3);
        assert_eq!(agg[1].count, 2);

        // Both should have m.annotation type
        for a in &agg {
            assert_eq!(a.relation_type, "m.annotation");
        }

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_aggregate_annotations_excludes_non_annotations() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}");

        // Insert 1 annotation
        let annot_params = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$annot_{suffix}"),
            relates_to_event_id: relates_to.clone(),
            relation_type: "m.annotation".to_string(),
            sender: format!("@user_{suffix}:example.com"),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            content: json!({"body": "👍"}),
        };
        storage.create_relation(annot_params).await.unwrap();

        // Insert 1 reference (should not appear in aggregation)
        let ref_params = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$ref_{suffix}"),
            relates_to_event_id: relates_to.clone(),
            relation_type: "m.reference".to_string(),
            sender: format!("@user_ref_{suffix}:example.com"),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            content: json!({"body": "ref"}),
        };
        storage.create_relation(ref_params).await.unwrap();

        let agg = storage
            .aggregate_annotations(&format!("!room_{suffix}:example.com"), &relates_to)
            .await
            .expect("aggregate_annotations should succeed");

        // Only the annotation should be aggregated
        assert_eq!(agg.len(), 1);
        assert_eq!(agg[0].count, 1);
        assert_eq!(agg[0].relation_type, "m.annotation");

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    // --- redact_relation ---

    #[tokio::test]
    async fn test_redact_relation_sets_flags_and_clears_content() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);
        let created = storage.create_relation(params).await.expect("create_relation should succeed");

        assert!(!created.is_redacted);
        assert!(created.content != json!({}));

        storage
            .redact_relation(&format!("!room_{suffix}:example.com"), &format!("$event_{suffix}"))
            .await
            .expect("redact_relation should succeed");

        // Verify by querying directly (bypassing is_redacted filter)
        let row: (bool, serde_json::Value) =
            sqlx::query_as("SELECT is_redacted, content FROM event_relations WHERE room_id = $1 AND event_id = $2")
                .bind(format!("!room_{suffix}:example.com"))
                .bind(format!("$event_{suffix}"))
                .fetch_one(&*pool)
                .await
                .expect("direct query should succeed");

        assert!(row.0, "is_redacted should be TRUE");
        assert_eq!(row.1, json!({}), "content should be empty object");

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    // --- delete_relation ---

    #[tokio::test]
    async fn test_delete_relation_removes_and_returns_true() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);
        let _ = storage.create_relation(params).await.expect("create_relation should succeed");

        let deleted = storage
            .delete_relation(
                &format!("!room_{suffix}:example.com"),
                &format!("$event_{suffix}"),
                &format!("@user_{suffix}:example.com"),
            )
            .await
            .expect("delete_relation should succeed");

        assert!(deleted, "delete should return true when a row is removed");

        // Verify it's gone
        let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM event_relations WHERE room_id = $1 AND event_id = $2")
            .bind(format!("!room_{suffix}:example.com"))
            .bind(format!("$event_{suffix}"))
            .fetch_optional(&*pool)
            .await
            .expect("direct query should succeed");

        assert!(row.is_none(), "row should be deleted");

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_delete_relation_returns_false_for_nonexistent() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);

        let deleted = storage
            .delete_relation(
                &format!("!room_{suffix}:example.com"),
                "$nonexistent",
                &format!("@user_{suffix}:example.com"),
            )
            .await
            .expect("delete_relation should succeed");

        assert!(!deleted, "delete should return false when no rows match");

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_delete_relation_returns_false_for_wrong_sender() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);
        let _ = storage.create_relation(params).await.expect("create_relation should succeed");

        // Try to delete with a different sender
        let deleted = storage
            .delete_relation(
                &format!("!room_{suffix}:example.com"),
                &format!("$event_{suffix}"),
                &format!("@other_{suffix}:example.com"),
            )
            .await
            .expect("delete_relation should succeed");

        assert!(!deleted, "delete should return false when sender does not match");

        // Row should still exist
        let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM event_relations WHERE room_id = $1 AND event_id = $2")
            .bind(format!("!room_{suffix}:example.com"))
            .bind(format!("$event_{suffix}"))
            .fetch_optional(&*pool)
            .await
            .expect("direct query should succeed");

        assert!(row.is_some(), "row should still exist");

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    // --- relation_exists ---

    #[tokio::test]
    async fn test_relation_exists_returns_true_for_existing() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);
        let _ = storage.create_relation(params).await.expect("create_relation should succeed");

        let exists = storage
            .relation_exists(
                &format!("!room_{suffix}:example.com"),
                &format!("$related_{suffix}"),
                "m.annotation",
                &format!("@user_{suffix}:example.com"),
            )
            .await
            .expect("relation_exists should succeed");

        assert!(exists);

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_relation_exists_returns_false_for_nonexistent() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);

        let exists = storage
            .relation_exists(
                &format!("!room_{suffix}:example.com"),
                "$nonexistent",
                "m.annotation",
                &format!("@user_{suffix}:example.com"),
            )
            .await
            .expect("relation_exists should succeed");

        assert!(!exists);

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_relation_exists_returns_false_after_redaction() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);
        let _ = storage.create_relation(params).await.expect("create_relation should succeed");

        // Redact it
        storage
            .redact_relation(&format!("!room_{suffix}:example.com"), &format!("$event_{suffix}"))
            .await
            .expect("redact_relation should succeed");

        // relation_exists should return false because it filters is_redacted = FALSE
        let exists = storage
            .relation_exists(
                &format!("!room_{suffix}:example.com"),
                &format!("$related_{suffix}"),
                "m.annotation",
                &format!("@user_{suffix}:example.com"),
            )
            .await
            .expect("relation_exists should succeed");

        assert!(!exists, "relation_exists should return false for redacted relations");

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }
}
