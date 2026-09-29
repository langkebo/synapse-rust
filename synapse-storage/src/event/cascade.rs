//! MSC3912: Relational Cascade Redaction support.
//!
//! Implements relationship-based cascade redaction where redacting a parent
//! event triggers redaction of all related events (replies, reactions, threads).
//!
//! References:
//! - MSC3912: https://github.com/matrix-org/matrix-spec-proposals/pull/3912
//! - Matrix Spec: Event Relationships

use super::EventStorage;

impl EventStorage {
    /// Find all events that reference the given event_id via relationship fields.
    ///
    /// Looks for:
    /// - `m.in_reply_to` → `event_id` field
    /// - `m.relates_to` → `event_id` field (for reactions, threads)
    ///
    /// Returns event IDs ordered by origin_server_ts (oldest first).
    ///
    /// `origin_server_ts` is **not** unique: two events in the same room can
    /// share a millisecond, and PostgreSQL is then free to return them in any
    /// order. `stream_ordering` is the repository's canonical monotone
    /// tie-break, so it is appended as the second key — the cascade walks
    /// reply/reaction chains, and an unstable order there makes the traversal
    /// (and therefore `redacted_count`) non-reproducible.
    pub async fn find_related_events(&self, event_id: &str, limit: i64) -> Result<Vec<String>, sqlx::Error> {
        // 单列投影 ⇒ `query_scalar!`（R6 ⑤：`query_as!` 不能构造元组；`events.event_id`
        // 是 `TEXT NOT NULL` ⇒ 直接得到 `Vec<String>`，原先的 `map(|(id,)| id)` 随之消失）。
        let rows = sqlx::query_scalar!(
            r#"
            SELECT event_id FROM events
            WHERE (content->>'m.in_reply_to' IS NOT NULL
                   AND content->'m.in_reply_to'->>'event_id' = $1)
               OR (content->'m.relates_to' IS NOT NULL
                   AND content->'m.relates_to'->>'event_id' = $1)
            ORDER BY origin_server_ts ASC, stream_ordering ASC
            LIMIT $2
            "#,
            event_id,
            limit,
        )
        .fetch_all(self.pool.as_ref())
        .await?;

        Ok(rows)
    }

    /// MSC3912: Find related events at a single level (no recursion).
    ///
    /// This is the storage layer for the MSC3912 client-side cascade redaction.
    /// Unlike [`find_related_events`], this method:
    /// - Filters by `rel_type` (e.g., "m.replace", "m.thread", "m.annotation")
    /// - Excludes the target event itself
    /// - Excludes already-redacted events
    /// - Supports wildcard (`*`) to match all rel_types
    ///
    /// # Arguments
    /// * `room_id` - Room to search in
    /// * `event_id` - Target event ID
    /// * `rel_types` - List of relationship types to match (use `["*"]` for all)
    ///
    /// # Returns
    /// Event IDs of related events (single layer only)
    ///
    /// # MSC3912 Compliance Note
    /// The wildcard (`["*"]`) matches **only** `m.relates_to` (MSC3912 standard).
    /// The legacy `m.in_reply_to` field is **deprecated** and intentionally excluded
    /// from wildcard matching. If you need to match legacy reply events, explicitly
    /// pass `["m.in_reply_to"]` as a specific rel_type (though this will fail
    /// the `rel_type` filter since `m.in_reply_to` events do not have a `rel_type` field).
    pub async fn find_related_events_single_layer(
        &self,
        room_id: &str,
        event_id: &str,
        rel_types: &[String],
    ) -> Result<Vec<String>, sqlx::Error> {
        // Wildcard: match all rel_types (MSC3912 standard: m.relates_to only)
        if rel_types.len() == 1 && rel_types[0] == "*" {
            let rows = sqlx::query_scalar!(
                r#"
                SELECT event_id FROM events
                WHERE room_id = $1
                  AND event_id != $2
                  AND is_redacted = false
                  AND content->'m.relates_to' IS NOT NULL
                  AND content->'m.relates_to'->>'event_id' = $2
                ORDER BY origin_server_ts ASC, stream_ordering ASC
                "#,
                room_id,
                event_id
            )
            .fetch_all(self.pool.as_ref())
            .await?;
            return Ok(rows.into_iter().collect());
        }

        // Specific rel_types: filter by rel_type field
        let rows = sqlx::query_scalar!(
            r#"
            SELECT event_id FROM events
            WHERE room_id = $1
              AND event_id != $2
              AND is_redacted = false
              AND content->'m.relates_to' IS NOT NULL
              AND content->'m.relates_to'->>'event_id' = $2
              AND content->'m.relates_to'->>'rel_type' = ANY($3)
            ORDER BY origin_server_ts ASC, stream_ordering ASC
            "#,
            room_id,
            event_id,
            rel_types
        )
        .fetch_all(self.pool.as_ref())
        .await?;

        Ok(rows.into_iter().collect())
    }

