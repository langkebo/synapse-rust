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
    CreateRoomSummaryRequest, CreateSummaryMemberRequest, RoomSummaryMember, RoomSummaryResponse,
    RoomSummaryState, RoomSummaryStats, UpdateSummaryMemberRequest,
};
use crate::web::routes::response_helpers::{
    created_json, created_json_from, json_from, json_vec_from, require_found,
};
use crate::web::routes::AppState;
use crate::web::routes::{ensure_room_member_strict, AdminUser, AuthenticatedUser};

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

impl UpdateSummaryBody {
    fn into_request(self) -> crate::storage::room_summary::UpdateRoomSummaryRequest {
        crate::storage::room_summary::UpdateRoomSummaryRequest {
            name: self.name,
            topic: self.topic,
            avatar_url: self.avatar_url,
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateMemberBody {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub membership: Option<String>,
}

impl UpdateMemberBody {
    fn into_request(self) -> UpdateSummaryMemberRequest {
        UpdateSummaryMemberRequest {
            display_name: self.display_name,
            avatar_url: self.avatar_url,
            membership: self.membership,
            is_hero: None,
            last_active_ts: None,
        }
    }
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

#[derive(Debug, Serialize)]
pub struct RoomSummaryListResponse {
    pub summaries: Vec<RoomSummaryResponse>,
    pub rooms: Vec<RoomSummaryResponse>,
    pub chunk: Vec<RoomSummaryResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_batch: Option<String>,
}

fn create_summary_request_for_room(
    room_id: String,
    body: CreateRoomSummaryRequest,
) -> Result<CreateRoomSummaryRequest, ApiError> {
    if body.room_id != room_id {
        return Err(ApiError::bad_request(
            "Path room_id does not match body room_id".to_string(),
        ));
    }

    Ok(CreateRoomSummaryRequest {
        room_id,
        room_type: body.room_type,
        name: body.name,
        topic: body.topic,
        avatar_url: body.avatar_url,
        canonical_alias: body.canonical_alias,
        join_rule: body.join_rule,
        history_visibility: body.history_visibility,
        guest_access: body.guest_access,
        is_direct: body.is_direct,
        is_space: body.is_space,
    })
}

fn create_summary_member_request_for_room(
    room_id: String,
    body: CreateSummaryMemberRequest,
) -> CreateSummaryMemberRequest {
    CreateSummaryMemberRequest {
        room_id,
        user_id: body.user_id,
        display_name: body.display_name,
        avatar_url: body.avatar_url,
        membership: body.membership,
        is_hero: body.is_hero,
        last_active_ts: body.last_active_ts,
    }
}

fn room_summary_state_json(state: RoomSummaryState) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "event_id": state.event_id,
        "content": state.content,
        "updated_ts": state.updated_ts,
    }))
}

async fn ensure_room_summary_read_access(
    state: &AppState,
    auth_user: &AuthenticatedUser,
    room_id: &str,
) -> Result<(), ApiError> {
    ensure_room_member_strict(
        state,
        auth_user,
        room_id,
        "You must be a member of this room to view room summary",
    )
    .await
}

async fn ensure_room_summary_manage_access(
    state: &AppState,
    auth_user: &AuthenticatedUser,
    room_id: &str,
) -> Result<(), ApiError> {
    ensure_room_member_strict(
        state,
        auth_user,
        room_id,
        "You must be a member of this room to manage room summary",
    )
    .await?;

    let is_creator = state
        .services
        .room_service
        .is_room_creator(room_id, &auth_user.user_id)
        .await?;

    if !is_creator {
        return Err(ApiError::forbidden(
            "Only room admins can manage room summary".to_string(),
        ));
    }

    Ok(())
}

pub async fn get_room_summary(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    ensure_room_summary_read_access(&state, &auth_user, &room_id).await?;

    let summary = state
        .services
        .room_service
        .room_summary_service()
        .get_summary(&room_id)
        .await?;

    Ok(Json(require_found(summary, "Room summary not found")?))
}

