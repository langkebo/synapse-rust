use regex::Regex;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use synapse_common::ApiError;
use synapse_storage::{application_service::*, EventStorage};
use tokio::fs;
use tracing::{error, info, instrument, warn};
use url::Url;

pub mod scheduler;
pub use scheduler::ApplicationServiceScheduler;

const APPSERVICE_RETRY_BACKOFF_BASE_MS: i64 = 5_000;
const APPSERVICE_RETRY_BACKOFF_MAX_MS: i64 = 5 * 60 * 1_000;
const APPSERVICE_FATAL_FAILURE_THRESHOLD: i32 = 3;
const APPSERVICE_RETRYABLE_FAILURE_THRESHOLD: i32 = 8;
const APPSERVICE_STATE_DELIVERY_STATUS: &str = "delivery_status";
const APPSERVICE_STATE_DELIVERY_LAST_ERROR: &str = "delivery_last_error";
const APPSERVICE_STATE_DELIVERY_LAST_FAILURE_KIND: &str = "delivery_last_failure_kind";
const APPSERVICE_STATE_DELIVERY_LAST_FAILURE_TS: &str = "delivery_last_failure_ts";
const APPSERVICE_STATE_DELIVERY_DISABLED_REASON: &str = "delivery_disabled_reason";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransactionFailureKind {
    Retryable,
    Fatal,
}

pub struct ApplicationServiceManager {
    storage: Arc<ApplicationServiceStorage>,
    event_storage: Arc<EventStorage>,
    http_client: Client,
    server_name: String,
}

#[derive(Debug, Deserialize)]
struct AppServiceConfigFile {
    id: String,
    url: String,
    as_token: String,
    hs_token: String,
    #[serde(default)]
    sender: Option<String>,
    #[serde(default)]
    sender_localpart: Option<String>,
    #[serde(default, rename = "rate_limited")]
    is_rate_limited: Option<bool>,
    #[serde(default)]
    protocols: Vec<String>,
    #[serde(default)]
    namespaces: AppServiceConfigNamespaces,
    #[serde(default)]
    description: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Default, Deserialize)]
struct AppServiceConfigNamespaces {
    #[serde(default)]
    users: Vec<AppServiceConfigNamespaceRule>,
    #[serde(default)]
    aliases: Vec<AppServiceConfigNamespaceRule>,
    #[serde(default)]
    rooms: Vec<AppServiceConfigNamespaceRule>,
}

#[derive(Debug, Deserialize)]
struct AppServiceConfigNamespaceRule {
    #[serde(rename = "exclusive")]
    exclusive: bool,
    regex: String,
    #[serde(default)]
    group_id: Option<String>,
}

