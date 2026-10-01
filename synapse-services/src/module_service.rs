use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;
use synapse_common::ThirdPartyRulesConfig;
use synapse_storage::module::*;
pub use synapse_storage::module::{
    AccountDataCallback, AccountValidity, CreateAccountDataCallbackRequest, CreateAccountValidityRequest,
    CreateMediaCallbackRequest, CreateModuleRequest, CreatePasswordAuthProviderRequest, MediaCallback, Module,
    PasswordAuthProvider, SpamCheckResult, ThirdPartyRuleResult,
};
use tracing::{error, info, instrument};

/// The `SpamCheckResultType` enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpamCheckResultType {
    #[serde(rename = "allow")]
    /// The `Allow` variant.
    Allow,
    #[serde(rename = "block")]
    /// The `Block` variant.
    Block,
    #[serde(rename = "shadow_ban")]
    /// The `ShadowBan` variant.
    ShadowBan,
}

impl SpamCheckResultType {
    /// See [`as_str`].
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Block => "block",
            Self::ShadowBan => "shadow_ban",
        }
    }
}

impl FromStr for SpamCheckResultType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "allow" => Ok(Self::Allow),
            "block" => Ok(Self::Block),
            "shadow_ban" => Ok(Self::ShadowBan),
            _ => Err(format!("Invalid spam check result type: {s}")),
        }
    }
}

/// The `SpamCheckContext` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpamCheckContext {
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `content` field.
    pub content: serde_json::Value,
}

/// The `SpamCheckOutput` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpamCheckOutput {
    /// The `result` field.
    pub result: SpamCheckResultType,
    /// The `score` field.
    pub score: i32,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `action_taken` field.
    pub action_taken: Option<String>,
}

/// The `SpamChecker` trait.
#[async_trait]
pub trait SpamChecker: Send + Sync {
    /// See [`name`].
    fn name(&self) -> &str;

    /// See [`check`].
    async fn check(&self, context: &SpamCheckContext) -> Result<SpamCheckOutput, ApiError>;
}

/// The `ThirdPartyRuleContext` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyRuleContext {
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `state_events` field.
    pub state_events: Vec<serde_json::Value>,
}

/// The `ThirdPartyRuleOutput` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyRuleOutput {
    #[serde(rename = "allowed")]
    /// The `is_allowed` field.
    pub is_allowed: bool,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `modified_content` field.
    pub modified_content: Option<serde_json::Value>,
}

/// The `ThirdPartyRule` trait.
#[async_trait]
pub trait ThirdPartyRule: Send + Sync {
    /// See [`name`].
    fn name(&self) -> &str;

    /// See [`check`].
    async fn check(&self, context: &ThirdPartyRuleContext) -> Result<ThirdPartyRuleOutput, ApiError>;
}

/// The `EventAdmissionGate` trait.
///
/// The narrow seam through which the event write path enforces third-party
/// `check_event_allowed` rules. It is deliberately *narrower* than
/// [`ModuleService`]: the messaging layer must be able to **enforce** admission
/// policy without holding the admin-facing read/write surface, and unit tests
/// need a double that does not require Postgres. A trait object rather than an
/// `Option<...>` because a gate that can be absent is a gate that can be
/// skipped — this is the one seam every event write goes through.
///
/// [`has_event_rules`] is separated from [`check_event_allowed`] so the write
/// path can take a zero-cost fast path (and avoid a state read) when no rules
/// are registered — the steady state for a server with no modules installed.
#[async_trait]
pub trait EventAdmissionGate: Send + Sync {
    /// Whether any third-party rule is currently registered.
    ///
    /// See [`check_event_allowed`].
    async fn has_event_rules(&self) -> bool;

    /// Check whether the prospective event is admitted, and return the
    /// (possibly rule-modified) content.
    ///
    /// See [`ThirdPartyRuleOutput`].
    async fn check_event_allowed(&self, context: &ThirdPartyRuleContext) -> Result<ThirdPartyRuleOutput, ApiError>;
}

#[async_trait]
impl EventAdmissionGate for ModuleService {
    async fn has_event_rules(&self) -> bool {
        !self.registry.read().await.third_party_rules().is_empty()
    }

    async fn check_event_allowed(&self, context: &ThirdPartyRuleContext) -> Result<ThirdPartyRuleOutput, ApiError> {
        self.check_third_party_rules(context).await
    }
}

