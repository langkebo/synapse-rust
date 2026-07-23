use std::sync::Arc;
use synapse_common::current_timestamp_millis;

use sqlx::PgPool;

use super::models::*;

#[derive(Clone)]
pub struct EventReportStorage {
    pool: Arc<PgPool>,
}

const REPORT_RATE_LIMIT_SELECT: &str = r"
    SELECT
        id,
        user_id,
        report_count,
        last_report_at,
        blocked_until_at,
        is_blocked,
        block_reason,
        created_ts,
        COALESCE(updated_ts, created_ts) AS updated_ts
    FROM report_rate_limits
";

impl EventReportStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_report(&self, request: CreateEventReportRequest) -> Result<EventReport, sqlx::Error> {
        let now = current_timestamp_millis();

        let row = sqlx::query_as::<_, EventReport>(
            r"
            INSERT INTO event_reports (
                event_id, room_id, reporter_user_id, reported_user_id, event_json,
                reason, description, score, received_ts, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'open')
            RETURNING *
            ",
        )
        .bind(&request.event_id)
        .bind(&request.room_id)
        .bind(&request.reporter_user_id)
        .bind(&request.reported_user_id)
        .bind(&request.event_json)
        .bind(&request.reason)
        .bind(&request.description)
        .bind(request.score.unwrap_or(0))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_report(&self, id: i64) -> Result<Option<EventReport>, sqlx::Error> {
        let row = sqlx::query_as::<_, EventReport>("SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason, description, status, score, received_ts, resolved_at, resolved_by, resolution_reason FROM event_reports WHERE id = $1")
            .bind(id)
            .fetch_optional(&*self.pool)
            .await?;

        Ok(row)
    }