impl ApplicationServiceManager {
    pub fn new(storage: Arc<ApplicationServiceStorage>, event_storage: Arc<EventStorage>, server_name: String) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|e| {
                tracing::warn!(
                    error = %e,
                    timeout_secs = 15_u64,
                    connect_timeout_secs = 5_u64,
                    pool_idle_timeout_secs = 60_u64,
                    "Failed to build HTTP client with custom config, using default"
                );
                Client::new()
            });

        Self { storage, event_storage, http_client, server_name }
    }

    #[instrument(skip(self, config_files))]
    pub async fn load_from_config_files(&self, config_files: &[String]) -> Result<Vec<ApplicationService>, ApiError> {
        let mut imported_services = Vec::with_capacity(config_files.len());

        for config_file in config_files {
            let config_path = Path::new(config_file);
            let service = self.load_from_config_file(config_path).await?;
            imported_services.push(service);
        }

        Ok(imported_services)
    }

    #[instrument(skip(self))]
    pub async fn load_from_config_file(&self, config_path: &Path) -> Result<ApplicationService, ApiError> {
        let config_display = config_path.display().to_string();
        let raw_config = fs::read_to_string(config_path)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to read application service config", &e))?;
        let request = self.parse_config_file_contents(&raw_config, &config_display)?;
        let service = self
            .storage
            .upsert_registration(request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to import application service config", &e))?;

        info!(config_path = %config_display, as_id = %service.as_id, "Application service config imported");
        Ok(service)
    }

    #[instrument(skip(self, request))]
    pub async fn register(&self, request: RegisterApplicationServiceRequest) -> Result<ApplicationService, ApiError> {
        info!(as_id = %request.as_id, "Registering application service");

        if let Some(existing) = self
            .storage
            .get_by_id(&request.as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check existing service", &e))?
        {
            return Err(ApiError::bad_request(format!("Application service '{}' already exists", existing.as_id)));
        }

        let service = self
            .storage
            .register(request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to register application service", &e))?;

        info!(as_id = %service.as_id, sender = %service.sender_localpart, "Application service registered successfully");
        Ok(service)
    }

    #[instrument(skip(self))]
    pub async fn get(&self, as_id: &str) -> Result<Option<ApplicationService>, ApiError> {
        self.storage
            .get_by_id(as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get application service", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_by_token(&self, as_token: &str) -> Result<Option<ApplicationService>, ApiError> {
        let service = self
            .storage
            .get_by_token(as_token)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get application service by token", &e))?;

        if let Some(ref svc) = service {
            let _ = self.storage.update_last_seen(&svc.as_id).await.map_err(|e| {
                warn!(%e, as_id = svc.as_id, "Failed to update last seen");
            });
        }

        Ok(service)
    }

    #[instrument(skip(self))]
    pub async fn get_all_active(&self) -> Result<Vec<ApplicationService>, ApiError> {
        self.storage
            .get_all_active()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get active services", &e))
    }

    #[instrument(skip(self))]
    pub async fn update(
        &self,
        as_id: &str,
        request: UpdateApplicationServiceRequest,
    ) -> Result<ApplicationService, ApiError> {
        info!(as_id = %as_id, "Updating application service");

        let service = self
            .storage
            .update(as_id, &request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update application service", &e))?;

        info!(as_id = %as_id, "Application service updated successfully");
        Ok(service)
    }

    #[instrument(skip(self))]
    pub async fn unregister(&self, as_id: &str) -> Result<(), ApiError> {
        info!(as_id = %as_id, "Unregistering application service");

        self.storage
            .unregister(as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to unregister application service", &e))?;

        info!(as_id = %as_id, "Application service unregistered successfully");
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn validate_token(&self, as_token: &str) -> Result<ApplicationService, ApiError> {
        self.get_by_token(as_token).await?.ok_or_else(|| ApiError::unauthorized("Invalid application service token"))
    }

    #[instrument(skip(self))]
    pub async fn set_state(
        &self,
        as_id: &str,
        state_key: &str,
        state_value: &str,
    ) -> Result<ApplicationServiceState, ApiError> {
        self.storage
            .set_state(as_id, state_key, state_value)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to set state", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_state(&self, as_id: &str, state_key: &str) -> Result<Option<ApplicationServiceState>, ApiError> {
        self.storage
            .get_state(as_id, state_key)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get state", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_all_states(&self, as_id: &str) -> Result<Vec<ApplicationServiceState>, ApiError> {
        self.storage.get_all_states(as_id).await.map_err(|e| ApiError::internal_with_log("Failed to get states", &e))
    }

    #[instrument(skip(self, content))]
    pub async fn push_event(
        &self,
        as_id: &str,
        room_id: &str,
        event_type: &str,
        sender: &str,
        content: serde_json::Value,
        state_key: Option<&str>,
    ) -> Result<ApplicationServiceEvent, ApiError> {
        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);

        let event = self
            .storage
            .add_event(&event_id, as_id, room_id, event_type, sender, content, state_key)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add event", &e))?;

        info!(as_id = %as_id, event_id = %event_id, room_id = %room_id, event_type = %event_type, "Event pushed to application service");
        Ok(event)
    }

    #[instrument(skip(self, content))]
    pub async fn enqueue_matching_event(
        &self,
        source_event_id: &str,
        room_id: &str,
        event_type: &str,
        sender: &str,
        content: &serde_json::Value,
        state_key: Option<&str>,
    ) -> Result<usize, ApiError> {
        let services = self.get_all_active().await?;
        let mut enqueued = 0_usize;

        for service in services {
            if !self.service_matches_event(&service, room_id, sender, state_key) {
                continue;
            }

            let queue_event_id = format!("{source_event_id}::{}", service.as_id);
            self.storage
                .add_event(&queue_event_id, &service.as_id, room_id, event_type, sender, content.clone(), state_key)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to enqueue application service event", &e))?;
            enqueued += 1;
        }

        Ok(enqueued)
    }

    #[instrument(skip(self))]
    pub async fn get_pending_events(&self, as_id: &str, limit: i64) -> Result<Vec<ApplicationServiceEvent>, ApiError> {
        self.storage
            .get_pending_events(as_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get pending events", &e))
    }

    #[instrument(skip(self))]
    pub async fn send_transaction(&self, as_id: &str, events: Vec<serde_json::Value>) -> Result<(), ApiError> {
        let service = self
            .storage
            .get_by_id(as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get service", &e))?
            .ok_or_else(|| ApiError::not_found("Application service not found"))?;

        let transaction_id = format!("{}", uuid::Uuid::new_v4());

        let _transaction = self
            .storage
            .create_transaction(as_id, &transaction_id, &events)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create transaction", &e))?;

        self.deliver_transaction(&service, &transaction_id, &events).await
    }

    pub async fn process_pending_for_service(&self, as_id: &str, batch_limit: i64) -> Result<usize, ApiError> {
        let service = self
            .storage
            .get_by_id(as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get application service", &e))?
            .ok_or_else(|| ApiError::not_found("Application service not found"))?;

        let pending_transactions =
            self.storage.get_pending_transactions(as_id).await.map_err(|e| {
                ApiError::internal_with_log("Failed to get pending application service transactions", &e)
            })?;
        if let Some(transaction) = pending_transactions.first() {
            let now = chrono::Utc::now().timestamp_millis();
            if !Self::is_transaction_ready_to_retry(transaction, now) {
                return Ok(0);
            }

            let events: Vec<serde_json::Value> = serde_json::from_value(transaction.events.clone()).map_err(|e| {
                ApiError::internal_with_log("Failed to decode pending application service transaction", &e)
            })?;
            self.deliver_transaction(&service, &transaction.transaction_id, &events).await?;
            return Ok(0);
        }

        let pending_events = self
            .storage
            .get_pending_events(as_id, batch_limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get pending application service events", &e))?;
        if pending_events.is_empty() {
            return Ok(0);
        }

        let events = self.build_transaction_events(&pending_events).await?;
        let transaction_id = uuid::Uuid::new_v4().to_string();
        self.storage
            .create_transaction(as_id, &transaction_id, &events)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create application service transaction", &e))?;
        self.deliver_transaction(&service, &transaction_id, &events).await?;

        Ok(pending_events.len())
    }

    pub async fn process_pending_queues(&self, batch_limit: i64) -> Result<usize, ApiError> {
        let services = self.get_all_active().await?;
        let mut dispatched = 0_usize;

        for service in services {
            dispatched += self.process_pending_for_service(&service.as_id, batch_limit).await?;
        }

        Ok(dispatched)
    }

    pub async fn start_sender(self: Arc<Self>, batch_limit: i64, flush_interval_secs: u64) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(flush_interval_secs.max(1)));
            loop {
                interval.tick().await;
                match self.process_pending_queues(batch_limit).await {
                    Ok(dispatched) if dispatched > 0 => {
                        info!(dispatched, batch_limit, "Dispatched application service pending events");
                    }
                    Ok(_) => {}
                    Err(error) => {
                        warn!(error = %error, "Failed to process application service pending queues");
                    }
                }
            }
        });
    }

    async fn deliver_transaction(
        &self,
        service: &ApplicationService,
        transaction_id: &str,
        events: &[serde_json::Value],
    ) -> Result<(), ApiError> {
        let url = format!("{}/transactions/{}", service.url, transaction_id);

        let response = self
            .http_client
            .put(&url)
            .header("Authorization", format!("Bearer {}", service.hs_token))
            .json(&json!({
                "events": events
            }))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                if let Err(e) = self.storage.complete_transaction(&service.as_id, transaction_id).await {
                    error!(%e, as_id = %service.as_id, transaction_id, "Failed to complete transaction");
                }
                self.record_delivery_success(&service.as_id).await;

                for event in events {
                    if let Some(event_id) = event
                        .get("queue_event_id")
                        .and_then(|value| value.as_str())
                        .or_else(|| event.get("event_id").and_then(|value| value.as_str()))
                    {
                        if let Err(e) = self.storage.mark_event_processed(event_id).await {
                            warn!(%e, as_id = %service.as_id, transaction_id, event_id, "Failed to mark event processed");
                        }
                    }
                }

                info!(as_id = %service.as_id, transaction_id, "Transaction sent successfully");
                Ok(())
            }
            Ok(resp) => {
                let status = resp.status();
                let error_body = resp.text().await.unwrap_or_default();
                let failure_kind = Self::classify_http_failure(status);
                let failure_reason = format!("HTTP {status}: {error_body}");
                self.handle_transaction_failure(service, transaction_id, &failure_reason, failure_kind).await;

                Err(ApiError::internal_with_log("Application service returned error", &format!("HTTP {status}")))
            }
            Err(e) => {
                self.handle_transaction_failure(
                    service,
                    transaction_id,
                    &e.to_string(),
                    TransactionFailureKind::Retryable,
                )
                .await;

                Err(ApiError::internal_with_log("Failed to send transaction", &e))
            }
        }
    }

    #[instrument(skip(self))]
    pub async fn query_user(&self, user_id: &str) -> Result<Option<String>, ApiError> {
        self.storage
            .is_user_in_namespace(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to query user namespace", &e))
    }

    #[instrument(skip(self))]
    pub async fn query_room_alias(&self, alias: &str) -> Result<Option<String>, ApiError> {
        self.storage
            .is_room_alias_in_namespace(alias)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to query room alias namespace", &e))
    }

    #[instrument(skip(self))]
    pub async fn query_room_id(&self, room_id: &str) -> Result<Option<String>, ApiError> {
        self.storage
            .is_room_id_in_namespace(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to query room namespace", &e))
    }

    #[instrument(skip(self))]
    pub async fn register_virtual_user(
        &self,
        as_id: &str,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<ApplicationServiceUser, ApiError> {
        info!(as_id = %as_id, user_id = %user_id, "Registering virtual user");

        let user = self
            .storage
            .register_virtual_user(as_id, user_id, displayname, avatar_url)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to register virtual user", &e))?;

        info!(as_id = %as_id, user_id = %user_id, "Virtual user registered successfully");
        Ok(user)
    }

    #[instrument(skip(self))]
    pub async fn get_virtual_users(&self, as_id: &str) -> Result<Vec<ApplicationServiceUser>, ApiError> {
        self.storage
            .get_virtual_users(as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get virtual users", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_namespaces(&self, as_id: &str) -> Result<NamespacesInfo, ApiError> {
        let users = self
            .storage
            .get_user_namespaces(as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get user namespaces", &e))?;
        let aliases = self
            .storage
            .get_room_alias_namespaces(as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room alias namespaces", &e))?;
        let rooms = self
            .storage
            .get_room_namespaces(as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room namespaces", &e))?;

        Ok(NamespacesInfo { users, aliases, rooms })
    }

    #[instrument(skip(self))]
    pub async fn get_statistics(&self) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage.get_statistics().await.map_err(|e| ApiError::internal_with_log("Failed to get statistics", &e))
    }

    pub async fn ping(&self, as_id: &str) -> Result<bool, ApiError> {
        let service = self
            .storage
            .get_by_id(as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get service", &e))?
            .ok_or_else(|| ApiError::not_found("Application service not found"))?;

        let url = format!("{}/_matrix/app/v1/ping", service.url);

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", service.hs_token))
            .json(&json!({}))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                let _ = self
                    .storage
                    .update_last_seen(as_id)
                    .await
                    .map_err(|e| warn!(%e, as_id, "Failed to update last seen"));
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn parse_config_file_contents(
        &self,
        raw_config: &str,
        config_label: &str,
    ) -> Result<RegisterApplicationServiceRequest, ApiError> {
        let config: AppServiceConfigFile = serde_yaml::from_str(raw_config).map_err(|e| {
            ApiError::bad_request(format!("Invalid application service config '{}': {}", config_label, e))
        })?;

        self.validate_config_file(&config, config_label)?;
        let AppServiceConfigFile {
            id,
            url,
            as_token,
            hs_token,
            sender,
            sender_localpart,
            is_rate_limited,
            protocols,
            namespaces,
            description,
            extra,
        } = config;

        let sender = self.normalize_sender(sender, sender_localpart, config_label)?;
        let namespaces = self.namespaces_to_json(&namespaces);
        let protocols = (!protocols.is_empty()).then_some(protocols);
        let description = description.and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        });
        let config_json = if extra.is_empty() {
            None
        } else {
            Some(serde_json::to_value(extra).map_err(|e| {
                ApiError::internal_with_log("Failed to serialize application service config extras", &e)
            })?)
        };

        Ok(RegisterApplicationServiceRequest {
            as_id: id.trim().to_owned(),
            url: url.trim().to_owned(),
            as_token: as_token.trim().to_owned(),
            hs_token: hs_token.trim().to_owned(),
            sender,
            description,
            is_rate_limited,
            protocols,
            namespaces: Some(namespaces),
            api_key: None,
            config: config_json,
        })
    }

    fn validate_config_file(&self, config: &AppServiceConfigFile, config_label: &str) -> Result<(), ApiError> {
        if config.id.trim().is_empty() {
            return Err(ApiError::bad_request(format!(
                "Application service config '{}' is missing a non-empty id",
                config_label
            )));
        }

        if config.url.trim().is_empty() {
            return Err(ApiError::bad_request(format!(
                "Application service config '{}' is missing a non-empty url",
                config_label
            )));
        }

        Url::parse(config.url.trim()).map_err(|e| {
            ApiError::bad_request(format!(
                "Application service config '{}' has invalid url '{}': {}",
                config_label, config.url, e
            ))
        })?;

        if config.as_token.trim().is_empty() {
            return Err(ApiError::bad_request(format!(
                "Application service config '{}' is missing a non-empty as_token",
                config_label
            )));
        }

        if config.hs_token.trim().is_empty() {
            return Err(ApiError::bad_request(format!(
                "Application service config '{}' is missing a non-empty hs_token",
                config_label
            )));
        }

        self.validate_namespace_rules("users", &config.namespaces.users, config_label)?;
        self.validate_namespace_rules("aliases", &config.namespaces.aliases, config_label)?;
        self.validate_namespace_rules("rooms", &config.namespaces.rooms, config_label)?;

        Ok(())
    }

    fn validate_namespace_rules(
        &self,
        namespace_kind: &str,
        rules: &[AppServiceConfigNamespaceRule],
        config_label: &str,
    ) -> Result<(), ApiError> {
        for rule in rules {
            let regex = rule.regex.trim();
            if regex.is_empty() {
                return Err(ApiError::bad_request(format!(
                    "Application service config '{}' has an empty {} namespace regex",
                    config_label, namespace_kind
                )));
            }

            Regex::new(regex).map_err(|e| {
                ApiError::bad_request(format!(
                    "Application service config '{}' has invalid {} namespace regex '{}': {}",
                    config_label, namespace_kind, regex, e
                ))
            })?;
        }

        Ok(())
    }

    fn normalize_sender(
        &self,
        sender: Option<String>,
        sender_localpart: Option<String>,
        config_label: &str,
    ) -> Result<String, ApiError> {
        let raw_sender = sender.or(sender_localpart).ok_or_else(|| {
            ApiError::bad_request(format!(
                "Application service config '{}' is missing sender or sender_localpart",
                config_label
            ))
        })?;
        let raw_sender = raw_sender.trim();

        if raw_sender.is_empty() {
            return Err(ApiError::bad_request(format!(
                "Application service config '{}' has an empty sender value",
                config_label
            )));
        }

        if let Some(stripped) = raw_sender.strip_prefix('@') {
            if let Some((localpart, server_name)) = stripped.split_once(':') {
                if !localpart.is_empty() && !server_name.is_empty() {
                    return Ok(raw_sender.to_owned());
                }
            }

            if stripped.is_empty() {
                return Err(ApiError::bad_request(format!(
                    "Application service config '{}' has an invalid sender '{}'",
                    config_label, raw_sender
                )));
            }

            return Ok(format!("@{}:{}", stripped, self.server_name));
        }

        if let Some((localpart, server_name)) = raw_sender.split_once(':') {
            if !localpart.is_empty() && !server_name.is_empty() {
                return Ok(format!("@{}:{}", localpart, server_name));
            }
        }

        Ok(format!("@{}:{}", raw_sender, self.server_name))
    }

    fn namespaces_to_json(&self, namespaces: &AppServiceConfigNamespaces) -> serde_json::Value {
        json!({
            "users": self.namespace_rules_to_json(&namespaces.users),
            "aliases": self.namespace_rules_to_json(&namespaces.aliases),
            "rooms": self.namespace_rules_to_json(&namespaces.rooms),
        })
    }

    fn namespace_rules_to_json(&self, rules: &[AppServiceConfigNamespaceRule]) -> Vec<serde_json::Value> {
        rules
            .iter()
            .map(|rule| {
                json!({
                    "exclusive": rule.exclusive,
                    "regex": rule.regex.trim(),
                    "group_id": rule.group_id,
                })
            })
            .collect()
    }

    fn service_matches_event(
        &self,
        service: &ApplicationService,
        room_id: &str,
        sender: &str,
        state_key: Option<&str>,
    ) -> bool {
        self.namespace_matches(&service.namespaces, "rooms", room_id)
            || self.namespace_matches(&service.namespaces, "users", sender)
            || state_key.is_some_and(|key| self.namespace_matches(&service.namespaces, "users", key))
    }

    fn namespace_matches(&self, namespaces: &serde_json::Value, namespace_kind: &str, candidate: &str) -> bool {
        namespaces
            .get(namespace_kind)
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|rule| rule.get("regex").and_then(|value| value.as_str()))
            .any(|pattern| Regex::new(pattern).is_ok_and(|regex| regex.is_match(candidate)))
    }

    async fn build_transaction_events(
        &self,
        pending_events: &[ApplicationServiceEvent],
    ) -> Result<Vec<serde_json::Value>, ApiError> {
        let mut events = Vec::with_capacity(pending_events.len());

        for pending_event in pending_events {
            events.push(self.build_transaction_event(pending_event).await?);
        }

        Ok(events)
    }

    async fn build_transaction_event(
        &self,
        pending_event: &ApplicationServiceEvent,
    ) -> Result<serde_json::Value, ApiError> {
        let source_event_id = Self::source_event_id(&pending_event.event_id);
        let source_event =
            self.event_storage.get_event(&source_event_id).await.map_err(|e| {
                ApiError::internal_with_log("Failed to load source room event for application service", &e)
            })?;

        if let Some(source_event) = source_event {
            return Ok(json!({
                "event_id": source_event.event_id,
                "queue_event_id": pending_event.event_id,
                "room_id": source_event.room_id,
                "type": source_event.event_type,
                "sender": source_event.user_id,
                "content": source_event.content,
                "state_key": source_event.state_key,
                "origin_server_ts": source_event.origin_server_ts,
            }));
        }

        warn!(
            queue_event_id = %pending_event.event_id,
            source_event_id = %source_event_id,
            "Falling back to minimal application service event payload because source room event was not found"
        );

        Ok(json!({
            "event_id": source_event_id,
            "queue_event_id": pending_event.event_id,
            "room_id": pending_event.room_id,
            "type": pending_event.event_type,
            "sender": pending_event.sender,
            "content": pending_event.content,
            "state_key": pending_event.state_key,
            "origin_server_ts": pending_event.origin_server_ts,
        }))
    }

    fn source_event_id(queue_event_id: &str) -> String {
        queue_event_id
            .rsplit_once("::")
            .map_or_else(|| queue_event_id.to_owned(), |(source_event_id, _)| source_event_id.to_owned())
    }

    async fn handle_transaction_failure(
        &self,
        service: &ApplicationService,
        transaction_id: &str,
        failure_reason: &str,
        failure_kind: TransactionFailureKind,
    ) {
        let failed_transaction =
            match self.storage.fail_transaction(&service.as_id, transaction_id, failure_reason).await {
                Ok(transaction) => transaction,
                Err(e) => {
                    error!(%e, as_id = %service.as_id, transaction_id, "Failed to fail transaction");
                    return;
                }
            };

        self.record_delivery_failure(&service.as_id, failure_reason, failure_kind, failed_transaction.sent_ts).await;

        if Self::should_disable_service(failure_kind, failed_transaction.retry_count) {
            self.disable_service_for_delivery_failure(service, &failed_transaction, failure_reason, failure_kind).await;
        }
    }

    async fn record_delivery_success(&self, as_id: &str) {
        self.set_delivery_state(as_id, APPSERVICE_STATE_DELIVERY_STATUS, "up").await;
        self.set_delivery_state(as_id, APPSERVICE_STATE_DELIVERY_LAST_ERROR, "").await;
        self.set_delivery_state(as_id, APPSERVICE_STATE_DELIVERY_DISABLED_REASON, "").await;
    }

    async fn record_delivery_failure(
        &self,
        as_id: &str,
        failure_reason: &str,
        failure_kind: TransactionFailureKind,
        failed_ts: i64,
    ) {
        self.set_delivery_state(as_id, APPSERVICE_STATE_DELIVERY_STATUS, "failing").await;
        self.set_delivery_state(as_id, APPSERVICE_STATE_DELIVERY_LAST_ERROR, failure_reason).await;
        self.set_delivery_state(as_id, APPSERVICE_STATE_DELIVERY_LAST_FAILURE_KIND, failure_kind.as_str()).await;
        self.set_delivery_state(as_id, APPSERVICE_STATE_DELIVERY_LAST_FAILURE_TS, &failed_ts.to_string()).await;
    }

    async fn disable_service_for_delivery_failure(
        &self,
        service: &ApplicationService,
        failed_transaction: &ApplicationServiceTransaction,
        failure_reason: &str,
        failure_kind: TransactionFailureKind,
    ) {
        let disable_reason = format!(
            "{} delivery failure threshold reached after {} attempts: {}",
            failure_kind.as_str(),
            failed_transaction.retry_count,
            failure_reason
        );

        match self.storage.update(&service.as_id, &UpdateApplicationServiceRequest::new().is_enabled(false)).await {
            Ok(_) => {
                self.set_delivery_state(&service.as_id, APPSERVICE_STATE_DELIVERY_STATUS, "disabled").await;
                self.set_delivery_state(&service.as_id, APPSERVICE_STATE_DELIVERY_DISABLED_REASON, &disable_reason)
                    .await;
                warn!(
                    as_id = %service.as_id,
                    transaction_id = %failed_transaction.transaction_id,
                    retry_count = failed_transaction.retry_count,
                    failure_kind = failure_kind.as_str(),
                    failure_reason = %failure_reason,
                    "Disabled application service after repeated delivery failures"
                );
            }
            Err(e) => {
                error!(
                    %e,
                    as_id = %service.as_id,
                    transaction_id = %failed_transaction.transaction_id,
                    "Failed to disable application service after repeated delivery failures"
                );
            }
        }
    }

    async fn set_delivery_state(&self, as_id: &str, state_key: &str, state_value: &str) {
        if let Err(e) = self.storage.set_state(as_id, state_key, state_value).await {
            warn!(%e, as_id, state_key, "Failed to update application service delivery state");
        }
    }

    fn is_transaction_ready_to_retry(transaction: &ApplicationServiceTransaction, now_ts: i64) -> bool {
        now_ts.saturating_sub(transaction.sent_ts) >= Self::retry_backoff_ms(transaction.retry_count)
    }

    fn retry_backoff_ms(retry_count: i32) -> i64 {
        if retry_count <= 0 {
            return 0;
        }

        let exponential = 1_i64.checked_shl((retry_count - 1).min(16) as u32).unwrap_or(i64::MAX);
        APPSERVICE_RETRY_BACKOFF_BASE_MS.saturating_mul(exponential).min(APPSERVICE_RETRY_BACKOFF_MAX_MS)
    }

    fn classify_http_failure(status: StatusCode) -> TransactionFailureKind {
        if status.is_server_error()
            || matches!(status, StatusCode::TOO_MANY_REQUESTS | StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_EARLY)
        {
            TransactionFailureKind::Retryable
        } else {
            TransactionFailureKind::Fatal
        }
    }

    fn should_disable_service(failure_kind: TransactionFailureKind, retry_count: i32) -> bool {
        match failure_kind {
            TransactionFailureKind::Fatal => retry_count >= APPSERVICE_FATAL_FAILURE_THRESHOLD,
            TransactionFailureKind::Retryable => retry_count >= APPSERVICE_RETRYABLE_FAILURE_THRESHOLD,
        }
    }
}

impl TransactionFailureKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Retryable => "retryable",
            Self::Fatal => "fatal",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct NamespacesInfo {
    pub users: Vec<ApplicationServiceNamespace>,
    pub aliases: Vec<ApplicationServiceNamespace>,
    pub rooms: Vec<ApplicationServiceNamespace>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manager() -> ApplicationServiceManager {
        let pool =
            Arc::new(sqlx::postgres::PgPoolOptions::new().connect_lazy_with(sqlx::postgres::PgConnectOptions::new()));

        ApplicationServiceManager::new(
            Arc::new(ApplicationServiceStorage::new(&pool)),
            Arc::new(EventStorage::new(&pool, "example.com".to_string())),
            "example.com".to_string(),
        )
    }

    #[test]
    fn test_namespaces_info_serialization() {
        let info = NamespacesInfo { users: vec![], aliases: vec![], rooms: vec![] };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("users"));
        assert!(json.contains("aliases"));
        assert!(json.contains("rooms"));
    }

    #[test]
    fn test_namespaces_info_with_data() {
        let namespace = synapse_storage::application_service::ApplicationServiceNamespace {
            id: 1,
            as_id: "test-as".to_string(),
            namespace_pattern: "@_.*:example.com".to_string(),
            is_exclusive: true,
            regex: "@_.*:example.com".to_string(),
            created_ts: 1234567890,
        };
        let info = NamespacesInfo { users: vec![namespace.clone()], aliases: vec![], rooms: vec![namespace] };
        assert_eq!(info.users.len(), 1);
        assert_eq!(info.rooms.len(), 1);
        assert!(info.aliases.is_empty());
    }

    #[test]
    fn test_update_request_builder() {
        let request = synapse_storage::application_service::UpdateApplicationServiceRequest::new()
            .url("http://new-url:8080")
            .description("New Description")
            .is_rate_limited(true)
            .is_enabled(true);

        assert_eq!(request.url, Some("http://new-url:8080".to_string()));
        assert_eq!(request.description, Some("New Description".to_string()));
        assert_eq!(request.is_rate_limited, Some(true));
        assert_eq!(request.is_enabled, Some(true));
    }

    #[test]
    fn test_update_request_partial() {
        let request = synapse_storage::application_service::UpdateApplicationServiceRequest::new()
            .description("Only Description Update");

        assert!(request.url.is_none());
        assert_eq!(request.description, Some("Only Description Update".to_string()));
        assert!(request.is_rate_limited.is_none());
        assert!(request.is_enabled.is_none());
    }

    #[test]
    fn test_update_request_protocols() {
        let request = synapse_storage::application_service::UpdateApplicationServiceRequest::new()
            .protocols(vec!["irc".to_string(), "matrix".to_string()]);

        assert_eq!(request.protocols.as_ref().unwrap().len(), 2);
        assert!(request.protocols.unwrap().contains(&"irc".to_string()));
    }

    #[test]
    fn test_namespace_rule_creation() {
        let rule = synapse_storage::application_service::NamespaceRule {
            is_exclusive: true,
            regex: "@_irc_.*:example\\.com".to_string(),
            group_id: Some("group:example.com".to_string()),
        };
        assert!(rule.is_exclusive);
        assert_eq!(rule.regex, "@_irc_.*:example\\.com");
        assert_eq!(rule.group_id, Some("group:example.com".to_string()));
    }

    #[test]
    fn test_namespace_rule_without_group() {
        let rule = synapse_storage::application_service::NamespaceRule {
            is_exclusive: false,
            regex: "#_.*:example\\.com".to_string(),
            group_id: None,
        };
        assert!(!rule.is_exclusive);
        assert!(rule.group_id.is_none());
    }

    #[test]
    fn test_namespaces_structure() {
        let namespaces = synapse_storage::application_service::Namespaces {
            users: vec![synapse_storage::application_service::NamespaceRule {
                is_exclusive: true,
                regex: "@_.*:example.com".to_string(),
                group_id: None,
            }],
            aliases: vec![],
            rooms: vec![],
        };
        assert_eq!(namespaces.users.len(), 1);
        assert!(namespaces.aliases.is_empty());
        assert!(namespaces.rooms.is_empty());
    }

    #[test]
    fn test_register_request_minimal() {
        let request = synapse_storage::application_service::RegisterApplicationServiceRequest {
            as_id: "minimal-as".to_string(),
            url: "http://localhost:8080".to_string(),
            as_token: "token".to_string(),
            hs_token: "hs_token".to_string(),
            sender: "@bot:example.com".to_string(),
            description: None,
            is_rate_limited: None,
            protocols: None,
            namespaces: None,
            api_key: None,
            config: None,
        };
        assert_eq!(request.as_id, "minimal-as");
        assert!(request.description.is_none());
        assert!(request.protocols.is_none());
    }

    #[test]
    fn test_register_request_full() {
        let request = synapse_storage::application_service::RegisterApplicationServiceRequest {
            as_id: "full-as".to_string(),
            url: "http://localhost:9999".to_string(),
            as_token: "as_token".to_string(),
            hs_token: "hs_token".to_string(),
            sender: "@bridge:example.com".to_string(),
            description: Some("A full bridge service".to_string()),
            is_rate_limited: Some(true),
            protocols: Some(vec!["irc".to_string()]),
            namespaces: Some(serde_json::json!({
                "users": [{"exclusive": true, "regex": "@_.*:example.com"}],
                "aliases": [],
                "rooms": []
            })),
            api_key: None,
            config: None,
        };
        assert_eq!(request.description, Some("A full bridge service".to_string()));
        assert_eq!(request.is_rate_limited, Some(true));
        assert!(request.namespaces.is_some());
    }

    #[test]
    fn test_parse_config_file_contents_normalizes_sender_localpart() {
        let manager = test_manager();
        let raw_config = r#"
id: irc-bridge
url: http://localhost:9999
as_token: appservice-token
hs_token: homeserver-token
sender_localpart: ircbot
rate_limited: false
protocols:
  - irc
namespaces:
  users:
    - exclusive: true
      regex: '@_irc_.*:example\.com'
  aliases: []
  rooms: []
receive_ephemeral: true
"#;

        let result = manager.parse_config_file_contents(raw_config, "inline");
        assert!(result.is_ok());
        let request = if let Ok(request) = result { request } else { return };

        assert_eq!(request.as_id, "irc-bridge");
        assert_eq!(request.sender, "@ircbot:example.com");
        assert_eq!(request.is_rate_limited, Some(false));
        assert_eq!(request.protocols, Some(vec!["irc".to_string()]));
        assert_eq!(request.config.unwrap()["receive_ephemeral"], serde_json::json!(true));
    }

    #[test]
    fn test_parse_config_file_contents_rejects_invalid_namespace_regex() {
        let manager = test_manager();
        let raw_config = r#"
id: bad-bridge
url: http://localhost:9999
as_token: appservice-token
hs_token: homeserver-token
sender: '@bridge:example.com'
namespaces:
  users:
    - exclusive: true
      regex: '['
  aliases: []
  rooms: []
"#;

        let result = manager.parse_config_file_contents(raw_config, "inline");
        assert!(result.is_err());
        let error_text = if let Err(error) = result { error.to_string() } else { String::new() };
        assert!(error_text.contains("invalid users namespace regex"));
    }

    #[test]
    fn test_service_matches_event_for_user_and_room_namespaces() {
        let manager = test_manager();
        let service = ApplicationService {
            id: 1,
            as_id: "bridge".to_string(),
            url: "http://localhost:9999".to_string(),
            as_token: "as-token".to_string(),
            hs_token: "hs-token".to_string(),
            sender_localpart: "@bridge:example.com".to_string(),
            is_enabled: true,
            is_rate_limited: false,
            protocols: vec![],
            namespaces: serde_json::json!({
                "users": [{"exclusive": true, "regex": "@_bridge_.*:example\\.com"}],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": "!bridge-.*:example\\.com"}]
            }),
            created_ts: 1,
            updated_ts: None,
            description: None,
            api_key: None,
            config: serde_json::json!({}),
        };

        assert!(manager.service_matches_event(&service, "!bridge-room:example.com", "@alice:example.com", None,));
        assert!(manager.service_matches_event(&service, "!random:example.com", "@_bridge_alice:example.com", None,));
        assert!(manager.service_matches_event(
            &service,
            "!random:example.com",
            "@alice:example.com",
            Some("@_bridge_bot:example.com"),
        ));
        assert!(!manager.service_matches_event(&service, "!random:example.com", "@alice:example.com", None,));
    }

    #[test]
    fn test_source_event_id_strips_appservice_suffix() {
        assert_eq!(
            ApplicationServiceManager::source_event_id("$abc123:example.com::bridge"),
            "$abc123:example.com".to_string()
        );
        assert_eq!(ApplicationServiceManager::source_event_id("$plain:example.com"), "$plain:example.com".to_string());
    }

    #[test]
    fn test_retry_backoff_ms_grows_and_caps() {
        assert_eq!(ApplicationServiceManager::retry_backoff_ms(0), 0);
        assert_eq!(ApplicationServiceManager::retry_backoff_ms(1), 5_000);
        assert_eq!(ApplicationServiceManager::retry_backoff_ms(2), 10_000);
        assert_eq!(ApplicationServiceManager::retry_backoff_ms(3), 20_000);
        assert_eq!(ApplicationServiceManager::retry_backoff_ms(10), 300_000);
    }

    #[test]
    fn test_is_transaction_ready_to_retry_respects_backoff_window() {
        let transaction = ApplicationServiceTransaction {
            id: 1,
            as_id: "bridge".to_string(),
            transaction_id: "txn".to_string(),
            events: serde_json::json!([]),
            sent_ts: 1_000,
            completed_ts: None,
            retry_count: 2,
            last_error: Some("boom".to_string()),
        };

        assert!(!ApplicationServiceManager::is_transaction_ready_to_retry(&transaction, 10_999));
        assert!(ApplicationServiceManager::is_transaction_ready_to_retry(&transaction, 11_000));
    }

    #[test]
    fn test_classify_http_failure_distinguishes_retryable_and_fatal_statuses() {
        assert_eq!(
            ApplicationServiceManager::classify_http_failure(StatusCode::BAD_GATEWAY),
            TransactionFailureKind::Retryable
        );
        assert_eq!(
            ApplicationServiceManager::classify_http_failure(StatusCode::TOO_MANY_REQUESTS),
            TransactionFailureKind::Retryable
        );
        assert_eq!(
            ApplicationServiceManager::classify_http_failure(StatusCode::UNAUTHORIZED),
            TransactionFailureKind::Fatal
        );
        assert_eq!(
            ApplicationServiceManager::classify_http_failure(StatusCode::NOT_FOUND),
            TransactionFailureKind::Fatal
        );
    }

    #[test]
    fn test_should_disable_service_uses_kind_specific_thresholds() {
        assert!(!ApplicationServiceManager::should_disable_service(TransactionFailureKind::Fatal, 2));
        assert!(ApplicationServiceManager::should_disable_service(TransactionFailureKind::Fatal, 3));
        assert!(!ApplicationServiceManager::should_disable_service(TransactionFailureKind::Retryable, 7));
        assert!(ApplicationServiceManager::should_disable_service(TransactionFailureKind::Retryable, 8));
    }
}