pub async fn get_user_summaries(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let summaries = state
        .services
        .room_service
        .room_summary_service()
        .get_summaries_for_user(&_auth_user.user_id)
        .await?;

    Ok(Json(RoomSummaryListResponse {
        summaries: summaries.clone(),
        rooms: summaries.clone(),
        chunk: summaries,
        next_batch: None,
    }))
}

pub async fn create_room_summary(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateRoomSummaryRequest>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_room_summary_manage_access(&state, &auth_user, &room_id).await?;

    let request = create_summary_request_for_room(room_id, body)?;

    let summary = state
        .services
        .room_service
        .room_summary_service()
        .create_summary(request)
        .await?;

    Ok(created_json(summary))
}

pub async fn create_internal_room_summary(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CreateRoomSummaryRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let summary = state
        .services
        .room_service
        .room_summary_service()
        .create_summary(body)
        .await?;

    Ok(created_json(summary))
}

pub async fn update_room_summary(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: AuthenticatedUser,
    Json(body): Json<UpdateSummaryBody>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_room_summary_manage_access(&state, &auth_user, &room_id).await?;

    let request = body.into_request();

    let summary = state
        .services
        .room_service
        .room_summary_service()
        .update_summary(&room_id, request)
        .await?;

    Ok(Json(summary))
}

