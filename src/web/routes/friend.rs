use super::AppState;
use crate::common::ApiError;
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn create_friend_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/enhanced/friends/search", get(search_users))
        .route("/_synapse/enhanced/friends", get(get_friends))
        .route(
            "/_synapse/enhanced/friend/request",
            post(send_friend_request),
        )
        .route(
            "/_synapse/enhanced/friend/requests",
            get(get_friend_requests),
        )
        .route(
            "/_synapse/enhanced/friend/request/{request_id}/accept",
            post(accept_friend_request),
        )
        .route(
            "/_synapse/enhanced/friend/request/{request_id}/decline",
            post(decline_friend_request),
        )
        .route(
            "/_synapse/enhanced/friend/blocks/{user_id}",
            get(get_blocked_users),
        )
        .route(
            "/_synapse/enhanced/friend/blocks/{user_id}",
            post(block_user),
        )
        .route(
            "/_synapse/enhanced/friend/blocks/{user_id}/{blocked_user_id}",
            delete(unblock_user),
        )
        .route(
            "/_synapse/enhanced/friend/categories/{user_id}",
            get(get_friend_categories),
        )
        .route(
            "/_synapse/enhanced/friend/categories/{user_id}",
            post(create_friend_category),
        )
        .route(
            "/_synapse/enhanced/friend/categories/{user_id}/{category_name}",
            put(update_friend_category),
        )
        .route(
            "/_synapse/enhanced/friend/categories/{user_id}/{category_name}",
            delete(delete_friend_category),
        )
        .route(
            "/_synapse/enhanced/friend/recommendations/{user_id}",
            get(get_friend_recommendations),
        )
}

#[derive(Debug, Deserialize)]
pub struct SendFriendRequestBody {
    pub user_id: String,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCategoryBody {
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BlockUserBody {
    pub user_id: String,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCategoryBody {
    pub name: Option<String>,
    pub color: Option<String>,
}

#[axum::debug_handler]
async fn get_friends(
    State(state): State<AppState>,
    auth_user: crate::web::routes::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    let friends = friend_storage
        .get_friends(&auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if friends.is_empty() {
        return Ok(Json(json!({
            "friends": [],
            "count": 0
        })));
    }

    let profiles = state
        .services
        .user_storage
        .get_user_profiles_batch(&friends)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let profile_map: HashMap<String, crate::storage::user::UserProfile> = profiles
        .into_iter()
        .map(|p| (p.user_id.clone(), p))
        .collect();

    let friend_list: Vec<Value> = friends
        .iter()
        .map(|friend_id| {
            if let Some(profile) = profile_map.get(friend_id) {
                json!({
                    "user_id": friend_id,
                    "display_name": profile.displayname,
                    "avatar_url": profile.avatar_url
                })
            } else {
                json!({ "user_id": friend_id })
            }
        })
        .collect();

    Ok(Json(json!({
        "friends": friend_list,
        "count": friend_list.len()
    })))
}

#[axum::debug_handler]
async fn send_friend_request(
    State(state): State<AppState>,
    auth_user: crate::web::routes::AuthenticatedUser,
    Json(body): Json<SendFriendRequestBody>,
) -> Result<Json<Value>, ApiError> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    let receiver_id = &body.user_id;

    if auth_user.user_id == *receiver_id {
        return Err(ApiError::bad_request(
            "Cannot send request to yourself".to_string(),
        ));
    }

    if friend_storage
        .is_friend(&auth_user.user_id, receiver_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::conflict("Already friends".to_string()));
    }

    if friend_storage
        .is_blocked(&auth_user.user_id, receiver_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::forbidden(
            "Cannot send request to this user".to_string(),
        ));
    }

    let request_id = friend_storage
        .create_request(&auth_user.user_id, receiver_id, body.message.as_deref())
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "request_id": request_id,
        "status": "pending"
    })))
}

#[axum::debug_handler]
async fn get_friend_requests(
    State(state): State<AppState>,
    auth_user: crate::web::routes::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    let requests = friend_storage
        .get_requests(&auth_user.user_id, "pending")
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if requests.is_empty() {
        return Ok(Json(json!({
            "requests": [],
            "count": 0
        })));
    }

    let sender_ids: Vec<String> = requests.iter().map(|req| req.sender_id.clone()).collect();
    let profiles = state
        .services
        .user_storage
        .get_user_profiles_batch(&sender_ids)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let profile_map: HashMap<String, crate::storage::user::UserProfile> = profiles
        .into_iter()
        .map(|p| (p.user_id.clone(), p))
        .collect();

    let request_list: Vec<Value> = requests
        .iter()
        .map(|req| {
            let profile = if let Some(p) = profile_map.get(&req.sender_id) {
                json!({
                    "user_id": req.sender_id,
                    "display_name": p.displayname,
                    "avatar_url": p.avatar_url
                })
            } else {
                json!({ "user_id": req.sender_id })
            };

            json!({
                "request_id": req.id,
                "sender": profile,
                "message": req.message,
                "created_ts": req.created_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "requests": request_list,
        "count": request_list.len()
    })))
}

