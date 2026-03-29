// Direct Message Routes - 直接消息路由
// DM room creation and management

use crate::services::room_service::CreateRoomConfig;
use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::Row;
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
    #[validate(length(max = 50))]
    pub visibility: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateDmRequest {
    pub content: Option<Value>,
    pub users: Option<Value>,
}

async fn load_direct_map(state: &AppState, user_id: &str) -> Result<Map<String, Value>, ApiError> {
    let row = sqlx::query("SELECT content FROM account_data WHERE user_id = $1 AND data_type = 'm.direct'")
        .bind(user_id)
        .fetch_optional(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load m.direct account data: {}", e)))?;

    match row {
        Some(row) => match row.get::<Option<Value>, _>("content") {
            Some(Value::Object(map)) => Ok(map),
            Some(_) => Err(ApiError::internal("Invalid m.direct account data format")),
            None => Ok(Map::new()),
        },
        None => Ok(Map::new()),
    }
}

async fn save_direct_map(
    state: &AppState,
    user_id: &str,
    direct_map: &Map<String, Value>,
) -> Result<(), ApiError> {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"
        INSERT INTO account_data (user_id, data_type, content, created_ts, updated_ts)
        VALUES ($1, 'm.direct', $2, $3, $3)
        ON CONFLICT (user_id, data_type) DO UPDATE SET content = EXCLUDED.content, updated_ts = EXCLUDED.updated_ts
        "#,
    )
    .bind(user_id)
    .bind(Value::Object(direct_map.clone()))
    .bind(now)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to save m.direct account data: {}", e)))?;

    Ok(())
}

fn ensure_room_in_direct_map(direct_map: &mut Map<String, Value>, target_user_id: &str, room_id: &str) {
    let entry = direct_map
        .entry(target_user_id.to_string())
        .or_insert_with(|| Value::Array(Vec::new()));

    if !entry.is_array() {
        *entry = Value::Array(Vec::new());
    }

    if let Some(rooms) = entry.as_array_mut() {
        if !rooms.iter().any(|value| value.as_str() == Some(room_id)) {
            rooms.push(Value::String(room_id.to_string()));
        }
    }
}

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

fn parse_dm_users(value: &Value) -> Result<Vec<String>, ApiError> {
    let users = value
        .as_array()
        .ok_or_else(|| ApiError::bad_request("users must be an array of Matrix user IDs"))?;

    let parsed: Vec<String> = users
        .iter()
        .map(|user| {
            user.as_str()
                .map(|value| value.to_string())
                .ok_or_else(|| ApiError::bad_request("users must contain only strings"))
        })
        .collect::<Result<_, _>>()?;

    if parsed.is_empty() {
        return Err(ApiError::bad_request(
            "users must contain at least one Matrix user ID",
        ));
    }

    Ok(parsed)
}

async fn build_direct_map_from_memberships(
    state: &AppState,
    user_id: &str,
) -> Result<Map<String, Value>, ApiError> {
    let rooms = state
        .services
        .room_storage
        .get_user_rooms(user_id)
        .await
        .map_err(|e| ApiError::database(format!("Failed to get user rooms: {}", e)))?;

    let mut direct_map = Map::new();

    for room_id in rooms {
        let join_members = state
            .services
            .member_storage
            .get_room_members(&room_id, "join")
            .await
            .map_err(|e| ApiError::database(format!("Failed to get room join members: {}", e)))?;

        let invited_members = state
            .services
            .member_storage
            .get_room_members(&room_id, "invite")
            .await
            .map_err(|e| ApiError::database(format!("Failed to get room invite members: {}", e)))?;

        let total_members = join_members.len() + invited_members.len();
        if total_members != 2 {
            continue;
        }

        let other_member = join_members
            .iter()
            .find(|member| member.user_id.as_str() != user_id)
            .or_else(|| {
                invited_members
                    .iter()
                    .find(|member| member.user_id.as_str() != user_id)
            });

        if let Some(member) = other_member {
            ensure_room_in_direct_map(&mut direct_map, &member.user_id, &room_id);
        }
    }

    Ok(direct_map)
}

pub async fn create_dm_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateDmRequest>,
) -> Result<Json<Value>, ApiError> {
    // Validate input
    body.validate()
        .map_err(|e| ApiError::bad_request(format!("Validation error: {}", e)))?;

    let invite_list = body.invite.clone().unwrap_or_default();

    let users_to_invite = if !invite_list.is_empty() {
        invite_list.clone()
    } else if let Some(ref uid) = body.user_id {
        vec![uid.clone()]
    } else {
        return Err(ApiError::bad_request(
            "At least one user must be invited to create a DM",
        ));
    };

    let config = CreateRoomConfig {
        name: body.name.clone(),
        visibility: Some(
            body.visibility
                .clone()
                .unwrap_or_else(|| "private".to_string()),
        ),
        preset: Some("private_chat".to_string()),
        invite_list: Some(users_to_invite.clone()),
        is_direct: Some(true),
        room_type: Some("m.direct".to_string()),
        ..Default::default()
    };

    let response = state
        .services
        .room_service
        .create_room(&auth_user.user_id, config)
        .await?;

    let room_id = response
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::internal("Failed to get room_id from create_room response"))?;

    let mut direct_map = load_direct_map(&state, &auth_user.user_id).await?;
    for user_id in &users_to_invite {
        ensure_room_in_direct_map(&mut direct_map, user_id, room_id);
    }
    save_direct_map(&state, &auth_user.user_id, &direct_map).await?;

    Ok(Json(json!({
        "room_id": room_id
    })))
}

