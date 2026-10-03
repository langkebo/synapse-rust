//!
//! Relations API Routes
//!
//! Implements Matrix Relations and Aggregations API
//! Spec: <https://spec.matrix.org/v1.8/client-server-api/#relationship-types>

use crate::routes::context::RoomContext;
use crate::routes::extractors::{EventId, RoomId};
use crate::routes::room_access::ensure_room_member_ctx;
use crate::routes::validators::{validate_event_id, validate_room_id};
use crate::routes::{AppState, AuthenticatedUser};
use axum::{
    extract::{Path, Query, State},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;
use synapse_services::relations_service::RelationQuery;

/// Client (`/_matrix/client/{v1,v3}`) relations **read** surface.
///
/// Only `GET` lives here. The 4-segment path is the spec's "relations filtered by
/// relation type and event type" read, so its 4th segment is `{event_type}` and
/// means exactly that.
///
/// This path used to also carry the write endpoint as `PUT …/{relType}/{txnId}`.
/// axum/matchit normalises path-parameter names, so the two methods could not be
/// registered as separate `.route()`s and the shared 4th segment had to be named
/// after one of the two meanings (it was named `{event_type}` while the handler
/// read it as a txn id — a path parameter that lied). The write endpoint now has
/// its own path under the vendor prefix: [`create_relations_vendor_router`].
fn create_relations_core_router() -> Router<AppState> {
    Router::new()
        .route("/rooms/{room_id}/relations/{event_id}/{rel_type}", get(get_relations))
        .route("/rooms/{room_id}/relations/{event_id}/{rel_type}/{event_type}", get(get_relations_by_type))
        .route("/rooms/{room_id}/aggregations/{event_id}/{rel_type}", get(get_aggregations))
}

/// Write surface for relations, mounted by the assembly at `/_matrix/vendor/v1`.
///
/// Sending a relation this way is **not** a spec endpoint — per the spec, clients
/// send an ordinary room event carrying `m.relates_to` — so per ISSUE-13 it lives
/// under the vendor prefix rather than squatting on the spec's read path. The
/// last segment is the client's `txn_id` and means exactly that: the service
/// records it through the same durable `room_event_txn_dedup` marker that
/// `/rooms/{roomId}/send/{eventType}/{txnId}` uses, so a retry returns the
/// original `event_id` instead of creating a second relation event.
///
/// The ledger's `registered_by` for this route is `relations` (the module that
/// defines this router), not `vendor` (the assembly that mounts it) — see
/// `scripts/contract/ledger_origins.txt`.
pub fn create_relations_vendor_router() -> Router<AppState> {
    Router::new().route("/rooms/{room_id}/relations/{event_id}/{rel_type}/{txn_id}", put(send_relation))
}

fn create_relations_with_event_router() -> Router<AppState> {
    create_relations_core_router().route("/rooms/{room_id}/relations/{event_id}", get(get_relations_by_event))
}

/// See [`create_relations_router`].
pub fn create_relations_router(state: AppState) -> Router<AppState> {
    let with_event_router = create_relations_with_event_router();

    Router::new()
        .nest("/_matrix/client/v1", with_event_router.clone())
        .nest("/_matrix/client/v3", with_event_router)
        .with_state(state)
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
    /// MSC3981: also return events that only relate to the target through
    /// another event. Stable spelling.
    recurse: Option<bool>,
    /// MSC3981: unstable spelling of the same parameter.
    #[serde(rename = "org.matrix.msc3981.recurse")]
    msc3981_recurse: Option<bool>,
}

impl RelationsQuery {
    /// The requested recursion mode, or `None` when neither spelling is present.
    ///
    /// MSC3981 distinguishes "absent" from `false`: the parameter is optional and
    /// defaults to `false`, but `recursion_depth` must be part of the response
    /// whenever the parameter **was passed**. Clients may use either the stable
    /// `recurse` spelling or the unstable `org.matrix.msc3981.recurse` one; the
    /// stable spelling wins when both are present.
    fn recurse_flag(&self) -> Option<bool> {
        self.recurse.or(self.msc3981_recurse)
    }
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
    /// MSC3981: the recursion depth limit the server applied. Present exactly
    /// when the request carried a `recurse` parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `recursion_depth` field.
    pub recursion_depth: Option<i32>,
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
    relations_response(&ctx, &auth_user, &room_id, &event_id, None, None, query).await
}

/// Get relations for an event
/// This endpoint is used to fetch all events that relate to a given event
async fn get_relations(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id, rel_type)): Path<(RoomId, EventId, String)>,
    Query(query): Query<RelationsQuery>,
) -> Result<Json<RelationsResponse>, ApiError> {
    relations_response(&ctx, &auth_user, &room_id, &event_id, Some(rel_type), None, query).await
}

/// spec 的 4 段路由 `/{relType}/{eventType}`：在 `rel_type` 之上再用事件自身的
/// `event_type` 收窄返回集（例如 `m.annotation` + `m.reaction` 只取表情回应）。
async fn get_relations_by_type(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id, rel_type, event_type)): Path<(RoomId, EventId, String, String)>,
    Query(query): Query<RelationsQuery>,
) -> Result<Json<RelationsResponse>, ApiError> {
    relations_response(&ctx, &auth_user, &room_id, &event_id, Some(rel_type), Some(event_type), query).await
}

