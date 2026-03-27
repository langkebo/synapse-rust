use serde::{Deserialize, Serialize};

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
    _server_name: String,
}

impl EventBroadcaster {
    pub fn new(server_name: String) -> Self {
        Self {
            _server_name: server_name,
        }
    }

    pub async fn broadcast_event(
        &self,
        room_id: &str,
        event: &serde_json::Value,
        _origin: &str,
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

        tracing::info!(
            "Would broadcast event {} to {} federation servers: {:?}",
            event_id,
            destinations.len(),
            destinations
        );

        Ok(())
    }

    async fn get_eligible_destinations(&self, _room_id: &str) -> Vec<String> {
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
