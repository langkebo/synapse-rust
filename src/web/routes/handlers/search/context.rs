use crate::common::ApiError;
use crate::web::routes::context::RoomContext;
use crate::web::routes::{ensure_room_member_strict_ctx, AuthenticatedUser};
use axum::extract::{Json, Path, Query, State};
use serde_json::{json, Value};
use synapse_services::search_service::TimestampDirection;

use std::collections::HashMap;

pub(crate) async fn get_event_context(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let room_id = room_id.replace("%21", "!").replace("%3A", ":");
    let event_id = event_id.replace("%24", "$");

    crate::web::routes::validate_room_id(&room_id)?;
    crate::web::routes::validate_event_id(&event_id)?;

    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(10).clamp(1, 100);

    ensure_room_member_strict_ctx(&ctx, &auth_user, &room_id, "Not a member of this room").await?;

    let target_event = ctx.room_service.messaging().get_event(&room_id, &event_id).await?;

    let target_ts = target_event.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(0);

    let context_window = ctx.search_service.get_event_context_window(&room_id, target_ts, limit as i64).await?;

    let events_before_list: Vec<Value> = context_window
        .events_before
        .iter()
        .map(|event| {
            json!({
                "event_id": event.event_id,
                "sender": event.sender,
                "type": event.event_type,
                "content": event.content.clone(),
                "origin_server_ts": event.origin_server_ts
            })
        })
        .collect();

    let events_after_list: Vec<Value> = context_window
        .events_after
        .iter()
        .map(|event| {
            json!({
                "event_id": event.event_id,
                "sender": event.sender,
                "type": event.event_type,
                "content": event.content.clone(),
                "origin_server_ts": event.origin_server_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "event": {
            "event_id": target_event.get("event_id").and_then(|v| v.as_str()).unwrap_or(""),
            "sender": target_event.get("sender").and_then(|v| v.as_str()).unwrap_or(""),
            "type": target_event.get("type").and_then(|v| v.as_str()).unwrap_or(""),
            "content": target_event.get("content"),
            "origin_server_ts": target_ts
        },
        "events_before": events_before_list,
        "events_after": events_after_list,
        "state": [],
        "start": events_before_list.last().and_then(|e| e.get("event_id").and_then(|v| v.as_str())).unwrap_or(&event_id),
        "end": events_after_list.last().and_then(|e| e.get("event_id").and_then(|v| v.as_str())).unwrap_or(&event_id)
    })))
}

pub(crate) async fn timestamp_to_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let ts: i64 = params
        .get("ts")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ApiError::bad_request("Missing ts parameter".to_string()))?;

    let dir = params.get("dir").map_or("f", |v| v.as_str());

    ensure_room_member_strict_ctx(&ctx, &auth_user, &room_id, "Not a member of this room").await?;

    let direction = if dir == "b" { TimestampDirection::Backward } else { TimestampDirection::Forward };

    let event = ctx.search_service.find_event_by_timestamp(&room_id, ts, direction).await?;

    match event {
        Some(event) => Ok(Json(json!({
            "event_id": event.event_id,
            "origin_server_ts": event.origin_server_ts
        }))),
        None => Err(ApiError::not_found("No event found at this timestamp".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn test_context_response_structure() {
        let response = json!({
            "event": {},
            "events_before": [],
            "events_after": [],
            "state": [],
            "start": "start_token",
            "end": "end_token"
        });

        assert!(response.get("event").is_some());
        assert!(response.get("events_before").is_some());
        assert!(response.get("events_after").is_some());
    }
}
