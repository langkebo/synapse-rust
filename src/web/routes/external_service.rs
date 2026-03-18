use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::common::ApiError;
use crate::services::external_service_integration::*;
use crate::web::routes::AppState;

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
            display_name: svc.name.unwrap_or_else(|| svc.as_id.clone()),
            is_enabled: svc.is_enabled,
            is_healthy: true,
            created_ts: svc.created_ts,
        }
    }
}

fn parse_service_type(s: &str) -> Result<ExternalServiceType, ApiError> {
    match s.to_lowercase().as_str() {
        "trendradar" => Ok(ExternalServiceType::TrendRadar),
        "openclaw" => Ok(ExternalServiceType::OpenClaw),
        "generic_webhook" | "webhook" => Ok(ExternalServiceType::GenericWebhook),
        "irc_bridge" | "irc" => Ok(ExternalServiceType::IrcBridge),
        "slack_bridge" | "slack" => Ok(ExternalServiceType::SlackBridge),
        "discord_bridge" | "discord" => Ok(ExternalServiceType::DiscordBridge),
        "custom" => Ok(ExternalServiceType::Custom),
        _ => Err(ApiError::bad_request(format!("Unknown service type: {}", s))),
    }
}

pub async fn register_external_service(
    State(state): State<AppState>,
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
        Arc::new(state.services.app_service_storage.clone()),
        state.services.server_name.clone(),
    );

    let service = integration.register_external_service(config).await?;

    Ok((
        StatusCode::CREATED,
        Json(ExternalServiceResponse::from(service)),
    ))
}

pub async fn list_external_services(
    State(state): State<AppState>,
    Query(query): Query<ListServicesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let stype = match query.service_type.as_deref() {
        Some("all") | None => None,
        Some(s) => Some(parse_service_type(s)?),
    };

    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.app_service_storage.clone()),
        state.services.server_name.clone(),
    );

    let services = integration.list_external_services(stype).await?;

    let response: Vec<ExternalServiceResponse> =
        services.into_iter().map(ExternalServiceResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_external_service_health(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.app_service_storage.clone()),
        state.services.server_name.clone(),
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
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.app_service_storage.clone()),
        state.services.server_name.clone(),
    );

    let is_healthy = integration.check_service_health(&as_id).await?;

    Ok(Json(serde_json::json!({
        "as_id": as_id,
        "is_healthy": is_healthy
    })))
}

pub async fn unregister_external_service(
    State(state): State<AppState>,
    Path(as_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.app_service_storage.clone()),
        state.services.server_name.clone(),
    );

    integration.unregister_external_service(&as_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn handle_trendradar_webhook(
    State(state): State<AppState>,
    Path(service_id): Path<String>,
    Json(payload): Json<TrendRadarPayload>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.app_service_storage.clone()),
        state.services.server_name.clone(),
    );

    integration.handle_trendradar_webhook(&service_id, payload).await?;

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "TrendRadar webhook processed successfully"
    })))
}

pub async fn handle_openclaw_webhook(
    State(state): State<AppState>,
    Path(service_id): Path<String>,
    Json(payload): Json<OpenClawPayload>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.app_service_storage.clone()),
        state.services.server_name.clone(),
    );

    integration.handle_openclaw_webhook(&service_id, payload).await?;

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "OpenClaw webhook processed successfully"
    })))
}

pub async fn handle_generic_webhook(
    State(state): State<AppState>,
    Path(service_id): Path<String>,
    Json(payload): Json<WebhookPayload>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.app_service_storage.clone()),
        state.services.server_name.clone(),
    );

    integration.handle_generic_webhook(&service_id, payload).await?;

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Webhook processed successfully"
    })))
}

pub async fn get_all_health_status(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let integration = ExternalServiceIntegration::new(
        Arc::new(state.services.app_service_storage.clone()),
        state.services.server_name.clone(),
    );

    let status_list = integration.get_all_health_status().await;

    Ok(Json(status_list))
}

pub fn create_external_service_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_synapse/admin/v1/external_services",
            get(list_external_services).post(register_external_service),
        )
        // 具体的路由必须在参数路由之前
        .route(
            "/_synapse/admin/v1/external_services/{as_id}/health",
            get(get_external_service_health),
        )
        .route(
            "/_synapse/admin/v1/external_services/{as_id}/health/check",
            post(check_service_health),
        )
        .route(
            "/_synapse/admin/v1/external_services/{as_id}",
            delete(unregister_external_service),
        )
        .route(
            "/_synapse/admin/v1/external_services/health",
            get(get_all_health_status),
        )
        .route(
            "/_synapse/external/trendradar/{service_id}/webhook",
            post(handle_trendradar_webhook),
        )
        .route(
            "/_synapse/external/openclaw/{service_id}/webhook",
            post(handle_openclaw_webhook),
        )
        .route(
            "/_synapse/external/webhook/{service_id}",
            post(handle_generic_webhook),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_service_type() {
        assert!(matches!(parse_service_type("trendradar"), Ok(ExternalServiceType::TrendRadar)));
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
