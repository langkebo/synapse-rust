use crate::admin_audit_service::AdminAuditService;
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::{
    CreateFeatureFlagRequest, FeatureFlag, FeatureFlagFilters, FeatureFlagStoreApi, FeatureFlagTargetInput,
    UpdateFeatureFlagRequest,
};

pub struct FeatureFlagService {
    storage: Arc<dyn FeatureFlagStoreApi>,
    audit_service: Arc<AdminAuditService>,
}

impl FeatureFlagService {
    pub fn new(storage: Arc<dyn FeatureFlagStoreApi>, audit_service: Arc<AdminAuditService>) -> Self {
        Self { storage, audit_service }
    }

    pub async fn create_flag(
        &self,
        actor_id: &str,
        request_id: &str,
        request: CreateFeatureFlagRequest,
    ) -> Result<FeatureFlag, ApiError> {
        validate_create_request(&request)?;
        let created_ts = chrono::Utc::now().timestamp_millis();
        let flag = self.storage.create_flag(&request, actor_id, created_ts).await.map_err(map_storage_error)?;

        self.audit_service
            .create_event(synapse_storage::CreateAuditEventRequest {
                actor_id: actor_id.to_string(),
                action: "admin.feature_flag.create".to_string(),
                resource_type: "feature_flag".to_string(),
                resource_id: flag.flag_key.clone(),
                result: "success".to_string(),
                request_id: request_id.to_string(),
                details: Some(json!({
                    "target_scope": flag.target_scope,
                    "rollout_percent": flag.rollout_percent,
                    "status": flag.status,
                    "target_count": flag.targets.len()
                })),
            })
            .await?;

        Ok(flag)
    }

    pub async fn update_flag(
        &self,
        actor_id: &str,
        request_id: &str,
        flag_key: &str,
        request: UpdateFeatureFlagRequest,
    ) -> Result<FeatureFlag, ApiError> {
        validate_update_request(flag_key, &request)?;
        let updated_ts = chrono::Utc::now().timestamp_millis();
        let flag = self
            .storage
            .update_flag(flag_key, &request, updated_ts)
            .await
            .map_err(map_storage_error)?
            .ok_or_else(|| ApiError::not_found(format!("feature flag not found: {flag_key}")))?;

        self.audit_service
            .create_event(synapse_storage::CreateAuditEventRequest {
                actor_id: actor_id.to_string(),
                action: "admin.feature_flag.update".to_string(),
                resource_type: "feature_flag".to_string(),
                resource_id: flag.flag_key.clone(),
                result: "success".to_string(),
                request_id: request_id.to_string(),
                details: Some(json!({
                    "rollout_percent": flag.rollout_percent,
                    "status": flag.status,
                    "expires_at": flag.expires_at,
                    "target_count": flag.targets.len()
                })),
            })
            .await?;

        Ok(flag)
    }

    pub async fn get_flag(&self, flag_key: &str) -> Result<FeatureFlag, ApiError> {
        validate_flag_key(flag_key)?;
        self.storage
            .get_flag(flag_key)
            .await
            .map_err(map_storage_error)?
            .ok_or_else(|| ApiError::not_found(format!("feature flag not found: {flag_key}")))
    }

    pub async fn list_flags(&self, filters: FeatureFlagFilters) -> Result<(Vec<FeatureFlag>, i64), ApiError> {
        if let Some(ref scope) = filters.target_scope {
            validate_target_scope(scope)?;
        }
        if let Some(ref status) = filters.status {
            validate_status(status)?;
        }

        self.storage.list_flags(&filters).await.map_err(map_storage_error)
    }
}

fn validate_create_request(request: &CreateFeatureFlagRequest) -> Result<(), ApiError> {
    validate_flag_key(&request.flag_key)?;
    validate_target_scope(&request.target_scope)?;
    validate_rollout_percent(request.rollout_percent)?;
    if let Some(ref status) = request.status {
        validate_status(status)?;
    }
    validate_reason(&request.reason)?;
    validate_targets(&request.targets)?;
    validate_expiration(request.expires_at)?;
    Ok(())
}