/// Consult the third-party event admission gate for a prospective event.
///
/// This is the **single** implementation of "ask the rules whether this event
/// may be persisted, and apply any content rewrite they returned". Every local
/// write entry point routes through it — the messaging chokepoints
/// ([`crate::room::messaging`]) and the membership flows
/// ([`crate::room::membership`]) — so the ordering rule (consult before the
/// state change, refuse with `403`) and the rewrite rule live in exactly one
/// place.
///
/// `allow_modification` is `true` only when the persisted bytes are the ones
/// *we* authored and are about to write, so a rule may rewrite `params.content`.
/// It is `false` for a PDU another server already signed and hashed (inbound
/// federation state) or whose signed/returned form is what we persist — a
/// rewrite there could not be honoured and would desync the event from its
/// signature.
///
/// A server with no rules registered takes the [`EventAdmissionGate::has_event_rules`]
/// fast path and never reads room state, so the steady state costs one
/// in-memory lock acquisition.
pub async fn consult_event_admission(
    gate: &dyn EventAdmissionGate,
    event_reader: &dyn synapse_storage::EventReader,
    params: &mut synapse_storage::CreateEventParams,
    allow_modification: bool,
) -> Result<(), ApiError> {
    if !gate.has_event_rules().await {
        return Ok(());
    }

    let state_events = event_reader
        .get_state_events(&params.room_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to read room state for event admission", e))?
        .iter()
        .map(|e| {
            serde_json::json!({
                "event_id": e.event_id,
                "sender": e.user_id,
                "type": e.event_type,
                "content": e.content,
                "state_key": e.state_key,
            })
        })
        .collect();

    let context = ThirdPartyRuleContext {
        event_id: params.event_id.clone(),
        room_id: params.room_id.clone(),
        sender: params.user_id.clone(),
        event_type: params.event_type.clone(),
        content: params.content.clone(),
        state_events,
    };

    let outcome = gate.check_event_allowed(&context).await?;

    if !outcome.is_allowed {
        return Err(ApiError::forbidden(
            outcome.reason.unwrap_or_else(|| "Event refused by a third-party rule".to_string()),
        ));
    }

    if allow_modification {
        if let Some(modified) = outcome.modified_content {
            params.content = modified;
        }
    }

    Ok(())
}

/// The `PasswordAuthContext` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordAuthContext {
    /// The `user_id` field.
    pub user_id: String,
    /// The `password` field.
    pub password: String,
    /// The `device_id` field.
    pub device_id: Option<String>,
    /// The `initial_device_display_name` field.
    pub initial_device_display_name: Option<String>,
}

/// The `PasswordAuthOutput` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordAuthOutput {
    /// The `valid` field.
    pub valid: bool,
    /// The `user_id` field.
    pub user_id: Option<String>,
}

/// The `PasswordAuthProviderTrait` trait.
#[async_trait]
pub trait PasswordAuthProviderTrait: Send + Sync {
    /// See [`name`].
    fn name(&self) -> &str;

    /// See [`check`].
    async fn check(&self, context: &PasswordAuthContext) -> Result<PasswordAuthOutput, ApiError>;
}

/// The `ModuleRegistry` struct.
pub struct ModuleRegistry {
    spam_checkers: Vec<Arc<dyn SpamChecker>>,
    third_party_rules: Vec<Arc<dyn ThirdPartyRule>>,
    password_providers: Vec<Arc<dyn PasswordAuthProviderTrait>>,
}

impl ModuleRegistry {
    /// See [`new`].
    pub fn new() -> Self {
        Self { spam_checkers: Vec::new(), third_party_rules: Vec::new(), password_providers: Vec::new() }
    }

    /// See [`register_spam_checker`].
    pub fn register_spam_checker(&mut self, checker: Arc<dyn SpamChecker>) {
        info!(module_name = %checker.name(), module_type = %"spam_checker", "Registering spam checker");
        self.spam_checkers.push(checker);
    }

    /// See [`register_third_party_rule`].
    pub fn register_third_party_rule(&mut self, rule: Arc<dyn ThirdPartyRule>) {
        info!(module_name = %rule.name(), module_type = %"third_party_rule", "Registering third party rule");
        self.third_party_rules.push(rule);
    }

    /// See [`register_password_provider`].
    pub fn register_password_provider(&mut self, provider: Arc<dyn PasswordAuthProviderTrait>) {
        info!(module_name = %provider.name(), module_type = %"password_provider", "Registering password provider");
        self.password_providers.push(provider);
    }

    /// See [`spam_checkers`].
    pub fn spam_checkers(&self) -> &[Arc<dyn SpamChecker>] {
        &self.spam_checkers
    }

    /// See [`third_party_rules`].
    pub fn third_party_rules(&self) -> &[Arc<dyn ThirdPartyRule>] {
        &self.third_party_rules
    }

