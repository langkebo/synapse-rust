use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

// Moderation domain group — re-exports invite_blocklist types under `moderation::`.
// Consumers should prefer `synapse_storage::moderation::InviteBlocklistStorage`
// over the flat `synapse_storage::InviteBlocklistStorage`.
pub use crate::invite_blocklist::{InviteBlocklistStorage, InviteBlocklistStoreApi};

/// The `ModerationRule` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ModerationRule {
    /// The `id` field.
    pub id: i64,
    /// The `rule_id` field.
    pub rule_id: String,
    /// The `server_id` field.
    pub server_id: Option<String>,
    /// The `rule_type` field.
    pub rule_type: String,
    /// The `pattern` field.
    pub pattern: String,
    /// The `action` field.
    pub action: String,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `created_by` field.
    pub created_by: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
    /// The `is_active` field.
    pub is_active: bool,
    /// The `priority` field.
    pub priority: i32,
}

/// The `CreateModerationRuleParams` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateModerationRuleParams {
    /// The `rule_type` field.
    pub rule_type: ModerationRuleType,
    /// The `pattern` field.
    pub pattern: String,
    /// The `action` field.
    pub action: ModerationAction,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `created_by` field.
    pub created_by: String,
    /// The `server_id` field.
    pub server_id: Option<String>,
    /// The `priority` field.
    pub priority: Option<i32>,
}

/// The `ModerationRuleType` enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModerationRuleType {
    #[serde(rename = "regex")]
    /// The `Regex` variant.
    Regex,
    #[serde(rename = "keyword")]
    /// The `Keyword` variant.
    Keyword,
    #[serde(rename = "domain")]
    /// The `Domain` variant.
    Domain,
    #[serde(rename = "user")]
    /// The `User` variant.
    User,
    #[serde(rename = "room")]
    /// The `Room` variant.
    Room,
    #[serde(rename = "media_hash")]
    /// The `MediaHash` variant.
    MediaHash,
}

impl ModerationRuleType {
    /// See [`as_str`].
    /// See [`as_str`].
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Regex => "regex",
            Self::Keyword => "keyword",
            Self::Domain => "domain",
            Self::User => "user",
            Self::Room => "room",
            Self::MediaHash => "media_hash",
        }
    }
}

/// The `ModerationAction` enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModerationAction {
    #[serde(rename = "block")]
    /// The `Block` variant.
    Block,
    #[serde(rename = "redact")]
    /// The `Redact` variant.
    Redact,
    #[serde(rename = "flag")]
    /// The `Flag` variant.
    Flag,
    #[serde(rename = "quarantine")]
    /// The `Quarantine` variant.
    Quarantine,
    #[serde(rename = "notify")]
    /// The `Notify` variant.
    Notify,
}

impl ModerationAction {
    /// See [`as_str`].
    /// See [`as_str`].
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Redact => "redact",
            Self::Flag => "flag",
            Self::Quarantine => "quarantine",
            Self::Notify => "notify",
        }
    }
}

/// The `ContentScanResult` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentScanResult {
    /// The `is_violation` field.
    pub is_violation: bool,
    /// The `matched_rules` field.
    pub matched_rules: Vec<MatchedRule>,
    /// The `action` field.
    pub action: Option<ModerationAction>,
    /// The `confidence` field.
    pub confidence: f32,
    /// The `scan_duration_ms` field.
    pub scan_duration_ms: u64,
}

/// The `MatchedRule` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedRule {
    /// The `rule_id` field.
    pub rule_id: String,
    /// The `rule_type` field.
    pub rule_type: String,
    /// The `pattern` field.
    pub pattern: String,
    /// The `matched_text` field.
    pub matched_text: String,
    /// The `confidence` field.
    pub confidence: f32,
}

/// The `ScanContentRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanContentRequest {
    /// The `content` field.
    pub content: String,
    /// The `content_type` field.
    pub content_type: ContentType,
    /// The `sender` field.
    pub sender: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `event_id` field.
    pub event_id: String,
}

