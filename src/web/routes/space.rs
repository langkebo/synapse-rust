use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::future::Future;
use validator::Validate;

use crate::common::ApiError;
pub(super) use crate::web::routes::response_helpers::{
    created_json_from, json_from, json_vec_from,
};
use crate::web::routes::{AppState, AuthenticatedUser, OptionalAuthenticatedUser};

mod children_hierarchy;
mod lifecycle_query;
mod membership_state;
mod summary;
mod types;

use children_hierarchy::create_space_children_hierarchy_routes;
use lifecycle_query::create_space_lifecycle_query_routes;
use membership_state::create_space_membership_state_routes;
use summary::create_space_summary_routes;
pub(super) use types::*;

pub(super) async fn resolve_space_by_room(
    state: &AppState,
    space_room_id: &str,
) -> Result<crate::storage::space::Space, ApiError> {
    state
        .services
        .space_service
        .get_space_by_room(space_room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Space not found"))
}

pub(super) async fn resolve_space(
    state: &AppState,
    space_identifier: &str,
) -> Result<crate::storage::space::Space, ApiError> {
    if let Some(space) = state.services.space_service.get_space(space_identifier).await? {
        return Ok(space);
    }

    resolve_space_by_room(state, space_identifier).await
}

pub(super) async fn with_resolved_space<T, F, Fut>(
    state: AppState,
    space_room_id: String,
    operation: F,
) -> Result<T, ApiError>
where
    F: FnOnce(AppState, crate::storage::space::Space) -> Fut,
    Fut: Future<Output = Result<T, ApiError>>,
{
    let space = resolve_space(&state, &space_room_id).await?;
    operation(state, space).await
}

pub(super) async fn can_user_view_space(
    state: &AppState,
    space: &crate::storage::space::Space,
    auth_user: &OptionalAuthenticatedUser,
) -> Result<bool, ApiError> {
    if space.is_public {
        return Ok(true);
    }

    match auth_user.user_id.as_deref() {
        Some(user_id) => state
            .services
            .space_service
            .check_user_can_see_space(&space.space_id, user_id)
            .await,
        None => Ok(false),
    }
}

pub(super) async fn ensure_space_visible(
    state: &AppState,
    space: &crate::storage::space::Space,
    auth_user: &OptionalAuthenticatedUser,
) -> Result<(), ApiError> {
    if can_user_view_space(state, space, auth_user).await? {
        return Ok(());
    }

    if auth_user.user_id.is_some() {
        Err(ApiError::forbidden("User cannot access this space"))
    } else {
        Err(ApiError::unauthorized(
            "Authentication required for private spaces",
        ))
    }
}

pub(super) async fn with_visible_space<T, F, Fut>(
    state: AppState,
    space_room_id: String,
    auth_user: OptionalAuthenticatedUser,
    operation: F,
) -> Result<T, ApiError>
where
    F: FnOnce(AppState, crate::storage::space::Space, OptionalAuthenticatedUser) -> Fut,
    Fut: Future<Output = Result<T, ApiError>>,
{
    let space = resolve_space(&state, &space_room_id).await?;
    ensure_space_visible(&state, &space, &auth_user).await?;
    operation(state, space, auth_user).await
}

pub(super) fn validate_request<T>(request: &T) -> Result<(), ApiError>
where
    T: Validate,
{
    request
        .validate()
        .map_err(|e| ApiError::bad_request(format!("Validation error: {}", e)))
}

pub fn create_space_router(state: AppState) -> Router<AppState> {
    let router = Router::new()
        .merge(create_space_lifecycle_query_routes())
        .merge(create_space_children_hierarchy_routes())
        .merge(create_space_membership_state_routes())
        .merge(create_space_summary_routes());

    // Apply the same routes to v1, r0, and v3 client prefixes
    Router::new()
        .nest("/_matrix/client/v1", router.clone())
        .nest("/_matrix/client/r0", router.clone())
        .nest("/_matrix/client/v3", router)
        .with_state(state)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_space_routes_structure() {
        let routes = vec![
            "/_matrix/client/v1/spaces",
            "/_matrix/client/v1/spaces/{space_id}",
            "/_matrix/client/v1/spaces/{space_id}/hierarchy",
            "/_matrix/client/v1/spaces/{space_id}/summary",
        ];

        for route in routes {
            assert!(route.starts_with("/_matrix/client/v1/spaces"));
        }
    }
}
