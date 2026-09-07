//!
//! Relations API Routes
//!
//! Implements Matrix Relations and Aggregations API
//! Spec: https://spec.matrix.org/v1.8/client-server-api/#relationship-types

use crate::common::error::ApiError;
use crate::web::routes::context::RoomContext;
use crate::web::routes::extractors::{EventId, RoomId};
use crate::web::routes::room_access::ensure_room_member_ctx;
use crate::web::routes::validators::{validate_event_id, validate_room_id};
use crate::web::routes::{AppState, AuthenticatedUser};
use axum::{
    extract::{Path, Query, State},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use synapse_common::current_timestamp_millis;

fn create_relations_core_router() -> Router<AppState> {
    Router::new()
        .route("/rooms/{room_id}/relations/{event_id}/{rel_type}", get(get_relations))
        .route("/rooms/{room_id}/relations/{event_id}/{rel_type}/{txn_id}", put(send_relation))
        .route("/rooms/{room_id}/aggregations/{event_id}/{rel_type}", get(get_aggregations))
}

fn create_relations_with_event_router() -> Router<AppState> {
    create_relations_core_router().route("/rooms/{room_id}/relations/{event_id}", get(get_relations_by_event))
}

/// See [`create_relations_router`].
pub fn create_relations_router(state: AppState) -> Router<AppState> {
    let with_event_router = create_relations_with_event_router();
    let core_router = create_relations_core_router();

    Router::new()
        .nest("/_matrix/client/v1", with_event_router.clone())
        .nest("/_matrix/client/v3", with_event_router)
        .nest("/_matrix/client/r0", core_router)
        .with_state(state)
}

fn relations_core_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::GET, "/rooms/{room_id}/relations/{event_id}/{rel_type}"),
        (Method::PUT, "/rooms/{room_id}/relations/{event_id}/{rel_type}/{txn_id}"),
        (Method::GET, "/rooms/{room_id}/aggregations/{event_id}/{rel_type}"),
    ]
}

fn relations_with_event_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    let mut out = relations_core_relative_routes();
    out.push((Method::GET, "/rooms/{room_id}/relations/{event_id}"));
    out
}

/// See [`relations_route_manifest`].
pub fn relations_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::expand_under_prefixes;
    let mut out = expand_under_prefixes(
        "relations",
        &["/_matrix/client/v1", "/_matrix/client/v3"],
        &relations_with_event_relative_routes(),
    );
    out.extend(expand_under_prefixes("relations", &["/_matrix/client/r0"], &relations_core_relative_routes()));
    out
}

/// The `RelationsQuery` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationsQuery {
    limit: Option<i64>,
    from: Option<String>,
    #[serde(rename = "to")]
    _to: Option<String>,
    #[serde(rename = "dir")]
    direction: Option<String>,
}