    /// See [`password_providers`].
    pub fn password_providers(&self) -> &[Arc<dyn PasswordAuthProviderTrait>] {
        &self.password_providers
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// The `ModuleService` struct.
pub struct ModuleService {
    storage: Arc<synapse_storage::module::ModuleStorage>,
    registry: Arc<tokio::sync::RwLock<ModuleRegistry>>,
}

impl ModuleService {
    /// See [`new`].
    pub fn new(storage: Arc<synapse_storage::module::ModuleStorage>) -> Self {
        Self { storage, registry: Arc::new(tokio::sync::RwLock::new(ModuleRegistry::new())) }
    }

    /// See [`register_module`].
    #[instrument(skip(self))]
    pub async fn register_module(&self, request: CreateModuleRequest) -> Result<Module, ApiError> {
        info!(module_name = %request.module_name, module_type = %request.module_type, "Registering module");

        let module = self
            .storage
            .register_module(request)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to register module", e))?;

        Ok(module)
    }

    /// See [`get_module`].
    #[instrument(skip(self))]
    pub async fn get_module(&self, module_name: &str) -> Result<Option<Module>, ApiError> {
        let module = self
            .storage
            .get_module(module_name)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get module", e))?;

        Ok(module)
    }

    /// See [`get_modules_by_type`].
    #[instrument(skip(self))]
    pub async fn get_modules_by_type(&self, module_type: &str) -> Result<Vec<Module>, ApiError> {
        let modules = self
            .storage
            .get_modules_by_type(module_type)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get modules", e))?;

        Ok(modules)
    }

    /// See [`get_all_modules`].
    #[instrument(skip(self))]
    pub async fn get_all_modules(
        &self,
        limit: i64,
        from: Option<String>,
    ) -> Result<(Vec<Module>, Option<String>), ApiError> {
        let (modules, next_from) = self
            .storage
            .get_all_modules(limit, from)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get modules", e))?;

        Ok((modules, next_from))
    }

    /// See [`update_module_config`].
    #[instrument(skip(self))]
    pub async fn update_module_config(&self, module_name: &str, config: serde_json::Value) -> Result<Module, ApiError> {
        let module = self
            .storage
            .update_module_config(module_name, config)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to update module config", e))?;

        Ok(module)
    }

    /// See [`enable_module`].
    #[instrument(skip(self))]
    pub async fn enable_module(&self, module_name: &str, enabled: bool) -> Result<Module, ApiError> {
        let module = self
            .storage
            .enable_module(module_name, enabled)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to enable/disable module", e))?;

        Ok(module)
    }

    /// See [`delete_module`].
    #[instrument(skip(self))]
    pub async fn delete_module(&self, module_name: &str) -> Result<(), ApiError> {
        self.storage
            .delete_module(module_name)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete module", e))?;

        Ok(())
    }

    /// See [`check_spam`].
    #[instrument(skip(self))]
    pub async fn check_spam(&self, context: &SpamCheckContext) -> Result<SpamCheckOutput, ApiError> {
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

        let mut final_result =
            SpamCheckOutput { result: SpamCheckResultType::Allow, score: 0, reason: None, action_taken: None };

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

                    let _ = self.storage.record_execution(&module_name, true, None).await;

                    let _ = self
                        .storage
                        .create_execution_log(CreateExecutionLogRequest {
                            module_name: module_name.clone(),
                            module_type: "spam_checker".to_string(),
                            event_id: Some(context.event_id.clone()),
                            room_id: Some(context.room_id.clone()),
                            execution_time_ms: execution_time,
                            is_success: true,
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

                    if matches!(final_result.result, SpamCheckResultType::Block | SpamCheckResultType::ShadowBan) {
                        break;
                    }
                }
                Err(e) => {
                    let execution_time = start.elapsed().as_millis() as i64;
                    let error_msg = e.to_string();

                    error!(
                        module_name = %module_name,
                        module_type = %"spam_checker",
                        event_id = %context.event_id,
                        room_id = %context.room_id,
                        execution_time_ms = execution_time,
                        error_message = %error_msg,
                        "Spam checker failed"
                    );

                    let _ = self.storage.record_execution(&module_name, false, Some(&error_msg)).await;

                    let _ = self
                        .storage
                        .create_execution_log(CreateExecutionLogRequest {
                            module_name: module_name.clone(),
                            module_type: "spam_checker".to_string(),
                            event_id: Some(context.event_id.clone()),
                            room_id: Some(context.room_id.clone()),
                            execution_time_ms: execution_time,
                            is_success: false,
                            error_message: Some(error_msg.clone()),
                            metadata: None,
                        })
                        .await;
                }
            }
        }

        Ok(final_result)
    }

    /// See [`check_third_party_rules`].
    #[instrument(skip(self))]
    pub async fn check_third_party_rules(
        &self,
        context: &ThirdPartyRuleContext,
    ) -> Result<ThirdPartyRuleOutput, ApiError> {
        let registry = self.registry.read().await;
        let rules = registry.third_party_rules();

        if rules.is_empty() {
            return Ok(ThirdPartyRuleOutput { is_allowed: true, reason: None, modified_content: None });
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
                            is_allowed: output.is_allowed,
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
                            is_success: true,
                            error_message: None,
                            metadata: Some(serde_json::json!({
                                "allowed": output.is_allowed,
                            })),
                        })
                        .await;

                    if !output.is_allowed {
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

                    error!(
                        module_name = %rule_name,
                        module_type = %"third_party_rule",
                        event_id = %context.event_id,
                        room_id = %context.room_id,
                        execution_time_ms = execution_time,
                        error_message = %error_msg,
                        "Third party rule failed"
                    );

                    let _ = self
                        .storage
                        .create_execution_log(CreateExecutionLogRequest {
                            module_name: rule_name.clone(),
                            module_type: "third_party_rule".to_string(),
                            event_id: Some(context.event_id.clone()),
                            room_id: Some(context.room_id.clone()),
                            execution_time_ms: execution_time,
                            is_success: false,
                            error_message: Some(error_msg),
                            metadata: None,
                        })
                        .await;
                }
            }
        }

        Ok(ThirdPartyRuleOutput {
            is_allowed: allowed,
            reason,
            modified_content: if current_content != context.content { Some(current_content) } else { None },
        })
    }

    /// See [`check_password_auth`].
    #[instrument(skip(self))]
    pub async fn check_password_auth(&self, context: &PasswordAuthContext) -> Result<PasswordAuthOutput, ApiError> {
        let registry = self.registry.read().await;
        let providers = registry.password_providers();

        if providers.is_empty() {
            return Ok(PasswordAuthOutput { valid: false, user_id: None });
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
                            is_success: true,
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

                    error!(
                        module_name = %provider_name,
                        module_type = %"password_provider",
                        username = %context.user_id,
                        execution_time_ms = execution_time,
                        error_message = %error_msg,
                        "Password provider failed"
                    );

                    let _ = self
                        .storage
                        .create_execution_log(CreateExecutionLogRequest {
                            module_name: provider_name.clone(),
                            module_type: "password_provider".to_string(),
                            event_id: None,
                            room_id: None,
                            execution_time_ms: execution_time,
                            is_success: false,
                            error_message: Some(error_msg),
                            metadata: None,
                        })
                        .await;
                }
            }
        }

        Ok(PasswordAuthOutput { valid: false, user_id: None })
    }

    /// See [`registry`].
    pub fn registry(&self) -> Arc<tokio::sync::RwLock<ModuleRegistry>> {
        self.registry.clone()
    }

    /// See [`register_spam_checker`].
    pub async fn register_spam_checker(&self, checker: Arc<dyn SpamChecker>) {
        let mut registry = self.registry.write().await;
        registry.register_spam_checker(checker);
    }

    /// See [`register_third_party_rule`].
    pub async fn register_third_party_rule(&self, rule: Arc<dyn ThirdPartyRule>) {
        let mut registry = self.registry.write().await;
        registry.register_third_party_rule(rule);
    }

    /// Register the third-party event admission rules described by
    /// [`ThirdPartyRulesConfig`], as a built-in [`SimpleThirdPartyRule`] per
    /// entry. This is the production wiring of the `check_event_allowed`
    /// trigger: it runs once at service assembly so the gate reported by
    /// [`EventAdmissionGate::has_event_rules`] is truthful from the first event
    /// write.
    ///
    /// An empty `config.rules` (the default) registers nothing, leaving the
    /// admission gate disabled.
    pub async fn register_configured_third_party_rules(&self, config: &ThirdPartyRulesConfig) {
        for configured in &config.rules {
            self.register_third_party_rule(Arc::new(SimpleThirdPartyRule::new(
                &configured.name,
                configured.blocked_event_types.clone(),
            )))
            .await;
        }
    }

    /// See [`register_password_provider`].
    pub async fn register_password_provider(&self, provider: Arc<dyn PasswordAuthProviderTrait>) {
        let mut registry = self.registry.write().await;
        registry.register_password_provider(provider);
    }

    /// See [`get_spam_check_result`].
    #[instrument(skip(self))]
    pub async fn get_spam_check_result(&self, event_id: &str) -> Result<Option<SpamCheckResult>, ApiError> {
        let result = self
            .storage
            .get_spam_check_result(event_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get spam check result", e))?;

        Ok(result)
    }

    /// See [`get_spam_check_results_by_sender`].
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
            .map_err(|e| ApiError::internal_with_cause("Failed to get spam check results", e))?;

        Ok(results)
    }

    /// See [`get_third_party_rule_results`].
    #[instrument(skip(self))]
    pub async fn get_third_party_rule_results(&self, event_id: &str) -> Result<Vec<ThirdPartyRuleResult>, ApiError> {
        let results = self
            .storage
            .get_third_party_rule_results(event_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get third party rule results", e))?;

        Ok(results)
    }

    /// Create a password-auth provider row.
    pub async fn create_password_auth_provider(
        &self,
        request: CreatePasswordAuthProviderRequest,
    ) -> Result<PasswordAuthProvider, ApiError> {
        self.storage
            .create_password_auth_provider(request)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create password auth provider", e))
    }

    /// List password-auth provider rows.
    pub async fn get_password_auth_providers(&self) -> Result<Vec<PasswordAuthProvider>, ApiError> {
        self.storage
            .get_password_auth_providers()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get password auth providers", e))
    }

    /// Create a media callback row.
    pub async fn create_media_callback(&self, request: CreateMediaCallbackRequest) -> Result<MediaCallback, ApiError> {
        self.storage
            .create_media_callback(request)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create media callback", e))
    }

    /// List enabled media callbacks, optionally filtered by callback type.
    pub async fn get_media_callbacks(&self, callback_type: Option<&str>) -> Result<Vec<MediaCallback>, ApiError> {
        self.storage
            .get_media_callbacks(callback_type)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get media callbacks", e))
    }

    /// Create an account-data callback row.
    pub async fn create_account_data_callback(
        &self,
        request: CreateAccountDataCallbackRequest,
    ) -> Result<AccountDataCallback, ApiError> {
        self.storage
            .create_account_data_callback(request)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create account data callback", e))
    }

    /// List enabled account-data callbacks.
    pub async fn get_account_data_callbacks(&self) -> Result<Vec<AccountDataCallback>, ApiError> {
        self.storage
            .get_account_data_callbacks()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get account data callbacks", e))
    }

    /// See [`get_execution_logs`].
    #[instrument(skip(self))]
    pub async fn get_execution_logs(&self, module_name: &str, limit: i64) -> Result<Vec<ModuleExecutionLog>, ApiError> {
        let logs = self
            .storage
            .get_execution_logs(module_name, limit)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get execution logs", e))?;

        Ok(logs)
    }
}

