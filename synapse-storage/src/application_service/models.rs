use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationService {
    pub id: i64,
    pub as_id: String,
    pub url: String,
    #[serde(skip_serializing)]
    pub as_token: String,
    #[serde(skip_serializing)]
    pub hs_token: String,
    #[serde(rename = "sender")]
    #[sqlx(rename = "sender_localpart")]
    pub sender_localpart: String,
    pub is_enabled: bool,
    pub is_rate_limited: bool,
    pub protocols: Vec<String>,
    pub namespaces: serde_json::Value,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub description: Option<String>,
    #[serde(skip_serializing)]
    pub api_key: Option<String>,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationServiceState {
    pub as_id: String,
    pub state_key: String,
    pub state_value: String,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationServiceEvent {
    pub event_id: String,
    pub as_id: String,
    pub room_id: String,
    pub event_type: String,
    pub sender: String,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
    pub origin_server_ts: i64,
    pub processed_ts: Option<i64>,
    pub transaction_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationServiceTransaction {
    pub id: i64,
    pub as_id: String,
    pub txn_id: String,
    pub transaction_id: Option<String>,
    pub events: serde_json::Value,
    pub sent_ts: Option<i64>,
    pub completed_ts: Option<i64>,
    pub retry_count: i32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationServiceNamespace {
    pub id: i64,
    pub as_id: String,
    pub namespace_pattern: String,
    pub is_exclusive: bool,
    pub regex: String,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationServiceUser {
    pub as_id: String,
    pub user_id: String,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterApplicationServiceRequest {
    pub as_id: String,
    pub url: String,
    pub as_token: String,
    pub hs_token: String,
    pub sender: String,
    pub description: Option<String>,
    pub is_rate_limited: Option<bool>,
    pub protocols: Option<Vec<String>>,
    pub namespaces: Option<serde_json::Value>,
    pub api_key: Option<String>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateApplicationServiceRequest {
    pub url: Option<String>,
    pub description: Option<String>,
    pub is_rate_limited: Option<bool>,
    pub protocols: Option<Vec<String>>,
    pub is_enabled: Option<bool>,
    pub api_key: Option<String>,
    pub config: Option<serde_json::Value>,
}

impl UpdateApplicationServiceRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn is_rate_limited(mut self, is_rate_limited: bool) -> Self {
        self.is_rate_limited = Some(is_rate_limited);
        self
    }

    pub fn protocols(mut self, protocols: Vec<String>) -> Self {
        self.protocols = Some(protocols);
        self
    }

    pub fn is_enabled(mut self, is_enabled: bool) -> Self {
        self.is_enabled = Some(is_enabled);
        self
    }

    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn config(mut self, config: serde_json::Value) -> Self {
        self.config = Some(config);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Namespaces {
    pub users: Vec<NamespaceRule>,
    pub aliases: Vec<NamespaceRule>,
    pub rooms: Vec<NamespaceRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceRule {
    pub is_exclusive: bool,
    pub regex: String,
    #[serde(default)]
    pub group_id: Option<String>,
}
