//! Search and full-text search methods for [`EventStorage`].

use sqlx::{Postgres, QueryBuilder};

use super::models::RoomEvent;
use super::EventStorage;

impl EventStorage {
    /// See [`search_room_messages_admin`].
    pub async fn search_room_messages_admin(
        &self,
        room_id: &str,
        search_pattern: &str,
        limit: i64,
        start_ts: Option<i64>,
        end_ts: Option<i64>,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        // 运行时 `query_as` 而非 `query!`：2026-10-21 新增 `start_ts` / `end_ts` 可选时间窗
        // （管理端 `start_date` / `end_date` 此前被 `deny_unknown_fields` 接受却从未生效），
        // 宏查询需要同步 `.sqlx` 离线缓存（本机 `SQLX_OFFLINE=true`），故改为运行时绑定。
        // `$4::bigint IS NULL` 让未传时间窗时谓词恒真，保持原有行为不变。
        let rows = sqlx::query_as::<_, (String, String, String, serde_json::Value, i64)>(
            r#"
            SELECT event_id, event_type, sender, content, origin_server_ts
            FROM events
            WHERE room_id = $1 AND event_type = 'm.room.message' AND content::text ILIKE $2
              AND is_redacted = false AND soft_failed = FALSE
              AND ($4::bigint IS NULL OR origin_server_ts >= $4)
              AND ($5::bigint IS NULL OR origin_server_ts <= $5)
            ORDER BY origin_server_ts DESC
            LIMIT $3
            "#,
        )
        .bind(room_id)
        .bind(search_pattern)
        .bind(limit)
        .bind(start_ts)
        .bind(end_ts)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(event_id, event_type, sender, content, origin_server_ts)| {
                serde_json::json!({
                    "event_id": event_id,
                    "type": event_type,
                    "sender": sender,
                    "content": content,
                    "origin_server_ts": origin_server_ts
                })
            })
            .collect())
    }

    /// See [`search_joined_room_events`].
    // reason: `search_joined_room_events` 是协议/存储字段的直传签名：参数逐个来自调用点，打包成结构体只会把同一份字段列表抄两遍
    #[allow(clippy::too_many_arguments)]
    pub async fn search_joined_room_events(
        &self,
        joined_rooms: &[String],
        search_pattern: &str,
        rooms: Option<&[String]>,
        not_rooms: Option<&[String]>,
        event_types: Option<&[String]>,
        senders: Option<&[String]>,
        cursor: Option<(&str, i64)>,
        limit: i64,
    ) -> Result<Vec<(String, String, String, String, serde_json::Value, i64, Option<i64>)>, sqlx::Error> {
        if joined_rooms.is_empty() {
            return Ok(Vec::new());
        }

        // `stream_ordering` is carried out of here (not returned to clients) because
        // it is the only key `m.room.history_visibility` can be evaluated against:
        // the service layer drops hits that predate the caller's membership
        // boundary. See `services/room/messaging/history_visibility.rs`.
        let mut query_builder = QueryBuilder::<Postgres>::new(
            "SELECT event_id, room_id, sender, event_type, content, origin_server_ts, stream_ordering FROM events \
             WHERE soft_failed = FALSE AND ",
        );

        query_builder.push("(content::text ILIKE ");
        query_builder.push_bind(search_pattern);
        query_builder.push(" OR sender ILIKE ");
        query_builder.push_bind(search_pattern);
        query_builder.push(")");

        query_builder.push(" AND room_id IN (");
        {
            let mut separated = query_builder.separated(", ");
            for room in joined_rooms {
                separated.push_bind(room);
            }
        }
        query_builder.push(")");

        let has_explicit_types = event_types.is_some_and(|types| !types.is_empty());
        if !has_explicit_types {
            // Default search surface: messages plus room name/topic, matching the
            // reindexer's type set (upstream Synapse v1.161 #20119 fixed its
            // reindexer to include `m.room.topic`).  Content is matched with
            // `content::text ILIKE`, so the JSON of these state events is
            // searchable without any schema change or backfill.
            query_builder.push(" AND event_type IN ('m.room.message', 'm.room.name', 'm.room.topic')");
        }

        if let Some(rooms) = rooms.filter(|rooms| !rooms.is_empty()) {
            query_builder.push(" AND room_id IN (");
            {
                let mut separated = query_builder.separated(", ");
                for room in rooms {
                    separated.push_bind(room);
                }
            }
            query_builder.push(")");
        }

        if let Some(not_rooms) = not_rooms.filter(|rooms| !rooms.is_empty()) {
            query_builder.push(" AND room_id NOT IN (");
            {
                let mut separated = query_builder.separated(", ");
                for room in not_rooms {
                    separated.push_bind(room);
                }
            }
            query_builder.push(")");
        }

        if let Some(event_types) = event_types.filter(|types| !types.is_empty()) {
            query_builder.push(" AND event_type IN (");
            {
                let mut separated = query_builder.separated(", ");
                for event_type in event_types {
                    separated.push_bind(event_type);
                }
            }
            query_builder.push(")");
        }

        if let Some(senders) = senders.filter(|senders| !senders.is_empty()) {
            query_builder.push(" AND sender IN (");
            {
                let mut separated = query_builder.separated(", ");
                for sender in senders {
                    separated.push_bind(sender);
                }
            }
            query_builder.push(")");
        }

        if let Some((event_id, origin_server_ts)) = cursor {
            query_builder.push(" AND (origin_server_ts, event_id) < (");
            query_builder.push_bind(origin_server_ts);
            query_builder.push(", ");
            query_builder.push_bind(event_id);
            query_builder.push(")");
        }

        query_builder.push(" ORDER BY origin_server_ts DESC, event_id DESC LIMIT ");
        query_builder.push_bind(limit);

        query_builder.build_query_as().fetch_all(&*self.pool).await
    }

    /// See [`search_postgres_messages`].
    pub async fn search_postgres_messages(
        &self,
        user_id: &str,
        query: &str,
        rank_cursor: Option<f64>,
        origin_server_ts_cursor: Option<i64>,
        event_id_cursor: Option<&str>,
        limit: i64,
    ) -> Result<Vec<(String, String, String, String, serde_json::Value, i64, f64)>, sqlx::Error> {
        if let (Some(rank), Some(origin_server_ts), Some(event_id)) =
            (rank_cursor, origin_server_ts_cursor, event_id_cursor)
        {
            // 运行时 `query_as` 而非 `query!`：`event_search` 的 JOIN 是 2026-10-21 才
            // 引入的，宏查询需要同步 `.sqlx` 离线缓存（本机 `SQLX_OFFLINE=true`），
            // 而元组行本身就是 `query!` 的变通写法；改用元组 `FromRow` 后直接拿到
            // `(.., rank: f64)`，不必逐字段断言可空性。
            let rows = sqlx::query_as::<_, (String, String, String, String, serde_json::Value, i64, f64)>(
                r#"
                SELECT
                    e.event_id,
                    e.room_id,
                    e.sender,
                    e.event_type,
                    e.content,
                    e.origin_server_ts,
                    ts_rank(es.vector, plainto_tsquery('english', $2))::float8 as "rank!"
                FROM events e
                INNER JOIN room_memberships rm ON e.room_id = rm.room_id AND rm.user_id = $1 AND rm.membership = 'join'
                -- `key = 'content.body'` 而不是只 join event_id：event_search 每个事件最多
                -- 三行（body/topic/name），这里的 WHERE 已限定 m.room.message，取 body 行
                -- 既避免笛卡尔重复，也让 GIN 只扫一个 key。
                INNER JOIN event_search es ON es.event_id = e.event_id AND es.key = 'content.body'
                WHERE e.event_type = 'm.room.message'
                    AND e.stream_ordering > 0
                    AND e.soft_failed = FALSE
                    AND es.vector @@ plainto_tsquery('english', $2)
                    -- `$3::float8` 是**必须**的：`ts_rank(...)` 的返回类型是 `real`，若直接与
                    -- `$3` 比较，PG 会把参数定型成 `real`，宏因此要求 `f32`，而本方法的游标参数
                    -- （以及上面 `::float8 as "rank!"` 的输出）都是 `f64`（D-74 同族：动态路径靠隐式
                    -- 放宽，宏把参数定型暴露出来）。显式转 `float8` 既保持 API 与精度，也让比较在同一
                    -- 精度域里进行。
                    AND (
                        ts_rank(es.vector, plainto_tsquery('english', $2)) < $3::float8
                        OR (
                            ts_rank(es.vector, plainto_tsquery('english', $2)) = $3::float8
                            AND e.origin_server_ts < $4
                        )
                        OR (
                            ts_rank(es.vector, plainto_tsquery('english', $2)) = $3::float8
                            AND e.origin_server_ts = $4
                            AND e.event_id < $5
                        )
                    )
                -- R6 的陷阱：断言别名 `rank!` 是**真列名**，`ORDER BY rank` 会解析不到 ⇒ 重复表达式。
                ORDER BY ts_rank(es.vector, plainto_tsquery('english', $2)) DESC,
                         e.origin_server_ts DESC, e.event_id DESC
                LIMIT $6
                "#,
            )
            .bind(user_id)
            .bind(query)
            .bind(rank)
            .bind(origin_server_ts)
            .bind(event_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await?;

            Ok(rows)
        } else {
            let rows = sqlx::query_as::<_, (String, String, String, String, serde_json::Value, i64, f64)>(
                r#"
                SELECT
                    e.event_id,
                    e.room_id,
                    e.sender,
                    e.event_type,
                    e.content,
                    e.origin_server_ts,
                    ts_rank(es.vector, plainto_tsquery('english', $2))::float8 as "rank!"
                FROM events e
                INNER JOIN room_memberships rm ON e.room_id = rm.room_id AND rm.user_id = $1 AND rm.membership = 'join'
                -- `key = 'content.body'` 而不是只 join event_id：event_search 每个事件最多
                -- 三行（body/topic/name），这里的 WHERE 已限定 m.room.message，取 body 行
                -- 既避免笛卡尔重复，也让 GIN 只扫一个 key。
                INNER JOIN event_search es ON es.event_id = e.event_id AND es.key = 'content.body'
                WHERE e.event_type = 'm.room.message'
                    AND e.stream_ordering > 0
                    AND e.soft_failed = FALSE
                    AND es.vector @@ plainto_tsquery('english', $2)
                ORDER BY ts_rank(es.vector, plainto_tsquery('english', $2)) DESC,
                         e.origin_server_ts DESC, e.event_id DESC
                LIMIT $3
                "#,
            )
            .bind(user_id)
            .bind(query)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await?;

            Ok(rows)
        }
    }

    /// Verify that the full-text index used by search exists and is valid.
    ///
    /// The index is created by the schema migration (`event_search_fts_idx` on
    /// `event_search`), **not** at startup. It used to be a startup
    /// `CREATE INDEX CONCURRENTLY ... ON events (to_tsvector('english', content))`,
    /// which had two problems: it added write amplification to `events` for an
    /// index that no query uses any more, and it never ran in test/CI databases
    /// (measured 2026-10-21: `synapse_test.events` had 377 indexes and none of
    /// them was `events_fts_idx`).
    ///
    /// What startup still owns is the failure mode: a `CONCURRENTLY` build that
    /// dies leaves an INVALID index behind, and `IF NOT EXISTS` then skips that
    /// name forever — every search silently degrades to a sequential scan. So this
    /// reports a missing or invalid index instead of trusting "the migration ran".
    pub async fn verify_postgres_search_index(&self) -> Result<(), sqlx::Error> {
        // Bind the index to the `event_search` table **this connection resolves**,
        // not to a bare name: `to_regclass('event_search_fts_idx')` resolves
        // through `search_path` and can land on a same-named index in another
        // schema (measured 2026-10-21: an isolated test schema whose index was
        // dropped still looked healthy because `public` had one). Matching on
        // `pg_class.relname` alone is worse still — `pg_class` is database-wide.
        let state: Option<(String, bool)> = sqlx::query_as(
            "SELECT c.relname, i.indisvalid FROM pg_index i JOIN pg_class c ON c.oid = i.indexrelid \
             WHERE i.indrelid = to_regclass('event_search') AND c.relname = 'event_search_fts_idx'",
        )
        .fetch_optional(&*self.pool)
        .await?;
        match state {
            Some((_, true)) => Ok(()),
            Some((name, false)) => Err(sqlx::Error::Protocol(format!(
                "index {name} exists but is INVALID: a concurrent build did not finish. \
                 Drop it with `DROP INDEX CONCURRENTLY {name}` and retry — search would \
                 otherwise silently fall back to a sequential scan."
            ))),
            None => Err(sqlx::Error::Protocol(
                "index event_search_fts_idx is missing: the schema migration that creates \
                 `event_search` has not been applied to this database."
                    .to_string(),
            )),
        }
    }

    /// Room-scoped postgres full-text search.
    /// Named differently from `search_postgres_messages` (which is user-scoped
    /// and returns tuples) to avoid method name collision.
    pub async fn search_room_postgres_messages(
        &self,
        room_id: &str,
        search_term: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        // 运行时 `query_as`：理由同 `search_postgres_messages`（`event_search` 是新的
        // 物理结构，宏查询需要 `.sqlx` 离线缓存）。别名必须按 `RoomEvent` 的
        // `#[sqlx(rename)]` 起名（`processed_at`），运行时 `FromRow` 认 rename，
        // 与 `query_as!` 的行为相反。
        let events = sqlx::query_as::<_, RoomEvent>(
            r#"
            SELECT e.event_id, e.room_id, COALESCE(e.user_id, e.sender) AS user_id, e.event_type, e.content,
                   e.state_key, COALESCE(e.depth, 0) AS depth,
                   COALESCE(e.origin_server_ts, 0) AS origin_server_ts,
                   COALESCE(e.origin_server_ts, 0) AS processed_at,
                   COALESCE(e.not_before, 0) AS not_before, e.status, COALESCE(e.origin, 'self') AS origin,
                   e.stream_ordering, e.redacts, e.unsigned
            FROM events e
            INNER JOIN event_search es ON es.event_id = e.event_id AND es.key = 'content.body'
            WHERE e.room_id = $1
              AND e.event_type = 'm.room.message'
              AND e.soft_failed = FALSE
              AND es.vector @@ plainto_tsquery('english', $2)
            ORDER BY e.origin_server_ts DESC
            LIMIT $3
            "#,
        )
        .bind(room_id)
        .bind(search_term)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(events)
    }
}
