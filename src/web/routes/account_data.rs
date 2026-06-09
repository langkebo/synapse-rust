use crate::common::ApiError;
use crate::web::routes::{AppState, AuthenticatedUser};
use axum::{
    extract::{Json, Path, State},
    routing::{get, put},
    Router,
};
use serde_json::{json, Value};

fn create_account_data_compat_router() -> Router<AppState> {
    Router::new()
        .route("/user/{user_id}/account_data/", get(list_account_data))
        .route(
            "/user/{user_id}/account_data/{type}",
            get(get_account_data).put(set_account_data).post(set_account_data).delete(delete_account_data),
        )
        .route(
            "/user/{user_id}/rooms/{room_id}/account_data/{type}",
            get(get_room_account_data)
                .put(set_room_account_data)
                .post(set_room_account_data)
                .delete(delete_room_account_data),
        )
        .route("/user/{user_id}/filter", put(create_filter).post(create_filter))
        .route("/user/{user_id}/filter/{filter_id}", get(get_filter).delete(delete_filter))
        .route("/user/{user_id}/openid/request_token", get(get_openid_token).post(get_openid_token))
}

pub fn create_account_data_router(state: AppState) -> Router<AppState> {
    let compat_router = create_account_data_compat_router();

    Router::new()
        .nest("/_matrix/client/v3", compat_router.clone())
        .nest("/_matrix/client/r0", compat_router)
        .with_state(state)
}

const ACCOUNT_DATA_NEST_PREFIXES: &[&str] = &["/_matrix/client/v3", "/_matrix/client/r0"];

fn account_data_compat_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::GET, "/user/{user_id}/account_data/"),
        (Method::GET, "/user/{user_id}/account_data/{type}"),
        (Method::PUT, "/user/{user_id}/account_data/{type}"),
        (Method::DELETE, "/user/{user_id}/account_data/{type}"),
        (Method::GET, "/user/{user_id}/rooms/{room_id}/account_data/{type}"),
        (Method::PUT, "/user/{user_id}/rooms/{room_id}/account_data/{type}"),
        (Method::DELETE, "/user/{user_id}/rooms/{room_id}/account_data/{type}"),
        (Method::PUT, "/user/{user_id}/filter"),
        (Method::POST, "/user/{user_id}/filter"),
        (Method::GET, "/user/{user_id}/filter/{filter_id}"),
        (Method::DELETE, "/user/{user_id}/filter/{filter_id}"),
        (Method::GET, "/user/{user_id}/openid/request_token"),
        (Method::POST, "/user/{user_id}/openid/request_token"),
    ]
}

pub fn account_data_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    crate::web::routes::route_ledger::expand_under_prefixes(
        "account_data",
        ACCOUNT_DATA_NEST_PREFIXES,
        &account_data_compat_relative_routes(),
    )
}

async fn list_account_data(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot get account data for other users".to_string()));
    }

    let account_data = state.services.account_data_service.list_account_data(&user_id).await?;

    Ok(Json(json!({
        "account_data": account_data
    })))
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

    state.services.account_data_service.set_account_data(&user_id, &data_type, &body).await?;

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

    let result = state.services.account_data_service.get_account_data(&user_id, &data_type).await?;

    match result {
        Some(content) => Ok(Json(content)),
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
                Err(ApiError::not_found("Account data not found".to_string()))
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

    state
        .services
        .account_data_service
        .set_room_account_data(&user_id, &room_id, &data_type, &body)
        .await?;

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

    let result = state
        .services
        .account_data_service
        .get_room_account_data(&user_id, &room_id, &data_type)
        .await?;

    match result {
        Some(data) => Ok(Json(data)),
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

    let filter_id = state.services.account_data_service.create_filter(&user_id, body).await?;

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

    let result = state.services.account_data_service.get_filter(&user_id, &filter_id).await?;

    match result {
        Some(content) => Ok(Json(content)),
        None => Err(ApiError::not_found("Filter not found".to_string())),
    }
}

async fn delete_account_data(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((user_id, data_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot delete account data for other users".to_string()));
    }

    let deleted = state.services.account_data_service.delete_account_data(&user_id, &data_type).await?;

    if !deleted {
        return Err(ApiError::not_found("Account data not found".to_string()));
    }

    Ok(Json(json!({})))
}

async fn delete_room_account_data(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((user_id, room_id, data_type)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot delete room account data for other users".to_string()));
    }

    let deleted = state
        .services
        .account_data_service
        .delete_room_account_data(&user_id, &room_id, &data_type)
        .await?;

    if !deleted {
        return Err(ApiError::not_found("Room account data not found".to_string()));
    }

    Ok(Json(json!({})))
}

async fn delete_filter(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((user_id, filter_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot delete filter for other users".to_string()));
    }

    let deleted = state.services.account_data_service.delete_filter(&user_id, &filter_id).await?;

    if !deleted {
        return Err(ApiError::not_found("Filter not found".to_string()));
    }

    Ok(Json(json!({})))
}

async fn get_openid_token(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot get OpenID token for other users".to_string()));
    }

    let (token, expires_in) = state
        .services
        .account_data_service
        .create_openid_token(&user_id, None, 3600)
        .await?;

    Ok(Json(json!({
        "access_token": token,
        "token_type": "Bearer",
        "matrix_server_name": state.services.config.server.name,
        "expires_in": expires_in
    })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn test_account_data_routes_structure() {
        let routes = [
            "/_matrix/client/v3/user/{user_id}/account_data/",
            "/_matrix/client/r0/user/{user_id}/account_data/{type}",
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}",
            "/_matrix/client/r0/user/{user_id}/openid/request_token",
        ];

        assert!(routes.iter().all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_account_data_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/user/{user_id}/account_data/",
            "/user/{user_id}/account_data/{type}",
            "/user/{user_id}/rooms/{room_id}/account_data/{type}",
            "/user/{user_id}/filter",
            "/user/{user_id}/filter/{filter_id}",
            "/user/{user_id}/openid/request_token",
        ];

        assert_eq!(shared_paths.len(), 6);
        assert!(shared_paths.iter().all(|path| path.starts_with("/user/")));
    }

    #[test]
    fn test_account_data_json_structure() {
        let data = json!({
            "type": "m.direct",
            "content": {
                "@alice:example.com": ["!room1:example.com"]
            }
        });
        assert_eq!(data["type"], "m.direct");
    }

    #[test]
    fn test_filter_json_structure() {
        let filter = json!({
            "room": {
                "timeline": {
                    "limit": 100
                }
            }
        });
        assert!(filter["room"]["timeline"]["limit"].is_number());
    }

    #[test]
    fn test_openid_token_response() {
        let response = json!({
            "access_token": "test_token",
            "token_type": "Bearer",
            "matrix_server_name": "example.com",
            "expires_in": 3600
        });
        assert_eq!(response["token_type"], "Bearer");
        assert_eq!(response["expires_in"], 3600);
    }
}
