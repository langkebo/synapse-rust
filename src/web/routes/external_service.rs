use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::common::ApiError;
use crate::services::external_service_integration::*;
use crate::web::routes::{AdminUser, AppState, AuthenticatedUser};
use crate::web::utils::auth::resolve_request_id;

#[derive(Debug, Deserialize)]
pub struct RegisterExternalServiceBody {
    pub service_type: String,
    pub service_id: String,
    pub display_name: String,
    pub webhook_url: Option<String>,
    pub api_key: Option<String>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateExternalServiceBody {
    pub webhook_url: Option<String>,
    pub api_key: Option<String>,
    pub config: Option<serde_json::Value>,
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListServicesQuery {
    #[serde(default)]
    pub service_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExternalServiceResponse {
    pub as_id: String,
    pub service_type: String,
    pub service_id: String,
    pub display_name: String,
    pub is_enabled: bool,
    pub is_healthy: bool,
    pub created_ts: i64,
}

impl From<crate::storage::application_service::ApplicationService> for ExternalServiceResponse {
    fn from(svc: crate::storage::application_service::ApplicationService) -> Self {
        let parts: Vec<&str> = svc.as_id.splitn(2, '_').collect();
        let (service_type, service_id) = if parts.len() == 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            ("unknown".to_string(), svc.as_id.clone())
        };

        Self {
            as_id: svc.as_id.clone(),
            service_type,
            service_id,
            display_name: svc.as_id.clone(),
            is_enabled: svc.is_enabled,
            is_healthy: true,
            created_ts: svc.created_ts,
        }
    }
}

fn parse_service_type(s: &str) -> Result<ExternalServiceType, ApiError> {
    match s.to_lowercase().as_str() {
        "trendradar" => Ok(ExternalServiceType::TrendRadar),
        #[cfg(feature = "openclaw-routes")]
        "openclaw" => Ok(ExternalServiceType::OpenClaw),
        "generic_webhook" | "webhook" => Ok(ExternalServiceType::GenericWebhook),
        "irc_bridge" | "irc" => Ok(ExternalServiceType::IrcBridge),
        "slack_bridge" | "slack" => Ok(ExternalServiceType::SlackBridge),
        "discord_bridge" | "discord" => Ok(ExternalServiceType::DiscordBridge),
        "custom" => Ok(ExternalServiceType::Custom),
        _ => Err(ApiError::bad_request(format!("Unknown service type: {}", s))),
    }
}

fn extract_webhook_auth(headers: &HeaderMap, payload_signature: Option<&str>) -> WebhookAuthInput {
    let token = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .or_else(|| headers.get("x-webhook-token").and_then(|value| value.to_str().ok()))
        .or_else(|| headers.get("x-api-key").and_then(|value| value.to_str().ok()))
        .map(ToOwned::to_owned);

    let signature = headers
        .get("x-webhook-signature")
        .and_then(|value| value.to_str().ok())
        .or(payload_signature)
        .map(ToOwned::to_owned);

    WebhookAuthInput { token, signature }
}

pub async fn register_external_service(
    State(state): State<AppState>,
    headers: HeaderMap,
    _admin: AdminUser,
    Json(body): Json<RegisterExternalServiceBody>,
) -> Result<impl IntoResponse, ApiError> {
    let service_type = parse_service_type(&body.service_type)?;

    let config = ExternalServiceConfig {
        service_type: service_type.clone(),
        service_id: body.service_id.clone(),
        display_name: body.display_name,
        webhook_url: body.webhook_url,
        api_key: body.api_key,
        config: body.config.unwrap_or(serde_json::json!({})),
        is_enabled: true,
    };

    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.admin.app_service_storage.clone()),
        state.services.core.server_name.clone(),
    );

    let request_id = resolve_request_id(&headers);
    let service = integration.register_external_service(&request_id, config).await?;

    Ok((StatusCode::CREATED, Json(ExternalServiceResponse::from(service))))
}

pub async fn list_external_services(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<ListServicesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let stype = match query.service_type.as_deref() {
        Some("all") | None => None,
        Some(s) => Some(parse_service_type(s)?),
    };

    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.admin.app_service_storage.clone()),
        state.services.core.server_name.clone(),
    );

    let services = integration.list_external_services(stype).await?;

    let response: Vec<ExternalServiceResponse> = services.into_iter().map(ExternalServiceResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_external_service_health(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.admin.app_service_storage.clone()),
        state.services.core.server_name.clone(),
    );

    let health = integration
        .get_service_health(&as_id)
        .await
        .ok_or_else(|| ApiError::not_found("Service health status not found"))?;

    Ok(Json(serde_json::json!({
        "service_id": health.service_id,
        "service_type": health.service_type.to_string(),
        "is_healthy": health.is_healthy,
        "last_check_ts": health.last_check_ts,
        "last_success_ts": health.last_success_ts,
        "last_error": health.last_error,
        "consecutive_failures": health.consecutive_failures,
    })))
}