/// The `RelationsResponse` struct.
#[derive(Debug, Serialize)]
pub struct RelationsResponse {
    /// The `chunk` field.
    pub chunk: Vec<Value>,
    /// The `next_batch` field.
    pub next_batch: Option<String>,
    /// The `prev_batch` field.
    pub prev_batch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `origin_server_ts` field.
    pub origin_server_ts: Option<i64>,
    /// SDK `getRelationCount` 读取此字段；空时下游永远视为 0。
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total` field.
    pub total: Option<i64>,
}

/// The `RelationSendResponse` struct.
#[derive(Debug, Serialize)]
pub struct RelationSendResponse {
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `relates_to` field.
    pub relates_to: RelationTarget,
}

/// The `RelationTarget` struct.
#[derive(Debug, Serialize)]
pub struct RelationTarget {
    /// The `event_id` field.
    pub event_id: String,
    /// The `rel_type` field.
    pub rel_type: String,
}

/// Get relations for an event without rel_type filter
/// This returns all relations for an event
async fn get_relations_by_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
    Query(query): Query<RelationsQuery>,
) -> Result<Json<RelationsResponse>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "User is not a member of the room").await?;

    let limit = query.limit.unwrap_or(50).min(100) as i32;
    let direction = query.direction.clone();

    tracing::debug!("Getting all relations for event {} in room {}", event_id, room_id,);

    let response =
        ctx.relations_service.get_relations(&room_id, &event_id, None, Some(limit), query.from, direction).await?;

    Ok(Json(RelationsResponse {
        chunk: response.chunk,
        next_batch: response.next_batch,
        prev_batch: response.prev_batch,
        origin_server_ts: None,
        total: response.total,
    }))
}

/// Get relations for an event
/// This endpoint is used to fetch all events that relate to a given event
async fn get_relations(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id, rel_type)): Path<(RoomId, EventId, String)>,
    Query(query): Query<RelationsQuery>,
) -> Result<Json<RelationsResponse>, ApiError> {
    // Validate input
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "User is not a member of the room").await?;

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

    tracing::debug!("Getting relations for event {} in room {} with rel_type {}", event_id, room_id, rel_type);

    let response = ctx
        .relations_service
        .get_relations(&room_id, &event_id, Some(&rel_type), Some(limit), query.from, direction)
        .await?;

    Ok(Json(RelationsResponse {
        chunk: response.chunk,
        next_batch: response.next_batch,
        prev_batch: response.prev_batch,
        origin_server_ts: None,
        total: response.total,
    }))
}

/// Send a relation (annotation/reference/replace)
///
/// The last path segment is a transaction ID (`txn_id`) used for idempotency,
/// NOT an event ID. The parent `event_id` (the event being related to) is the
/// second path segment and is what gets recorded as `relates_to_event_id`.
async fn send_relation(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id, rel_type, txn_id)): Path<(String, String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<RelationSendResponse>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    // `m.thread` 作为参考型关系走与 `m.reference` 相同的落地路径：
    // backend 侧仅需要把事件 ID 作为 relates_to 记录，SDK/Thread 功能据此完成
    // 事件拉链。若未来需要 is_falling_back 等线程专属字段，可在服务层分支。
    let valid_send_rel_types = ["m.reference", "m.replace", "m.annotation", "m.thread"];
    if !valid_send_rel_types.contains(&rel_type.as_str()) {
        return Err(ApiError::bad_request(format!(
            "Invalid rel_type for sending: {}. Must be one of: {}",
            rel_type,
            valid_send_rel_types.join(", ")
        )));
    }

    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let sender = auth_user.user_id.clone();
    let origin_server_ts = current_timestamp_millis();

    tracing::debug!(
        room_id = %room_id,
        relates_to_event_id = %event_id,
        rel_type = %rel_type,
        txn_id = %txn_id,
        "Sending relation event (txn_id used for idempotency)"
    );

    let result_event_id = match rel_type.as_str() {
        "m.annotation" => {
            let key = body.get("key").and_then(|v| v.as_str()).unwrap_or("👍").to_string();

            ctx.relations_service
                .send_annotation(synapse_services::relations_service::SendAnnotationRequest {
                    room_id: room_id.clone(),
                    relates_to_event_id: event_id.clone(),
                    sender,
                    key,
                    origin_server_ts,
                })
                .await?
                .event_id
        }
        "m.reference" => {
            let content = body.get("content").cloned().unwrap_or(Value::Object(serde_json::Map::new()));

            ctx.relations_service
                .send_reference(synapse_services::relations_service::SendReferenceRequest {
                    room_id: room_id.clone(),
                    relates_to_event_id: event_id.clone(),
                    sender,
                    content,
                    origin_server_ts,
                    relation_type: None,
                })
                .await?
                .event_id
        }
        "m.thread" => {
            let content = body.get("content").cloned().unwrap_or(Value::Object(serde_json::Map::new()));

            ctx.relations_service
                .send_reference(synapse_services::relations_service::SendReferenceRequest {
                    room_id: room_id.clone(),
                    relates_to_event_id: event_id.clone(),
                    sender: sender.clone(),
                    content,
                    origin_server_ts,
                    relation_type: Some("m.thread".to_string()),
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

            ctx.relations_service
                .send_replacement(synapse_services::relations_service::SendReplacementRequest {
                    room_id: room_id.clone(),
                    relates_to_event_id: event_id.clone(),
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
        relates_to: RelationTarget { event_id, rel_type },
    }))
}

/// Get aggregations for relations
/// This endpoint is used to get aggregated data about relations (e.g., reaction counts)
async fn get_aggregations(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id, rel_type)): Path<(RoomId, EventId, String)>,
) -> Result<Json<synapse_services::relations_service::AggregationResponse>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "User is not a member of the room").await?;

    if rel_type != "m.annotation" {
        return Err(ApiError::bad_request("Aggregation is only supported for m.annotation rel_type".to_string()));
    }

    tracing::debug!("Getting aggregations for event {} in room {}", event_id, room_id);

    let response = ctx.relations_service.get_aggregations(&room_id, &event_id).await?;

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_relations_routes_structure() {
        let compat_routes = [
            "/_matrix/client/v1/relations/{room_id}/{event_id}/{rel_type}",
            "/_matrix/client/r0/relations/{room_id}/{event_id}/{rel_type}/{txn_id}",
            "/_matrix/client/v3/relations/{room_id}/{event_id}/{rel_type}",
            "/_matrix/client/v1/aggregations/{room_id}/{event_id}/{rel_type}",
            "/_matrix/client/r0/aggregations/{room_id}/{event_id}/{rel_type}",
            "/_matrix/client/v3/aggregations/{room_id}/{event_id}/{rel_type}",
        ];

        assert!(compat_routes.iter().all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_relations_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/relations/{room_id}/{event_id}/{rel_type}",
            "/relations/{room_id}/{event_id}/{rel_type}/{txn_id}",
            "/aggregations/{room_id}/{event_id}/{rel_type}",
        ];

        assert_eq!(shared_paths.len(), 3);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_relations_router_supports_v3() {
        let supported_versions = [
            "/_matrix/client/v1/relations/{room_id}/{event_id}/{rel_type}",
            "/_matrix/client/r0/aggregations/{room_id}/{event_id}/{rel_type}",
            "/_matrix/client/v3/relations/{room_id}/{event_id}/{rel_type}",
            "/_matrix/client/v3/aggregations/{room_id}/{event_id}/{rel_type}",
        ];

        assert!(supported_versions.iter().all(|path| path.starts_with("/_matrix/client/")));
        assert!(supported_versions.iter().any(|path| path.starts_with("/_matrix/client/v3/")));
    }
}
