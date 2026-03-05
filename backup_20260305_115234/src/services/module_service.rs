use crate::common::error::ApiError;
use crate::storage::module::*;
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, instrument};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpamCheckResultType {
    #[serde(rename = "allow")]
    Allow,
    #[serde(rename = "block")]
    Block,
    #[serde(rename = "shadow_ban")]
    ShadowBan,
}

impl SpamCheckResultType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpamCheckResultType::Allow => "allow",
            SpamCheckResultType::Block => "block",
            SpamCheckResultType::ShadowBan => "shadow_ban",
        }
    }
}

impl FromStr for SpamCheckResultType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "allow" => Ok(SpamCheckResultType::Allow),
            "block" => Ok(SpamCheckResultType::Block),
            "shadow_ban" => Ok(SpamCheckResultType::ShadowBan),
            _ => Err(format!("Invalid spam check result type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpamCheckContext {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpamCheckOutput {
    pub result: SpamCheckResultType,
    pub score: i32,
    pub reason: Option<String>,
    pub action_taken: Option<String>,
}

#[async_trait]
pub trait SpamChecker: Send + Sync {
    fn name(&self) -> &str;

    async fn check(&self, context: &SpamCheckContext) -> Result<SpamCheckOutput, ApiError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyRuleContext {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: serde_json::Value,
    pub state_events: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyRuleOutput {
    pub allowed: bool,
    pub reason: Option<String>,
    pub modified_content: Option<serde_json::Value>,
}

#[async_trait]
pub trait ThirdPartyRule: Send + Sync {
    fn name(&self) -> &str;

    async fn check(
        &self,
        context: &ThirdPartyRuleContext,
    ) -> Result<ThirdPartyRuleOutput, ApiError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordAuthContext {
    pub user_id: String,
    pub password: String,
    pub device_id: Option<String>,
    pub initial_device_display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordAuthOutput {
    pub valid: bool,
    pub user_id: Option<String>,
}

#[async_trait]
pub trait PasswordAuthProviderTrait: Send + Sync {
    fn name(&self) -> &str;

    async fn check(&self, context: &PasswordAuthContext) -> Result<PasswordAuthOutput, ApiError>;
}

pub struct ModuleRegistry {
    spam_checkers: Vec<Arc<dyn SpamChecker>>,
    third_party_rules: Vec<Arc<dyn ThirdPartyRule>>,
    password_providers: Vec<Arc<dyn PasswordAuthProviderTrait>>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self {
            spam_checkers: Vec::new(),
            third_party_rules: Vec::new(),
            password_providers: Vec::new(),
        }
    }

    pub fn register_spam_checker(&mut self, checker: Arc<dyn SpamChecker>) {
        info!("Registering spam checker: {}", checker.name());
        self.spam_checkers.push(checker);
    }

    pub fn register_third_party_rule(&mut self, rule: Arc<dyn ThirdPartyRule>) {
        info!("Registering third party rule: {}", rule.name());
        self.third_party_rules.push(rule);
    }

    pub fn register_password_provider(&mut self, provider: Arc<dyn PasswordAuthProviderTrait>) {
        info!("Registering password provider: {}", provider.name());
        self.password_providers.push(provider);
    }

    pub fn spam_checkers(&self) -> &[Arc<dyn SpamChecker>] {
        &self.spam_checkers
    }

    pub fn third_party_rules(&self) -> &[Arc<dyn ThirdPartyRule>] {
        &self.third_party_rules
    }

    pub fn password_providers(&self) -> &[Arc<dyn PasswordAuthProviderTrait>] {
        &self.password_providers
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ModuleService {
    storage: Arc<ModuleStorage>,
    registry: Arc<tokio::sync::RwLock<ModuleRegistry>>,
}

impl ModuleService {
    pub fn new(storage: Arc<ModuleStorage>) -> Self {
        Self {
            storage,
            registry: Arc::new(tokio::sync::RwLock::new(ModuleRegistry::new())),
        }
    }

    #[instrument(skip(self))]
    pub async fn register_module(&self, request: CreateModuleRequest) -> Result<Module, ApiError> {
        info!(
            "Registering module: {} ({})",
            request.module_name, request.module_type
        );

        let module = self
            .storage
            .register_module(request)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to register module: {}", e)))?;

        Ok(module)
    }

    #[instrument(skip(self))]
    pub async fn get_module(&self, module_name: &str) -> Result<Option<Module>, ApiError> {
        let module = self
            .storage
            .get_module(module_name)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get module: {}", e)))?;

        Ok(module)
    }

    #[instrument(skip(self))]
    pub async fn get_modules_by_type(&self, module_type: &str) -> Result<Vec<Module>, ApiError> {
        let modules = self
            .storage
            .get_modules_by_type(module_type)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get modules: {}", e)))?;

        Ok(modules)
    }

    #[instrument(skip(self))]
    pub async fn get_all_modules(&self, limit: i64, offset: i64) -> Result<Vec<Module>, ApiError> {
        let modules = self
            .storage
            .get_all_modules(limit, offset)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get modules: {}", e)))?;

        Ok(modules)
    }

    #[instrument(skip(self))]
    pub async fn update_module_config(
        &self,
        module_name: &str,
        config: serde_json::Value,
    ) -> Result<Module, ApiError> {
        let module = self
            .storage
            .update_module_config(module_name, config)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update module config: {}", e)))?;

        Ok(module)
    }

    #[instrument(skip(self))]
    pub async fn enable_module(
        &self,
        module_name: &str,
        enabled: bool,
    ) -> Result<Module, ApiError> {
        let module = self
            .storage
            .enable_module(module_name, enabled)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to enable/disable module: {}", e)))?;

        Ok(module)
    }

    #[instrument(skip(self))]
    pub async fn delete_module(&self, module_name: &str) -> Result<(), ApiError> {
        self.storage
            .delete_module(module_name)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete module: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn check_spam(
        &self,
        context: &SpamCheckContext,
    ) -> Result<SpamCheckOutput, ApiError> {
        let registry = self.registry.read().await;
        let checkers = registry.spam_checkers();

        if checkers.is_empty() {
            return Ok(SpamCheckOutput {
                result: SpamCheckResultType::Allow,
                score: 0,
                reason: None,
                action_taken: None,
            });
        }

        let mut final_result = SpamCheckOutput {
            result: SpamCheckResultType::Allow,
            score: 0,
            reason: None,
            action_taken: None,
        };

        for checker in checkers {
            let start = Instant::now();
            let module_name = checker.name().to_string();

            match checker.check(context).await {
                Ok(output) => {
                    let execution_time = start.elapsed().as_millis() as i64;

                    let _ = self
                        .storage
                        .create_spam_check_result(CreateSpamCheckRequest {
                            event_id: context.event_id.clone(),
                            room_id: context.room_id.clone(),
                            sender: context.sender.clone(),
                            event_type: context.event_type.clone(),
                            content: context.content.clone(),
                            result: output.result.as_str().to_string(),
                            score: Some(output.score),
                            reason: output.reason.clone(),
                            checker_module: module_name.clone(),
                            action_taken: output.action_taken.clone(),
                        })
                        .await;

                    let _ = self
                        .storage
                        .record_execution(&module_name, true, None)
                        .await;

                    let _ = self
                        .storage
                        .create_execution_log(CreateExecutionLogRequest {
                            module_name: module_name.clone(),
                            module_type: "spam_checker".to_string(),
                            event_id: Some(context.event_id.clone()),
                            room_id: Some(context.room_id.clone()),
                            execution_time_ms: execution_time,
                            success: true,
                            error_message: None,
                            metadata: Some(serde_json::json!({
                                "result": output.result.as_str(),
                                "score": output.score,
                            })),
                        })
                        .await;

                    if output.score > final_result.score {
                        final_result = output;
                    }

                    if matches!(
                        final_result.result,
                        SpamCheckResultType::Block | SpamCheckResultType::ShadowBan
                    ) {
                        break;
                    }
                }
                Err(e) => {
                    let execution_time = start.elapsed().as_millis() as i64;
                    let error_msg = e.to_string();

                    error!("Spam checker {} failed: {}", module_name, error_msg);

                    let _ = self
                        .storage
                        .record_execution(&module_name, false, Some(&error_msg))
                        .await;

                    let _ = self
                        .storage
                        .create_execution_log(CreateExecutionLogRequest {
                            module_name: module_name.clone(),
                            module_type: "spam_checker".to_string(),
                            event_id: Some(context.event_id.clone()),
                            room_id: Some(context.room_id.clone()),
                            execution_time_ms: execution_time,
                            success: false,
                            error_message: Some(error_msg.clone()),
                            metadata: None,
                        })
                        .await;
                }
            }
        }

        Ok(final_result)
    }

    #[instrument(skip(self))]
    pub async fn check_third_party_rules(
        &self,
        context: &ThirdPartyRuleContext,
    ) -> Result<ThirdPartyRuleOutput, ApiError> {
        let registry = self.registry.read().await;
        let rules = registry.third_party_rules();

        if rules.is_empty() {
            return Ok(ThirdPartyRuleOutput {
                allowed: true,
                reason: None,
                modified_content: None,
            });
        }

        let mut current_content = context.content.clone();
        let mut allowed = true;
        let mut reason = None;

        for rule in rules {
            let start = Instant::now();
            let rule_name = rule.name().to_string();

            let mut rule_context = context.clone();
            rule_context.content = current_content.clone();

            match rule.check(&rule_context).await {
                Ok(output) => {
                    let execution_time = start.elapsed().as_millis() as i64;

                    let _ = self
                        .storage
                        .create_third_party_rule_result(CreateThirdPartyRuleRequest {
                            event_id: context.event_id.clone(),
                            room_id: context.room_id.clone(),
                            sender: context.sender.clone(),
                            event_type: context.event_type.clone(),
                            rule_name: rule_name.clone(),
                            allowed: output.allowed,
                            reason: output.reason.clone(),
                            modified_content: output.modified_content.clone(),
                        })
                        .await;

                    let _ = self
                        .storage
                        .create_execution_log(CreateExecutionLogRequest {
                            module_name: rule_name.clone(),
                            module_type: "third_party_rule".to_string(),
                            event_id: Some(context.event_id.clone()),
                            room_id: Some(context.room_id.clone()),
                            execution_time_ms: execution_time,
                            success: true,
                            error_message: None,
                            metadata: Some(serde_json::json!({
                                "allowed": output.allowed,
                            })),
                        })
                        .await;

                    if !output.allowed {
                        allowed = false;
                        reason = output.reason;
                        break;
                    }

                    if let Some(modified) = output.modified_content {
                        current_content = modified;
                    }
                }
                Err(e) => {
                    let execution_time = start.elapsed().as_millis() as i64;
                    let error_msg = e.to_string();

                    error!("Third party rule {} failed: {}", rule_name, error_msg);

                    let _ = self
                        .storage
                        .create_execution_log(CreateExecutionLogRequest {
                            module_name: rule_name.clone(),
                            module_type: "third_party_rule".to_string(),
                            event_id: Some(context.event_id.clone()),
                            room_id: Some(context.room_id.clone()),
                            execution_time_ms: execution_time,
                            success: false,
                            error_message: Some(error_msg),
                            metadata: None,
                        })
                        .await;
                }
            }
        }

        Ok(ThirdPartyRuleOutput {
            allowed,
            reason,
            modified_content: if current_content != context.content {
                Some(current_content)
            } else {
                None
            },
        })
    }

    #[instrument(skip(self))]
    pub async fn check_password_auth(
        &self,
        context: &PasswordAuthContext,
    ) -> Result<PasswordAuthOutput, ApiError> {
        let registry = self.registry.read().await;
        let providers = registry.password_providers();

        if providers.is_empty() {
            return Ok(PasswordAuthOutput {
                valid: false,
                user_id: None,
            });
        }

        for provider in providers {
            let start = Instant::now();
            let provider_name = provider.name().to_string();

            match provider.check(context).await {
                Ok(output) => {
                    let execution_time = start.elapsed().as_millis() as i64;

                    let _ = self
                        .storage
                        .create_execution_log(CreateExecutionLogRequest {
                            module_name: provider_name.clone(),
                            module_type: "password_provider".to_string(),
                            event_id: None,
                            room_id: None,
                            execution_time_ms: execution_time,
                            success: true,
                            error_message: None,
                            metadata: Some(serde_json::json!({
                                "valid": output.valid,
                                "user_id": output.user_id,
                            })),
                        })
                        .await;

                    if output.valid {
                        return Ok(output);
                    }
                }
                Err(e) => {
                    let execution_time = start.elapsed().as_millis() as i64;
                    let error_msg = e.to_string();

                    error!("Password provider {} failed: {}", provider_name, error_msg);

                    let _ = self
                        .storage
                        .create_execution_log(CreateExecutionLogRequest {
                            module_name: provider_name.clone(),
                            module_type: "password_provider".to_string(),
                            event_id: None,
                            room_id: None,
                            execution_time_ms: execution_time,
                            success: false,
                            error_message: Some(error_msg),
                            metadata: None,
                        })
                        .await;
                }
            }
        }

        Ok(PasswordAuthOutput {
            valid: false,
            user_id: None,
        })
    }

    pub fn registry(&self) -> Arc<tokio::sync::RwLock<ModuleRegistry>> {
        self.registry.clone()
    }

    pub async fn register_spam_checker(&self, checker: Arc<dyn SpamChecker>) {
        let mut registry = self.registry.write().await;
        registry.register_spam_checker(checker);
    }

    pub async fn register_third_party_rule(&self, rule: Arc<dyn ThirdPartyRule>) {
        let mut registry = self.registry.write().await;
        registry.register_third_party_rule(rule);
    }

    pub async fn register_password_provider(&self, provider: Arc<dyn PasswordAuthProviderTrait>) {
        let mut registry = self.registry.write().await;
        registry.register_password_provider(provider);
    }

    #[instrument(skip(self))]
    pub async fn get_spam_check_result(
        &self,
        event_id: &str,
    ) -> Result<Option<SpamCheckResult>, ApiError> {
        let result = self
            .storage
            .get_spam_check_result(event_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get spam check result: {}", e)))?;

        Ok(result)
    }

    #[instrument(skip(self))]
    pub async fn get_spam_check_results_by_sender(
        &self,
        sender: &str,
        limit: i64,
    ) -> Result<Vec<SpamCheckResult>, ApiError> {
        let results = self
            .storage
            .get_spam_check_results_by_sender(sender, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get spam check results: {}", e)))?;

        Ok(results)
    }

    #[instrument(skip(self))]
    pub async fn get_third_party_rule_results(
        &self,
        event_id: &str,
    ) -> Result<Vec<ThirdPartyRuleResult>, ApiError> {
        let results = self
            .storage
            .get_third_party_rule_results(event_id)
            .await
            .map_err(|e| {
                ApiError::internal(format!("Failed to get third party rule results: {}", e))
            })?;

        Ok(results)
    }

    #[instrument(skip(self))]
    pub async fn get_execution_logs(
        &self,
        module_name: &str,
        limit: i64,
    ) -> Result<Vec<ModuleExecutionLog>, ApiError> {
        let logs = self
            .storage
            .get_execution_logs(module_name, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get execution logs: {}", e)))?;

        Ok(logs)
    }
}

pub struct AccountValidityService {
    storage: Arc<ModuleStorage>,
}

impl AccountValidityService {
    pub fn new(storage: Arc<ModuleStorage>) -> Self {
        Self { storage }
    }

    #[instrument(skip(self))]
    pub async fn create_validity(
        &self,
        request: CreateAccountValidityRequest,
    ) -> Result<AccountValidity, ApiError> {
        let validity = self
            .storage
            .create_account_validity(request)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create account validity: {}", e)))?;

        Ok(validity)
    }

    #[instrument(skip(self))]
    pub async fn get_validity(&self, user_id: &str) -> Result<Option<AccountValidity>, ApiError> {
        let validity = self
            .storage
            .get_account_validity(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get account validity: {}", e)))?;

        Ok(validity)
    }

    #[instrument(skip(self))]
    pub async fn is_account_valid(&self, user_id: &str) -> Result<bool, ApiError> {
        let validity = self.get_validity(user_id).await?;

        if let Some(v) = validity {
            let now = Utc::now().timestamp_millis();
            Ok(v.is_valid && v.expiration_ts > now)
        } else {
            Ok(true)
        }
    }

    #[instrument(skip(self))]
    pub async fn renew_account(
        &self,
        user_id: &str,
        token: &str,
        new_expiration_ts: i64,
    ) -> Result<AccountValidity, ApiError> {
        let validity = self
            .storage
            .renew_account(user_id, token, new_expiration_ts)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to renew account: {}", e)))?;

        Ok(validity)
    }

    #[instrument(skip(self))]
    pub async fn set_renewal_token(&self, user_id: &str, token: &str) -> Result<(), ApiError> {
        self.storage
            .set_renewal_token(user_id, token)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to set renewal token: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_expired_accounts(
        &self,
        before_ts: i64,
    ) -> Result<Vec<AccountValidity>, ApiError> {
        let accounts = self
            .storage
            .get_expired_accounts(before_ts)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get expired accounts: {}", e)))?;

        Ok(accounts)
    }
}

pub struct SimpleSpamChecker {
    name: String,
    blocked_words: Vec<String>,
    max_message_length: usize,
}

impl SimpleSpamChecker {
    pub fn new(name: &str, blocked_words: Vec<String>, max_message_length: usize) -> Self {
        Self {
            name: name.to_string(),
            blocked_words,
            max_message_length,
        }
    }
}

#[async_trait]
impl SpamChecker for SimpleSpamChecker {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self, context: &SpamCheckContext) -> Result<SpamCheckOutput, ApiError> {
        let content_str = context.content.to_string();

        if content_str.len() > self.max_message_length {
            return Ok(SpamCheckOutput {
                result: SpamCheckResultType::Block,
                score: 100,
                reason: Some(format!(
                    "Message exceeds maximum length of {} characters",
                    self.max_message_length
                )),
                action_taken: Some("blocked".to_string()),
            });
        }

        for word in &self.blocked_words {
            if content_str.to_lowercase().contains(&word.to_lowercase()) {
                return Ok(SpamCheckOutput {
                    result: SpamCheckResultType::Block,
                    score: 80,
                    reason: Some(format!("Message contains blocked word: {}", word)),
                    action_taken: Some("blocked".to_string()),
                });
            }
        }

        Ok(SpamCheckOutput {
            result: SpamCheckResultType::Allow,
            score: 0,
            reason: None,
            action_taken: None,
        })
    }
}

pub struct SimpleThirdPartyRule {
    name: String,
    blocked_event_types: Vec<String>,
}

impl SimpleThirdPartyRule {
    pub fn new(name: &str, blocked_event_types: Vec<String>) -> Self {
        Self {
            name: name.to_string(),
            blocked_event_types,
        }
    }
}

#[async_trait]
impl ThirdPartyRule for SimpleThirdPartyRule {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(
        &self,
        context: &ThirdPartyRuleContext,
    ) -> Result<ThirdPartyRuleOutput, ApiError> {
        for blocked_type in &self.blocked_event_types {
            if context.event_type == *blocked_type {
                return Ok(ThirdPartyRuleOutput {
                    allowed: false,
                    reason: Some(format!("Event type {} is blocked", blocked_type)),
                    modified_content: None,
                });
            }
        }

        Ok(ThirdPartyRuleOutput {
            allowed: true,
            reason: None,
            modified_content: None,
        })
    }
}
