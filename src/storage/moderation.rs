use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ModerationRule {
    pub id: i64,
    pub rule_id: String,
    pub server_id: Option<String>,
    pub rule_type: String,
    pub pattern: String,
    pub action: String,
    pub reason: Option<String>,
    pub created_by: String,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub is_active: bool,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateModerationRuleParams {
    pub rule_type: ModerationRuleType,
    pub pattern: String,
    pub action: ModerationAction,
    pub reason: Option<String>,
    pub created_by: String,
    pub server_id: Option<String>,
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModerationRuleType {
    #[serde(rename = "regex")]
    Regex,
    #[serde(rename = "keyword")]
    Keyword,
    #[serde(rename = "domain")]
    Domain,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "room")]
    Room,
    #[serde(rename = "media_hash")]
    MediaHash,
}

impl ModerationRuleType {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModerationAction {
    #[serde(rename = "block")]
    Block,
    #[serde(rename = "redact")]
    Redact,
    #[serde(rename = "flag")]
    Flag,
    #[serde(rename = "quarantine")]
    Quarantine,
    #[serde(rename = "notify")]
    Notify,
}

impl ModerationAction {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentScanResult {
    pub is_violation: bool,
    pub matched_rules: Vec<MatchedRule>,
    pub action: Option<ModerationAction>,
    pub confidence: f32,
    pub scan_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedRule {
    pub rule_id: String,
    pub rule_type: String,
    pub pattern: String,
    pub matched_text: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanContentRequest {
    pub content: String,
    pub content_type: ContentType,
    pub sender: String,
    pub room_id: String,
    pub event_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentType {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "image")]
    Image,
    #[serde(rename = "video")]
    Video,
    #[serde(rename = "audio")]
    Audio,
    #[serde(rename = "file")]
    File,
}

#[derive(Clone)]
pub struct ModerationStorage {
    pool: Arc<Pool<Postgres>>,
}

impl ModerationStorage {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    pub async fn create_rule(
        &self,
        params: CreateModerationRuleParams,
    ) -> Result<ModerationRule, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let rule_id = format!("mod_{}", uuid::Uuid::new_v4().simple());

        sqlx::query_as::<_, ModerationRule>(
            r#"
            INSERT INTO moderation_rules 
                (rule_id, server_id, rule_type, pattern, action, reason, created_by, created_ts, updated_ts, is_active, priority)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8, true, $9)
            RETURNING *
            "#,
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

    pub async fn get_rule(&self, rule_id: &str) -> Result<Option<ModerationRule>, sqlx::Error> {
        sqlx::query_as::<_, ModerationRule>(
            r#"
            SELECT * FROM moderation_rules WHERE rule_id = $1 AND is_active = true
            "#,
        )
        .bind(rule_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_all_rules(&self) -> Result<Vec<ModerationRule>, sqlx::Error> {
        sqlx::query_as::<_, ModerationRule>(
            r#"
            SELECT * FROM moderation_rules 
            WHERE is_active = true 
            ORDER BY priority DESC, created_ts ASC
            "#,
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_rules_by_type(
        &self,
        rule_type: &str,
    ) -> Result<Vec<ModerationRule>, sqlx::Error> {
        sqlx::query_as::<_, ModerationRule>(
            r#"
            SELECT * FROM moderation_rules 
            WHERE rule_type = $1 AND is_active = true 
            ORDER BY priority DESC, created_ts ASC
            "#,
        )
        .bind(rule_type)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn update_rule(
        &self,
        rule_id: &str,
        pattern: Option<&str>,
        action: Option<&str>,
        reason: Option<&str>,
        priority: Option<i32>,
    ) -> Result<ModerationRule, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as::<_, ModerationRule>(
            r#"
            UPDATE moderation_rules 
            SET 
                pattern = COALESCE($2, pattern),
                action = COALESCE($3, action),
                reason = COALESCE($4, reason),
                priority = COALESCE($5, priority),
                updated_ts = $6
            WHERE rule_id = $1
            RETURNING *
            "#,
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

    pub async fn delete_rule(&self, rule_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE moderation_rules 
            SET is_active = false
            WHERE rule_id = $1
            "#,
        )
        .bind(rule_id)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ModerationLog {
    pub id: i64,
    pub rule_id: String,
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub content_hash: String,
    pub action_taken: String,
    pub confidence: f32,
    pub created_ts: i64,
}

#[derive(Clone)]
pub struct ModerationLogStorage {
    pool: Arc<Pool<Postgres>>,
}

impl ModerationLogStorage {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

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
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO moderation_logs 
                (rule_id, event_id, room_id, sender, content_hash, action_taken, confidence, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
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

    pub async fn get_logs_for_event(
        &self,
        event_id: &str,
    ) -> Result<Vec<ModerationLog>, sqlx::Error> {
        sqlx::query_as::<_, ModerationLog>(
            r#"
            SELECT * FROM moderation_logs WHERE event_id = $1 ORDER BY created_ts DESC
            "#,
        )
        .bind(event_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_logs_for_room(
        &self,
        room_id: &str,
        limit: i32,
    ) -> Result<Vec<ModerationLog>, sqlx::Error> {
        sqlx::query_as::<_, ModerationLog>(
            r#"
            SELECT * FROM moderation_logs WHERE room_id = $1 
            ORDER BY created_ts DESC LIMIT $2
            "#,
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_logs_for_sender(
        &self,
        sender: &str,
        limit: i32,
    ) -> Result<Vec<ModerationLog>, sqlx::Error> {
        sqlx::query_as::<_, ModerationLog>(
            r#"
            SELECT * FROM moderation_logs WHERE sender = $1 
            ORDER BY created_ts DESC LIMIT $2
            "#,
        )
        .bind(sender)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn cleanup_old_logs(&self, older_than_days: i32) -> Result<u64, sqlx::Error> {
        let cutoff_ts = chrono::Utc::now().timestamp_millis()
            - (older_than_days as i64 * 24 * 3600 * 1000);

        let result = sqlx::query(
            r#"
            DELETE FROM moderation_logs WHERE created_ts < $1
            "#,
        )
        .bind(cutoff_ts)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
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
        let types = [
            ContentType::Text,
            ContentType::Image,
            ContentType::Video,
            ContentType::Audio,
            ContentType::File,
        ];

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
