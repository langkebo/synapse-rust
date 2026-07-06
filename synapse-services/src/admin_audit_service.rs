use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::{AuditEvent, AuditEventFilters, AuditEventStoreApi, CreateAuditEventRequest};
use tracing::{error, instrument};

type AuditListResult = Result<(Vec<AuditEvent>, i64, Option<String>), ApiError>;

pub struct AdminAuditService {
    storage: Arc<dyn AuditEventStoreApi>,
}

impl AdminAuditService {
    pub fn new(storage: Arc<dyn AuditEventStoreApi>) -> Self {
        Self { storage }
    }

    #[instrument(skip(self))]
    pub async fn create_event(&self, request: CreateAuditEventRequest) -> Result<AuditEvent, ApiError> {
        validate_request(&request)?;

        let event_id = uuid::Uuid::new_v4().to_string();
        let created_ts = chrono::Utc::now().timestamp_millis();

        self.storage.create_event(&event_id, created_ts, &request).await.map_err(|error| {
            error!(target: "security_audit", event_id = %event_id, "failed to persist audit event: {}", error);
            ApiError::internal("An internal error occurred".to_string())
        })
    }

    #[instrument(skip(self))]
    pub async fn get_event(&self, event_id: &str) -> Result<Option<AuditEvent>, ApiError> {
        self.storage
            .get_event(event_id)
            .await
            .map_err(|error| ApiError::internal_with_log("Failed to load audit event", &error))
    }

    #[instrument(skip(self))]
    pub async fn list_events(&self, filters: AuditEventFilters) -> AuditListResult {
        self.storage
            .list_events(&filters)
            .await
            .map_err(|error| ApiError::internal_with_log("Failed to list audit events", &error))
    }
}

fn validate_request(request: &CreateAuditEventRequest) -> Result<(), ApiError> {
    if is_blank(&request.actor_id) {
        return Err(ApiError::bad_request("actor_id is required"));
    }

    if is_blank(&request.action) {
        return Err(ApiError::bad_request("action is required"));
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
        "success" | "denied" | "failed" | "failure" => Ok(()),
        _ => Err(ApiError::bad_request("result must be one of success, denied, failed, failure")),
    }
}

fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::test_mocks::InMemoryAuditEventStore;

    fn test_service() -> AdminAuditService {
        AdminAuditService::new(Arc::new(InMemoryAuditEventStore::new()))
    }

    fn valid_request() -> CreateAuditEventRequest {
        CreateAuditEventRequest {
            actor_id: "@admin:example.com".into(),
            action: "user.deactivate".into(),
            resource_type: "user".into(),
            resource_id: "@target:example.com".into(),
            result: "success".into(),
            request_id: "req-123".into(),
            details: Some(serde_json::json!({"reason": "spam"})),
        }
    }

    // ── is_blank ────────────────────────────────────────────────────

    #[test]
    fn is_blank_empty_string() {
        assert!(is_blank(""));
    }

    #[test]
    fn is_blank_whitespace_only() {
        assert!(is_blank("   "));
        assert!(is_blank("\t"));
        assert!(is_blank("\n"));
    }

    #[test]
    fn is_blank_non_blank() {
        assert!(!is_blank("admin"));
        assert!(!is_blank(" admin "));
    }

    // ── create_event validation ─────────────────────────────────────

    #[tokio::test]
    async fn create_event_success() {
        let svc = test_service();
        let event = svc.create_event(valid_request()).await.unwrap();
        assert_eq!(event.actor_id, "@admin:example.com");
        assert_eq!(event.action, "user.deactivate");
        assert_eq!(event.result, "success");
        assert!(!event.event_id.is_empty());
    }

    #[tokio::test]
    async fn create_event_missing_actor_id() {
        let svc = test_service();
        let req = CreateAuditEventRequest { actor_id: "".into(), ..valid_request() };
        let err = svc.create_event(req).await.unwrap_err();
        assert!(err.to_string().contains("actor_id is required"));
    }

    #[tokio::test]
    async fn create_event_missing_action() {
        let svc = test_service();
        let req = CreateAuditEventRequest { action: "".into(), ..valid_request() };
        let err = svc.create_event(req).await.unwrap_err();
        assert!(err.to_string().contains("action is required"));
    }

    #[tokio::test]
    async fn create_event_missing_resource_type() {
        let svc = test_service();
        let req = CreateAuditEventRequest { resource_type: "".into(), ..valid_request() };
        let err = svc.create_event(req).await.unwrap_err();
        assert!(err.to_string().contains("resource_type is required"));
    }

    #[tokio::test]
    async fn create_event_missing_resource_id() {
        let svc = test_service();
        let req = CreateAuditEventRequest { resource_id: "".into(), ..valid_request() };
        let err = svc.create_event(req).await.unwrap_err();
        assert!(err.to_string().contains("resource_id is required"));
    }

    #[tokio::test]
    async fn create_event_missing_request_id() {
        let svc = test_service();
        let req = CreateAuditEventRequest { request_id: "".into(), ..valid_request() };
        let err = svc.create_event(req).await.unwrap_err();
        assert!(err.to_string().contains("request_id is required"));
    }

    #[tokio::test]
    async fn create_event_whitespace_only_fields_rejected() {
        let svc = test_service();
        let req = CreateAuditEventRequest { actor_id: "  ".into(), ..valid_request() };
        let err = svc.create_event(req).await.unwrap_err();
        assert!(err.to_string().contains("actor_id is required"));
    }

    #[tokio::test]
    async fn create_event_invalid_result_field() {
        let svc = test_service();
        let req = CreateAuditEventRequest { result: "unknown".into(), ..valid_request() };
        let err = svc.create_event(req).await.unwrap_err();
        assert!(err.to_string().contains("result must be one of"));
    }

    #[tokio::test]
    async fn create_event_all_valid_result_values() {
        let svc = test_service();
        for result in &["success", "denied", "failed", "failure"] {
            let req = CreateAuditEventRequest { result: result.to_string(), ..valid_request() };
            assert!(svc.create_event(req).await.is_ok(), "result '{}' should be valid", result);
        }
    }

    // ── get_event ───────────────────────────────────────────────────

    #[tokio::test]
    async fn get_event_returns_created_event() {
        let svc = test_service();
        let created = svc.create_event(valid_request()).await.unwrap();
        let found = svc.get_event(&created.event_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().event_id, created.event_id);
    }

    #[tokio::test]
    async fn get_event_returns_none_for_unknown() {
        let svc = test_service();
        assert!(svc.get_event("nonexistent").await.unwrap().is_none());
    }

    // ── list_events ─────────────────────────────────────────────────

    fn default_filters() -> AuditEventFilters {
        AuditEventFilters { limit: 100, ..Default::default() }
    }

    #[tokio::test]
    async fn list_events_returns_all() {
        let svc = test_service();
        svc.create_event(valid_request()).await.unwrap();
        let (events, total, _next) = svc.list_events(default_filters()).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn list_events_filters_by_actor() {
        let svc = test_service();
        svc.create_event(valid_request()).await.unwrap();
        svc.create_event(CreateAuditEventRequest { actor_id: "@other:example.com".into(), ..valid_request() })
            .await
            .unwrap();

        let filters =
            AuditEventFilters { actor_id: Some("@other:example.com".into()), limit: 100, ..Default::default() };
        let (events, total, _next) = svc.list_events(filters).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(events[0].actor_id, "@other:example.com");
    }

    #[tokio::test]
    async fn list_events_empty() {
        let svc = test_service();
        let (events, total, _next) = svc.list_events(default_filters()).await.unwrap();
        assert_eq!(total, 0);
        assert!(events.is_empty());
    }
}
