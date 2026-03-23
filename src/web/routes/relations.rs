//!
//! Relations API Routes
//!
//! Implements Matrix Relations and Aggregations API
//! Spec: https://spec.matrix.org/v1.8/client-server-api/#relationship-types

use crate::common::error::ApiError;
use crate::web::routes::AppState;
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

    let _limit = query.limit.unwrap_or(50).min(100) as i32;
    let _direction = query.direction.as_deref().unwrap_or("f");

    tracing::debug!(
        "Getting relations for event {} in room {} with rel_type {}",
        event_id,
        room_id,
        rel_type
    );

    // TODO: Query relations from the database using state.services
    // This would query the event_relations table
    // For now, return empty results as placeholder
    let _services = &state.services;

    // Placeholder: Query event relations from database
    // let relations = services.relations_service.get_relations(...)
    // For now, return empty chunk
    let chunk: Vec<Value> = Vec::new();

    Ok(Json(RelationsResponse {
        chunk,
        next_batch: None,
        prev_batch: None,
        origin_server_ts: None,
    }))
}

/// Send a relation (annotation/reference/replace)
async fn send_relation(
    State(state): State<AppState>,
    Path((room_id, event_id, rel_type, target_event_id)): Path<(String, String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<RelationSendResponse>, ApiError> {
    // Validate input
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    validate_event_id(&target_event_id)?;

    // Validate rel_type
    let valid_send_rel_types = ["m.reference", "m.replace", "m.annotation"];
    if !valid_send_rel_types.contains(&rel_type.as_str()) {
        return Err(ApiError::bad_request(format!(
            "Invalid rel_type for sending: {}. Must be one of: {}",
            rel_type,
            valid_send_rel_types.join(", ")
        )));
    }

    let _services = &state.services;

    // Get sender from body or use authenticated user
    let _sender = body
        .get("sender")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    tracing::debug!(
        "Sending relation: {} -> {} (rel_type: {})",
        event_id,
        target_event_id,
        rel_type
    );

    // TODO: Implement actual relation sending based on rel_type
    // For m.annotation (reactions):
    //   - Store the reaction in event_relations table
    // For m.replace (edits):
    //   - Replace the original event content
    // For m.reference:
    //   - Store the reference

    // Placeholder implementation
    match rel_type.as_str() {
        "m.annotation" => {
            // Handle reactions
            let _key = body.get("key").and_then(|v| v.as_str()).unwrap_or("");
            // services.relations_service.add_annotation(...).await?;
            tracing::debug!("Adding annotation/reaction");
        }
        "m.replace" => {
            // Handle edits
            let _content = body.get("content").cloned().unwrap_or(Value::Object(serde_json::Map::new()));
            // services.relations_service.replace_event(...).await?;
            tracing::debug!("Replacing event content");
        }
        "m.reference" => {
            // Handle references
            // services.relations_service.add_reference(...).await?;
            tracing::debug!("Adding reference");
        }
        _ => {}
    }

    // For now, just return success response
    Ok(Json(RelationSendResponse {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        relates_to: RelationTarget {
            event_id: target_event_id,
            rel_type,
        },
    }))
}

/// Aggregation response for relations (e.g., reactions)
#[derive(Debug, Serialize)]
pub struct AggregationResponse {
    pub chunk: Vec<AggregationItem>,
}

#[derive(Debug, Serialize)]
pub struct AggregationItem {
    #[serde(rename = "type")]
    pub event_type: String,
    pub origin_server_ts: i64,
    pub sender: String,
    pub count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// Get aggregations for relations
/// This endpoint is used to get aggregated data about relations (e.g., reaction counts)
async fn get_aggregations(
    State(_state): State<AppState>,
    Path((room_id, event_id, rel_type)): Path<(String, String, String)>,
) -> Result<Json<AggregationResponse>, ApiError> {
    // Validate input
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    // Only m.annotation (reactions) supports aggregation currently
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

    // TODO: Query aggregated reaction data from database
    // This would typically query something like:
    // SELECT event_type, COUNT(*) as count, key FROM event_relations
    // WHERE room_id = ? AND event_id = ? AND rel_type = 'm.annotation'
    // GROUP BY event_type, key

    // Placeholder: Return empty aggregation
    let items: Vec<AggregationItem> = Vec::new();

    Ok(Json(AggregationResponse { chunk: items }))
}
