use crate::federation::client::{FederationClient, FederationTransaction};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationEvent {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: serde_json::Value,
    pub origin: String,
    pub destination: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EventBroadcaster {
    server_name: String,
    federation_client: Option<Arc<FederationClient>>,
}

impl EventBroadcaster {
    pub fn new(server_name: String) -> Self {
        Self {
            server_name,
            federation_client: None,
        }
    }

    pub fn with_client(mut self, client: Arc<FederationClient>) -> Self {
        self.federation_client = Some(client);
        self
    }

    pub fn set_client(&mut self, client: Arc<FederationClient>) {
        self.federation_client = Some(client);
    }

    pub async fn broadcast_event(
        &self,
        room_id: &str,
        event: &serde_json::Value,
        origin: &str,
    ) -> Result<(), FederationBroadcastError> {
        let event_id = event
            .get("event_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        tracing::info!(
            "Broadcasting event {} in room {} to federation servers",
            event_id,
            room_id
        );

        let destinations = self.get_eligible_destinations(room_id).await;

        if destinations.is_empty() {
            tracing::debug!(
                "No eligible destinations for event broadcast in room {}",
                room_id
            );
            return Ok(());
        }

        let client = match &self.federation_client {
            Some(c) => c,
            None => {
                tracing::warn!(
                    "FederationClient not configured, skipping broadcast of event {}",
                    event_id
                );
                return Ok(());
            }
        };

        let txn_id = format!(
            "txn_{}_{}",
            chrono::Utc::now().timestamp_millis(),
            uuid::Uuid::new_v4()
        );

        for destination in &destinations {
            if destination == &self.server_name {
                continue;
            }

            let transaction = FederationTransaction {
                transaction_id: txn_id.clone(),
                origin: origin.to_string(),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
                destination: destination.clone(),
                pdus: vec![event.clone()],
                edus: vec![],
            };

            match client.send_transaction(destination, &transaction).await {
                Ok(_) => {
                    tracing::info!("Successfully sent event {} to {}", event_id, destination);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to send event {} to {}: {}",
                        event_id,
                        destination,
                        e
                    );
                }
            }
        }

        Ok(())
    }

    pub async fn broadcast_edu(
        &self,
        destination: &str,
        edu: &serde_json::Value,
        origin: &str,
    ) -> Result<(), FederationBroadcastError> {
        if destination == self.server_name {
            return Ok(());
        }

        let client = match &self.federation_client {
            Some(c) => c,
            None => {
                tracing::warn!("FederationClient not configured, skipping EDU broadcast");
                return Ok(());
            }
        };

        let txn_id = format!(
            "edu_{}_{}",
            chrono::Utc::now().timestamp_millis(),
            uuid::Uuid::new_v4()
        );

        let transaction = FederationTransaction {
            transaction_id: txn_id,
            origin: origin.to_string(),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            destination: destination.to_string(),
            pdus: vec![],
            edus: vec![edu.clone()],
        };

        client
            .send_transaction(destination, &transaction)
            .await
            .map_err(|e| FederationBroadcastError::SendFailed(e.to_string()))?;

        Ok(())
    }

    async fn get_eligible_destinations(&self, _room_id: &str) -> Vec<String> {
        let client = match &self.federation_client {
            Some(c) => c,
            None => return Vec::new(),
        };

        let _ = client;
        Vec::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FederationBroadcastError {
    #[error("Failed to send event: {0}")]
    SendFailed(String),
    #[error("Invalid event data: {0}")]
    InvalidEvent(String),
    #[error("Network error: {0}")]
    NetworkError(String),
}
