use async_trait::async_trait;

use super::models::*;
use super::repository::EventReportStorage;

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait EventReportStoreApi: Send + Sync {
    async fn create_report(&self, request: CreateEventReportRequest) -> Result<EventReport, sqlx::Error>;
    async fn get_report(&self, id: i64) -> Result<Option<EventReport>, sqlx::Error>;
    async fn get_reports_by_event(&self, event_id: &str) -> Result<Vec<EventReport>, sqlx::Error>;
    async fn get_reports_by_room(
        &self,
        room_id: &str,
        limit: i64,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error>;
    async fn get_reports_by_reporter(
        &self,
        reporter_user_id: &str,
        limit: i64,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error>;
    async fn get_reports_by_status(
        &self,
        status: &str,
        limit: i64,
        since_score: Option<i32>,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error>;
    async fn get_all_reports(
        &self,
        limit: i64,
        since_score: Option<i32>,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error>;
    async fn update_report(&self, id: i64, request: UpdateEventReportRequest) -> Result<EventReport, sqlx::Error>;
    async fn delete_report(&self, id: i64) -> Result<(), sqlx::Error>;
    async fn check_rate_limit(&self, user_id: &str) -> Result<ReportRateLimitCheck, sqlx::Error>;
    async fn record_report(&self, user_id: &str) -> Result<(), sqlx::Error>;
    async fn block_user_reports(&self, user_id: &str, blocked_until: i64, reason: &str) -> Result<(), sqlx::Error>;
    async fn unblock_user_reports(&self, user_id: &str) -> Result<(), sqlx::Error>;
    async fn count_reports_by_status(&self, status: &str) -> Result<i64, sqlx::Error>;
    async fn count_all_reports(&self) -> Result<i64, sqlx::Error>;
    fn add_history(
        &self,
        report_id: i64,
        action: &str,
        actor_user_id: Option<&str>,
        actor_role: Option<&str>,
        old_status: Option<&str>,
        new_status: Option<&str>,
        reason: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<EventReportHistory, sqlx::Error>;
    fn get_report_history(&self, report_id: i64) -> Result<Vec<EventReportHistory>, sqlx::Error>;
    fn get_stats(&self, days: i32) -> Result<Vec<EventReportStats>, sqlx::Error>;
}

#[async_trait]
impl EventReportStoreApi for EventReportStorage {
    async fn create_report(&self, request: CreateEventReportRequest) -> Result<EventReport, sqlx::Error> {
        self.create_report(request).await
    }

    async fn get_report(&self, id: i64) -> Result<Option<EventReport>, sqlx::Error> {
        self.get_report(id).await
    }

    async fn get_reports_by_event(&self, event_id: &str) -> Result<Vec<EventReport>, sqlx::Error> {
        self.get_reports_by_event(event_id).await
    }

    async fn get_reports_by_room(
        &self,
        room_id: &str,
        limit: i64,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        self.get_reports_by_room(room_id, limit, since_ts, since_id).await
    }

    async fn get_reports_by_reporter(
        &self,
        reporter_user_id: &str,
        limit: i64,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        self.get_reports_by_reporter(reporter_user_id, limit, since_ts, since_id).await
    }

    async fn get_reports_by_status(
        &self,
        status: &str,
        limit: i64,
        since_score: Option<i32>,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        self.get_reports_by_status(status, limit, since_score, since_ts, since_id).await
    }

    async fn get_all_reports(
        &self,
        limit: i64,
        since_score: Option<i32>,
        since_ts: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<EventReport>, sqlx::Error> {
        self.get_all_reports(limit, since_score, since_ts, since_id).await
    }

    async fn update_report(&self, id: i64, request: UpdateEventReportRequest) -> Result<EventReport, sqlx::Error> {
        self.update_report(id, request).await
    }

    async fn delete_report(&self, id: i64) -> Result<(), sqlx::Error> {
        self.delete_report(id).await
    }

    async fn check_rate_limit(&self, user_id: &str) -> Result<ReportRateLimitCheck, sqlx::Error> {
        self.check_rate_limit(user_id).await
    }

    async fn record_report(&self, user_id: &str) -> Result<(), sqlx::Error> {
        self.record_report(user_id).await
    }

    async fn block_user_reports(&self, user_id: &str, blocked_until: i64, reason: &str) -> Result<(), sqlx::Error> {
        self.block_user_reports(user_id, blocked_until, reason).await
    }

    async fn unblock_user_reports(&self, user_id: &str) -> Result<(), sqlx::Error> {
        self.unblock_user_reports(user_id).await
    }

    async fn count_reports_by_status(&self, status: &str) -> Result<i64, sqlx::Error> {
        self.count_reports_by_status(status).await
    }

    async fn count_all_reports(&self) -> Result<i64, sqlx::Error> {
        self.count_all_reports().await
    }

    fn add_history(
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
        self.add_history(report_id, action, actor_user_id, actor_role, old_status, new_status, reason, metadata)
    }

    fn get_report_history(&self, report_id: i64) -> Result<Vec<EventReportHistory>, sqlx::Error> {
        self.get_report_history(report_id)
    }

    fn get_stats(&self, days: i32) -> Result<Vec<EventReportStats>, sqlx::Error> {
        self.get_stats(days)
    }
}
