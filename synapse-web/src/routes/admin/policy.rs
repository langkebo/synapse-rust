//! MSC4284: Policy server admin HTTP endpoints.
//!
//! Exposes two admin-only endpoints that operators can use to inspect the
//! policy-server configuration and trigger ad-hoc policy checks:
//!
//! - `GET /_synapse/admin/v1/policy/status` — returns the current
//!   `PolicyServerConfig` state (enabled, endpoint, fail_mode).
//! - `POST /_synapse/admin/v1/policy/check` — synchronously invokes
//!   `PolicyService::check_policy` for a `(room_id, user_id, action)`
//!   triple and returns the resulting `PolicyResult`.
//!
//! Both endpoints require `AdminUser` authentication (enforced by the
//! admin-auth middleware layer that wraps the entire admin module router).

use crate::routes::context::AdminContext;
use crate::routes::AdminUser;
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use synapse_common::ApiError;
use synapse_services::policy_service::PolicyResult;

/// Actions recognised by the policy check endpoint. Kept in lock-step with
/// `PolicyService::check_policy`'s `action` parameter — extending this list
/// requires extending the service-side dispatch as well.
const KNOWN_ACTIONS: &[&str] = &["create", "join", "invite", "send"];

/// See [`create_policy_router`].
pub fn create_policy_router() -> Router<crate::routes::AppState> {
    Router::new()
        .route("/_synapse/admin/v1/policy/status", get(get_policy_status))
        .route("/_synapse/admin/v1/policy/check", post(check_policy))
}

/// Request body for `POST /_synapse/admin/v1/policy/check`.
///
/// All three fields are required; `validate()` additionally enforces that
/// `action` is one of [`KNOWN_ACTIONS`] and that `room_id` / `user_id` are
/// non-empty after trimming. This fail-closed posture prevents a malformed
/// request from accidentally bypassing the policy server.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyCheckRequest {
    /// The room being acted upon, e.g. `!room:example.com`.
    pub room_id: String,
    /// The user performing the action, e.g. `@alice:example.com`.
    pub user_id: String,
    /// The action being performed. Must be one of [`KNOWN_ACTIONS`].
    pub action: String,
}

impl PolicyCheckRequest {
    /// Validate the request after JSON deserialization.
    ///
    /// Returns `Err(ApiError::bad_request)` if any field is empty/whitespace
    /// or if `action` is not a known policy action.
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.room_id.trim().is_empty() {
            return Err(ApiError::bad_request("room_id must not be empty"));
        }
        if self.user_id.trim().is_empty() {
            return Err(ApiError::bad_request("user_id must not be empty"));
        }
        if !KNOWN_ACTIONS.contains(&self.action.as_str()) {
            return Err(ApiError::bad_request(format!(
                "action must be one of {:?}, got {:?}",
                KNOWN_ACTIONS, self.action
            )));
        }
        Ok(())
    }
}

/// Response body for `GET /_synapse/admin/v1/policy/status`.
///
/// Mirrors the documented MSC4284 admin API shape: exactly three fields
/// (`enabled`, `endpoint`, `fail_mode`). The `endpoint` is `None` (serialized
/// as JSON `null`) when the policy server is not configured.
#[derive(Debug, Clone, Serialize)]
pub struct PolicyStatusResponse {
    /// Whether the policy server integration is enabled.
    pub enabled: bool,
    /// The configured policy server endpoint, or `None` if unconfigured.
    pub endpoint: Option<String>,
    /// `"fail_open"` or `"fail_closed"` — what happens when the policy
    /// server is unreachable.
    pub fail_mode: String,
}

/// Map a `PolicyServerConfig::fail_open` boolean to the MSC4284 string form.
///
/// `true` → `"fail_open"` (operations proceed on policy-server failure).
/// `false` → `"fail_closed"` (operations are denied on failure — the safer
/// default for moderation).
fn fail_mode_string(fail_open: bool) -> &'static str {
    if fail_open {
        "fail_open"
    } else {
        "fail_closed"
    }
}

/// See [`get_policy_status`].
#[allow(clippy::unused_async)]
pub async fn get_policy_status(_admin: AdminUser, State(ctx): State<AdminContext>) -> Result<Json<Value>, ApiError> {
    let cfg = &ctx.config.policy_server;
    let resp = PolicyStatusResponse {
        enabled: cfg.is_configured(),
        endpoint: cfg.endpoint.clone(),
        fail_mode: fail_mode_string(cfg.fail_open).to_string(),
    };
    Ok(Json(json!({
        "enabled": resp.enabled,
        "endpoint": resp.endpoint,
        "fail_mode": resp.fail_mode,
    })))
}

