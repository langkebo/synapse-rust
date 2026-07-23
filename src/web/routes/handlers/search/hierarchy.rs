use crate::common::ApiError;
use crate::web::routes::context::RoomContext;
use crate::web::routes::{validate_room_id, AuthenticatedUser};
use axum::extract::{Json, Path, Query, State};
use serde_json::{json, Value};

use std::collections::HashMap;

pub(crate) async fn build_room_hierarchy_response(
    ctx: &RoomContext,
    room_id: &str,
    user_id: &str,
    max_depth: i32,
    suggested_only: bool,
    limit: i32,
    from: Option<&str>,
) -> Result<Value, ApiError> {
    let room_opt = ctx
        .room_service
        .state()
        .get_room_record(room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to load room", &e))?;

    if let Some(space) = ctx.space_service.get_space_by_room(room_id).await? {
        let response = ctx
            .space_service
            .get_space_hierarchy_v1(
                &space.space_id,
                max_depth.max(1),
                suggested_only,
                Some(limit.max(1)),
                from,
                Some(user_id),
            )
            .await?;

        let mut response_value = serde_json::to_value(response)
            .map_err(|e| ApiError::internal_with_log("Failed to serialize hierarchy", &e))?;

        if let Some(obj) = response_value.as_object_mut() {
            let rooms = obj.get("rooms").and_then(|r| r.as_array()).map_or(0, |a| a.len());
            let has_space_self = obj
                .get("rooms")
                .and_then(|r| r.as_array())
                .is_some_and(|a| a.iter().any(|r| r.get("room_id").and_then(|v| v.as_str()) == Some(room_id)));

            if !has_space_self || rooms <= 1 {
                let state_events = ctx.room_service.messaging().get_state_events(room_id).await?;

                let mut children_state = Vec::new();
                let mut child_room_ids = Vec::new();

                for ev in &state_events {
                    if ev.get("type").and_then(|v| v.as_str()) == Some("m.space.child") {
                        let via = ev
                            .get("content")
                            .and_then(|c| c.get("via"))
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>())
                            .unwrap_or_default();
                        if !via.is_empty() {
                            children_state.push(json!({
                                "type": "m.space.child",
                                "state_key": ev.get("state_key"),
                                "content": ev.get("content"),
                                "origin_server_ts": ev.get("origin_server_ts"),
                            }));

                            if max_depth > 0 {
                                if let Some(sk) = ev.get("state_key").and_then(|v| v.as_str()) {
                                    child_room_ids.push(sk.to_string());
                                }
                            }
                        }
                    }
                }

                let child_rooms_map = ctx.room_service.collect_child_rooms(&child_room_ids).await?;

                if !child_rooms_map.is_empty() || !has_space_self {
                    let space_room_type = state_events
                        .iter()
                        .find(|e| e.get("type").and_then(|v| v.as_str()) == Some("m.room.create"))
                        .and_then(|e| e.get("content"))
                        .and_then(|c| c.get("type"))
                        .and_then(|v| v.as_str())
                        .map_or(Value::Null, |s| Value::String(s.to_string()));

                    if let Some(rooms_arr) = obj.get_mut("rooms").and_then(|r| r.as_array_mut()) {
                        if !has_space_self {
                            if let Some(ref r) = room_opt {
                                rooms_arr.insert(0, json!({
                                    "room_id": r.room_id,
                                    "name": r.name,
                                    "topic": r.topic,
                                    "avatar_url": r.avatar_url,
                                    "join_rule": r.join_rule,
                                    "guest_access": if r.is_public { "can_join" } else { "forbidden" },
                                    "guest_can_join": r.is_public,
                                    "world_readable": r.history_visibility == "world_readable",
                                    "num_joined_members": r.member_count,
                                    "children": child_rooms_map.iter().filter_map(|r| r.get("room_id").and_then(|v| v.as_str()).map(String::from)).collect::<Vec<_>>(),
                                    "children_state": children_state,
                                    "room_type": space_room_type,
                                }));
                            }
                        } else if let Some(first) = rooms_arr.first_mut() {
                            if let Some(first_obj) = first.as_object_mut() {
                                first_obj.insert("children_state".to_string(), json!(children_state));
                                first_obj.insert(
                                    "children".to_string(),
                                    json!(child_rooms_map
                                        .iter()
                                        .filter_map(|r| r.get("room_id").and_then(|v| v.as_str()).map(String::from))
                                        .collect::<Vec<_>>()),
                                );
                            }
                        }
                        rooms_arr.extend(child_rooms_map);
                    }
                }
            }
        }

        return Ok(response_value);
    }

    let room = room_opt.ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let world_readable = room.history_visibility == "world_readable";

    let state_events = ctx.room_service.messaging().get_state_events(room_id).await?;

    let room_type = state_events
        .iter()
        .find(|e| e.get("type").and_then(|v| v.as_str()) == Some("m.room.create"))
        .and_then(|e| e.get("content"))
        .and_then(|c| c.get("type"))
        .and_then(|v| v.as_str())
        .map_or(Value::Null, |s| Value::String(s.to_string()));

    let mut children_state = Vec::new();
    let mut child_room_ids = Vec::new();

    for ev in &state_events {
        if ev.get("type").and_then(|v| v.as_str()) == Some("m.space.child") {
            let via = ev
                .get("content")
                .and_then(|c| c.get("via"))
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>())
                .unwrap_or_default();
            if !via.is_empty() {
                children_state.push(json!({
                    "type": "m.space.child",
                    "state_key": ev.get("state_key"),
                    "content": ev.get("content"),
                    "origin_server_ts": ev.get("origin_server_ts"),
                }));

                if max_depth > 0 {
                    if let Some(sk) = ev.get("state_key").and_then(|v| v.as_str()) {
                        child_room_ids.push(sk.to_string());
                    }
                }
            }
        }
    }

    let child_rooms = ctx.room_service.collect_child_rooms(&child_room_ids).await?;

    let mut rooms = vec![json!({
        "room_id": room.room_id,
        "name": room.name,
        "topic": room.topic,
        "avatar_url": room.avatar_url,
        "join_rule": room.join_rule,
        "guest_access": if room.is_public { "can_join" } else { "forbidden" },
        "guest_can_join": room.is_public,
        "world_readable": world_readable,
        "num_joined_members": room.member_count,
        "children": child_rooms.iter().filter_map(|r| r.get("room_id").and_then(|v| v.as_str()).map(String::from)).collect::<Vec<_>>(),
        "children_state": children_state,
        "room_type": room_type,
        "required_state_info": []
    })];
    rooms.extend(child_rooms);

    Ok(json!({
        "rooms": rooms,
        "next_batch": Value::Null
    }))
}