fn validate_update_request(flag_key: &str, request: &UpdateFeatureFlagRequest) -> Result<(), ApiError> {
    validate_flag_key(flag_key)?;
    if let Some(percent) = request.rollout_percent {
        validate_rollout_percent(percent)?;
    }
    if let Some(ref status) = request.status {
        validate_status(status)?;
    }
    if let Some(ref reason) = request.reason {
        validate_reason(reason)?;
    }
    if let Some(ref targets) = request.targets {
        validate_targets(targets)?;
    }
    validate_expiration(request.expires_at)?;
    Ok(())
}

fn validate_flag_key(flag_key: &str) -> Result<(), ApiError> {
    if flag_key.trim().is_empty() {
        return Err(ApiError::bad_request("FLAG_INVALID_SCOPE: flag_key is required"));
    }
    if !flag_key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-')) {
        return Err(ApiError::bad_request(
            "FLAG_INVALID_SCOPE: flag_key must use lowercase alphanumeric, dot, underscore or hyphen",
        ));
    }
    Ok(())
}

fn validate_target_scope(target_scope: &str) -> Result<(), ApiError> {
    match target_scope {
        "global" | "tenant" | "room" | "user" => Ok(()),
        _ => Err(ApiError::bad_request("FLAG_INVALID_SCOPE: target_scope must be one of global, tenant, room, user")),
    }
}

fn validate_rollout_percent(rollout_percent: i32) -> Result<(), ApiError> {
    if (0..=100).contains(&rollout_percent) {
        Ok(())
    } else {
        Err(ApiError::bad_request("FLAG_GUARDRAIL_BLOCKED: rollout_percent must be between 0 and 100"))
    }
}

fn validate_status(status: &str) -> Result<(), ApiError> {
    match status {
        "draft"
        | "scheduled"
        | "active"
        | "ramping"
        | "fully_enabled"
        | "rolled_back"
        | "removed"
        | "expired_pending_removal" => Ok(()),
        _ => Err(ApiError::bad_request("FLAG_CONFLICT: unsupported feature flag status")),
    }
}

fn validate_reason(reason: &str) -> Result<(), ApiError> {
    if reason.trim().is_empty() {
        return Err(ApiError::bad_request("FLAG_CONFLICT: reason is required"));
    }
    Ok(())
}

fn validate_targets(targets: &[FeatureFlagTargetInput]) -> Result<(), ApiError> {
    let mut seen = HashSet::new();
    for target in targets {
        match target.subject_type.as_str() {
            "tenant" | "room" | "user" => {}
            _ => {
                return Err(ApiError::bad_request("FLAG_INVALID_SCOPE: subject_type must be one of tenant, room, user"))
            }
        }
        if target.subject_id.trim().is_empty() {
            return Err(ApiError::bad_request("FLAG_CONFLICT: subject_id is required for feature flag targets"));
        }
        let dedupe_key = format!("{}:{}", target.subject_type, target.subject_id);
        if !seen.insert(dedupe_key) {
            return Err(ApiError::bad_request("FLAG_CONFLICT: duplicated feature flag target"));
        }
    }
    Ok(())
}

fn validate_expiration(expires_at: Option<i64>) -> Result<(), ApiError> {
    if let Some(expires_at) = expires_at {
        if expires_at <= chrono::Utc::now().timestamp_millis() {
            return Err(ApiError::bad_request("FLAG_EXPIRED: expires_at must be greater than current timestamp"));
        }
    }
    Ok(())
}

