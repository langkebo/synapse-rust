use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
    Router,
    routing::{get, post, put, delete},
};
use serde::{Deserialize, Serialize};

use crate::common::ApiError;
use crate::storage::application_service::{RegisterApplicationServiceRequest, ApplicationService, ApplicationServiceUser};
use crate::web::routes::AuthenticatedUser;
use crate::web::routes::AppState;

#[derive(Debug, Deserialize)]
pub struct RegisterAppServiceBody {
    pub id: String,
    pub url: String,
    pub as_token: String,
    pub hs_token: String,
    pub sender: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub rate_limited: Option<bool>,
    pub protocols: Option<Vec<String>>,
    pub namespaces: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAppServiceBody {
    pub url: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub rate_limited: Option<bool>,
    pub protocols: Option<Vec<String>>,
    pub is_active: Option<bool>,
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
    pub name: Option<String>,
    pub description: Option<String>,
    pub rate_limited: bool,
    pub protocols: Vec<String>,
    pub is_active: bool,
    pub created_ts: i64,
    pub last_seen_ts: Option<i64>,
}

impl From<ApplicationService> for AppServiceResponse {
    fn from(svc: ApplicationService) -> Self {
        Self {
            id: svc.id,
            as_id: svc.as_id,
            url: svc.url,
            sender: svc.sender,
            name: svc.name,
            description: svc.description,
            rate_limited: svc.rate_limited,
            protocols: svc.protocols,
            is_active: svc.is_active,
            created_ts: svc.created_ts,
            last_seen_ts: svc.last_seen_ts,
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
    _auth_user: AuthenticatedUser,
    Json(body): Json<RegisterAppServiceBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = RegisterApplicationServiceRequest {
        as_id: body.id,
        url: body.url,
        as_token: body.as_token,
        hs_token: body.hs_token,
        sender: body.sender,
        name: body.name,
        description: body.description,
        rate_limited: body.rate_limited,
        protocols: body.protocols,
        namespaces: body.namespaces,
    };

    let service = state.services.app_service_manager.register(request).await?;
    
    Ok((StatusCode::CREATED, Json(AppServiceResponse::from(service))))
}

pub async fn get_app_service(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let service = state.services.app_service_manager.get(&as_id).await?
        .ok_or_else(|| ApiError::not_found("Application service not found"))?;
    
    Ok(Json(AppServiceResponse::from(service)))
}

pub async fn list_app_services(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let services = state.services.app_service_manager.get_all_active().await?;
    
    let response: Vec<AppServiceResponse> = services.into_iter().map(AppServiceResponse::from).collect();
    
    Ok(Json(response))
}

pub async fn update_app_service(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<UpdateAppServiceBody>,
) -> Result<impl IntoResponse, ApiError> {
    let service = state.services.app_service_manager.update(
        &as_id,
        body.url.as_deref(),
        body.name.as_deref(),
        body.description.as_deref(),
        body.rate_limited,
        body.protocols.as_deref(),
        body.is_active,
    ).await?;
    
    Ok(Json(AppServiceResponse::from(service)))
}

pub async fn delete_app_service(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    state.services.app_service_manager.unregister(&as_id).await?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn ping_app_service(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
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
    Json(body): Json<SetStateBody>,
) -> Result<impl IntoResponse, ApiError> {
    let state_entry = state.services.app_service_manager.set_state(
        &as_id,
        &body.state_key,
        &body.state_value,
    ).await?;
    
    Ok(Json(serde_json::json!({
        "as_id": state_entry.as_id,
        "state_key": state_entry.state_key,
        "state_value": state_entry.state_value,
        "updated_ts": state_entry.updated_ts
    })))
}

pub async fn get_app_service_state(
    State(state): State<AppState>,
    Path((as_id, state_key)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let state_entry = state.services.app_service_manager.get_state(&as_id, &state_key).await?
        .ok_or_else(|| ApiError::not_found("State not found"))?;
    
    Ok(Json(serde_json::json!({
        "as_id": state_entry.as_id,
        "state_key": state_entry.state_key,
        "state_value": state_entry.state_value,
        "updated_ts": state_entry.updated_ts
    })))
}

pub async fn get_app_service_states(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let states = state.services.app_service_manager.get_all_states(&as_id).await?;
    
    Ok(Json(states))
}

pub async fn register_virtual_user(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    Json(body): Json<RegisterVirtualUserBody>,
) -> Result<impl IntoResponse, ApiError> {
    let user = state.services.app_service_manager.register_virtual_user(
        &as_id,
        &body.user_id,
        body.displayname.as_deref(),
        body.avatar_url.as_deref(),
    ).await?;
    
    Ok((StatusCode::CREATED, Json(VirtualUserResponse::from(user))))
}

pub async fn get_virtual_users(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let users = state.services.app_service_manager.get_virtual_users(&as_id).await?;
    
    let response: Vec<VirtualUserResponse> = users.into_iter().map(VirtualUserResponse::from).collect();
    
    Ok(Json(response))
}

pub async fn get_namespaces(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let namespaces = state.services.app_service_manager.get_namespaces(&as_id).await?;
    
    Ok(Json(namespaces))
}

pub async fn get_pending_events(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    Query(query): Query<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let events = state.services.app_service_manager.get_pending_events(&as_id, limit).await?;
    
    Ok(Json(events))
}

pub async fn push_event(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    Json(body): Json<PushEventBody>,
) -> Result<impl IntoResponse, ApiError> {
    let event = state.services.app_service_manager.push_event(
        &as_id,
        &body.room_id,
        &body.event_type,
        &body.sender,
        body.content,
        body.state_key.as_deref(),
    ).await?;
    
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "event_id": event.event_id,
        "as_id": event.as_id,
        "room_id": event.room_id,
        "event_type": event.event_type
    }))))
}

