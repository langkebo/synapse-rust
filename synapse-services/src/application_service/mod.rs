use reqwest::Client;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use synapse_common::ApiError;
use synapse_storage::application_service::*;
use synapse_storage::event::EventStoreApi;
use tokio::fs;
use tracing::{info, instrument, warn};

pub mod scheduler;
pub use scheduler::ApplicationServiceScheduler;

mod models;
#[cfg(test)]
mod tests;
mod transaction;

pub use models::NamespacesInfo;

pub struct ApplicationServiceManager {
    storage: Arc<dyn ApplicationServiceStoreApi>,
    event_storage: Arc<dyn EventStoreApi>,
    http_client: Client,
    server_name: String,
}

impl ApplicationServiceManager {
    pub fn new(
        storage: Arc<dyn ApplicationServiceStoreApi>,
        event_storage: Arc<dyn EventStoreApi>,
        server_name: String,
    ) -> Self {
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
        self.validate_namespace_exclusivity(&request.as_id, request.namespaces.as_ref()).await?;
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

        self.validate_namespace_exclusivity(&request.as_id, request.namespaces.as_ref()).await?;

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
        let service = self
            .storage
            .get_by_id(as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get application service", &e))?
            .ok_or_else(|| ApiError::not_found("Application service not found"))?;

        if !self.service_matches_event(&service, room_id, sender, state_key) {
            return Err(ApiError::forbidden(
                "Explicitly pushed events must target a room, sender, or state_key owned by the application service",
            ));
        }

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
    pub async fn count_pending_events(&self, as_id: &str) -> Result<i64, ApiError> {
        self.storage
            .count_pending_events(as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to count pending events", &e))
    }

    #[instrument(skip(self))]
    pub async fn count_pending_transactions(&self, as_id: &str) -> Result<i64, ApiError> {
        self.storage
            .count_pending_transactions(as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to count pending transactions", &e))
    }

    pub async fn start_sender(self: Arc<Self>, batch_limit: i64, flush_interval_secs: u64) {
        let scheduler = Arc::new(ApplicationServiceScheduler::with_options(
            self,
            batch_limit.max(1) as usize,
            flush_interval_secs.max(1).saturating_mul(1_000),
        ));
        let _ = scheduler.start(tokio_util::sync::CancellationToken::new());
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

        let service = self
            .storage
            .get_by_id(as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get application service", &e))?
            .ok_or_else(|| ApiError::not_found("Application service not found"))?;

        if !Self::is_local_user_id(user_id, &self.server_name) {
            return Err(ApiError::bad_request(format!(
                "Virtual user '{}' must belong to the local server '{}'",
                user_id, self.server_name
            )));
        }

        if !Self::namespace_matches(&service.namespaces, "users", user_id, true) {
            return Err(ApiError::forbidden(
                "Virtual user must match an exclusive user namespace owned by the application service",
            ));
        }

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
        let statistics = self
            .storage
            .get_statistics()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get statistics", &e))?;
        let mut enriched = Vec::with_capacity(statistics.len());

        for mut entry in statistics {
            let as_id = entry
                .get("as_id")
                .and_then(|value| value.as_str())
                .ok_or_else(|| ApiError::internal("Application service statistics entry missing as_id"))?;
            let states = self
                .storage
                .get_all_states(as_id)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get scheduler states", &e))?;

            if let Some(object) = entry.as_object_mut() {
                object.insert("scheduler".to_string(), Self::scheduler_statistics_from_states(&states));
            }

            enriched.push(entry);
        }

        Ok(enriched)
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
}
