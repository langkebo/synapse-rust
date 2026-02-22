use crate::common::ApiError;
use crate::web::routes::{AppState, AuthenticatedUser};
use axum::{
    extract::{Json, Path, State},
    routing::{get, put},
    Router,
};
use serde_json::{json, Value};
use sqlx::Row;

pub fn create_account_data_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/v3/user/{user_id}/account_data/{type}",
            put(set_account_data),
        )
        .route(
            "/_matrix/client/v3/user/{user_id}/account_data/{type}",
            get(get_account_data),
        )
        .route(
            "/_matrix/client/r0/user/{user_id}/account_data/{type}",
            put(set_account_data),
        )
        .route(
            "/_matrix/client/r0/user/{user_id}/account_data/{type}",
            get(get_account_data),
        )
        .route(
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}",
            put(set_room_account_data),
        )
        .route(
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}",
            get(get_room_account_data),
        )
        .route(
            "/_matrix/client/r0/user/{user_id}/rooms/{room_id}/account_data/{type}",
            put(set_room_account_data),
        )
        .route(
            "/_matrix/client/r0/user/{user_id}/rooms/{room_id}/account_data/{type}",
            get(get_room_account_data),
        )
        .route(
            "/_matrix/client/v3/user/{user_id}/filter",
            put(create_filter),
        )
        .route(
            "/_matrix/client/v3/user/{user_id}/filter/{filter_id}",
            get(get_filter),
        )
        .route(
            "/_matrix/client/r0/user/{user_id}/filter",
            put(create_filter),
        )
        .route(
            "/_matrix/client/r0/user/{user_id}/filter/{filter_id}",
            get(get_filter),
        )
        .route(
            "/_matrix/client/v3/user/{user_id}/openid/request_token",
            get(get_openid_token),
        )
        .route(
            "/_matrix/client/r0/user/{user_id}/openid/request_token",
            get(get_openid_token),
        )
        .with_state(state)
}

async fn set_account_data(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((user_id, data_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot set account data for other users".to_string()));
    }

    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"
        INSERT INTO account_data (user_id, data_type, content, updated_at)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (user_id, data_type) DO UPDATE SET content = $3, updated_at = $4
        "#
    )
    .bind(&user_id)
    .bind(&data_type)
    .bind(&body)
    .bind(now)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to save account data: {}", e)))?;

    Ok(Json(json!({})))
}

async fn get_account_data(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((user_id, data_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot get account data for other users".to_string()));
    }

    let result = sqlx::query(
        "SELECT content FROM account_data WHERE user_id = $1 AND data_type = $2"
    )
    .bind(&user_id)
    .bind(&data_type)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match result {
        Some(row) => Ok(Json(row.get::<Option<Value>, _>("content").unwrap_or(json!({})))),
        None => {
            if data_type == "m.push_rules" {
                Ok(Json(json!({
                    "global": {
                        "content": [],
                        "override": [],
                        "room": [],
                        "sender": [],
                        "underride": []
                    }
                })))
            } else {
                Ok(Json(json!({})))
            }
        }
    }
}

async fn set_room_account_data(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((user_id, room_id, data_type)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot set account data for other users".to_string()));
    }

    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"
        INSERT INTO room_account_data (user_id, room_id, data_type, content, updated_at)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (user_id, room_id, data_type) DO UPDATE SET content = $4, updated_at = $5
        "#
    )
    .bind(&user_id)
    .bind(&room_id)
    .bind(&data_type)
    .bind(&body)
    .bind(now)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to save room account data: {}", e)))?;

    Ok(Json(json!({})))
}

async fn get_room_account_data(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((user_id, room_id, data_type)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot get account data for other users".to_string()));
    }

    let result = sqlx::query(
        "SELECT content FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = $3"
    )
    .bind(&user_id)
    .bind(&room_id)
    .bind(&data_type)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match result {
        Some(row) => Ok(Json(row.get::<Option<Value>, _>("content").unwrap_or(json!({})))),
        None => Err(ApiError::not_found("Room account data not found".to_string())),
    }
}

async fn create_filter(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot create filter for other users".to_string()));
    }

    let filter_id = crate::common::random_string(16);
    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"
        INSERT INTO filters (filter_id, user_id, content, created_at)
        VALUES ($1, $2, $3, $4)
        "#
    )
    .bind(&filter_id)
    .bind(&user_id)
    .bind(&body)
    .bind(now)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to save filter: {}", e)))?;

    Ok(Json(json!({
        "filter_id": filter_id
    })))
}

async fn get_filter(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((user_id, filter_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot get filter for other users".to_string()));
    }

    let result = sqlx::query(
        "SELECT content FROM filters WHERE filter_id = $1 AND user_id = $2"
    )
    .bind(&filter_id)
    .bind(&user_id)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match result {
        Some(row) => Ok(Json(row.get::<Option<Value>, _>("content").unwrap_or(json!({})))),
        None => Err(ApiError::not_found("Filter not found".to_string())),
    }
}

async fn get_openid_token(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot get OpenID token for other users".to_string()));
    }

    let token = crate::common::random_string(32);
    let expires_in = 3600;
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        r#"
        INSERT INTO openid_tokens (token, user_id, created_at, expires_at)
        VALUES ($1, $2, $3, $4)
        "#
    )
    .bind(&token)
    .bind(&user_id)
    .bind(now)
    .bind(now + expires_in)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to create OpenID token: {}", e)))?;

    Ok(Json(json!({
        "access_token": token,
        "token_type": "Bearer",
        "matrix_server_name": state.services.config.server.name,
        "expires_in": expires_in
    })))
}
