use crate::common::ApiError;
use crate::web::routes::extractors::auth::AuthenticatedUser;
use crate::web::utils::auth::resolve_request_id;
use axum::{
    extract::{Json, State},
    http::HeaderMap,
};
use serde_json::Value;
use std::collections::HashMap;
use synapse_services::room::service::CreateRoomConfig;

use crate::web::routes::context::RoomContext;

/// Parsed result of the `invite` field: user IDs and per-user reasons.
type ParsedInvites = (Vec<String>, HashMap<String, String>);

/// Parse the `invite` field of `POST /createRoom`.
///
/// Per MSC4491, each entry may be either:
/// - a string user ID (`"@alice:example.com"`), or
/// - an object with `user_id` (required) and `reason` (optional) fields.
///
/// Returns `(user_ids, reasons)` where `reasons` maps user_id → reason for
/// entries that included a non-empty reason.
pub(crate) fn parse_invite_entries(invite_value: &Value) -> Result<ParsedInvites, ApiError> {
    let invites =
        invite_value.as_array().ok_or_else(|| ApiError::invalid_param("invite must be an array".to_string()))?;

    let mut user_ids = Vec::with_capacity(invites.len());
    let mut reasons = HashMap::new();

    for entry in invites {
        match entry {
            Value::String(s) => {
                user_ids.push(s.clone());
            }
            Value::Object(obj) => {
                let user_id = obj
                    .get("user_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ApiError::invalid_param("invite object must contain 'user_id'".to_string()))?;
                user_ids.push(user_id.to_string());
                if let Some(reason) = obj.get("reason").and_then(|v| v.as_str()) {
                    if !reason.is_empty() {
                        reasons.insert(user_id.to_string(), reason.to_string());
                    }
                }
            }
            _ => {
                return Err(ApiError::invalid_param(
                    "invite entries must be strings or objects with 'user_id'".to_string(),
                ));
            }
        }
    }

    Ok((user_ids, reasons))
}

pub(crate) async fn create_private_room(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    headers: HeaderMap,
    Json(mut body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    body["preset"] = serde_json::Value::String("private_chat".to_string());
    body["visibility"] = serde_json::Value::String("private".to_string());
    create_room(State(ctx), auth_user, headers, Json(body)).await
}

pub(crate) async fn create_room(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    // S25 / WEB-01: 访客不得创建房间。此前手动 validate_token 丢弃了 is_guest
    // 标志，访客可自由建房；改用 AuthenticatedUser 提取器后同时获得审计埋点
    // （提取器在认证成功时按 POST/PUT/DELETE 自动写审计事件）。
    if auth_user.is_guest {
        return Err(ApiError::forbidden("Guests cannot create rooms".to_string()));
    }
    let request_id = resolve_request_id(&headers);
    let user_id = auth_user.user_id.as_str();

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

    let (invite, invite_reasons) = match body.get("invite") {
        Some(value) => {
            let (user_ids, reasons) = parse_invite_entries(value)?;
            if user_ids.len() > 100 {
                return Err(ApiError::bad_request("Too many invites (max 100)".to_string()));
            }
            (Some(user_ids), if reasons.is_empty() { None } else { Some(reasons) })
        }
        None => (None, None),
    };

    let preset = body.get("preset").and_then(|v| v.as_str());
    if let Some(p) = preset {
        if p != "private_chat" && p != "trusted_private_chat" && p != "public_chat" {
            return Err(ApiError::bad_request("Invalid preset value".to_string()));
        }
    }

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
        invite_reasons,
        preset: preset.map(|s| s.to_string()),
        room_type: room_type.map(|s| s.to_string()),
        is_direct,
        room_version,
        creation_content,
        initial_state,
        power_level_content_override,
        ..Default::default()
    };

    let result = ctx.room_service.lifecycle().create_room(user_id, config.clone()).await?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_invite_entries_accepts_plain_strings() {
        let value = json!(["@alice:example.com", "@bob:example.com"]);
        let (user_ids, reasons) = parse_invite_entries(&value).unwrap();
        assert_eq!(user_ids, vec!["@alice:example.com", "@bob:example.com"]);
        assert!(reasons.is_empty());
    }

    #[test]
    fn parse_invite_entries_accepts_objects_with_reason() {
        let value = json!([
            {"user_id": "@alice:example.com", "reason": "Welcome to the team!"},
            {"user_id": "@bob:example.com"}
        ]);
        let (user_ids, reasons) = parse_invite_entries(&value).unwrap();
        assert_eq!(user_ids, vec!["@alice:example.com", "@bob:example.com"]);
        assert_eq!(
            reasons.get("@alice:example.com"),
            Some(&"Welcome to the team!".to_string()),
            "reason should be captured for alice"
        );
        assert!(!reasons.contains_key("@bob:example.com"), "no reason should be captured for bob");
    }

    #[test]
    fn parse_invite_entries_accepts_mixed_string_and_object_entries() {
        let value = json!([
            "@alice:example.com",
            {"user_id": "@bob:example.com", "reason": "Project kick-off"}
        ]);
        let (user_ids, reasons) = parse_invite_entries(&value).unwrap();
        assert_eq!(user_ids, vec!["@alice:example.com", "@bob:example.com"]);
        assert_eq!(reasons.get("@bob:example.com"), Some(&"Project kick-off".to_string()));
    }

    #[test]
    fn parse_invite_entries_ignores_empty_reason() {
        let value = json!([{"user_id": "@alice:example.com", "reason": ""}]);
        let (user_ids, reasons) = parse_invite_entries(&value).unwrap();
        assert_eq!(user_ids, vec!["@alice:example.com"]);
        assert!(reasons.is_empty(), "empty reason should not be stored");
    }

    #[test]
    fn parse_invite_entries_rejects_non_array() {
        let value = json!("@alice:example.com");
        assert!(parse_invite_entries(&value).is_err());
    }

    #[test]
    fn parse_invite_entries_rejects_object_without_user_id() {
        let value = json!([{"reason": "missing user_id"}]);
        assert!(parse_invite_entries(&value).is_err());
    }

    #[test]
    fn parse_invite_entries_rejects_non_string_non_object_entry() {
        let value = json!([123]);
        assert!(parse_invite_entries(&value).is_err());
    }
}