/// The `ContentType` enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentType {
    #[serde(rename = "text")]
    /// The `Text` variant.
    Text,
    #[serde(rename = "image")]
    /// The `Image` variant.
    Image,
    #[serde(rename = "video")]
    /// The `Video` variant.
    Video,
    #[serde(rename = "audio")]
    /// The `Audio` variant.
    Audio,
    #[serde(rename = "file")]
    /// The `File` variant.
    File,
}

/// Store API for moderation rules.
#[async_trait]
pub trait ModerationStoreApi: Send + Sync {
    /// See [`create_rule`].
    async fn create_rule(&self, params: CreateModerationRuleParams) -> Result<ModerationRule, sqlx::Error>;
    /// See [`get_rule`].
    async fn get_rule(&self, rule_id: &str) -> Result<Option<ModerationRule>, sqlx::Error>;
    /// See [`get_all_rules`].
    async fn get_all_rules(&self) -> Result<Vec<ModerationRule>, sqlx::Error>;
    /// See [`get_rules_by_type`].
    async fn get_rules_by_type(&self, rule_type: &str) -> Result<Vec<ModerationRule>, sqlx::Error>;
    /// See [`update_rule`].
    async fn update_rule(
        &self,
        rule_id: &str,
        pattern: Option<&str>,
        action: Option<&str>,
        reason: Option<&str>,
        priority: Option<i32>,
    ) -> Result<ModerationRule, sqlx::Error>;
    /// See [`delete_rule`].
    async fn delete_rule(&self, rule_id: &str) -> Result<bool, sqlx::Error>;
}

/// The `ModerationStorage` struct.
#[derive(Clone)]
pub struct ModerationStorage {
    pool: Arc<Pool<Postgres>>,
}

