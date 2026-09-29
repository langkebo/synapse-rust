//! DAG traversal methods for [`EventStorage`].

use std::collections::HashSet;

use sqlx::Row;

use super::models::PersistedGraphFields;
use super::EventStorage;

/// Decode `events.prev_state_events`（JSONB，形如 `["$e1", "$e2"]`）。
///
/// D-91（先修）：两处调用点原先都写 `serde_json::from_value(json).unwrap_or_default()`，
/// 把"列里的 JSON 形状不对"静默降级成"没有前驱状态事件" —— 与"该列本就为 NULL"无法区分，
/// 属数据路径上的吞错（`unwrap_or_default` 的典型形态，与 D-33/D-72 同族）。
/// 现在 fail-closed：形状不对返回 `sqlx::Error::Decode`，让脏数据在调用点可见，
/// 而不是被当成"这个事件没有状态前驱"继续参与 DAG 遍历。
fn prev_state_events_from_json(event_id: &str, json: serde_json::Value) -> Result<Vec<String>, sqlx::Error> {
    serde_json::from_value(json).map_err(|e| {
        sqlx::Error::Decode(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("events.prev_state_events for {event_id} is not a JSON array of strings: {e}"),
        )))
    })
}

impl EventStorage {
    /// Batch-check which event IDs exist locally.  Returns the subset of
    /// `event_ids` that are **missing** from the `events` table.  Used by
    /// the inbound transaction handler to decide whether to trigger
    /// `get_missing_events` against the origin server.
    pub async fn find_missing_event_ids(&self, event_ids: &[String]) -> Result<Vec<String>, sqlx::Error> {
        if event_ids.is_empty() {
            return Ok(Vec::new());
        }
        let existing: Vec<String> = sqlx::query_scalar(
            r"
            SELECT event_id FROM events
            WHERE event_id = ANY($1)
            ",
        )
        .bind(event_ids)
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
        let collected: Vec<String> = sqlx::query_scalar(
            r"
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
            SELECT DISTINCT event_id FROM dag_walk
            WHERE event_id <> ALL($1)
            LIMIT $3
            ",
        )
        .bind(latest_events)
        .bind(earliest_events)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        if collected.is_empty() {
            return Ok(Vec::new());
        }

        // Fetch the collected events as JSON values, filtered by room_id for
        // safety (the DAG walk should already be room-scoped, but this
        // prevents any cross-room leakage).
        let events: Vec<serde_json::Value> = sqlx::query(
            r"
            SELECT event_id, room_id, sender, event_type, content, state_key,
                   origin_server_ts, depth, origin
            FROM events
            WHERE room_id = $1 AND event_id = ANY($2)
            ORDER BY origin_server_ts ASC
            LIMIT $3
            ",
        )
        .bind(room_id)
        .bind(&collected)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "event_id": row.get::<Option<String>, _>("event_id"),
                "room_id": row.get::<Option<String>, _>("room_id"),
                "sender": row.get::<Option<String>, _>("sender"),
                "type": row.get::<Option<String>, _>("event_type"),
                "content": row.get::<Option<serde_json::Value>, _>("content"),
                "state_key": row.get::<Option<String>, _>("state_key"),
                "origin_server_ts": row.get::<Option<i64>, _>("origin_server_ts"),
                "depth": row.get::<Option<i64>, _>("depth"),
                "origin": row.get::<Option<String>, _>("origin"),
            })
        })
        .collect();

        Ok(events)
    }

    /// See [`get_forward_extremities_count`].
    ///
    /// D-92（先修）：原实现读的是 `content->>'prev_event_id'`，那是**旧的 JSONB 内容约定** ——
    /// 现代写入路径（`create_event_with_graph` / `create_state_event_with_dag`）只写
    /// `event_edges`，没有任何生产代码再往 `content` 里塞 `prev_event_id`，于是那个子查询恒为空集、
    /// `NOT IN (空)` 恒真，再加上额外的 `state_key IS NOT NULL` 过滤，它实际返回的是
    /// **"房间里的状态事件数"**，与 [`Self::get_forward_extremities_in_room`] 的"DAG 叶节点"不是
    /// 同一个概念（同一职责的第二份实现，铁律 2；管理员端点的 `forward_extremities` 字段因此长期
    /// 报错数）。改为与 `_in_room` 同一定义（`event_edges` 上的 `NOT EXISTS`），两者永远一致。
    pub async fn get_forward_extremities_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            r"
            SELECT COUNT(*) FROM events e
            WHERE e.room_id = $1
              AND NOT EXISTS (
                  SELECT 1 FROM event_edges g
                  WHERE g.prev_event_id = e.event_id
              )
            ",
        )
        .bind(room_id)
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
    /// `create_state_event_with_dag`), so it is a real DAG query — unlike
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
        let rows: Vec<(String,)> = sqlx::query_as(
            r"
            SELECT event_id FROM events
            WHERE room_id = $1
            ORDER BY origin_server_ts DESC NULLS LAST, stream_ordering DESC NULLS LAST, event_id DESC
            LIMIT $2
            ",
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    // =========================================================================
    // MSC4242 State DAG methods (P2-14)
    // =========================================================================

    /// Get the `prev_state_events` for a given event (MSC4242 State DAG).
    ///
    /// Returns `None` if the event does not exist or has no `prev_state_events`
    /// (i.e. it is not a state event in an MSC4242 room version).
    ///
    /// This is the state-DAG equivalent of reading `prev_events` for the room
    /// DAG. The returned event IDs form the edges of the state DAG.
    pub async fn get_prev_state_events(&self, event_id: &str) -> Result<Option<Vec<String>>, sqlx::Error> {
        let row: Option<(Option<serde_json::Value>,)> =
            sqlx::query_as("SELECT prev_state_events FROM events WHERE event_id = $1")
                .bind(event_id)
                .fetch_optional(&*self.pool)
                .await?;

        match row {
            None => Ok(None),
            Some((None,)) => Ok(None),
            Some((Some(json),)) => {
                let ids = prev_state_events_from_json(event_id, json)?;
                if ids.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(ids))
                }
            }
        }
    }

    /// Get the state DAG edges for a room — all `(event_id, prev_state_event_id)`
    /// pairs where `prev_state_events` is non-NULL.
    ///
    /// Used by `/get_missing_events` to walk the state DAG
    /// when backfilling missing state events (MSC4242).
    ///
    /// ⚠️ **Note**: The comment previously claimed this was used by `/send_join`,
    /// but `/send_join` does not call this function. The only production caller
    /// is `federation/events.rs` (via `get_missing_events_between`).
    ///
    /// Returns a flat list of `(event_id, prev_state_event_id)` edges.
    pub async fn get_state_dag_edges(&self, room_id: &str) -> Result<Vec<(String, String)>, sqlx::Error> {
        let rows: Vec<(String, serde_json::Value)> = sqlx::query_as(
            r"
            SELECT event_id, prev_state_events
            FROM events
            WHERE room_id = $1 AND prev_state_events IS NOT NULL
            ORDER BY origin_server_ts ASC, stream_ordering ASC
            ",
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        let mut edges = Vec::new();
        for (event_id, prev_json) in rows {
            let prev_ids = prev_state_events_from_json(&event_id, prev_json)?;
            for prev_id in prev_ids {
                edges.push((event_id.clone(), prev_id));
            }
        }
        Ok(edges)
    }

    /// Find state events in a room that reference any of `missing_event_ids`
    /// in their `prev_state_events`. Used by the `/get_missing_events`
    /// federation handler to determine which state DAG events need backfilling
    /// (MSC4242 mandates servers fill in unknown `prev_state_events`).
    ///
    /// Returns the event IDs that reference at least one missing event.
    pub async fn find_events_referencing_missing_state(
        &self,
        room_id: &str,
        missing_event_ids: &[String],
    ) -> Result<Vec<String>, sqlx::Error> {
        if missing_event_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<(String,)> = sqlx::query_as(
            r"
            SELECT event_id
            FROM events
            WHERE room_id = $1
              AND prev_state_events IS NOT NULL
              AND prev_state_events ?| $2::text[]
            ",
        )
        .bind(room_id)
        .bind(missing_event_ids)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }
}
