use crate::routes::context::AdminContext;
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{any, delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::routes::extractors::UserId;
use crate::routes::response_helpers::{created_json_from, empty_json, json_from, json_vec_from, require_found};
use crate::routes::validators::validate_as_id;
use crate::routes::{AdminUser, AppState, AuthenticatedUser};
use synapse_common::ApiError;
use synapse_services::application_service::{ApplicationService, UpdateApplicationServiceRequest};
use synapse_services::application_service::{
    ApplicationServiceState, ApplicationServiceUser, RegisterApplicationServiceRequest,
};

/// The `RegisterAppServiceBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterAppServiceBody {
    /// The `id` field.
    pub id: String,
    /// The `url` field.
    pub url: String,
    /// The `as_token` field.
    pub as_token: String,
    /// The `hs_token` field.
    pub hs_token: String,
    /// The `sender` field.
    pub sender: Option<String>,
    /// The `sender_localpart` field.
    pub sender_localpart: Option<String>,
    /// The `description` field.
    pub description: Option<String>,
    #[serde(rename = "rate_limited")]
    /// The `is_rate_limited` field.
    pub is_rate_limited: Option<bool>,
    /// The `protocols` field.
    pub protocols: Option<Vec<String>>,
    /// The `namespaces` field.
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
            is_rate_limited: self.is_rate_limited,
            protocols: self.protocols,
            namespaces: self.namespaces,
            api_key: None,
            config: None,
        })
    }
}

/// The `UpdateAppServiceBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateAppServiceBody {
    /// The `url` field.
    pub url: Option<String>,
    /// The `description` field.
    pub description: Option<String>,
    #[serde(rename = "rate_limited")]
    /// The `is_rate_limited` field.
    pub is_rate_limited: Option<bool>,
    /// The `protocols` field.
    pub protocols: Option<Vec<String>>,
    /// The `is_enabled` field.
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
        if let Some(rate_limited) = self.is_rate_limited {
            request = request.is_rate_limited(rate_limited);
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

/// The `SetStateBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetStateBody {
    /// The `state_key` field.
    pub state_key: String,
    /// The `state_value` field.
    pub state_value: String,
}

/// The `RegisterVirtualUserBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterVirtualUserBody {
    /// The `user_id` field.
    pub user_id: String,
    /// The `displayname` field.
    pub displayname: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
}

/// The `PushEventBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PushEventBody {
    /// The `room_id` field.
    pub room_id: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `sender` field.
    pub sender: String,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `state_key` field.
    pub state_key: Option<String>,
}

/// The `QueryLimit` struct.
#[derive(Debug, Deserialize)]
pub struct QueryLimit {
    /// The `limit` field.
    pub limit: Option<i64>,
}

/// The `QueryUser` struct.
#[derive(Debug, Deserialize)]
pub struct QueryUser {
    /// The `user_id` field.
    pub user_id: String,
}

/// The `QueryAlias` struct.
#[derive(Debug, Deserialize)]
pub struct QueryAlias {
    /// The `alias` field.
    pub alias: String,
}

/// The `AppServiceResponse` struct.
#[derive(Debug, Serialize)]
pub struct AppServiceResponse {
    /// The `id` field.
    pub id: i64,
    /// The `as_id` field.
    pub as_id: String,
    /// The `url` field.
    pub url: String,
    /// The `sender` field.
    pub sender: String,
    /// The `description` field.
    pub description: Option<String>,
    #[serde(rename = "rate_limited")]
    /// The `is_rate_limited` field.
    pub is_rate_limited: bool,
    /// The `protocols` field.
    pub protocols: Vec<String>,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `created_ts` field.
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
            is_rate_limited: svc.is_rate_limited,
            protocols: svc.protocols,
            is_enabled: svc.is_enabled,
            created_ts: svc.created_ts,
        }
    }
}

/// The `VirtualUserResponse` struct.
#[derive(Debug, Serialize)]
pub struct VirtualUserResponse {
    /// The `as_id` field.
    pub as_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `displayname` field.
    pub displayname: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `created_ts` field.
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

fn app_service_state_json(state_entry: &ApplicationServiceState) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "as_id": state_entry.as_id,
        "state_key": state_entry.state_key,
        "state_value": state_entry.state_value,
        "updated_ts": state_entry.updated_ts
    }))
}

