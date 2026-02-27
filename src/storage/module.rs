use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{info, instrument};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Module {
    pub id: i32,
    pub module_name: String,
    pub module_type: String,
    pub version: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub priority: i32,
    pub config: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub last_executed_ts: Option<i64>,
    pub execution_count: i32,
    pub error_count: i32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateModuleRequest {
    pub module_name: String,
    pub module_type: String,
    pub version: String,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpamCheckResult {
    pub id: i32,
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: Option<serde_json::Value>,
    pub result: String,
    pub score: i32,
    pub reason: Option<String>,
    pub checker_module: String,
    pub checked_ts: i64,
    pub action_taken: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSpamCheckRequest {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: serde_json::Value,
    pub result: String,
    pub score: Option<i32>,
    pub reason: Option<String>,
    pub checker_module: String,
    pub action_taken: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ThirdPartyRuleResult {
    pub id: i32,
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub rule_name: String,
    pub allowed: bool,
    pub reason: Option<String>,
    pub modified_content: Option<serde_json::Value>,
    pub checked_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateThirdPartyRuleRequest {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub rule_name: String,
    pub allowed: bool,
    pub reason: Option<String>,
    pub modified_content: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ModuleExecutionLog {
    pub id: i32,
    pub module_name: String,
    pub module_type: String,
    pub event_id: Option<String>,
    pub room_id: Option<String>,
    pub execution_time_ms: i64,
    pub success: bool,
    pub error_message: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub executed_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateExecutionLogRequest {
    pub module_name: String,
    pub module_type: String,
    pub event_id: Option<String>,
    pub room_id: Option<String>,
    pub execution_time_ms: i64,
    pub success: bool,
    pub error_message: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AccountValidity {
    pub user_id: String,
    pub expiration_ts: i64,
    pub email_sent_ts: Option<i64>,
    pub renewal_token: Option<String>,
    pub renewal_token_ts: Option<i64>,
    pub is_valid: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAccountValidityRequest {
    pub user_id: String,
    pub expiration_ts: i64,
    pub is_valid: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PasswordAuthProvider {
    pub id: i32,
    pub provider_name: String,
    pub provider_type: String,
    pub config: Option<serde_json::Value>,
    pub enabled: bool,
    pub priority: i32,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePasswordAuthProviderRequest {
    pub provider_name: String,
    pub provider_type: String,
    pub config: serde_json::Value,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PresenceRoute {
    pub id: i32,
    pub route_name: String,
    pub route_type: String,
    pub config: Option<serde_json::Value>,
    pub enabled: bool,
    pub priority: i32,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePresenceRouteRequest {
    pub route_name: String,
    pub route_type: String,
    pub config: serde_json::Value,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MediaCallback {
    pub id: i32,
    pub callback_name: String,
    pub callback_type: String,
    pub url: String,
    pub method: String,
    pub headers: Option<serde_json::Value>,
    pub enabled: bool,
    pub timeout_ms: i32,
    pub retry_count: i32,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMediaCallbackRequest {
    pub callback_name: String,
    pub callback_type: String,
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<serde_json::Value>,
    pub enabled: Option<bool>,
    pub timeout_ms: Option<i32>,
    pub retry_count: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RateLimitCallback {
    pub id: i32,
    pub callback_name: String,
    pub callback_type: String,
    pub config: Option<serde_json::Value>,
    pub enabled: bool,
    pub priority: i32,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRateLimitCallbackRequest {
    pub callback_name: String,
    pub callback_type: String,
    pub config: serde_json::Value,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AccountDataCallback {
    pub id: i32,
    pub callback_name: String,
    pub callback_type: String,
    pub config: Option<serde_json::Value>,
    pub enabled: bool,
    pub priority: i32,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAccountDataCallbackRequest {
    pub callback_name: String,
    pub callback_type: String,
    pub config: serde_json::Value,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

#[derive(Clone)]
pub struct ModuleStorage {
    pool: Arc<PgPool>,
}

impl ModuleStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    #[instrument(skip(self))]
    pub async fn register_module(
        &self,
        request: CreateModuleRequest,
    ) -> Result<Module, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, Module>(
            r#"
            INSERT INTO modules (
                module_name, module_type, version, description, enabled, priority, config, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
            RETURNING *
            "#,
        )
        .bind(&request.module_name)
        .bind(&request.module_type)
        .bind(&request.version)
        .bind(&request.description)
        .bind(request.enabled.unwrap_or(true))
        .bind(request.priority.unwrap_or(100))
        .bind(&request.config)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        info!(
            "Registered module: {} ({})",
            request.module_name, request.module_type
        );
        Ok(row)
    }

    #[instrument(skip(self))]
    pub async fn get_module(&self, module_name: &str) -> Result<Option<Module>, sqlx::Error> {
        let row = sqlx::query_as::<_, Module>("SELECT * FROM modules WHERE module_name = $1")
            .bind(module_name)
            .fetch_optional(&*self.pool)
            .await?;

        Ok(row)
    }

    #[instrument(skip(self))]
    pub async fn get_modules_by_type(&self, module_type: &str) -> Result<Vec<Module>, sqlx::Error> {
        let rows = sqlx::query_as::<_, Module>(
            "SELECT * FROM modules WHERE module_type = $1 AND enabled = true ORDER BY priority ASC",
        )
        .bind(module_type)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    #[instrument(skip(self))]
    pub async fn get_all_modules(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Module>, sqlx::Error> {
        let rows = sqlx::query_as::<_, Module>(
            "SELECT * FROM modules ORDER BY module_type, priority ASC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    #[instrument(skip(self))]
    pub async fn update_module_config(
        &self,
        module_name: &str,
        config: serde_json::Value,
    ) -> Result<Module, sqlx::Error> {
        let row = sqlx::query_as::<_, Module>(
            r#"
            UPDATE modules SET config = $2
            WHERE module_name = $1
            RETURNING *
            "#,
        )
        .bind(module_name)
        .bind(&config)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    #[instrument(skip(self))]
    pub async fn enable_module(
        &self,
        module_name: &str,
        enabled: bool,
    ) -> Result<Module, sqlx::Error> {
        let row = sqlx::query_as::<_, Module>(
            r#"
            UPDATE modules SET enabled = $2
            WHERE module_name = $1
            RETURNING *
            "#,
        )
        .bind(module_name)
        .bind(enabled)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    #[instrument(skip(self))]
    pub async fn delete_module(&self, module_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM modules WHERE module_name = $1")
            .bind(module_name)
            .execute(&*self.pool)
            .await?;

        info!("Deleted module: {}", module_name);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn record_execution(
        &self,
        module_name: &str,
        success: bool,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE modules SET
                last_executed_ts = $2,
                execution_count = execution_count + 1,
                error_count = CASE WHEN $3 THEN error_count ELSE error_count + 1 END,
                last_error = $4
            WHERE module_name = $1
            "#,
        )
        .bind(module_name)
        .bind(now)
        .bind(success)
        .bind(error)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn create_spam_check_result(
        &self,
        request: CreateSpamCheckRequest,
    ) -> Result<SpamCheckResult, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, SpamCheckResult>(
            r#"
            INSERT INTO spam_check_results (
                event_id, room_id, sender, event_type, content, result, score, reason, checker_module, checked_ts, action_taken
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (event_id, checker_module) DO UPDATE SET
                result = EXCLUDED.result,
                score = EXCLUDED.score,
                reason = EXCLUDED.reason,
                checked_ts = EXCLUDED.checked_ts,
                action_taken = EXCLUDED.action_taken
            RETURNING *
            "#,
        )
        .bind(&request.event_id)
        .bind(&request.room_id)
        .bind(&request.sender)
        .bind(&request.event_type)
        .bind(&request.content)
        .bind(&request.result)
        .bind(request.score.unwrap_or(0))
        .bind(&request.reason)
        .bind(&request.checker_module)
        .bind(now)
        .bind(&request.action_taken)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    #[instrument(skip(self))]
    pub async fn get_spam_check_result(
        &self,
        event_id: &str,
    ) -> Result<Option<SpamCheckResult>, sqlx::Error> {
        let row = sqlx::query_as::<_, SpamCheckResult>(
            "SELECT * FROM spam_check_results WHERE event_id = $1 ORDER BY checked_ts DESC LIMIT 1",
        )
        .bind(event_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    #[instrument(skip(self))]
    pub async fn get_spam_check_results_by_sender(
        &self,
        sender: &str,
        limit: i64,
    ) -> Result<Vec<SpamCheckResult>, sqlx::Error> {
        let rows = sqlx::query_as::<_, SpamCheckResult>(
            "SELECT * FROM spam_check_results WHERE sender = $1 ORDER BY checked_ts DESC LIMIT $2",
        )
        .bind(sender)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    #[instrument(skip(self))]
    pub async fn create_third_party_rule_result(
        &self,
        request: CreateThirdPartyRuleRequest,
    ) -> Result<ThirdPartyRuleResult, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, ThirdPartyRuleResult>(
            r#"
            INSERT INTO third_party_rule_results (
                event_id, room_id, sender, event_type, rule_name, allowed, reason, modified_content, checked_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (event_id, rule_name) DO UPDATE SET
                allowed = EXCLUDED.allowed,
                reason = EXCLUDED.reason,
                modified_content = EXCLUDED.modified_content,
                checked_ts = EXCLUDED.checked_ts
            RETURNING *
            "#,
        )
        .bind(&request.event_id)
        .bind(&request.room_id)
        .bind(&request.sender)
        .bind(&request.event_type)
        .bind(&request.rule_name)
        .bind(request.allowed)
        .bind(&request.reason)
        .bind(&request.modified_content)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    #[instrument(skip(self))]
    pub async fn get_third_party_rule_results(
        &self,
        event_id: &str,
    ) -> Result<Vec<ThirdPartyRuleResult>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ThirdPartyRuleResult>(
            "SELECT * FROM third_party_rule_results WHERE event_id = $1 ORDER BY checked_ts DESC",
        )
        .bind(event_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    #[instrument(skip(self))]
    pub async fn create_execution_log(
        &self,
        request: CreateExecutionLogRequest,
    ) -> Result<ModuleExecutionLog, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, ModuleExecutionLog>(
            r#"
            INSERT INTO module_execution_logs (
                module_name, module_type, event_id, room_id, execution_time_ms, success, error_message, metadata, executed_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            "#,
        )
        .bind(&request.module_name)
        .bind(&request.module_type)
        .bind(&request.event_id)
        .bind(&request.room_id)
        .bind(request.execution_time_ms)
        .bind(request.success)
        .bind(&request.error_message)
        .bind(&request.metadata)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    #[instrument(skip(self))]
    pub async fn get_execution_logs(
        &self,
        module_name: &str,
        limit: i64,
    ) -> Result<Vec<ModuleExecutionLog>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ModuleExecutionLog>(
            "SELECT * FROM module_execution_logs WHERE module_name = $1 ORDER BY executed_ts DESC LIMIT $2",
        )
        .bind(module_name)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    #[instrument(skip(self))]
    pub async fn create_account_validity(
        &self,
        request: CreateAccountValidityRequest,
    ) -> Result<AccountValidity, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, AccountValidity>(
            r#"
            INSERT INTO account_validity (user_id, expiration_ts, is_valid, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $4)
            ON CONFLICT (user_id) DO UPDATE SET
                expiration_ts = EXCLUDED.expiration_ts,
                is_valid = EXCLUDED.is_valid,
                updated_ts = EXCLUDED.updated_ts
            RETURNING *
            "#,
        )
        .bind(&request.user_id)
        .bind(request.expiration_ts)
        .bind(request.is_valid.unwrap_or(true))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    #[instrument(skip(self))]
    pub async fn get_account_validity(
        &self,
        user_id: &str,
    ) -> Result<Option<AccountValidity>, sqlx::Error> {
        let row = sqlx::query_as::<_, AccountValidity>(
            "SELECT * FROM account_validity WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    #[instrument(skip(self))]
    pub async fn renew_account(
        &self,
        user_id: &str,
        renewal_token: &str,
        new_expiration_ts: i64,
    ) -> Result<AccountValidity, sqlx::Error> {
        let row = sqlx::query_as::<_, AccountValidity>(
            r#"
            UPDATE account_validity SET
                expiration_ts = $3,
                renewal_token = NULL,
                renewal_token_ts = NULL,
                is_valid = true
            WHERE user_id = $1 AND renewal_token = $2
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(renewal_token)
        .bind(new_expiration_ts)
        .fetch_optional(&*self.pool)
        .await?;

        row.ok_or_else(|| sqlx::Error::RowNotFound)
    }

    #[instrument(skip(self))]
    pub async fn set_renewal_token(&self, user_id: &str, token: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            "UPDATE account_validity SET renewal_token = $2, renewal_token_ts = $3 WHERE user_id = $1",
        )
        .bind(user_id)
        .bind(token)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_expired_accounts(
        &self,
        before_ts: i64,
    ) -> Result<Vec<AccountValidity>, sqlx::Error> {
        let rows = sqlx::query_as::<_, AccountValidity>(
            "SELECT * FROM account_validity WHERE expiration_ts < $1 AND is_valid = true",
        )
        .bind(before_ts)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    #[instrument(skip(self))]
    pub async fn create_password_auth_provider(
        &self,
        request: CreatePasswordAuthProviderRequest,
    ) -> Result<PasswordAuthProvider, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, PasswordAuthProvider>(
            r#"
            INSERT INTO password_auth_providers (
                provider_name, provider_type, config, enabled, priority, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $6)
            RETURNING *
            "#,
        )
        .bind(&request.provider_name)
        .bind(&request.provider_type)
        .bind(&request.config)
        .bind(request.enabled.unwrap_or(true))
        .bind(request.priority.unwrap_or(100))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    #[instrument(skip(self))]
    pub async fn get_password_auth_providers(
        &self,
    ) -> Result<Vec<PasswordAuthProvider>, sqlx::Error> {
        let rows = sqlx::query_as::<_, PasswordAuthProvider>(
            "SELECT * FROM password_auth_providers WHERE enabled = true ORDER BY priority ASC",
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    #[instrument(skip(self))]
    pub async fn create_presence_route(
        &self,
        request: CreatePresenceRouteRequest,
    ) -> Result<PresenceRoute, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, PresenceRoute>(
            r#"
            INSERT INTO presence_routes (
                route_name, route_type, config, enabled, priority, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $6)
            RETURNING *
            "#,
        )
        .bind(&request.route_name)
        .bind(&request.route_type)
        .bind(&request.config)
        .bind(request.enabled.unwrap_or(true))
        .bind(request.priority.unwrap_or(100))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    #[instrument(skip(self))]
    pub async fn get_presence_routes(&self) -> Result<Vec<PresenceRoute>, sqlx::Error> {
        let rows = sqlx::query_as::<_, PresenceRoute>(
            "SELECT * FROM presence_routes WHERE enabled = true ORDER BY priority ASC",
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    #[instrument(skip(self))]
    pub async fn create_media_callback(
        &self,
        request: CreateMediaCallbackRequest,
    ) -> Result<MediaCallback, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, MediaCallback>(
            r#"
            INSERT INTO media_callbacks (
                callback_name, callback_type, url, method, headers, enabled, timeout_ms, retry_count, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
            RETURNING *
            "#,
        )
        .bind(&request.callback_name)
        .bind(&request.callback_type)
        .bind(&request.url)
        .bind(request.method.unwrap_or_else(|| "POST".to_string()))
        .bind(&request.headers)
        .bind(request.enabled.unwrap_or(true))
        .bind(request.timeout_ms.unwrap_or(5000))
        .bind(request.retry_count.unwrap_or(3))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    #[instrument(skip(self))]
    pub async fn get_media_callbacks(
        &self,
        callback_type: Option<&str>,
    ) -> Result<Vec<MediaCallback>, sqlx::Error> {
        let rows = if let Some(cb_type) = callback_type {
            sqlx::query_as::<_, MediaCallback>(
                "SELECT * FROM media_callbacks WHERE enabled = true AND callback_type = $1",
            )
            .bind(cb_type)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as::<_, MediaCallback>("SELECT * FROM media_callbacks WHERE enabled = true")
                .fetch_all(&*self.pool)
                .await?
        };

        Ok(rows)
    }

    #[instrument(skip(self))]
    pub async fn create_rate_limit_callback(
        &self,
        request: CreateRateLimitCallbackRequest,
    ) -> Result<RateLimitCallback, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, RateLimitCallback>(
            r#"
            INSERT INTO rate_limit_callbacks (
                callback_name, callback_type, config, enabled, priority, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $6)
            RETURNING *
            "#,
        )
        .bind(&request.callback_name)
        .bind(&request.callback_type)
        .bind(&request.config)
        .bind(request.enabled.unwrap_or(true))
        .bind(request.priority.unwrap_or(100))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    #[instrument(skip(self))]
    pub async fn get_rate_limit_callbacks(&self) -> Result<Vec<RateLimitCallback>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RateLimitCallback>(
            "SELECT * FROM rate_limit_callbacks WHERE enabled = true ORDER BY priority ASC",
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    #[instrument(skip(self))]
    pub async fn create_account_data_callback(
        &self,
        request: CreateAccountDataCallbackRequest,
    ) -> Result<AccountDataCallback, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, AccountDataCallback>(
            r#"
            INSERT INTO account_data_callbacks (
                callback_name, callback_type, config, enabled, priority, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $6)
            RETURNING *
            "#,
        )
        .bind(&request.callback_name)
        .bind(&request.callback_type)
        .bind(&request.config)
        .bind(request.enabled.unwrap_or(true))
        .bind(request.priority.unwrap_or(100))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    #[instrument(skip(self))]
    pub async fn get_account_data_callbacks(
        &self,
    ) -> Result<Vec<AccountDataCallback>, sqlx::Error> {
        let rows = sqlx::query_as::<_, AccountDataCallback>(
            "SELECT * FROM account_data_callbacks WHERE enabled = true ORDER BY priority ASC",
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_creation() {
        let module = Module {
            id: 1,
            module_name: "test_module".to_string(),
            module_type: "spam_checker".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Test module".to_string()),
            enabled: true,
            priority: 0,
            config: Some(serde_json::json!({"key": "value"})),
            created_ts: 1234567890,
            updated_ts: 1234567890,
            error_count: 0,
            execution_count: 1,
            last_error: None,
            last_executed_ts: Some(1234567890),
        };
        assert_eq!(module.module_name, "test_module");
        assert!(module.enabled);
    }

    #[test]
    fn test_create_module_request() {
        let request = CreateModuleRequest {
            module_name: "new_module".to_string(),
            module_type: "spam_checker".to_string(),
            version: "1.0.0".to_string(),
            description: Some("New module".to_string()),
            enabled: Some(true),
            priority: Some(0),
            config: Some(serde_json::json!({"setting": true})),
        };
        assert_eq!(request.module_name, "new_module");
    }

    #[test]
    fn test_spam_check_result_creation() {
        let result = SpamCheckResult {
            id: 1,
            event_id: "$event:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: None,
            result: "allow".to_string(),
            score: 0,
            reason: None,
            checker_module: "test_module".to_string(),
            checked_ts: 1234567890,
            action_taken: None,
        };
        assert_eq!(result.result, "allow");
    }

    #[test]
    fn test_spam_check_result_ban() {
        let result = SpamCheckResult {
            id: 2,
            event_id: "$event2:example.com".to_string(),
            room_id: "!room2:example.com".to_string(),
            sender: "@bob:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: None,
            result: "ban".to_string(),
            score: -100,
            reason: Some("Spam detected".to_string()),
            checker_module: "test_module".to_string(),
            checked_ts: 1234567890,
            action_taken: Some("ban".to_string()),
        };
        assert_eq!(result.result, "ban");
    }

    #[test]
    fn test_account_validity() {
        let validity = AccountValidity {
            user_id: "@alice:example.com".to_string(),
            expiration_ts: 1234567890,
            email_sent_ts: Some(1234567890),
            renewal_token: Some("token123".to_string()),
            renewal_token_ts: Some(1234567890),
            is_valid: true,
            created_ts: 1234567800,
            updated_ts: 1234567890,
        };
        assert!(validity.is_valid);
    }

    #[test]
    fn test_password_auth_provider() {
        let provider = PasswordAuthProvider {
            id: 1,
            provider_name: "default".to_string(),
            provider_type: "password".to_string(),
            config: None,
            enabled: true,
            priority: 0,
            created_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert!(provider.enabled);
    }

    #[test]
    fn test_create_execution_log_request() {
        let request = CreateExecutionLogRequest {
            module_name: "test_module".to_string(),
            module_type: "spam_checker".to_string(),
            event_id: Some("$event:example.com".to_string()),
            room_id: Some("!room:example.com".to_string()),
            execution_time_ms: 50,
            success: true,
            error_message: None,
            metadata: None,
        };
        assert!(request.success);
    }
}