pub async fn query_user(
    State(state): State<AppState>,
    Query(query): Query<QueryUser>,
) -> Result<impl IntoResponse, ApiError> {
    let as_id = state.services.app_service_manager.query_user(&query.user_id).await?;
    
    Ok(Json(serde_json::json!({
        "user_id": query.user_id,
        "application_service": as_id
    })))
}

pub async fn query_room_alias(
    State(state): State<AppState>,
    Query(query): Query<QueryAlias>,
) -> Result<impl IntoResponse, ApiError> {
    let as_id = state.services.app_service_manager.query_room_alias(&query.alias).await?;
    
    Ok(Json(serde_json::json!({
        "alias": query.alias,
        "application_service": as_id
    })))
}

pub async fn get_statistics(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let stats = state.services.app_service_manager.get_statistics().await?;
    
    Ok(Json(stats))
}

pub async fn app_service_ping(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let as_token = extract_as_token(&headers)?;
    
    let service = state.services.app_service_manager.validate_token(&as_token).await?;
    
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
    
    let service = state.services.app_service_manager.validate_token(&as_token).await?;
    
    if service.as_id != as_id {
        return Err(ApiError::forbidden("Application service ID mismatch"));
    }

    let events = body.get("events")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();

    state.services.app_service_manager.send_transaction(&as_id, events.clone()).await?;
    
    Ok(Json(serde_json::json!({})))
}

pub async fn app_service_user_query(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let as_token = extract_as_token(&headers)?;
    
    let service = state.services.app_service_manager.validate_token(&as_token).await?;
    
    let namespace_as_id = state.services.app_service_manager.query_user(&user_id).await?;
    
    if namespace_as_id.as_ref() != Some(&service.as_id) {
        return Err(ApiError::forbidden("User not in application service namespace"));
    }
    
    Ok(Json(serde_json::json!({})))
}

pub async fn app_service_room_alias_query(
    State(state): State<AppState>,
    Path(alias): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let as_token = extract_as_token(&headers)?;
    
    let service = state.services.app_service_manager.validate_token(&as_token).await?;
    
    let namespace_as_id = state.services.app_service_manager.query_room_alias(&alias).await?;
    
    if namespace_as_id.as_ref() != Some(&service.as_id) {
        return Err(ApiError::forbidden("Room alias not in application service namespace"));
    }
    
    Ok(Json(serde_json::json!({})))
}

pub fn create_app_service_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/app/v1/ping", post(app_service_ping))
        .route("/_matrix/app/v1/transactions/{as_id}/{txn_id}", put(app_service_transactions))
        .route("/_matrix/app/v1/users/{user_id}", get(app_service_user_query))
        .route("/_matrix/app/v1/rooms/{alias}", get(app_service_room_alias_query))
        .route("/_synapse/admin/v1/appservices", get(list_app_services))
        .route("/_synapse/admin/v1/appservices", post(register_app_service))
        .route("/_synapse/admin/v1/appservices/{as_id}", get(get_app_service))
        .route("/_synapse/admin/v1/appservices/{as_id}", put(update_app_service))
        .route("/_synapse/admin/v1/appservices/{as_id}", delete(delete_app_service))
        .route("/_synapse/admin/v1/appservices/{as_id}/ping", post(ping_app_service))
        .route("/_synapse/admin/v1/appservices/{as_id}/state", post(set_app_service_state))
        .route("/_synapse/admin/v1/appservices/{as_id}/state", get(get_app_service_states))
        .route("/_synapse/admin/v1/appservices/{as_id}/state/{state_key}", get(get_app_service_state))
        .route("/_synapse/admin/v1/appservices/{as_id}/users", post(register_virtual_user))
        .route("/_synapse/admin/v1/appservices/{as_id}/users", get(get_virtual_users))
        .route("/_synapse/admin/v1/appservices/{as_id}/namespaces", get(get_namespaces))
        .route("/_synapse/admin/v1/appservices/{as_id}/events", get(get_pending_events))
        .route("/_synapse/admin/v1/appservices/{as_id}/events", post(push_event))
        .route("/_synapse/admin/v1/appservices/query/user", get(query_user))
        .route("/_synapse/admin/v1/appservices/query/alias", get(query_room_alias))
        .route("/_synapse/admin/v1/appservices/statistics", get(get_statistics))
        .with_state(state)
}
