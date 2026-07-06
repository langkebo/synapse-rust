//! DAG traversal methods for [`EventStorage`].

use std::collections::{HashSet, VecDeque};

use sqlx::Row;

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

        let earliest_set: HashSet<&str> = earliest_events.iter().map(|s| s.as_str()).collect();

        // BFS backwards from latest_events via event_edges.prev_event_id,
        // stopping at earliest_events.  Collect visited event IDs that are
        // neither in earliest_events nor in latest_events.
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = latest_events.iter().cloned().collect();
        let mut collected: Vec<String> = Vec::new();

        for id in latest_events {
            visited.insert(id.clone());
        }

        while let Some(current) = queue.pop_front() {
            if collected.len() as i64 >= limit {
                break;
            }

            // Walk prev_event_id edges for `current`.
            let prev_ids: Vec<String> = sqlx::query_scalar(
                r"
                SELECT prev_event_id FROM event_edges
                WHERE event_id = $1
                ",
            )
            .bind(&current)
            .fetch_all(&*self.pool)
            .await?;

            for prev_id in prev_ids {
                if earliest_set.contains(prev_id.as_str()) {
                    continue;
                }
                if visited.insert(prev_id.clone()) {
                    collected.push(prev_id.clone());
                    queue.push_back(prev_id);
                }
            }
        }

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

    pub async fn get_forward_extremities_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            r"
            SELECT COUNT(*) FROM events
            WHERE room_id = $1
            AND state_key IS NOT NULL
            AND event_id NOT IN (
                SELECT content->>'prev_event_id' FROM events
                WHERE room_id = $1 AND content->>'prev_event_id' IS NOT NULL
            )
            ",
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
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
}
