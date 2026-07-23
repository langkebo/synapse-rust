use crate::common::ApiError;
use crate::web::utils::auth::{bearer_token, resolve_request_id};
use axum::{
    extract::{Json, State},
    http::HeaderMap,
};
use serde_json::Value;
use synapse_services::room::service::CreateRoomConfig;

use crate::web::routes::context::RoomContext;

pub(crate) async fn create_private_room(
    State(ctx): State<RoomContext>,
    headers: HeaderMap,
    Json(mut body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    body["preset"] = serde_json::Value::String("private_chat".to_string());
    body["visibility"] = serde_json::Value::String("private".to_string());
    create_room(State(ctx), headers, Json(body)).await
}

pub(crate) async fn create_room(
    State(ctx): State<RoomContext>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    let token = bearer_token(&headers)?;
    let (user_id, _, _, _, _) = ctx.token_auth.validate_token(&token).await?;

    let visibility = body.get("visibility").and_then(|v| v.as_str());
    if let Some(v) = visibility {
        if v != "public" && v != "private" {
            return Err(ApiError::bad_request("Visibility must be 'public' or 'private'".to_string()));
        }
    }

    let room_alias = body.get("room_alias_name").and_then(|v| v.as_str());
    if let Some(alias) = room_alias {
        if alias.len() > 255 {
            return Err(ApiError::bad_request("Room alias name too long".to_string()));
        }
        if !alias.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.') {
            return Err(ApiError::bad_request("Invalid characters in room alias name".to_string()));
        }
    }

    let name = body.get("name").and_then(|v| v.as_str());
    if let Some(n) = name {
        if n.len() > 255 {
            return Err(ApiError::bad_request("Room name too long".to_string()));
        }
    }

    let topic = body.get("topic").and_then(|v| v.as_str());
    if let Some(t) = topic {
        if t.len() > 4096 {
            return Err(ApiError::bad_request("Room topic too long".to_string()));
        }
    }

    let invite = match body.get("invite") {
        Some(value) => {
            let Some(invites) = value.as_array() else {
                return Err(ApiError::invalid_param("invite must be an array".to_string()));
            };
            let mut invitees = Vec::with_capacity(invites.len());
            for invitee in invites {
                let Some(user_id) = invitee.as_str() else {
                    return Err(ApiError::invalid_param("invite entries must be strings".to_string()));
                };
                invitees.push(user_id.to_string());
            }
            Some(invitees)
        }
        None => None,
    };

    if let Some(ref inv) = invite {
        if inv.len() > 100 {
            return Err(ApiError::bad_request("Too many invites (max 100)".to_string()));
        }
    }

    let preset = body.get("preset").and_then(|v| v.as_str());

    let room_type = body
        .get("room_type")
        .and_then(|v| v.as_str())
        .or_else(|| body.get("creation_content").and_then(|cc| cc.get("type")).and_then(|v| v.as_str()));

    let is_direct = body.get("is_direct").and_then(|v| v.as_bool());
    let room_version = body.get("room_version").and_then(|v| v.as_str()).map(str::to_owned);
    let mut creation_content = body.get("creation_content").cloned();
    if let Some(map) = creation_content.as_mut().and_then(|value| value.as_object_mut()) {
        map.remove("creator");
        map.remove("room_version");
        map.remove("predecessor");
    }
    let initial_state = body.get("initial_state").and_then(|v| v.as_array()).cloned();
    let power_level_content_override = body.get("power_level_content_override").cloned();

    let config = CreateRoomConfig {
        visibility: visibility.map(|s| s.to_string()),
        room_alias_name: room_alias.map(|s| s.to_string()),
        name: name.map(|s| s.to_string()),
        topic: topic.map(|s| s.to_string()),
        invite_list: invite,
        preset: preset.map(|s| s.to_string()),
        room_type: room_type.map(|s| s.to_string()),
        is_direct,
        room_version,
        creation_content,
        initial_state,
        power_level_content_override,
        ..Default::default()
    };

    let result = ctx.room_service.lifecycle().create_room(&user_id, config.clone()).await?;

    if config.room_type.as_deref() == Some("m.space") {
        let space_request = synapse_storage::space::CreateSpaceRequest {
            room_id: result.get("room_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            name: config.name.clone(),
            topic: config.topic.clone(),
            avatar_url: None,
            creator: user_id.to_string(),
            join_rule: config.preset.clone(),
            visibility: config.visibility.clone(),
            is_public: config.visibility.as_ref().map(|v| v == "public"),
            parent_space_id: None,
        };
        if let Err(e) = ctx.space_service.create_space(space_request).await {
            ::tracing::error!(
                request_id = %request_id,
                user_id = %user_id,
                room_type = ?config.room_type,
                error = %e,
                "Failed to create space record"
            );
        }
    }

    Ok(Json(result))
}
