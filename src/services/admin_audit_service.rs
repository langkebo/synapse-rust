use crate::common::ApiError;
use crate::storage::{AuditEvent, AuditEventFilters, AuditEventStorage, CreateAuditEventRequest};
use std::sync::Arc;
use tracing::{error, instrument};

pub struct AdminAuditService {
    storage: Arc<AuditEventStorage>,
}

impl AdminAuditService {
    pub fn new(storage: Arc<AuditEventStorage>) -> Self {
        Self { storage }
    }

    #[instrument(skip(self))]
    pub async fn create_event(
        &self,
        request: CreateAuditEventRequest,
    ) -> Result<AuditEvent, ApiError> {
        validate_request(&request)?;

        let event_id = uuid::Uuid::new_v4().to_string();
        let created_ts = chrono::Utc::now().timestamp_millis();

        self.storage
            .create_event(&event_id, created_ts, &request)
            .await
            .map_err(|error| {
                error!(target: "security_audit", event_id = %event_id, "failed to persist audit event: {}", error);
                ApiError::internal(format!("Failed to persist audit event: {}", error))
            })
    }

    #[instrument(skip(self))]
    pub async fn get_event(&self, event_id: &str) -> Result<Option<AuditEvent>, ApiError> {
        self.storage
            .get_event(event_id)
            .await
            .map_err(|error| ApiError::internal(format!("Failed to load audit event: {}", error)))
    }

    #[instrument(skip(self))]
    pub async fn list_events(
        &self,
        filters: AuditEventFilters,
    ) -> Result<(Vec<AuditEvent>, i64), ApiError> {
        self.storage
            .list_events(&filters)
            .await
            .map_err(|error| ApiError::internal(format!("Failed to list audit events: {}", error)))
    }
}

fn validate_request(request: &CreateAuditEventRequest) -> Result<(), ApiError> {
    if is_blank(&request.actor_id) {
        return Err(ApiError::bad_request("actor_id is required"));
    }

    if is_blank(&request.action) {
        return Err(ApiError::bad_request("action is required"));
    }

    if !request
        .action
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':'))
    {
        return Err(ApiError::bad_request("action contains invalid characters"));
    }

    if is_blank(&request.resource_type) {
        return Err(ApiError::bad_request("resource_type is required"));
    }

    if is_blank(&request.resource_id) {
        return Err(ApiError::bad_request("resource_id is required"));
    }

    if is_blank(&request.request_id) {
        return Err(ApiError::bad_request("request_id is required"));
    }

    match request.result.as_str() {
        "success" | "denied" | "failed" => Ok(()),
        _ => Err(ApiError::bad_request(
            "result must be one of success, denied, failed",
        )),
    }
}

fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}
