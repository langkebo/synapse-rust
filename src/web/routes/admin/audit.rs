use crate::common::ApiError;
use crate::storage::{AuditEventFilters, CreateAuditEventRequest};
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn create_audit_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_synapse/admin/v1/audit/events",
            post(create_audit_event).get(list_audit_events),
        )
        .route(
            "/_synapse/admin/v1/audit/events/{event_id}",
            get(get_audit_event),
        )
}

#[derive(Debug, Deserialize)]
pub struct CreateAuditEventBody {
    pub actor_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub result: String,
    pub request_id: String,
    pub details: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct AuditEventQueryParams {
    pub actor_id: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub result: Option<String>,
    pub limit: Option<i64>,
    pub from: Option<i64>,
}

#[axum::debug_handler]
pub async fn create_audit_event(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<CreateAuditEventBody>,
) -> Result<Json<Value>, ApiError> {
    let event = state
        .services
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

#[axum::debug_handler]
pub async fn list_audit_events(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(params): Query<AuditEventQueryParams>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.limit.unwrap_or(100).clamp(1, 200);
    let offset = params.from.unwrap_or(0).max(0);

    let (events, total) = state
        .services
        .admin_audit_service
        .list_events(AuditEventFilters {
            actor_id: params.actor_id,
            action: params.action,
            resource_type: params.resource_type,
            resource_id: params.resource_id,
            result: params.result,
            limit,
            offset,
        })
        .await?;

    let next_token = if offset + limit < total {
        Some(offset + limit)
    } else {
        None
    };

    Ok(Json(json!({
        "events": events,
        "total": total,
        "next_token": next_token
    })))
}

#[axum::debug_handler]
pub async fn get_audit_event(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(event_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let event = state
        .services
        .admin_audit_service
        .get_event(&event_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Audit event not found"))?;

    Ok(Json(json!(event)))
}

pub(crate) fn resolve_request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("req-{}", uuid::Uuid::new_v4()))
}

pub(crate) async fn record_audit_event(
    state: &AppState,
    actor_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    request_id: String,
    details: Value,
) -> Result<(), ApiError> {
    state
        .services
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
        .await?;

    Ok(())
}
