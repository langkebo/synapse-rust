//! MSC3912: Relational Cascade Redaction support.
//!
//! Implements relationship-based cascade redaction where redacting a parent
//! event triggers redaction of all related events (replies, reactions, threads).
//!
//! References:
//! - MSC3912: https://github.com/matrix-org/matrix-spec-proposals/pull/3912
//! - Matrix Spec: Event Relationships

use super::EventStorage;
use serde_json::Value;

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
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT event_id FROM events
            WHERE (content->>'m.in_reply_to' IS NOT NULL
                   AND content->'m.in_reply_to'->>'event_id' = $1)
               OR (content->'m.relates_to' IS NOT NULL
                   AND content->'m.relates_to'->>'event_id' = $1)
            ORDER BY origin_server_ts ASC, stream_ordering ASC
            LIMIT $2
            "#,
        )
        .bind(event_id)
        .bind(limit)
        .fetch_all(self.pool.as_ref())
        .await?;

        Ok(rows.into_iter().map(|(id,)| id).collect())
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
    /// * `redacted_by` - Optional user ID performing the redaction
    /// * `max_depth` - Maximum recursion depth (default 5)
    ///
    /// # Returns
    /// Number of events successfully redacted
    pub async fn cascade_redact_event(
        &self,
        event_id: &str,
        redacted_by: Option<&str>,
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
            self.redact_event_content(&target_id, redacted_by).await?;
            redacted_count += 1;
        }

        Ok(redacted_count)
    }

    /// Get the full JSON representation of an event for federation redaction.
    ///
    /// This reconstructs the complete PDU including all fields needed for
    /// hash computation and signature verification.
    ///
    /// Only the assembled JSON object is selected. The previous version also
    /// re-selected five of its own inputs (`event_id`, `state_key`, `depth`,
    /// `origin_server_ts`, `origin`) as extra columns and discarded them,
    /// which is what made the row type trip `clippy::type_complexity`.
    pub async fn get_full_event_json(&self, event_id: &str) -> Result<Option<Value>, sqlx::Error> {
        sqlx::query_scalar(
            r#"
                SELECT json_build_object(
                    'event_id', event_id,
                    'type', event_type,
                    'room_id', room_id,
                    'sender', sender,
                    'content', content,
                    'state_key', state_key,
                    'depth', COALESCE(depth, 0),
                    'origin_server_ts', COALESCE(origin_server_ts, 0),
                    'origin', COALESCE(origin, 'self'),
                    'prev_events', COALESCE(prev_events, '[]'::json),
                    'auth_events', COALESCE(auth_events, '[]'::json)
                )
                FROM events
                WHERE event_id = $1
                "#,
        )
        .bind(event_id)
        .fetch_optional(self.pool.as_ref())
        .await
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