pub(crate) async fn get_room_hierarchy(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50);

    let max_depth = params.get("max_depth").and_then(|v| v.parse().ok()).unwrap_or(3);

    let mut response =
        build_room_hierarchy_response(&ctx, &room_id, &auth_user.user_id, max_depth, false, limit, None).await?;

    if let Some(object) = response.as_object_mut() {
        object.insert("max_depth".to_string(), json!(max_depth));
    }

    Ok(Json(response))
}

/// GET /_matrix/client/v3/rooms/{room_id}/hierarchy
/// Returns a list of child rooms and spaces of a given room
pub(crate) async fn get_room_hierarchy_v3(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50);

    let max_depth = params.get("max_depth").and_then(|v| v.parse().ok()).unwrap_or(1);

    let suggested_only = params.get("suggested_only").is_some_and(|v| v == "true");

    let mut response = build_room_hierarchy_response(
        &ctx,
        &room_id,
        &auth_user.user_id,
        max_depth,
        suggested_only,
        limit,
        params.get("from").map(|value| value.as_str()),
    )
    .await?;

    if let Some(object) = response.as_object_mut() {
        object.insert("max_depth".to_string(), json!(max_depth));
    }

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn test_hierarchy_response_structure() {
        let response = json!({
            "rooms": [],
            "next_batch": null
        });

        assert!(response.get("rooms").is_some());
    }
}