/// See [`check_policy`].
#[axum::debug_handler]
pub async fn check_policy(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Json(body): Json<PolicyCheckRequest>,
) -> Result<Json<Value>, ApiError> {
    body.validate()?;

    // The policy check endpoint treats `room_id` as the entity being acted
    // upon and `user_id` as the actor. This matches the room-scoped actions
    // ("create", "join", "invite", "send") exposed by `PolicyService`.
    let result = ctx.policy_service.check_policy("room", &body.room_id, &body.user_id, &body.action).await;

    let resp = match result {
        PolicyResult::Allow => json!({ "result": "allow", "allowed": true }),
        PolicyResult::Deny(reason) => json!({ "result": "deny", "allowed": false, "reason": reason }),
    };
    Ok(Json(resp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_common::ApiErrorKind;

    #[test]
    fn policy_check_request_parses_valid_json() {
        let json = r#"{"room_id": "!room:example.com", "user_id": "@alice:example.com", "action": "join"}"#;
        let req: PolicyCheckRequest = serde_json::from_str(json).expect("valid JSON should parse");
        assert_eq!(req.room_id, "!room:example.com");
        assert_eq!(req.user_id, "@alice:example.com");
        assert_eq!(req.action, "join");
    }

    #[test]
    fn policy_check_request_rejects_missing_room_id() {
        let json = r#"{"user_id": "@alice:example.com", "action": "join"}"#;
        let result: Result<PolicyCheckRequest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "missing room_id should fail deserialization");
    }

    #[test]
    fn policy_check_request_rejects_missing_user_id() {
        let json = r#"{"room_id": "!room:example.com", "action": "join"}"#;
        let result: Result<PolicyCheckRequest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "missing user_id should fail deserialization");
    }

    #[test]
    fn policy_check_request_rejects_missing_action() {
        let json = r#"{"room_id": "!room:example.com", "user_id": "@alice:example.com"}"#;
        let result: Result<PolicyCheckRequest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "missing action should fail deserialization");
    }

    #[test]
    fn policy_check_request_validate_accepts_known_actions() {
        for action in ["create", "join", "invite", "send"] {
            let req = PolicyCheckRequest {
                room_id: "!room:example.com".to_string(),
                user_id: "@alice:example.com".to_string(),
                action: action.to_string(),
            };
            req.validate().unwrap_or_else(|e| panic!("action '{action}' should be valid: {e:?}"));
        }
    }

    #[test]
    fn policy_check_request_validate_rejects_unknown_action() {
        // Fail-closed: unknown actions must be rejected so a malformed request
        // cannot bypass the policy server by naming an action the handler
        // does not know how to dispatch.
        let req = PolicyCheckRequest {
            room_id: "!room:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            action: "delete_everything".to_string(),
        };
        let err = req.validate().expect_err("unknown action should be rejected");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
    }

    #[test]
    fn policy_check_request_validate_rejects_empty_room_id() {
        let req = PolicyCheckRequest {
            room_id: String::new(),
            user_id: "@alice:example.com".to_string(),
            action: "join".to_string(),
        };
        req.validate().expect_err("empty room_id should be rejected");
    }

    #[test]
    fn policy_check_request_validate_rejects_empty_user_id() {
        let req = PolicyCheckRequest {
            room_id: "!room:example.com".to_string(),
            user_id: String::new(),
            action: "join".to_string(),
        };
        req.validate().expect_err("empty user_id should be rejected");
    }

    #[test]
    fn policy_check_request_validate_rejects_whitespace_only_fields() {
        // Whitespace-only fields must be treated as empty after trimming.
        let req = PolicyCheckRequest {
            room_id: "   ".to_string(),
            user_id: "@alice:example.com".to_string(),
            action: "join".to_string(),
        };
        req.validate().expect_err("whitespace-only room_id should be rejected");
    }

    #[test]
    fn policy_status_response_serializes_to_expected_shape() {
        // The GET /policy/status response must contain exactly the three
        // fields documented in the MSC4284 admin API: enabled, endpoint,
        // fail_mode. This locks down the shape against accidental drift.
        let resp = PolicyStatusResponse {
            enabled: true,
            endpoint: Some("https://policy.example.com".to_string()),
            fail_mode: "fail_closed".to_string(),
        };
        let json = serde_json::to_value(&resp).expect("response should serialize");
        assert_eq!(json["enabled"], serde_json::Value::Bool(true));
        assert_eq!(json["endpoint"], serde_json::json!("https://policy.example.com"));
        assert_eq!(json["fail_mode"], serde_json::json!("fail_closed"));
    }

    #[test]
    fn policy_status_response_serializes_disabled_state() {
        // When the policy server is disabled, endpoint should be null and
        // fail_mode should still be present (defaults to "fail_open" per
        // PolicyServerConfig::default).
        let resp = PolicyStatusResponse { enabled: false, endpoint: None, fail_mode: "fail_open".to_string() };
        let json = serde_json::to_value(&resp).expect("disabled response should serialize");
        assert_eq!(json["enabled"], serde_json::Value::Bool(false));
        assert!(json["endpoint"].is_null());
        assert_eq!(json["fail_mode"], serde_json::json!("fail_open"));
    }

    #[test]
    fn fail_mode_string_is_fail_open_when_config_fail_open_true() {
        // Maps PolicyServerConfig.fail_open = true → "fail_open".
        assert_eq!(fail_mode_string(true), "fail_open");
    }

    #[test]
    fn fail_mode_string_is_fail_closed_when_config_fail_open_false() {
        // Maps PolicyServerConfig.fail_open = false → "fail_closed".
        // Fail-closed is the safer default for moderation: if the policy
        // server is unreachable, deny the operation.
        assert_eq!(fail_mode_string(false), "fail_closed");
    }
}
