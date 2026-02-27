use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EventReport {
    pub id: i64,
    pub event_id: String,
    pub room_id: String,
    pub reporter_user_id: String,
    pub reported_user_id: Option<String>,
    pub event_json: Option<serde_json::Value>,
    pub reason: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub score: i32,
    pub received_ts: i64,
    pub resolved_ts: Option<i64>,
    pub resolved_by: Option<String>,
    pub resolution_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EventReportHistory {
    pub id: i64,
    pub report_id: i64,
    pub action: String,
    pub actor_user_id: Option<String>,
    pub actor_role: Option<String>,
    pub old_status: Option<String>,
    pub new_status: Option<String>,
    pub reason: Option<String>,
    pub created_ts: i64,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ReportRateLimit {
    pub id: i64,
    pub user_id: String,
    pub report_count: i32,
    pub last_report_ts: Option<i64>,
    pub blocked_until_ts: Option<i64>,
    pub is_blocked: bool,
    pub block_reason: Option<String>,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EventReportStats {
    pub id: i64,
    pub stat_date: chrono::NaiveDate,
    pub total_reports: i32,
    pub open_reports: i32,
    pub resolved_reports: i32,
    pub dismissed_reports: i32,
    pub avg_resolution_time_ms: Option<i64>,
    pub reports_by_reason: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEventReportRequest {
    pub event_id: String,
    pub room_id: String,
    pub reporter_user_id: String,
    pub reported_user_id: Option<String>,
    pub event_json: Option<serde_json::Value>,
    pub reason: Option<String>,
    pub description: Option<String>,
    pub score: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateEventReportRequest {
    pub status: Option<String>,
    pub score: Option<i32>,
    pub resolved_by: Option<String>,
    pub resolution_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportRateLimitCheck {
    pub is_allowed: bool,
    pub remaining_reports: i32,
    pub block_reason: Option<String>,
}

#[derive(Clone)]
pub struct EventReportStorage {
    pool: Arc<PgPool>,
}

impl EventReportStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_report(
        &self,
        request: CreateEventReportRequest,
    ) -> Result<EventReport, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, EventReport>(
            r#"
            INSERT INTO event_reports (
                event_id, room_id, reporter_user_id, reported_user_id, event_json,
                reason, description, score, received_ts, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'open')
            RETURNING *
            "#,
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
        let row = sqlx::query_as::<_, EventReport>("SELECT * FROM event_reports WHERE id = $1")
            .bind(id)
            .fetch_optional(&*self.pool)
            .await?;

        Ok(row)
    }

    pub async fn get_reports_by_event(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        let rows = sqlx::query_as::<_, EventReport>(
            "SELECT * FROM event_reports WHERE event_id = $1 ORDER BY received_ts DESC",
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
        offset: i64,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        let rows = sqlx::query_as::<_, EventReport>(
            "SELECT * FROM event_reports WHERE room_id = $1 ORDER BY received_ts DESC LIMIT $2 OFFSET $3",
        )
        .bind(room_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_reports_by_reporter(
        &self,
        reporter_user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        let rows = sqlx::query_as::<_, EventReport>(
            "SELECT * FROM event_reports WHERE reporter_user_id = $1 ORDER BY received_ts DESC LIMIT $2 OFFSET $3",
        )
        .bind(reporter_user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_reports_by_status(
        &self,
        status: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        let rows = sqlx::query_as::<_, EventReport>(
            "SELECT * FROM event_reports WHERE status = $1 ORDER BY score DESC, received_ts DESC LIMIT $2 OFFSET $3",
        )
        .bind(status)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_all_reports(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        let rows = sqlx::query_as::<_, EventReport>(
            "SELECT * FROM event_reports ORDER BY score DESC, received_ts DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn update_report(
        &self,
        id: i64,
        request: UpdateEventReportRequest,
    ) -> Result<EventReport, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let resolved_ts = if request.status.as_deref() == Some("resolved")
            || request.status.as_deref() == Some("dismissed")
        {
            Some(now)
        } else {
            None
        };

        let row = sqlx::query_as::<_, EventReport>(
            r#"
            UPDATE event_reports SET
                status = COALESCE($2, status),
                score = COALESCE($3, score),
                resolved_by = COALESCE($4, resolved_by),
                resolution_reason = COALESCE($5, resolution_reason),
                resolved_ts = COALESCE($6, resolved_ts)
            WHERE id = $1
            RETURNING *
            "#,
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
        sqlx::query("DELETE FROM event_reports WHERE id = $1")
            .bind(id)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add_history(
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
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, EventReportHistory>(
            r#"
            INSERT INTO event_report_history (
                report_id, action, actor_user_id, actor_role, old_status, new_status, reason, created_ts, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            "#,
        )
        .bind(report_id)
        .bind(action)
        .bind(actor_user_id)
        .bind(actor_role)
        .bind(old_status)
        .bind(new_status)
        .bind(reason)
        .bind(now)
        .bind(metadata)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_report_history(
        &self,
        report_id: i64,
    ) -> Result<Vec<EventReportHistory>, sqlx::Error> {
        let rows = sqlx::query_as::<_, EventReportHistory>(
            "SELECT * FROM event_report_history WHERE report_id = $1 ORDER BY created_ts DESC",
        )
        .bind(report_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn check_rate_limit(
        &self,
        user_id: &str,
    ) -> Result<ReportRateLimitCheck, sqlx::Error> {
        let limit = sqlx::query_as::<_, ReportRateLimit>(
            "SELECT * FROM report_rate_limits WHERE user_id = $1",
        )
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
                    if let Some(blocked_until) = l.blocked_until_ts {
                        let now = Utc::now().timestamp_millis();
                        if blocked_until < now {
                            sqlx::query("UPDATE report_rate_limits SET is_blocked = FALSE, blocked_until_ts = NULL, block_reason = NULL WHERE user_id = $1")
                                .bind(user_id)
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

                let one_day_ago = Utc::now().timestamp_millis() - 86400000;
                if l.last_report_ts.unwrap_or(0) > one_day_ago {
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
        let now = Utc::now().timestamp_millis();
        let one_day_ago = now - 86400000;

        let existing = sqlx::query_as::<_, ReportRateLimit>(
            "SELECT * FROM report_rate_limits WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        match existing {
            Some(l) => {
                let new_count = if l.last_report_ts.unwrap_or(0) < one_day_ago {
                    1
                } else {
                    l.report_count + 1
                };

                sqlx::query(
                    "UPDATE report_rate_limits SET report_count = $2, last_report_ts = $3 WHERE user_id = $1",
                )
                .bind(user_id)
                .bind(new_count)
                .bind(now)
                .execute(&*self.pool)
                .await?;
            }
            None => {
                sqlx::query(
                    "INSERT INTO report_rate_limits (user_id, report_count, last_report_ts, created_ts, updated_ts) VALUES ($1, 1, $2, $2, $2)",
                )
                .bind(user_id)
                .bind(now)
                .execute(&*self.pool)
                .await?;
            }
        }

        Ok(())
    }

    pub async fn block_user_reports(
        &self,
        user_id: &str,
        blocked_until: i64,
        reason: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO report_rate_limits (user_id, is_blocked, blocked_until_ts, block_reason, created_ts, updated_ts)
            VALUES ($1, TRUE, $2, $3, $4, $4)
            ON CONFLICT (user_id) DO UPDATE SET
                is_blocked = TRUE,
                blocked_until_ts = $2,
                block_reason = $3,
                updated_ts = $4
            "#,
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
        sqlx::query(
            "UPDATE report_rate_limits SET is_blocked = FALSE, blocked_until_ts = NULL, block_reason = NULL WHERE user_id = $1",
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_stats(&self, days: i32) -> Result<Vec<EventReportStats>, sqlx::Error> {
        let rows = sqlx::query_as::<_, EventReportStats>(
            "SELECT * FROM event_report_stats WHERE stat_date >= CURRENT_DATE - $1 ORDER BY stat_date DESC",
        )
        .bind(days)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn count_reports_by_status(&self, status: &str) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_reports WHERE status = $1")
            .bind(status)
            .fetch_one(&*self.pool)
            .await?;

        Ok(count)
    }

    pub async fn count_all_reports(&self) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_reports")
            .fetch_one(&*self.pool)
            .await?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_report_creation() {
        let report = EventReport {
            id: 1,
            event_id: "$event:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            reporter_user_id: "@reporter:example.com".to_string(),
            reported_user_id: Some("@reported:example.com".to_string()),
            event_json: None,
            reason: Some("Spam".to_string()),
            description: None,
            status: "pending".to_string(),
            score: 0,
            received_ts: 1234567890,
            resolved_ts: None,
            resolved_by: None,
            resolution_reason: None,
        };
        assert_eq!(report.id, 1);
        assert_eq!(report.event_id, "$event:example.com");
    }

    #[test]
    fn test_event_report_with_reason() {
        let report = EventReport {
            id: 2,
            event_id: "$event2:example.com".to_string(),
            room_id: "!room2:example.com".to_string(),
            reporter_user_id: "@reporter2:example.com".to_string(),
            reported_user_id: None,
            event_json: None,
            reason: Some("Inappropriate content".to_string()),
            description: Some("Test description".to_string()),
            status: "open".to_string(),
            score: -50,
            received_ts: 1234567890,
            resolved_ts: None,
            resolved_by: None,
            resolution_reason: None,
        };
        assert!(report.reason.is_some());
        assert_eq!(report.reason.unwrap(), "Inappropriate content");
    }

    #[test]
    fn test_event_report_history_creation() {
        let history = EventReportHistory {
            id: 1,
            report_id: 1,
            action: "status_change".to_string(),
            actor_user_id: Some("@admin:example.com".to_string()),
            actor_role: Some("moderator".to_string()),
            old_status: Some("pending".to_string()),
            new_status: Some("resolved".to_string()),
            reason: Some("Reviewed and resolved".to_string()),
            created_ts: 1234567890,
            metadata: None,
        };
        assert_eq!(history.report_id, 1);
        assert!(history.actor_user_id.is_some());
    }

    #[test]
    fn test_report_rate_limit_creation() {
        let rate_limit = ReportRateLimit {
            id: 1,
            user_id: "@user:example.com".to_string(),
            report_count: 5,
            last_report_ts: Some(1234567890),
            blocked_until_ts: None,
            is_blocked: false,
            block_reason: None,
            created_ts: 1234567800,
            updated_ts: 1234567890,
        };
        assert_eq!(rate_limit.report_count, 5);
    }

    #[test]
    fn test_create_event_report_request() {
        let request = CreateEventReportRequest {
            event_id: "$new_event:example.com".to_string(),
            room_id: "!new_room:example.com".to_string(),
            reporter_user_id: "@reporter:example.com".to_string(),
            reported_user_id: Some("@reported:example.com".to_string()),
            reason: Some("New report".to_string()),
            description: None,
            event_json: None,
            score: Some(0),
        };
        assert_eq!(request.event_id, "$new_event:example.com");
    }

    #[test]
    fn test_update_event_report_request() {
        let request = UpdateEventReportRequest {
            status: Some("resolved".to_string()),
            score: Some(0),
            resolved_by: Some("@admin:example.com".to_string()),
            resolution_reason: Some("Resolved by admin".to_string()),
        };
        assert!(request.status.is_some());
        assert!(request.resolved_by.is_some());
    }
}
