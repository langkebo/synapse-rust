use crate::routes::extractors::auth::AuthenticatedUser;
use crate::utils::auth::resolve_request_id;
use axum::{
    extract::{Json, State},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use synapse_common::error::MatrixErrorCode;
use synapse_common::ApiError;
use synapse_services::room::service::CreateRoomConfig;

use crate::routes::context::RoomContext;
use crate::routes::MatrixJson;

/// Parsed result of the `invite` field: user IDs and per-user reasons.
type ParsedInvites = (Vec<String>, HashMap<String, String>);

/// One entry of the `invite` array of `POST /createRoom`.
///
/// Per MSC4491 each entry may be either:
/// - a string user ID (`"@alice:example.com"`), or
/// - an object with `user_id` (required) and `reason` (optional) fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum InviteEntry {
    UserId(String),
    WithReason(InviteWithReason),
}

/// Object form of an [`InviteEntry`].
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct InviteWithReason {
    pub user_id: String,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Strongly-typed request body for `POST /createRoom`.
///
/// I-04: the handler used to deserialize into `serde_json::Value` and read each
/// field with `as_str()` / `as_bool()`, so a wrong JSON type was silently
/// dropped (`{"name":123}` created a nameless room) and unknown keys were
/// ignored. Typing the body makes both cases fail during deserialization,
/// which `MatrixJson` maps to `400 M_BAD_JSON`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateRoomBody {
    pub visibility: Option<String>,
    pub room_alias_name: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub invite: Option<Vec<InviteEntry>>,
    /// Spec-defined third-party invites. Unimplemented, so a non-empty list is
    /// rejected explicitly instead of being accepted and discarded.
    pub invite_3pid: Option<Vec<Value>>,
    pub preset: Option<String>,
    pub is_direct: Option<bool>,
    pub room_version: Option<String>,
    pub creation_content: Option<Map<String, Value>>,
    pub initial_state: Option<Vec<Value>>,
    pub power_level_content_override: Option<Map<String, Value>>,
    /// Extension: `m.space` rooms (`creation_content.type` is the
    /// spec-compliant alternative).
    pub room_type: Option<String>,
    /// Extension: front-end "create anyway" escape hatch for duplicate names.
    pub ignore_duplicate_name: Option<bool>,
}

/// Extract user IDs and per-user invite reasons from the `invite` array.
///
/// Returns `(user_ids, reasons)` where `reasons` maps user_id → reason for
/// entries that included a non-empty reason.
pub(crate) fn parse_invite_entries(invite_entries: &[InviteEntry]) -> ParsedInvites {
    let mut user_ids = Vec::with_capacity(invite_entries.len());
    let mut reasons = HashMap::new();

    for entry in invite_entries {
        let (user_id, reason) = match entry {
            InviteEntry::UserId(user_id) => (user_id.as_str(), None),
            InviteEntry::WithReason(entry) => (entry.user_id.as_str(), entry.reason.as_deref()),
        };

        user_ids.push(user_id.to_string());
        if let Some(reason) = reason.filter(|reason| !reason.is_empty()) {
            reasons.insert(user_id.to_string(), reason.to_string());
        }
    }

    (user_ids, reasons)
}

/// See [`create_private_room`].
pub(crate) async fn create_private_room(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    headers: HeaderMap,
    MatrixJson(mut body): MatrixJson<CreateRoomBody>,
) -> Result<Json<Value>, ApiError> {
    body.preset = Some("private_chat".to_string());
    body.visibility = Some("private".to_string());
    create_room(State(ctx), auth_user, headers, MatrixJson(body)).await
}

