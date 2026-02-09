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
use validator::{Validate, ValidationError};

// HP-5 FIX: Efficient error pattern matching using pre-compiled regex
use once_cell::sync::Lazy;

static UNIQUE_VIOLATION_PATTERNS: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"(?i)(duplicate key|unique constraint|23505|duplicatekeyvalue|duplicate_key|violates unique constraint)")
        .expect("Unique violation regex pattern is invalid")
});

static FOREIGN_KEY_PATTERNS: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"(?i)(foreign key constraint|23503)")
        .expect("Foreign key violation regex pattern is invalid")
});

/// Check if a database error indicates a unique constraint violation
fn is_unique_violation_error(error_msg: &str) -> bool {
    UNIQUE_VIOLATION_PATTERNS.is_match(error_msg)
}

/// Check if a database error indicates a foreign key constraint violation
fn is_foreign_key_violation_error(error_msg: &str) -> bool {
    FOREIGN_KEY_PATTERNS.is_match(error_msg)
}

pub fn create_friend_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/enhanced/friends/search", get(search_users))
        .route("/_synapse/enhanced/friends", get(get_friends))
        .route("/_synapse/enhanced/friends/batch", post(get_friends_batch))
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
}

fn validate_color(color: &str) -> Result<(), ValidationError> {
    if !color.starts_with('#') {
        return Err(ValidationError::new("Color must start with #"));
    }
    if color.len() != 7 {
        return Err(ValidationError::new("Color must be 7 characters long"));
    }
    if !color.chars().skip(1).all(|c| c.is_ascii_hexdigit()) {
        return Err(ValidationError::new("Color must be valid hex"));
    }
    Ok(())
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct SendFriendRequestBody {
    #[serde(alias = "userId", alias = "user_id")]
    #[validate(length(min = 1, max = 255, message = "User ID must be between 1 and 255 characters"))]
    pub user_id: String,
    #[serde(alias = "message")]
    #[validate(length(max = 500, message = "Message must not exceed 500 characters"))]
    pub message: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateCategoryBody {
    #[validate(length(min = 1, max = 50, message = "Category name must be between 1 and 50 characters"))]
    pub name: String,
    #[validate(custom(function = "validate_color"))]
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct BlockUserBody {
    #[validate(length(min = 1, max = 255))]
    pub user_id: String,
    #[validate(length(max = 200))]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateCategoryBody {
    #[validate(length(min = 1, max = 50))]
    pub name: Option<String>,
    #[validate(custom(function = "validate_color"))]
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct BatchGetFriendsBody {
    #[validate(length(min = 1, message = "Must provide at least one user ID"))]
    pub user_ids: Vec<String>,
}

#[axum::debug_handler]
async fn get_friends_batch(
    State(state): State<AppState>,
    auth_user: crate::web::routes::AuthenticatedUser,
    Json(body): Json<BatchGetFriendsBody>,
) -> Result<Json<Value>, ApiError> {
    if let Err(e) = body.validate() {
        return Err(ApiError::bad_request(e.to_string()));
    }
    
    let friend_service = crate::services::FriendService::new(&state.services, &state.services.user_storage.pool);
    let friends_data = friend_service
        .get_friends_batch(&auth_user.user_id, body.user_ids)
        .await?;

    Ok(Json(json!({
        "friends": friends_data
    })))
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
    if let Err(e) = body.validate() {
        return Err(ApiError::bad_request(e.to_string()));
    }
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
        let friend = friend_storage
            .get_friendship(&auth_user.user_id, receiver_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if let Some(friendship) = friend {
            return Ok(Json(json!({
                "status": "already_friends",
                "friend": friendship,
            })));
        }

        return Err(ApiError::bad_request("Friendship not found".to_string()));
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
    Path(request_id): Path<String>,
    auth_user: crate::web::routes::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    // CRITICAL FIX: Simplified and safe parsing
    let request_id_i64: i64 = request_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid request ID format".to_string()))?;

    if request_id_i64 <= 0 {
        return Err(ApiError::bad_request(
            "Invalid request ID format".to_string(),
        ));
    }

    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    friend_storage
        .accept_request(request_id_i64, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    Ok(Json(json!({ "status": "accepted" })))
}

#[axum::debug_handler]
async fn decline_friend_request(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
    auth_user: crate::web::routes::AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    // CRITICAL FIX: Simplified and safe parsing
    let request_id_i64: i64 = request_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid request ID format".to_string()))?;

    if request_id_i64 <= 0 {
        return Err(ApiError::bad_request(
            "Invalid request ID format".to_string(),
        ));
    }

    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    friend_storage
        .decline_request(request_id_i64, &auth_user.user_id)
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
    if let Err(e) = body.validate() {
        return Err(ApiError::bad_request(e.to_string()));
    }
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
    if let Err(e) = body.validate() {
        return Err(ApiError::bad_request(e.to_string()));
    }
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
    if let Err(e) = body.validate() {
        return Err(ApiError::bad_request(e.to_string()));
    }
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "Cannot update categories for another user".to_string(),
        ));
    }

    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);

    let new_name = body.name.as_deref();
    let new_color = body.color.as_deref();

    if let Some(name) = new_name {
        if name != category_name {
            let categories = friend_storage
                .get_categories(&user_id)
                .await
                .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

            if categories.iter().any(|c| c.name == *name) {
                return Err(ApiError::bad_request(format!(
                    "Category '{}' already exists. Please use a different name.",
                    name
                )));
            }
        }
    }

    friend_storage
        .update_category_by_name(&user_id, &category_name, new_name, new_color)
        .await
        .map_err(|e| {
            let error_msg = format!("{:?}", e);

            // HP-5 FIX: Use efficient regex matching instead of multiple contains
            if is_unique_violation_error(&error_msg) {
                ApiError::bad_request(format!(
                    "Category '{}' already exists. Please choose a different name.",
                    new_name.unwrap_or(&category_name)
                ))
            } else if is_foreign_key_violation_error(&error_msg) {
                ApiError::bad_request(
                    "Cannot update category: referenced data not found".to_string(),
                )
            } else {
                ApiError::internal(format!("Failed to update category: {}", e))
            }
        })?;

    Ok(Json(json!({
        "status": "updated",
        "category_name": category_name,
        "message": "Category updated successfully"
    })))
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

    // HP-1 FIX: Use batch queries to avoid N+1 problem
    let user_ids: Vec<String> = users.iter().map(|u| u.user_id.clone()).collect();
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);

    // Single batch query for all friendship checks
    let friend_set = friend_storage
        .batch_check_friends(&auth_user.user_id, &user_ids)
        .await
        .map_err(|_e| ApiError::internal("Database error: failed to check friendships"))?;

    // Single batch query for all blocked checks
    let blocked_set = friend_storage
        .batch_check_blocked(&auth_user.user_id, &user_ids)
        .await
        .map_err(|_e| ApiError::internal("Database error: failed to check blocked users"))?;

    let mut results = Vec::new();
    for user in users {
        if user.user_id == auth_user.user_id {
            continue;
        }

        let is_friend = friend_set.contains(&user.user_id);
        let is_blocked = blocked_set.contains(&user.user_id);

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