/// 三个 `/relations` 读路由（2 段 / 3 段 / 4 段）的公共实现。
async fn relations_response(
    ctx: &RoomContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
    event_id: &str,
    rel_type: Option<String>,
    event_type: Option<String>,
    query: RelationsQuery,
) -> Result<Json<RelationsResponse>, ApiError> {
    validate_room_id(room_id)?;
    validate_event_id(event_id)?;

    if let Some(rel_type) = rel_type.as_deref() {
        let valid_rel_types = ["m.reference", "m.replace", "m.thread", "m.annotation"];
        if !valid_rel_types.contains(&rel_type) {
            return Err(ApiError::bad_request(format!(
                "Invalid rel_type: {}. Must be one of: {}",
                rel_type,
                valid_rel_types.join(", ")
            )));
        }
    }

    ensure_room_member_ctx(ctx, auth_user, room_id, "User is not a member of the room").await?;

    let relation_query = RelationQuery {
        rel_type,
        event_type,
        limit: Some(query.limit.unwrap_or(50).min(100) as i32),
        recurse: query.recurse_flag(),
        from: query.from,
        direction: query.direction.clone(),
    };

    tracing::debug!(room_id = %room_id, event_id = %event_id, "Getting relations");

    let response = ctx.relations_service.get_relations(room_id, event_id, relation_query).await?;

    Ok(Json(RelationsResponse {
        chunk: response.chunk,
        next_batch: response.next_batch,
        prev_batch: response.prev_batch,
        origin_server_ts: None,
        total: response.total,
        recursion_depth: response.recursion_depth,
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

    // Authorization, not just existence — mirroring `get_relations`, which has
    // always called `ensure_room_member_ctx` while this write path did not. Any
    // authenticated user could previously write `m.reference` / `m.replace` /
    // `m.thread` / `m.annotation` into ANY existing room.
    //
    // Power-level key: a reaction is governed by `events["m.reaction"]`; the other
    // three relation types land as ordinary room messages.
    let pl_event_type = if rel_type == "m.annotation" { "m.reaction" } else { "m.room.message" };
    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "You must be a room member to send relations").await?;
    ctx.room_auth.verify_message_event_write(&room_id, &auth_user.user_id, pl_event_type).await?;

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
                    txn_id: Some(txn_id.clone()),
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
                    txn_id: Some(txn_id.clone()),
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
                    txn_id: Some(txn_id.clone()),
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
                    txn_id: Some(txn_id.clone()),
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
    /// 4 段的 `PUT` **只能**出现在 vendor 前缀上，client 前缀的 4 段路径只服务 `GET`。
    ///
    /// 断言的是**派生表**（真实注册面）而不是手写字符串常量：旧的两个用例只比较常量是否
    /// 以 `/_matrix/client/` 开头，路由真改了它们也不会红（铁律 8 的反面）。
    #[test]
    fn relations_write_route_lives_under_vendor_prefix_only() {
        let ledger = crate::routes::assembly::declared_ledger_all();
        let puts: Vec<&str> = ledger
            .iter()
            .filter(|entry| entry.method == axum::http::Method::PUT && entry.path.contains("/relations/"))
            .map(|entry| entry.path)
            .collect();

        assert_eq!(
            puts,
            vec!["/_matrix/vendor/v1/rooms/{room_id}/relations/{event_id}/{rel_type}/{txn_id}"],
            "关系写入端点必须只在 vendor 前缀上，且末段是显式 txn_id"
        );
    }

    /// 读取面仍是 spec 形状：client 4 段路径服务 `GET`（`{event_type}` 过滤）。
    #[test]
    fn client_four_segment_relations_path_is_read_only() {
        let ledger = crate::routes::assembly::declared_ledger_all();
        let methods: Vec<_> = ledger
            .iter()
            .filter(|entry| {
                entry.path == "/_matrix/client/v3/rooms/{room_id}/relations/{event_id}/{rel_type}/{event_type}"
            })
            .map(|entry| entry.method.clone())
            .collect();

        assert_eq!(methods, vec![axum::http::Method::GET]);
    }

    use super::RelationsQuery;

    fn parse(value: serde_json::Value) -> RelationsQuery {
        serde_json::from_value(value).expect("query parameters should deserialize")
    }

    /// MSC3981: `recurse` has been a stable parameter since client-server v1.10,
    /// so a compliant client sending either spelling must not be rejected. This
    /// is the regression that made the endpoint answer **400** — the struct used
    /// to carry `#[serde(deny_unknown_fields)]` with no `recurse` field at all.
    #[test]
    fn msc3981_recurse_accepts_both_spellings() {
        assert_eq!(parse(serde_json::json!({"recurse": true})).recurse_flag(), Some(true));
        assert_eq!(parse(serde_json::json!({"recurse": false})).recurse_flag(), Some(false));
        assert_eq!(parse(serde_json::json!({"org.matrix.msc3981.recurse": true})).recurse_flag(), Some(true));
    }

    /// Absent and `false` are different requests: MSC3981 mandates
    /// `recursion_depth` in the response for the latter but not the former.
    #[test]
    fn msc3981_recurse_defaults_to_absent_not_false() {
        assert_eq!(parse(serde_json::json!({})).recurse_flag(), None);
        assert_eq!(parse(serde_json::json!({"limit": 10})).recurse_flag(), None);
    }

    /// The stable spelling wins when both are sent, and unknown parameters are
    /// still rejected (the strictness that caught `recurse` in the first place
    /// must not be dropped wholesale to fix it).
    #[test]
    fn msc3981_recurse_prefers_the_stable_spelling_and_keeps_rejecting_junk() {
        assert_eq!(
            parse(serde_json::json!({"recurse": false, "org.matrix.msc3981.recurse": true})).recurse_flag(),
            Some(false)
        );
        assert!(serde_json::from_value::<RelationsQuery>(serde_json::json!({"recursion_depth": 3})).is_err());
    }
}
