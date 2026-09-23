use crate::routes::context::RoomContext;
use axum::{
    extract::{Path, State},
    routing::put,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use synapse_common::current_timestamp_millis;

use crate::routes::room_access::ensure_room_member_ctx;
use crate::routes::{AppState, AuthenticatedUser};
use synapse_common::error::ApiError;

fn create_reactions_compat_router() -> Router<AppState> {
    Router::new().route("/rooms/{room_id}/send/m.reaction/{txn_id}", put(add_reaction))
}

/// See [`create_reactions_router`].
pub fn create_reactions_router(state: AppState) -> Router<AppState> {
    let compat_router = create_reactions_compat_router();

    Router::new().nest("/_matrix/client/v3", compat_router).with_state(state)
}

/// The `RelatesTo` struct.
#[derive(Debug, Deserialize)]
pub struct RelatesTo {
    /// The `event_id` field.
    pub event_id: String,
    #[serde(rename = "rel_type")]
    /// The `rel_type` field.
    pub rel_type: String,
    #[serde(default)]
    /// The `is_falling_back` field.
    pub is_falling_back: Option<bool>,
}

/// The `ReactionResponse` struct.
#[derive(Debug, Serialize)]
pub struct ReactionResponse {
    /// The `event_id` field.
    pub event_id: String,
}

/// 添加 reaction 到事件 (m.annotation)
async fn add_reaction(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, _txn_id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ReactionResponse>, ApiError> {
    // 验证房间存在
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    // Authorization, not just existence.
    //
    // `room_exists` is not an access check: without these two the endpoint let
    // ANY authenticated user inject `m.annotation` events into ANY existing room
    // (including private ones they are not in) — cross-room write / spam.
    // `ensure_room_member_ctx` is the same membership gate the relations read
    // path uses, and `verify_message_event_write` resolves the joined user's
    // power level against the room's `events["m.reaction"]`/`events_default`
    // (a non-member gets -1, so it is fail-closed on its own too).
    ensure_room_member_ctx(&ctx, &auth_user, &room_id, "You must be a room member to react").await?;
    ctx.room_auth.verify_message_event_write(&room_id, &auth_user.user_id, "m.reaction").await?;

    // 提取 relates_to 信息
    let relates_to_value = body.get("m.relates_to").or_else(|| body.get("relates_to"));
    let relates_to = relates_to_value
        .and_then(|v| serde_json::from_value::<RelatesTo>(v.clone()).ok())
        .ok_or_else(|| ApiError::bad_request("Missing m.relates_to".to_string()))?;

    // 验证 rel_type 是 annotation (reaction)
    if relates_to.rel_type != "m.annotation" {
        return Err(ApiError::bad_request("rel_type must be m.annotation for reactions".to_string()));
    }

    // 提取 reaction 内容 (emoji)
    let annotation = body.get("body").and_then(|v| v.as_str()).unwrap_or("👍").to_string();

    let origin_server_ts = current_timestamp_millis();
    let relation = ctx
        .relations_service
        .send_annotation(synapse_services::relations_service::SendAnnotationRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.event_id.clone(),
            sender: auth_user.user_id.clone(),
            key: annotation.clone(),
            origin_server_ts,
        })
        .await?;

    tracing::info!(
        "User {} added reaction {} to event {} in room {}",
        auth_user.user_id,
        annotation,
        relates_to.event_id,
        room_id
    );

    Ok(Json(ReactionResponse { event_id: relation.event_id }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::assembly::declared_ledger_all;
    use crate::routes::route_ledger::RouteEntry;
    use axum::http::Method;

    /// Helper: extract reactions routes from the derived route ledger.
    ///
    /// This mirrors the pattern used in burn_after_read_route_tests.rs — instead
    /// of hardcoding expected paths as strings, we filter the actual derived
    /// route table by `registered_by == "reactions"`.
    fn reactions_route_manifest() -> Vec<RouteEntry> {
        declared_ledger_all().iter().filter(|e| e.registered_by == "reactions").cloned().collect()
    }

    #[test]
    fn test_relates_to_parse() {
        let json = r#"{
            "event_id": "$test_event",
            "rel_type": "m.annotation"
        }"#;
        let relates: RelatesTo = serde_json::from_str(json).expect("Failed to parse RelatesTo JSON");
        assert_eq!(relates.event_id, "$test_event");
        assert_eq!(relates.rel_type, "m.annotation");
    }

    /// High-standard router structure test: verify the real derived route
    /// manifest contains exactly one reactions endpoint with the correct method.
    ///
    /// Unlike the original weak test that only checked hardcoded string arrays,
    /// this validates against the actual route table generated from the real
    /// router assembly.
    #[test]
    fn test_reactions_routes_structure_from_real_ledger() {
        let manifest = reactions_route_manifest();

        // Reactions only has one logical endpoint: PUT /rooms/{room_id}/send/m.reaction/{txn_id}
        assert_eq!(
            manifest.len(),
            1,
            "reactions manifest must declare exactly 1 (method, path) entry, got {}",
            manifest.len()
        );

        let entry = &manifest[0];
        assert_eq!(entry.method, Method::PUT, "reactions endpoint must be PUT");
        assert_eq!(
            entry.path, "/_matrix/client/v3/rooms/{room_id}/send/m.reaction/{txn_id}",
            "reactions path mismatch"
        );
        assert_eq!(entry.registered_by, "reactions", "registered_by must be 'reactions'");
    }

    /// Verify all reactions routes use the v3 client prefix and m.reaction event type.
    #[test]
    fn test_reactions_routes_use_v3_prefix_and_annotation_type() {
        let manifest = reactions_route_manifest();

        assert!(
            manifest.iter().all(|e| e.path.starts_with("/_matrix/client/v3/")),
            "all reactions routes must use /_matrix/client/v3/ prefix"
        );

        assert!(
            manifest.iter().all(|e| e.path.contains("/send/m.reaction/")),
            "all reactions routes must use m.reaction event type"
        );
    }

    /// Verify reactions router contains only write endpoints (no read/annotation queries).
    ///
    /// The reactions module is write-only (sending annotations); read operations
    /// live under the relations module (/relations or /annotations paths).
    #[test]
    fn test_reactions_router_contains_only_write_endpoints() {
        let manifest = reactions_route_manifest();

        assert!(
            manifest.iter().all(|e| !e.path.contains("/relations/") && !e.path.contains("/annotations/")),
            "reactions router must not contain read endpoints (/relations/ or /annotations/)"
        );

        // All entries should be PUT (write) methods
        assert!(
            manifest.iter().all(|e| e.method == Method::PUT),
            "reactions router must only contain PUT (write) endpoints"
        );
    }

    /// Verify txn_id path parameter exists for idempotency.
    #[test]
    fn test_reactions_route_exposes_txn_id_parameter() {
        let manifest = reactions_route_manifest();

        assert!(
            manifest.iter().all(|e| e.path.contains("{txn_id}")),
            "reactions route must expose txn_id path parameter for idempotency"
        );
    }
}
