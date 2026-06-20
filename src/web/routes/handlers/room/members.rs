use super::ensure_room_view_access;
use crate::common::ApiError;
use crate::web::routes::{
    extract_token_from_headers, is_joined_room_member, is_joined_room_member_or_creator, validate_membership,
    validate_room_id, validate_user_id, AppState, AuthenticatedUser,
};
use crate::web::utils::auth::resolve_request_id;
use axum::{
    extract::{Json, Path, State},
    http::HeaderMap,
};
use serde_json::{json, Value};

pub(crate) async fn join_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _, _, _) = state.services.core.auth_service.validate_token(&token).await?;

    state.services.rooms.room_service.join_room(&room_id, &user_id).await?;
    Ok(Json(json!({
        "room_id": room_id,
        "joined_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn join_room_by_id_or_alias(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id_or_alias): Path<String>,
    body: Option<Json<serde_json::Value>>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _, _, _) = state.services.core.auth_service.validate_token(&token).await?;

    let room_id = if room_id_or_alias.starts_with('!') {
        validate_room_id(&room_id_or_alias)?;
        room_id_or_alias.clone()
    } else if room_id_or_alias.starts_with('#') {
        state
            .services
            .rooms
            .room_service
            .get_room_by_alias(&room_id_or_alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {e}")))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    } else {
        let alias = format!("#{}:{}", room_id_or_alias, state.services.core.server_name);
        state
            .services
            .rooms
            .room_service
            .get_room_by_alias(&alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {e}")))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    };

    let via_servers = body.and_then(|b| b.get("via_servers").and_then(|v| v.as_array()).cloned()).unwrap_or_default();

    ::tracing::info!(
        request_id = %request_id,
        user_id = %user_id,
        room_id = %room_id,
        via_servers = ?via_servers,
        "User joining room by id or alias"
    );

    state.services.rooms.room_service.join_room(&room_id, &user_id).await?;

    Ok(Json(json!({
        "room_id": room_id
    })))
}

pub(crate) async fn leave_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    validate_room_id(&room_id)?;
    state.services.rooms.room_service.leave_room(&room_id, &auth_user.user_id).await?;
    #[cfg(feature = "friends")]
    if let Err(error) = state
        .services
        .extensions
        .friend_room_service
        .sync_dm_room_membership_change(&room_id, &auth_user.user_id, "left", Some(&auth_user.user_id), None)
        .await
    {
        ::tracing::warn!(
            request_id = %request_id,
            room_id = %room_id,
            user_id = %auth_user.user_id,
            error = %error,
            "Failed to sync friend DM leave state"
        );
    }
    Ok(Json(json!({})))
}

pub(crate) async fn knock_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id_or_alias): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _, _, _) = state.services.core.auth_service.validate_token(&token).await?;

    let room_id = if room_id_or_alias.starts_with('!') {
        validate_room_id(&room_id_or_alias)?;
        room_id_or_alias.clone()
    } else if room_id_or_alias.starts_with('#') {
        state
            .services
            .rooms
            .room_service
            .get_room_by_alias(&room_id_or_alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {e}")))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    } else {
        let alias = format!("#{}:{}", room_id_or_alias, state.services.core.server_name);
        state
            .services
            .rooms
            .room_service
            .get_room_by_alias(&alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {e}")))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    };

    let reason = body.get("reason").and_then(|v| v.as_str()).map(|s| s.to_string());

    ::tracing::info!(
        request_id = %request_id,
        user_id = %user_id,
        room_id = %room_id,
        "User knocking on room"
    );

    state.services.rooms.room_service.knock_room(&room_id, &user_id, reason.as_deref()).await?;

    Ok(Json(json!({
        "room_id": room_id
    })))
}

pub(crate) async fn invite_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let invitee = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(invitee)?;

    state.services.core.auth_service.can_invite_user(&room_id, &auth_user.user_id).await?;

    state.services.rooms.room_service.invite_user(&room_id, &auth_user.user_id, invitee).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "invited_user_id": invitee,
        "invited_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn invite_user_by_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _, _, _) = state.services.core.auth_service.validate_token(&token).await?;

    validate_room_id(&room_id)?;

    let invitee = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(invitee)?;

    state.services.core.auth_service.can_invite_user(&room_id, &user_id).await?;

    ::tracing::info!(
        request_id = %request_id,
        user_id = %user_id,
        invitee = %invitee,
        room_id = %room_id,
        "User inviting another user to room"
    );

    state.services.rooms.room_service.invite_user(&room_id, &user_id, invitee).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "invited_user_id": invitee,
        "invited_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn get_room_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    let request_id = resolve_request_id(&headers);

    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _, _, _) = state.services.core.auth_service.validate_token(&token).await?;

    let room = state.services.rooms.room_service.get_room(&room_id).await?;

    let is_member =
        is_joined_room_member_or_creator(&state, &user_id, &room_id, room.get("creator").and_then(|v| v.as_str()))
            .await?;

    if !room.get("is_public").and_then(|v| v.as_bool()).unwrap_or(false) && !is_member {
        ::tracing::warn!(
            target: "security_audit",
            request_id = %request_id,
            event = "unauthorized_room_members_access",
            user_id = %user_id,
            room_id = %room_id,
            "User attempted to access members of private room without being a member"
        );
        return Err(ApiError::forbidden(
            "You must be a member to view the member list of this private room".to_string(),
        ));
    }

    let membership_filter = params.get("membership").map(|s| s.as_str());
    let not_membership_filter = params.get("not_membership").map(|s| s.as_str());

    if let Some(mf) = membership_filter {
        validate_membership(mf)?;
    }
    if let Some(nmf) = not_membership_filter {
        validate_membership(nmf)?;
    }

    let members = state.services.rooms.room_service.get_room_members(&room_id, &user_id).await?;

    let filtered = if membership_filter.is_some() || not_membership_filter.is_some() {
        if let Some(chunk) = members.get("chunk").and_then(|c| c.as_array()) {
            let filtered_events: Vec<Value> = chunk
                .iter()
                .filter(|event| {
                    let event_membership =
                        event.get("content").and_then(|c| c.get("membership")).and_then(|m| m.as_str()).unwrap_or("");

                    if let Some(mf) = membership_filter {
                        event_membership == mf
                    } else {
                        true
                    }
                })
                .filter(|event| {
                    let event_membership =
                        event.get("content").and_then(|c| c.get("membership")).and_then(|m| m.as_str()).unwrap_or("");

                    if let Some(nmf) = not_membership_filter {
                        event_membership != nmf
                    } else {
                        true
                    }
                })
                .cloned()
                .collect();

            let mut result = members.clone();
            result["chunk"] = Value::Array(filtered_events);
            result
        } else {
            members
        }
    } else {
        members
    };

    Ok(Json(filtered))
}

pub(crate) async fn get_room_members_recent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _, _, _) = state.services.core.auth_service.validate_token(&token).await?;
    let members = state.services.rooms.room_service.get_room_members(&room_id, &user_id).await?;

    let from = params.get("from").and_then(|value| value.parse::<usize>().ok()).unwrap_or(0);
    let limit = params.get("limit").and_then(|value| value.parse::<usize>().ok()).unwrap_or(100).min(1000);

    let chunk = members.get("chunk").and_then(|value| value.as_array()).cloned().unwrap_or_default();

    let end_index = std::cmp::min(from.saturating_add(limit), chunk.len());
    let sliced_chunk = if from < chunk.len() { chunk[from..end_index].to_vec() } else { Vec::new() };

    Ok(Json(json!({
        "chunk": sliced_chunk,
        "start": from.to_string(),
        "end": end_index.to_string()
    })))
}

