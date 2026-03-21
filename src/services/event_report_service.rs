use crate::common::ApiError;
use crate::storage::event_report::*;
use std::sync::Arc;
use tracing::{info, instrument};

pub struct EventReportService {
    storage: Arc<EventReportStorage>,
}

impl EventReportService {
    pub fn new(storage: Arc<EventReportStorage>) -> Self {
        Self { storage }
    }

    #[instrument(skip(self))]
    pub async fn create_report(
        &self,
        request: CreateEventReportRequest,
    ) -> Result<EventReport, ApiError> {
        info!(
            "Creating event report for event: {} in room: {}",
            request.event_id, request.room_id
        );

        let rate_check = self
            .storage
            .check_rate_limit(&request.reporter_user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check rate limit: {}", e)))?;

        if !rate_check.is_allowed {
            return Err(ApiError::bad_request(
                rate_check
                    .block_reason
                    .unwrap_or_else(|| "Rate limit exceeded".to_string()),
            ));
        }

        let report = self
            .storage
            .create_report(request.clone())
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create report: {}", e)))?;

        self.storage
            .record_report(&request.reporter_user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to record report: {}", e)))?;

        self.storage
            .add_history(
                report.id,
                "created",
                Some(&request.reporter_user_id),
                Some("reporter"),
                None,
                Some("open"),
                None,
                None,
            )
            .await
            .ok();

        info!("Created event report: {}", report.id);

        Ok(report)
    }

    #[instrument(skip(self))]
    pub async fn get_report(&self, id: i64) -> Result<Option<EventReport>, ApiError> {
        let report = self
            .storage
            .get_report(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get report: {}", e)))?;

        Ok(report)
    }

    #[instrument(skip(self))]
    pub async fn get_reports_by_event(&self, event_id: &str) -> Result<Vec<EventReport>, ApiError> {
        let reports = self
            .storage
            .get_reports_by_event(event_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get reports: {}", e)))?;

        Ok(reports)
    }

    #[instrument(skip(self))]
    pub async fn get_reports_by_room(
        &self,
        room_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EventReport>, ApiError> {
        let reports = self
            .storage
            .get_reports_by_room(room_id, limit, offset)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get reports: {}", e)))?;

        Ok(reports)
    }

    #[instrument(skip(self))]
    pub async fn get_reports_by_reporter(
        &self,
        reporter_user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EventReport>, ApiError> {
        let reports = self
            .storage
            .get_reports_by_reporter(reporter_user_id, limit, offset)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get reports: {}", e)))?;

        Ok(reports)
    }

    #[instrument(skip(self))]
    pub async fn get_reports_by_status(
        &self,
        status: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EventReport>, ApiError> {
        let reports = self
            .storage
            .get_reports_by_status(status, limit, offset)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get reports: {}", e)))?;

        Ok(reports)
    }

    #[instrument(skip(self))]
    pub async fn get_all_reports(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EventReport>, ApiError> {
        let reports = self
            .storage
            .get_all_reports(limit, offset)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get reports: {}", e)))?;

        Ok(reports)
    }

    #[instrument(skip(self))]
    pub async fn update_report(
        &self,
        id: i64,
        request: UpdateEventReportRequest,
        actor_user_id: &str,
    ) -> Result<EventReport, ApiError> {
        let old_report = self
            .storage
            .get_report(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get report: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Report not found"))?;

        let updated_report = self
            .storage
            .update_report(id, request.clone())
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update report: {}", e)))?;

        self.storage
            .add_history(
                id,
                "status_change",
                Some(actor_user_id),
                Some("admin"),
                Some(&old_report.status),
                request.status.as_deref(),
                request.resolution_reason.as_deref(),
                None,
            )
            .await
            .ok();

        info!(
            "Updated event report: {} to status: {:?}",
            id, request.status
        );

        Ok(updated_report)
    }

    #[instrument(skip(self))]
    pub async fn resolve_report(
        &self,
        id: i64,
        resolved_by: &str,
        resolution_reason: &str,
    ) -> Result<EventReport, ApiError> {
        let request = UpdateEventReportRequest {
            status: Some("resolved".to_string()),
            score: None,
            resolved_by: Some(resolved_by.to_string()),
            resolution_reason: Some(resolution_reason.to_string()),
        };

        self.update_report(id, request, resolved_by).await
    }

    #[instrument(skip(self))]
    pub async fn dismiss_report(
        &self,
        id: i64,
        dismissed_by: &str,
        reason: &str,
    ) -> Result<EventReport, ApiError> {
        let request = UpdateEventReportRequest {
            status: Some("dismissed".to_string()),
            score: None,
            resolved_by: Some(dismissed_by.to_string()),
            resolution_reason: Some(reason.to_string()),
        };

        self.update_report(id, request, dismissed_by).await
    }

    #[instrument(skip(self))]
    pub async fn delete_report(&self, id: i64) -> Result<(), ApiError> {
        self.storage
            .delete_report(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete report: {}", e)))?;

        info!("Deleted event report: {}", id);

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_report_history(
        &self,
        report_id: i64,
    ) -> Result<Vec<EventReportHistory>, ApiError> {
        let history = self
            .storage
            .get_report_history(report_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get history: {}", e)))?;

        Ok(history)
    }

    #[instrument(skip(self))]
    pub async fn check_rate_limit(&self, user_id: &str) -> Result<ReportRateLimitCheck, ApiError> {
        let check = self
            .storage
            .check_rate_limit(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check rate limit: {}", e)))?;

        Ok(check)
    }

    #[instrument(skip(self))]
    pub async fn block_user_reports(
        &self,
        user_id: &str,
        blocked_until: i64,
        reason: &str,
    ) -> Result<(), ApiError> {
        self.storage
            .block_user_reports(user_id, blocked_until, reason)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to block user: {}", e)))?;

        info!(
            "Blocked user {} from reporting until {}",
            user_id, blocked_until
        );

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn unblock_user_reports(&self, user_id: &str) -> Result<(), ApiError> {
        self.storage
            .unblock_user_reports(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to unblock user: {}", e)))?;

        info!("Unblocked user {} from reporting", user_id);

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_stats(&self, days: i32) -> Result<Vec<EventReportStats>, ApiError> {
        let stats = self
            .storage
            .get_stats(days)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get stats: {}", e)))?;

        Ok(stats)
    }

    #[instrument(skip(self))]
    pub async fn count_reports_by_status(&self, status: &str) -> Result<i64, ApiError> {
        let count = self
            .storage
            .count_reports_by_status(status)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to count reports: {}", e)))?;

        Ok(count)
    }

    #[instrument(skip(self))]
    pub async fn count_all_reports(&self) -> Result<i64, ApiError> {
        let count = self
            .storage
            .count_all_reports()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to count reports: {}", e)))?;

        Ok(count)
    }

    #[instrument(skip(self))]
    pub async fn get_open_reports(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EventReport>, ApiError> {
        self.get_reports_by_status("open", limit, offset).await
    }

    #[instrument(skip(self))]
    pub async fn escalate_report(
        &self,
        id: i64,
        actor_user_id: &str,
    ) -> Result<EventReport, ApiError> {
        let old_report = self
            .storage
            .get_report(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get report: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Report not found"))?;

        let request = UpdateEventReportRequest {
            status: Some("investigating".to_string()),
            score: Some(old_report.score + 10),
            resolved_by: None,
            resolution_reason: None,
        };

        let updated = self
            .storage
            .update_report(id, request.clone())
            .await
            .map_err(|e| ApiError::internal(format!("Failed to escalate report: {}", e)))?;

        self.storage
            .add_history(
                id,
                "escalated",
                Some(actor_user_id),
                Some("admin"),
                Some(&old_report.status),
                Some("investigating"),
                None,
                None,
            )
            .await
            .ok();

        info!("Escalated event report: {}", id);

        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    fn create_test_report() -> crate::storage::event_report::EventReport {
        crate::storage::event_report::EventReport {
            id: 1,
            event_id: "$event123:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            reporter_user_id: "@reporter:example.com".to_string(),
            reported_user_id: Some("@reported:example.com".to_string()),
            event_json: Some(serde_json::json!({"type": "m.room.message"})),
            reason: Some("spam".to_string()),
            description: Some("Spam content".to_string()),
            status: "open".to_string(),
            score: 10,
            received_ts: 1234567890,
            resolved_ts: None,
            resolved_by: None,
            resolution_reason: None,
        }
    }

    #[test]
    fn test_event_report_structure() {
        let report = create_test_report();
        assert_eq!(report.id, 1);
        assert_eq!(report.event_id, "$event123:example.com");
        assert_eq!(report.status, "open");
        assert_eq!(report.score, 10);
    }

    #[test]
    fn test_event_report_status() {
        let report = create_test_report();
        assert_eq!(report.status, "open");
        assert!(report.resolved_ts.is_none());
        assert!(report.resolved_by.is_none());
    }

    #[test]
    fn test_create_event_report_request() {
        let request = crate::storage::event_report::CreateEventReportRequest {
            event_id: "$event:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            reporter_user_id: "@user:example.com".to_string(),
            reported_user_id: Some("@offender:example.com".to_string()),
            event_json: Some(serde_json::json!({"type": "m.room.message"})),
            reason: Some("inappropriate".to_string()),
            description: Some("Inappropriate content".to_string()),
            score: Some(20),
        };
        assert_eq!(request.event_id, "$event:example.com");
        assert_eq!(request.score, Some(20));
    }

    #[test]
    fn test_update_event_report_request() {
        let request = crate::storage::event_report::UpdateEventReportRequest {
            status: Some("resolved".to_string()),
            score: Some(50),
            resolved_by: Some("@admin:example.com".to_string()),
            resolution_reason: Some("Action taken".to_string()),
        };
        assert_eq!(request.status, Some("resolved".to_string()));
        assert!(request.resolved_by.is_some());
    }

    #[test]
    fn test_update_event_report_request_default() {
        let request = crate::storage::event_report::UpdateEventReportRequest::default();
        assert!(request.status.is_none());
        assert!(request.score.is_none());
        assert!(request.resolved_by.is_none());
    }

    #[test]
    fn test_report_rate_limit_check() {
        let check = crate::storage::event_report::ReportRateLimitCheck {
            is_allowed: true,
            remaining_reports: 5,
            block_reason: None,
        };
        assert!(check.is_allowed);
        assert_eq!(check.remaining_reports, 5);
        assert!(check.block_reason.is_none());
    }

    #[test]
    fn test_report_rate_limit_blocked() {
        let check = crate::storage::event_report::ReportRateLimitCheck {
            is_allowed: false,
            remaining_reports: 0,
            block_reason: Some("Too many reports".to_string()),
        };
        assert!(!check.is_allowed);
        assert_eq!(check.remaining_reports, 0);
        assert!(check.block_reason.is_some());
    }

    #[test]
    fn test_event_report_history() {
        let history = crate::storage::event_report::EventReportHistory {
            id: 1,
            report_id: 1,
            action: "status_change".to_string(),
            actor_user_id: Some("@admin:example.com".to_string()),
            actor_role: Some("admin".to_string()),
            old_status: Some("open".to_string()),
            new_status: Some("investigating".to_string()),
            reason: None,
            created_ts: 1234567890,
            metadata: None,
        };
        assert_eq!(history.action, "status_change");
        assert!(history.actor_user_id.is_some());
    }

    #[test]
    fn test_report_rate_limit_structure() {
        let rate_limit = crate::storage::event_report::ReportRateLimit {
            id: 1,
            user_id: "@user:example.com".to_string(),
            report_count: 3,
            last_report_ts: Some(1234567890),
            blocked_until_ts: None,
            is_blocked: false,
            block_reason: None,
            created_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert_eq!(rate_limit.report_count, 3);
        assert!(!rate_limit.is_blocked);
    }

    #[test]
    fn test_event_report_stats() {
        let stats = crate::storage::event_report::EventReportStats {
            id: 1,
            date: chrono::NaiveDate::from_ymd_opt(2026, 3, 13).unwrap(),
            total_reports: 100,
            open_reports: 20,
            resolved_reports: 70,
            dismissed_reports: 10,
            avg_resolution_time_hours: Some(24),
            created_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert_eq!(stats.total_reports, 100);
        assert_eq!(stats.open_reports, 20);
        assert_eq!(stats.resolved_reports, 70);
    }

    #[test]
    fn test_report_with_resolved_state() {
        let mut report = create_test_report();
        report.status = "resolved".to_string();
        report.resolved_ts = Some(1234567999);
        report.resolved_by = Some("@admin:example.com".to_string());
        report.resolution_reason = Some("User banned".to_string());

        assert_eq!(report.status, "resolved");
        assert!(report.resolved_ts.is_some());
        assert!(report.resolved_by.is_some());
    }

    #[test]
    fn test_report_score_escalation() {
        let report = create_test_report();
        let initial_score = report.score;
        let escalated_score = initial_score + 10;

        assert_eq!(initial_score, 10);
        assert_eq!(escalated_score, 20);
    }
}