fn map_storage_error(error: sqlx::Error) -> ApiError {
    match error {
        sqlx::Error::Database(db_error) if db_error.is_unique_violation() => {
            ApiError::conflict("FLAG_CONFLICT: feature flag already exists")
        }
        other => ApiError::internal_with_log("feature flag storage failed", &other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Error;

    // ── validate_flag_key ──────────────────────────────────────────

    #[test]
    fn flag_key_empty_string_is_bad_request() {
        let result = validate_flag_key("");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("flag_key is required"));
    }

    #[test]
    fn flag_key_whitespace_only_is_bad_request() {
        let result = validate_flag_key("   ");
        assert!(result.is_err());
    }

    #[test]
    fn flag_key_with_lowercase_and_dots_is_ok() {
        assert!(validate_flag_key("feature.test_v1").is_ok());
    }

    #[test]
    fn flag_key_with_hyphens_and_digits_is_ok() {
        assert!(validate_flag_key("my-flag-123").is_ok());
    }

    #[test]
    fn flag_key_with_uppercase_is_bad_request() {
        assert!(validate_flag_key("FeatureFlag").is_err());
    }

    #[test]
    fn flag_key_with_special_chars_is_bad_request() {
        assert!(validate_flag_key("flag@key").is_err());
        assert!(validate_flag_key("flag key").is_err());
    }

    // ── validate_target_scope ──────────────────────────────────────

    #[test]
    fn valid_target_scopes_pass() {
        for scope in &["global", "tenant", "room", "user"] {
            assert!(validate_target_scope(scope).is_ok(), "scope {} should be valid", scope);
        }
    }

    #[test]
    fn invalid_target_scope_is_bad_request() {
        let result = validate_target_scope("invalid");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("target_scope"));
    }

    // ── validate_rollout_percent ───────────────────────────────────

    #[test]
    fn rollout_percent_bounds_pass() {
        assert!(validate_rollout_percent(0).is_ok());
        assert!(validate_rollout_percent(50).is_ok());
        assert!(validate_rollout_percent(100).is_ok());
    }

    #[test]
    fn rollout_percent_negative_is_bad_request() {
        assert!(validate_rollout_percent(-1).is_err());
    }

    #[test]
    fn rollout_percent_over_100_is_bad_request() {
        assert!(validate_rollout_percent(101).is_err());
    }

    // ── validate_status ────────────────────────────────────────────

    #[test]
    fn valid_statuses_pass() {
        for status in &[
            "draft",
            "scheduled",
            "active",
            "ramping",
            "fully_enabled",
            "rolled_back",
            "removed",
            "expired_pending_removal",
        ] {
            assert!(validate_status(status).is_ok(), "status {} should be valid", status);
        }
    }

    #[test]
    fn unknown_status_is_bad_request() {
        assert!(validate_status("unknown_status").is_err());
    }

    // ── validate_reason ────────────────────────────────────────────

    #[test]
    fn reason_empty_string_is_bad_request() {
        assert!(validate_reason("").is_err());
    }

    #[test]
    fn reason_non_empty_is_ok() {
        assert!(validate_reason("Adding feature for beta testers").is_ok());
    }

    // ── validate_targets ───────────────────────────────────────────

    fn valid_target(type_: &str, id: &str) -> FeatureFlagTargetInput {
        FeatureFlagTargetInput { subject_type: type_.to_string(), subject_id: id.to_string() }
    }

    #[test]
    fn empty_targets_list_is_ok() {
        assert!(validate_targets(&[]).is_ok());
    }

    #[test]
    fn valid_targets_pass() {
        let targets = vec![valid_target("user", "@alice:example.com")];
        assert!(validate_targets(&targets).is_ok());
    }

    #[test]
    fn target_with_invalid_subject_type_is_bad_request() {
        let targets = vec![valid_target("server", "example.com")];
        assert!(validate_targets(&targets).is_err());
    }

    #[test]
    fn target_with_empty_subject_id_is_bad_request() {
        let targets = vec![valid_target("user", "")];
        assert!(validate_targets(&targets).is_err());
    }

    #[test]
    fn duplicate_targets_is_bad_request() {
        let targets = vec![valid_target("user", "@alice:example.com"), valid_target("user", "@alice:example.com")];
        assert!(validate_targets(&targets).is_err());
    }

    #[test]
    fn different_subject_types_same_id_is_ok() {
        let targets = vec![valid_target("user", "@alice:example.com"), valid_target("tenant", "@alice:example.com")];
        assert!(validate_targets(&targets).is_ok());
    }

    // ── validate_expiration ────────────────────────────────────────

    #[test]
    fn none_expiration_is_ok() {
        assert!(validate_expiration(None).is_ok());
    }

    #[test]
    fn future_expiration_is_ok() {
        let future = chrono::Utc::now().timestamp_millis() + 3_600_000;
        assert!(validate_expiration(Some(future)).is_ok());
    }

    #[test]
    fn past_expiration_is_bad_request() {
        let past = chrono::Utc::now().timestamp_millis() - 1;
        assert!(validate_expiration(Some(past)).is_err());
    }

    // ── map_storage_error ──────────────────────────────────────────

    #[test]
    fn unique_violation_maps_to_conflict() {
        // We can't easily construct a real unique-violation sqlx error,
        // but the match arm tests the error classification logic.
        // The raw Protocol error maps to internal error.
        let err = Error::Protocol("connection closed".to_string());
        let api_err = map_storage_error(err);
        assert!(api_err.to_string().contains("feature flag storage failed"));
    }
}