pub(crate) async fn get_joined_members(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room = state.services.rooms.room_service.get_room(&room_id).await?;

    let is_member = is_joined_room_member(&state, &auth_user.user_id, &room_id).await?;

    if !room.get("is_public").and_then(|v| v.as_bool()).unwrap_or(false) && !is_member {
        return Err(ApiError::forbidden(
            "You must be a member to view the joined members of this private room".to_string(),
        ));
    }

    let members = state.services.rooms.room_service.get_joined_members_with_profiles(&room_id).await?;

    let joined: std::collections::HashMap<String, Value> = members
        .into_iter()
        .map(|m| {
            let user_id = m.user_id.clone();
            let display_name = m.display_name.clone().or_else(|| {
                let uid = &user_id;
                uid.strip_prefix('@').and_then(|s| s.split(':').next()).map(|s| s.to_string())
            });
            (
                user_id,
                json!({
                    "display_name": display_name,
                    "avatar_url": m.avatar_url
                }),
            )
        })
        .collect();

    Ok(Json(json!({
        "joined": joined
    })))
}

pub(crate) async fn get_room_membership(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, target_user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_user_id(&target_user_id)?;

    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let membership = state
        .services
        .rooms
        .room_service
        .get_room_member_record(&room_id, &target_user_id)
        .await?
        .map_or_else(|| "leave".to_string(), |m| m.membership);

    Ok(Json(json!({
        "membership": membership
    })))
}

