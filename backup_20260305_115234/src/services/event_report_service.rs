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