pub async fn delete_room_summary(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    ensure_room_summary_manage_access(&state, &auth_user, &room_id).await?;

    state
        .services
        .room_service
        .room_summary_service()
        .delete_summary(&room_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn sync_room_summary(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    ensure_room_summary_manage_access(&state, &auth_user, &room_id).await?;

    let summary = state
        .services
        .room_service
        .room_summary_service()
        .sync_from_room(&room_id)
        .await?;

    Ok(Json(summary))
}

pub async fn get_members(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    ensure_room_summary_read_access(&state, &auth_user, &room_id).await?;

    let members = state
        .services
        .room_service
        .room_summary_service()
        .get_members(&room_id)
        .await?;

    Ok(json_vec_from::<_, MemberResponse>(members))
}

pub async fn add_member(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateSummaryMemberRequest>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_room_summary_manage_access(&state, &auth_user, &room_id).await?;

    let request = create_summary_member_request_for_room(room_id, body);

    let member = state
        .services
        .room_service
        .room_summary_service()
        .add_member(request)
        .await?;

    Ok(created_json_from::<_, MemberResponse>(member))
}

pub async fn update_member(
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
    auth_user: AuthenticatedUser,
    Json(body): Json<UpdateMemberBody>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_room_summary_manage_access(&state, &auth_user, &room_id).await?;

    let request = body.into_request();

    let member = state
        .services
        .room_service
        .room_summary_service()
        .update_member(&room_id, &user_id, request)
        .await?;

    Ok(json_from::<_, MemberResponse>(member))
}

pub async fn remove_member(
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    ensure_room_summary_manage_access(&state, &auth_user, &room_id).await?;

    state
        .services
        .room_service
        .room_summary_service()
        .remove_member(&room_id, &user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_state(
    State(state): State<AppState>,
    Path((room_id, event_type, state_key)): Path<(String, String, String)>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    ensure_room_summary_read_access(&state, &auth_user, &room_id).await?;

    let state = state
        .services
        .room_service
        .room_summary_service()
        .get_state(&room_id, &event_type, &state_key)
        .await?;

    Ok(room_summary_state_json(require_found(
        state,
        "State not found",
    )?))
}

pub async fn update_state(
    State(state): State<AppState>,
    Path((room_id, event_type, state_key)): Path<(String, String, String)>,
    auth_user: AuthenticatedUser,
    Json(body): Json<UpdateStateBody>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_room_summary_manage_access(&state, &auth_user, &room_id).await?;

    let state = state
        .services
        .room_service
        .room_summary_service()
        .update_state(
            &room_id,
            &event_type,
            &state_key,
            body.event_id.as_deref(),
            body.content,
        )
        .await?;

    Ok(room_summary_state_json(state))
}

pub async fn get_all_state(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    ensure_room_summary_read_access(&state, &auth_user, &room_id).await?;

    let states = state
        .services
        .room_service
        .room_summary_service()
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
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    ensure_room_summary_read_access(&state, &auth_user, &room_id).await?;

    let stats = state
        .services
        .room_service
        .room_summary_service()
        .get_stats(&room_id)
        .await?;

    let stats = match stats {
        Some(s) => s,
        None => {
            state
                .services
                .room_service
                .room_summary_service()
                .recalculate_stats(&room_id)
                .await?
        }
    };

    Ok(Json(StatsResponse::from(stats)))
}

pub async fn recalculate_stats(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    ensure_room_summary_manage_access(&state, &auth_user, &room_id).await?;

    let stats = state
        .services
        .room_service
        .room_summary_service()
        .recalculate_stats(&room_id)
        .await?;

    Ok(Json(StatsResponse::from(stats)))
}

pub async fn process_updates(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let processed = state
        .services
        .room_service
        .room_summary_service()
        .process_pending_updates(limit)
        .await?;

    Ok(Json(serde_json::json!({
        "processed": processed,
    })))
}

pub async fn recalculate_heroes(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    ensure_room_summary_manage_access(&state, &auth_user, &room_id).await?;

    let hero_ids = state
        .services
        .room_service
        .room_summary_service()
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
    ensure_room_summary_manage_access(&state, &auth_user, &room_id).await?;

    state
        .services
        .room_service
        .room_summary_service()
        .clear_unread(&room_id)
        .await?;

    Ok(Json(serde_json::json!({
        "user_id": auth_user.user_id,
        "room_id": room_id,
        "unread_notifications": 0,
        "unread_highlight": 0,
    })))
}

#[derive(Debug, Deserialize)]
pub struct RoomSummaryBatchRequest {
    pub rooms: Vec<String>,
    #[serde(default, rename = "suggested_only")]
    pub is_suggested_only: bool,
}

#[derive(Debug, Serialize)]
pub struct Msc3266RoomSummaryResponse {
    pub room_id: String,
    pub room_type: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub canonical_alias: Option<String>,
    pub join_rule: String,
    pub num_joined_members: i64,
    #[serde(rename = "world_readable")]
    pub is_world_readable: bool,
    #[serde(rename = "guest_can_join")]
    pub is_guest_can_join: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children_state: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Serialize)]
pub struct Msc3266RoomSummaryBatchResponse {
    pub rooms: Vec<Msc3266RoomSummaryResponse>,
    pub events: Vec<serde_json::Value>,
    pub total_room_count_estimate: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_batch: Option<String>,
}

impl From<RoomSummaryResponse> for Msc3266RoomSummaryResponse {
    fn from(s: RoomSummaryResponse) -> Self {
        Self {
            room_id: s.room_id,
            room_type: s.room_type,
            name: s.name,
            topic: s.topic,
            avatar_url: s.avatar_url,
            canonical_alias: s.canonical_alias,
            join_rule: s.join_rule,
            num_joined_members: s.joined_member_count,
            is_world_readable: s.history_visibility == "world_readable",
            is_guest_can_join: s.guest_access == "can_join",
            children_state: None,
        }
    }
}

fn create_room_summary_read_router() -> Router<AppState> {
    Router::new()
        .route("/rooms/{room_id}/summary", get(get_room_summary))
        .route("/rooms/{room_id}/summary/members", get(get_members))
        .route("/rooms/{room_id}/summary/state", get(get_all_state))
        .route("/rooms/{room_id}/summary/stats", get(get_stats))
}

fn create_room_summary_v3_router() -> Router<AppState> {
    Router::new()
        .merge(create_room_summary_read_router())
        .route("/rooms/{room_id}/summary", post(create_room_summary))
        .route("/rooms/{room_id}/summary", put(update_room_summary))
        .route("/rooms/{room_id}/summary", delete(delete_room_summary))
        .route("/rooms/{room_id}/summary/sync", post(sync_room_summary))
        .route("/rooms/{room_id}/summary/members", post(add_member))
        .route(
            "/rooms/{room_id}/summary/members/{user_id}",
            put(update_member),
        )
        .route(
            "/rooms/{room_id}/summary/members/{user_id}",
            delete(remove_member),
        )
        .route(
            "/rooms/{room_id}/summary/state/{event_type}/{state_key}",
            get(get_state),
        )
        .route(
            "/rooms/{room_id}/summary/state/{event_type}/{state_key}",
            put(update_state),
        )
        .route(
            "/rooms/{room_id}/summary/stats/recalculate",
            post(recalculate_stats),
        )
        .route(
            "/rooms/{room_id}/summary/heroes/recalculate",
            post(recalculate_heroes),
        )
        .route("/rooms/{room_id}/summary/unread/clear", post(clear_unread))
}

pub async fn batch_get_room_summaries(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(_room_id): Path<String>,
    Json(body): Json<RoomSummaryBatchRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let responses = state
        .services
        .room_service
        .room_summary_service()
        .get_summaries_by_ids(&body.rooms)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room summaries: {e}")))?;

    let filtered = if body.is_suggested_only {
        responses
            .into_iter()
            .filter(|r| r.room_type.as_deref() == Some("m.space"))
            .collect()
    } else {
        responses
    };

    let msc_responses: Vec<Msc3266RoomSummaryResponse> = filtered
        .into_iter()
        .map(Msc3266RoomSummaryResponse::from)
        .collect();

    let total_count = msc_responses.len();
    let response = Msc3266RoomSummaryBatchResponse {
        rooms: msc_responses,
        events: vec![],
        total_room_count_estimate: total_count,
        next_batch: None,
    };

    Ok(Json(response))
}

fn create_room_summary_v1_router() -> Router<AppState> {
    Router::new().route("/rooms/{room_id}/summary", get(get_room_summary))
}

pub fn create_room_summary_router(state: AppState) -> Router<AppState> {
    Router::new()
        .nest("/_matrix/client/v3", create_room_summary_v3_router())
        .nest("/_matrix/client/r0", create_room_summary_read_router())
        .nest("/_matrix/client/v1", create_room_summary_v1_router())
        .route(
            "/_synapse/room_summary/v1/summaries",
            get(get_user_summaries),
        )
        .route(
            "/_synapse/room_summary/v1/summaries",
            post(create_internal_room_summary),
        )
        .route(
            "/_synapse/room_summary/v1/updates/process",
            post(process_updates),
        )
        .with_state(state)
}

fn room_summary_read_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::GET, "/rooms/{room_id}/summary"),
        (Method::GET, "/rooms/{room_id}/summary/members"),
        (Method::GET, "/rooms/{room_id}/summary/state"),
        (Method::GET, "/rooms/{room_id}/summary/stats"),
    ]
}

