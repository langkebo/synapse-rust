use crate::web::routes::context::AdminContext;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState, AuthenticatedUser};
use crate::web::utils::auth::resolve_request_id;
use synapse_services::external_service_integration::*;

/// The `RegisterExternalServiceBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterExternalServiceBody {
    /// The `service_type` field.
    pub service_type: String,
    /// The `service_id` field.
    pub service_id: String,
    /// The `display_name` field.
    pub display_name: String,
    /// The `webhook_url` field.
    pub webhook_url: Option<String>,
    /// The `api_key` field.
    pub api_key: Option<String>,
    /// The `config` field.
    pub config: Option<serde_json::Value>,
}

/// The `UpdateExternalServiceBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateExternalServiceBody {
    /// The `webhook_url` field.
    pub webhook_url: Option<String>,
    /// The `api_key` field.
    pub api_key: Option<String>,
    /// The `config` field.
    pub config: Option<serde_json::Value>,
    /// The `is_enabled` field.
    pub is_enabled: Option<bool>,
}

/// The `ListServicesQuery` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListServicesQuery {
    #[serde(default)]
    /// The `service_type` field.
    pub service_type: Option<String>,
}

/// The `ExternalServiceResponse` struct.
#[derive(Debug, Serialize)]
pub struct ExternalServiceResponse {
    /// The `as_id` field.
    pub as_id: String,
    /// The `service_type` field.
    pub service_type: String,
    /// The `service_id` field.
    pub service_id: String,
    /// The `display_name` field.
    pub display_name: String,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `is_healthy` field.
    pub is_healthy: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
}