    pub async fn get_reports_by_event(&self, event_id: &str) -> Result<Vec<EventReport>, sqlx::Error> {
        let rows = sqlx::query_as::<_, EventReport>(
            "SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason, description, status, score, received_ts, resolved_at, resolved_by, resolution_reason FROM event_reports WHERE event_id = $1 ORDER BY received_ts DESC, id DESC",
        )
        .bind(event_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_reports_by_room(
        &self,
        room_id: &str,
        limit: i64,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        let rows = if let (Some(ts), Some(id)) = (since_ts, since_id) {
            sqlx::query_as::<_, EventReport>(
                "SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason, description, status, score, received_ts, resolved_at, resolved_by, resolution_reason FROM event_reports WHERE room_id = $1 AND (received_ts < $3 OR (received_ts = $3 AND id < $4)) ORDER BY received_ts DESC, id DESC LIMIT $2"
            )
            .bind(room_id)
            .bind(limit)
            .bind(ts)
            .bind(id)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as::<_, EventReport>(
                "SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason, description, status, score, received_ts, resolved_at, resolved_by, resolution_reason FROM event_reports WHERE room_id = $1 ORDER BY received_ts DESC, id DESC LIMIT $2",
            )
            .bind(room_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await?
        };

        Ok(rows)
    }

    pub async fn get_reports_by_reporter(
        &self,
        reporter_user_id: &str,
        limit: i64,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        let rows = if let (Some(ts), Some(id)) = (since_ts, since_id) {
            sqlx::query_as::<_, EventReport>(
                "SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason, description, status, score, received_ts, resolved_at, resolved_by, resolution_reason FROM event_reports WHERE reporter_user_id = $1 AND (received_ts < $3 OR (received_ts = $3 AND id < $4)) ORDER BY received_ts DESC, id DESC LIMIT $2"
            )
            .bind(reporter_user_id)
            .bind(limit)
            .bind(ts)
            .bind(id)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as::<_, EventReport>(
                "SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason, description, status, score, received_ts, resolved_at, resolved_by, resolution_reason FROM event_reports WHERE reporter_user_id = $1 ORDER BY received_ts DESC, id DESC LIMIT $2",
            )
            .bind(reporter_user_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await?
        };

        Ok(rows)
    }

    pub async fn get_reports_by_status(
        &self,
        status: &str,
        limit: i64,
        since_score: Option<i32>,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        let rows = if let (Some(score), Some(ts), Some(id)) = (since_score, since_ts, since_id) {
            sqlx::query_as::<_, EventReport>(
                r"
                SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason, description, status, score, received_ts, resolved_at, resolved_by, resolution_reason FROM event_reports
                WHERE status = $1 AND (
                    score < $3 OR
                    (score = $3 AND received_ts < $4) OR
                    (score = $3 AND received_ts = $4 AND id < $5)
                )
                ORDER BY score DESC, received_ts DESC, id DESC LIMIT $2
                ",
            )
            .bind(status)
            .bind(limit)
            .bind(score)
            .bind(ts)
            .bind(id)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as::<_, EventReport>(
                "SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason, description, status, score, received_ts, resolved_at, resolved_by, resolution_reason FROM event_reports WHERE status = $1 ORDER BY score DESC, received_ts DESC, id DESC LIMIT $2",
            )
            .bind(status)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await?
        };

        Ok(rows)
    }

    pub async fn get_all_reports(
        &self,
        limit: i64,
        since_score: Option<i32>,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        let rows = if let (Some(score), Some(ts), Some(id)) = (since_score, since_ts, since_id) {
            sqlx::query_as::<_, EventReport>(
                r"
                SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason, description, status, score, received_ts, resolved_at, resolved_by, resolution_reason FROM event_reports
                WHERE (
                    score < $2 OR
                    (score = $2 AND received_ts < $3) OR
                    (score = $2 AND received_ts = $3 AND id < $4)
                )
                ORDER BY score DESC, received_ts DESC, id DESC LIMIT $1
                ",
            )
            .bind(limit)
            .bind(score)
            .bind(ts)
            .bind(id)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as::<_, EventReport>(
                "SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason, description, status, score, received_ts, resolved_at, resolved_by, resolution_reason FROM event_reports ORDER BY score DESC, received_ts DESC, id DESC LIMIT $1",
            )
            .bind(limit)
            .fetch_all(&*self.pool)
            .await?
        };

        Ok(rows)
    }

    pub async fn update_report(&self, id: i64, request: UpdateEventReportRequest) -> Result<EventReport, sqlx::Error> {
        let now = current_timestamp_millis();

        let resolved_ts =
            if request.status.as_deref() == Some("resolved") || request.status.as_deref() == Some("dismissed") {
                Some(now)
            } else {
                None
            };

        let row = sqlx::query_as::<_, EventReport>(
            r"
            UPDATE event_reports SET
                status = COALESCE($2, status),
                score = COALESCE($3, score),
                resolved_by = COALESCE($4, resolved_by),
                resolution_reason = COALESCE($5, resolution_reason),
                resolved_at = COALESCE($6, resolved_at)
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id)
        .bind(&request.status)
        .bind(request.score)
        .bind(&request.resolved_by)
        .bind(&request.resolution_reason)
        .bind(resolved_ts)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_report(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM event_reports WHERE id = $1").bind(id).execute(&*self.pool).await?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_history(
        &self,
        report_id: i64,
        action: &str,
        actor_user_id: Option<&str>,
        actor_role: Option<&str>,
        old_status: Option<&str>,
        new_status: Option<&str>,
        reason: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<EventReportHistory, sqlx::Error> {
        let now = current_timestamp_millis();
        tracing::info!(
            report_id = report_id,
            action = action,
            actor = ?actor_user_id,
            old_status = ?old_status,
            new_status = ?new_status,
            "event report history"
        );
        Ok(EventReportHistory {
            id: 0,
            report_id,
            action: action.to_string(),
            actor_user_id: actor_user_id.map(|s| s.to_string()),
            actor_role: actor_role.map(|s| s.to_string()),
            old_status: old_status.map(|s| s.to_string()),
            new_status: new_status.map(|s| s.to_string()),
            reason: reason.map(|s| s.to_string()),
            created_ts: now,
            metadata,
        })
    }

    pub fn get_report_history(&self, _report_id: i64) -> Result<Vec<EventReportHistory>, sqlx::Error> {
        Ok(vec![])
    }

    pub async fn check_rate_limit(&self, user_id: &str) -> Result<ReportRateLimitCheck, sqlx::Error> {
        let limit = sqlx::query_as::<_, ReportRateLimit>(&format!("{REPORT_RATE_LIMIT_SELECT} WHERE user_id = $1"))
            .bind(user_id)
            .fetch_optional(&*self.pool)
            .await?;

        let max_reports_per_day = 50;

        match limit {
            None => Ok(ReportRateLimitCheck {
                is_allowed: true,
                remaining_reports: max_reports_per_day,
                block_reason: None,
            }),
            Some(l) => {
                if l.is_blocked {
                    if let Some(blocked_until) = l.blocked_until_at {
                        let now = current_timestamp_millis();
                        if blocked_until < now {
                            sqlx::query(
                                "UPDATE report_rate_limits SET is_blocked = FALSE, blocked_until_at = NULL, block_reason = NULL, updated_ts = $2 WHERE user_id = $1",
                            )
                                .bind(user_id)
                                .bind(now)
                                .execute(&*self.pool)
                                .await?;
                            return Ok(ReportRateLimitCheck {
                                is_allowed: true,
                                remaining_reports: max_reports_per_day,
                                block_reason: None,
                            });
                        }
                    }
                    return Ok(ReportRateLimitCheck {
                        is_allowed: false,
                        remaining_reports: 0,
                        block_reason: l.block_reason,
                    });
                }

                let one_day_ago = current_timestamp_millis() - 86_400_000;
                if l.last_report_at.is_some_and(|last_report_at| last_report_at > one_day_ago) {
                    if l.report_count >= max_reports_per_day {
                        return Ok(ReportRateLimitCheck {
                            is_allowed: false,
                            remaining_reports: 0,
                            block_reason: Some("Daily report limit exceeded".to_string()),
                        });
                    }
                    return Ok(ReportRateLimitCheck {
                        is_allowed: true,
                        remaining_reports: max_reports_per_day - l.report_count,
                        block_reason: None,
                    });
                }

                Ok(ReportRateLimitCheck {
                    is_allowed: true,
                    remaining_reports: max_reports_per_day,
                    block_reason: None,
                })
            }
        }
    }

    pub async fn record_report(&self, user_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        let one_day_ago = now - 86_400_000;

        let existing = sqlx::query_as::<_, ReportRateLimit>(&format!("{REPORT_RATE_LIMIT_SELECT} WHERE user_id = $1"))
            .bind(user_id)
            .fetch_optional(&*self.pool)
            .await?;

        match existing {
            Some(l) => {
                let new_count = if l.last_report_at.is_none_or(|last_report_at| last_report_at < one_day_ago) {
                    1
                } else {
                    l.report_count + 1
                };

                sqlx::query(
                    "UPDATE report_rate_limits SET report_count = $2, last_report_at = $3, updated_ts = $3 WHERE user_id = $1",
                )
                .bind(user_id)
                .bind(new_count)
                .bind(now)
                .execute(&*self.pool)
                .await?;
            }
            None => {
                sqlx::query(
                    "INSERT INTO report_rate_limits (user_id, report_count, last_report_at, created_ts, updated_ts) VALUES ($1, 1, $2, $2, $2)",
                )
                .bind(user_id)
                .bind(now)
                .execute(&*self.pool)
                .await?;
            }
        }

        Ok(())
    }

    pub async fn block_user_reports(&self, user_id: &str, blocked_until: i64, reason: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query(
            r"
            INSERT INTO report_rate_limits (user_id, is_blocked, blocked_until_at, block_reason, created_ts, updated_ts)
            VALUES ($1, TRUE, $2, $3, $4, $4)
            ON CONFLICT (user_id) DO UPDATE SET
                is_blocked = TRUE,
                blocked_until_at = $2,
                block_reason = $3,
                updated_ts = $4
            ",
        )
        .bind(user_id)
        .bind(blocked_until)
        .bind(reason)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn unblock_user_reports(&self, user_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query(
            "UPDATE report_rate_limits SET is_blocked = FALSE, blocked_until_at = NULL, block_reason = NULL, updated_ts = $2 WHERE user_id = $1",
        )
        .bind(user_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub fn get_stats(&self, _days: i32) -> Result<Vec<EventReportStats>, sqlx::Error> {
        Ok(vec![])
    }

    pub async fn count_reports_by_status(&self, status: &str) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_reports WHERE status = $1")
            .bind(status)
            .fetch_one(&*self.pool)
            .await?;

        Ok(count)
    }

    pub async fn count_all_reports(&self) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_reports").fetch_one(&*self.pool).await?;

        Ok(count)
    }
}