/// The `AccountValidityService` struct.
pub struct AccountValidityService {
    storage: Arc<synapse_storage::module::ModuleStorage>,
}

impl AccountValidityService {
    /// See [`new`].
    pub fn new(storage: Arc<synapse_storage::module::ModuleStorage>) -> Self {
        Self { storage }
    }

    /// See [`create_validity`].
    #[instrument(skip(self))]
    pub async fn create_validity(&self, request: CreateAccountValidityRequest) -> Result<AccountValidity, ApiError> {
        let validity = self
            .storage
            .create_account_validity(request)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create account validity", e))?;

        Ok(validity)
    }

    /// See [`get_validity`].
    #[instrument(skip(self))]
    pub async fn get_validity(&self, user_id: &str) -> Result<Option<AccountValidity>, ApiError> {
        let validity = self
            .storage
            .get_account_validity(user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get account validity", e))?;

        Ok(validity)
    }

    /// See [`is_account_valid`].
    #[instrument(skip(self))]
    pub async fn is_account_valid(&self, user_id: &str) -> Result<bool, ApiError> {
        let validity = self.get_validity(user_id).await?;

        if let Some(v) = validity {
            let now = current_timestamp_millis();
            Ok(v.is_valid && v.expiration_at.is_none_or(|e| e > now))
        } else {
            Ok(true)
        }
    }

    /// See [`renew_account`].
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
            .map_err(|e| ApiError::internal_with_cause("Failed to renew account", e))?;

        Ok(validity)
    }

