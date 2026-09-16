use crate::common::ApiError;
use crate::web::routes::context::AdminContext;
use crate::web::routes::AdminUser;
use crate::web::utils::auth::resolve_request_id as resolve_request_id_from_headers;
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_storage::audit::{decode_audit_event_cursor, AuditEventFilters, CreateAuditEventRequest};

/// See [`create_audit_router`].
pub fn create_audit_router() -> Router<crate::web::routes::AppState> {
    Router::new()
        .route("/_synapse/admin/v1/audit/events", post(create_audit_event).get(list_audit_events))
        .route("/_synapse/admin/v1/audit/events/{event_id}", get(get_audit_event))
}

/// The `CreateAuditEventBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAuditEventBody {
    /// The `actor_id` field.
    pub actor_id: String,
    /// The `action` field.
    pub action: String,
    /// The `resource_type` field.
    pub resource_type: String,
    /// The `resource_id` field.
    pub resource_id: String,
    /// The `result` field.
    pub result: String,
    /// The `request_id` field.
    pub request_id: String,
    /// The `details` field.
    pub details: Option<Value>,
}

/// The `AuditEventQueryParams` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditEventQueryParams {
    /// The `actor_id` field.
    pub actor_id: Option<String>,
    /// The `action` field.
    pub action: Option<String>,
    /// The `resource_type` field.
    pub resource_type: Option<String>,
    /// The `resource_id` field.
    pub resource_id: Option<String>,
    /// The `result` field.
    pub result: Option<String>,
    /// The `limit` field.
    pub limit: Option<i64>,
    /// The `from` field.
    pub from: Option<String>,
}

/// See [`create_audit_event`].
#[axum::debug_handler]
pub async fn create_audit_event(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Json(body): Json<CreateAuditEventBody>,
) -> Result<Json<Value>, ApiError> {
    let event = ctx
        .admin_audit_service
        .create_event(CreateAuditEventRequest {
            actor_id: body.actor_id,
            action: body.action,
            resource_type: body.resource_type,
            resource_id: body.resource_id,
            result: body.result,
            request_id: body.request_id,
            details: body.details,
        })
        .await?;

    Ok(Json(json!(event)))
}

/// See [`list_audit_events`].
#[axum::debug_handler]
pub async fn list_audit_events(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Query(params): Query<AuditEventQueryParams>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.limit.unwrap_or(100).clamp(1, 200);
    // Synapse-compatible fallback: some older callers use `from=0` to mean
    // "start from the first page" rather than providing our tuple cursor.
    let from = match params.from.as_deref() {
        None | Some("") | Some("0") => None,
        other => decode_audit_event_cursor(other),
    };

    if matches!(
        params.from.as_deref(),
        Some(value) if !value.is_empty() && value != "0"
    ) && from.is_none()
    {
        return Err(ApiError::bad_request("Invalid from cursor"));
    }

    let (events, total, next_batch) = ctx
        .admin_audit_service
        .list_events(AuditEventFilters {
            actor_id: params.actor_id,
            action: params.action,
            resource_type: params.resource_type,
            resource_id: params.resource_id,
            result: params.result,
            limit,
            from,
        })
        .await?;

    Ok(Json(json!({
        "events": events,
        "total": total,
        "next_batch": next_batch
    })))
}

/// See [`get_audit_event`].
#[axum::debug_handler]
pub async fn get_audit_event(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(event_id): Path<EventId>,
) -> Result<Json<Value>, ApiError> {
    let event = ctx
        .admin_audit_service
        .get_event(&event_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Audit event not found"))?;

    Ok(Json(json!(event)))
}

/// See [`resolve_request_id`].
pub(crate) fn resolve_request_id(headers: &HeaderMap) -> String {
    resolve_request_id_from_headers(headers)
}

/// See [`record_audit_event`].
pub(crate) async fn record_audit_event(
    ctx: &AdminContext,
    actor_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    request_id: String,
    details: Value,
) -> Result<(), ApiError> {
    let result = ctx
        .admin_audit_service
        .create_event(CreateAuditEventRequest {
            actor_id: actor_id.to_string(),
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: resource_id.to_string(),
            result: "success".to_string(),
            request_id,
            details: Some(details),
        })
        .await;

    if let Err(error) = result {
        ::tracing::warn!(
            target: "admin_audit",
            %error,
            "Failed to record admin audit event, but continuing to ensure API availability"
        );
    }

    Ok(())
}

use crate::web::routes::extractors::EventId;
