use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
    Router,
    routing::{get, post, put, delete},
};
use serde::{Deserialize, Serialize};

use crate::common::ApiError;
use crate::storage::space::{CreateSpaceRequest, AddChildRequest, UpdateSpaceRequest};
use crate::web::routes::AuthenticatedUser;
use crate::web::routes::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateSpaceBody {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub join_rule: Option<String>,
    pub visibility: Option<String>,
    pub is_public: Option<bool>,
    pub parent_space_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddChildBody {
    pub room_id: String,
    pub via_servers: Vec<String>,
    pub order: Option<String>,
    pub suggested: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSpaceBody {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub join_rule: Option<String>,
    pub visibility: Option<String>,
    pub is_public: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct InviteUserBody {
    pub user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct HierarchyQuery {
    pub max_depth: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct SpaceResponse {
    pub space_id: String,
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub creator: String,
    pub join_rule: String,
    pub visibility: String,
    pub is_public: bool,
    pub creation_ts: i64,
    pub updated_ts: Option<i64>,
    pub parent_space_id: Option<String>,
}

impl From<crate::storage::space::Space> for SpaceResponse {
    fn from(space: crate::storage::space::Space) -> Self {
        Self {
            space_id: space.space_id,
            room_id: space.room_id,
            name: space.name,
            topic: space.topic,
            avatar_url: space.avatar_url,
            creator: space.creator,
            join_rule: space.join_rule,
            visibility: space.visibility,
            is_public: space.is_public,
            creation_ts: space.creation_ts,
            updated_ts: space.updated_ts,
            parent_space_id: space.parent_space_id,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SpaceChildResponse {
    pub space_id: String,
    pub room_id: String,
    pub via_servers: Vec<String>,
    pub order: Option<String>,
    pub suggested: bool,
    pub added_by: String,
    pub added_ts: i64,
}

impl From<crate::storage::space::SpaceChild> for SpaceChildResponse {
    fn from(child: crate::storage::space::SpaceChild) -> Self {
        Self {
            space_id: child.space_id,
            room_id: child.room_id,
            via_servers: child.via_servers,
            order: child.order,
            suggested: child.suggested,
            added_by: child.added_by,
            added_ts: child.added_ts,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SpaceMemberResponse {
    pub space_id: String,
    pub user_id: String,
    pub membership: String,
    pub joined_ts: i64,
    pub inviter: Option<String>,
}

impl From<crate::storage::space::SpaceMember> for SpaceMemberResponse {
    fn from(member: crate::storage::space::SpaceMember) -> Self {
        Self {
            space_id: member.space_id,
            user_id: member.user_id,
            membership: member.membership,
            joined_ts: member.joined_ts,
            inviter: member.inviter,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SpaceHierarchyResponse {
    pub space: SpaceResponse,
    pub children: Vec<SpaceChildResponse>,
    pub members: Vec<SpaceMemberResponse>,
}

pub async fn create_space(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateSpaceBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateSpaceRequest {
        room_id: body.room_id,
        name: body.name,
        topic: body.topic,
        avatar_url: body.avatar_url,
        creator: auth_user.user_id.clone(),
        join_rule: body.join_rule,
        visibility: body.visibility,
        is_public: body.is_public,
        parent_space_id: body.parent_space_id,
    };

    let space = state.services.space_service.create_space(request).await?;
    
    Ok((StatusCode::CREATED, Json(SpaceResponse::from(space))))
}

pub async fn get_space(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let space = state.services.space_service.get_space(&space_id).await?
        .ok_or_else(|| ApiError::not_found("Space not found"))?;
    
    Ok(Json(SpaceResponse::from(space)))
}

pub async fn get_space_by_room(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let space = state.services.space_service.get_space_by_room(&room_id).await?
        .ok_or_else(|| ApiError::not_found("Space not found for this room"))?;
    
    Ok(Json(SpaceResponse::from(space)))
}

pub async fn update_space(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: AuthenticatedUser,
    Json(body): Json<UpdateSpaceBody>,
) -> Result<impl IntoResponse, ApiError> {
    let mut request = UpdateSpaceRequest::new();
    
    if let Some(name) = body.name {
        request = request.name(name);
    }
    if let Some(topic) = body.topic {
        request = request.topic(topic);
    }
    if let Some(avatar_url) = body.avatar_url {
        request = request.avatar_url(avatar_url);
    }
    if let Some(join_rule) = body.join_rule {
        request = request.join_rule(join_rule);
    }
    if let Some(visibility) = body.visibility {
        request = request.visibility(visibility);
    }
    if let Some(is_public) = body.is_public {
        request = request.is_public(is_public);
    }
    
    let space = state.services.space_service.update_space(&space_id, &request, &auth_user.user_id).await?;
    
    Ok(Json(SpaceResponse::from(space)))
}

pub async fn delete_space(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    state.services.space_service.delete_space(&space_id, &auth_user.user_id).await?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn add_child(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: AuthenticatedUser,
    Json(body): Json<AddChildBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = AddChildRequest {
        space_id,
        room_id: body.room_id,
        via_servers: body.via_servers,
        order: body.order,
        suggested: body.suggested,
        added_by: auth_user.user_id.clone(),
    };

    let child = state.services.space_service.add_child(request).await?;
    
    Ok((StatusCode::CREATED, Json(SpaceChildResponse::from(child))))
}

pub async fn remove_child(
    State(state): State<AppState>,
    Path((space_id, room_id)): Path<(String, String)>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    state.services.space_service.remove_child(&space_id, &room_id, &auth_user.user_id).await?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_space_children(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let children = state.services.space_service.get_space_children(&space_id).await?;
    
    let response: Vec<SpaceChildResponse> = children.into_iter().map(SpaceChildResponse::from).collect();
    
    Ok(Json(response))
}

pub async fn get_space_members(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let members = state.services.space_service.get_space_members(&space_id).await?;
    
    let response: Vec<SpaceMemberResponse> = members.into_iter().map(SpaceMemberResponse::from).collect();
    
    Ok(Json(response))
}

pub async fn invite_user(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: AuthenticatedUser,
    Json(body): Json<InviteUserBody>,
) -> Result<impl IntoResponse, ApiError> {
    let member = state.services.space_service.invite_user(&space_id, &body.user_id, &auth_user.user_id).await?;
    
    Ok((StatusCode::CREATED, Json(SpaceMemberResponse::from(member))))
}

pub async fn join_space(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let member = state.services.space_service.join_space(&space_id, &auth_user.user_id).await?;
    
    Ok(Json(SpaceMemberResponse::from(member)))
}

pub async fn leave_space(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    state.services.space_service.leave_space(&space_id, &auth_user.user_id).await?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_user_spaces(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let spaces = state.services.space_service.get_user_spaces(&auth_user.user_id).await?;
    
    let response: Vec<SpaceResponse> = spaces.into_iter().map(SpaceResponse::from).collect();
    
    Ok(Json(response))
}

pub async fn get_public_spaces(
    State(state): State<AppState>,
    Query(query): Query<PaginationQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    
    let spaces = state.services.space_service.get_public_spaces(limit, offset).await?;
    
    let response: Vec<SpaceResponse> = spaces.into_iter().map(SpaceResponse::from).collect();
    
    Ok(Json(response))
}

pub async fn get_space_hierarchy(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    Query(query): Query<HierarchyQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let max_depth = query.max_depth.unwrap_or(1);
    
    let hierarchy = state.services.space_service.get_space_hierarchy(&space_id, max_depth).await?;
    
    let response = SpaceHierarchyResponse {
        space: SpaceResponse::from(hierarchy.space),
        children: hierarchy.children.into_iter().map(SpaceChildResponse::from).collect(),
        members: hierarchy.members.into_iter().map(SpaceMemberResponse::from).collect(),
    };
    
    Ok(Json(response))
}

pub async fn get_space_summary(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let summary = state.services.space_service.get_space_summary(&space_id).await?
        .ok_or_else(|| ApiError::not_found("Space summary not found"))?;
    
    Ok(Json(summary))
}

pub async fn search_spaces(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(10);
    
    let spaces = state.services.space_service.search_spaces(&query.query, limit).await?;
    
    let response: Vec<SpaceResponse> = spaces.into_iter().map(SpaceResponse::from).collect();
    
    Ok(Json(response))
}

pub async fn get_space_statistics(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let stats = state.services.space_service.get_space_statistics().await?;
    
    Ok(Json(stats))
}

#[derive(Debug, Deserialize)]
pub struct HierarchyV1Query {
    pub max_depth: Option<i32>,
    pub suggested_only: Option<bool>,
    pub limit: Option<i32>,
    pub from: Option<String>,
}

pub async fn get_space_hierarchy_v1(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    Query(query): Query<HierarchyV1Query>,
    user_id: Option<axum::extract::Extension<String>>,
) -> Result<impl IntoResponse, ApiError> {
    let max_depth = query.max_depth.unwrap_or(1);
    let suggested_only = query.suggested_only.unwrap_or(false);
    let user_id_str = user_id.as_ref().map(|u| u.0.as_str());

    let response = state.services.space_service
        .get_space_hierarchy_v1(&space_id, max_depth, suggested_only, query.limit, query.from.as_deref(), user_id_str)
        .await?;
    
    Ok(Json(response))
}

pub async fn get_parent_spaces(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let spaces = state.services.space_service.get_parent_spaces(&room_id).await?;
    
    let response: Vec<SpaceResponse> = spaces.into_iter().map(SpaceResponse::from).collect();
    
    Ok(Json(response))
}

pub async fn get_space_tree_path(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let path = state.services.space_service.get_space_tree_path(&space_id).await?;
    
    let response: Vec<SpaceResponse> = path.into_iter().map(SpaceResponse::from).collect();
    
    Ok(Json(response))
}

pub async fn get_space_summary_with_children(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    user_id: Option<axum::extract::Extension<String>>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id_str = user_id.as_ref().map(|u| u.0.as_str());
    
    let summary = state.services.space_service
        .get_space_summary_with_children(&space_id, user_id_str)
        .await?;
    
    Ok(Json(summary))
}

pub fn create_space_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v1/spaces", post(create_space))
        .route("/_matrix/client/v1/spaces/public", get(get_public_spaces))
        .route("/_matrix/client/v1/spaces/search", get(search_spaces))
        .route("/_matrix/client/v1/spaces/statistics", get(get_space_statistics))
        .route("/_matrix/client/v1/spaces/user", get(get_user_spaces))
        .route("/_matrix/client/v1/spaces/{space_id}", get(get_space))
        .route("/_matrix/client/v1/spaces/{space_id}", put(update_space))
        .route("/_matrix/client/v1/spaces/{space_id}", delete(delete_space))
        .route("/_matrix/client/v1/spaces/{space_id}/children", get(get_space_children))
        .route("/_matrix/client/v1/spaces/{space_id}/children", post(add_child))
        .route("/_matrix/client/v1/spaces/{space_id}/children/{room_id}", delete(remove_child))
        .route("/_matrix/client/v1/spaces/{space_id}/members", get(get_space_members))
        .route("/_matrix/client/v1/spaces/{space_id}/invite", post(invite_user))
        .route("/_matrix/client/v1/spaces/{space_id}/join", post(join_space))
        .route("/_matrix/client/v1/spaces/{space_id}/leave", post(leave_space))
        .route("/_matrix/client/v1/spaces/{space_id}/hierarchy", get(get_space_hierarchy))
        .route("/_matrix/client/v1/spaces/{space_id}/hierarchy/v1", get(get_space_hierarchy_v1))
        .route("/_matrix/client/v1/spaces/{space_id}/summary", get(get_space_summary))
        .route("/_matrix/client/v1/spaces/{space_id}/summary/with_children", get(get_space_summary_with_children))
        .route("/_matrix/client/v1/spaces/{space_id}/tree_path", get(get_space_tree_path))
        .route("/_matrix/client/v1/spaces/room/{room_id}", get(get_space_by_room))
        .route("/_matrix/client/v1/spaces/room/{room_id}/parents", get(get_parent_spaces))
        .with_state(state)
}