    /// See [`set_renewal_token`].
    #[instrument(skip(self))]
    pub async fn set_renewal_token(&self, user_id: &str, token: &str) -> Result<(), ApiError> {
        self.storage
            .set_renewal_token(user_id, token)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to set renewal token", e))?;

        Ok(())
    }

    /// See [`get_expired_accounts`].
    #[instrument(skip(self))]
    pub async fn get_expired_accounts(&self, before_ts: i64) -> Result<Vec<AccountValidity>, ApiError> {
        let accounts = self
            .storage
            .get_expired_accounts(before_ts)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get expired accounts", e))?;

        Ok(accounts)
    }
}

/// The `SimpleSpamChecker` struct.
pub struct SimpleSpamChecker {
    name: String,
    blocked_words: Vec<String>,
    max_message_length: usize,
}

impl SimpleSpamChecker {
    /// See [`new`].
    pub fn new(name: &str, blocked_words: Vec<String>, max_message_length: usize) -> Self {
        Self { name: name.to_string(), blocked_words, max_message_length }
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
                reason: Some(format!("Message exceeds maximum length of {} characters", self.max_message_length)),
                action_taken: Some("blocked".to_string()),
            });
        }

        for word in &self.blocked_words {
            if content_str.to_lowercase().contains(&word.to_lowercase()) {
                return Ok(SpamCheckOutput {
                    result: SpamCheckResultType::Block,
                    score: 80,
                    reason: Some(format!("Message contains blocked word: {word}")),
                    action_taken: Some("blocked".to_string()),
                });
            }
        }

        Ok(SpamCheckOutput { result: SpamCheckResultType::Allow, score: 0, reason: None, action_taken: None })
    }
}

