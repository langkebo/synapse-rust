use crate::common::ApiError;
use crate::storage::sliding_sync::{
    SlidingSyncRequest, SlidingSyncResponse,
};
use crate::web::routes::{AppState, AuthenticatedUser};
use axum::{
    extract::State,
    routing::post,
    Json, Router,
};

/// Sliding Sync endpoint
/// Matrix MSC3575: https://github.com/matrix-org/matrix-spec-proposals/pull/3575
pub fn create_sliding_sync_router(_state: AppState) -> Router<AppState> {
    Router::new()
        // Main sliding sync endpoint (POST)
        .route("/_matrix/client/v3/sync", post(sliding_sync))
        // Alternative with sync key
        .route(
            "/_matrix/client/unstable/org.matrix.msc3575/sync",
            post(sliding_sync),
        )
}

#[axum::debug_handler]
async fn sliding_sync(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<SlidingSyncRequest>,
) -> Result<Json<SlidingSyncResponse>, ApiError> {
    tracing::debug!(
        "Sliding sync request from user: {}, pos: {:?}, lists: {:?}",
        auth_user.user_id,
        body.pos,
        body.lists
    );

    // Get device_id or use default
    let device_id = auth_user.device_id.unwrap_or_else(|| "default".to_string());

    // Call the sliding sync service
    let response = state
        .services
        .sliding_sync_service
        .sync(&auth_user.user_id, &device_id, body)
        .await?;

    Ok(Json(response))
}