    /// Recursively find all descendant events that should be redacted when
    /// the given event is redacted.
    ///
    /// Uses BFS to traverse the relationship graph up to max_depth levels.
    /// Returns all event IDs including the original.
    ///
    /// The original `event_id` is intentionally **not** pre-seeded into the
    /// `visited` set: the seed is dequeued on the first level and inserted
    /// there, so the root is both filtered against cycles and included in the
    /// result. Pre-seeding it (the previous behaviour) made the first-level
    /// `visited.insert(root)` return `false`, so the root was silently dropped
    /// and [`Self::cascade_redact_event`] never redacted the event the admin
    /// asked for.
    pub async fn find_cascade_targets(&self, event_id: &str, max_depth: u32) -> Result<Vec<String>, sqlx::Error> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut queue = vec![event_id.to_string()];

        for _ in 0..max_depth {
            if queue.is_empty() {
                break;
            }

            let current_level: Vec<String> = std::mem::take(&mut queue);
            let mut next_level = Vec::new();

            for id in current_level {
                // Skip if already processed; the root lands here on the first
                // iteration and is therefore part of `result`.
                if !visited.insert(id.clone()) {
                    continue;
                }
                result.push(id.clone());

                // Find related events
                let related = self.find_related_events(&id, 1000).await?;
                next_level.extend(related);
            }

            queue = next_level;
        }

        Ok(result)
    }

    /// Perform cascade redaction on all events related to the target event.
    ///
    /// This is the main entry point for MSC3912 cascade redaction.
    /// Returns the number of events redacted.
    ///
    /// # Arguments
    /// * `event_id` - The event to redact and cascade from
    /// * `redaction_event_id` - The id of the `m.room.redaction` event that
    ///   caused this redaction (a self-referential FK to `events.event_id`),
    ///   or `None` when the redaction has no causing event (an operator/server
    ///   action such as the admin cascade endpoint)
    /// * `max_depth` - Maximum recursion depth (default 5)
    ///
    /// # Returns
    /// Number of events successfully redacted
    pub async fn cascade_redact_event(
        &self,
        event_id: &str,
        redaction_event_id: Option<&str>,
        max_depth: u32,
    ) -> Result<u64, sqlx::Error> {
        // Find all cascade targets
        let targets = self.find_cascade_targets(event_id, max_depth).await?;

        if targets.is_empty() {
            return Ok(0);
        }

        // Redact each event
        let mut redacted_count = 0u64;
        for target_id in targets {
            self.redact_event_content(&target_id, redaction_event_id).await?;
            redacted_count += 1;
        }

        Ok(redacted_count)
    }
}

#[cfg(test)]
mod tests {
    // Note: these paths require a real database connection, so the DB-backed
    // coverage lives in `synapse-storage/src/event/db_tests.rs` (gated on the
    // integration lane). This module previously held a `use super::*;` pair
    // whose only body was a commented-out block, which `-D warnings` rejected
    // as unused imports.
}
