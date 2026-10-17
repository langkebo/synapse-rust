//! DAG traversal methods for [`EventStorage`].

use std::collections::HashSet;

use super::models::PersistedGraphFields;
use super::EventStorage;

impl EventStorage {
    /// Batch-check which event IDs exist locally.  Returns the subset of
    /// `event_ids` that are **missing** from the `events` table.  Used by
    /// the inbound transaction handler to decide whether to trigger
    /// `get_missing_events` against the origin server.
    pub async fn find_missing_event_ids(&self, event_ids: &[String]) -> Result<Vec<String>, sqlx::Error> {
        if event_ids.is_empty() {
            return Ok(Vec::new());
        }
        let existing: Vec<String> = sqlx::query_scalar!(
            r#"
            SELECT event_id FROM events
            WHERE event_id = ANY($1)
            "#,
            event_ids
        )
        .fetch_all(&*self.pool)
        .await?;

        let existing_set: HashSet<&str> = existing.iter().map(|s| s.as_str()).collect();
        let missing = event_ids.iter().filter(|id| !existing_set.contains(id.as_str())).cloned().collect();
        Ok(missing)
    }

    /// Walk `event_edges` to find events that sit between `earliest_events`
    /// and `latest_events` in the DAG — i.e. events that the requester is
    /// missing.  Returns at most `limit` events as JSON values suitable for
    /// the `/get_missing_events` federation response.
    ///
    /// The traversal walks **backwards** from `latest_events` following
    /// `prev_event_id` edges until it hits any of `earliest_events` or
    /// exhausts the reachable sub-graph, collecting events that are not in
    /// `earliest_events` and not in `latest_events`.
    pub async fn get_missing_events_between(
        &self,
        room_id: &str,
        earliest_events: &[String],
        latest_events: &[String],
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        if latest_events.is_empty() {
            return Ok(Vec::new());
        }

        // S18/STO-01: Replace per-event BFS loop (one DB query per DAG node,
        // hundreds of round-trips for deep DAGs) with a single recursive CTE
        // that walks the entire sub-graph in one query.
        //
        // The CTE starts from prev_event_id edges of latest_events, recursively
        // follows prev_event_id edges, and skips earliest_events (WHERE clause
        // excludes them from both base and recursive cases). UNION (not UNION
        // ALL) deduplicates for defensive cycle prevention — Matrix DAGs are
        // acyclic, but malformed data or bugs could create cycles.
        // `dag_walk.event_id` 由 `UNION` 的输出列构成：PG 的 Describe **不给集合运算的输出列
        // 透传 NOT NULL**（R4 ②），而两个分支都取自 `event_edges.prev_event_id`（NOT NULL）
        // ⇒ 按 R4 断言非空。
        let collected: Vec<String> = sqlx::query_scalar!(
            r#"
            WITH RECURSIVE dag_walk AS (
                SELECT ee.prev_event_id AS event_id
                FROM event_edges ee
                WHERE ee.event_id = ANY($1)
                  AND ee.prev_event_id <> ALL($2)

                UNION

                SELECT ee.prev_event_id
                FROM event_edges ee
                INNER JOIN dag_walk dw ON ee.event_id = dw.event_id
                WHERE ee.prev_event_id <> ALL($2)
            )
            SELECT DISTINCT event_id AS "event_id!" FROM dag_walk
            WHERE event_id <> ALL($1)
            LIMIT $3
            "#,
            latest_events,
            earliest_events,
            limit
        )
        .fetch_all(&*self.pool)
        .await?;

        if collected.is_empty() {
            return Ok(Vec::new());
        }

        // Fetch the collected events as JSON values, filtered by room_id for
        // safety (the DAG walk should already be room-scoped, but this
        // prevents any cross-room leakage).
        // 宏按真 catalog 定型：非空列（event_id/room_id/sender/event_type/content/
        // origin_server_ts）给 `T`，可空列（state_key/depth/origin）给 `Option<T>` —— 序列化
        // 结果与原先"一律 `Option`"逐字节相同，但列名/类型错配在编译期就会被证伪。
        let events: Vec<serde_json::Value> = sqlx::query!(
            r#"
            SELECT event_id, room_id, sender, event_type, content, state_key,
                   origin_server_ts, depth, origin
            FROM events
            WHERE room_id = $1 AND event_id = ANY($2)
            ORDER BY origin_server_ts ASC
            LIMIT $3
            "#,
            room_id,
            &collected,
            limit
        )
        .fetch_all(&*self.pool)
        .await?
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "event_id": row.event_id,
                "room_id": row.room_id,
                "sender": row.sender,
                "type": row.event_type,
                "content": row.content,
                "state_key": row.state_key,
                "origin_server_ts": row.origin_server_ts,
                "depth": row.depth,
                "origin": row.origin,
            })
        })
        .collect();

        Ok(events)
    }

    /// See [`get_forward_extremities_count`].
    ///
    /// D-92（先修）：原实现读的是 `content->>'prev_event_id'`，那是**旧的 JSONB 内容约定** ——
    /// 现代写入路径（`create_event_with_graph` / `create_event_with_pdu`）只写
    /// `event_edges`，没有任何生产代码再往 `content` 里塞 `prev_event_id`，于是那个子查询恒为空集、
    /// `NOT IN (空)` 恒真，再加上额外的 `state_key IS NOT NULL` 过滤，它实际返回的是
    /// **"房间里的状态事件数"**，与 [`Self::get_forward_extremities_in_room`] 的"DAG 叶节点"不是
    /// 同一个概念（同一职责的第二份实现，铁律 2；管理员端点的 `forward_extremities` 字段因此长期
    /// 报错数）。改为与 `_in_room` 同一定义（`event_edges` 上的 `NOT EXISTS`），两者永远一致。
    pub async fn get_forward_extremities_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        // `COUNT(*)` 无关系来源 ⇒ Describe 不透传 NOT NULL（R4 ①）⇒ 断言（计数恒不为 NULL）。
        let count: i64 = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!" FROM events e
            WHERE e.room_id = $1
              AND NOT EXISTS (
                  SELECT 1 FROM event_edges g
                  WHERE g.prev_event_id = e.event_id
              )
            "#,
            room_id
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    /// The room's **forward extremities**: events that no other event in the
    /// room references as a parent.
    ///
    /// This is the set a newly-created event must list in `prev_events` to
    /// extend every branch of the room DAG. It is derived from `event_edges`,
    /// which the graph write paths populate (`create_event_with_graph` /
    /// `create_event_with_pdu`), so it is a real DAG query — unlike
    /// [`Self::get_latest_event_ids_in_room`], which merely returns the newest
    /// events by `origin_server_ts` for backfill seeding and would report an
    /// ancestor as a tip.
    ///
    /// ⚠️ Rows written without graph metadata (the plain `create_event` path)
    /// have no `event_edges` at all and therefore look like extremities. Rooms
    /// created before graph metadata was persisted can over-report; rooms
    /// created after (every local write now goes through a graph path) do not.
    ///
    /// Ordering is newest-first with a deterministic tie-break so callers get a
    /// reproducible `prev_events` array.
    pub async fn get_forward_extremities_in_room(&self, room_id: &str, limit: i64) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_scalar!(
            r"
            SELECT e.event_id FROM events e
            WHERE e.room_id = $1
              AND NOT EXISTS (
                  SELECT 1 FROM event_edges g
                  WHERE g.prev_event_id = e.event_id
              )
            ORDER BY e.origin_server_ts DESC NULLS LAST, e.stream_ordering DESC NULLS LAST, e.event_id DESC
            LIMIT $2
            ",
            room_id,
            limit
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    /// The graph fields persisted for one event.
    ///
    /// `Ok(None)` means no such event row. The inner `Option`s are `None` when
    /// the row was written without graph metadata (plain `create_event`), which
    /// callers must treat as "cannot build a PDU" rather than papering over.
    pub async fn get_event_graph_fields(&self, event_id: &str) -> Result<Option<PersistedGraphFields>, sqlx::Error> {
        let row = sqlx::query!("SELECT depth, prev_events, auth_events FROM events WHERE event_id = $1", event_id)
            .fetch_optional(&*self.pool)
            .await?;

        Ok(row.map(|row| PersistedGraphFields {
            depth: row.depth,
            prev_events: row.prev_events,
            auth_events: row.auth_events,
        }))
    }

    /// Returns the `event_id`s of the most recent events in a room, ordered
    /// by `origin_server_ts DESC`.  Used to seed outbound `/backfill` requests
    /// — the caller passes these IDs as the `v=` query parameters so the
    /// remote server knows which point in the DAG to walk backwards from.
    pub async fn get_latest_event_ids_in_room(&self, room_id: &str, limit: i64) -> Result<Vec<String>, sqlx::Error> {
        // 单列 ⇒ `query_scalar!`（`query_as!` 不构造元组，R6 ⑤）。
        let rows = sqlx::query_scalar!(
            r#"
            SELECT event_id FROM events
            WHERE room_id = $1
            ORDER BY origin_server_ts DESC NULLS LAST, stream_ordering DESC NULLS LAST, event_id DESC
            LIMIT $2
            "#,
            room_id,
            limit
        )
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows)
    }
}
