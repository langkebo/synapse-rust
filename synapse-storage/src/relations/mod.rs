use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

/// The `EventRelation` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EventRelation {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `relates_to_event_id` field.
    pub relates_to_event_id: String,
    /// The `relation_type` field.
    pub relation_type: String,
    /// The `sender` field.
    pub sender: String,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `is_redacted` field.
    pub is_redacted: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `CreateRelationParams` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRelationParams {
    /// The `room_id` field.
    pub room_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `relates_to_event_id` field.
    pub relates_to_event_id: String,
    /// The `relation_type` field.
    pub relation_type: String,
    /// The `sender` field.
    pub sender: String,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
    /// The `content` field.
    pub content: serde_json::Value,
}

/// The `RelationQueryParams` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationQueryParams {
    /// The `room_id` field.
    pub room_id: String,
    /// The `relates_to_event_id` field.
    pub relates_to_event_id: String,
    /// The `relation_type` field.
    pub relation_type: Option<String>,
    /// The `limit` field.
    pub limit: Option<i32>,
    /// The `from` field.
    pub from: Option<String>,
    /// The `direction` field.
    pub direction: Option<String>,
    /// MSC3981: also traverse the relations of related events, instead of only
    /// returning the relations of `relates_to_event_id` itself.
    pub recurse: bool,
    /// `event_type` 过滤（spec 的 `/{relType}/{eventType}` 路由）；`None` ＝ 不过滤。
    pub event_type: Option<String>,
}

/// MSC3981 recursion budget, counted in relation hops from the requested event.
///
/// With `recurse = true` the traversal follows relation edges while the
/// traversed event's depth is `<= MSC3981_RECURSION_DEPTH`; the direct relations
/// sit at depth `0`, so the deepest event a response can contain is 4 hops away
/// from the requested event. The value is reported to clients as
/// `recursion_depth` and is deliberately identical to upstream Synapse
/// (`synapse/storage/databases/main/relations.py`) so that clients which compare
/// the advertised depth against their own recursion behave the same way against
/// both servers.
pub const MSC3981_RECURSION_DEPTH: i32 = 3;

/// A `/relations` row together with the topological ordering key the query
/// sorted and paginated on.
///
/// MSC3981 requires `/relations` to return events in topological order — the
/// order `/messages` returns the same events in for the same `dir` — for the
/// recursive and the non-recursive query alike. That key is
/// `events.stream_ordering`, which is why the query joins `events`. Relation
/// rows whose event has no `events` row (representable, since
/// `event_relations.event_id` carries no foreign key) fall back to the
/// relation's own `origin_server_ts` instead.
#[derive(Debug, Clone)]
pub struct OrderedEventRelation {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `relates_to_event_id` field.
    pub relates_to_event_id: String,
    /// The `relation_type` field.
    pub relation_type: String,
    /// The `sender` field.
    pub sender: String,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `is_redacted` field.
    pub is_redacted: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The topological ordering key (see the type-level docs).
    pub stream_ordering: i64,
}

/// The `AggregationResult` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationResult {
    /// The `relation_type` field.
    pub relation_type: String,
    /// The `key` field.
    pub key: Option<String>,
    /// The `count` field.
    pub count: i64,
    /// The `sender` field.
    pub sender: Option<String>,
}

/// Parse a keyset pagination cursor of the form `<ordering key>:<event_id>`.
///
/// The ordering key is the column [`get_relations`] sorts on
/// ([`OrderedEventRelation::stream_ordering`]). Matrix event IDs always start
/// with `$`, so the leading segment before the first `:` can only be the key
/// when it parses as `i64`; anything else is not a cursor this server issued.
/// Returns `None` for such malformed tokens; callers treat them as "no cursor"
/// rather than paginating on a half-understood token.
pub fn parse_keyset_cursor(from: &str) -> Option<(i64, String)> {
    let (ts_str, eid) = from.split_once(':')?;
    let ts = ts_str.parse::<i64>().ok()?;
    Some((ts, eid.to_string()))
}

