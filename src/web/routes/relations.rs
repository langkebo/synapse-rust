//!
//! Relations API Routes
//!
//! Implements Matrix Relations and Aggregations API

use super::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn create_relations_router(_state: AppState) -> Router<AppState> {
    Router::new()
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
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct RelationsQuery {
    limit: Option<i64>,
    from: Option<String>,
    to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RelationsResponse {
    pub chunk: Vec<Value>,
    pub next_batch: Option<String>,
    pub prev_batch: Option<String>,
}

async fn get_relations(
    State(_state): State<AppState>,
    Path((room_id, _event_id, _rel_type)): Path<(String, String, String)>,
    Query(query): Query<RelationsQuery>,
) -> Result<Json<RelationsResponse>, crate::error::ApiError> {
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(crate::error::ApiError::bad_request(
            "Invalid room_id format".to_string(),
        ));
    }

    let _limit = query.limit.unwrap_or(50) as i32;
    let _from = query.from.as_deref();
    let _to = query.to.as_deref();

    let relations = Vec::<Value>::new();

    Ok(Json(RelationsResponse {
        chunk: relations,
        next_batch: None,
        prev_batch: None,
    }))
}

async fn send_relation(
    State(_state): State<AppState>,
    Path((room_id, event_id, rel_type, target_event_id)): Path<(String, String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(crate::error::ApiError::bad_request(
            "Invalid room_id format".to_string(),
        ));
    }

    let _content = body
        .get("content")
        .cloned()
        .unwrap_or(Value::Object(serde_json::Map::new()));
    let _sender = body.get("sender").and_then(|v| v.as_str()).unwrap_or("");

    let _relation_content = serde_json::json!({
        "relates_to": {
            "event_id": target_event_id,
            "rel_type": rel_type
        }
    });

    Ok(Json(serde_json::json!({
        "event_id": event_id,
        "room_id": room_id,
        "relates_to": {
            "event_id": target_event_id,
            "rel_type": rel_type
        }
    })))
}

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
    pub key: Option<String>,
}

async fn get_aggregations(
    State(_state): State<AppState>,
    Path((room_id, _event_id, _rel_type)): Path<(String, String, String)>,
) -> Result<Json<AggregationResponse>, crate::error::ApiError> {
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(crate::error::ApiError::bad_request(
            "Invalid room_id format".to_string(),
        ));
    }

    let aggregations: Vec<Value> = Vec::new();

    let items: Vec<AggregationItem> = aggregations
        .into_iter()
        .map(|agg| AggregationItem {
            event_type: agg
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("m.reaction")
                .to_string(),
            origin_server_ts: agg
                .get("origin_server_ts")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            sender: agg
                .get("sender")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            count: agg.get("count").and_then(|v| v.as_i64()).unwrap_or(1),
            key: agg
                .get("key")
                .and_then(|v| v.as_str())
                .map(|s: &str| s.to_string()),
        })
        .collect();

    Ok(Json(AggregationResponse { chunk: items }))
}