impl From<synapse_storage::application_service::ApplicationService> for ExternalServiceResponse {
    fn from(svc: synapse_storage::application_service::ApplicationService) -> Self {
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

fn external_service_integration(ctx: &AdminContext) -> Arc<ExternalServiceIntegration> {
    ctx.external_service_integration.clone()
}

fn parse_service_type(s: &str) -> Result<ExternalServiceType, ApiError> {
    match s.to_lowercase().as_str() {
        "trendradar" => Ok(ExternalServiceType::TrendRadar),
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

/// VULN-03/04/05 fix: webhook auth guard middleware.
///
/// Ensures at least one auth credential is present before the handler runs.
/// Without this, the `Json<Payload>` extractor deserializes the request body
/// first and returns 422 (leaking the API contract) when no credentials are
/// provided. This middleware returns 401 immediately if no credential header
/// is found, preventing body deserialization.
///
/// The actual credential *validation* still happens inside each handler via
/// `extract_webhook_auth`; this guard only checks *presence*.
async fn webhook_auth_guard(request: Request<Body>, next: axum::middleware::Next) -> Response {
    let headers = request.headers();
    let has_credential = headers.contains_key("authorization")
        || headers.contains_key("x-webhook-token")
        || headers.contains_key("x-api-key")
        || headers.contains_key("x-webhook-signature");

    if !has_credential {
        return ApiError::missing_token().into_response();
    }

    next.run(request).await
}

/// See [`register_external_service`].
pub async fn register_external_service(
    State(ctx): State<AdminContext>,
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

    let integration = external_service_integration(&ctx);

    let request_id = resolve_request_id(&headers);
    let service = integration.register_external_service(&request_id, config).await?;

    Ok((StatusCode::CREATED, Json(ExternalServiceResponse::from(service))))
}

/// See [`list_external_services`].
pub async fn list_external_services(
    State(ctx): State<AdminContext>,
    _admin: AdminUser,
    Query(query): Query<ListServicesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let stype = match query.service_type.as_deref() {
        Some("all") | None => None,
        Some(s) => Some(parse_service_type(s)?),
    };

    let integration = external_service_integration(&ctx);

    let services = integration.list_external_services(stype).await?;

    let response: Vec<ExternalServiceResponse> = services.into_iter().map(ExternalServiceResponse::from).collect();

    Ok(Json(response))
}

/// See [`get_external_service_health`].
pub async fn get_external_service_health(
    State(ctx): State<AdminContext>,
    Path(as_id): Path<String>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let integration = external_service_integration(&ctx);

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

/// See [`check_service_health`].
pub async fn check_service_health(
    State(ctx): State<AdminContext>,
    Path(as_id): Path<String>,
    headers: HeaderMap,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let integration = external_service_integration(&ctx);

    let request_id = resolve_request_id(&headers);
    let is_healthy = integration.check_service_health(&request_id, &as_id).await?;

    Ok(Json(serde_json::json!({
        "as_id": as_id,
        "is_healthy": is_healthy
    })))
}

/// See [`unregister_external_service`].
pub async fn unregister_external_service(
    State(ctx): State<AdminContext>,
    Path(as_id): Path<String>,
    headers: HeaderMap,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let integration = external_service_integration(&ctx);

    let request_id = resolve_request_id(&headers);
    integration.unregister_external_service(&request_id, &as_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// See [`update_external_service`].
pub async fn update_external_service(
    State(ctx): State<AdminContext>,
    Path(as_id): Path<String>,
    headers: HeaderMap,
    _admin: AdminUser,
    Json(body): Json<UpdateExternalServiceBody>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = external_service_integration(&ctx);

    let mut request = synapse_storage::application_service::UpdateApplicationServiceRequest::new();
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

/// See [`handle_trendradar_webhook`].
pub async fn handle_trendradar_webhook(
    State(ctx): State<AdminContext>,
    Path(service_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<TrendRadarPayload>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = external_service_integration(&ctx);

    let request_id = resolve_request_id(&headers);
    integration
        .handle_trendradar_webhook(&request_id, &service_id, payload, extract_webhook_auth(&headers, None))
        .await?;

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "TrendRadar webhook processed successfully"
    })))
}

/// See [`handle_generic_webhook`].
pub async fn handle_generic_webhook(
    State(ctx): State<AdminContext>,
    Path(service_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<WebhookPayload>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = external_service_integration(&ctx);

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

/// See [`get_all_health_status`].
pub async fn get_all_health_status(
    State(ctx): State<AdminContext>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let integration = external_service_integration(&ctx);

    let status_list = integration.get_all_health_status().await;

    Ok(Json(status_list))
}

/// See [`client_update_external_service`].
pub async fn client_update_external_service(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    _admin: AdminUser,
    Path(service_id): Path<String>,
    Json(body): Json<UpdateExternalServiceBody>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = external_service_integration(&ctx);

    let mut request = synapse_storage::application_service::UpdateApplicationServiceRequest::new();
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

/// See [`client_delete_external_service`].
pub async fn client_delete_external_service(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    _admin: AdminUser,
    Path(service_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = external_service_integration(&ctx);

    let request_id = resolve_request_id(&headers);
    integration.unregister_external_service(&request_id, &service_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// See [`client_health_check_all`].
pub async fn client_health_check_all(
    State(ctx): State<AdminContext>,
    _user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let integration = external_service_integration(&ctx);

    let status_list = integration.get_all_health_status().await;

    Ok(Json(status_list))
}

/// See [`create_external_service_router`].
pub fn create_external_service_router(state: AppState) -> Router<AppState> {
    let admin_routes =
        Router::new()
            .route("/_synapse/admin/v1/external_services", get(list_external_services).post(register_external_service))
            .route("/_synapse/admin/v1/external_services/{as_id}/health", get(get_external_service_health))
            .route("/_synapse/admin/v1/external_services/{as_id}/health/check", post(check_service_health))
            .route(
                "/_synapse/admin/v1/external_services/{as_id}",
                put(update_external_service).delete(unregister_external_service),
            )
            .route("/_synapse/admin/v1/external_services/health", get(get_all_health_status))
            .route_layer(
                axum::middleware::from_fn_with_state(
                    <crate::web::routes::context::AdminContext as axum::extract::FromRef<
                        crate::web::routes::AppState,
                    >>::from_ref(&state),
                    crate::web::middleware::admin_auth_middleware,
                ),
            );

    let admin_v1_routes =
        Router::new()
            .route("/_matrix/admin/v1/external_services", get(list_external_services).post(register_external_service))
            .route(
                "/_matrix/admin/v1/external_services/{as_id}",
                put(update_external_service).delete(unregister_external_service),
            )
            .route("/_matrix/admin/v1/external_services/health", get(get_all_health_status))
            .route_layer(
                axum::middleware::from_fn_with_state(
                    <crate::web::routes::context::AdminContext as axum::extract::FromRef<
                        crate::web::routes::AppState,
                    >>::from_ref(&state),
                    crate::web::middleware::admin_auth_middleware,
                ),
            );

    let client_v1_routes = Router::new()
        .route("/_matrix/client/v1/external_services/health", get(client_health_check_all))
        .route(
            "/_matrix/client/v1/external_services/{service_id}",
            put(client_update_external_service).delete(client_delete_external_service),
        )
        // ISSUE-13: vendor 前缀（私有端点，client 前缀保留为向后兼容别名）
        .route("/_matrix/vendor/v1/external_services/health", get(client_health_check_all))
        .route(
            "/_matrix/vendor/v1/external_services/{service_id}",
            put(client_update_external_service).delete(client_delete_external_service),
        );

    let public_routes = Router::new()
        .route("/_synapse/external/trendradar/{service_id}/webhook", post(handle_trendradar_webhook))
        .route("/_synapse/external/webhook/{service_id}", post(handle_generic_webhook));

    // VULN-03/04/05: require at least one auth credential header before body
    // deserialization. Without this, `Json<Payload>` returns 422 (leaking the
    // API contract) when no credentials are provided.
    let public_routes = public_routes.route_layer(axum::middleware::from_fn(webhook_auth_guard));

    public_routes.merge(admin_routes).merge(admin_v1_routes).merge(client_v1_routes).with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_service_type() {
        assert!(matches!(parse_service_type("trendradar"), Ok(ExternalServiceType::TrendRadar)));
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