pub async fn get_dm_rooms(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;
    let mut dm_rooms = load_direct_map(&state, user_id).await?;
    if dm_rooms.is_empty() {
        dm_rooms = build_direct_map_from_memberships(&state, user_id).await?;
    }

    Ok(Json(json!({
        "rooms": dm_rooms
    })))
}

pub async fn update_dm_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<UpdateDmRequest>,
) -> Result<Json<Value>, ApiError> {
    let mut direct_map = load_direct_map(&state, &auth_user.user_id).await?;
    if let Some(users) = body.users {
        let users = parse_dm_users(&users)?;
        remove_room_from_direct_map(&mut direct_map, &room_id);
        for user_id in users {
            ensure_room_in_direct_map(&mut direct_map, &user_id, &room_id);
        }
        save_direct_map(&state, &auth_user.user_id, &direct_map).await?;
    } else if let Some(content) = body.content {
        let direct_map = content
            .as_object()
            .cloned()
            .ok_or_else(|| ApiError::bad_request("content must be an m.direct object"))?;
        save_direct_map(&state, &auth_user.user_id, &direct_map).await?;
    }

    Ok(Json(json!({})))
}

pub async fn check_room_dm(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let mut direct_map = load_direct_map(&state, &auth_user.user_id).await?;
    if direct_map.is_empty() {
        direct_map = build_direct_map_from_memberships(&state, &auth_user.user_id).await?;
    }

    let is_dm = direct_map.values().any(|value| {
        value
            .as_array()
            .map(|rooms| rooms.iter().any(|room| room.as_str() == Some(&room_id)))
            .unwrap_or(false)
    });

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
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;
    let mut direct_map = load_direct_map(&state, user_id).await?;
    if direct_map.is_empty() {
        direct_map = build_direct_map_from_memberships(&state, user_id).await?;
    }

    let partner_id = direct_map
        .iter()
        .find_map(|(partner_id, rooms)| {
            rooms.as_array().and_then(|entries| {
                entries
                    .iter()
                    .any(|entry| entry.as_str() == Some(&room_id))
                    .then(|| partner_id.clone())
            })
        })
        .ok_or_else(|| ApiError::not_found("DM partner not found".to_string()))?;

    let join_members = state
        .services
        .member_storage
        .get_room_members(&room_id, "join")
        .await
        .map_err(|e| ApiError::database(format!("Failed to get room join members: {}", e)))?;

    let invited_members = state
        .services
        .member_storage
        .get_room_members(&room_id, "invite")
        .await
        .map_err(|e| ApiError::database(format!("Failed to get room invite members: {}", e)))?;

    let other_member = join_members
        .iter()
        .find(|member| member.user_id == partner_id)
        .or_else(|| invited_members.iter().find(|member| member.user_id == partner_id))
        .ok_or_else(|| ApiError::not_found("DM partner not found".to_string()))?;

    Ok(Json(json!({
        "room_id": room_id,
        "user_id": other_member.user_id,
        "display_name": other_member.display_name.clone().unwrap_or_default(),
        "avatar_url": other_member.avatar_url.clone().unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_map_helpers_preserve_matrix_shape() {
        let mut direct_map = Map::new();

        ensure_room_in_direct_map(&mut direct_map, "@bob:example.com", "!room1:example.com");
        ensure_room_in_direct_map(&mut direct_map, "@bob:example.com", "!room1:example.com");
        ensure_room_in_direct_map(&mut direct_map, "@carol:example.com", "!room2:example.com");

        assert_eq!(
            direct_map["@bob:example.com"],
            json!(["!room1:example.com"])
        );
        assert_eq!(
            direct_map["@carol:example.com"],
            json!(["!room2:example.com"])
        );

        remove_room_from_direct_map(&mut direct_map, "!room1:example.com");
        assert!(direct_map.get("@bob:example.com").is_none());
        assert!(direct_map.get("@carol:example.com").is_some());
    }

    #[test]
    fn test_parse_dm_users_requires_string_array() {
        let parsed = parse_dm_users(&json!(["@bob:example.com", "@carol:example.com"])).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], "@bob:example.com");
        assert!(parse_dm_users(&json!("invalid")).is_err());
        assert!(parse_dm_users(&json!([])).is_err());
    }
}