/// The `SimpleThirdPartyRule` struct.
pub struct SimpleThirdPartyRule {
    name: String,
    blocked_event_types: Vec<String>,
}

impl SimpleThirdPartyRule {
    /// See [`new`].
    pub fn new(name: &str, blocked_event_types: Vec<String>) -> Self {
        Self { name: name.to_string(), blocked_event_types }
    }
}

#[async_trait]
impl ThirdPartyRule for SimpleThirdPartyRule {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self, context: &ThirdPartyRuleContext) -> Result<ThirdPartyRuleOutput, ApiError> {
        for blocked_type in &self.blocked_event_types {
            if context.event_type == *blocked_type {
                return Ok(ThirdPartyRuleOutput {
                    is_allowed: false,
                    reason: Some(format!("Event type {blocked_type} is blocked")),
                    modified_content: None,
                });
            }
        }

        Ok(ThirdPartyRuleOutput { is_allowed: true, reason: None, modified_content: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== SpamCheckResultType tests ==========

    #[test]
    fn test_spam_check_result_type_as_str() {
        assert_eq!(SpamCheckResultType::Allow.as_str(), "allow");
        assert_eq!(SpamCheckResultType::Block.as_str(), "block");
        assert_eq!(SpamCheckResultType::ShadowBan.as_str(), "shadow_ban");
    }

    #[test]
    fn test_spam_check_result_type_from_str() {
        assert_eq!("allow".parse::<SpamCheckResultType>().unwrap().as_str(), "allow");
        assert_eq!("block".parse::<SpamCheckResultType>().unwrap().as_str(), "block");
        assert_eq!("shadow_ban".parse::<SpamCheckResultType>().unwrap().as_str(), "shadow_ban");
    }

    #[test]
    fn test_spam_check_result_type_from_str_invalid() {
        assert!("invalid".parse::<SpamCheckResultType>().is_err());
        assert!("".parse::<SpamCheckResultType>().is_err());
    }

    // ========== SpamCheckContext tests ==========

    #[test]
    fn test_spam_check_context() {
        let ctx = SpamCheckContext {
            event_id: "$ev1:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": "hello"}),
        };
        assert_eq!(ctx.event_type, "m.room.message");
        assert_eq!(ctx.sender, "@alice:example.com");
    }

    // ========== SpamCheckOutput tests ==========

    #[test]
    fn test_spam_check_output_allow() {
        let output = SpamCheckOutput { result: SpamCheckResultType::Allow, score: 0, reason: None, action_taken: None };
        assert_eq!(output.result.as_str(), "allow");
        assert_eq!(output.score, 0);
    }

    #[test]
    fn test_spam_check_output_block() {
        let output = SpamCheckOutput {
            result: SpamCheckResultType::Block,
            score: 100,
            reason: Some("blocked word".to_string()),
            action_taken: Some("blocked".to_string()),
        };
        assert_eq!(output.result.as_str(), "block");
        assert_eq!(output.score, 100);
    }

    // ========== ThirdPartyRuleContext tests ==========