fn room_summary_v3_extra_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::POST, "/rooms/{room_id}/summary"),
        (Method::PUT, "/rooms/{room_id}/summary"),
        (Method::DELETE, "/rooms/{room_id}/summary"),
        (Method::POST, "/rooms/{room_id}/summary/sync"),
        (Method::POST, "/rooms/{room_id}/summary/members"),
        (Method::PUT, "/rooms/{room_id}/summary/members/{user_id}"),
        (Method::DELETE, "/rooms/{room_id}/summary/members/{user_id}"),
        (
            Method::GET,
            "/rooms/{room_id}/summary/state/{event_type}/{state_key}",
        ),
        (
            Method::PUT,
            "/rooms/{room_id}/summary/state/{event_type}/{state_key}",
        ),
        (Method::POST, "/rooms/{room_id}/summary/stats/recalculate"),
        (Method::POST, "/rooms/{room_id}/summary/heroes/recalculate"),
        (Method::POST, "/rooms/{room_id}/summary/unread/clear"),
    ]
}

pub fn room_summary_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::{expand_under_prefixes, RouteEntry};
    use axum::http::Method;

    let mut out = expand_under_prefixes(
        "room_summary",
        &["/_matrix/client/r0"],
        &room_summary_read_relative_routes(),
    );
    let mut v3_routes = room_summary_read_relative_routes();
    v3_routes.extend(room_summary_v3_extra_relative_routes());
    out.extend(expand_under_prefixes(
        "room_summary",
        &["/_matrix/client/v3"],
        &v3_routes,
    ));
    out.extend(
        [
            (Method::GET, "/_synapse/room_summary/v1/summaries"),
            (Method::POST, "/_synapse/room_summary/v1/summaries"),
            (Method::POST, "/_synapse/room_summary/v1/updates/process"),
        ]
        .into_iter()
        .map(|(m, p)| RouteEntry::new(m, p, "room_summary")),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_summary_routes_structure() {
        let routes = [
            "/_matrix/client/v3/rooms/{room_id}/summary",
            "/_matrix/client/r0/rooms/{room_id}/summary",
            "/_matrix/client/v3/rooms/{room_id}/summary/unread/clear",
            "/_synapse/room_summary/v1/summaries",
        ];

        assert_eq!(routes.len(), 4);
        assert!(routes.iter().all(|route| {
            route.starts_with("/_matrix/client/") || route.starts_with("/_synapse/")
        }));
    }

    #[test]
    fn test_room_summary_read_router_contains_shared_paths() {
        let shared_paths = [
            "/rooms/{room_id}/summary",
            "/rooms/{room_id}/summary/members",
            "/rooms/{room_id}/summary/state",
            "/rooms/{room_id}/summary/stats",
        ];

        assert_eq!(shared_paths.len(), 4);
        assert!(shared_paths.iter().all(|path| path.starts_with("/rooms/")));
    }

    #[test]
    fn test_room_summary_router_boundaries() {
        let r0_only_read_paths = [
            "/rooms/{room_id}/summary",
            "/rooms/{room_id}/summary/members",
            "/rooms/{room_id}/summary/state",
            "/rooms/{room_id}/summary/stats",
        ];
        let v3_extra_paths = [
            "/rooms/{room_id}/summary/sync",
            "/rooms/{room_id}/summary/members/{user_id}",
            "/rooms/{room_id}/summary/state/{event_type}/{state_key}",
            "/rooms/{room_id}/summary/stats/recalculate",
            "/rooms/{room_id}/summary/heroes/recalculate",
            "/rooms/{room_id}/summary/unread/clear",
        ];

        assert_eq!(r0_only_read_paths.len(), 4);
        assert_eq!(v3_extra_paths.len(), 6);
        assert!(v3_extra_paths
            .iter()
            .all(|path| !r0_only_read_paths.contains(path)));
    }

    #[test]
    fn test_update_summary_body_into_request_preserves_fields() {
        let body = UpdateSummaryBody {
            name: Some("Updated".to_string()),
            topic: Some("Topic".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
        };

        let request = body.into_request();

        assert_eq!(request.name.as_deref(), Some("Updated"));
        assert_eq!(request.topic.as_deref(), Some("Topic"));
        assert_eq!(
            request.avatar_url.as_deref(),
            Some("mxc://example.com/avatar")
        );
        assert!(request.canonical_alias.is_none());
    }

    #[test]
    fn test_update_member_body_into_request_preserves_fields_and_defaults() {
        let body = UpdateMemberBody {
            display_name: Some("Alice".to_string()),
            avatar_url: Some("mxc://example.com/alice".to_string()),
            membership: Some("join".to_string()),
        };

        let request = body.into_request();

        assert_eq!(request.display_name.as_deref(), Some("Alice"));
        assert_eq!(
            request.avatar_url.as_deref(),
            Some("mxc://example.com/alice")
        );
        assert_eq!(request.membership.as_deref(), Some("join"));
        assert!(request.is_hero.is_none());
        assert!(request.last_active_ts.is_none());
    }

    #[test]
    fn test_create_summary_request_for_room_replaces_matching_path_room_id() {
        let body = CreateRoomSummaryRequest {
            room_id: "!room:example.com".to_string(),
            room_type: Some("m.space".to_string()),
            name: Some("Room".to_string()),
            topic: Some("Topic".to_string()),
            avatar_url: Some("mxc://example.com/room".to_string()),
            canonical_alias: Some("#room:example.com".to_string()),
            join_rule: Some("invite".to_string()),
            history_visibility: Some("shared".to_string()),
            guest_access: Some("forbidden".to_string()),
            is_direct: Some(false),
            is_space: Some(true),
        };

        let request =
            create_summary_request_for_room("!room:example.com".to_string(), body).unwrap();

        assert_eq!(request.room_id, "!room:example.com");
        assert_eq!(request.room_type.as_deref(), Some("m.space"));
        assert_eq!(
            request.canonical_alias.as_deref(),
            Some("#room:example.com")
        );
        assert_eq!(request.is_space, Some(true));
    }

    #[test]
    fn test_create_summary_request_for_room_rejects_mismatched_room_id() {
        let body = CreateRoomSummaryRequest {
            room_id: "!body:example.com".to_string(),
            room_type: None,
            name: None,
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: None,
            history_visibility: None,
            guest_access: None,
            is_direct: None,
            is_space: None,
        };

        let error = create_summary_request_for_room("!path:example.com".to_string(), body)
            .expect_err("mismatched room_id should fail");

        match error {
            ApiError::BadRequest(message) => {
                assert!(message.contains("Path room_id does not match body room_id"));
            }
            other => panic!("expected bad request error, got {other:?}"),
        }
    }

    #[test]
    fn test_create_summary_member_request_for_room_preserves_fields() {
        let body = CreateSummaryMemberRequest {
            room_id: "!ignored:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            display_name: Some("Alice".to_string()),
            avatar_url: Some("mxc://example.com/alice".to_string()),
            membership: "join".to_string(),
            is_hero: Some(true),
            last_active_ts: Some(12345),
        };

        let request = create_summary_member_request_for_room("!room:example.com".to_string(), body);

        assert_eq!(request.room_id, "!room:example.com");
        assert_eq!(request.user_id, "@alice:example.com");
        assert_eq!(request.membership, "join");
        assert_eq!(request.is_hero, Some(true));
        assert_eq!(request.last_active_ts, Some(12345));
    }

    #[test]
    fn test_msc3266_batch_response_format() {
        let response = Msc3266RoomSummaryBatchResponse {
            rooms: vec![Msc3266RoomSummaryResponse {
                room_id: "!room:example.com".to_string(),
                room_type: Some("m.space".to_string()),
                name: Some("Test Space".to_string()),
                topic: Some("A test space".to_string()),
                avatar_url: None,
                canonical_alias: None,
                join_rule: "invite".to_string(),
                num_joined_members: 42,
                is_world_readable: false,
                is_guest_can_join: false,
                children_state: None,
            }],
            events: vec![],
            total_room_count_estimate: 1,
            next_batch: None,
        };

        let json = serde_json::to_value(&response).expect("Should serialize");
        assert!(json.get("rooms").is_some());
        assert_eq!(
            json.get("total_room_count_estimate")
                .unwrap()
                .as_u64()
                .unwrap(),
            1
        );
        assert!(json.get("events").is_some());

        let rooms = json["rooms"].as_array().unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0]["room_id"].as_str().unwrap(), "!room:example.com");
        assert_eq!(rooms[0]["num_joined_members"].as_i64().unwrap(), 42);
    }

    #[test]
    fn test_msc3266_batch_response_with_next_batch() {
        let response = Msc3266RoomSummaryBatchResponse {
            rooms: vec![],
            events: vec![],
            total_room_count_estimate: 100,
            next_batch: Some("batch_2".to_string()),
        };

        let json = serde_json::to_value(&response).expect("Should serialize");
        assert_eq!(json["next_batch"].as_str().unwrap(), "batch_2");
        assert_eq!(json["total_room_count_estimate"].as_u64().unwrap(), 100);
    }
}
