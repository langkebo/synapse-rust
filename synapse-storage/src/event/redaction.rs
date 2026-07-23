//! Event report and redaction methods for [`EventStorage`].

use super::models::{EventReport, EventReportId};
use super::EventStorage;
use synapse_common::current_timestamp_millis;

impl EventStorage {
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
        let row = sqlx::query_as::<_, EventReportId>(
            r"
            INSERT INTO event_reports (event_id, room_id, reporter_user_id, reason, score, received_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            ",
        )
        .bind(event_id)
        .bind(room_id)
        .bind(reporter_user_id)
        .bind(reason)
        .bind(score)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;
        Ok(row.id)
    }

    pub async fn update_event_report_score(&self, report_id: i64, score: i32) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE event_reports SET score = $1 WHERE id = $2
            ",
        )
        .bind(score)
        .bind(report_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_event_report_score_by_event(&self, event_id: &str, score: i32) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE event_reports SET score = $1 WHERE event_id = $2
            ",
        )
        .bind(score)
        .bind(event_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_event_report(&self, event_id: &str) -> Result<Vec<EventReport>, sqlx::Error> {
        sqlx::query_as::<_, EventReport>(
            r"
            SELECT id, event_id, room_id, reporter_user_id, reason, score, received_ts, resolved_at, resolved_by
            FROM event_reports WHERE event_id = $1 ORDER BY received_ts DESC
            ",
        )
        .bind(event_id)
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
    /// `redacted_by` optionally records the user_id of the redactor.
    pub async fn redact_event_content(&self, event_id: &str, redacted_by: Option<&str>) -> Result<(), sqlx::Error> {
        // Fetch the event type and content so we can apply the per-type
        // retention table from synapse_common::redaction.
        let row: Option<(String, serde_json::Value)> =
            sqlx::query_as("SELECT event_type, content FROM events WHERE event_id = $1")
                .bind(event_id)
                .fetch_optional(&*self.pool)
                .await?;

        let Some((event_type, content)) = row else {
            // Event not found — nothing to redact.  This is benign for
            // federation redaction PDUs that target events we don't have.
            return Ok(());
        };

        let redacted_content = synapse_common::redaction::redact_content(&event_type, &content);
        let now = current_timestamp_millis();

        sqlx::query(
            "UPDATE events SET content = $1, is_redacted = true, redacted_at = $2, redacted_by = $3 WHERE event_id = $4",
        )
        .bind(&redacted_content)
        .bind(now)
        .bind(redacted_by)
        .bind(event_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }
}
