use crate::common::ApiError;
use crate::storage::application_service::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalServiceConfig {
    pub service_type: ExternalServiceType,
    pub service_id: String,
    pub display_name: String,
    pub webhook_url: Option<String>,
    pub api_key: Option<String>,
    pub config: serde_json::Value,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExternalServiceType {
    TrendRadar,
    OpenClaw,
    GenericWebhook,
    IrcBridge,
    SlackBridge,
    DiscordBridge,
    Custom,
}

impl std::fmt::Display for ExternalServiceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExternalServiceType::TrendRadar => write!(f, "trendradar"),
            ExternalServiceType::OpenClaw => write!(f, "openclaw"),
            ExternalServiceType::GenericWebhook => write!(f, "generic_webhook"),
            ExternalServiceType::IrcBridge => write!(f, "irc_bridge"),
            ExternalServiceType::SlackBridge => write!(f, "slack_bridge"),
            ExternalServiceType::DiscordBridge => write!(f, "discord_bridge"),
            ExternalServiceType::Custom => write!(f, "custom"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendRadarConfig {
    pub topic: String,
    pub server_url: Option<String>,
    pub include_rss: bool,
    pub include_hotlist: bool,
    pub keywords: Vec<String>,
    pub max_items: usize,
}

impl Default for TrendRadarConfig {
    fn default() -> Self {
        Self {
            topic: "matrix-news".to_string(),
            server_url: None,
            include_rss: true,
            include_hotlist: true,
            keywords: vec![],
            max_items: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendRadarPayload {
    pub title: String,
    pub content: String,
    pub source: String,
    pub timestamp: i64,
    pub url: Option<String>,
    pub keywords: Vec<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenClawConfig {
    pub agent_id: String,
    pub api_endpoint: String,
    pub capabilities: Vec<String>,
    pub auto_respond: bool,
}

impl Default for OpenClawConfig {
    fn default() -> Self {
        Self {
            agent_id: String::new(),
            api_endpoint: "http://localhost:8080".to_string(),
            capabilities: vec!["message".to_string(), "reaction".to_string()],
            auto_respond: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenClawPayload {
    pub action: String,
    pub room_id: String,
    pub event_id: String,
    pub content: serde_json::Value,
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    pub event_type: String,
    pub timestamp: i64,
    pub data: serde_json::Value,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WebhookAuthInput {
    pub token: Option<String>,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealthStatus {
    pub service_id: String,
    pub service_type: ExternalServiceType,
    pub is_healthy: bool,
    pub last_check_ts: i64,
    pub last_success_ts: Option<i64>,
    pub last_error: Option<String>,
    pub consecutive_failures: i32,
}

pub struct ExternalServiceIntegration {
    storage: Arc<ApplicationServiceStorage>,
    http_client: Client,
    server_name: String,
    health_status: Arc<tokio::sync::RwLock<HashMap<String, ServiceHealthStatus>>>,
}

impl ExternalServiceIntegration {
    pub fn new(storage: Arc<ApplicationServiceStorage>, server_name: String) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| {
                warn!("Failed to build HTTP client with custom config, using default");
                Client::new()
            });

        Self {
            storage,
            http_client,
            server_name,
            health_status: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    fn webhook_secrets<'a>(&self, service: &'a ApplicationService) -> Vec<&'a str> {
        let mut secrets = vec![service.as_token.as_str(), service.hs_token.as_str()];

        if let Some(api_key) = service.api_key.as_deref() {
            secrets.push(api_key);
        }

        for key in ["webhook_secret", "secret", "api_key"] {
            if let Some(secret) = service.config.get(key).and_then(|value| value.as_str()) {
                secrets.push(secret);
            }
        }

        secrets
    }

    fn verify_webhook_auth(
        &self,
        service: &ApplicationService,
        auth: &WebhookAuthInput,
        payload: &serde_json::Value,
    ) -> Result<(), ApiError> {
        let secrets = self.webhook_secrets(service);

        if auth
            .token
            .as_deref()
            .into_iter()
            .chain(auth.signature.as_deref())
            .any(|candidate| secrets.contains(&candidate))
        {
            return Ok(());
        }

        let payload_bytes = serde_json::to_vec(payload).map_err(|e| {
            ApiError::internal(format!("Failed to serialize webhook payload: {}", e))
        })?;

        let signature_matches = auth
            .signature
            .as_deref()
            .map(|signature| {
                let normalized = signature.strip_prefix("sha256=").unwrap_or(signature);
                secrets.iter().any(|secret| {
                    let expected = URL_SAFE_NO_PAD
                        .encode(crate::common::crypto::hmac_sha256(secret, &payload_bytes));
                    normalized == expected || signature == expected
                })
            })
            .unwrap_or(false);

        if signature_matches {
            return Ok(());
        }

        Err(ApiError::unauthorized(
            "Missing or invalid webhook credential".to_string(),
        ))
    }

    #[instrument(skip(self, config))]
    pub async fn register_external_service(
        &self,
        config: ExternalServiceConfig,
    ) -> Result<ApplicationService, ApiError> {
        info!(
            "Registering external service: type={}, id={}",
            config.service_type, config.service_id
        );

        let as_id = format!("{}_{}", config.service_type, config.service_id);

        if self
            .storage
            .get_by_id(&as_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check existing service: {}", e)))?
            .is_some()
        {
            return Err(ApiError::bad_request(format!(
                "External service '{}' already exists",
                as_id
            )));
        }

        let namespaces = self.generate_namespaces_for_service(&config);

        let request = RegisterApplicationServiceRequest {
            as_id: as_id.clone(),
            url: config.webhook_url.clone().unwrap_or_default(),
            as_token: uuid::Uuid::new_v4().to_string(),
            hs_token: uuid::Uuid::new_v4().to_string(),
            sender: format!("@{}-bot:{}", config.service_id, self.server_name),
            description: Some(format!("{} integration", config.service_type)),
            rate_limited: Some(false),
            protocols: Some(vec![config.service_type.to_string()]),
            namespaces: Some(namespaces),
            api_key: config.api_key.clone(),
            config: Some(config.config.clone()),
        };

        let service = self.storage.register(request).await.map_err(|e| {
            ApiError::internal(format!("Failed to register external service: {}", e))
        })?;

        self.health_status.write().await.insert(
            as_id.clone(),
            ServiceHealthStatus {
                service_id: as_id,
                service_type: config.service_type,
                is_healthy: true,
                last_check_ts: Utc::now().timestamp_millis(),
                last_success_ts: None,
                last_error: None,
                consecutive_failures: 0,
            },
        );

        info!(
            "External service registered successfully: {}",
            config.service_id
        );
        Ok(service)
    }

    fn generate_namespaces_for_service(&self, config: &ExternalServiceConfig) -> serde_json::Value {
        let user_prefix = format!("@{}_{{}}:{}", config.service_id, self.server_name);
        let alias_prefix = format!("#{}_{{}}:{}", config.service_id, self.server_name);

        match config.service_type {
            ExternalServiceType::TrendRadar => {
                serde_json::json!({
                    "users": [{
                        "exclusive": true,
                        "regex": format!(r"@trendradar_.*:{}", self.server_name)
                    }],
                    "aliases": [{
                        "exclusive": true,
                        "regex": format!(r"#trendradar_.*:{}", self.server_name)
                    }],
                    "rooms": []
                })
            }
            ExternalServiceType::OpenClaw => {
                serde_json::json!({
                    "users": [{
                        "exclusive": true,
                        "regex": format!(r"@openclaw_.*:{}", self.server_name)
                    }],
                    "aliases": [],
                    "rooms": []
                })
            }
            ExternalServiceType::IrcBridge => {
                serde_json::json!({
                    "users": [{
                        "exclusive": true,
                        "regex": format!(r"@irc_.*:{}", self.server_name)
                    }],
                    "aliases": [{
                        "exclusive": true,
                        "regex": format!(r"#irc_.*:{}", self.server_name)
                    }],
                    "rooms": []
                })
            }
            _ => {
                serde_json::json!({
                    "users": [{
                        "exclusive": true,
                        "regex": user_prefix.replace("{}", r".*")
                    }],
                    "aliases": [{
                        "exclusive": true,
                        "regex": alias_prefix.replace("{}", r".*")
                    }],
                    "rooms": []
                })
            }
        }
    }

    #[instrument(skip(self, payload))]
    pub async fn handle_trendradar_webhook(
        &self,
        service_id: &str,
        payload: TrendRadarPayload,
        auth: WebhookAuthInput,
    ) -> Result<(), ApiError> {
        info!(
            "Handling TrendRadar webhook: service={}, title={}",
            service_id, payload.title
        );

        let as_id = format!("trendradar_{}", service_id);
        let service = self
            .storage
            .get_by_id(&as_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get service: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Service not found"))?;

        let signed_payload = serde_json::to_value(&payload).map_err(|e| {
            ApiError::internal(format!("Failed to serialize webhook payload: {}", e))
        })?;
        self.verify_webhook_auth(&service, &auth, &signed_payload)?;

        let event_content = serde_json::json!({
            "msgtype": "m.text",
            "body": format!("**{}**\n\n{}\n\n来源: {} | 关键词: {}",
                payload.title,
                payload.content,
                payload.source,
                payload.keywords.join(", ")
            ),
            "format": "org.matrix.custom.html",
            "formatted_body": format!(
                "<h3>{}</h3><p>{}</p><p><small>来源: {} | 关键词: {}</small></p>",
                payload.title,
                payload.content,
                payload.source,
                payload.keywords.join(", ")
            ),
            "external_url": payload.url,
            "source": payload.source,
            "keywords": payload.keywords,
        });

        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);
        let room_id = format!("!trendradar_{}:{}", service_id, self.server_name);

        self.storage
            .add_event(
                &event_id,
                &as_id,
                &room_id,
                "m.room.message",
                &format!("@trendradar_{}:{}", service_id, self.server_name),
                event_content,
                None,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to add event: {}", e)))?;

        self.update_health_status(&as_id, true, None).await;

        Ok(())
    }

    #[instrument(skip(self, payload))]
    pub async fn handle_openclaw_webhook(
        &self,
        service_id: &str,
        payload: OpenClawPayload,
        auth: WebhookAuthInput,
    ) -> Result<(), ApiError> {
        info!(
            "Handling OpenClaw webhook: service={}, action={}",
            service_id, payload.action
        );

        let as_id = format!("openclaw_{}", service_id);
        let service = self
            .storage
            .get_by_id(&as_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get service: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Service not found"))?;

        let signed_payload = serde_json::to_value(&payload).map_err(|e| {
            ApiError::internal(format!("Failed to serialize webhook payload: {}", e))
        })?;
        self.verify_webhook_auth(&service, &auth, &signed_payload)?;

        let event_content = match payload.action.as_str() {
            "message" => serde_json::json!({
                "msgtype": "m.text",
                "body": payload.content.get("text").and_then(|t| t.as_str()).unwrap_or(""),
                "agent_id": service_id,
            }),
            "reaction" => serde_json::json!({
                "m.relates_to": {
                    "rel_type": "m.annotation",
                    "event_id": payload.event_id,
                    "key": payload.content.get("emoji").and_then(|e| e.as_str()).unwrap_or("👍"),
                }
            }),
            _ => {
                return Err(ApiError::bad_request(format!(
                    "Unknown OpenClaw action: {}",
                    payload.action
                )));
            }
        };

        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);
        let event_type = if payload.action == "reaction" {
            "m.reaction"
        } else {
            "m.room.message"
        };

        self.storage
            .add_event(
                &event_id,
                &as_id,
                &payload.room_id,
                event_type,
                &format!("@openclaw_{}:{}", service_id, self.server_name),
                event_content,
                None,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to add event: {}", e)))?;

        self.update_health_status(&as_id, true, None).await;

        Ok(())
    }

    #[instrument(skip(self, payload))]
    pub async fn handle_generic_webhook(
        &self,
        service_id: &str,
        payload: WebhookPayload,
        auth: WebhookAuthInput,
    ) -> Result<(), ApiError> {
        info!(
            "Handling generic webhook: service={}, event_type={}",
            service_id, payload.event_type
        );

        let as_id = format!("generic_webhook_{}", service_id);

        let service = self
            .storage
            .get_by_id(&as_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get service: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Service not found"))?;

        let mut signed_payload = serde_json::to_value(&payload).map_err(|e| {
            ApiError::internal(format!("Failed to serialize webhook payload: {}", e))
        })?;
        if let Some(object) = signed_payload.as_object_mut() {
            object.remove("signature");
        }
        self.verify_webhook_auth(&service, &auth, &signed_payload)?;

        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);
        let room_id = payload
            .data
            .get("room_id")
            .and_then(|r| r.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("!webhook_{}:{}", service_id, self.server_name));

        self.storage
            .add_event(
                &event_id,
                &as_id,
                &room_id,
                &payload.event_type,
                &format!("@webhook_{}:{}", service_id, self.server_name),
                payload.data.clone(),
                None,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to add event: {}", e)))?;

        self.update_health_status(&as_id, true, None).await;

        Ok(())
    }

    async fn update_health_status(&self, as_id: &str, success: bool, error: Option<String>) {
        let mut status = self.health_status.write().await;
        if let Some(health) = status.get_mut(as_id) {
            health.last_check_ts = Utc::now().timestamp_millis();
            if success {
                health.is_healthy = true;
                health.last_success_ts = Some(health.last_check_ts);
                health.consecutive_failures = 0;
                health.last_error = None;
            } else {
                health.consecutive_failures += 1;
                health.last_error = error;
                if health.consecutive_failures >= 3 {
                    health.is_healthy = false;
                }
            }
        }
    }

    #[instrument(skip(self))]
    pub async fn get_service_health(&self, service_id: &str) -> Option<ServiceHealthStatus> {
        let status = self.health_status.read().await;
        status.get(service_id).cloned()
    }

    #[instrument(skip(self))]
    pub async fn get_all_health_status(&self) -> Vec<ServiceHealthStatus> {
        let status = self.health_status.read().await;
        status.values().cloned().collect()
    }

    #[instrument(skip(self))]
    pub async fn check_service_health(&self, as_id: &str) -> Result<bool, ApiError> {
        let service = self
            .storage
            .get_by_id(as_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get service: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Service not found"))?;

        if service.url.is_empty() {
            return Ok(true);
        }

        let health_url = format!("{}/health", service.url);

        match self
            .http_client
            .get(&health_url)
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                self.update_health_status(as_id, true, None).await;
                Ok(true)
            }
            Ok(resp) => {
                let error = format!("Health check failed: HTTP {}", resp.status());
                self.update_health_status(as_id, false, Some(error.clone()))
                    .await;
                Ok(false)
            }
            Err(e) => {
                let error = format!("Health check failed: {}", e);
                self.update_health_status(as_id, false, Some(error.clone()))
                    .await;
                Ok(false)
            }
        }
    }

    #[instrument(skip(self))]
    pub async fn unregister_external_service(&self, service_id: &str) -> Result<(), ApiError> {
        info!("Unregistering external service: {}", service_id);

        self.storage
            .unregister(service_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to unregister service: {}", e)))?;

        self.health_status.write().await.remove(service_id);

        Ok(())
    }

    #[instrument(skip(self, request))]
    pub async fn update_external_service(
        &self,
        as_id: &str,
        request: UpdateApplicationServiceRequest,
    ) -> Result<ApplicationService, ApiError> {
        self.storage
            .update(as_id, &request)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update service: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn list_external_services(
        &self,
        service_type: Option<ExternalServiceType>,
    ) -> Result<Vec<ApplicationService>, ApiError> {
        let services = self
            .storage
            .get_all_active()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get services: {}", e)))?;

        if let Some(stype) = service_type {
            let prefix = format!("{}_", stype);
            Ok(services
                .into_iter()
                .filter(|s| s.as_id.starts_with(&prefix))
                .collect())
        } else {
            Ok(services)
        }
    }

    #[instrument(skip(self))]
    pub async fn send_to_external_service(
        &self,
        as_id: &str,
        event: serde_json::Value,
    ) -> Result<(), ApiError> {
        let service = self
            .storage
            .get_by_id(as_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get service: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Service not found"))?;

        if service.url.is_empty() {
            debug!("Service {} has no URL configured, skipping send", as_id);
            return Ok(());
        }

        let response = self
            .http_client
            .post(&service.url)
            .header("Authorization", format!("Bearer {}", service.hs_token))
            .json(&event)
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                self.update_health_status(as_id, true, None).await;
                Ok(())
            }
            Ok(resp) => {
                let error = format!("External service returned HTTP {}", resp.status());
                self.update_health_status(as_id, false, Some(error.clone()))
                    .await;
                Err(ApiError::internal(error))
            }
            Err(e) => {
                let error = format!("Failed to send to external service: {}", e);
                self.update_health_status(as_id, false, Some(error.clone()))
                    .await;
                Err(ApiError::internal(error))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_external_service_type_display() {
        assert_eq!(ExternalServiceType::TrendRadar.to_string(), "trendradar");
        assert_eq!(ExternalServiceType::OpenClaw.to_string(), "openclaw");
        assert_eq!(
            ExternalServiceType::GenericWebhook.to_string(),
            "generic_webhook"
        );
    }

    #[test]
    fn test_trendradar_config_default() {
        let config = TrendRadarConfig::default();
        assert_eq!(config.topic, "matrix-news");
        assert!(config.include_rss);
        assert!(config.include_hotlist);
        assert_eq!(config.max_items, 20);
    }

    #[test]
    fn test_openclaw_config_default() {
        let config = OpenClawConfig::default();
        assert_eq!(config.api_endpoint, "http://localhost:8080");
        assert!(!config.auto_respond);
    }

    #[test]
    fn test_trendradar_payload_serialization() {
        let payload = TrendRadarPayload {
            title: "Test News".to_string(),
            content: "Test content".to_string(),
            source: "test".to_string(),
            timestamp: 1234567890,
            url: Some("https://example.com".to_string()),
            keywords: vec!["test".to_string()],
            metadata: None,
        };

        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: TrendRadarPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(payload.title, deserialized.title);
    }

    #[test]
    fn test_webhook_payload_serialization() {
        let payload = WebhookPayload {
            event_type: "m.room.message".to_string(),
            timestamp: 1234567890,
            data: serde_json::json!({"body": "test"}),
            signature: Some("sig123".to_string()),
        };

        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: WebhookPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(payload.event_type, deserialized.event_type);
    }

    #[tokio::test]
    async fn test_verify_webhook_auth_accepts_direct_token_and_hmac_signature() {
        let integration = ExternalServiceIntegration::new(
            Arc::new(
                crate::storage::application_service::ApplicationServiceStorage::new(&Arc::new(
                    sqlx::postgres::PgPoolOptions::new()
                        .connect_lazy("postgres://localhost/test")
                        .expect("lazy pool"),
                )),
            ),
            "example.com".to_string(),
        );
        let service = ApplicationService {
            id: 1,
            as_id: "generic_webhook_test".to_string(),
            url: String::new(),
            as_token: "as-token".to_string(),
            hs_token: "hs-token".to_string(),
            sender_localpart: "@bot:example.com".to_string(),
            is_enabled: true,
            rate_limited: false,
            protocols: vec![],
            namespaces: serde_json::json!({}),
            created_ts: 0,
            updated_ts: None,
            description: None,
            api_key: Some("api-key".to_string()),
            config: serde_json::json!({
                "webhook_secret": "config-secret"
            }),
        };
        let payload = serde_json::json!({
            "event_type": "m.room.message",
            "timestamp": 1,
            "data": { "body": "hello" }
        });

        integration
            .verify_webhook_auth(
                &service,
                &WebhookAuthInput {
                    token: Some("api-key".to_string()),
                    signature: None,
                },
                &payload,
            )
            .expect("api key should authenticate");

        let signature = format!(
            "sha256={}",
            URL_SAFE_NO_PAD.encode(crate::common::crypto::hmac_sha256(
                "config-secret",
                serde_json::to_vec(&payload).unwrap()
            ))
        );

        integration
            .verify_webhook_auth(
                &service,
                &WebhookAuthInput {
                    token: None,
                    signature: Some(signature),
                },
                &payload,
            )
            .expect("hmac signature should authenticate");
    }

    #[test]
    fn test_service_health_status() {
        let status = ServiceHealthStatus {
            service_id: "test".to_string(),
            service_type: ExternalServiceType::TrendRadar,
            is_healthy: true,
            last_check_ts: 1234567890,
            last_success_ts: Some(1234567890),
            last_error: None,
            consecutive_failures: 0,
        };

        assert!(status.is_healthy);
        assert_eq!(status.consecutive_failures, 0);
    }
}