/// See [`create_room`].
pub(crate) async fn create_room(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    headers: HeaderMap,
    MatrixJson(body): MatrixJson<CreateRoomBody>,
) -> Result<Json<Value>, ApiError> {
    // S25 / WEB-01: 访客不得创建房间。此前手动 validate_token 丢弃了 is_guest
    // 标志，访客可自由建房；改用 AuthenticatedUser 提取器后同时获得审计埋点
    // （提取器在认证成功时按 POST/PUT/DELETE 自动写审计事件）。
    if auth_user.is_guest {
        return Err(ApiError::forbidden("Guests cannot create rooms".to_string()));
    }

    // 请求体类型与未知字段已由 `CreateRoomBody` + `MatrixJson` 在反序列化阶段
    // 兜住（400 M_BAD_JSON），这里只做值域校验。
    if body.invite_3pid.as_ref().is_some_and(|entries| !entries.is_empty()) {
        return Err(ApiError::invalid_param("invite_3pid is not supported; invite users by Matrix ID".to_string()));
    }

    let request_id = resolve_request_id(&headers);
    let user_id = auth_user.user_id.as_str();

    if let Some(visibility) = body.visibility.as_deref() {
        if visibility != "public" && visibility != "private" {
            return Err(ApiError::bad_request("Visibility must be 'public' or 'private'".to_string()));
        }
    }

    if let Some(alias) = body.room_alias_name.as_deref() {
        if alias.len() > 255 {
            return Err(ApiError::bad_request("Room alias name too long".to_string()));
        }
        if !alias.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.') {
            return Err(ApiError::bad_request("Invalid characters in room alias name".to_string()));
        }
    }

    let name = body.name.as_deref();
    if let Some(n) = name {
        if n.len() > 255 {
            return Err(ApiError::bad_request("Room name too long".to_string()));
        }
    }

    if let Some(t) = body.topic.as_deref() {
        if t.len() > 4096 {
            return Err(ApiError::bad_request("Room topic too long".to_string()));
        }
    }

    let (invite, invite_reasons) = match body.invite.as_deref() {
        Some(entries) => {
            let (user_ids, reasons) = parse_invite_entries(entries);
            if user_ids.len() > 100 {
                return Err(ApiError::bad_request("Too many invites (max 100)".to_string()));
            }
            (Some(user_ids), if reasons.is_empty() { None } else { Some(reasons) })
        }
        None => (None, None),
    };

    if let Some(p) = body.preset.as_deref() {
        if p != "private_chat" && p != "trusted_private_chat" && p != "public_chat" {
            return Err(ApiError::bad_request("Invalid preset value".to_string()));
        }
    }

    let room_type = body.room_type.clone().or_else(|| {
        body.creation_content.as_ref().and_then(|cc| cc.get("type")).and_then(|v| v.as_str()).map(str::to_owned)
    });

    // 同名防重（前端创建群/空间时的"确认继续"由 ignore_duplicate_name 逃生阀支持）：
    // 用户在已有同名群/空间时再次创建会产生视觉上的"重复项"，这里在服务端统一拦截。
    // - 普通房间/群：按用户已加入的同名群查重（search_rooms_for_user 只查该用户自己的房间）
    // - 空间：按同名空间查重（search_spaces 限定该用户）
    // 命中时返回 409 M_ROOM_IN_USE，前端捕获后自行查询本地同名群数量并弹窗让用户选择
    // "仍然创建"（重发时带 ignore_duplicate_name: true）。
    let ignore_duplicate = body.ignore_duplicate_name.unwrap_or(false);
    if let Some(room_name) = name.filter(|n| !n.trim().is_empty()) {
        if !ignore_duplicate {
            let is_space = room_type.as_deref() == Some("m.space");
            let has_duplicate = if is_space {
                ctx.space_service
                    .search_spaces(room_name, 20, Some(user_id))
                    .await
                    .unwrap_or_default()
                    .iter()
                    .any(|s| s.name.as_deref() == Some(room_name))
            } else {
                ctx.search_service
                    .search_rooms_for_user(user_id, room_name, 20)
                    .await
                    .unwrap_or_default()
                    .iter()
                    .any(|r| r.name.as_deref() == Some(room_name))
            };

            if has_duplicate {
                return Err(ApiError::conflict_with(
                    MatrixErrorCode::RoomInUse,
                    format!("Room name '{room_name}' is already in use"),
                ));
            }
        }
    }

    let mut creation_content = body.creation_content;
    if let Some(map) = creation_content.as_mut() {
        map.remove("creator");
        map.remove("room_version");
        map.remove("predecessor");
    }

    let config = CreateRoomConfig {
        visibility: body.visibility,
        room_alias_name: body.room_alias_name,
        name: body.name,
        topic: body.topic,
        invite_list: invite,
        invite_reasons,
        preset: body.preset,
        room_type,
        is_direct: body.is_direct,
        room_version: body.room_version,
        creation_content: creation_content.map(Value::Object),
        initial_state: body.initial_state,
        power_level_content_override: body.power_level_content_override.map(Value::Object),
        ..Default::default()
    };

    let result = ctx.room_service.lifecycle().create_room(user_id, config.clone()).await?;

    if config.room_type.as_deref() == Some("m.space") {
        let space_request = synapse_services::room::space::CreateSpaceRequest {
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

    fn parse_invites(invite: &Value) -> Result<ParsedInvites, serde_json::Error> {
        serde_json::from_value::<CreateRoomBody>(json!({ "invite": invite }))
            .map(|body| parse_invite_entries(body.invite.as_deref().unwrap_or_default()))
    }

    #[test]
    fn parse_invite_entries_accepts_plain_strings() {
        let (user_ids, reasons) = parse_invites(&json!(["@alice:example.com", "@bob:example.com"])).unwrap();
        assert_eq!(user_ids, vec!["@alice:example.com", "@bob:example.com"]);
        assert!(reasons.is_empty());
    }

    #[test]
    fn parse_invite_entries_accepts_objects_with_reason() {
        let (user_ids, reasons) = parse_invites(&json!([
            {"user_id": "@alice:example.com", "reason": "Welcome to the team!"},
            {"user_id": "@bob:example.com"}
        ]))
        .unwrap();
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
        let (user_ids, reasons) = parse_invites(&json!([
            "@alice:example.com",
            {"user_id": "@bob:example.com", "reason": "Project kick-off"}
        ]))
        .unwrap();
        assert_eq!(user_ids, vec!["@alice:example.com", "@bob:example.com"]);
        assert_eq!(reasons.get("@bob:example.com"), Some(&"Project kick-off".to_string()));
    }

    #[test]
    fn parse_invite_entries_ignores_empty_reason() {
        let (user_ids, reasons) = parse_invites(&json!([{"user_id": "@alice:example.com", "reason": ""}])).unwrap();
        assert_eq!(user_ids, vec!["@alice:example.com"]);
        assert!(reasons.is_empty(), "empty reason should not be stored");
    }

    #[test]
    fn create_room_body_rejects_non_array_invite() {
        assert!(parse_invites(&json!("@alice:example.com")).is_err());
    }

    #[test]
    fn create_room_body_rejects_invite_object_without_user_id() {
        assert!(parse_invites(&json!([{"reason": "missing user_id"}])).is_err());
    }

    #[test]
    fn create_room_body_rejects_non_string_non_object_invite_entry() {
        assert!(parse_invites(&json!([123])).is_err());
    }

    #[test]
    fn create_room_body_rejects_non_string_name() {
        let error = serde_json::from_value::<CreateRoomBody>(json!({"name": 123})).unwrap_err();
        assert!(error.to_string().contains("invalid type"), "unexpected error: {error}");
    }

    #[test]
    fn create_room_body_rejects_unknown_fields() {
        let error = serde_json::from_value::<CreateRoomBody>(json!({"name": "room", "not_a_field": true})).unwrap_err();
        assert!(error.to_string().contains("unknown field"), "unexpected error: {error}");
    }

    #[test]
    fn create_room_body_accepts_documented_fields() {
        let body: CreateRoomBody = serde_json::from_value(json!({
            "visibility": "public",
            "room_alias_name": "alias",
            "name": "room",
            "topic": "topic",
            "invite": ["@alice:example.com", {"user_id": "@bob:example.com", "reason": "hi"}],
            "preset": "public_chat",
            "is_direct": false,
            "room_version": "12",
            "creation_content": {"m.federate": false},
            "initial_state": [{"type": "m.room.topic", "state_key": "", "content": {"topic": "t"}}],
            "power_level_content_override": {"events": {"m.room.name": 50}},
            "room_type": "m.space",
            "ignore_duplicate_name": true
        }))
        .unwrap();

        assert_eq!(body.name.as_deref(), Some("room"));
        assert_eq!(body.invite.as_deref().map(<[InviteEntry]>::len), Some(2));
        assert_eq!(body.ignore_duplicate_name, Some(true));
    }
}