#[axum::debug_handler]
async fn accept_friend_request(
    State(state): State<AppState>,
    Path(request_id): Path<i64>,
    auth_user: crate::web::routes::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    friend_storage
        .accept_request(request_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    Ok(Json(json!({ "status": "accepted" })))
}

#[axum::debug_handler]
async fn decline_friend_request(
    State(state): State<AppState>,
    Path(request_id): Path<i64>,
    auth_user: crate::web::routes::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    friend_storage
        .decline_request(request_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    Ok(Json(json!({ "status": "declined" })))
}

#[axum::debug_handler]
async fn get_blocked_users(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    auth_user: crate::web::routes::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "Cannot view blocked users for another user".to_string(),
        ));
    }
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    let blocked = friend_storage
        .get_blocked_users(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    Ok(Json(json!({ "blocked_users": blocked })))
}

#[axum::debug_handler]
async fn block_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    auth_user: crate::web::routes::AuthenticatedUser,
    Json(body): Json<BlockUserBody>,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "Cannot modify blocked users for another user".to_string(),
        ));
    }
    if user_id == body.user_id {
        return Err(ApiError::bad_request("Cannot block yourself".to_string()));
    }
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    friend_storage
        .block_user(&user_id, &body.user_id, body.reason.as_deref())
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    Ok(Json(json!({ "status": "blocked" })))
}

#[axum::debug_handler]
async fn unblock_user(
    State(state): State<AppState>,
    Path((user_id, blocked_user_id)): Path<(String, String)>,
    auth_user: crate::web::routes::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "Cannot modify blocked users for another user".to_string(),
        ));
    }
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    friend_storage
        .unblock_user(&user_id, &blocked_user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    Ok(Json(json!({ "status": "unblocked" })))
}

#[axum::debug_handler]
async fn get_friend_categories(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    auth_user: crate::web::routes::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "Cannot view categories for another user".to_string(),
        ));
    }
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    let categories = friend_storage
        .get_categories(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    Ok(Json(json!({ "categories": categories })))
}

#[axum::debug_handler]
async fn create_friend_category(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    auth_user: crate::web::routes::AuthenticatedUser,
    Json(body): Json<CreateCategoryBody>,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "Cannot create categories for another user".to_string(),
        ));
    }
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    let color = body.color.unwrap_or_else(|| "#000000".to_string());
    let category_id = friend_storage
        .create_category(&user_id, &body.name, &color)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    Ok(Json(json!({ "category_id": category_id })))
}

#[axum::debug_handler]
async fn update_friend_category(
    State(state): State<AppState>,
    Path((user_id, category_name)): Path<(String, String)>,
    auth_user: crate::web::routes::AuthenticatedUser,
    Json(body): Json<UpdateCategoryBody>,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "Cannot update categories for another user".to_string(),
        ));
    }
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    friend_storage
        .update_category_by_name(
            &user_id,
            &category_name,
            body.name.as_deref(),
            body.color.as_deref(),
        )
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    Ok(Json(
        json!({ "status": "updated", "category_name": category_name }),
    ))
}

#[axum::debug_handler]
async fn delete_friend_category(
    State(state): State<AppState>,
    Path((user_id, category_name)): Path<(String, String)>,
    auth_user: crate::web::routes::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "Cannot delete categories for another user".to_string(),
        ));
    }
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    friend_storage
        .delete_category_by_name(&user_id, &category_name)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    Ok(Json(
        json!({ "status": "deleted", "category_name": category_name }),
    ))
}

#[axum::debug_handler]
async fn get_friend_recommendations(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    auth_user: crate::web::routes::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "Cannot view recommendations for another user".to_string(),
        ));
    }
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    let recommendation_ids = friend_storage
        .get_recommendations(&user_id, 10)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let mut recommendations = Vec::new();

    for rec_id in recommendation_ids {
        let profile = state
            .services
            .registration_service
            .get_profile(&rec_id)
            .await
            .unwrap_or(json!({ "user_id": rec_id }));

        recommendations.push(json!({
            "user_id": rec_id,
            "display_name": profile["displayname"],
            "avatar_url": profile["avatar_url"],
            "reason": "Common rooms"
        }));
    }

    Ok(Json(json!({
        "recommendations": recommendations,
        "count": recommendations.len()
    })))
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub query: Option<String>,
    pub search_term: Option<String>,
    pub limit: Option<i64>,
}

#[axum::debug_handler]
async fn search_users(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
    auth_user: crate::web::routes::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let limit = params.limit.unwrap_or(20).min(100);
    let search_query = params.search_term.or(params.query).ok_or_else(|| {
        ApiError::bad_request("Missing 'search_term' or 'query' parameter".to_string())
    })?;

    let users = state
        .services
        .user_storage
        .search_users(&search_query, limit)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let mut results = Vec::new();
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);

    for user in users {
        if user.user_id == auth_user.user_id {
            continue;
        }

        let is_friend = friend_storage
            .is_friend(&auth_user.user_id, &user.user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let is_blocked = friend_storage
            .is_blocked(&auth_user.user_id, &user.user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let profile = json!({
            "user_id": user.user_id,
            "username": user.username,
            "display_name": user.displayname.unwrap_or_else(|| user.username.clone()),
            "avatar_url": user.avatar_url,
            "is_friend": is_friend,
            "is_blocked": is_blocked
        });
        results.push(profile);
    }

    Ok(Json(json!({
        "results": results,
        "count": results.len()
    })))
}
