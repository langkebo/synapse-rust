use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use tracing::{info, instrument};

fn decode_module_cursor(cursor: &str) -> Option<(&str, i32, &str)> {
    let mut parts = cursor.split('|');
    let module_type = parts.next()?;
    let priority = parts.next()?.parse::<i32>().ok()?;
    let module_name = parts.next()?;
    if module_type.is_empty() || module_name.is_empty() || parts.next().is_some() {
        return None;
    }
    Some((module_type, priority, module_name))
}

fn encode_module_cursor(module_type: &str, priority: i32, module_name: &str) -> String {
    format!("{module_type}|{priority}|{module_name}")
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_module_cursor, encode_module_cursor};

    #[test]
    fn test_module_cursor_round_trip() {
        let cursor = encode_module_cursor("spam_checker", 10, "basic-module");
        assert_eq!(decode_module_cursor(&cursor), Some(("spam_checker", 10, "basic-module")));
    }

    #[test]
    fn test_module_cursor_rejects_invalid_value() {
        assert_eq!(decode_module_cursor("bad-cursor"), None);
        assert_eq!(decode_module_cursor("type|x|name"), None);
        assert_eq!(decode_module_cursor("type|1|"), None);
    }
}

/// The `Module` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Module {
    /// The `id` field.
    pub id: i64,
    /// The `module_name` field.
    pub module_name: String,
    /// The `module_type` field.
    pub module_type: String,
    /// The `version` field.
    pub version: String,
    /// The `description` field.
    pub description: Option<String>,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `priority` field.
    pub priority: i32,
    /// The `config` field.
    pub config: Option<serde_json::Value>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
    /// The `last_executed_ts` field.
    pub last_executed_ts: Option<i64>,
    /// The `execution_count` field.
    pub execution_count: i32,
    /// The `error_count` field.
    pub error_count: i32,
    /// The `last_error` field.
    pub last_error: Option<String>,
}

/// The `CreateModuleRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateModuleRequest {
    /// The `module_name` field.
    pub module_name: String,
    /// The `module_type` field.
    pub module_type: String,
    /// The `version` field.
    pub version: String,
    /// The `description` field.
    pub description: Option<String>,
    /// The `is_enabled` field.
    pub is_enabled: Option<bool>,
    /// The `priority` field.
    pub priority: Option<i32>,
    /// The `config` field.
    pub config: Option<serde_json::Value>,
}

/// The `SpamCheckResult` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpamCheckResult {
    /// The `id` field.
    pub id: i64,
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `content` field.
    pub content: Option<serde_json::Value>,
    /// The `result` field.
    pub result: String,
    /// The `score` field.
    pub score: i32,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `checker_module` field.
    pub checker_module: String,
    /// The `checked_ts` field.
    pub checked_ts: i64,
    /// The `action_taken` field.
    pub action_taken: Option<String>,
}

/// The `CreateSpamCheckRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSpamCheckRequest {
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
    /// The `result` field.
    pub result: String,
    /// The `score` field.
    pub score: Option<i32>,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `checker_module` field.
    pub checker_module: String,
    /// The `action_taken` field.
    pub action_taken: Option<String>,
}

/// The `ThirdPartyRuleResult` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ThirdPartyRuleResult {
    /// The `id` field.
    pub id: i64,
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `rule_name` field.
    pub rule_name: String,
    #[serde(rename = "allowed")]
    #[sqlx(rename = "is_allowed")]
    /// The `is_allowed` field.
    pub is_allowed: bool,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `modified_content` field.
    pub modified_content: Option<serde_json::Value>,
    /// The `checked_ts` field.
    pub checked_ts: i64,
}

/// The `CreateThirdPartyRuleRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateThirdPartyRuleRequest {
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `rule_name` field.
    pub rule_name: String,
    #[serde(rename = "allowed")]
    /// The `is_allowed` field.
    pub is_allowed: bool,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `modified_content` field.
    pub modified_content: Option<serde_json::Value>,
}

/// The `ModuleExecutionLog` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ModuleExecutionLog {
    /// The `id` field.
    pub id: i64,
    /// The `module_name` field.
    pub module_name: String,
    /// The `module_type` field.
    pub module_type: String,
    /// The `event_id` field.
    pub event_id: Option<String>,
    /// The `room_id` field.
    pub room_id: Option<String>,
    /// The `execution_time_ms` field.
    pub execution_time_ms: Option<i64>,
    /// The `is_success` field.
    pub is_success: bool,
    /// The `error_message` field.
    pub error_message: Option<String>,
    /// The `metadata` field.
    pub metadata: Option<serde_json::Value>,
    /// The `executed_ts` field.
    pub executed_ts: i64,
}