pub async fn check_service_health(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    headers: HeaderMap,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.admin.app_service_storage.clone()),
        state.services.core.server_name.clone(),
    );

    let request_id = resolve_request_id(&headers);
    let is_healthy = integration.check_service_health(&request_id, &as_id).await?;

    Ok(Json(serde_json::json!({
        "as_id": as_id,
        "is_healthy": is_healthy
    })))
}

pub async fn unregister_external_service(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    headers: HeaderMap,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.admin.app_service_storage.clone()),
        state.services.core.server_name.clone(),
    );

    let request_id = resolve_request_id(&headers);
    integration.unregister_external_service(&request_id, &as_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_external_service(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
    headers: HeaderMap,
    _admin: AdminUser,
    Json(body): Json<UpdateExternalServiceBody>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.admin.app_service_storage.clone()),
        state.services.core.server_name.clone(),
    );

    let mut request = crate::storage::application_service::UpdateApplicationServiceRequest::new();
    if let Some(webhook_url) = body.webhook_url {
        request = request.url(webhook_url);
    }
    if let Some(api_key) = body.api_key {
        request = request.api_key(api_key);
    }
    if let Some(config) = body.config {
        request = request.config(config);
    }
    if let Some(is_enabled) = body.is_enabled {
        request = request.is_enabled(is_enabled);
    }

    let request_id = resolve_request_id(&headers);
    let service = integration.update_external_service(&request_id, &as_id, request).await?;
    Ok(Json(ExternalServiceResponse::from(service)))
}

pub async fn handle_trendradar_webhook(
    State(state): State<AppState>,
    Path(service_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<TrendRadarPayload>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.admin.app_service_storage.clone()),
        state.services.core.server_name.clone(),
    );

    let request_id = resolve_request_id(&headers);
    integration
        .handle_trendradar_webhook(&request_id, &service_id, payload, extract_webhook_auth(&headers, None))
        .await?;

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "TrendRadar webhook processed successfully"
    })))
}

#[cfg(feature = "openclaw-routes")]
pub async fn handle_openclaw_webhook(
    State(state): State<AppState>,
    Path(service_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<OpenClawPayload>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.admin.app_service_storage.clone()),
        state.services.core.server_name.clone(),
    );

    let request_id = resolve_request_id(&headers);
    integration
        .handle_openclaw_webhook(&request_id, &service_id, payload, extract_webhook_auth(&headers, None))
        .await?;

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "OpenClaw webhook processed successfully"
    })))
}

pub async fn handle_generic_webhook(
    State(state): State<AppState>,
    Path(service_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<WebhookPayload>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.admin.app_service_storage.clone()),
        state.services.core.server_name.clone(),
    );

    let request_id = resolve_request_id(&headers);
    integration
        .handle_generic_webhook(
            &request_id,
            &service_id,
            payload.clone(),
            extract_webhook_auth(&headers, payload.signature.as_deref()),
        )
        .await?;

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Webhook processed successfully"
    })))
}

pub async fn get_all_health_status(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.admin.app_service_storage.clone()),
        state.services.core.server_name.clone(),
    );

    let status_list = integration.get_all_health_status().await;

    Ok(Json(status_list))
}

pub async fn client_update_external_service(
    State(state): State<AppState>,
    headers: HeaderMap,
    _admin: AdminUser,
    Path(service_id): Path<String>,
    Json(body): Json<UpdateExternalServiceBody>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.admin.app_service_storage.clone()),
        state.services.core.server_name.clone(),
    );

    let mut request = crate::storage::application_service::UpdateApplicationServiceRequest::new();
    if let Some(webhook_url) = body.webhook_url {
        request = request.url(webhook_url);
    }
    if let Some(api_key) = body.api_key {
        request = request.api_key(api_key);
    }
    if let Some(config) = body.config {
        request = request.config(config);
    }
    if let Some(is_enabled) = body.is_enabled {
        request = request.is_enabled(is_enabled);
    }

    let request_id = resolve_request_id(&headers);
    let service = integration.update_external_service(&request_id, &service_id, request).await?;
    Ok(Json(ExternalServiceResponse::from(service)))
}

pub async fn client_delete_external_service(
    State(state): State<AppState>,
    headers: HeaderMap,
    _admin: AdminUser,
    Path(service_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.admin.app_service_storage.clone()),
        state.services.core.server_name.clone(),
    );

    let request_id = resolve_request_id(&headers);
    integration.unregister_external_service(&request_id, &service_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn client_health_check_all(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.admin.app_service_storage.clone()),
        state.services.core.server_name.clone(),
    );

    let status_list = integration.get_all_health_status().await;

    Ok(Json(status_list))
}

