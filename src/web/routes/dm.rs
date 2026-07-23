// Direct Message Routes - 直接消息路由
// DM room creation and management

use crate::web::routes::context::RoomContext;
use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use synapse_common::current_timestamp_millis;
use synapse_services::friend_room_service::FriendRoomCreateRoomConfig;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateDmRequest {
    #[validate(length(max = 100))]
    pub user_id: Option<String>,
    #[validate(length(max = 100))]
    pub invite: Option<Vec<String>>,
    pub is_direct: Option<bool>,
    #[validate(length(max = 255))]
    pub name: Option<String>,
    #[validate(length(max = 1024))]
    pub topic: Option<String>,
    #[validate(length(max = 50))]
    pub visibility: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateDmRequest {
    pub content: Option<Value>,
    pub users: Option<Value>,
}

enum DirectMapUpdateAction {
    ReplaceRoomTargets(Vec<String>),
    OverwriteMap(Map<String, Value>),
}

#[cfg(not(feature = "friends"))]
async fn load_direct_map(ctx: &RoomContext, user_id: &str) -> Result<Map<String, Value>, ApiError> {
    let content = ctx.account_data_service.get_account_data(user_id, "m.direct").await?;

    match content {
        Some(Value::Object(map)) => Ok(map),
        Some(_) => Err(ApiError::internal("Invalid m.direct account data format")),
        None => Ok(Map::new()),
    }
}

#[cfg(not(feature = "friends"))]
async fn save_direct_map(ctx: &RoomContext, user_id: &str, direct_map: &Map<String, Value>) -> Result<(), ApiError> {
    ctx.account_data_service.set_account_data(user_id, "m.direct", &Value::Object(direct_map.clone())).await?;

    Ok(())
}

#[cfg(any(test, not(feature = "friends")))]
fn ensure_room_in_direct_map(direct_map: &mut Map<String, Value>, target_user_id: &str, room_id: &str) {
    let entry = direct_map.entry(target_user_id.to_string()).or_insert_with(|| Value::Array(Vec::new()));

    if !entry.is_array() {
        *entry = Value::Array(Vec::new());
    }

    if let Some(rooms) = entry.as_array_mut() {
        if !rooms.iter().any(|value| value.as_str() == Some(room_id)) {
            rooms.push(Value::String(room_id.to_string()));
        }
    }
}

#[cfg(any(test, not(feature = "friends")))]
fn remove_room_from_direct_map(direct_map: &mut Map<String, Value>, room_id: &str) {
    direct_map.retain(|_, value| {
        if let Some(rooms) = value.as_array_mut() {
            rooms.retain(|room| room.as_str() != Some(room_id));
            !rooms.is_empty()
        } else {
            false
        }
    });
}

#[cfg(any(test, not(feature = "friends")))]
fn get_room_direct_users(direct_map: &Map<String, Value>, room_id: &str) -> Vec<String> {
    direct_map
        .iter()
        .filter_map(|(user_id, value)| {
            value
                .as_array()
                .and_then(|rooms| rooms.iter().any(|room| room.as_str() == Some(room_id)).then_some(user_id))
                .cloned()
        })
        .collect()
}

#[cfg(any(test, not(feature = "friends")))]
fn get_direct_room_for_user(direct_map: &Map<String, Value>, target_user_id: &str) -> Option<String> {
    direct_map
        .get(target_user_id)
        .and_then(|value| value.as_array())
        .and_then(|rooms| rooms.first())
        .and_then(|room| room.as_str())
        .map(ToOwned::to_owned)
}

#[cfg(any(test, not(feature = "friends")))]
fn merge_direct_links(direct_map: &mut Map<String, Value>, links: impl IntoIterator<Item = (String, String)>) {
    for (user_id, room_id) in links {
        ensure_room_in_direct_map(direct_map, &user_id, &room_id);
    }
}

fn parse_dm_users(value: &Value) -> Result<Vec<String>, ApiError> {
    let parsed = match value {
        Value::Array(users) => users
            .iter()
            .map(|user| {
                user.as_str()
                    .map(|value| value.to_string())
                    .ok_or_else(|| ApiError::bad_request("users must contain only strings"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Value::Object(users) => users.keys().cloned().collect::<Vec<_>>(),
        _ => {
            return Err(ApiError::bad_request("users must be an array of Matrix user IDs or an m.direct user map"));
        }
    };

    if parsed.is_empty() {
        return Err(ApiError::bad_request("users must contain at least one Matrix user ID"));
    }

    Ok(parsed)
}

#[cfg(not(feature = "friends"))]
async fn build_direct_map_from_memberships(ctx: &RoomContext, user_id: &str) -> Result<Map<String, Value>, ApiError> {
    let rooms = ctx.room_service.membership().get_joined_rooms(user_id).await?;

    let mut direct_map = Map::new();

    for room_id in rooms {
        let join_members = ctx.room_service.membership().get_room_members_by_membership(&room_id, "join").await?;

        let invited_members = ctx.room_service.membership().get_room_members_by_membership(&room_id, "invite").await?;

        let total_members = join_members.len() + invited_members.len();
        if total_members != 2 {
            continue;
        }

        let other_member = join_members
            .iter()
            .find(|member| member.user_id.as_str() != user_id)
            .or_else(|| invited_members.iter().find(|member| member.user_id.as_str() != user_id));

        if let Some(member) = other_member {
            ensure_room_in_direct_map(&mut direct_map, &member.user_id, &room_id);
        }
    }

    Ok(direct_map)
}

#[cfg(not(feature = "friends"))]
async fn build_direct_map_from_friend_links(
    _ctx: &RoomContext,
    _user_id: &str,
) -> Result<Map<String, Value>, ApiError> {
    Ok(Map::new())
}

#[cfg(not(feature = "friends"))]
async fn load_effective_direct_map(ctx: &RoomContext, user_id: &str) -> Result<Map<String, Value>, ApiError> {
    let mut direct_map = load_direct_map(ctx, user_id).await?;
    let friend_links = build_direct_map_from_friend_links(ctx, user_id).await?;
    merge_direct_links(
        &mut direct_map,
        friend_links.into_iter().flat_map(|(user_id, value)| {
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|room| room.as_str().map(|room_id| (user_id.clone(), room_id.to_owned())))
                .collect::<Vec<_>>()
        }),
    );

    if direct_map.is_empty() {
        direct_map = build_direct_map_from_memberships(ctx, user_id).await?;
    }

    Ok(direct_map)
}

#[cfg(not(feature = "friends"))]
async fn upsert_direct_room_links(
    ctx: &RoomContext,
    user_id: &str,
    target_user_ids: &[String],
    room_id: &str,
) -> Result<Map<String, Value>, ApiError> {
    let mut direct_map = load_direct_map(ctx, user_id).await?;
    for target_user_id in target_user_ids {
        ensure_room_in_direct_map(&mut direct_map, target_user_id, room_id);
    }
    save_direct_map(ctx, user_id, &direct_map).await?;
    Ok(direct_map)
}

fn parse_direct_map_update(body: UpdateDmRequest) -> Result<Option<DirectMapUpdateAction>, ApiError> {
    if let Some(users) = body.users {
        return parse_dm_users(&users).map(DirectMapUpdateAction::ReplaceRoomTargets).map(Some);
    }

    if let Some(content) = body.content {
        let content =
            content.as_object().cloned().ok_or_else(|| ApiError::bad_request("content must be an m.direct object"))?;

        if let Some(user_id) = content.get("user_id").and_then(|value| value.as_str()) {
            return Ok(Some(DirectMapUpdateAction::ReplaceRoomTargets(vec![user_id.to_string()])));
        }

        if let Some(users) = content.get("users") {
            return parse_dm_users(users).map(DirectMapUpdateAction::ReplaceRoomTargets).map(Some);
        }
        return Ok(Some(DirectMapUpdateAction::OverwriteMap(content)));
    }

    Ok(None)
}

#[cfg(not(feature = "friends"))]
async fn apply_direct_map_update(
    ctx: &RoomContext,
    user_id: &str,
    room_id: &str,
    action: DirectMapUpdateAction,
) -> Result<Map<String, Value>, ApiError> {
    #[cfg(not(feature = "friends"))]
    {
        return match action {
            DirectMapUpdateAction::ReplaceRoomTargets(target_user_ids) => {
                let mut direct_map = load_direct_map(ctx, user_id).await?;
                remove_room_from_direct_map(&mut direct_map, room_id);
                for target_user_id in &target_user_ids {
                    ensure_room_in_direct_map(&mut direct_map, target_user_id, room_id);
                }
                save_direct_map(ctx, user_id, &direct_map).await?;
                Ok(direct_map)
            }
            DirectMapUpdateAction::OverwriteMap(direct_map) => {
                save_direct_map(ctx, user_id, &direct_map).await?;
                Ok(direct_map)
            }
        };
    }
}

async fn load_direct_room_snapshot(
    ctx: &RoomContext,
    user_id: &str,
    room_id: &str,
) -> Result<(Map<String, Value>, Vec<String>, bool), ApiError> {
    #[cfg(feature = "friends")]
    {
        let snapshot: synapse_services::friend_room_service::DirectRoomSnapshot =
            ctx.friend_room_service.get_direct_room_snapshot(user_id, room_id).await?;
        Ok((snapshot.direct_map, snapshot.users, snapshot.is_direct))
    }

    #[cfg(not(feature = "friends"))]
    {
        let direct_map: Map<String, Value> = load_effective_direct_map(ctx, user_id).await?;
        let users: Vec<String> = get_room_direct_users(&direct_map, room_id);
        let is_direct: bool = !users.is_empty();
        Ok((direct_map, users, is_direct))
    }
}

async fn update_direct_room_snapshot(
    ctx: &RoomContext,
    user_id: &str,
    room_id: &str,
    action: DirectMapUpdateAction,
) -> Result<(Map<String, Value>, Vec<String>, bool), ApiError> {
    #[cfg(feature = "friends")]
    {
        let action: synapse_services::friend_room_service::DirectMapUpdateAction = match action {
            DirectMapUpdateAction::ReplaceRoomTargets(target_user_ids) => {
                synapse_services::friend_room_service::DirectMapUpdateAction::ReplaceRoomTargets {
                    room_id: room_id.to_string(),
                    target_user_ids,
                }
            }
            DirectMapUpdateAction::OverwriteMap(direct_map) => {
                synapse_services::friend_room_service::DirectMapUpdateAction::OverwriteMap(direct_map)
            }
        };

        let snapshot: synapse_services::friend_room_service::DirectRoomSnapshot =
            ctx.friend_room_service.update_direct_room_snapshot(user_id, room_id, action).await?;
        Ok((snapshot.direct_map, snapshot.users, snapshot.is_direct))
    }

    #[cfg(not(feature = "friends"))]
    {
        let direct_map: Map<String, Value> = apply_direct_map_update(ctx, user_id, room_id, action).await?;
        let users: Vec<String> = get_room_direct_users(&direct_map, room_id);
        let is_direct: bool = !users.is_empty();
        Ok((direct_map, users, is_direct))
    }
}

#[cfg(not(feature = "friends"))]
async fn persist_friend_dm_link_if_applicable(
    _ctx: &RoomContext,
    _owner_user_id: &str,
    _invitees: &[String],
    _room_id: &str,
) -> Result<(), ApiError> {
    Ok(())
}

#[cfg(not(feature = "friends"))]
async fn find_existing_direct_room_id(
    ctx: &RoomContext,
    owner_user_id: &str,
    invitees: &[String],
) -> Result<Option<String>, ApiError> {
    if invitees.len() != 1 {
        return Ok(None);
    }

    let direct_map = load_effective_direct_map(ctx, owner_user_id).await?;
    Ok(get_direct_room_for_user(&direct_map, &invitees[0]))
}

#[cfg(feature = "friends")]
async fn create_dm_room_via_service(
    ctx: &RoomContext,
    owner_user_id: &str,
    invitee_user_ids: &[String],
    body: &CreateDmRequest,
) -> Result<String, ApiError> {
    let config = FriendRoomCreateRoomConfig {
        name: body.name.clone(),
        topic: body.topic.clone(),
        invite_list: Some(invitee_user_ids.to_vec()),
        is_direct: Some(body.is_direct.unwrap_or(true)),
        visibility: body.visibility.clone().or_else(|| Some("private".to_string())),
        preset: Some("private_chat".to_string()),
        room_type: (invitee_user_ids.len() > 1).then(|| "m.direct".to_string()),
        ..Default::default()
    };

    let result = ctx
        .friend_room_service
        .create_or_reuse_direct_message_room(owner_user_id, invitee_user_ids, config, Some(owner_user_id))
        .await?;

    Ok(result.room_id)
}

#[cfg(not(feature = "friends"))]
async fn create_dm_room_via_service(
    ctx: &RoomContext,
    owner_user_id: &str,
    invitee_user_ids: &[String],
    body: &CreateDmRequest,
) -> Result<String, ApiError> {
    let room_id = if invitee_user_ids.len() == 1 {
        if let Some(existing_room_id) = find_existing_direct_room_id(ctx, owner_user_id, invitee_user_ids).await? {
            existing_room_id
        } else {
            let config = CreateRoomConfig {
                name: body.name.clone(),
                topic: body.topic.clone(),
                invite_list: Some(invitee_user_ids.to_vec()),
                is_direct: Some(true),
                visibility: body.visibility.clone().or_else(|| Some("private".to_string())),
                preset: Some("private_chat".to_string()),
                ..Default::default()
            };

            let created = ctx.room_service.lifecycle().create_room(owner_user_id, config).await?;

            created["room_id"]
                .as_str()
                .ok_or_else(|| ApiError::internal("Failed to create DM room".to_string()))?
                .to_string()
        }
    } else {
        let config = CreateRoomConfig {
            name: body.name.clone(),
            topic: body.topic.clone(),
            visibility: Some(body.visibility.clone().unwrap_or_else(|| "private".to_string())),
            preset: Some("private_chat".to_string()),
            invite_list: Some(invitee_user_ids.to_vec()),
            is_direct: Some(body.is_direct.unwrap_or(true)),
            room_type: Some("m.direct".to_string()),
            ..Default::default()
        };

        let response = ctx.room_service.lifecycle().create_room(owner_user_id, config).await?;

        response
            .get("room_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::internal("Failed to get room_id from create_room response"))?
            .to_string()
    };

    persist_friend_dm_link_if_applicable(ctx, owner_user_id, invitee_user_ids, &room_id).await?;
    upsert_direct_room_links(ctx, owner_user_id, invitee_user_ids, &room_id).await?;

    Ok(room_id)
}

#[cfg(feature = "friends")]
async fn load_dm_partner_info(
    ctx: &RoomContext,
    user_id: &str,
    room_id: &str,
) -> Result<(String, String, String), ApiError> {
    let partner = ctx
        .friend_room_service
        .get_dm_partner_for_room(user_id, room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("DM partner not found".to_string()))?;

    Ok((partner.user_id, partner.display_name, partner.avatar_url))
}

#[cfg(not(feature = "friends"))]
async fn load_dm_partner_info(
    ctx: &RoomContext,
    user_id: &str,
    room_id: &str,
) -> Result<(String, String, String), ApiError> {
    let direct_map = load_effective_direct_map(ctx, user_id).await?;

    let partner_id = direct_map
        .iter()
        .find_map(|(partner_id, rooms)| {
            rooms.as_array().and_then(|entries| {
                entries.iter().any(|entry| entry.as_str() == Some(room_id)).then(|| partner_id.clone())
            })
        })
        .ok_or_else(|| ApiError::not_found("DM partner not found".to_string()))?;

    let join_members = ctx.room_service.membership().get_room_members_by_membership(room_id, "join").await?;

    let invited_members = ctx.room_service.membership().get_room_members_by_membership(room_id, "invite").await?;

    let other_member = join_members
        .iter()
        .find(|member| member.user_id.as_str() == partner_id)
        .or_else(|| invited_members.iter().find(|member| member.user_id.as_str() == partner_id))
        .ok_or_else(|| ApiError::not_found("DM partner not found".to_string()))?;

    Ok((
        other_member.user_id.to_string(),
        other_member.display_name.clone().unwrap_or_default(),
        other_member.avatar_url.clone().unwrap_or_default(),
    ))
}

pub async fn create_dm_room(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateDmRequest>,
) -> Result<Json<Value>, ApiError> {
    // Validate input
    body.validate().map_err(|e| ApiError::bad_request(format!("Validation error: {e}")))?;

    let invite_list = body.invite.clone().unwrap_or_default();

    if invite_list.len() > 20 {
        return Err(ApiError::bad_request("DM room cannot have more than 20 invitees"));
    }

    let users_to_invite = if !invite_list.is_empty() {
        invite_list.clone()
    } else if let Some(ref uid) = body.user_id {
        vec![uid.clone()]
    } else {
        return Err(ApiError::bad_request("At least one user must be invited to create a DM"));
    };

    let room_id = create_dm_room_via_service(&ctx, &auth_user.user_id, &users_to_invite, &body).await?;

    Ok(Json(json!({ "room_id": room_id })))
}

pub async fn get_dm_rooms(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    #[cfg(feature = "friends")]
    {
        let dm_rooms = ctx.friend_room_service.get_effective_direct_map(user_id).await?;
        Ok(Json(json!({ "rooms": dm_rooms })))
    }

    #[cfg(not(feature = "friends"))]
    {
        let dm_rooms = load_effective_direct_map(&ctx, user_id).await?;
        Ok(Json(json!({
            "rooms": dm_rooms
        })))
    }
}

pub async fn update_dm_room(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<UpdateDmRequest>,
) -> Result<Json<Value>, ApiError> {
    let (mut direct_map, mut users, _) = load_direct_room_snapshot(&ctx, &auth_user.user_id, &room_id).await?;

    let now = current_timestamp_millis();

    if let Some(action) = parse_direct_map_update(body)? {
        (direct_map, users, _) = update_direct_room_snapshot(&ctx, &auth_user.user_id, &room_id, action).await?;
    }

    Ok(Json(json!({
        "room_id": room_id,
        "users": users,
        "direct_map": direct_map,
        "updated_ts": now
    })))
}

pub async fn check_room_dm(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let (_, _, is_dm) = load_direct_room_snapshot(&ctx, &auth_user.user_id, &room_id).await?;

    if is_dm {
        Ok(Json(json!({
            "room_id": room_id,
            "m.direct": true
        })))
    } else {
        Err(ApiError::not_found("Room is not a DM".to_string()))
    }
}

pub async fn get_dm_partner_route(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let (partner_user_id, display_name, avatar_url) = load_dm_partner_info(&ctx, &auth_user.user_id, &room_id).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "user_id": partner_user_id,
        "display_name": display_name,
        "avatar_url": avatar_url
    })))
}

pub fn create_dm_router(state: AppState) -> Router<AppState> {
    let v3_router = Router::new()
        .route("/direct", get(get_dm_rooms))
        .route("/direct/{room_id}", put(update_dm_room))
        .route("/rooms/{room_id}/dm", get(check_room_dm))
        .route("/rooms/{room_id}/dm/partner", get(get_dm_partner_route));

    Router::new()
        .route("/_matrix/client/r0/create_dm", post(create_dm_room))
        .route("/_matrix/client/v3/create_dm", post(create_dm_room))
        .route("/_matrix/client/r0/direct", get(get_dm_rooms))
        .route("/_matrix/client/r0/direct/{room_id}", put(update_dm_room))
        .nest("/_matrix/client/v3", v3_router)
        .with_state(state)
}

pub fn dm_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::POST, "/_matrix/client/r0/create_dm"),
        (Method::POST, "/_matrix/client/v3/create_dm"),
        (Method::GET, "/_matrix/client/r0/direct"),
        (Method::PUT, "/_matrix/client/r0/direct/{room_id}"),
        (Method::GET, "/_matrix/client/v3/direct"),
        (Method::PUT, "/_matrix/client/v3/direct/{room_id}"),
        (Method::GET, "/_matrix/client/v3/rooms/{room_id}/dm"),
        (Method::GET, "/_matrix/client/v3/rooms/{room_id}/dm/partner"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "dm"))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_map_helpers_preserve_matrix_shape() {
        let mut direct_map = Map::new();

        ensure_room_in_direct_map(&mut direct_map, "@bob:example.com", "!room1:example.com");
        ensure_room_in_direct_map(&mut direct_map, "@bob:example.com", "!room1:example.com");
        ensure_room_in_direct_map(&mut direct_map, "@carol:example.com", "!room2:example.com");

        assert_eq!(direct_map["@bob:example.com"], json!(["!room1:example.com"]));
        assert_eq!(direct_map["@carol:example.com"], json!(["!room2:example.com"]));

        remove_room_from_direct_map(&mut direct_map, "!room1:example.com");
        assert!(direct_map.get("@bob:example.com").is_none());
        assert!(direct_map.get("@carol:example.com").is_some());
    }

    #[test]
    fn test_parse_dm_users_requires_string_array() {
        let parsed = parse_dm_users(&json!(["@bob:example.com", "@carol:example.com"])).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], "@bob:example.com");
        let object_parsed = parse_dm_users(&json!({
            "@bob:example.com": {},
            "@carol:example.com": {"is_direct": true}
        }))
        .unwrap();
        assert_eq!(object_parsed.len(), 2);
        assert!(object_parsed.contains(&"@bob:example.com".to_string()));
        assert!(parse_dm_users(&json!("invalid")).is_err());
        assert!(parse_dm_users(&json!([])).is_err());
    }

    #[test]
    fn test_get_room_direct_users_returns_matching_entries() {
        let direct_map = serde_json::from_value::<Map<String, Value>>(json!({
            "@bob:example.com": ["!room1:example.com", "!room2:example.com"],
            "@carol:example.com": ["!room2:example.com"]
        }))
        .unwrap();

        let room1_users = get_room_direct_users(&direct_map, "!room1:example.com");
        assert_eq!(room1_users, vec!["@bob:example.com".to_string()]);

        let mut room2_users = get_room_direct_users(&direct_map, "!room2:example.com");
        room2_users.sort();
        assert_eq!(room2_users, vec!["@bob:example.com".to_string(), "@carol:example.com".to_string()]);
    }

    #[test]
    fn test_merge_direct_links_preserves_existing_entries() {
        let mut direct_map = serde_json::from_value::<Map<String, Value>>(json!({
            "@bob:example.com": ["!room1:example.com"]
        }))
        .unwrap();

        merge_direct_links(
            &mut direct_map,
            vec![
                ("@bob:example.com".to_string(), "!room1:example.com".to_string()),
                ("@bob:example.com".to_string(), "!room2:example.com".to_string()),
                ("@carol:example.com".to_string(), "!room3:example.com".to_string()),
            ],
        );

        assert_eq!(direct_map["@bob:example.com"], json!(["!room1:example.com", "!room2:example.com"]));
        assert_eq!(direct_map["@carol:example.com"], json!(["!room3:example.com"]));
    }

    #[test]
    fn test_get_direct_room_for_user_returns_first_room() {
        let direct_map = serde_json::from_value::<Map<String, Value>>(json!({
            "@bob:example.com": ["!room1:example.com", "!room2:example.com"]
        }))
        .unwrap();

        assert_eq!(get_direct_room_for_user(&direct_map, "@bob:example.com").as_deref(), Some("!room1:example.com"));
        assert_eq!(get_direct_room_for_user(&direct_map, "@carol:example.com"), None);
    }
}
