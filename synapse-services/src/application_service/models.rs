use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use synapse_common::ApiError;
use synapse_storage::application_service::*;
use url::Url;

use crate::application_service::ApplicationServiceManager;

#[derive(Debug, Deserialize)]
pub(super) struct AppServiceConfigFile {
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
pub(super) struct AppServiceConfigNamespaces {
    #[serde(default)]
    users: Vec<AppServiceConfigNamespaceRule>,
    #[serde(default)]
    aliases: Vec<AppServiceConfigNamespaceRule>,
    #[serde(default)]
    rooms: Vec<AppServiceConfigNamespaceRule>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AppServiceConfigNamespaceRule {
    #[serde(rename = "exclusive")]
    exclusive: bool,
    regex: String,
    #[serde(default)]
    group_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NamespacesInfo {
    pub users: Vec<ApplicationServiceNamespace>,
    pub aliases: Vec<ApplicationServiceNamespace>,
    pub rooms: Vec<ApplicationServiceNamespace>,
}

impl ApplicationServiceManager {
    pub(super) fn parse_config_file_contents(
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

    pub(super) fn service_matches_event(
        &self,
        service: &ApplicationService,
        room_id: &str,
        sender: &str,
        state_key: Option<&str>,
    ) -> bool {
        Self::namespace_matches(&service.namespaces, "rooms", room_id, false)
            || Self::namespace_matches(&service.namespaces, "users", sender, false)
            || state_key.is_some_and(|key| Self::namespace_matches(&service.namespaces, "users", key, false))
    }

    pub(super) fn namespace_matches(
        namespaces: &serde_json::Value,
        namespace_kind: &str,
        candidate: &str,
        exclusive_only: bool,
    ) -> bool {
        namespaces
            .get(namespace_kind)
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter(|rule| !exclusive_only || rule.get("exclusive").and_then(|value| value.as_bool()) == Some(true))
            .filter_map(|rule| rule.get("regex").and_then(|value| value.as_str()))
            .any(|pattern| Regex::new(pattern).is_ok_and(|regex| regex.is_match(candidate)))
    }

    pub(super) async fn validate_namespace_exclusivity(
        &self,
        as_id: &str,
        namespaces: Option<&serde_json::Value>,
    ) -> Result<(), ApiError> {
        for namespace_pattern in Self::exclusive_namespace_patterns(namespaces, "users") {
            if let Some(conflicting_as_id) =
                self.storage.find_user_namespace_conflict(as_id, &namespace_pattern).await.map_err(|e| {
                    ApiError::internal_with_log("Failed to validate appservice user namespace ownership", &e)
                })?
            {
                return Err(ApiError::conflict(format!(
                    "Exclusive user namespace '{}' is already owned by application service '{}'",
                    namespace_pattern, conflicting_as_id
                )));
            }
        }

        for namespace_pattern in Self::exclusive_namespace_patterns(namespaces, "aliases") {
            if let Some(conflicting_as_id) =
                self.storage.find_room_alias_namespace_conflict(as_id, &namespace_pattern).await.map_err(|e| {
                    ApiError::internal_with_log("Failed to validate appservice room alias namespace ownership", &e)
                })?
            {
                return Err(ApiError::conflict(format!(
                    "Exclusive room alias namespace '{}' is already owned by application service '{}'",
                    namespace_pattern, conflicting_as_id
                )));
            }
        }

        for namespace_pattern in Self::exclusive_namespace_patterns(namespaces, "rooms") {
            if let Some(conflicting_as_id) =
                self.storage.find_room_namespace_conflict(as_id, &namespace_pattern).await.map_err(|e| {
                    ApiError::internal_with_log("Failed to validate appservice room namespace ownership", &e)
                })?
            {
                return Err(ApiError::conflict(format!(
                    "Exclusive room namespace '{}' is already owned by application service '{}'",
                    namespace_pattern, conflicting_as_id
                )));
            }
        }

        Ok(())
    }

    pub(super) fn exclusive_namespace_patterns(
        namespaces: Option<&serde_json::Value>,
        namespace_kind: &str,
    ) -> Vec<String> {
        namespaces
            .and_then(|value| value.get(namespace_kind))
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter(|rule| rule.get("exclusive").and_then(|value| value.as_bool()) == Some(true))
            .filter_map(|rule| rule.get("regex").and_then(|value| value.as_str()))
            .map(str::trim)
            .filter(|pattern| !pattern.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }

    pub(super) fn is_local_user_id(user_id: &str, server_name: &str) -> bool {
        user_id
            .strip_prefix('@')
            .and_then(|stripped| stripped.split_once(':'))
            .is_some_and(|(localpart, user_server_name)| !localpart.is_empty() && user_server_name == server_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── is_local_user_id ──────────────────────────────────────────────

    #[test]
    fn local_user_id_matches_server_name() {
        assert!(ApplicationServiceManager::is_local_user_id("@alice:myserver.com", "myserver.com"));
    }

    #[test]
    fn remote_user_id_different_server() {
        assert!(!ApplicationServiceManager::is_local_user_id("@alice:other.com", "myserver.com"));
    }

    #[test]
    fn user_id_without_at_prefix_is_not_local() {
        assert!(!ApplicationServiceManager::is_local_user_id("alice:myserver.com", "myserver.com"));
    }

    #[test]
    fn user_id_without_server_part_is_not_local() {
        assert!(!ApplicationServiceManager::is_local_user_id("@alice", "myserver.com"));
    }

    #[test]
    fn empty_localpart_is_not_local() {
        assert!(!ApplicationServiceManager::is_local_user_id("@:myserver.com", "myserver.com"));
    }

    #[test]
    fn empty_user_id_is_not_local() {
        assert!(!ApplicationServiceManager::is_local_user_id("", "myserver.com"));
    }

    // ── namespace_matches ─────────────────────────────────────────────

    fn make_namespaces() -> serde_json::Value {
        json!({
            "users": [
                {"exclusive": true, "regex": "@.*:myserver\\.com", "group_id": null},
                {"exclusive": false, "regex": "@_webhook_.*", "group_id": "webhooks"},
            ],
            "rooms": [
                {"exclusive": true, "regex": "!bridge_.*:myserver\\.com"},
            ],
            "aliases": [
                {"exclusive": false, "regex": "#bridge_.*:myserver\\.com"},
            ]
        })
    }

    #[test]
    fn namespace_matches_user_by_regex() {
        let ns = make_namespaces();
        assert!(ApplicationServiceManager::namespace_matches(&ns, "users", "@alice:myserver.com", false));
    }

    #[test]
    fn namespace_matches_room_by_regex() {
        let ns = make_namespaces();
        assert!(ApplicationServiceManager::namespace_matches(&ns, "rooms", "!bridge_test:myserver.com", false));
    }

    #[test]
    fn namespace_matches_alias_by_regex() {
        let ns = make_namespaces();
        assert!(ApplicationServiceManager::namespace_matches(&ns, "aliases", "#bridge_test:myserver.com", false));
    }

    #[test]
    fn namespace_does_not_match_unrelated_candidate() {
        let ns = make_namespaces();
        assert!(!ApplicationServiceManager::namespace_matches(&ns, "users", "@bob:other.com", false));
    }

    #[test]
    fn namespace_matches_exclusive_only_filters_non_exclusive() {
        let ns = make_namespaces();
        // @_webhook_* is non-exclusive, so exclusive_only=true should not match it
        assert!(!ApplicationServiceManager::namespace_matches(&ns, "users", "@_webhook_test", true));
        // But @.*:myserver\.com IS exclusive, so it should match
        assert!(ApplicationServiceManager::namespace_matches(&ns, "users", "@alice:myserver.com", true));
    }

    #[test]
    fn namespace_matches_missing_kind_returns_false() {
        let ns = make_namespaces();
        assert!(!ApplicationServiceManager::namespace_matches(&ns, "nonexistent", "@alice:myserver.com", false));
    }

    #[test]
    fn namespace_matches_empty_namespaces_returns_false() {
        let empty = json!({});
        assert!(!ApplicationServiceManager::namespace_matches(&empty, "users", "@alice:myserver.com", false));
    }

    #[test]
    fn namespace_matches_invalid_regex_returns_false() {
        let ns = json!({
            "users": [
                {"exclusive": false, "regex": "[unclosed_group"},
            ]
        });
        assert!(!ApplicationServiceManager::namespace_matches(&ns, "users", "test", false));
    }

    // ── exclusive_namespace_patterns ──────────────────────────────────

    #[test]
    fn exclusive_patterns_returns_only_exclusive_regexes() {
        let ns = make_namespaces();
        let patterns = ApplicationServiceManager::exclusive_namespace_patterns(Some(&ns), "users");
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0], "@.*:myserver\\.com");
    }

    #[test]
    fn exclusive_patterns_filters_non_exclusive() {
        let ns = make_namespaces();
        let patterns = ApplicationServiceManager::exclusive_namespace_patterns(Some(&ns), "aliases");
        // The aliases rule is non-exclusive, so nothing returned
        assert!(patterns.is_empty());
    }

    #[test]
    fn exclusive_patterns_none_input_returns_empty() {
        let patterns = ApplicationServiceManager::exclusive_namespace_patterns(None, "users");
        assert!(patterns.is_empty());
    }

    #[test]
    fn exclusive_patterns_missing_kind_returns_empty() {
        let ns = make_namespaces();
        let patterns = ApplicationServiceManager::exclusive_namespace_patterns(Some(&ns), "nonexistent");
        assert!(patterns.is_empty());
    }

    #[test]
    fn exclusive_patterns_filters_empty_regex() {
        let ns = json!({
            "users": [
                {"exclusive": true, "regex": "  "},
            ]
        });
        let patterns = ApplicationServiceManager::exclusive_namespace_patterns(Some(&ns), "users");
        assert!(patterns.is_empty());
    }

    #[test]
    fn exclusive_patterns_multiple_exclusive() {
        let ns = json!({
            "users": [
                {"exclusive": true, "regex": "@a.*"},
                {"exclusive": false, "regex": "@b.*"},
                {"exclusive": true, "regex": "@c.*"},
            ]
        });
        let patterns = ApplicationServiceManager::exclusive_namespace_patterns(Some(&ns), "users");
        assert_eq!(patterns.len(), 2);
        assert_eq!(patterns[0], "@a.*");
        assert_eq!(patterns[1], "@c.*");
    }
}
