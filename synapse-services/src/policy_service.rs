//! MSC4284: Policy Server service
//!
//! Allows configuring an external policy server to moderate rooms, users,
//! and content. When enabled, the homeserver consults the policy server
//! before allowing room creation, joins, and invites.
//!
//! This is a basic framework implementation. When `enabled = false`, all
//! policy checks return `PolicyResult::Allow`. When `enabled = true`,
//! the service sends HTTP requests to the configured policy server endpoint.

use serde::{Deserialize, Serialize};
use synapse_common::config::PolicyServerConfig;
use tracing;

/// Result of a policy check.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyResult {
    /// The operation is allowed.
    Allow,
    /// The operation is denied. Contains a human-readable reason.
    Deny(String),
}

/// Request body sent to the policy server.
#[derive(Debug, Serialize)]
struct PolicyRequest {
    /// The type of entity being checked: "room", "user", or "content".
    entity_type: String,
    /// The identifier of the entity being checked.
    entity_id: String,
    /// The user performing the operation.
    actor: String,
    /// The action being performed: "create", "join", "invite", "send".
    action: String,
}

/// Response body from the policy server.
#[derive(Debug, Deserialize)]
struct PolicyResponse {
    /// Whether the operation is allowed.
    allowed: bool,
    /// Optional reason for denial.
    reason: Option<String>,
}

pub struct PolicyService {
    config: PolicyServerConfig,
    client: reqwest::Client,
}

impl PolicyService {
    pub fn new(config: PolicyServerConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_default();

        Self { config, client }
    }

    /// Check whether an operation is allowed by the policy server.
    ///
    /// When the policy server is disabled, this always returns `PolicyResult::Allow`.
    /// When enabled, it sends an HTTP POST to the policy server endpoint.
    /// On network failure, the behavior depends on `fail_open` configuration.
    pub async fn check_policy(&self, entity_type: &str, entity_id: &str, actor: &str, action: &str) -> PolicyResult {
        if !self.config.is_configured() {
            return PolicyResult::Allow;
        }

        let endpoint = match &self.config.endpoint {
            Some(e) => e,
            None => return PolicyResult::Allow,
        };

        let request = PolicyRequest {
            entity_type: entity_type.to_string(),
            entity_id: entity_id.to_string(),
            actor: actor.to_string(),
            action: action.to_string(),
        };

        let mut http_request = self.client.post(format!("{endpoint}/v1/check"));

        if let Some(ref api_key) = self.config.api_key {
            http_request = http_request.header("Authorization", format!("Bearer {api_key}"));
        }

        match http_request.json(&request).send().await {
            Ok(response) => match response.json::<PolicyResponse>().await {
                Ok(policy_response) => {
                    if policy_response.allowed {
                        PolicyResult::Allow
                    } else {
                        PolicyResult::Deny(
                            policy_response.reason.unwrap_or_else(|| "Denied by policy server".to_string()),
                        )
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, fail_open = self.config.fail_open, "Failed to parse policy server response");
                    self.fail_open_or_deny("Failed to parse policy server response")
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, fail_open = self.config.fail_open, "Policy server request failed");
                self.fail_open_or_deny("Policy server unreachable")
            }
        }
    }

    fn fail_open_or_deny(&self, reason: &str) -> PolicyResult {
        if self.config.fail_open {
            tracing::info!("Policy server unreachable, failing open (allowing operation)");
            PolicyResult::Allow
        } else {
            PolicyResult::Deny(reason.to_string())
        }
    }

    /// Convenience: check policy for room creation.
    pub async fn check_room_create(&self, room_id: &str, creator: &str) -> PolicyResult {
        self.check_policy("room", room_id, creator, "create").await
    }

    /// Convenience: check policy for joining a room.
    pub async fn check_room_join(&self, room_id: &str, user_id: &str) -> PolicyResult {
        self.check_policy("room", room_id, user_id, "join").await
    }

    /// Convenience: check policy for inviting a user.
    pub async fn check_room_invite(&self, _room_id: &str, inviter: &str, invitee: &str) -> PolicyResult {
        // For invites, the entity_id is the invitee, and the action is "invite"
        // with the room_id as additional context
        self.check_policy("user", invitee, inviter, "invite").await
    }

    /// Convenience: check policy for sending content.
    pub async fn check_content_send(&self, room_id: &str, sender: &str) -> PolicyResult {
        self.check_policy("content", room_id, sender, "send").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_result_allow_serialization() {
        let result = PolicyResult::Allow;
        let json = serde_json::to_string(&result).unwrap();
        assert_eq!(json, "\"allow\"");
    }

    #[test]
    fn test_policy_result_deny_serialization() {
        let result = PolicyResult::Deny("spam".to_string());
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("deny"));
        assert!(json.contains("spam"));
    }

    #[test]
    fn test_policy_server_config_default_disabled() {
        let config = PolicyServerConfig::default();
        assert!(!config.enabled);
        assert!(!config.is_configured());
    }

    #[test]
    fn test_policy_server_config_configured() {
        let config = PolicyServerConfig {
            enabled: true,
            endpoint: Some("https://policy.example.com".to_string()),
            api_key: None,
            timeout_secs: 5,
            fail_open: true,
        };
        assert!(config.is_configured());
    }

    #[test]
    fn test_policy_server_config_enabled_but_no_endpoint() {
        let config =
            PolicyServerConfig { enabled: true, endpoint: None, api_key: None, timeout_secs: 5, fail_open: true };
        assert!(!config.is_configured());
    }

    #[tokio::test]
    async fn test_policy_service_disabled_returns_allow() {
        let config = PolicyServerConfig::default();
        let service = PolicyService::new(config);
        let result = service.check_policy("room", "!room:test.com", "@user:test.com", "join").await;
        assert_eq!(result, PolicyResult::Allow);
    }

    #[tokio::test]
    async fn test_policy_service_convenience_methods() {
        let config = PolicyServerConfig::default();
        let service = PolicyService::new(config);

        assert_eq!(service.check_room_create("!room:test.com", "@user:test.com").await, PolicyResult::Allow);
        assert_eq!(service.check_room_join("!room:test.com", "@user:test.com").await, PolicyResult::Allow);
        assert_eq!(
            service.check_room_invite("!room:test.com", "@a:test.com", "@b:test.com").await,
            PolicyResult::Allow
        );
        assert_eq!(service.check_content_send("!room:test.com", "@user:test.com").await, PolicyResult::Allow);
    }
}