pub(crate) fn extract_as_token(headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .ok_or_else(|| ApiError::unauthorized("Missing or invalid authorization header"))
}

/// See [`register_app_service`].
pub async fn register_app_service(
    State(ctx): State<AdminContext>,
    _admin: AdminUser,
    Json(body): Json<RegisterAppServiceBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = body.into_request()?;

    let service = ctx.app_service_manager.register(request).await?;

    Ok(created_json_from::<_, AppServiceResponse>(service))
}

/// See [`get_app_service`].
pub async fn get_app_service(
    State(ctx): State<AdminContext>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    validate_as_id(&as_id)?;
    let service = ctx.app_service_manager.get(&as_id).await?;

    Ok(json_from::<_, AppServiceResponse>(require_found(service, "Application service not found")?))
}

/// See [`list_app_services`].
pub async fn list_app_services(
    State(ctx): State<AdminContext>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let services = ctx.app_service_manager.get_all_active().await?;

    Ok(json_vec_from::<_, AppServiceResponse>(services))
}

/// See [`update_app_service`].
pub async fn update_app_service(
    State(ctx): State<AdminContext>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
    Json(body): Json<UpdateAppServiceBody>,
) -> Result<impl IntoResponse, ApiError> {
    validate_as_id(&as_id)?;
    let request = body.into_request();

    let service = ctx.app_service_manager.update(&as_id, request).await?;

    Ok(json_from::<_, AppServiceResponse>(service))
}

