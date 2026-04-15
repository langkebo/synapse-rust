use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::common::ApiError;
use crate::storage::application_service::{
    ApplicationService, ApplicationServiceState, ApplicationServiceUser,
    RegisterApplicationServiceRequest, UpdateApplicationServiceRequest,
};
use crate::web::routes::response_helpers::{
    created_json_from, empty_json, json_from, json_vec_from, require_found,
};
use crate::web::routes::{AdminUser, AppState, AuthenticatedUser};

#[derive(Debug, Deserialize)]
pub struct RegisterAppServiceBody {
    pub id: String,
    pub url: String,
    pub as_token: String,
    pub hs_token: String,
    pub sender: Option<String>,
    pub sender_localpart: Option<String>,
    pub description: Option<String>,
    pub rate_limited: Option<bool>,
    pub protocols: Option<Vec<String>>,
    pub namespaces: Option<serde_json::Value>,
}

impl RegisterAppServiceBody {
    fn into_request(self) -> Result<RegisterApplicationServiceRequest, ApiError> {
        let sender = self
            .sender
            .or(self.sender_localpart)
            .ok_or_else(|| ApiError::bad_request("Missing sender or sender_localpart"))?;

        Ok(RegisterApplicationServiceRequest {
            as_id: self.id,
            url: self.url,
            as_token: self.as_token,
            hs_token: self.hs_token,
            sender,
            description: self.description,
            rate_limited: self.rate_limited,
            protocols: self.protocols,
            namespaces: self.namespaces,
            api_key: None,
            config: None,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateAppServiceBody {
    pub url: Option<String>,
    pub description: Option<String>,
    pub rate_limited: Option<bool>,
    pub protocols: Option<Vec<String>>,
    pub is_enabled: Option<bool>,
}

impl UpdateAppServiceBody {
    fn into_request(self) -> UpdateApplicationServiceRequest {
        let mut request = UpdateApplicationServiceRequest::new();

        if let Some(url) = self.url {
            request = request.url(url);
        }
        if let Some(description) = self.description {
            request = request.description(description);
        }
        if let Some(rate_limited) = self.rate_limited {
            request = request.rate_limited(rate_limited);
        }
        if let Some(protocols) = self.protocols {
            request = request.protocols(protocols);
        }
        if let Some(is_enabled) = self.is_enabled {
            request = request.is_enabled(is_enabled);
        }

        request
    }
}

#[derive(Debug, Deserialize)]
pub struct SetStateBody {
    pub state_key: String,
    pub state_value: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterVirtualUserBody {
    pub user_id: String,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PushEventBody {
    pub room_id: String,
    pub event_type: String,
    pub sender: String,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct QueryLimit {
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct QueryUser {
    pub user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct QueryAlias {
    pub alias: String,
}

#[derive(Debug, Serialize)]
pub struct AppServiceResponse {
    pub id: i64,
    pub as_id: String,
    pub url: String,
    pub sender: String,
    pub description: Option<String>,
    pub rate_limited: bool,
    pub protocols: Vec<String>,
    pub is_enabled: bool,
    pub created_ts: i64,
}

impl From<ApplicationService> for AppServiceResponse {
    fn from(svc: ApplicationService) -> Self {
        Self {
            id: svc.id,
            as_id: svc.as_id,
            url: svc.url,
            sender: svc.sender_localpart,
            description: svc.description,
            rate_limited: svc.rate_limited,
            protocols: svc.protocols,
            is_enabled: svc.is_enabled,
            created_ts: svc.created_ts,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct VirtualUserResponse {
    pub as_id: String,
    pub user_id: String,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub created_ts: i64,
}

impl From<ApplicationServiceUser> for VirtualUserResponse {
    fn from(user: ApplicationServiceUser) -> Self {
        Self {
            as_id: user.as_id,
            user_id: user.user_id,
            displayname: user.displayname,
            avatar_url: user.avatar_url,
            created_ts: user.created_ts,
        }
    }
}

fn app_service_state_json(state_entry: ApplicationServiceState) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "as_id": state_entry.as_id,
        "state_key": state_entry.state_key,
        "state_value": state_entry.state_value,
        "updated_ts": state_entry.updated_ts
    }))
}

fn extract_as_token(headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .ok_or_else(|| ApiError::unauthorized("Missing or invalid authorization header"))
}

pub async fn register_app_service(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<RegisterAppServiceBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = body.into_request()?;

    let service = state.services.app_service_manager.register(request).await?;

    Ok(created_json_from::<_, AppServiceResponse>(service))
}

pub async fn get_app_service(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let service = state.services.app_service_manager.get(&as_id).await?;

    Ok(json_from::<_, AppServiceResponse>(require_found(
        service,
        "Application service not found",
    )?))
}

pub async fn list_app_services(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let services = state.services.app_service_manager.get_all_active().await?;

    Ok(json_vec_from::<_, AppServiceResponse>(services))
}

pub async fn update_app_service(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
    Json(body): Json<UpdateAppServiceBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = body.into_request();

    let service = state
        .services
        .app_service_manager
        .update(&as_id, request)
        .await?;

    Ok(json_from::<_, AppServiceResponse>(service))
}

pub async fn delete_app_service(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .app_service_manager
        .unregister(&as_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn ping_app_service(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let is_alive = state.services.app_service_manager.ping(&as_id).await?;

    Ok(Json(serde_json::json!({
        "as_id": as_id,
        "alive": is_alive
    })))
}

pub async fn set_app_service_state(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
    Json(body): Json<SetStateBody>,
) -> Result<impl IntoResponse, ApiError> {
    let state_entry = state
        .services
        .app_service_manager
        .set_state(&as_id, &body.state_key, &body.state_value)
        .await?;

    Ok(app_service_state_json(state_entry))
}

pub async fn get_app_service_state(
    State(state): State<AppState>,
    Path((as_id, state_key)): Path<(String, String)>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let state_entry = state
        .services
        .app_service_manager
        .get_state(&as_id, &state_key)
        .await?;

    Ok(app_service_state_json(require_found(
        state_entry,
        "State not found",
    )?))
}

pub async fn get_app_service_states(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let states = state
        .services
        .app_service_manager
        .get_all_states(&as_id)
        .await?;

    Ok(Json(states))
}

pub async fn register_virtual_user(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
    Json(body): Json<RegisterVirtualUserBody>,
) -> Result<impl IntoResponse, ApiError> {
    let user = state
        .services
        .app_service_manager
        .register_virtual_user(
            &as_id,
            &body.user_id,
            body.displayname.as_deref(),
            body.avatar_url.as_deref(),
        )
        .await?;

    Ok(created_json_from::<_, VirtualUserResponse>(user))
}

pub async fn get_virtual_users(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let users = state
        .services
        .app_service_manager
        .get_virtual_users(&as_id)
        .await?;

    Ok(json_vec_from::<_, VirtualUserResponse>(users))
}

pub async fn get_namespaces(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let namespaces = state
        .services
        .app_service_manager
        .get_namespaces(&as_id)
        .await?;

    Ok(Json(namespaces))
}

pub async fn get_pending_events(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
    Query(query): Query<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let events = state
        .services
        .app_service_manager
        .get_pending_events(&as_id, limit)
        .await?;

    Ok(Json(events))
}

pub async fn push_event(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
    Json(body): Json<PushEventBody>,
) -> Result<impl IntoResponse, ApiError> {
    let event = state
        .services
        .app_service_manager
        .push_event(
            &as_id,
            &body.room_id,
            &body.event_type,
            &body.sender,
            body.content,
            body.state_key.as_deref(),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "event_id": event.event_id,
            "as_id": event.as_id,
            "room_id": event.room_id,
            "event_type": event.event_type
        })),
    ))
}

pub async fn query_user(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<QueryUser>,
) -> Result<impl IntoResponse, ApiError> {
    let as_id = state
        .services
        .app_service_manager
        .query_user(&query.user_id)
        .await?;

    Ok(Json(serde_json::json!({
        "user_id": query.user_id,
        "application_service": as_id
    })))
}

pub async fn query_room_alias(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<QueryAlias>,
) -> Result<impl IntoResponse, ApiError> {
    let as_id = state
        .services
        .app_service_manager
        .query_room_alias(&query.alias)
        .await?;

    Ok(Json(serde_json::json!({
        "alias": query.alias,
        "application_service": as_id
    })))
}

pub async fn get_statistics(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let stats = state.services.app_service_manager.get_statistics().await?;

    Ok(Json(stats))
}

pub async fn app_service_ping(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let as_token = extract_as_token(&headers)?;

    let service = state
        .services
        .app_service_manager
        .validate_token(&as_token)
        .await?;

    Ok(Json(serde_json::json!({
        "as_id": service.as_id
    })))
}

pub async fn app_service_transactions(
    State(state): State<AppState>,
    Path((as_id, _txn_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    let as_token = extract_as_token(&headers)?;

    let service = state
        .services
        .app_service_manager
        .validate_token(&as_token)
        .await?;

    if service.as_id != as_id {
        return Err(ApiError::forbidden("Application service ID mismatch"));
    }

    let events = body
        .get("events")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();

    state
        .services
        .app_service_manager
        .send_transaction(&as_id, events.clone())
        .await?;

    Ok(empty_json())
}

pub async fn app_service_user_query(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let as_token = extract_as_token(&headers)?;

    let service = state
        .services
        .app_service_manager
        .validate_token(&as_token)
        .await?;

    let namespace_as_id = state
        .services
        .app_service_manager
        .query_user(&user_id)
        .await?;

    if namespace_as_id.as_ref() != Some(&service.as_id) {
        return Err(ApiError::forbidden(
            "User not in application service namespace",
        ));
    }

    Ok(empty_json())
}

pub async fn app_service_room_alias_query(
    State(state): State<AppState>,
    Path(alias): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let as_token = extract_as_token(&headers)?;

    let service = state
        .services
        .app_service_manager
        .validate_token(&as_token)
        .await?;

    let namespace_as_id = state
        .services
        .app_service_manager
        .query_room_alias(&alias)
        .await?;

    if namespace_as_id.as_ref() != Some(&service.as_id) {
        return Err(ApiError::forbidden(
            "Room alias not in application service namespace",
        ));
    }

    Ok(empty_json())
}

pub async fn app_service_query(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let service = state.services.app_service_manager.get(&as_id).await?;

    let service = require_found(service, "Application service not found")?;

    Ok(Json(serde_json::json!({
        "id": service.as_id,
        "url": service.url,
        "sender": service.sender_localpart,
        "description": service.description,
        "is_enabled": service.is_enabled,
        "protocols": service.protocols,
    })))
}

pub fn create_app_service_router(state: AppState) -> Router<AppState> {
    let public_routes = Router::new()
        .route(
            "/_matrix/client/v1/user/{user_id}/appservice",
            get(get_user_appservice),
        )
        .route("/_matrix/app/v1/ping", post(app_service_ping))
        .route(
            "/_matrix/app/v1/transactions/{as_id}/{txn_id}",
            put(app_service_transactions),
        )
        .route(
            "/_matrix/app/v1/users/{user_id}",
            get(app_service_user_query),
        )
        .route(
            "/_matrix/app/v1/rooms/{alias}",
            get(app_service_room_alias_query),
        )
        .route("/_matrix/app/v1/{as_id}", get(app_service_query));

    let admin_routes = Router::new()
        .route("/_synapse/admin/v1/appservices", get(list_app_services))
        .route("/_synapse/admin/v1/appservices", post(register_app_service))
        .route(
            "/_synapse/admin/v1/appservices/{as_id}",
            get(get_app_service),
        )
        .route(
            "/_synapse/admin/v1/appservices/{as_id}",
            put(update_app_service),
        )
        .route(
            "/_synapse/admin/v1/appservices/{as_id}",
            delete(delete_app_service),
        )
        .route(
            "/_synapse/admin/v1/appservices/{as_id}/ping",
            post(ping_app_service),
        )
        .route(
            "/_synapse/admin/v1/appservices/{as_id}/state",
            post(set_app_service_state),
        )
        .route(
            "/_synapse/admin/v1/appservices/{as_id}/state",
            get(get_app_service_states),
        )
        .route(
            "/_synapse/admin/v1/appservices/{as_id}/state/{state_key}",
            get(get_app_service_state),
        )
        .route(
            "/_synapse/admin/v1/appservices/{as_id}/users",
            post(register_virtual_user),
        )
        .route(
            "/_synapse/admin/v1/appservices/{as_id}/users",
            get(get_virtual_users),
        )
        .route(
            "/_synapse/admin/v1/appservices/{as_id}/namespaces",
            get(get_namespaces),
        )
        .route(
            "/_synapse/admin/v1/appservices/{as_id}/events",
            get(get_pending_events),
        )
        .route(
            "/_synapse/admin/v1/appservices/{as_id}/events",
            post(push_event),
        )
        .route("/_synapse/admin/v1/appservices/query/user", get(query_user))
        .route(
            "/_synapse/admin/v1/appservices/query/alias",
            get(query_room_alias),
        )
        .route(
            "/_synapse/admin/v1/appservices/statistics",
            get(get_statistics),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::web::middleware::admin_auth_middleware,
        ));

    public_routes.merge(admin_routes).with_state(state)
}

#[axum::debug_handler]
async fn get_user_appservice(
    State(_state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    Ok(Json(serde_json::json!({
        "user_id": user_id,
        "appservices": []
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_app_service_body_into_request_uses_sender_fallback() {
        let body = RegisterAppServiceBody {
            id: "as-1".to_string(),
            url: "https://example.com".to_string(),
            as_token: "as-token".to_string(),
            hs_token: "hs-token".to_string(),
            sender: None,
            sender_localpart: Some("@bot:example.com".to_string()),
            description: Some("desc".to_string()),
            rate_limited: Some(true),
            protocols: Some(vec!["irc".to_string()]),
            namespaces: Some(serde_json::json!({"users": [], "aliases": [], "rooms": []})),
        };

        let request = body.into_request().expect("sender fallback should succeed");

        assert_eq!(request.as_id, "as-1");
        assert_eq!(request.sender, "@bot:example.com");
        assert_eq!(request.protocols, Some(vec!["irc".to_string()]));
    }

    #[test]
    fn test_register_app_service_body_into_request_requires_sender() {
        let body = RegisterAppServiceBody {
            id: "as-1".to_string(),
            url: "https://example.com".to_string(),
            as_token: "as-token".to_string(),
            hs_token: "hs-token".to_string(),
            sender: None,
            sender_localpart: None,
            description: None,
            rate_limited: None,
            protocols: None,
            namespaces: None,
        };

        let error = body.into_request().expect_err("missing sender should fail");

        match error {
            ApiError::BadRequest(message) => {
                assert!(message.contains("Missing sender or sender_localpart"));
            }
            other => panic!("expected bad request error, got {:?}", other),
        }
    }

    #[test]
    fn test_update_app_service_body_into_request_preserves_fields() {
        let body = UpdateAppServiceBody {
            url: Some("https://updated.example.com".to_string()),
            description: Some("updated".to_string()),
            rate_limited: Some(false),
            protocols: Some(vec!["slack".to_string(), "irc".to_string()]),
            is_enabled: Some(true),
        };

        let request = body.into_request();

        assert_eq!(request.url.as_deref(), Some("https://updated.example.com"));
        assert_eq!(request.description.as_deref(), Some("updated"));
        assert_eq!(request.rate_limited, Some(false));
        assert_eq!(
            request.protocols,
            Some(vec!["slack".to_string(), "irc".to_string()])
        );
        assert_eq!(request.is_enabled, Some(true));
    }
}
