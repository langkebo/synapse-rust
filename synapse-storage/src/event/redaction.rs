//! Event report and redaction methods for [`EventStorage`].

use super::models::{EventReport, EventReportId};
use super::EventStorage;
use synapse_common::current_timestamp_millis;

impl EventStorage {
    /// See [`report_event`].
    pub async fn report_event(
        &self,
        event_id: &str,
        room_id: &str,
        _reported_user_id: &str,
        reporter_user_id: &str,
        reason: Option<&str>,
        score: i32,
    ) -> Result<i64, sqlx::Error> {
        let now = current_timestamp_millis();
        let row = sqlx::query_as!(
            EventReportId,
            r"
            INSERT INTO event_reports (event_id, room_id, reporter_user_id, reason, score, received_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            ",
            event_id,
            room_id,
            reporter_user_id,
            reason,
            score,
            now,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(row.id)
    }

    /// See [`update_event_report_score`].
    pub async fn update_event_report_score(&self, report_id: i64, score: i32) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            UPDATE event_reports SET score = $1 WHERE id = $2
            ",
            score,
            report_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`update_event_report_score_by_event`].
    pub async fn update_event_report_score_by_event(&self, event_id: &str, score: i32) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            UPDATE event_reports SET score = $1 WHERE event_id = $2
            ",
            score,
            event_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`get_event_report`].
    pub async fn get_event_report(&self, event_id: &str) -> Result<Vec<EventReport>, sqlx::Error> {
        // ⚠️ R6/D-19：`EventReport.resolved_ts` 靠 `#[sqlx(rename = "resolved_at")]` 与列对齐，
        // 而 `query_as!` **不认 rename** ⇒ SQL 里显式写 `resolved_at AS "resolved_ts"`。
        // R4：`score` 列可空（无 NOT NULL）而字段是非 `Option` 的 `i32` ⇒ 断言 `AS "score!"`；
        // "谁保证非空"= 唯一写者（`report_event` 的 INSERT 与两处 UPDATE 都显式写入该列）。
        sqlx::query_as!(
            EventReport,
            r#"
            SELECT id, event_id, room_id, reporter_user_id, reason, score AS "score!", received_ts,
                   resolved_at AS "resolved_ts", resolved_by
            FROM event_reports WHERE event_id = $1 ORDER BY received_ts DESC
            "#,
            event_id,
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// Redacts an event's content in-place according to the Matrix redaction
    /// rules for room versions 1-10 (P0-06).
    ///
    /// Unlike the previous implementation which cleared content to `{}`, this
    /// fetches the event type and retains the spec-mandated fields per event
    /// type (e.g. `membership` for `m.room.member`, `users`/`ban`/... for
    /// `m.room.power_levels`).  This keeps redacted state events functional
    /// and matches Synapse/Synapse-Rust federation hash computation.
    ///
    /// It is also **the single chokepoint** through which an event in `events`
    /// becomes redacted (the only other `is_redacted = TRUE` writes in this
    /// crate target `thread_replies` / `event_relations`, not `events`), which
    /// is why the MSC4354 un-stick below lives here rather than at each of the
    /// four callers (client handler, MSC3912 cascade, admin batch, worker).
    ///
    /// `redaction_event_id` is the id of the `m.room.redaction` event that
    /// caused this redaction — a self-referential FK to `events.event_id`
    /// (`fk_events_redacted_by`), **not** a user id. It is `None` when the
    /// redaction has no causing event (a server/operator action).
    pub async fn redact_event_content(
        &self,
        event_id: &str,
        redaction_event_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        // Fetch the event type and content so we can apply the per-type
        // retention table from synapse_common::redaction.
        // 用 `query!` 而不是元组版 `query_as!`：`query_as!` 不能构造元组（它按字段构造结构体）。
        let row = sqlx::query!("SELECT event_type, content FROM events WHERE event_id = $1", event_id)
            .fetch_optional(&*self.pool)
            .await?;

        let Some(row) = row else {
            // Event not found — nothing to redact.  This is benign for
            // federation redaction PDUs that target events we don't have.
            // Nothing can be pinned to a row that does not exist either, so the
            // MSC4354 cleanup below has nothing to do in this case.
            return Ok(());
        };

        let redacted_content = synapse_common::redaction::redact_content(&row.event_type, &row.content);
        let now = current_timestamp_millis();

        // Redacting the event and un-sticking it are one unit of work: a
        // half-applied pair would leave a pin pointing at content that no longer
        // exists (and a client rendering a "pinned" row it cannot fetch).
        let mut tx = self.pool.begin().await?;

        sqlx::query!(
            "UPDATE events SET content = $1, is_redacted = true, redacted_at = $2, redacted_by = $3 WHERE event_id = $4",
            &redacted_content,
            now,
            redaction_event_id,
            event_id,
        )
        .execute(&mut *tx)
        .await?;

        // MSC4354 Sticky Events / upstream #20323: "Remove the stickiness of
        // Sticky Events when they are redacted after persistence."
        //
        // The delete is keyed on `event_id` alone, across every user and room
        // that pinned it: a redacted event is redacted for everyone, so keeping
        // the pin for the *other* users who had pinned it would be the same bug
        // one row over. Upstream does the same
        // (`sticky_events.py::delete_sticky_events_txn`, called from the event
        // persistence path) and likewise deletes rather than clearing a flag.
        //
        // ⚠️ This is the only writer of `room_sticky_events` outside
        // `sticky_event.rs`, and that is deliberate: enforcing the invariant at
        // the one redaction chokepoint is what makes it impossible for a future
        // redaction path to forget it (four callers exist today).
        //
        // There is no index on `room_sticky_events.event_id`, so this is a scan
        // of that table. It is deliberately left unindexed: the table holds one
        // row per `(room_id, user_id, event_type)` ever pinned — upserted in
        // place, bounded by users × rooms × event types — unlike upstream's
        // append-only `sticky_events`, whose missing index is why they route the
        // lookup through `events.stream_ordering`.
        sqlx::query!("DELETE FROM room_sticky_events WHERE event_id = $1", event_id).execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(())
    }

    /// Admin Redact API: Find event IDs in a room within an optional time
    /// range for batch redaction.
    ///
    /// - `before_ts`: only redact events with `origin_server_ts < before_ts`
    /// - `after_ts`: only redact events with `origin_server_ts > after_ts`
    /// - `limit`: cap the number of events returned (default 1000)
    ///
    /// Excludes already-redacted events and `m.room.create` (cannot redact
    /// the room create event without destroying the room).
    pub async fn find_event_ids_for_redaction(
        &self,
        room_id: &str,
        before_ts: Option<i64>,
        after_ts: Option<i64>,
        limit: i64,
    ) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<String> = match (before_ts, after_ts) {
            (Some(before), Some(after)) => {
                sqlx::query_scalar!(
                    r"
                    SELECT event_id FROM events
                    WHERE room_id = $1
                      AND is_redacted = false
                      AND event_type != 'm.room.create'
                      AND COALESCE(origin_server_ts, 0) < $2
                      AND COALESCE(origin_server_ts, 0) > $3
                    ORDER BY origin_server_ts ASC
                    LIMIT $4
                    ",
                    room_id,
                    before,
                    after,
                    limit,
                )
                .fetch_all(&*self.pool)
                .await?
            }
            (Some(before), None) => {
                sqlx::query_scalar!(
                    r"
                    SELECT event_id FROM events
                    WHERE room_id = $1
                      AND is_redacted = false
                      AND event_type != 'm.room.create'
                      AND COALESCE(origin_server_ts, 0) < $2
                    ORDER BY origin_server_ts ASC
                    LIMIT $3
                    ",
                    room_id,
                    before,
                    limit,
                )
                .fetch_all(&*self.pool)
                .await?
            }
            (None, Some(after)) => {
                sqlx::query_scalar!(
                    r"
                    SELECT event_id FROM events
                    WHERE room_id = $1
                      AND is_redacted = false
                      AND event_type != 'm.room.create'
                      AND COALESCE(origin_server_ts, 0) > $2
                    ORDER BY origin_server_ts ASC
                    LIMIT $3
                    ",
                    room_id,
                    after,
                    limit,
                )
                .fetch_all(&*self.pool)
                .await?
            }
            (None, None) => {
                sqlx::query_scalar!(
                    r"
                    SELECT event_id FROM events
                    WHERE room_id = $1
                      AND is_redacted = false
                      AND event_type != 'm.room.create'
                    ORDER BY origin_server_ts ASC
                    LIMIT $2
                    ",
                    room_id,
                    limit,
                )
                .fetch_all(&*self.pool)
                .await?
            }
        };
        Ok(rows)
    }

    /// Admin Redact API: Batch redact multiple events by event_id.
    ///
    /// Returns the number of events redacted. Uses individual UPDATEs rather
    /// than a bulk UPDATE because each event type has a different redaction
    /// retention table (see `redact_content`).
    ///
    /// `redaction_event_id` is the id of the `m.room.redaction` event that
    /// caused this redaction (a self-referential FK to `events.event_id`), or
    /// `None` for an operator/server action with no causing event.
    pub async fn batch_redact_events(
        &self,
        event_ids: &[String],
        redaction_event_id: Option<&str>,
    ) -> Result<u64, sqlx::Error> {
        let mut redacted = 0u64;
        for event_id in event_ids {
            self.redact_event_content(event_id, redaction_event_id).await?;
            redacted += 1;
        }
        Ok(redacted)
    }
}