pub(crate) async fn get_membership_events(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(100).min(1000) as i64;

    let memberships = state.services.rooms.room_service.get_membership_history(&room_id, limit).await?;

    let events: Vec<Value> = memberships
        .into_iter()
        .map(|m| {
            json!({
                "event_id": m.event_id,
                "type": m.event_type,
                "sender": m.sender,
                "state_key": m.user_id,
                "content": {
                    "membership": m.membership
                },
                "origin_server_ts": m.joined_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "events": events
    })))
}

pub(crate) async fn get_room_invites(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let _invites = state.services.rooms.room_service.get_joined_members_with_profiles(&room_id).await?;

    let invited_members: Vec<serde_json::Value> = state
        .services
        .rooms
        .room_service
        .get_room_members_by_membership(&room_id, "invite")
        .await?
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "user_id": m.user_id,
                "sender": m.sender,
                "display_name": m.display_name,
                "avatar_url": m.avatar_url,
                "event_id": m.event_id,
                "reason": m.reason,
                "updated_ts": m.updated_ts
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "invites": invited_members,
        "total": invited_members.len()
    })))
}

pub(crate) async fn kick_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    validate_room_id(&room_id)?;

    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(target)?;

    let reason = body.get("reason").and_then(|v| v.as_str());
    if let Some(r) = reason {
        if r.len() > 512 {
            return Err(ApiError::bad_request("Reason too long".to_string()));
        }
    }

    state.services.rooms.room_service.kick_user(&room_id, target, &auth_user.user_id, reason).await?;

    #[cfg(feature = "friends")]
    if let Err(error) = state
        .services
        .extensions
        .friend_room_service
        .sync_dm_room_membership_change(&room_id, target, "kicked", Some(&auth_user.user_id), reason)
        .await
    {
        ::tracing::warn!(
            request_id = %request_id,
            room_id = %room_id,
            user_id = %auth_user.user_id,
            target_user_id = %target,
            error = %error,
            "Failed to sync friend DM kick state"
        );
    }

    Ok(Json(json!({})))
}

pub(crate) async fn ban_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    validate_room_id(&room_id)?;

    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(target)?;

    let reason = body.get("reason").and_then(|v| v.as_str());
    if let Some(r) = reason {
        if r.len() > 512 {
            return Err(ApiError::bad_request("Reason too long".to_string()));
        }
    }

    state.services.rooms.room_service.ban_user(&room_id, target, &auth_user.user_id, reason).await?;

    #[cfg(feature = "friends")]
    if let Err(error) = state
        .services
        .extensions
        .friend_room_service
        .sync_dm_room_membership_change(&room_id, target, "banned", Some(&auth_user.user_id), reason)
        .await
    {
        ::tracing::warn!(
            request_id = %request_id,
            room_id = %room_id,
            user_id = %auth_user.user_id,
            target_user_id = %target,
            error = %error,
            "Failed to sync friend DM ban state"
        );
    }

    Ok(Json(json!({})))
}

pub(crate) async fn unban_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(target)?;

    state.services.rooms.room_service.unban_user(&room_id, target, &auth_user.user_id).await?;

    Ok(Json(json!({})))
}