/// The `CreateExecutionLogRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateExecutionLogRequest {
    /// The `module_name` field.
    pub module_name: String,
    /// The `module_type` field.
    pub module_type: String,
    /// The `event_id` field.
    pub event_id: Option<String>,
    /// The `room_id` field.
    pub room_id: Option<String>,
    /// The `execution_time_ms` field.
    pub execution_time_ms: i64,
    /// The `is_success` field.
    pub is_success: bool,
    /// The `error_message` field.
    pub error_message: Option<String>,
    /// The `metadata` field.
    pub metadata: Option<serde_json::Value>,
}

/// The `AccountValidity` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AccountValidity {
    /// The `user_id` field.
    pub user_id: String,
    /// The `expiration_at` field.
    pub expiration_at: Option<i64>,
    /// The `last_check_at` field.
    pub last_check_at: Option<i64>,
    /// The `renewal_token` field.
    pub renewal_token: Option<String>,
    /// 内存中的临时状态，不持久化到数据库。记录 renewal_token 的生成时间，
    /// 用于判断 token 是否过期，服务重启后会丢失。
    #[sqlx(skip)]
    /// The `renewal_token_ts` field.
    pub renewal_token_ts: Option<i64>,
    /// The `is_valid` field.
    pub is_valid: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `CreateAccountValidityRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAccountValidityRequest {
    /// The `user_id` field.
    pub user_id: String,
    /// The `expiration_at` field.
    pub expiration_at: i64,
    /// The `is_valid` field.
    pub is_valid: Option<bool>,
}

/// The `PasswordAuthProvider` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PasswordAuthProvider {
    /// The `id` field.
    pub id: i64,
    /// The `provider_name` field.
    pub provider_name: String,
    /// The `provider_type` field.
    pub provider_type: String,
    /// The `config` field.
    pub config: Option<serde_json::Value>,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `priority` field.
    pub priority: i32,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `CreatePasswordAuthProviderRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePasswordAuthProviderRequest {
    /// The `provider_name` field.
    pub provider_name: String,
    /// The `provider_type` field.
    pub provider_type: String,
    /// The `config` field.
    pub config: serde_json::Value,
    /// The `is_enabled` field.
    pub is_enabled: Option<bool>,
    /// The `priority` field.
    pub priority: Option<i32>,
}

/// The `MediaCallback` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MediaCallback {
    /// The `id` field.
    pub id: i64,
    /// The `callback_type` field.
    pub callback_type: String,
    /// The `media_id` field.
    pub media_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `status` field.
    pub status: String,
    /// The `result` field.
    pub result: Option<serde_json::Value>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `completed_ts` field.
    pub completed_ts: Option<i64>,
    /// The `is_enabled` field.
    pub is_enabled: bool,
}

/// The `CreateMediaCallbackRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMediaCallbackRequest {
    /// The `callback_name` field.
    pub callback_name: String,
    /// The `callback_type` field.
    pub callback_type: String,
    /// The `url` field.
    pub url: String,
    /// The `method` field.
    pub method: Option<String>,
    /// The `headers` field.
    pub headers: Option<serde_json::Value>,
    /// The `is_enabled` field.
    pub is_enabled: Option<bool>,
    /// The `timeout_ms` field.
    pub timeout_ms: Option<i32>,
    /// The `retry_count` field.
    pub retry_count: Option<i32>,
}