/// Encode a keyset cursor from a relation row's ordering columns.
///
/// Used by the service layer to build `next_batch` / `prev_batch` tokens that
/// `get_relations` can later parse via [`parse_keyset_cursor`]. The first
/// component is the row's ordering key, i.e.
/// [`OrderedEventRelation::stream_ordering`] (topological order), not
/// necessarily the event's `origin_server_ts`.
pub fn encode_keyset_cursor(ordering_key: i64, event_id: &str) -> String {
    format!("{ordering_key}:{event_id}")
}

// ── Trait ───────────────────────────────────────────────────────────────

/// The `RelationsStoreApi` trait.
#[async_trait]
pub trait RelationsStoreApi: Send + Sync {
    /// See [`create_relation`].
    async fn create_relation(&self, params: CreateRelationParams) -> Result<EventRelation, sqlx::Error>;
    /// DB-03-a: transactional variant of `create_relation` for use within a
    /// caller-managed transaction (e.g. `send_message` writes both an event
    /// and a relation that must commit or roll back atomically).
    async fn create_relation_in_tx(
        &self,
        params: CreateRelationParams,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<EventRelation, sqlx::Error>;
    /// See [`get_relation`].
    async fn get_relation(&self, room_id: &str, event_id: &str) -> Result<Option<EventRelation>, sqlx::Error>;
    /// See [`get_relations`].
    async fn get_relations(&self, params: RelationQueryParams) -> Result<Vec<OrderedEventRelation>, sqlx::Error>;
    /// See [`count_relations`].
    async fn count_relations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: Option<&str>,
    ) -> Result<i64, sqlx::Error>;
    /// See [`get_replacement`].
    async fn get_replacement(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        sender: &str,
    ) -> Result<Option<EventRelation>, sqlx::Error>;
    /// See [`aggregate_annotations`].
    async fn aggregate_annotations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
    ) -> Result<Vec<AggregationResult>, sqlx::Error>;
    /// See [`redact_relation`].
    async fn redact_relation(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error>;
    /// See [`relation_exists`].
    async fn relation_exists(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: &str,
        sender: &str,
    ) -> Result<bool, sqlx::Error>;
}

/// The `RelationsStorage` struct.
#[derive(Clone)]
pub struct RelationsStorage {
    /// The `pool` field.
    pub pool: Arc<Pool<Postgres>>,
}

