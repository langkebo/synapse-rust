use crate::common::error::ApiError;
use crate::storage::federation_blacklist::*;
use regex::Regex;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone)]
pub struct FederationBlacklistService {
    storage: Arc<FederationBlacklistStorage>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CheckResult {
    pub is_blocked: bool,
    pub is_whitelisted: bool,
    pub is_quarantined: bool,
    pub reason: Option<String>,
    pub matched_rule: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AddBlacklistRequest {
    pub server_name: String,
    pub block_type: String,
    pub reason: Option<String>,
    pub expires_in_days: Option<i32>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CheckServerRequest {
    pub server_name: String,
}

impl FederationBlacklistService {
    pub fn new(storage: Arc<FederationBlacklistStorage>) -> Self {
        Self { storage }
    }

    pub async fn add_to_blacklist(
        &self,
        request: AddBlacklistRequest,
        blocked_by: &str,
    ) -> Result<FederationBlacklist, ApiError> {
        if !matches!(
            request.block_type.as_str(),
            "blacklist" | "whitelist" | "quarantine"
        ) {
            return Err(ApiError::bad_request("Invalid block type"));
        }

        let expires_at = request.expires_in_days.map(|days| {
            let expiry = chrono::Utc::now() + chrono::Duration::days(days as i64);
            expiry.timestamp_millis()
        });

        let storage_request = crate::storage::federation_blacklist::AddBlacklistRequest {
            server_name: request.server_name,
            block_type: request.block_type,
            reason: request.reason,
            blocked_by: blocked_by.to_string(),
            expires_at,
            metadata: None,
        };

        let entry = self.storage.add_to_blacklist(storage_request).await?;

        self.storage
            .create_log(crate::storage::federation_blacklist::CreateLogRequest {
                server_name: entry.server_name.clone(),
                action: "add".to_string(),
                old_status: None,
                new_status: Some(entry.block_type.clone()),
                reason: entry.reason.clone(),
                performed_by: blocked_by.to_string(),
                ip_address: None,
                user_agent: None,
                metadata: None,
            })
            .await?;

        info!(
            "Added {} to {} by {}",
            entry.server_name, entry.block_type, blocked_by
        );
        Ok(entry)
    }

    pub async fn remove_from_blacklist(
        &self,
        server_name: &str,
        performed_by: &str,
    ) -> Result<(), ApiError> {
        self.storage
            .remove_from_blacklist(server_name, performed_by)
            .await
    }

    pub async fn check_server(&self, server_name: &str) -> Result<CheckResult, ApiError> {
        let is_whitelisted = self.storage.is_server_whitelisted(server_name).await?;

        if is_whitelisted {
            return Ok(CheckResult {
                is_blocked: false,
                is_whitelisted: true,
                is_quarantined: false,
                reason: Some("Server is whitelisted".to_string()),
                matched_rule: None,
            });
        }

        let entry = self.storage.get_blacklist_entry(server_name).await?;

        if let Some(entry) = entry {
            if let Some(expires_at) = entry.expires_at {
                let now = chrono::Utc::now().timestamp_millis();
                if expires_at < now {
                    return Ok(CheckResult {
                        is_blocked: false,
                        is_whitelisted: false,
                        is_quarantined: false,
                        reason: Some("Block has expired".to_string()),
                        matched_rule: None,
                    });
                }
            }

            return Ok(CheckResult {
                is_blocked: entry.block_type == "blacklist",
                is_whitelisted: false,
                is_quarantined: entry.block_type == "quarantine",
                reason: entry.reason.clone(),
                matched_rule: Some("direct_block".to_string()),
            });
        }

        let rules = self.storage.get_all_rules().await?;
        for rule in rules {
            if self.matches_rule(server_name, &rule)? {
                return Ok(CheckResult {
                    is_blocked: rule.action == "block",
                    is_whitelisted: rule.action == "allow",
                    is_quarantined: rule.action == "quarantine",
                    reason: rule.description.clone(),
                    matched_rule: Some(rule.rule_name.clone()),
                });
            }
        }

        let default_action = self
            .storage
            .get_config("default_action")
            .await?
            .unwrap_or_else(|| "allow".to_string());

        Ok(CheckResult {
            is_blocked: default_action == "block",
            is_whitelisted: false,
            is_quarantined: default_action == "quarantine",
            reason: Some(format!("Default action: {}", default_action)),
            matched_rule: Some("default".to_string()),
        })
    }

    fn matches_rule(
        &self,
        server_name: &str,
        rule: &FederationBlacklistRule,
    ) -> Result<bool, ApiError> {
        match rule.rule_type.as_str() {
            "domain" => Ok(server_name == rule.pattern),
            "regex" => {
                let re = Regex::new(&rule.pattern)
                    .map_err(|e| ApiError::internal(format!("Invalid regex pattern: {}", e)))?;
                Ok(re.is_match(server_name))
            }
            "wildcard" => {
                let pattern = rule.pattern.replace('*', ".*");
                let re = Regex::new(&format!("^{}$", pattern))
                    .map_err(|e| ApiError::internal(format!("Invalid wildcard pattern: {}", e)))?;
                Ok(re.is_match(server_name))
            }
            "cidr" => Ok(false),
            _ => Ok(false),
        }
    }

    pub async fn record_access(
        &self,
        server_name: &str,
        success: bool,
        response_time_ms: Option<f64>,
    ) -> Result<FederationAccessStats, ApiError> {
        let stats = self
            .storage
            .update_access_stats(crate::storage::federation_blacklist::UpdateStatsRequest {
                server_name: server_name.to_string(),
                success,
                response_time_ms,
            })
            .await?;

        if !success {
            self.check_auto_blacklist(server_name).await?;
        }

        Ok(stats)
    }

    async fn check_auto_blacklist(&self, server_name: &str) -> Result<(), ApiError> {
        let auto_blacklist_enabled = self
            .storage
            .get_config_as_bool("enable_auto_blacklist", true)
            .await?;

        if !auto_blacklist_enabled {
            return Ok(());
        }

        let threshold = self
            .storage
            .get_config_as_int("auto_blacklist_threshold", 10)
            .await?;
        let window_minutes = self
            .storage
            .get_config_as_int("auto_blacklist_window_minutes", 60)
            .await?;

        let stats = self.storage.get_access_stats(server_name).await?;

        if let Some(stats) = stats {
            if stats.failed_requests >= threshold as i64 {
                if let Some(last_failure_ts) = stats.last_failure_ts {
                    let window_start = (chrono::Utc::now()
                        - chrono::Duration::minutes(window_minutes as i64))
                    .timestamp_millis();
                    if last_failure_ts > window_start {
                        info!(
                            "Auto-blacklisting server {} due to {} failures",
                            server_name, stats.failed_requests
                        );

                        self.add_to_blacklist(
                            AddBlacklistRequest {
                                server_name: server_name.to_string(),
                                block_type: "blacklist".to_string(),
                                reason: Some(format!(
                                    "Auto-blacklisted: {} failures in {} minutes",
                                    stats.failed_requests, window_minutes
                                )),
                                expires_in_days: Some(7),
                            },
                            "system",
                        )
                        .await?;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn get_blacklist(
        &self,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<FederationBlacklist>, ApiError> {
        self.storage.get_all_blacklist(limit, offset).await
    }

    pub async fn get_stats(
        &self,
        server_name: &str,
    ) -> Result<Option<FederationAccessStats>, ApiError> {
        self.storage.get_access_stats(server_name).await
    }

    pub async fn cleanup_expired(&self) -> Result<u64, ApiError> {
        self.storage.cleanup_expired_entries().await
    }

    pub async fn create_rule(
        &self,
        request: crate::storage::federation_blacklist::CreateRuleRequest,
    ) -> Result<FederationBlacklistRule, ApiError> {
        if !matches!(
            request.rule_type.as_str(),
            "domain" | "regex" | "wildcard" | "cidr"
        ) {
            return Err(ApiError::bad_request("Invalid rule type"));
        }

        if !matches!(
            request.action.as_str(),
            "block" | "allow" | "quarantine" | "rate_limit"
        ) {
            return Err(ApiError::bad_request("Invalid action"));
        }

        self.storage.create_rule(request).await
    }

    pub async fn get_rules(&self) -> Result<Vec<FederationBlacklistRule>, ApiError> {
        self.storage.get_all_rules().await
    }
}