/// The `AccountDataCallback` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AccountDataCallback {
    /// The `id` field.
    pub id: i64,
    /// The `callback_name` field.
    pub callback_name: String,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `data_types` field.
    pub data_types: Option<Vec<String>>,
    /// The `config` field.
    pub config: Option<serde_json::Value>,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `CreateAccountDataCallbackRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAccountDataCallbackRequest {
    /// The `callback_name` field.
    pub callback_name: String,
    /// The `config` field.
    pub config: serde_json::Value,
    /// The `is_enabled` field.
    pub is_enabled: Option<bool>,
    /// The `data_types` field.
    pub data_types: Option<Vec<String>>,
}

/// The `ModuleStorage` struct.
#[derive(Clone)]
pub struct ModuleStorage {
    pool: Arc<PgPool>,
}

impl ModuleStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`register_module`].
    #[instrument(skip(self))]
    pub async fn register_module(&self, request: CreateModuleRequest) -> Result<Module, sqlx::Error> {
        let now = current_timestamp_millis();

        let row = sqlx::query_as::<_, Module>(
            r"
            INSERT INTO modules (
                module_name, module_type, version, description, is_enabled, priority, config, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
            RETURNING *
            ",
        )
        .bind(&request.module_name)
        .bind(&request.module_type)
        .bind(&request.version)
        .bind(&request.description)
        .bind(request.is_enabled.unwrap_or(true))
        .bind(request.priority.unwrap_or(100))
        .bind(&request.config)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        info!("Registered module: {} ({})", request.module_name, request.module_type);
        Ok(row)
    }

    /// See [`get_module`].
    #[instrument(skip(self))]
    pub async fn get_module(&self, module_name: &str) -> Result<Option<Module>, sqlx::Error> {
        let row = sqlx::query_as::<_, Module>("SELECT id, module_name, module_type, version, description, is_enabled, priority, config, created_ts, updated_ts, last_executed_ts, execution_count, error_count, last_error FROM modules WHERE module_name = $1")
            .bind(module_name)
            .fetch_optional(&*self.pool)
            .await?;

        Ok(row)
    }

    /// See [`get_modules_by_type`].
    #[instrument(skip(self))]
    pub async fn get_modules_by_type(&self, module_type: &str) -> Result<Vec<Module>, sqlx::Error> {
        let rows = sqlx::query_as::<_, Module>(
            "SELECT id, module_name, module_type, version, description, is_enabled, priority, config, created_ts, updated_ts, last_executed_ts, execution_count, error_count, last_error FROM modules WHERE module_type = $1 AND is_enabled = true ORDER BY priority ASC",
        )
        .bind(module_type)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`get_all_modules`].
    #[instrument(skip(self))]
    pub async fn get_all_modules(
        &self,
        limit: i64,
        from: Option<String>,
    ) -> Result<(Vec<Module>, Option<String>), sqlx::Error> {
        let decoded = from.as_deref().and_then(decode_module_cursor);
        let cursor_module_type = decoded.map(|(module_type, _, _)| module_type);
        let cursor_priority = decoded.map(|(_, priority, _)| priority);
        let cursor_module_name = decoded.map(|(_, _, module_name)| module_name);
        let rows = sqlx::query_as::<_, Module>(
            "SELECT id, module_name, module_type, version, description, is_enabled, priority, config, created_ts, updated_ts, last_executed_ts, execution_count, error_count, last_error FROM modules
             WHERE ($2::TEXT IS NULL AND $3::INT4 IS NULL AND $4::TEXT IS NULL)
                OR module_type > $2
                OR (module_type = $2 AND priority > $3)
                OR (module_type = $2 AND priority = $3 AND module_name > $4)
             ORDER BY module_type ASC, priority ASC, module_name ASC
             LIMIT $1",
        )
        .bind(limit)
        .bind(cursor_module_type)
        .bind(cursor_priority)
        .bind(cursor_module_name)
        .fetch_all(&*self.pool)
        .await?;

        let next_from = if rows.len() as i64 == limit {
            rows.last().map(|row| encode_module_cursor(&row.module_type, row.priority, &row.module_name))
        } else {
            None
        };

        Ok((rows, next_from))
    }

    /// See [`update_module_config`].
    #[instrument(skip(self))]
    pub async fn update_module_config(
        &self,
        module_name: &str,
        config: serde_json::Value,
    ) -> Result<Module, sqlx::Error> {
        let row = sqlx::query_as::<_, Module>(
            r"
            UPDATE modules SET config = $2
            WHERE module_name = $1
            RETURNING *
            ",
        )
        .bind(module_name)
        .bind(&config)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`enable_module`].
    #[instrument(skip(self))]
    pub async fn enable_module(&self, module_name: &str, is_enabled: bool) -> Result<Module, sqlx::Error> {
        let row = sqlx::query_as::<_, Module>(
            r"
            UPDATE modules SET is_enabled = $2
            WHERE module_name = $1
            RETURNING *
            ",
        )
        .bind(module_name)
        .bind(is_enabled)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`delete_module`].
    #[instrument(skip(self))]
    pub async fn delete_module(&self, module_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM modules WHERE module_name = $1").bind(module_name).execute(&*self.pool).await?;

        info!("Deleted module: {}", module_name);
        Ok(())
    }

    /// See [`record_execution`].
    #[instrument(skip(self))]
    pub async fn record_execution(
        &self,
        module_name: &str,
        success: bool,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query(
            r"
            UPDATE modules SET
                last_executed_ts = $2,
                execution_count = execution_count + 1,
                error_count = CASE WHEN $3 THEN error_count ELSE error_count + 1 END,
                last_error = $4
            WHERE module_name = $1
            ",
        )
        .bind(module_name)
        .bind(now)
        .bind(success)
        .bind(error)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`create_spam_check_result`].
    #[instrument(skip(self))]
    pub async fn create_spam_check_result(
        &self,
        request: CreateSpamCheckRequest,
    ) -> Result<SpamCheckResult, sqlx::Error> {
        let now = current_timestamp_millis();
        let score = request.score.unwrap_or(0);

        let row = sqlx::query_as::<_, SpamCheckResult>(
            r"
            INSERT INTO spam_check_results (
                event_id, room_id, sender, event_type, content, result, score,
                reason, checker_module, checked_ts, action_taken, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $10)
            RETURNING id, event_id, room_id, sender, event_type, content, result, score,
                reason, checker_module, checked_ts, action_taken
            ",
        )
        .bind(&request.event_id)
        .bind(&request.room_id)
        .bind(&request.sender)
        .bind(&request.event_type)
        .bind(&request.content)
        .bind(&request.result)
        .bind(score)
        .bind(&request.reason)
        .bind(&request.checker_module)
        .bind(now)
        .bind(&request.action_taken)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`get_spam_check_result`].
    #[instrument(skip(self))]
    pub async fn get_spam_check_result(&self, event_id: &str) -> Result<Option<SpamCheckResult>, sqlx::Error> {
        sqlx::query_as::<_, SpamCheckResult>(
            r"
            SELECT id, event_id, room_id, sender, event_type, content, result, score,
                reason, checker_module, checked_ts, action_taken
            FROM spam_check_results
            WHERE event_id = $1
            ORDER BY checked_ts DESC, id DESC
            LIMIT 1
            ",
        )
        .bind(event_id)
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`get_spam_check_results_by_sender`].
    #[instrument(skip(self))]
    pub async fn get_spam_check_results_by_sender(
        &self,
        sender: &str,
        limit: i64,
    ) -> Result<Vec<SpamCheckResult>, sqlx::Error> {
        sqlx::query_as::<_, SpamCheckResult>(
            r"
            SELECT id, event_id, room_id, sender, event_type, content, result, score,
                reason, checker_module, checked_ts, action_taken
            FROM spam_check_results
            WHERE sender = $1
            ORDER BY checked_ts DESC, id DESC
            LIMIT $2
            ",
        )
        .bind(sender)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`create_third_party_rule_result`].
    #[instrument(skip(self))]
    pub async fn create_third_party_rule_result(
        &self,
        request: CreateThirdPartyRuleRequest,
    ) -> Result<ThirdPartyRuleResult, sqlx::Error> {
        let now = current_timestamp_millis();

        let row = sqlx::query_as::<_, ThirdPartyRuleResult>(
            r"
            INSERT INTO third_party_rule_results (
                event_id, room_id, sender, event_type, rule_name,
                is_allowed, reason, modified_content, checked_ts, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
            RETURNING id, event_id, room_id, sender, event_type, rule_name,
                is_allowed, reason, modified_content, checked_ts
            ",
        )
        .bind(&request.event_id)
        .bind(&request.room_id)
        .bind(&request.sender)
        .bind(&request.event_type)
        .bind(&request.rule_name)
        .bind(request.is_allowed)
        .bind(&request.reason)
        .bind(&request.modified_content)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`get_third_party_rule_results`].
    #[instrument(skip(self))]
    pub async fn get_third_party_rule_results(&self, event_id: &str) -> Result<Vec<ThirdPartyRuleResult>, sqlx::Error> {
        sqlx::query_as::<_, ThirdPartyRuleResult>(
            r"
            SELECT id, event_id, room_id, sender, event_type, rule_name,
                is_allowed, reason, modified_content, checked_ts
            FROM third_party_rule_results
            WHERE event_id = $1
            ORDER BY checked_ts DESC, id DESC
            ",
        )
        .bind(event_id)
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`create_execution_log`].
    #[instrument(skip(self))]
    pub async fn create_execution_log(
        &self,
        request: CreateExecutionLogRequest,
    ) -> Result<ModuleExecutionLog, sqlx::Error> {
        let now = current_timestamp_millis();

        let row = sqlx::query_as::<_, ModuleExecutionLog>(
            r"
            INSERT INTO module_execution_logs (
                module_name, module_type, event_id, room_id, execution_time_ms, is_success, error_message, metadata, executed_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            ",
        )
        .bind(&request.module_name)
        .bind(&request.module_type)
        .bind(&request.event_id)
        .bind(&request.room_id)
        .bind(request.execution_time_ms)
        .bind(request.is_success)
        .bind(&request.error_message)
        .bind(&request.metadata)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`get_execution_logs`].
    #[instrument(skip(self))]
    pub async fn get_execution_logs(
        &self,
        module_name: &str,
        limit: i64,
    ) -> Result<Vec<ModuleExecutionLog>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ModuleExecutionLog>(
            r"
            SELECT id, module_name, module_type, event_id, room_id, execution_time_ms,
                is_success, error_message, metadata, executed_ts
            FROM module_execution_logs WHERE module_name = $1 ORDER BY executed_ts DESC LIMIT $2
            ",
        )
        .bind(module_name)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`create_account_validity`].
    #[instrument(skip(self))]
    pub async fn create_account_validity(
        &self,
        request: CreateAccountValidityRequest,
    ) -> Result<AccountValidity, sqlx::Error> {
        let now = current_timestamp_millis();

        let row = sqlx::query_as::<_, AccountValidity>(
            r"
            INSERT INTO account_validity (user_id, expiration_at, is_valid, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $4)
            ON CONFLICT (user_id) DO UPDATE SET
                expiration_at = EXCLUDED.expiration_at,
                is_valid = EXCLUDED.is_valid,
                updated_ts = EXCLUDED.updated_ts
            RETURNING
                user_id,
                expiration_at,
                last_check_at,
                renewal_token,
                is_valid,
                created_ts,
                COALESCE(updated_ts, created_ts) AS updated_ts
            ",
        )
        .bind(&request.user_id)
        .bind(request.expiration_at)
        .bind(request.is_valid.unwrap_or(true))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`get_account_validity`].
    #[instrument(skip(self))]
    pub async fn get_account_validity(&self, user_id: &str) -> Result<Option<AccountValidity>, sqlx::Error> {
        let row = sqlx::query_as::<_, AccountValidity>(
            r"
            SELECT
                user_id,
                expiration_at,
                last_check_at,
                renewal_token,
                is_valid,
                created_ts,
                COALESCE(updated_ts, created_ts) AS updated_ts
            FROM account_validity
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`renew_account`].
    #[instrument(skip(self))]
    pub async fn renew_account(
        &self,
        user_id: &str,
        renewal_token: &str,
        new_expiration_at: i64,
    ) -> Result<AccountValidity, sqlx::Error> {
        let row = sqlx::query_as::<_, AccountValidity>(
            r"
            UPDATE account_validity SET
                expiration_at = $3,
                renewal_token = NULL,
                is_valid = true
            WHERE user_id = $1 AND renewal_token = $2
            RETURNING
                user_id,
                expiration_at,
                last_check_at,
                renewal_token,
                is_valid,
                created_ts,
                COALESCE(updated_ts, created_ts) AS updated_ts
            ",
        )
        .bind(user_id)
        .bind(renewal_token)
        .bind(new_expiration_at)
        .fetch_optional(&*self.pool)
        .await?;

        row.ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// See [`set_renewal_token`].
    #[instrument(skip(self))]
    pub async fn set_renewal_token(&self, user_id: &str, token: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE account_validity SET renewal_token = $2 WHERE user_id = $1")
            .bind(user_id)
            .bind(token)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    /// See [`get_expired_accounts`].
    #[instrument(skip(self))]
    pub async fn get_expired_accounts(&self, before_ts: i64) -> Result<Vec<AccountValidity>, sqlx::Error> {
        let rows = sqlx::query_as::<_, AccountValidity>(
            r"
            SELECT
                user_id,
                expiration_at,
                last_check_at,
                renewal_token,
                is_valid,
                created_ts,
                COALESCE(updated_ts, created_ts) AS updated_ts
            FROM account_validity
            WHERE expiration_at < $1 AND is_valid = true
            ",
        )
        .bind(before_ts)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`create_password_auth_provider`].
    #[instrument(skip(self))]
    pub async fn create_password_auth_provider(
        &self,
        _request: CreatePasswordAuthProviderRequest,
    ) -> Result<PasswordAuthProvider, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }

    /// See [`get_password_auth_providers`].
    #[instrument(skip(self))]
    pub async fn get_password_auth_providers(&self) -> Result<Vec<PasswordAuthProvider>, sqlx::Error> {
        Ok(vec![])
    }

    /// See [`create_media_callback`].
    #[instrument(skip(self))]
    pub async fn create_media_callback(
        &self,
        request: CreateMediaCallbackRequest,
    ) -> Result<MediaCallback, sqlx::Error> {
        let now = current_timestamp_millis();

        let row = sqlx::query_as::<_, MediaCallback>(
            r"
            INSERT INTO media_callbacks (
                callback_name, callback_type, url, method, headers, is_enabled, timeout_ms, retry_count, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
            RETURNING *
            ",
        )
        .bind(&request.callback_name)
        .bind(&request.callback_type)
        .bind(&request.url)
        .bind(request.method.unwrap_or_else(|| "POST".to_string()))
        .bind(&request.headers)
        .bind(request.is_enabled.unwrap_or(true))
        .bind(request.timeout_ms.unwrap_or(5000))
        .bind(request.retry_count.unwrap_or(3))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`get_media_callbacks`].
    #[instrument(skip(self))]
    pub async fn get_media_callbacks(&self, callback_type: Option<&str>) -> Result<Vec<MediaCallback>, sqlx::Error> {
        let rows = if let Some(cb_type) = callback_type {
            sqlx::query_as::<_, MediaCallback>(
                "SELECT id, callback_type, media_id, user_id, status, result, created_ts, completed_ts, is_enabled FROM media_callbacks WHERE is_enabled = true AND callback_type = $1",
            )
            .bind(cb_type)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as::<_, MediaCallback>("SELECT id, callback_type, media_id, user_id, status, result, created_ts, completed_ts, is_enabled FROM media_callbacks WHERE is_enabled = true")
                .fetch_all(&*self.pool)
                .await?
        };

        Ok(rows)
    }

    /// See [`create_account_data_callback`].
    #[instrument(skip(self))]
    pub async fn create_account_data_callback(
        &self,
        request: CreateAccountDataCallbackRequest,
    ) -> Result<AccountDataCallback, sqlx::Error> {
        let now = current_timestamp_millis();

        let row = sqlx::query_as::<_, AccountDataCallback>(
            r"
            INSERT INTO account_data_callbacks (
                callback_name, config, is_enabled, data_types, created_ts
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, callback_name, is_enabled, data_types, config, created_ts
            ",
        )
        .bind(&request.callback_name)
        .bind(&request.config)
        .bind(request.is_enabled.unwrap_or(true))
        .bind(&request.data_types)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`get_account_data_callbacks`].
    #[instrument(skip(self))]
    pub async fn get_account_data_callbacks(&self) -> Result<Vec<AccountDataCallback>, sqlx::Error> {
        let rows = sqlx::query_as::<_, AccountDataCallback>(
            "SELECT id, callback_name, is_enabled, data_types, config, created_ts FROM account_data_callbacks WHERE is_enabled = true ORDER BY created_ts DESC, id DESC",
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
            is_enabled: true,
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
        assert!(module.is_enabled);
    }

    #[test]
    fn test_create_module_request() {
        let request = CreateModuleRequest {
            module_name: "new_module".to_string(),
            module_type: "spam_checker".to_string(),
            version: "1.0.0".to_string(),
            description: Some("New module".to_string()),
            is_enabled: Some(true),
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
            expiration_at: Some(1234567890),
            last_check_at: Some(1234567890),
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
            is_enabled: true,
            priority: 0,
            created_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert!(provider.is_enabled);
    }

    #[test]
    fn test_create_execution_log_request() {
        let request = CreateExecutionLogRequest {
            module_name: "test_module".to_string(),
            module_type: "spam_checker".to_string(),
            event_id: Some("$event:example.com".to_string()),
            room_id: Some("!room:example.com".to_string()),
            execution_time_ms: 50,
            is_success: true,
            error_message: None,
            metadata: None,
        };
        assert!(request.is_success);
    }
}
