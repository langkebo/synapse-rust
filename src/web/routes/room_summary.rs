use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::common::ApiError;
use crate::storage::room_summary::{
    CreateRoomSummaryRequest, CreateSummaryMemberRequest, RoomSummaryMember, RoomSummaryStats,
    UpdateSummaryMemberRequest,
};
use crate::web::routes::AppState;
use crate::web::routes::AuthenticatedUser;

#[derive(Debug, Deserialize)]
pub struct QueryLimit {
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSummaryBody {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMemberBody {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub membership: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStateBody {
    pub event_id: Option<String>,
    pub content: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct MemberResponse {
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub membership: String,
    pub is_hero: bool,
}

impl From<RoomSummaryMember> for MemberResponse {
    fn from(m: RoomSummaryMember) -> Self {
        Self {
            user_id: m.user_id,
            display_name: m.display_name,
            avatar_url: m.avatar_url,
            membership: m.membership,
            is_hero: m.is_hero,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub room_id: String,
    pub total_events: i64,
    pub total_state_events: i64,
    pub total_messages: i64,
    pub total_media: i64,
    pub storage_size: i64,
}

impl From<RoomSummaryStats> for StatsResponse {
    fn from(s: RoomSummaryStats) -> Self {
        Self {
            room_id: s.room_id,
            total_events: s.total_events,
            total_state_events: s.total_state_events,
            total_messages: s.total_messages,
            total_media: s.total_media,
            storage_size: s.storage_size,
        }
    }
}

pub async fn get_room_summary(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let summary = state
        .services
        .room_summary_service
        .get_summary(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room summary not found"))?;

    Ok(Json(summary))
}

pub async fn get_user_summaries(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let summaries = state
        .services
        .room_summary_service
        .get_summaries_for_user(&auth_user.user_id)
        .await?;

    Ok(Json(summaries))
}

pub async fn create_room_summary(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CreateRoomSummaryRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let summary = state
        .services
        .room_summary_service
        .create_summary(body)
        .await?;

    Ok((StatusCode::CREATED, Json(summary)))
}

pub async fn update_room_summary(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<UpdateSummaryBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = crate::storage::room_summary::UpdateRoomSummaryRequest {
        name: body.name,
        topic: body.topic,
        avatar_url: body.avatar_url,
        ..Default::default()
    };

    let summary = state
        .services
        .room_summary_service
        .update_summary(&room_id, request)
        .await?;

    Ok(Json(summary))
}

pub async fn delete_room_summary(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .room_summary_service
        .delete_summary(&room_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn sync_room_summary(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let summary = state
        .services
        .room_summary_service
        .sync_from_room(&room_id)
        .await?;

    Ok(Json(summary))
}

pub async fn get_members(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let members = state
        .services
        .room_summary_service
        .get_members(&room_id)
        .await?;

    let response: Vec<MemberResponse> = members.into_iter().map(MemberResponse::from).collect();

    Ok(Json(response))
}

pub async fn add_member(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CreateSummaryMemberRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateSummaryMemberRequest {
        room_id,
        user_id: body.user_id,
        display_name: body.display_name,
        avatar_url: body.avatar_url,
        membership: body.membership,
        is_hero: body.is_hero,
        last_active_ts: body.last_active_ts,
    };

    let member = state
        .services
        .room_summary_service
        .add_member(request)
        .await?;

    Ok((StatusCode::CREATED, Json(MemberResponse::from(member))))
}

pub async fn update_member(
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<UpdateMemberBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = UpdateSummaryMemberRequest {
        display_name: body.display_name,
        avatar_url: body.avatar_url,
        membership: body.membership,
        is_hero: None,
        last_active_ts: None,
    };

    let member = state
        .services
        .room_summary_service
        .update_member(&room_id, &user_id, request)
        .await?;

    Ok(Json(MemberResponse::from(member)))
}

pub async fn remove_member(
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .room_summary_service
        .remove_member(&room_id, &user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_state(
    State(state): State<AppState>,
    Path((room_id, event_type, state_key)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state
        .services
        .room_summary_service
        .get_state(&room_id, &event_type, &state_key)
        .await?
        .ok_or_else(|| ApiError::not_found("State not found"))?;

    Ok(Json(serde_json::json!({
        "event_id": state.event_id,
        "content": state.content,
        "updated_ts": state.updated_ts,
    })))
}

pub async fn update_state(
    State(state): State<AppState>,
    Path((room_id, event_type, state_key)): Path<(String, String, String)>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<UpdateStateBody>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state
        .services
        .room_summary_service
        .update_state(
            &room_id,
            &event_type,
            &state_key,
            body.event_id.as_deref(),
            body.content,
        )
        .await?;

    Ok(Json(serde_json::json!({
        "event_id": state.event_id,
        "content": state.content,
        "updated_ts": state.updated_ts,
    })))
}

pub async fn get_all_state(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let states = state
        .services
        .room_summary_service
        .get_all_state(&room_id)
        .await?;

    let response: Vec<serde_json::Value> = states
        .into_iter()
        .map(|s| {
            serde_json::json!({
                "event_type": s.event_type,
                "state_key": s.state_key,
                "event_id": s.event_id,
                "content": s.content,
            })
        })
        .collect();

    Ok(Json(response))
}

pub async fn get_stats(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let stats = state
        .services
        .room_summary_service
        .get_stats(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Stats not found"))?;

    Ok(Json(StatsResponse::from(stats)))
}

pub async fn recalculate_stats(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let stats = state
        .services
        .room_summary_service
        .recalculate_stats(&room_id)
        .await?;

    Ok(Json(StatsResponse::from(stats)))
}

pub async fn process_updates(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(query): Query<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let processed = state
        .services
        .room_summary_service
        .process_pending_updates(limit)
        .await?;

    Ok(Json(serde_json::json!({
        "processed": processed,
    })))
}

pub async fn recalculate_heroes(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let hero_ids = state
        .services
        .room_summary_service
        .recalculate_heroes(&room_id)
        .await?;

    Ok(Json(serde_json::json!({
        "heroes": hero_ids,
    })))
}

pub async fn clear_unread(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .room_summary_service
        .clear_unread(&room_id)
        .await?;

    Ok(Json(serde_json::json!({
        "user_id": auth_user.user_id,
        "room_id": room_id,
        "unread_notifications": 0,
        "unread_highlight": 0,
    })))
}

pub fn create_room_summary_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary",
            get(get_room_summary),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary",
            put(update_room_summary),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary",
            delete(delete_room_summary),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/sync",
            post(sync_room_summary),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/members",
            get(get_members),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/members",
            post(add_member),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/members/{user_id}",
            put(update_member),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/members/{user_id}",
            delete(remove_member),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/state",
            get(get_all_state),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/state/{event_type}/{state_key}",
            get(get_state),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/state/{event_type}/{state_key}",
            put(update_state),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/stats",
            get(get_stats),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/stats/recalculate",
            post(recalculate_stats),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/heroes/recalculate",
            post(recalculate_heroes),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/unread/clear",
            post(clear_unread),
        )
        .route(
            "/_synapse/room_summary/v1/summaries",
            get(get_user_summaries),
        )
        .route(
            "/_synapse/room_summary/v1/summaries",
            post(create_room_summary),
        )
        .route(
            "/_synapse/room_summary/v1/updates/process",
            post(process_updates),
        )
        .with_state(state)
}
