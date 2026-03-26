//!
//! Relations API Routes
//!
//! Implements Matrix Relations and Aggregations API
//! Spec: https://spec.matrix.org/v1.8/client-server-api/#relationship-types

use crate::common::error::ApiError;
use crate::web::routes::{AppState, AuthenticatedUser};
use axum::{
    extract::{Path, Query, State},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn create_relations_router(state: AppState) -> Router<AppState> {
    Router::new()
        // v1 路径 (Matrix 1.6+)
        .route(
            "/_matrix/client/v1/relations/{room_id}/{event_id}/{rel_type}",
            get(get_relations),
        )
        .route(
            "/_matrix/client/v1/relations/{room_id}/{event_id}/{rel_type}/{event_id}",
            put(send_relation),
        )
        .route(
            "/_matrix/client/v1/aggregations/{room_id}/{event_id}/{rel_type}",
            get(get_aggregations),
        )
        // r0 路径兼容
        .route(
            "/_matrix/client/r0/relations/{room_id}/{event_id}/{rel_type}",
            get(get_relations),
        )
        .route(
            "/_matrix/client/r0/relations/{room_id}/{event_id}/{rel_type}/{event_id}",
            put(send_relation),
        )
        .route(
            "/_matrix/client/r0/aggregations/{room_id}/{event_id}/{rel_type}",
            get(get_aggregations),
        )
        .with_state(state)
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct RelationsQuery {
    limit: Option<i64>,
    from: Option<String>,
    to: Option<String>,
    #[serde(rename = "dir")]
    direction: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RelationsResponse {
    pub chunk: Vec<Value>,
    pub next_batch: Option<String>,
    pub prev_batch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_server_ts: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct RelationSendResponse {
    pub event_id: String,
    pub room_id: String,
    pub relates_to: RelationTarget,
}

#[derive(Debug, Serialize)]
pub struct RelationTarget {
    pub event_id: String,
    pub rel_type: String,
}

/// Validates room_id format
fn validate_room_id(room_id: &str) -> Result<(), ApiError> {
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(ApiError::bad_request("Invalid room_id format".to_string()));
    }
    Ok(())
}

/// Validates event_id format
fn validate_event_id(event_id: &str) -> Result<(), ApiError> {
    if !event_id.starts_with('$') {
        return Err(ApiError::bad_request("Invalid event_id format".to_string()));
    }
    Ok(())
}

/// Get relations for an event
/// This endpoint is used to fetch all events that relate to a given event
async fn get_relations(
    State(state): State<AppState>,
    Path((room_id, event_id, rel_type)): Path<(String, String, String)>,
    Query(query): Query<RelationsQuery>,
) -> Result<Json<RelationsResponse>, ApiError> {
    // Validate input
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    // Validate rel_type
    let valid_rel_types = ["m.reference", "m.replace", "m.thread", "m.annotation"];
    if !valid_rel_types.contains(&rel_type.as_str()) {
        return Err(ApiError::bad_request(format!(
            "Invalid rel_type: {}. Must be one of: {}",
            rel_type,
            valid_rel_types.join(", ")
        )));
    }

    let limit = query.limit.unwrap_or(50).min(100) as i32;
    let direction = query.direction.clone();

    tracing::debug!(
        "Getting relations for event {} in room {} with rel_type {}",
        event_id,
        room_id,
        rel_type
    );

    let response = state
        .services
        .relations_service
        .get_relations(
            &room_id,
            &event_id,
            Some(&rel_type),
            Some(limit),
            query.from,
            direction,
        )
        .await?;

    Ok(Json(RelationsResponse {
        chunk: response.chunk,
        next_batch: response.next_batch,
        prev_batch: response.prev_batch,
        origin_server_ts: None,
    }))
}

/// Send a relation (annotation/reference/replace)
async fn send_relation(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id, rel_type, target_event_id)): Path<(String, String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<RelationSendResponse>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    validate_event_id(&target_event_id)?;

    let valid_send_rel_types = ["m.reference", "m.replace", "m.annotation"];
    if !valid_send_rel_types.contains(&rel_type.as_str()) {
        return Err(ApiError::bad_request(format!(
            "Invalid rel_type for sending: {}. Must be one of: {}",
            rel_type,
            valid_send_rel_types.join(", ")
        )));
    }

    if !state.services.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let sender = auth_user.user_id.clone();
    let origin_server_ts = chrono::Utc::now().timestamp_millis();

    let result_event_id = match rel_type.as_str() {
        "m.annotation" => {
            let key = body
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("👍")
                .to_string();

            state
                .services
                .relations_service
                .send_annotation(crate::services::relations_service::SendAnnotationRequest {
                    room_id: room_id.clone(),
                    relates_to_event_id: target_event_id.clone(),
                    sender,
                    key,
                    origin_server_ts,
                })
                .await?
                .event_id
        }
        "m.reference" => {
            let content = body
                .get("content")
                .cloned()
                .unwrap_or(Value::Object(serde_json::Map::new()));

            state
                .services
                .relations_service
                .send_reference(crate::services::relations_service::SendReferenceRequest {
                    room_id: room_id.clone(),
                    relates_to_event_id: target_event_id.clone(),
                    sender,
                    content,
                    origin_server_ts,
                })
                .await?
                .event_id
        }
        "m.replace" => {
            let new_content = body
                .get("content")
                .cloned()
                .or_else(|| body.get("m.new_content").cloned())
                .unwrap_or(Value::Object(serde_json::Map::new()));

            state
                .services
                .relations_service
                .send_replacement(crate::services::relations_service::SendReplacementRequest {
                    room_id: room_id.clone(),
                    relates_to_event_id: target_event_id.clone(),
                    sender,
                    new_content,
                    origin_server_ts,
                })
                .await?
                .event_id
        }
        _ => event_id.clone(),
    };

    Ok(Json(RelationSendResponse {
        event_id: result_event_id,
        room_id,
        relates_to: RelationTarget {
            event_id: target_event_id,
            rel_type,
        },
    }))
}

/// Get aggregations for relations
/// This endpoint is used to get aggregated data about relations (e.g., reaction counts)
async fn get_aggregations(
    State(state): State<AppState>,
    Path((room_id, event_id, rel_type)): Path<(String, String, String)>,
) -> Result<Json<crate::services::relations_service::AggregationResponse>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    if rel_type != "m.annotation" {
        return Err(ApiError::bad_request(
            "Aggregation is only supported for m.annotation rel_type".to_string(),
        ));
    }

    tracing::debug!(
        "Getting aggregations for event {} in room {}",
        event_id,
        room_id
    );

    let response = state
        .services
        .relations_service
        .get_aggregations(&room_id, &event_id)
        .await?;

    Ok(Json(response))
}