pub fn create_external_service_router(state: AppState) -> Router<AppState> {
    let admin_routes = Router::new()
        .route("/_synapse/admin/v1/external_services", get(list_external_services).post(register_external_service))
        .route("/_synapse/admin/v1/external_services/{as_id}/health", get(get_external_service_health))
        .route("/_synapse/admin/v1/external_services/{as_id}/health/check", post(check_service_health))
        .route(
            "/_synapse/admin/v1/external_services/{as_id}",
            put(update_external_service).delete(unregister_external_service),
        )
        .route("/_synapse/admin/v1/external_services/health", get(get_all_health_status))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::web::middleware::admin_auth_middleware,
        ));

    let admin_v1_routes = Router::new()
        .route("/_matrix/admin/v1/external_services", get(list_external_services).post(register_external_service))
        .route(
            "/_matrix/admin/v1/external_services/{as_id}",
            put(update_external_service).delete(unregister_external_service),
        )
        .route("/_matrix/admin/v1/external_services/health", get(get_all_health_status))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::web::middleware::admin_auth_middleware,
        ));

    let client_v1_routes =
        Router::new().route("/_matrix/client/v1/external_services/health", get(client_health_check_all)).route(
            "/_matrix/client/v1/external_services/{service_id}",
            put(client_update_external_service).delete(client_delete_external_service),
        );

    let public_routes = Router::new()
        .route("/_synapse/external/trendradar/{service_id}/webhook", post(handle_trendradar_webhook))
        .route("/_synapse/external/webhook/{service_id}", post(handle_generic_webhook));
    #[cfg(feature = "openclaw-routes")]
    let public_routes =
        public_routes.route("/_synapse/external/openclaw/{service_id}/webhook", post(handle_openclaw_webhook));

    public_routes.merge(admin_routes).merge(admin_v1_routes).merge(client_v1_routes).with_state(state)
}

pub fn external_service_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;

    let mut entries: Vec<RouteEntry> = [
        (Method::GET, "/_synapse/admin/v1/external_services"),
        (Method::POST, "/_synapse/admin/v1/external_services"),
        (Method::GET, "/_synapse/admin/v1/external_services/{as_id}/health"),
        (Method::POST, "/_synapse/admin/v1/external_services/{as_id}/health/check"),
        (Method::PUT, "/_synapse/admin/v1/external_services/{as_id}"),
        (Method::DELETE, "/_synapse/admin/v1/external_services/{as_id}"),
        (Method::GET, "/_synapse/admin/v1/external_services/health"),
        (Method::POST, "/_synapse/external/trendradar/{service_id}/webhook"),
        (Method::POST, "/_synapse/external/webhook/{service_id}"),
        (Method::GET, "/_matrix/admin/v1/external_services"),
        (Method::POST, "/_matrix/admin/v1/external_services"),
        (Method::PUT, "/_matrix/admin/v1/external_services/{as_id}"),
        (Method::DELETE, "/_matrix/admin/v1/external_services/{as_id}"),
        (Method::GET, "/_matrix/admin/v1/external_services/health"),
        (Method::GET, "/_matrix/client/v1/external_services/health"),
        (Method::PUT, "/_matrix/client/v1/external_services/{service_id}"),
        (Method::DELETE, "/_matrix/client/v1/external_services/{service_id}"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "external_service"))
    .collect();

    #[cfg(feature = "openclaw-routes")]
    entries.push(RouteEntry::new(Method::POST, "/_synapse/external/openclaw/{service_id}/webhook", "external_service"));

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_service_type() {
        assert!(matches!(parse_service_type("trendradar"), Ok(ExternalServiceType::TrendRadar)));
        #[cfg(feature = "openclaw-routes")]
        assert!(matches!(parse_service_type("openclaw"), Ok(ExternalServiceType::OpenClaw)));
        assert!(matches!(parse_service_type("webhook"), Ok(ExternalServiceType::GenericWebhook)));
        assert!(matches!(parse_service_type("irc"), Ok(ExternalServiceType::IrcBridge)));
        assert!(parse_service_type("unknown").is_err());
    }

    #[test]
    fn test_register_external_service_body_deserialization() {
        let json = r#"{
            "service_type": "trendradar",
            "service_id": "news-bot",
            "display_name": "News Bot",
            "webhook_url": "https://example.com/webhook",
            "config": {"topic": "tech"}
        }"#;

        let body: RegisterExternalServiceBody = serde_json::from_str(json).unwrap();
        assert_eq!(body.service_type, "trendradar");
        assert_eq!(body.service_id, "news-bot");
        assert!(body.webhook_url.is_some());
    }
}