    #[test]
    fn test_third_party_rule_context() {
        let ctx = ThirdPartyRuleContext {
            event_id: "$ev1:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@bob:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": "test"}),
            state_events: vec![],
        };
        assert_eq!(ctx.event_type, "m.room.message");
        assert!(ctx.state_events.is_empty());
    }

    // ========== ThirdPartyRuleOutput tests ==========

    #[test]
    fn test_third_party_rule_output_allowed() {
        let output = ThirdPartyRuleOutput { is_allowed: true, reason: None, modified_content: None };
        assert!(output.is_allowed);
    }

    #[test]
    fn test_third_party_rule_output_blocked() {
        let output = ThirdPartyRuleOutput {
            is_allowed: false,
            reason: Some("blocked type".to_string()),
            modified_content: None,
        };
        assert!(!output.is_allowed);
    }

    // ========== PasswordAuthContext tests ==========

    #[test]
    fn test_password_auth_context() {
        let ctx = PasswordAuthContext {
            user_id: "@alice:example.com".to_string(),
            password: "secret".to_string(),
            device_id: Some("DEV123".to_string()),
            initial_device_display_name: Some("My Device".to_string()),
        };
        assert_eq!(ctx.user_id, "@alice:example.com");
        assert_eq!(ctx.device_id, Some("DEV123".to_string()));
    }

    // ========== PasswordAuthOutput tests ==========

    #[test]
    fn test_password_auth_output_valid() {
        let output = PasswordAuthOutput { valid: true, user_id: Some("@alice:example.com".to_string()) };
        assert!(output.valid);
        assert_eq!(output.user_id, Some("@alice:example.com".to_string()));
    }

    #[test]
    fn test_password_auth_output_invalid() {
        let output = PasswordAuthOutput { valid: false, user_id: None };
        assert!(!output.valid);
        assert!(output.user_id.is_none());
    }

    // ========== ModuleRegistry tests ==========

    #[test]
    fn test_module_registry_new() {
        let registry = ModuleRegistry::new();
        assert!(registry.spam_checkers().is_empty());
        assert!(registry.third_party_rules().is_empty());
        assert!(registry.password_providers().is_empty());
    }

    #[test]
    fn test_module_registry_default() {
        let registry = ModuleRegistry::default();
        assert!(registry.spam_checkers().is_empty());
    }

    // ========== SimpleSpamChecker tests ==========

    #[test]
    fn test_simple_spam_checker_allow() {
        let checker = SimpleSpamChecker::new("test", vec!["spam".to_string()], 1000);
        assert_eq!(checker.name(), "test");
    }

    #[test]
    fn test_simple_spam_checker_block_word() {
        let checker = SimpleSpamChecker::new("test", vec!["spam".to_string()], 1000);
        let ctx = SpamCheckContext {
            event_id: "$ev1:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": "this is spam content"}),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(checker.check(&ctx)).unwrap();
        assert_eq!(result.result.as_str(), "block");
        assert_eq!(result.score, 80);
        assert!(result.reason.unwrap().contains("spam"));
    }

    #[test]
    fn test_simple_spam_checker_allow_clean() {
        let checker = SimpleSpamChecker::new("test", vec!["badword".to_string()], 1000);
        let ctx = SpamCheckContext {
            event_id: "$ev2:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@bob:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": "hello world"}),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(checker.check(&ctx)).unwrap();
        assert_eq!(result.result.as_str(), "allow");
        assert_eq!(result.score, 0);
    }

    #[test]
    fn test_simple_spam_checker_too_long() {
        let checker = SimpleSpamChecker::new("test", vec!["spam".to_string()], 10);
        let ctx = SpamCheckContext {
            event_id: "$ev3:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@carol:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": "this is a very long message that exceeds the limit"}),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(checker.check(&ctx)).unwrap();
        assert_eq!(result.result.as_str(), "block");
        assert_eq!(result.score, 100);
        assert!(result.reason.unwrap().contains("maximum length"));
    }

    #[test]
    fn test_simple_spam_checker_case_insensitive() {
        let checker = SimpleSpamChecker::new("test", vec!["SPAM".to_string()], 1000);
        let ctx = SpamCheckContext {
            event_id: "$ev4:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": "this is spam lowercase"}),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(checker.check(&ctx)).unwrap();
        assert_eq!(result.result.as_str(), "block");
    }

    // ========== SimpleThirdPartyRule tests ==========

    #[test]
    fn test_simple_third_party_rule_name() {
        let rule = SimpleThirdPartyRule::new("my_rule", vec!["m.room.redaction".to_string()]);
        assert_eq!(rule.name(), "my_rule");
    }

    #[test]
    fn test_simple_third_party_rule_blocked_type() {
        let rule = SimpleThirdPartyRule::new("test", vec!["m.room.redaction".to_string()]);
        let ctx = ThirdPartyRuleContext {
            event_id: "$ev1:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            event_type: "m.room.redaction".to_string(),
            content: serde_json::json!({}),
            state_events: vec![],
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(rule.check(&ctx)).unwrap();
        assert!(!result.is_allowed);
    }

    #[test]
    fn test_simple_third_party_rule_allowed_type() {
        let rule = SimpleThirdPartyRule::new("test", vec!["m.room.redaction".to_string()]);
        let ctx = ThirdPartyRuleContext {
            event_id: "$ev2:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@bob:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({}),
            state_events: vec![],
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(rule.check(&ctx)).unwrap();
        assert!(result.is_allowed);
    }

    #[test]
    fn test_simple_third_party_rule_no_blocked_types() {
        let rule = SimpleThirdPartyRule::new("test", vec![]);
        let ctx = ThirdPartyRuleContext {
            event_id: "$ev3:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@carol:example.com".to_string(),
            event_type: "m.room.redaction".to_string(),
            content: serde_json::json!({}),
            state_events: vec![],
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(rule.check(&ctx)).unwrap();
        assert!(result.is_allowed);
    }

    // ========== EventAdmissionGate impl tests ==========

    /// A `ModuleService` whose storage is a lazy pool that never connects.
    /// With no rules registered both gate methods return before touching storage;
    /// once a rule *is* registered its result persistence is best-effort
    /// (`let _ =`), so the unreachable pool is safe. The acquire timeout is
    /// bounded so those best-effort writes fail fast instead of stalling the test
    /// on the pool's 30s default.
    fn module_service_without_db() -> ModuleService {
        let pool = Arc::new(
            sqlx::postgres::PgPoolOptions::new()
                .acquire_timeout(std::time::Duration::from_millis(50))
                .connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused")
                .expect("lazy pool"),
        );
        ModuleService::new(Arc::new(ModuleStorage::new(&pool)))
    }

    #[tokio::test]
    async fn event_admission_gate_reports_no_rules_until_one_is_registered() {
        let service = module_service_without_db();
        assert!(!service.has_event_rules().await, "a fresh service has no third-party rules");

        service
            .register_third_party_rule(Arc::new(SimpleThirdPartyRule::new(
                "blocker",
                vec!["m.room.message".to_string()],
            )))
            .await;
        assert!(service.has_event_rules().await, "registering a rule must be reflected by the gate");
    }

    #[tokio::test]
    async fn event_admission_gate_surfaces_a_rule_refusal() {
        let service = module_service_without_db();
        service
            .register_third_party_rule(Arc::new(SimpleThirdPartyRule::new(
                "blocker",
                vec!["m.room.message".to_string()],
            )))
            .await;

        let context = ThirdPartyRuleContext {
            event_id: "$ev:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({ "body": "hi" }),
            state_events: vec![],
        };

        let outcome = service.check_event_allowed(&context).await.expect("gate check");
        assert!(!outcome.is_allowed, "the blocking rule must refuse m.room.message");
    }

    #[tokio::test]
    async fn register_configured_third_party_rules_wires_config_into_the_gate() {
        // The production trigger path: a `third_party_rules` config entry must
        // become a registered rule, flip `has_event_rules()` on, and then refuse
        // the event types it names.
        let service = module_service_without_db();
        assert!(!service.has_event_rules().await, "no rules before the config is applied");

        let config = ThirdPartyRulesConfig {
            rules: vec![synapse_common::ThirdPartyRuleConfig {
                name: "block_redactions".to_string(),
                blocked_event_types: vec!["m.room.redaction".to_string()],
            }],
        };
        service.register_configured_third_party_rules(&config).await;

        assert!(service.has_event_rules().await, "config-registered rules must reach the gate");

        let blocked = ThirdPartyRuleContext {
            event_id: "$ev:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            event_type: "m.room.redaction".to_string(),
            content: serde_json::json!({}),
            state_events: vec![],
        };
        assert!(!service.check_event_allowed(&blocked).await.expect("blocked check").is_allowed);

        let allowed = ThirdPartyRuleContext { event_type: "m.room.message".to_string(), ..blocked };
        assert!(service.check_event_allowed(&allowed).await.expect("allowed check").is_allowed);
    }

    #[tokio::test]
    async fn register_configured_third_party_rules_leaves_gate_disabled_for_empty_config() {
        let service = module_service_without_db();
        service.register_configured_third_party_rules(&ThirdPartyRulesConfig::default()).await;
        assert!(!service.has_event_rules().await, "an empty config must not enable the gate");
    }
}
