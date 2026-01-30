use super::AppState;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn create_friend_router(state: AppState) -> Router {
    Router::new()
        .route("/_synapse/enhanced/friends/{user_id}", get(get_friends))
        .route(
            "/_synapse/enhanced/friend/request/{user_id}",
            post(send_friend_request),
        )
        .route(
            "/_synapse/enhanced/friend/requests/{user_id}",
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
            "/_synapse/enhanced/friend/blocks/{user_id}",
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
        .with_state(state)
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
    Path(user_id): Path<String>,
) -> Result<Json<Value>, String> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    let friends = friend_storage
        .get_friends(&user_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?;

    let mut friend_list = Vec::new();
    for friend_id in friends {
        let profile = json!({
            "user_id": friend_id
        });
        friend_list.push(profile);
    }

    Ok(Json(json!({
        "friends": friend_list,
        "count": friend_list.len()
    })))
}

#[axum::debug_handler]
async fn send_friend_request(
    State(state): State<AppState>,
    Path(sender_id): Path<String>,
    Json(body): Json<SendFriendRequestBody>,
) -> Result<Json<Value>, String> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    let receiver_id = &body.user_id;

    if sender_id == *receiver_id {
        return Err("Cannot send request to yourself".to_string());
    }

    if friend_storage
        .is_friend(&sender_id, receiver_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?
    {
        return Err("Already friends".to_string());
    }

    if friend_storage
        .is_blocked(&sender_id, receiver_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?
    {
        return Err("Cannot send request to this user".to_string());
    }

    let request_id = friend_storage
        .create_request(&sender_id, receiver_id, body.message.as_deref())
        .await
        .map_err(|e| format!("Database error: {}", e))?;

    Ok(Json(json!({
        "request_id": request_id,
        "status": "pending"
    })))
}

#[axum::debug_handler]
async fn get_friend_requests(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, String> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    let requests = friend_storage
        .get_requests(&user_id, "pending")
        .await
        .map_err(|e| format!("Database error: {}", e))?;

    let mut request_list = Vec::new();
    for req in requests {
        let profile = json!({ "user_id": req.sender_id });
        request_list.push(json!({
            "request_id": req.id,
            "sender": profile,
            "message": req.message,
            "created_ts": req.created_ts
        }));
    }

    Ok(Json(json!({
        "requests": request_list,
        "count": request_list.len()
    })))
}

#[axum::debug_handler]
async fn accept_friend_request(
    State(state): State<AppState>,
    Path(request_id): Path<i64>,
) -> Result<Json<Value>, String> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    friend_storage
        .accept_request(request_id, "")
        .await
        .map_err(|e| format!("Database error: {}", e))?;
    Ok(Json(json!({ "status": "accepted" })))
}

#[axum::debug_handler]
async fn decline_friend_request(
    State(state): State<AppState>,
    Path(request_id): Path<i64>,
) -> Result<Json<Value>, String> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    friend_storage
        .decline_request(request_id, "")
        .await
        .map_err(|e| format!("Database error: {}", e))?;
    Ok(Json(json!({ "status": "declined" })))
}

#[axum::debug_handler]
async fn get_blocked_users(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, String> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    let blocked = friend_storage
        .get_blocked_users(&user_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?;
    Ok(Json(json!({ "blocked_users": blocked })))
}

#[axum::debug_handler]
async fn block_user(
    State(state): State<AppState>,
    Json(body): Json<BlockUserBody>,
) -> Result<Json<Value>, String> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    friend_storage
        .block_user(&body.user_id, &body.user_id, body.reason.as_deref())
        .await
        .map_err(|e| format!("Database error: {}", e))?;
    Ok(Json(json!({ "status": "blocked" })))
}

#[axum::debug_handler]
async fn unblock_user(
    State(state): State<AppState>,
    Path((user_id, blocked_user_id)): Path<(String, String)>,
) -> Result<Json<Value>, String> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    friend_storage
        .unblock_user(&user_id, &blocked_user_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?;
    Ok(Json(json!({ "status": "unblocked" })))
}

#[axum::debug_handler]
async fn get_friend_categories(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, String> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    let categories = friend_storage
        .get_categories(&user_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?;
    Ok(Json(json!({ "categories": categories })))
}

#[axum::debug_handler]
async fn create_friend_category(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<CreateCategoryBody>,
) -> Result<Json<Value>, String> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    let color = body.color.unwrap_or_else(|| "#000000".to_string());
    let category_id = friend_storage
        .create_category(&user_id, &body.name, &color)
        .await
        .map_err(|e| format!("Database error: {}", e))?;
    Ok(Json(json!({ "category_id": category_id })))
}

#[axum::debug_handler]
async fn update_friend_category(
    State(state): State<AppState>,
    Path((user_id, category_name)): Path<(String, String)>,
    Json(body): Json<UpdateCategoryBody>,
) -> Result<Json<Value>, String> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    friend_storage
        .update_category_by_name(
            &user_id,
            &category_name,
            body.name.as_deref(),
            body.color.as_deref(),
        )
        .await
        .map_err(|e| format!("Database error: {}", e))?;
    Ok(Json(
        json!({ "status": "updated", "category_name": category_name }),
    ))
}

#[axum::debug_handler]
async fn delete_friend_category(
    State(state): State<AppState>,
    Path((user_id, category_name)): Path<(String, String)>,
) -> Result<Json<Value>, String> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    friend_storage
        .delete_category_by_name(&user_id, &category_name)
        .await
        .map_err(|e| format!("Database error: {}", e))?;
    Ok(Json(
        json!({ "status": "deleted", "category_name": category_name }),
    ))
}

#[axum::debug_handler]
async fn get_friend_recommendations(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, String> {
    let friend_storage = crate::services::FriendStorage::new(&state.services.user_storage.pool);
    let _friends = friend_storage
        .get_friends(&user_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?;

    let recommendations = vec![
        json!({
            "user_id": "@alice:localhost",
            "display_name": "Alice",
            "reason": "Same room members"
        }),
        json!({
            "user_id": "@bob:localhost",
            "display_name": "Bob",
            "reason": "Popular user"
        }),
    ];

    Ok(Json(json!({
        "recommendations": recommendations,
        "count": recommendations.len()
    })))
}
