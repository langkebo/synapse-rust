use std::sync::Arc;
use synapse_common::current_timestamp_millis;

use sqlx::PgPool;

use super::models::*;

/// The `EventReportStorage` struct.
#[derive(Clone)]
pub struct EventReportStorage {
    pool: Arc<PgPool>,
}

impl EventReportStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`create_report`].
    pub async fn create_report(&self, request: CreateEventReportRequest) -> Result<EventReport, sqlx::Error> {
        let now = current_timestamp_millis();

        let row = sqlx::query_as!(
            EventReport,
            r#"
            INSERT INTO event_reports (
                event_id, room_id, reporter_user_id, reported_user_id, event_json,
                reason, description, score, received_ts, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'open')
            RETURNING id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason,
                      description, status AS "status!", score AS "score!", received_ts,
                      resolved_at AS "resolved_ts", resolved_by, resolution_reason
            "#,
            request.event_id.as_str(),
            request.room_id.as_str(),
            request.reporter_user_id.as_str(),
            request.reported_user_id.as_deref(),
            request.event_json.as_ref(),
            request.reason.as_deref(),
            request.description.as_deref(),
            request.score.unwrap_or(0),
            now,
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`get_report`].
    pub async fn get_report(&self, id: i64) -> Result<Option<EventReport>, sqlx::Error> {
        let row = sqlx::query_as!(
            EventReport,
            r#"
            SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason,
                   description, status AS "status!", score AS "score!", received_ts,
                   resolved_at AS "resolved_ts", resolved_by, resolution_reason
            FROM event_reports WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`get_reports_by_event`].
    pub async fn get_reports_by_event(&self, event_id: &str) -> Result<Vec<EventReport>, sqlx::Error> {
        let rows = sqlx::query_as!(
            EventReport,
            r#"
            SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason,
                   description, status AS "status!", score AS "score!", received_ts,
                   resolved_at AS "resolved_ts", resolved_by, resolution_reason
            FROM event_reports WHERE event_id = $1 ORDER BY received_ts DESC, id DESC
            "#,
            event_id,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`get_reports_by_room`].
    pub async fn get_reports_by_room(
        &self,
        room_id: &str,
        limit: i64,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        let rows = if let (Some(ts), Some(id)) = (since_ts, since_id) {
            sqlx::query_as!(
                EventReport,
                r#"
                SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason,
                       description, status AS "status!", score AS "score!", received_ts,
                       resolved_at AS "resolved_ts", resolved_by, resolution_reason
                FROM event_reports
                WHERE room_id = $1 AND (received_ts < $3 OR (received_ts = $3 AND id < $4))
                ORDER BY received_ts DESC, id DESC LIMIT $2
                "#,
                room_id,
                limit,
                ts,
                id,
            )
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as!(
                EventReport,
                r#"
                SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason,
                       description, status AS "status!", score AS "score!", received_ts,
                       resolved_at AS "resolved_ts", resolved_by, resolution_reason
                FROM event_reports WHERE room_id = $1 ORDER BY received_ts DESC, id DESC LIMIT $2
                "#,
                room_id,
                limit,
            )
            .fetch_all(&*self.pool)
            .await?
        };

        Ok(rows)
    }

    /// See [`get_reports_by_reporter`].
    pub async fn get_reports_by_reporter(
        &self,
        reporter_user_id: &str,
        limit: i64,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        let rows = if let (Some(ts), Some(id)) = (since_ts, since_id) {
            sqlx::query_as!(
                EventReport,
                r#"
                SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason,
                       description, status AS "status!", score AS "score!", received_ts,
                       resolved_at AS "resolved_ts", resolved_by, resolution_reason
                FROM event_reports
                WHERE reporter_user_id = $1 AND (received_ts < $3 OR (received_ts = $3 AND id < $4))
                ORDER BY received_ts DESC, id DESC LIMIT $2
                "#,
                reporter_user_id,
                limit,
                ts,
                id,
            )
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as!(
                EventReport,
                r#"
                SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason,
                       description, status AS "status!", score AS "score!", received_ts,
                       resolved_at AS "resolved_ts", resolved_by, resolution_reason
                FROM event_reports WHERE reporter_user_id = $1 ORDER BY received_ts DESC, id DESC LIMIT $2
                "#,
                reporter_user_id,
                limit,
            )
            .fetch_all(&*self.pool)
            .await?
        };

        Ok(rows)
    }

    /// See [`get_reports_by_status`].
    pub async fn get_reports_by_status(
        &self,
        status: &str,
        limit: i64,
        since_score: Option<i32>,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        let rows = if let (Some(score), Some(ts), Some(id)) = (since_score, since_ts, since_id) {
            sqlx::query_as!(
                EventReport,
                r#"
                SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason,
                       description, status AS "status!", score AS "score!", received_ts,
                       resolved_at AS "resolved_ts", resolved_by, resolution_reason
                FROM event_reports
                WHERE status = $1 AND (
                    score < $3 OR
                    (score = $3 AND received_ts < $4) OR
                    (score = $3 AND received_ts = $4 AND id < $5)
                )
                ORDER BY score DESC, received_ts DESC, id DESC LIMIT $2
                "#,
                status,
                limit,
                score,
                ts,
                id,
            )
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as!(
                EventReport,
                r#"
                SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason,
                       description, status AS "status!", score AS "score!", received_ts,
                       resolved_at AS "resolved_ts", resolved_by, resolution_reason
                FROM event_reports WHERE status = $1 ORDER BY score DESC, received_ts DESC, id DESC LIMIT $2
                "#,
                status,
                limit,
            )
            .fetch_all(&*self.pool)
            .await?
        };

        Ok(rows)
    }

    /// See [`get_all_reports`].
    pub async fn get_all_reports(
        &self,
        limit: i64,
        since_score: Option<i32>,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        let rows = if let (Some(score), Some(ts), Some(id)) = (since_score, since_ts, since_id) {
            sqlx::query_as!(
                EventReport,
                r#"
                SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason,
                       description, status AS "status!", score AS "score!", received_ts,
                       resolved_at AS "resolved_ts", resolved_by, resolution_reason
                FROM event_reports
                WHERE (
                    score < $2 OR
                    (score = $2 AND received_ts < $3) OR
                    (score = $2 AND received_ts = $3 AND id < $4)
                )
                ORDER BY score DESC, received_ts DESC, id DESC LIMIT $1
                "#,
                limit,
                score,
                ts,
                id,
            )
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as!(
                EventReport,
                r#"
                SELECT id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason,
                       description, status AS "status!", score AS "score!", received_ts,
                       resolved_at AS "resolved_ts", resolved_by, resolution_reason
                FROM event_reports ORDER BY score DESC, received_ts DESC, id DESC LIMIT $1
                "#,
                limit,
            )
            .fetch_all(&*self.pool)
            .await?
        };

        Ok(rows)
    }

    /// See [`update_report`].
    pub async fn update_report(&self, id: i64, request: UpdateEventReportRequest) -> Result<EventReport, sqlx::Error> {
        let now = current_timestamp_millis();

        let resolved_ts =
            if request.status.as_deref() == Some("resolved") || request.status.as_deref() == Some("dismissed") {
                Some(now)
            } else {
                None
            };

        let row = sqlx::query_as!(
            EventReport,
            r#"
            UPDATE event_reports SET
                status = COALESCE($2, status),
                score = COALESCE($3, score),
                resolved_by = COALESCE($4, resolved_by),
                resolution_reason = COALESCE($5, resolution_reason),
                resolved_at = COALESCE($6, resolved_at)
            WHERE id = $1
            RETURNING id, event_id, room_id, reporter_user_id, reported_user_id, event_json, reason,
                      description, status AS "status!", score AS "score!", received_ts,
                      resolved_at AS "resolved_ts", resolved_by, resolution_reason
            "#,
            id,
            request.status.as_deref(),
            request.score,
            request.resolved_by.as_deref(),
            request.resolution_reason.as_deref(),
            resolved_ts,
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`delete_report`].
    pub async fn delete_report(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM event_reports WHERE id = $1", id).execute(&*self.pool).await?;

        Ok(())
    }

    /// See [`add_history`].
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

    /// See [`get_report_history`].
    pub fn get_report_history(&self, _report_id: i64) -> Result<Vec<EventReportHistory>, sqlx::Error> {
        Ok(vec![])
    }

    /// See [`check_rate_limit`].
    ///
    /// STO-05: 行锁查询 —— 在事务内按 user_id 选中报告限流行并 `FOR UPDATE`，
    /// 避免「读已过期封锁 → 并发重复解锁」的 TOCTOU 窗口。
    pub async fn check_rate_limit(&self, user_id: &str) -> Result<ReportRateLimitCheck, sqlx::Error> {
        // STO-05: SELECT→UPDATE 包在事务里并 FOR UPDATE 行锁，
        // 消除「读已过期封锁 → 并发重复解锁」的 TOCTOU 窗口。
        let mut tx = self.pool.begin().await?;

        let limit = sqlx::query_as!(
            ReportRateLimit,
            r#"
            SELECT
                id,
                user_id,
                report_count AS "report_count!",
                last_report_at,
                blocked_until_at,
                is_blocked AS "is_blocked!",
                block_reason,
                created_ts,
                COALESCE(updated_ts, created_ts) AS "updated_ts!"
            FROM report_rate_limits
            WHERE user_id = $1
            FOR UPDATE
            "#,
            user_id,
        )
        .fetch_optional(&mut *tx)
        .await?;

        let max_reports_per_day = 50;

        let result = match limit {
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
                            sqlx::query!(
                                "UPDATE report_rate_limits SET is_blocked = FALSE, blocked_until_at = NULL, block_reason = NULL, updated_ts = $2 WHERE user_id = $1",
                                user_id,
                                now,
                            )
                            .execute(&mut *tx)
                            .await?;
                            return {
                                tx.commit().await?;
                                Ok(ReportRateLimitCheck {
                                    is_allowed: true,
                                    remaining_reports: max_reports_per_day,
                                    block_reason: None,
                                })
                            };
                        }
                    }
                    return {
                        tx.commit().await?;
                        Ok(ReportRateLimitCheck {
                            is_allowed: false,
                            remaining_reports: 0,
                            block_reason: l.block_reason,
                        })
                    };
                }

                let one_day_ago = current_timestamp_millis() - 86_400_000;
                if l.last_report_at.is_some_and(|last_report_at| last_report_at > one_day_ago) {
                    if l.report_count >= max_reports_per_day {
                        Ok(ReportRateLimitCheck {
                            is_allowed: false,
                            remaining_reports: 0,
                            block_reason: Some("Daily report limit exceeded".to_string()),
                        })
                    } else {
                        Ok(ReportRateLimitCheck {
                            is_allowed: true,
                            remaining_reports: max_reports_per_day - l.report_count,
                            block_reason: None,
                        })
                    }
                } else {
                    Ok(ReportRateLimitCheck {
                        is_allowed: true,
                        remaining_reports: max_reports_per_day,
                        block_reason: None,
                    })
                }
            }
        };

        tx.commit().await?;
        result
    }

    /// See [`record_report`].
    pub async fn record_report(&self, user_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        let one_day_ago = now - 86_400_000;

        // STO-05: 原子 UPSERT。此前 SELECT→计算→UPDATE/INSERT 两步走，
        // 并发下丢失计数（两个请求读到同一 report_count 都 +1 写回同值），
        // 新用户并发 INSERT 还会撞唯一约束报错。
        sqlx::query!(
            r"
            INSERT INTO report_rate_limits (user_id, report_count, last_report_at, created_ts, updated_ts)
            VALUES ($1, 1, $2, $2, $2)
            ON CONFLICT (user_id) DO UPDATE SET
                report_count = CASE
                    WHEN report_rate_limits.last_report_at IS NULL OR report_rate_limits.last_report_at < $3 THEN 1
                    ELSE report_rate_limits.report_count + 1
                END,
                last_report_at = $2,
                updated_ts = $2
            ",
            user_id,
            now,
            one_day_ago,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`block_user_reports`].
    pub async fn block_user_reports(&self, user_id: &str, blocked_until: i64, reason: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query!(
            r"
            INSERT INTO report_rate_limits (user_id, is_blocked, blocked_until_at, block_reason, created_ts, updated_ts)
            VALUES ($1, TRUE, $2, $3, $4, $4)
            ON CONFLICT (user_id) DO UPDATE SET
                is_blocked = TRUE,
                blocked_until_at = $2,
                block_reason = $3,
                updated_ts = $4
            ",
            user_id,
            blocked_until,
            reason,
            now,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`unblock_user_reports`].
    pub async fn unblock_user_reports(&self, user_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query!(
            "UPDATE report_rate_limits SET is_blocked = FALSE, blocked_until_at = NULL, block_reason = NULL, updated_ts = $2 WHERE user_id = $1",
            user_id,
            now,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`get_stats`].
    pub fn get_stats(&self, _days: i32) -> Result<Vec<EventReportStats>, sqlx::Error> {
        Ok(vec![])
    }

    /// See [`count_reports_by_status`].
    pub async fn count_reports_by_status(&self, status: &str) -> Result<i64, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!" FROM event_reports WHERE status = $1"#, status)
                .fetch_one(&*self.pool)
                .await?;

        Ok(count)
    }

    /// See [`count_all_reports`].
    pub async fn count_all_reports(&self) -> Result<i64, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!" FROM event_reports"#).fetch_one(&*self.pool).await?;

        Ok(count)
    }
}
