use crate::common::ApiError;
use crate::services::admin_audit_service::AdminAuditService;
use crate::storage::{
    CreateFeatureFlagRequest, FeatureFlag, FeatureFlagFilters, FeatureFlagStorage,
    FeatureFlagTargetInput, UpdateFeatureFlagRequest,
};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;

pub struct FeatureFlagService {
    storage: Arc<FeatureFlagStorage>,
    audit_service: Arc<AdminAuditService>,
}

impl FeatureFlagService {
    pub fn new(storage: Arc<FeatureFlagStorage>, audit_service: Arc<AdminAuditService>) -> Self {
        Self {
            storage,
            audit_service,
        }
    }

    pub async fn create_flag(
        &self,
        actor_id: &str,
        request_id: &str,
        request: CreateFeatureFlagRequest,
    ) -> Result<FeatureFlag, ApiError> {
        validate_create_request(&request)?;
        let created_ts = chrono::Utc::now().timestamp_millis();
        let flag = self
            .storage
            .create_flag(&request, actor_id, created_ts)
            .await
            .map_err(map_storage_error)?;

        self.audit_service
            .create_event(crate::storage::CreateAuditEventRequest {
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
            .ok_or_else(|| ApiError::not_found(format!("feature flag not found: {}", flag_key)))?;

        self.audit_service
            .create_event(crate::storage::CreateAuditEventRequest {
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
            .ok_or_else(|| ApiError::not_found(format!("feature flag not found: {}", flag_key)))
    }

    pub async fn list_flags(
        &self,
        filters: FeatureFlagFilters,
    ) -> Result<(Vec<FeatureFlag>, i64), ApiError> {
        if let Some(ref scope) = filters.target_scope {
            validate_target_scope(scope)?;
        }
        if let Some(ref status) = filters.status {
            validate_status(status)?;
        }

        self.storage
            .list_flags(&filters)
            .await
            .map_err(map_storage_error)
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

fn validate_update_request(
    flag_key: &str,
    request: &UpdateFeatureFlagRequest,
) -> Result<(), ApiError> {
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
        return Err(ApiError::bad_request(
            "FLAG_INVALID_SCOPE: flag_key is required",
        ));
    }
    if !flag_key
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
    {
        return Err(ApiError::bad_request(
            "FLAG_INVALID_SCOPE: flag_key must use lowercase alphanumeric, dot, underscore or hyphen",
        ));
    }
    Ok(())
}

fn validate_target_scope(target_scope: &str) -> Result<(), ApiError> {
    match target_scope {
        "global" | "tenant" | "room" | "user" => Ok(()),
        _ => Err(ApiError::bad_request(
            "FLAG_INVALID_SCOPE: target_scope must be one of global, tenant, room, user",
        )),
    }
}

fn validate_rollout_percent(rollout_percent: i32) -> Result<(), ApiError> {
    if (0..=100).contains(&rollout_percent) {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "FLAG_GUARDRAIL_BLOCKED: rollout_percent must be between 0 and 100",
        ))
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
        _ => Err(ApiError::bad_request(
            "FLAG_CONFLICT: unsupported feature flag status",
        )),
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
                return Err(ApiError::bad_request(
                    "FLAG_INVALID_SCOPE: subject_type must be one of tenant, room, user",
                ))
            }
        }
        if target.subject_id.trim().is_empty() {
            return Err(ApiError::bad_request(
                "FLAG_CONFLICT: subject_id is required for feature flag targets",
            ));
        }
        let dedupe_key = format!("{}:{}", target.subject_type, target.subject_id);
        if !seen.insert(dedupe_key) {
            return Err(ApiError::bad_request(
                "FLAG_CONFLICT: duplicated feature flag target",
            ));
        }
    }
    Ok(())
}

fn validate_expiration(expires_at: Option<i64>) -> Result<(), ApiError> {
    if let Some(expires_at) = expires_at {
        if expires_at <= chrono::Utc::now().timestamp_millis() {
            return Err(ApiError::bad_request(
                "FLAG_EXPIRED: expires_at must be greater than current timestamp",
            ));
        }
    }
    Ok(())
}

fn map_storage_error(error: sqlx::Error) -> ApiError {
    match error {
        sqlx::Error::Database(db_error) if db_error.is_unique_violation() => {
            ApiError::conflict("FLAG_CONFLICT: feature flag already exists")
        }
        other => ApiError::internal(format!("feature flag storage failed: {}", other)),
    }
}