impl RelationsStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`create_relation`].
    pub async fn create_relation(&self, params: CreateRelationParams) -> Result<EventRelation, sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query_as!(
            EventRelation,
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
            &params.room_id,
            &params.event_id,
            &params.relates_to_event_id,
            &params.relation_type,
            &params.sender,
            params.origin_server_ts,
            &params.content,
            now,
        )
        .fetch_one(&*self.pool)
        .await
    }

    /// Transactional variant — executes within a caller-supplied transaction.
    /// Used by `send_message` (DB-03-a) to keep event + relation writes atomic.
    pub async fn create_relation_in_tx(
        &self,
        params: CreateRelationParams,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<EventRelation, sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query_as!(
            EventRelation,
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
            &params.room_id,
            &params.event_id,
            &params.relates_to_event_id,
            &params.relation_type,
            &params.sender,
            params.origin_server_ts,
            &params.content,
            now,
        )
        .fetch_one(&mut **tx)
        .await
    }

    /// See [`get_relation`].
    pub async fn get_relation(&self, room_id: &str, event_id: &str) -> Result<Option<EventRelation>, sqlx::Error> {
        sqlx::query_as!(
            EventRelation,
            r#"
            SELECT id, room_id, event_id, relates_to_event_id, relation_type,
                   sender, origin_server_ts, content, is_redacted, created_ts
            FROM event_relations
            WHERE room_id = $1 AND event_id = $2 AND is_redacted = FALSE
            "#,
            room_id,
            event_id,
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`count_relations`].
    pub async fn count_relations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM event_relations
            WHERE room_id = $1 AND relates_to_event_id = $2
              AND ($3::text IS NULL OR relation_type = $3)
              AND is_redacted = FALSE
            "#,
            room_id,
            relates_to_event_id,
            relation_type,
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(count)
    }

    /// See [`get_relations`].
    ///
    /// 键集（keyset）分页：游标为 `<ordering key>:<event_id>`，用行值比较
    /// `(stream_ordering, event_id) {<|>} (key, eid)`，与 ORDER BY 完全一致。
    /// 排序键是 `events.stream_ordering`（拓扑序，与同 `dir` 的 `/messages`
    /// 一致，MSC3981 要求二者始终一致），因此 `events` 在 CTE 内 join。
    /// `params.recurse = true` 时递归项生效：沿关系边继续下行，深度上限
    /// [`MSC3981_RECURSION_DEPTH`]；`false` 时递归项被 `$5` 短路，退化为原来的
    /// 单层查询（结果集与递归打开时相同：过滤施加在**返回集**上，与上游
    /// Synapse 的实现一致，而非 MSC 正文那句"过滤同时剪枝中间节点"）。
    pub async fn get_relations(&self, params: RelationQueryParams) -> Result<Vec<OrderedEventRelation>, sqlx::Error> {
        let limit = i64::from(params.limit.unwrap_or(50).min(100));
        let backward = params.direction.as_deref() == Some("b");
        // 游标拆成两个**非空**实参：`stream_ordering`/`origin_server_ts` 恒 > 0，
        // 故 0 表示"无游标"；`relation_type` 恒非空串，故空串表示"不过滤"。
        // （`query_as!` 在本查询形状下不接受 `Option<_>` 形参。）
        let (from_key, from_event_id) = match params.from.as_deref().and_then(parse_keyset_cursor) {
            Some((key, event_id)) => (key, event_id),
            None => (0, String::new()),
        };
        let relation_type = params.relation_type.clone().unwrap_or_default();
        let event_type = params.event_type.clone().unwrap_or_default();

        sqlx::query_as!(
            OrderedEventRelation,
            r#"
            WITH RECURSIVE relation_tree AS (
                -- 直接关系：被请求事件的一层关系行。
                SELECT er.id, er.room_id, er.event_id, er.relates_to_event_id,
                       er.relation_type, er.sender, er.origin_server_ts, er.content,
                       er.is_redacted, er.created_ts,
                       -- `events.stream_ordering` 有 DEFAULT nextval(...)，本仓所有
                       -- 写入路径都不显式提供该列，因此恒非空；列本身只是没声明
                       -- NOT NULL，故此处断言。`events` 用 LEFT JOIN：关系行没有
                       -- 外键指向 `events`，孤儿行（只有测试夹具会造）按其自身
                       -- `origin_server_ts` 排序，而不是从结果里消失。
                       COALESCE(e.stream_ordering, er.origin_server_ts) AS stream_ordering,
                       e.event_type AS event_type,
                       0 AS depth
                FROM event_relations er
                LEFT JOIN events e ON e.event_id = er.event_id
                WHERE er.room_id = $1
                  AND er.relates_to_event_id = $2
                  AND er.is_redacted = FALSE
                UNION
                -- MSC3981：关系的关系。`$6 = TRUE` 不成立（缺省路径）时这一项不产出
                -- 任何行，整个 CTE 退化为上面的一层查询。深度上限同时兜住环：环上的
                -- 行因 depth 不同而不被 UNION 去重，只能靠上限终止。
                SELECT er.id, er.room_id, er.event_id, er.relates_to_event_id,
                       er.relation_type, er.sender, er.origin_server_ts, er.content,
                       er.is_redacted, er.created_ts,
                       COALESCE(e.stream_ordering, er.origin_server_ts) AS stream_ordering,
                       e.event_type AS event_type,
                       rt.depth + 1
                FROM event_relations er
                INNER JOIN relation_tree rt ON rt.event_id = er.relates_to_event_id
                LEFT JOIN events e ON e.event_id = er.event_id
                WHERE er.room_id = $1
                  AND er.is_redacted = FALSE
                  AND $6 = TRUE
                  AND rt.depth <= $7
            )
            -- 每一列都要断言非空：PG 的 Describe 不给递归 CTE 的输出列透传
            -- NOT NULL（R4 的 UNION/CTE 型），而 `event_relations` 的这些列在
            -- schema 里全是 NOT NULL，`stream_ordering` 见上面的 COALESCE。
            SELECT id AS "id!", room_id AS "room_id!", event_id AS "event_id!",
                   relates_to_event_id AS "relates_to_event_id!",
                   relation_type AS "relation_type!", sender AS "sender!",
                   origin_server_ts AS "origin_server_ts!", content AS "content!",
                   is_redacted AS "is_redacted!", created_ts AS "created_ts!",
                   stream_ordering AS "stream_ordering!"
            FROM relation_tree
            WHERE ($8 = '' OR relation_type = $8)
              AND ($10 = '' OR event_type = $10)
              AND ($3::bigint = 0
                   OR ($5 = TRUE
                       AND (stream_ordering < $3 OR (stream_ordering = $3 AND event_id < $4)))
                   OR ($5 = FALSE
                       AND (stream_ordering > $3 OR (stream_ordering = $3 AND event_id > $4))))
            ORDER BY
                CASE WHEN $5 = TRUE THEN stream_ordering END DESC,
                CASE WHEN $5 = TRUE THEN event_id END DESC,
                CASE WHEN $5 = FALSE THEN stream_ordering END ASC,
                CASE WHEN $5 = FALSE THEN event_id END ASC
            LIMIT $9
            "#,
            params.room_id,
            params.relates_to_event_id,
            from_key,
            from_event_id,
            backward,
            params.recurse,
            MSC3981_RECURSION_DEPTH,
            relation_type,
            limit,
            event_type,
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_annotations`].
    pub async fn get_annotations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        limit: Option<i32>,
    ) -> Result<Vec<EventRelation>, sqlx::Error> {
        let limit = limit.unwrap_or(50).min(100);

        sqlx::query_as!(
            EventRelation,
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
            room_id,
            relates_to_event_id,
            i64::from(limit),
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_references`].
    pub async fn get_references(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        limit: Option<i32>,
    ) -> Result<Vec<EventRelation>, sqlx::Error> {
        let limit = limit.unwrap_or(50).min(100);

        sqlx::query_as!(
            EventRelation,
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
            room_id,
            relates_to_event_id,
            i64::from(limit),
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_replacement`].
    pub async fn get_replacement(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        sender: &str,
    ) -> Result<Option<EventRelation>, sqlx::Error> {
        sqlx::query_as!(
            EventRelation,
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
            room_id,
            relates_to_event_id,
            sender,
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`aggregate_annotations`].
    pub async fn aggregate_annotations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
    ) -> Result<Vec<AggregationResult>, sqlx::Error> {
        sqlx::query_as!(
            AggregationResult,
            r#"
            SELECT
                relation_type,
                content->>'body' as key,
                COUNT(*) AS "count!",
                NULL::text as sender
            FROM event_relations
            WHERE room_id = $1 AND relates_to_event_id = $2
              AND relation_type = 'm.annotation'
              AND is_redacted = FALSE
            GROUP BY relation_type, content->>'body'
            ORDER BY COUNT(*) DESC
            "#,
            room_id,
            relates_to_event_id,
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`redact_relation`].
    pub async fn redact_relation(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE event_relations
            SET is_redacted = TRUE, content = '{}'
            WHERE room_id = $1 AND event_id = $2
            "#,
            room_id,
            event_id,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`delete_relation`].
    pub async fn delete_relation(&self, room_id: &str, event_id: &str, sender: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM event_relations
            WHERE room_id = $1 AND event_id = $2 AND sender = $3
            "#,
            room_id,
            event_id,
            sender,
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`relation_exists`].
    pub async fn relation_exists(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: &str,
        sender: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"
            SELECT 1 AS hit FROM event_relations
            WHERE room_id = $1 AND relates_to_event_id = $2
              AND relation_type = $3 AND sender = $4
              AND is_redacted = FALSE
            LIMIT 1
            "#,
            room_id,
            relates_to_event_id,
            relation_type,
            sender,
        )
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

    async fn create_relation_in_tx(
        &self,
        params: CreateRelationParams,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<EventRelation, sqlx::Error> {
        self.create_relation_in_tx(params, tx).await
    }

    async fn get_relation(&self, room_id: &str, event_id: &str) -> Result<Option<EventRelation>, sqlx::Error> {
        self.get_relation(room_id, event_id).await
    }

    async fn get_relations(&self, params: RelationQueryParams) -> Result<Vec<OrderedEventRelation>, sqlx::Error> {
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
            recurse: false,
            event_type: None,
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
    use std::sync::Arc;

    /// 每个测试一个从迁移 baseline 克隆出来的独立 schema（返回 guard 与 pool）。
    ///
    /// 2026-09-21：原先用共享 `public` 池。共享池的问题：测试结果取决于环境里 `public` 的
    /// 状态（本地 `public` 落后于迁移 baseline 时会直接 42P01），且并行测试互相影响。
    /// 按铁律 7 消除状态共享：per-test schema 由模板克隆，表一定存在、行数从 0 开始。
    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<sqlx::PgPool>) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated test pool");
        let pool = isolated.pool();
        (isolated, pool)
    }

    /// Ensure a room row exists so FK constraints on event_relations are satisfied.
    async fn ensure_test_room(pool: &Pool<Postgres>, room_id: &str) {
        let now = current_timestamp_millis();
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
            event_id: format!("$event_{suffix}:localhost"),
            relates_to_event_id: format!("$related_{suffix}:localhost"),
            relation_type: "m.annotation".to_string(),
            sender: format!("@user_{suffix}:example.com"),
            origin_server_ts: current_timestamp_millis(),
            content: json!({"body": "👍"}),
        }
    }

    // --- create_relation ---

    #[tokio::test]
    async fn test_create_relation_returns_valid_record() {
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);

        let rel = storage.create_relation(params).await.expect("create_relation should succeed");

        assert!(rel.id > 0);
        assert_eq!(rel.room_id, format!("!room_{suffix}:example.com"));
        assert_eq!(rel.event_id, format!("$event_{suffix}:localhost"));
        assert_eq!(rel.relates_to_event_id, format!("$related_{suffix}:localhost"));
        assert_eq!(rel.relation_type, "m.annotation");
        assert_eq!(rel.sender, format!("@user_{suffix}:example.com"));
        assert!(!rel.is_redacted);
        assert!(rel.created_ts > 0);

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_create_relation_upsert_updates_existing() {
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);

        let params1 = make_params(&suffix);
        let rel1 = storage.create_relation(params1).await.expect("first create_relation should succeed");

        // Upsert with same (event_id, relation_type, sender) but different content
        let params2 = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$event_{suffix}:localhost"),
            relates_to_event_id: format!("$related_{suffix}:localhost"),
            relation_type: "m.annotation".to_string(),
            sender: format!("@user_{suffix}:example.com"),
            origin_server_ts: current_timestamp_millis() + 1000,
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);
        let created = storage.create_relation(params).await.expect("create_relation should succeed");

        let found = storage
            .get_relation(&format!("!room_{suffix}:example.com"), &format!("$event_{suffix}:localhost"))
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);
        let _ = storage.create_relation(params).await.expect("create_relation should succeed");

        // Redact it
        storage
            .redact_relation(&format!("!room_{suffix}:example.com"), &format!("$event_{suffix}:localhost"))
            .await
            .expect("redact_relation should succeed");

        // get_relation should skip redacted rows
        let result = storage
            .get_relation(&format!("!room_{suffix}:example.com"), &format!("$event_{suffix}:localhost"))
            .await
            .expect("get_relation should succeed");

        assert!(result.is_none());

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    // --- count_relations ---

    #[tokio::test]
    async fn test_count_relations_no_filter() {
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}:localhost");

        // Insert 3 annotations for the same target
        for i in 0..3 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$event_{suffix}_{i}:localhost"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_{i}_{suffix}:example.com"),
                origin_server_ts: current_timestamp_millis(),
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}:localhost");

        // Insert 2 annotations
        for i in 0..2 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$annot_{suffix}_{i}:localhost"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_{i}_{suffix}:example.com"),
                origin_server_ts: current_timestamp_millis(),
                content: json!({"body": "👍"}),
            };
            storage.create_relation(params).await.unwrap();
        }

        // Insert 1 reference
        let ref_params = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$ref_{suffix}:localhost"),
            relates_to_event_id: relates_to.clone(),
            relation_type: "m.reference".to_string(),
            sender: format!("@user_ref_{suffix}:example.com"),
            origin_server_ts: current_timestamp_millis(),
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}:localhost");

        // Insert 5 relations with staggered timestamps
        let mut event_ids = Vec::new();
        for i in 0..5 {
            let event_id = format!("$event_{suffix}_{i}:localhost");
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
            recurse: false,
            event_type: None,
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}:localhost");

        for i in 0..5 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$event_{suffix}_{i}:localhost"),
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
            recurse: false,
            event_type: None,
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}:localhost");

        for i in 0..5 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$event_{suffix}_{i}:localhost"),
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
                recurse: false,
                event_type: None,
            })
            .await
            .expect("first page should succeed");

        assert_eq!(page1.len(), 2);

        // Get second page using keyset cursor (ts:event_id format)。
        // 行值比较 (origin_server_ts, event_id) > (ts, eid) 才能正确匹配 ORDER BY。
        let last = page1.last().unwrap();
        let cursor = encode_keyset_cursor(last.origin_server_ts, &last.event_id);
        let page2 = storage
            .get_relations(RelationQueryParams {
                room_id: format!("!room_{suffix}:example.com"),
                relates_to_event_id: relates_to.clone(),
                relation_type: None,
                limit: Some(10),
                from: Some(cursor),
                direction: Some("f".to_string()),
                recurse: false,
                event_type: None,
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}:localhost");

        // Insert 2 annotations and 1 reference
        for i in 0..2 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$annot_{suffix}_{i}:localhost"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_{i}_{suffix}:example.com"),
                origin_server_ts: current_timestamp_millis(),
                content: json!({"body": "👍"}),
            };
            storage.create_relation(params).await.unwrap();
        }
        let ref_params = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$ref_{suffix}:localhost"),
            relates_to_event_id: relates_to.clone(),
            relation_type: "m.reference".to_string(),
            sender: format!("@user_ref_{suffix}:example.com"),
            origin_server_ts: current_timestamp_millis(),
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
                recurse: false,
                event_type: None,
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}:localhost");

        // Insert 2 annotations
        for i in 0..2 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$annot_{suffix}_{i}:localhost"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_{i}_{suffix}:example.com"),
                origin_server_ts: current_timestamp_millis(),
                content: json!({"body": "👍"}),
            };
            storage.create_relation(params).await.unwrap();
        }

        // Insert 1 reference (should not appear in annotations)
        let ref_params = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$ref_{suffix}:localhost"),
            relates_to_event_id: relates_to.clone(),
            relation_type: "m.reference".to_string(),
            sender: format!("@user_ref_{suffix}:example.com"),
            origin_server_ts: current_timestamp_millis(),
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}:localhost");

        for i in 0..5 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$annot_{suffix}_{i}:localhost"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_{i}_{suffix}:example.com"),
                origin_server_ts: current_timestamp_millis(),
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}:localhost");

        // Insert 2 references
        for i in 0..2 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$ref_{suffix}_{i}:localhost"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.reference".to_string(),
                sender: format!("@user_{i}_{suffix}:example.com"),
                origin_server_ts: current_timestamp_millis(),
                content: json!({"body": format!("ref {}", i)}),
            };
            storage.create_relation(params).await.unwrap();
        }

        // Insert 1 annotation (should not appear in references)
        let annot_params = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$annot_{suffix}:localhost"),
            relates_to_event_id: relates_to.clone(),
            relation_type: "m.annotation".to_string(),
            sender: format!("@user_annot_{suffix}:example.com"),
            origin_server_ts: current_timestamp_millis(),
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}:localhost");
        let sender = format!("@user_{suffix}:example.com");

        // Insert two replacements (same sender, same target) with staggered timestamps
        for i in 0..2 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$replace_{suffix}_{i}:localhost"),
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}:localhost");

        let params = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$replace_{suffix}:localhost"),
            relates_to_event_id: relates_to.clone(),
            relation_type: "m.replace".to_string(),
            sender: format!("@alice_{suffix}:example.com"),
            origin_server_ts: current_timestamp_millis(),
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}:localhost");

        // Insert 3 thumbs up and 2 thumbs down
        for i in 0..3 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$annot_up_{suffix}_{i}:localhost"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_up_{i}_{suffix}:example.com"),
                origin_server_ts: current_timestamp_millis(),
                content: json!({"body": "👍"}),
            };
            storage.create_relation(params).await.unwrap();
        }
        for i in 0..2 {
            let params = CreateRelationParams {
                room_id: format!("!room_{suffix}:example.com"),
                event_id: format!("$annot_down_{suffix}_{i}:localhost"),
                relates_to_event_id: relates_to.clone(),
                relation_type: "m.annotation".to_string(),
                sender: format!("@user_down_{i}_{suffix}:example.com"),
                origin_server_ts: current_timestamp_millis(),
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let relates_to = format!("$related_{suffix}:localhost");

        // Insert 1 annotation
        let annot_params = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$annot_{suffix}:localhost"),
            relates_to_event_id: relates_to.clone(),
            relation_type: "m.annotation".to_string(),
            sender: format!("@user_{suffix}:example.com"),
            origin_server_ts: current_timestamp_millis(),
            content: json!({"body": "👍"}),
        };
        storage.create_relation(annot_params).await.unwrap();

        // Insert 1 reference (should not appear in aggregation)
        let ref_params = CreateRelationParams {
            room_id: format!("!room_{suffix}:example.com"),
            event_id: format!("$ref_{suffix}:localhost"),
            relates_to_event_id: relates_to.clone(),
            relation_type: "m.reference".to_string(),
            sender: format!("@user_ref_{suffix}:example.com"),
            origin_server_ts: current_timestamp_millis(),
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);
        let created = storage.create_relation(params).await.expect("create_relation should succeed");

        assert!(!created.is_redacted);
        assert!(created.content != json!({}));

        storage
            .redact_relation(&format!("!room_{suffix}:example.com"), &format!("$event_{suffix}:localhost"))
            .await
            .expect("redact_relation should succeed");

        // Verify by querying directly (bypassing is_redacted filter)
        let row: (bool, serde_json::Value) =
            sqlx::query_as("SELECT is_redacted, content FROM event_relations WHERE room_id = $1 AND event_id = $2")
                .bind(format!("!room_{suffix}:example.com"))
                .bind(format!("$event_{suffix}:localhost"))
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);
        let _ = storage.create_relation(params).await.expect("create_relation should succeed");

        let deleted = storage
            .delete_relation(
                &format!("!room_{suffix}:example.com"),
                &format!("$event_{suffix}:localhost"),
                &format!("@user_{suffix}:example.com"),
            )
            .await
            .expect("delete_relation should succeed");

        assert!(deleted, "delete should return true when a row is removed");

        // Verify it's gone
        let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM event_relations WHERE room_id = $1 AND event_id = $2")
            .bind(format!("!room_{suffix}:example.com"))
            .bind(format!("$event_{suffix}:localhost"))
            .fetch_optional(&*pool)
            .await
            .expect("direct query should succeed");

        assert!(row.is_none(), "row should be deleted");

        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;
    }

    #[tokio::test]
    async fn test_delete_relation_returns_false_for_nonexistent() {
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);
        let _ = storage.create_relation(params).await.expect("create_relation should succeed");

        // Try to delete with a different sender
        let deleted = storage
            .delete_relation(
                &format!("!room_{suffix}:example.com"),
                &format!("$event_{suffix}:localhost"),
                &format!("@other_{suffix}:example.com"),
            )
            .await
            .expect("delete_relation should succeed");

        assert!(!deleted, "delete should return false when sender does not match");

        // Row should still exist
        let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM event_relations WHERE room_id = $1 AND event_id = $2")
            .bind(format!("!room_{suffix}:example.com"))
            .bind(format!("$event_{suffix}:localhost"))
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);
        let _ = storage.create_relation(params).await.expect("create_relation should succeed");

        let exists = storage
            .relation_exists(
                &format!("!room_{suffix}:example.com"),
                &format!("$related_{suffix}:localhost"),
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
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
        let (_isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        cleanup_relations(&pool, &suffix).await;
        ensure_test_room(&pool, &format!("!room_{suffix}:example.com")).await;

        let storage = RelationsStorage::new(&pool);
        let params = make_params(&suffix);
        let _ = storage.create_relation(params).await.expect("create_relation should succeed");

        // Redact it
        storage
            .redact_relation(&format!("!room_{suffix}:example.com"), &format!("$event_{suffix}:localhost"))
            .await
            .expect("redact_relation should succeed");

        // relation_exists should return false because it filters is_redacted = FALSE
        let exists = storage
            .relation_exists(
                &format!("!room_{suffix}:example.com"),
                &format!("$related_{suffix}:localhost"),
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
