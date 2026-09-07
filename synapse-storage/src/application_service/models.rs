use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// The `ApplicationService` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationService {
    /// The `id` field.
    pub id: i64,
    /// The `as_id` field.
    pub as_id: String,
    /// The `url` field.
    pub url: String,
    #[serde(skip_serializing)]
    /// The `as_token` field.
    pub as_token: String,
    #[serde(skip_serializing)]
    /// The `hs_token` field.
    pub hs_token: String,
    #[serde(rename = "sender")]
    #[sqlx(rename = "sender_localpart")]
    /// The `sender_localpart` field.
    pub sender_localpart: String,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `is_rate_limited` field.
    pub is_rate_limited: bool,
    /// The `protocols` field.
    pub protocols: Vec<String>,
    /// The `namespaces` field.
    pub namespaces: serde_json::Value,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
    /// The `description` field.
    pub description: Option<String>,
    #[serde(skip_serializing)]
    /// The `api_key` field.
    pub api_key: Option<String>,
    /// The `config` field.
    pub config: serde_json::Value,
}

/// The `ApplicationServiceState` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationServiceState {
    /// The `as_id` field.
    pub as_id: String,
    /// The `state_key` field.
    pub state_key: String,
    /// The `state_value` field.
    pub state_value: String,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `ApplicationServiceEvent` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationServiceEvent {
    /// The `event_id` field.
    pub event_id: String,
    /// The `as_id` field.
    pub as_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `sender` field.
    pub sender: String,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `state_key` field.
    pub state_key: Option<String>,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
    /// The `processed_ts` field.
    pub processed_ts: Option<i64>,
    /// The `transaction_id` field.
    pub transaction_id: Option<String>,
}

/// The `ApplicationServiceTransaction` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationServiceTransaction {
    /// The `id` field.
    pub id: i64,
    /// The `as_id` field.
    pub as_id: String,
    /// The `txn_id` field.
    pub txn_id: String,
    /// The `transaction_id` field.
    pub transaction_id: Option<String>,
    /// The `events` field.
    pub events: serde_json::Value,
    /// The `sent_ts` field.
    pub sent_ts: Option<i64>,
    /// The `completed_ts` field.
    pub completed_ts: Option<i64>,
    /// The `retry_count` field.
    pub retry_count: i32,
    /// The `last_error` field.
    pub last_error: Option<String>,
}

/// The `ApplicationServiceNamespace` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationServiceNamespace {
    /// The `id` field.
    pub id: i64,
    /// The `as_id` field.
    pub as_id: String,
    /// The `namespace_pattern` field.
    pub namespace_pattern: String,
    /// The `is_exclusive` field.
    pub is_exclusive: bool,
    /// The `regex` field.
    pub regex: String,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `ApplicationServiceUser` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationServiceUser {
    /// The `as_id` field.
    pub as_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `displayname` field.
    pub displayname: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `RegisterApplicationServiceRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterApplicationServiceRequest {
    /// The `as_id` field.
    pub as_id: String,
    /// The `url` field.
    pub url: String,
    /// The `as_token` field.
    pub as_token: String,
    /// The `hs_token` field.
    pub hs_token: String,
    /// The `sender` field.
    pub sender: String,
    /// The `description` field.
    pub description: Option<String>,
    /// The `is_rate_limited` field.
    pub is_rate_limited: Option<bool>,
    /// The `protocols` field.
    pub protocols: Option<Vec<String>>,
    /// The `namespaces` field.
    pub namespaces: Option<serde_json::Value>,
    /// The `api_key` field.
    pub api_key: Option<String>,
    /// The `config` field.
    pub config: Option<serde_json::Value>,
}

/// The `UpdateApplicationServiceRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateApplicationServiceRequest {
    /// The `url` field.
    pub url: Option<String>,
    /// The `description` field.
    pub description: Option<String>,
    /// The `is_rate_limited` field.
    pub is_rate_limited: Option<bool>,
    /// The `protocols` field.
    pub protocols: Option<Vec<String>>,
    /// The `is_enabled` field.
    pub is_enabled: Option<bool>,
    /// The `api_key` field.
    pub api_key: Option<String>,
    /// The `config` field.
    pub config: Option<serde_json::Value>,
}

impl UpdateApplicationServiceRequest {
    /// See [`new`].
    /// See [`new`].
    pub fn new() -> Self {
        Self::default()
    }

    /// See [`url`].
    /// See [`url`].
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// See [`description`].
    /// See [`description`].
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// See [`is_rate_limited`].
    /// See [`is_rate_limited`].
    pub fn is_rate_limited(mut self, is_rate_limited: bool) -> Self {
        self.is_rate_limited = Some(is_rate_limited);
        self
    }

    /// See [`protocols`].
    /// See [`protocols`].
    pub fn protocols(mut self, protocols: Vec<String>) -> Self {
        self.protocols = Some(protocols);
        self
    }

    /// See [`is_enabled`].
    /// See [`is_enabled`].
    pub fn is_enabled(mut self, is_enabled: bool) -> Self {
        self.is_enabled = Some(is_enabled);
        self
    }

    /// See [`api_key`].
    /// See [`api_key`].
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// See [`config`].
    /// See [`config`].
    pub fn config(mut self, config: serde_json::Value) -> Self {
        self.config = Some(config);
        self
    }
}

/// The `Namespaces` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Namespaces {
    /// The `users` field.
    pub users: Vec<NamespaceRule>,
    /// The `aliases` field.
    pub aliases: Vec<NamespaceRule>,
    /// The `rooms` field.
    pub rooms: Vec<NamespaceRule>,
}

/// The `NamespaceRule` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceRule {
    /// The `is_exclusive` field.
    pub is_exclusive: bool,
    /// The `regex` field.
    pub regex: String,
    #[serde(default)]
    /// The `group_id` field.
    pub group_id: Option<String>,
}
