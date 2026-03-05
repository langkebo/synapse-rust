use crate::common::ApiError;
use crate::storage::application_service::*;
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, instrument, warn};

pub struct ApplicationServiceManager {
    storage: Arc<ApplicationServiceStorage>,
    http_client: Client,
    server_name: String,
}

impl ApplicationServiceManager {
    pub fn new(storage: Arc<ApplicationServiceStorage>, server_name: String) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| {
                tracing::warn!("Failed to build HTTP client with custom config, using default");
                Client::new()
            });

        Self {
            storage,
            http_client,
            server_name,
        }
    }

    #[instrument(skip(self, request))]
    pub async fn register(
        &self,
        request: RegisterApplicationServiceRequest,
    ) -> Result<ApplicationService, ApiError> {
        info!("Registering application service: as_id={}", request.as_id);

        if let Some(existing) =
            self.storage.get_by_id(&request.as_id).await.map_err(|e| {
                ApiError::internal(format!("Failed to check existing service: {}", e))
            })?
        {
            return Err(ApiError::bad_request(format!(
                "Application service '{}' already exists",
                existing.as_id
            )));
        }

        let service = self.storage.register(request).await.map_err(|e| {
            ApiError::internal(format!("Failed to register application service: {}", e))
        })?;

        info!(
            "Application service registered successfully: as_id={}",
            service.as_id
        );
        Ok(service)
    }

    #[instrument(skip(self))]
    pub async fn get(&self, as_id: &str) -> Result<Option<ApplicationService>, ApiError> {
        self.storage
            .get_by_id(as_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get application service: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_by_token(
        &self,
        as_token: &str,
    ) -> Result<Option<ApplicationService>, ApiError> {
        let service = self.storage.get_by_token(as_token).await.map_err(|e| {
            ApiError::internal(format!("Failed to get application service by token: {}", e))
        })?;

        if let Some(ref svc) = service {
            let _ = self
                .storage
                .update_last_seen(&svc.as_id)
                .await
                .map_err(|e| warn!("Failed to update last seen: {}", e));
        }

        Ok(service)
    }

    #[instrument(skip(self))]
    pub async fn get_all_active(&self) -> Result<Vec<ApplicationService>, ApiError> {
        self.storage
            .get_all_active()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get active services: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn update(
        &self,
        as_id: &str,
        request: UpdateApplicationServiceRequest,
    ) -> Result<ApplicationService, ApiError> {
        info!("Updating application service: as_id={}", as_id);

        let service = self.storage.update(as_id, &request).await.map_err(|e| {
            ApiError::internal(format!("Failed to update application service: {}", e))
        })?;

        info!("Application service updated successfully: as_id={}", as_id);
        Ok(service)
    }

    #[instrument(skip(self))]
    pub async fn unregister(&self, as_id: &str) -> Result<(), ApiError> {
        info!("Unregistering application service: as_id={}", as_id);

        self.storage.unregister(as_id).await.map_err(|e| {
            ApiError::internal(format!("Failed to unregister application service: {}", e))
        })?;

        info!(
            "Application service unregistered successfully: as_id={}",
            as_id
        );
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn validate_token(&self, as_token: &str) -> Result<ApplicationService, ApiError> {
        self.get_by_token(as_token)
            .await?
            .ok_or_else(|| ApiError::unauthorized("Invalid application service token"))
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
            .map_err(|e| ApiError::internal(format!("Failed to set state: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_state(
        &self,
        as_id: &str,
        state_key: &str,
    ) -> Result<Option<ApplicationServiceState>, ApiError> {
        self.storage
            .get_state(as_id, state_key)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_all_states(
        &self,
        as_id: &str,
    ) -> Result<Vec<ApplicationServiceState>, ApiError> {
        self.storage
            .get_all_states(as_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get states: {}", e)))
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
            .add_event(
                &event_id, as_id, room_id, event_type, sender, content, state_key,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to add event: {}", e)))?;

        info!(
            "Event pushed to application service: as_id={}, event_id={}",
            as_id, event_id
        );
        Ok(event)
    }

    #[instrument(skip(self))]
    pub async fn get_pending_events(
        &self,
        as_id: &str,
        limit: i64,
    ) -> Result<Vec<ApplicationServiceEvent>, ApiError> {
        self.storage
            .get_pending_events(as_id, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get pending events: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn send_transaction(
        &self,
        as_id: &str,
        events: Vec<serde_json::Value>,
    ) -> Result<(), ApiError> {
        let service = self
            .storage
            .get_by_id(as_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get service: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Application service not found"))?;

        let transaction_id = format!("{}", uuid::Uuid::new_v4());

        let _transaction = self
            .storage
            .create_transaction(as_id, &transaction_id, &events)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create transaction: {}", e)))?;

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
                let _ = self
                    .storage
                    .complete_transaction(as_id, &transaction_id)
                    .await
                    .map_err(|e| error!("Failed to complete transaction: {}", e));

                for event in &events {
                    if let Some(event_id) = event.get("event_id").and_then(|e| e.as_str()) {
                        let _ = self
                            .storage
                            .mark_event_processed(event_id, &transaction_id)
                            .await
                            .map_err(|e| warn!("Failed to mark event processed: {}", e));
                    }
                }

                info!(
                    "Transaction sent successfully: as_id={}, txn_id={}",
                    as_id, transaction_id
                );
                Ok(())
            }
            Ok(resp) => {
                let status = resp.status();
                let error_body = resp.text().await.unwrap_or_default();

                let _ = self
                    .storage
                    .fail_transaction(
                        as_id,
                        &transaction_id,
                        &format!("HTTP {}: {}", status, error_body),
                    )
                    .await
                    .map_err(|e| error!("Failed to fail transaction: {}", e));

                Err(ApiError::internal(format!(
                    "Application service returned error: HTTP {}",
                    status
                )))
            }
            Err(e) => {
                let _ = self
                    .storage
                    .fail_transaction(as_id, &transaction_id, &e.to_string())
                    .await
                    .map_err(|e| error!("Failed to fail transaction: {}", e));

                Err(ApiError::internal(format!(
                    "Failed to send transaction: {}",
                    e
                )))
            }
        }
    }

    #[instrument(skip(self))]
    pub async fn query_user(&self, user_id: &str) -> Result<Option<String>, ApiError> {
        self.storage
            .is_user_in_namespace(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to query user namespace: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn query_room_alias(&self, alias: &str) -> Result<Option<String>, ApiError> {
        self.storage
            .is_room_alias_in_namespace(alias)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to query room alias namespace: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn query_room_id(&self, room_id: &str) -> Result<Option<String>, ApiError> {
        self.storage
            .is_room_id_in_namespace(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to query room namespace: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn register_virtual_user(
        &self,
        as_id: &str,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<ApplicationServiceUser, ApiError> {
        info!(
            "Registering virtual user: as_id={}, user_id={}",
            as_id, user_id
        );

        let user = self
            .storage
            .register_virtual_user(as_id, user_id, displayname, avatar_url)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to register virtual user: {}", e)))?;

        info!("Virtual user registered successfully: user_id={}", user_id);
        Ok(user)
    }

    #[instrument(skip(self))]
    pub async fn get_virtual_users(
        &self,
        as_id: &str,
    ) -> Result<Vec<ApplicationServiceUser>, ApiError> {
        self.storage
            .get_virtual_users(as_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get virtual users: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_namespaces(&self, as_id: &str) -> Result<NamespacesInfo, ApiError> {
        let users = self
            .storage
            .get_user_namespaces(as_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get user namespaces: {}", e)))?;
        let aliases = self
            .storage
            .get_room_alias_namespaces(as_id)
            .await
            .map_err(|e| {
                ApiError::internal(format!("Failed to get room alias namespaces: {}", e))
            })?;
        let rooms = self
            .storage
            .get_room_namespaces(as_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room namespaces: {}", e)))?;

        Ok(NamespacesInfo {
            users,
            aliases,
            rooms,
        })
    }

    #[instrument(skip(self))]
    pub async fn get_statistics(&self) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage
            .get_statistics()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get statistics: {}", e)))
    }

    pub async fn ping(&self, as_id: &str) -> Result<bool, ApiError> {
        let service = self
            .storage
            .get_by_id(as_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get service: {}", e)))?
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
                    .map_err(|e| warn!("Failed to update last seen: {}", e));
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct NamespacesInfo {
    pub users: Vec<ApplicationServiceNamespace>,
    pub aliases: Vec<ApplicationServiceNamespace>,
    pub rooms: Vec<ApplicationServiceNamespace>,
}

use serde::Serialize;