impl ModerationStorage {
    /// See [`new`].
    /// See [`new`].
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    /// See [`create_rule`].
    /// See [`create_rule`].
    pub async fn create_rule(&self, params: CreateModerationRuleParams) -> Result<ModerationRule, sqlx::Error> {
        let now = current_timestamp_millis();
        let rule_id = format!("mod_{}", uuid::Uuid::new_v4().simple());

        sqlx::query_as::<_, ModerationRule>(
            r"
            INSERT INTO moderation_rules
                (rule_id, server_id, rule_type, pattern, action, reason, created_by, created_ts, updated_ts, is_active, priority)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8, true, $9)
            RETURNING *
            ",
        )
        .bind(&rule_id)
        .bind(&params.server_id)
        .bind(params.rule_type.as_str())
        .bind(&params.pattern)
        .bind(params.action.as_str())
        .bind(&params.reason)
        .bind(&params.created_by)
        .bind(now)
        .bind(params.priority.unwrap_or(100))
        .fetch_one(&*self.pool)
        .await
    }

    /// See [`get_rule`].
    /// See [`get_rule`].
    pub async fn get_rule(&self, rule_id: &str) -> Result<Option<ModerationRule>, sqlx::Error> {
        sqlx::query_as::<_, ModerationRule>(
            r"
            SELECT id, rule_id, server_id, rule_type, pattern, action, reason, created_by, created_ts, updated_ts, is_active, priority FROM moderation_rules WHERE rule_id = $1 AND is_active = true
            ",
        )
        .bind(rule_id)
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`get_all_rules`].
    /// See [`get_all_rules`].
    pub async fn get_all_rules(&self) -> Result<Vec<ModerationRule>, sqlx::Error> {
        sqlx::query_as::<_, ModerationRule>(
            r"
            SELECT id, rule_id, server_id, rule_type, pattern, action, reason, created_by, created_ts, updated_ts, is_active, priority FROM moderation_rules
            WHERE is_active = true
            ORDER BY priority DESC, created_ts ASC
            ",
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_rules_by_type`].
    /// See [`get_rules_by_type`].
    pub async fn get_rules_by_type(&self, rule_type: &str) -> Result<Vec<ModerationRule>, sqlx::Error> {
        sqlx::query_as::<_, ModerationRule>(
            r"
            SELECT id, rule_id, server_id, rule_type, pattern, action, reason, created_by, created_ts, updated_ts, is_active, priority FROM moderation_rules
            WHERE rule_type = $1 AND is_active = true
            ORDER BY priority DESC, created_ts ASC
            ",
        )
        .bind(rule_type)
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`update_rule`].
    pub async fn update_rule(
        &self,
        rule_id: &str,
        pattern: Option<&str>,
        action: Option<&str>,
        reason: Option<&str>,
        priority: Option<i32>,
    ) -> Result<ModerationRule, sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query_as::<_, ModerationRule>(
            r"
            UPDATE moderation_rules
            SET
                pattern = COALESCE($2, pattern),
                action = COALESCE($3, action),
                reason = COALESCE($4, reason),
                priority = COALESCE($5, priority),
                updated_ts = $6
            WHERE rule_id = $1
            RETURNING *
            ",
        )
        .bind(rule_id)
        .bind(pattern)
        .bind(action)
        .bind(reason)
        .bind(priority)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    /// See [`delete_rule`].
    /// See [`delete_rule`].
    pub async fn delete_rule(&self, rule_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r"
            UPDATE moderation_rules
            SET is_active = false
            WHERE rule_id = $1 AND is_active = TRUE
            ",
        )
        .bind(rule_id)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl ModerationStoreApi for ModerationStorage {
    async fn create_rule(&self, params: CreateModerationRuleParams) -> Result<ModerationRule, sqlx::Error> {
        self.create_rule(params).await
    }

    async fn get_rule(&self, rule_id: &str) -> Result<Option<ModerationRule>, sqlx::Error> {
        self.get_rule(rule_id).await
    }

    async fn get_all_rules(&self) -> Result<Vec<ModerationRule>, sqlx::Error> {
        self.get_all_rules().await
    }

    async fn get_rules_by_type(&self, rule_type: &str) -> Result<Vec<ModerationRule>, sqlx::Error> {
        self.get_rules_by_type(rule_type).await
    }

    async fn update_rule(
        &self,
        rule_id: &str,
        pattern: Option<&str>,
        action: Option<&str>,
        reason: Option<&str>,
        priority: Option<i32>,
    ) -> Result<ModerationRule, sqlx::Error> {
        self.update_rule(rule_id, pattern, action, reason, priority).await
    }

    async fn delete_rule(&self, rule_id: &str) -> Result<bool, sqlx::Error> {
        self.delete_rule(rule_id).await
    }
}

/// The `ModerationLog` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ModerationLog {
    /// The `id` field.
    pub id: i64,
    /// The `rule_id` field.
    pub rule_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `content_hash` field.
    pub content_hash: String,
    /// The `action_taken` field.
    pub action_taken: String,
    /// The `confidence` field.
    pub confidence: f32,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// Store API for moderation action logs.
#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait ModerationLogStoreApi: Send + Sync {
    /// See [`log_action`].
    async fn log_action(
        &self,
        rule_id: &str,
        event_id: &str,
        room_id: &str,
        sender: &str,
        content_hash: &str,
        action_taken: &str,
        confidence: f32,
    ) -> Result<(), sqlx::Error>;
    /// See [`get_logs_for_event`].
    async fn get_logs_for_event(&self, event_id: &str) -> Result<Vec<ModerationLog>, sqlx::Error>;
    /// See [`get_logs_for_room`].
    async fn get_logs_for_room(&self, room_id: &str, limit: i32) -> Result<Vec<ModerationLog>, sqlx::Error>;
    /// See [`get_logs_for_sender`].
    async fn get_logs_for_sender(&self, sender: &str, limit: i32) -> Result<Vec<ModerationLog>, sqlx::Error>;
    /// See [`cleanup_old_logs`].
    async fn cleanup_old_logs(&self, older_than_days: i32) -> Result<u64, sqlx::Error>;
}

/// The `ModerationLogStorage` struct.
#[derive(Clone)]
pub struct ModerationLogStorage {
    pool: Arc<Pool<Postgres>>,
}

impl ModerationLogStorage {
    /// See [`new`].
    /// See [`new`].
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    /// See [`log_action`].
    #[allow(clippy::too_many_arguments)]
    pub async fn log_action(
        &self,
        rule_id: &str,
        event_id: &str,
        room_id: &str,
        sender: &str,
        content_hash: &str,
        action_taken: &str,
        confidence: f32,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query(
            r"
            INSERT INTO moderation_logs
                (rule_id, event_id, room_id, sender, content_hash, action_taken, confidence, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ",
        )
        .bind(rule_id)
        .bind(event_id)
        .bind(room_id)
        .bind(sender)
        .bind(content_hash)
        .bind(action_taken)
        .bind(confidence)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`get_logs_for_event`].
    /// See [`get_logs_for_event`].
    pub async fn get_logs_for_event(&self, event_id: &str) -> Result<Vec<ModerationLog>, sqlx::Error> {
        sqlx::query_as::<_, ModerationLog>(
            r"
            SELECT id, rule_id, event_id, room_id, sender, content_hash, action_taken, confidence, created_ts FROM moderation_logs WHERE event_id = $1 ORDER BY created_ts DESC
            ",
        )
        .bind(event_id)
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_logs_for_room`].
    /// See [`get_logs_for_room`].
    pub async fn get_logs_for_room(&self, room_id: &str, limit: i32) -> Result<Vec<ModerationLog>, sqlx::Error> {
        sqlx::query_as::<_, ModerationLog>(
            r"
            SELECT id, rule_id, event_id, room_id, sender, content_hash, action_taken, confidence, created_ts FROM moderation_logs WHERE room_id = $1
            ORDER BY created_ts DESC LIMIT $2
            ",
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_logs_for_sender`].
    /// See [`get_logs_for_sender`].
    pub async fn get_logs_for_sender(&self, sender: &str, limit: i32) -> Result<Vec<ModerationLog>, sqlx::Error> {
        sqlx::query_as::<_, ModerationLog>(
            r"
            SELECT id, rule_id, event_id, room_id, sender, content_hash, action_taken, confidence, created_ts FROM moderation_logs WHERE sender = $1
            ORDER BY created_ts DESC LIMIT $2
            ",
        )
        .bind(sender)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`cleanup_old_logs`].
    /// See [`cleanup_old_logs`].
    pub async fn cleanup_old_logs(&self, older_than_days: i32) -> Result<u64, sqlx::Error> {
        let cutoff_ts = current_timestamp_millis() - (older_than_days as i64 * 24 * 3600 * 1000);

        let result = sqlx::query(
            r"
            DELETE FROM moderation_logs WHERE created_ts < $1
            ",
        )
        .bind(cutoff_ts)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

#[async_trait]
impl ModerationLogStoreApi for ModerationLogStorage {
    async fn log_action(
        &self,
        rule_id: &str,
        event_id: &str,
        room_id: &str,
        sender: &str,
        content_hash: &str,
        action_taken: &str,
        confidence: f32,
    ) -> Result<(), sqlx::Error> {
        self.log_action(rule_id, event_id, room_id, sender, content_hash, action_taken, confidence).await
    }

    async fn get_logs_for_event(&self, event_id: &str) -> Result<Vec<ModerationLog>, sqlx::Error> {
        self.get_logs_for_event(event_id).await
    }

    async fn get_logs_for_room(&self, room_id: &str, limit: i32) -> Result<Vec<ModerationLog>, sqlx::Error> {
        self.get_logs_for_room(room_id, limit).await
    }

    async fn get_logs_for_sender(&self, sender: &str, limit: i32) -> Result<Vec<ModerationLog>, sqlx::Error> {
        self.get_logs_for_sender(sender, limit).await
    }

    async fn cleanup_old_logs(&self, older_than_days: i32) -> Result<u64, sqlx::Error> {
        self.cleanup_old_logs(older_than_days).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moderation_rule_type() {
        assert_eq!(ModerationRuleType::Regex.as_str(), "regex");
        assert_eq!(ModerationRuleType::Keyword.as_str(), "keyword");
        assert_eq!(ModerationRuleType::Domain.as_str(), "domain");
        assert_eq!(ModerationRuleType::User.as_str(), "user");
        assert_eq!(ModerationRuleType::Room.as_str(), "room");
        assert_eq!(ModerationRuleType::MediaHash.as_str(), "media_hash");
    }

    #[test]
    fn test_moderation_action() {
        assert_eq!(ModerationAction::Block.as_str(), "block");
        assert_eq!(ModerationAction::Redact.as_str(), "redact");
        assert_eq!(ModerationAction::Flag.as_str(), "flag");
        assert_eq!(ModerationAction::Quarantine.as_str(), "quarantine");
        assert_eq!(ModerationAction::Notify.as_str(), "notify");
    }

    #[test]
    fn test_create_moderation_rule_params() {
        let params = CreateModerationRuleParams {
            rule_type: ModerationRuleType::Keyword,
            pattern: "spam".to_string(),
            action: ModerationAction::Flag,
            reason: Some("Spam detection".to_string()),
            created_by: "@admin:example.com".to_string(),
            server_id: None,
            priority: Some(100),
        };

        assert_eq!(params.pattern, "spam");
        assert!(params.reason.is_some());
    }

    #[test]
    fn test_content_scan_result() {
        let result = ContentScanResult {
            is_violation: true,
            matched_rules: vec![MatchedRule {
                rule_id: "mod_123".to_string(),
                rule_type: "keyword".to_string(),
                pattern: "spam".to_string(),
                matched_text: "spam content".to_string(),
                confidence: 0.95,
            }],
            action: Some(ModerationAction::Block),
            confidence: 0.95,
            scan_duration_ms: 50,
        };

        assert!(result.is_violation);
        assert_eq!(result.matched_rules.len(), 1);
    }

    #[test]
    fn test_matched_rule() {
        let rule = MatchedRule {
            rule_id: "mod_abc".to_string(),
            rule_type: "regex".to_string(),
            pattern: r"\b\d{16}\b".to_string(),
            matched_text: "1234567890123456".to_string(),
            confidence: 0.99,
        };

        assert_eq!(rule.rule_id, "mod_abc");
        assert_eq!(rule.confidence, 0.99);
    }

    #[test]
    fn test_scan_content_request() {
        let request = ScanContentRequest {
            content: "Test message".to_string(),
            content_type: ContentType::Text,
            sender: "@alice:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            event_id: "$event123".to_string(),
        };

        assert_eq!(request.content, "Test message");
    }

    #[test]
    fn test_content_type() {
        let types = [ContentType::Text, ContentType::Image, ContentType::Video, ContentType::Audio, ContentType::File];

        assert_eq!(types.len(), 5);
    }

    #[test]
    fn test_moderation_rule_struct() {
        let rule = ModerationRule {
            id: 1,
            rule_id: "mod_xyz".to_string(),
            server_id: None,
            rule_type: "keyword".to_string(),
            pattern: "test".to_string(),
            action: "flag".to_string(),
            reason: Some("Test rule".to_string()),
            created_by: "@admin:example.com".to_string(),
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
            is_active: true,
            priority: 100,
        };

        assert_eq!(rule.rule_id, "mod_xyz");
        assert!(rule.is_active);
    }

    #[test]
    fn test_moderation_log_struct() {
        let log = ModerationLog {
            id: 1,
            rule_id: "mod_abc".to_string(),
            event_id: "$event456".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@bob:example.com".to_string(),
            content_hash: "sha256_hash".to_string(),
            action_taken: "flagged".to_string(),
            confidence: 0.85,
            created_ts: 1234567890000,
        };

        assert_eq!(log.action_taken, "flagged");
        assert_eq!(log.confidence, 0.85);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::time::Duration;

    async fn test_pool() -> Arc<Pool<Postgres>> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    #[tokio::test]
    async fn test_create_rule_returns_valid_record() {
        let pool = test_pool().await;
        let storage = ModerationStorage::new(pool);
        let params = CreateModerationRuleParams {
            rule_type: ModerationRuleType::Keyword,
            pattern: "spam_word".to_string(),
            action: ModerationAction::Flag,
            reason: Some("Test spam detection".to_string()),
            created_by: "@admin:test.com".to_string(),
            server_id: None,
            priority: Some(100),
        };

        let rule = storage.create_rule(params).await.expect("create_rule should succeed");

        assert!(rule.id > 0);
        assert!(!rule.rule_id.is_empty());
        assert!(rule.rule_id.starts_with("mod_"));
        assert_eq!(rule.rule_type, "keyword");
        assert_eq!(rule.pattern, "spam_word");
        assert_eq!(rule.action, "flag");
        assert!(rule.is_active);
        assert_eq!(rule.priority, 100);
    }

    #[tokio::test]
    async fn test_get_rule_finds_created_rule() {
        let pool = test_pool().await;
        let storage = ModerationStorage::new(pool);
        let params = CreateModerationRuleParams {
            rule_type: ModerationRuleType::Domain,
            pattern: "baddomain.com".to_string(),
            action: ModerationAction::Block,
            reason: None,
            created_by: "@admin:test.com".to_string(),
            server_id: None,
            priority: None,
        };

        let created = storage.create_rule(params).await.unwrap();
        let found = storage.get_rule(&created.rule_id).await.unwrap().expect("rule should be found");

        assert_eq!(found.rule_id, created.rule_id);
        assert_eq!(found.pattern, "baddomain.com");
        assert_eq!(found.rule_type, "domain");
    }

    #[tokio::test]
    async fn test_get_rule_returns_none_for_nonexistent() {
        let pool = test_pool().await;
        let storage = ModerationStorage::new(pool);

        let result = storage.get_rule("mod_nonexistent_12345").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_rules_by_type_filters_correctly() {
        let pool = test_pool().await;
        let storage = ModerationStorage::new(pool);
        let suffix = uuid::Uuid::new_v4();

        // Create one keyword rule
        storage
            .create_rule(CreateModerationRuleParams {
                rule_type: ModerationRuleType::Keyword,
                pattern: format!("keyword_{suffix}"),
                action: ModerationAction::Flag,
                reason: None,
                created_by: "@admin:test.com".to_string(),
                server_id: None,
                priority: None,
            })
            .await
            .unwrap();

        // Create one domain rule
        storage
            .create_rule(CreateModerationRuleParams {
                rule_type: ModerationRuleType::Domain,
                pattern: format!("domain_{suffix}.com"),
                action: ModerationAction::Block,
                reason: None,
                created_by: "@admin:test.com".to_string(),
                server_id: None,
                priority: None,
            })
            .await
            .unwrap();

        let keyword_rules = storage.get_rules_by_type("keyword").await.unwrap();
        assert!(keyword_rules.iter().all(|r| r.rule_type == "keyword"));
    }

    #[tokio::test]
    async fn test_update_rule_changes_fields() {
        let pool = test_pool().await;
        let storage = ModerationStorage::new(pool);
        let params = CreateModerationRuleParams {
            rule_type: ModerationRuleType::Regex,
            pattern: r"old_pattern".to_string(),
            action: ModerationAction::Flag,
            reason: None,
            created_by: "@admin:test.com".to_string(),
            server_id: None,
            priority: Some(50),
        };

        let created = storage.create_rule(params).await.unwrap();
        let updated =
            storage.update_rule(&created.rule_id, Some(r"new_pattern"), Some("block"), None, Some(200)).await.unwrap();

        assert_eq!(updated.pattern, "new_pattern");
        assert_eq!(updated.action, "block");
        assert_eq!(updated.priority, 200);
    }

    #[tokio::test]
    async fn test_delete_rule_soft_deletes() {
        let pool = test_pool().await;
        let storage = ModerationStorage::new(pool);
        let params = CreateModerationRuleParams {
            rule_type: ModerationRuleType::User,
            pattern: "@baduser:test.com".to_string(),
            action: ModerationAction::Block,
            reason: None,
            created_by: "@admin:test.com".to_string(),
            server_id: None,
            priority: None,
        };

        let created = storage.create_rule(params).await.unwrap();
        let deleted = storage.delete_rule(&created.rule_id).await.unwrap();
        assert!(deleted, "delete_rule should return true");

        // After soft-delete, get_rule should not find it
        let found = storage.get_rule(&created.rule_id).await.unwrap();
        assert!(found.is_none(), "soft-deleted rule should not be retrievable");
    }
}