/// See [`delete_app_service`].
pub async fn delete_app_service(
    State(ctx): State<AdminContext>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    validate_as_id(&as_id)?;
    ctx.app_service_manager.unregister(&as_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// See [`ping_app_service`].
pub async fn ping_app_service(
    State(ctx): State<AdminContext>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    validate_as_id(&as_id)?;
    let is_alive = ctx.app_service_manager.ping(&as_id).await?;

    Ok(Json(serde_json::json!({
        "as_id": as_id,
        "alive": is_alive
    })))
}

/// See [`set_app_service_state`].
pub async fn set_app_service_state(
    State(ctx): State<AdminContext>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
    Json(body): Json<SetStateBody>,
) -> Result<impl IntoResponse, ApiError> {
    validate_as_id(&as_id)?;
    let state_entry = ctx.app_service_manager.set_state(&as_id, &body.state_key, &body.state_value).await?;

    Ok(app_service_state_json(&state_entry))
}

/// See [`get_app_service_state`].
pub async fn get_app_service_state(
    State(ctx): State<AdminContext>,
    Path((as_id, state_key)): Path<(String, String)>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    validate_as_id(&as_id)?;
    let state_entry = ctx.app_service_manager.get_state(&as_id, &state_key).await?;

    Ok(app_service_state_json(&require_found(state_entry, "State not found")?))
}

/// See [`get_app_service_states`].
pub async fn get_app_service_states(
    State(ctx): State<AdminContext>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    validate_as_id(&as_id)?;
    let states = ctx.app_service_manager.get_all_states(&as_id).await?;

    Ok(Json(states))
}

// =============================================================================
// MSC4512: Application Services Proxy
// =============================================================================

/// MSC4512: Proxy a request to the Application Service.
///
/// Proxies the request to the AS endpoint at `/{as_id}/{...path}`.
/// Requires valid HS token authentication via the `Authorization: Bearer <hs_token>` header.
/// Note: This is an internal implementation detail — the full AS proxy semantics
/// will be validated against the MSC4512 spec before production rollout.
pub async fn proxy_to_as(
    State(ctx): State<AdminContext>,
    Path((as_id, _path)): Path<(String, String)>,
    _headers: HeaderMap,
    _method: axum::http::Method,
    _body: Bytes,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    use axum::body::Body;

    validate_as_id(&as_id)?;

    let service = ctx.app_service_manager.get(&as_id).await?;
    let service = require_found(service, "Application service not found")?;

    if !service.is_enabled {
        return Err(ApiError::bad_request("Application service is disabled"));
    }

    // Placeholder response — full proxy implementation pending MSC4512 spec validation
    Ok((
        StatusCode::NOT_IMPLEMENTED,
        Body::from(
            serde_json::json!({
                "errcode": "M_NOT_IMPLEMENTED",
                "error": "MSC4512 AS proxy not yet implemented"
            })
            .to_string(),
        ),
    ))
}

/// See [`register_virtual_user`].
pub async fn register_virtual_user(
    State(ctx): State<AdminContext>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
    Json(body): Json<RegisterVirtualUserBody>,
) -> Result<impl IntoResponse, ApiError> {
    validate_as_id(&as_id)?;
    let user = ctx
        .app_service_manager
        .register_virtual_user(&as_id, &body.user_id, body.displayname.as_deref(), body.avatar_url.as_deref())
        .await?;

    Ok(created_json_from::<_, VirtualUserResponse>(user))
}

/// See [`get_virtual_users`].
pub async fn get_virtual_users(
    State(ctx): State<AdminContext>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    validate_as_id(&as_id)?;
    let users = ctx.app_service_manager.get_virtual_users(&as_id).await?;

    Ok(json_vec_from::<_, VirtualUserResponse>(users))
}

/// See [`get_namespaces`].
pub async fn get_namespaces(
    State(ctx): State<AdminContext>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    validate_as_id(&as_id)?;
    let namespaces = ctx.app_service_manager.get_namespaces(&as_id).await?;

    Ok(Json(namespaces))
}

/// See [`get_pending_events`].
pub async fn get_pending_events(
    State(ctx): State<AdminContext>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
    Query(query): Query<QueryLimit>,
) -> Result<impl IntoResponse, ApiError> {
    validate_as_id(&as_id)?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let events = ctx.app_service_manager.get_pending_events(&as_id, limit).await?;

    Ok(Json(events))
}

/// See [`push_event`].
pub async fn push_event(
    State(ctx): State<AdminContext>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
    Json(body): Json<PushEventBody>,
) -> Result<impl IntoResponse, ApiError> {
    validate_as_id(&as_id)?;
    let event = ctx
        .app_service_manager
        .push_event(&as_id, &body.room_id, &body.event_type, &body.sender, body.content, body.state_key.as_deref())
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

/// See [`query_user`].
pub async fn query_user(
    State(ctx): State<AdminContext>,
    _admin: AdminUser,
    Query(query): Query<QueryUser>,
) -> Result<impl IntoResponse, ApiError> {
    let as_id = ctx.app_service_manager.query_user(&query.user_id).await?;

    Ok(Json(serde_json::json!({
        "user_id": query.user_id,
        "application_service": as_id,
        "exists": as_id.is_some()
    })))
}

/// See [`query_room_alias`].
pub async fn query_room_alias(
    State(ctx): State<AdminContext>,
    _admin: AdminUser,
    Query(query): Query<QueryAlias>,
) -> Result<impl IntoResponse, ApiError> {
    let as_id = ctx.app_service_manager.query_room_alias(&query.alias).await?;

    Ok(Json(serde_json::json!({
        "alias": query.alias,
        "application_service": as_id,
        "exists": as_id.is_some()
    })))
}

/// See [`get_statistics`].
pub async fn get_statistics(State(ctx): State<AdminContext>, _admin: AdminUser) -> Result<impl IntoResponse, ApiError> {
    let stats = ctx.app_service_manager.get_statistics().await?;

    Ok(Json(stats))
}

/// See [`app_service_ping`].
pub async fn app_service_ping(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let as_token = extract_as_token(&headers)?;

    let service = ctx.app_service_manager.validate_token(&as_token).await?;

    Ok(Json(serde_json::json!({
        "as_id": service.as_id
    })))
}

/// See [`app_service_transactions`].
pub async fn app_service_transactions(
    State(ctx): State<AdminContext>,
    Path((as_id, _txn_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    let as_token = extract_as_token(&headers)?;

    let service = ctx.app_service_manager.validate_token(&as_token).await?;

    if service.as_id != as_id {
        return Err(ApiError::forbidden("Application service ID mismatch"));
    }

    let events = body.get("events").and_then(|e| e.as_array()).cloned().unwrap_or_default();

    ctx.app_service_manager.send_transaction(&as_id, events).await?;

    Ok(empty_json())
}

/// See [`app_service_user_query`].
pub async fn app_service_user_query(
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let as_token = extract_as_token(&headers)?;

    let service = ctx.app_service_manager.validate_token(&as_token).await?;

    let namespace_as_id = ctx.app_service_manager.query_user(&user_id).await?;

    if namespace_as_id.as_ref() != Some(&service.as_id) {
        return Err(ApiError::forbidden("User not in application service namespace"));
    }

    Ok(empty_json())
}

/// See [`app_service_room_alias_query`].
pub async fn app_service_room_alias_query(
    State(ctx): State<AdminContext>,
    Path(alias): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let as_token = extract_as_token(&headers)?;

    let service = ctx.app_service_manager.validate_token(&as_token).await?;

    let namespace_as_id = ctx.app_service_manager.query_room_alias(&alias).await?;

    if namespace_as_id.as_ref() != Some(&service.as_id) {
        return Err(ApiError::forbidden("Room alias not in application service namespace"));
    }

    Ok(empty_json())
}

/// See [`app_service_query`].
pub async fn app_service_query(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    Path(as_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // Same `as_token` gate as every sibling in this file (`app_service_ping`,
    // `app_service_user_query`, `app_service_room_alias_query`, ...). Without it
    // this endpoint answered anonymously for any `as_id`, disclosing the
    // application service's `url`, `sender_localpart`, `description` and
    // `protocols` — configuration disclosure plus an `as_id` enumerator.
    //
    // Authentication is checked before `as_id` is validated so an unauthenticated
    // caller gets no format oracle either. The ownership check mirrors the
    // siblings' namespace check: a token may only describe its own AS.
    let as_token = extract_as_token(&headers)?;
    let token_service = ctx.app_service_manager.validate_token(&as_token).await?;
    if token_service.as_id != as_id {
        return Err(ApiError::forbidden("Token does not belong to this application service".to_string()));
    }

    validate_as_id(&as_id)?;
    let service = ctx.app_service_manager.get(&as_id).await?;

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

/// See [`create_app_service_router`].
pub fn create_app_service_router(state: &AppState) -> Router<AppState> {
    let public_routes = Router::new()
        .route("/_matrix/client/v1/user/{user_id}/appservice", get(get_user_appservice))
        .route("/_matrix/app/v1/ping", post(app_service_ping))
        .route("/_matrix/app/v1/transactions/{as_id}/{txn_id}", put(app_service_transactions))
        .route("/_matrix/app/v1/users/{user_id}", get(app_service_user_query))
        .route("/_matrix/app/v1/rooms/{alias}", get(app_service_room_alias_query))
        .route("/_matrix/app/v1/{as_id}", get(app_service_query))
        .route("/_matrix/app/v1/proxy/{as_id}/*path", any(proxy_to_as))
        .route("/_matrix/client/v1/proxy/{as_id}/*path", any(proxy_to_as))
        .route("/_matrix/client/v3/appservice/user", get(query_user))
        .route("/_matrix/client/v3/appservice/alias", get(query_room_alias));

    let admin_routes = Router::new()
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
        .route_layer(axum::middleware::from_fn_with_state(
            <crate::routes::context::AdminContext as axum::extract::FromRef<crate::routes::AppState>>::from_ref(state),
            crate::middleware::admin_auth_middleware,
        ));

    public_routes.merge(admin_routes)
}

#[allow(clippy::unused_async)]
async fn get_user_appservice(
    State(_ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<UserId>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = user_id.as_str();
    if auth_user.user_id != user_id {
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
            is_rate_limited: Some(true),
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
            is_rate_limited: None,
            protocols: None,
            namespaces: None,
        };

        let error = body.into_request().expect_err("missing sender should fail");

        assert!(error.is_bad_request());
        assert!(error.internal_message().contains("Missing sender or sender_localpart"));
    }

    #[test]
    fn test_update_app_service_body_into_request_preserves_fields() {
        let body = UpdateAppServiceBody {
            url: Some("https://updated.example.com".to_string()),
            description: Some("updated".to_string()),
            is_rate_limited: Some(false),
            protocols: Some(vec!["slack".to_string(), "irc".to_string()]),
            is_enabled: Some(true),
        };

        let request = body.into_request();

        assert_eq!(request.url.as_deref(), Some("https://updated.example.com"));
        assert_eq!(request.description.as_deref(), Some("updated"));
        assert_eq!(request.is_rate_limited, Some(false));
        assert_eq!(request.protocols, Some(vec!["slack".to_string(), "irc".to_string()]));
        assert_eq!(request.is_enabled, Some(true));
    }
}
