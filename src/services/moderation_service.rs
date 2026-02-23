use crate::common::*;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use serde_json::json;

#[derive(Debug, Clone)]
pub struct ReputationConfig {
    pub initial_score: i32,
    pub max_score: i32,
    pub min_score: i32,
    pub report_penalty: i32,
    pub positive_feedback_bonus: i32,
    pub auto_warn_threshold: i32,
    pub auto_ban_threshold: i32,
}

impl Default for ReputationConfig {
    fn default() -> Self {
        Self {
            initial_score: 50,
            max_score: 100,
            min_score: 0,
            report_penalty: 10,
            positive_feedback_bonus: 5,
            auto_warn_threshold: 20,
            auto_ban_threshold: 0,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserReputation {
    pub user_id: String,
    pub reputation_score: i32,
    pub total_reports: i32,
    pub accepted_reports: i32,
    pub false_reports: i32,
    pub last_report_ts: Option<i64>,
    pub last_update_ts: i64,
    pub warnings_count: i32,
    pub is_banned: bool,
    pub ban_reason: Option<String>,
    pub ban_expires_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ModerationAction {
    pub action_type: String,
    pub reason: String,
    pub expires_at: Option<i64>,
    pub report_id: i64,
}

#[derive(Clone)]
pub struct ModerationService {
    pool: Arc<Pool<Postgres>>,
    config: ReputationConfig,
}

impl ModerationService {
    pub fn new(pool: Arc<Pool<Postgres>>, config: Option<ReputationConfig>) -> Self {
        Self {
            pool,
            config: config.unwrap_or_default(),
        }
    }

    pub async fn init_schema(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_reputations (
                user_id TEXT PRIMARY KEY,
                reputation_score INTEGER NOT NULL DEFAULT 50,
                total_reports INTEGER NOT NULL DEFAULT 0,
                accepted_reports INTEGER NOT NULL DEFAULT 0,
                false_reports INTEGER NOT NULL DEFAULT 0,
                last_report_ts BIGINT,
                last_update_ts BIGINT NOT NULL,
                warnings_count INTEGER NOT NULL DEFAULT 0,
                is_banned BOOLEAN NOT NULL DEFAULT FALSE,
                ban_reason TEXT,
                ban_expires_at BIGINT
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS moderation_actions (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                action_type TEXT NOT NULL,
                reason TEXT,
                report_id BIGINT,
                created_ts BIGINT NOT NULL,
                expires_at BIGINT,
                revoked BOOLEAN NOT NULL DEFAULT FALSE,
                revoked_reason TEXT,
                revoked_at BIGINT
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS content_filters (
                id BIGSERIAL PRIMARY KEY,
                filter_type TEXT NOT NULL,
                pattern TEXT NOT NULL,
                severity TEXT NOT NULL DEFAULT 'warning',
                is_active BOOLEAN NOT NULL DEFAULT TRUE,
                created_ts BIGINT NOT NULL,
                created_by TEXT
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_moderation_actions_user_id ON moderation_actions(user_id)"
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_content_filters_type ON content_filters(filter_type)",
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_or_create_reputation(
        &self,
        user_id: &str,
    ) -> Result<UserReputation, sqlx::Error> {
        let existing = sqlx::query_as::<_, UserReputation>(
            "SELECT * FROM user_reputations WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(reputation) = existing {
            Ok(reputation)
        } else {
            let now = chrono::Utc::now().timestamp();
            let reputation = UserReputation {
                user_id: user_id.to_string(),
                reputation_score: self.config.initial_score,
                total_reports: 0,
                accepted_reports: 0,
                false_reports: 0,
                last_report_ts: None,
                last_update_ts: now,
                warnings_count: 0,
                is_banned: false,
                ban_reason: None,
                ban_expires_at: None,
            };

            sqlx::query(
                r#"
                INSERT INTO user_reputations (user_id, reputation_score, total_reports, accepted_reports, 
                    false_reports, last_report_ts, last_update_ts, warnings_count, is_banned, ban_reason, ban_expires_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                "#,
            )
            .bind(user_id)
            .bind(reputation.reputation_score)
            .bind(reputation.total_reports)
            .bind(reputation.accepted_reports)
            .bind(reputation.false_reports)
            .bind(reputation.last_report_ts)
            .bind(reputation.last_update_ts)
            .bind(reputation.warnings_count)
            .bind(reputation.is_banned)
            .bind(&reputation.ban_reason)
            .bind(reputation.ban_expires_at)
            .execute(&*self.pool)
            .await?;

            Ok(reputation)
        }
    }

    pub async fn update_reputation_on_report(
        &self,
        user_id: &str,
        report_id: i64,
    ) -> Result<ModerationAction, ApiError> {
        let mut reputation = self.get_or_create_reputation(user_id).await?;
        let now = chrono::Utc::now().timestamp();

        reputation.total_reports += 1;
        reputation.reputation_score =
            (reputation.reputation_score - self.config.report_penalty).max(self.config.min_score);
        reputation.last_report_ts = Some(now);
        reputation.last_update_ts = now;

        sqlx::query(
            r#"
            UPDATE user_reputations SET 
                total_reports = $1,
                reputation_score = $2,
                last_report_ts = $3,
                last_update_ts = $4
            WHERE user_id = $5
            "#,
        )
        .bind(reputation.total_reports)
        .bind(reputation.reputation_score)
        .bind(reputation.last_report_ts)
        .bind(reputation.last_update_ts)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        let mut action = None;

        if reputation.reputation_score <= self.config.auto_warn_threshold
            && reputation.reputation_score > self.config.auto_ban_threshold
        {
            reputation.warnings_count += 1;
            sqlx::query("UPDATE user_reputations SET warnings_count = $1 WHERE user_id = $2")
                .bind(reputation.warnings_count)
                .bind(user_id)
                .execute(&*self.pool)
                .await?;

            let _warn_id = self
                .create_moderation_action(
                    user_id,
                    "warning",
                    &format!(
                        "Automatic warning: reputation score dropped to {}",
                        reputation.reputation_score
                    ),
                    Some(report_id),
                    None,
                )
                .await?;

            action = Some(ModerationAction {
                action_type: "warning".to_string(),
                reason: format!(
                    "Automatic warning: reputation score dropped to {}",
                    reputation.reputation_score
                ),
                expires_at: None,
                report_id,
            });
        }

        if reputation.reputation_score <= self.config.auto_ban_threshold {
            self.ban_user(
                user_id,
                "Automatic ban: reputation score below threshold",
                None,
                None,
            )
            .await?;

            let _ban_id = self
                .create_moderation_action(
                    user_id,
                    "ban",
                    &format!(
                        "Automatic ban: reputation score dropped to {}",
                        reputation.reputation_score
                    ),
                    Some(report_id),
                    None,
                )
                .await?;

            action = Some(ModerationAction {
                action_type: "ban".to_string(),
                reason: format!(
                    "Automatic ban: reputation score dropped to {}",
                    reputation.reputation_score
                ),
                expires_at: None,
                report_id,
            });
        }

        action.ok_or_else(|| ApiError::internal("No moderation action triggered".to_string()))
    }

    pub async fn resolve_report(
        &self,
        report_id: i64,
        accepted: bool,
        moderator_id: &str,
    ) -> Result<(), ApiError> {
        let report = sqlx::query_as::<_, (i64, String, i32)>(
            "SELECT id, user_id, score FROM event_reports WHERE id = $1",
        )
        .bind(report_id)
        .fetch_optional(&*self.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("Report not found".to_string()))?;

        let user_id = report.1;
        let report_score = report.2;

        let mut reputation = self.get_or_create_reputation(&user_id).await?;
        let now = chrono::Utc::now().timestamp();

        if accepted {
            reputation.accepted_reports += 1;
            let additional_penalty = (report_score.abs() / 10).max(1);
            reputation.reputation_score =
                (reputation.reputation_score - additional_penalty).max(self.config.min_score);
        } else {
            reputation.false_reports += 1;
            reputation.reputation_score = (reputation.reputation_score
                + self.config.positive_feedback_bonus)
                .min(self.config.max_score);
        }

        reputation.last_update_ts = now;

        sqlx::query(
            r#"
            UPDATE user_reputations SET 
                accepted_reports = $1,
                false_reports = $2,
                reputation_score = $3,
                last_update_ts = $4
            WHERE user_id = $5
            "#,
        )
        .bind(reputation.accepted_reports)
        .bind(reputation.false_reports)
        .bind(reputation.reputation_score)
        .bind(reputation.last_update_ts)
        .bind(&user_id)
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            "UPDATE event_reports SET moderator_id = $1, resolved = TRUE, resolved_at = $2 WHERE id = $3",
        )
        .bind(moderator_id)
        .bind(now)
        .bind(report_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn ban_user(
        &self,
        user_id: &str,
        reason: &str,
        expires_at: Option<i64>,
        report_id: Option<i64>,
    ) -> Result<i64, ApiError> {
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            r#"
            UPDATE user_reputations SET 
                is_banned = TRUE,
                ban_reason = $1,
                ban_expires_at = $2,
                last_update_ts = $3
            WHERE user_id = $4
            "#,
        )
        .bind(reason)
        .bind(expires_at)
        .bind(now)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        self.create_moderation_action(user_id, "ban", reason, report_id, expires_at)
            .await
    }

    pub async fn unban_user(&self, user_id: &str, reason: &str) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            r#"
            UPDATE user_reputations SET 
                is_banned = FALSE,
                ban_reason = NULL,
                ban_expires_at = NULL,
                last_update_ts = $1
            WHERE user_id = $2
            "#,
        )
        .bind(now)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        self.create_moderation_action(user_id, "unban", reason, None, None)
            .await?;

        Ok(())
    }

    async fn create_moderation_action(
        &self,
        user_id: &str,
        action_type: &str,
        reason: &str,
        report_id: Option<i64>,
        expires_at: Option<i64>,
    ) -> Result<i64, ApiError> {
        let now = chrono::Utc::now().timestamp();

        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO moderation_actions (user_id, action_type, reason, report_id, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
        )
        .bind(user_id)
        .bind(action_type)
        .bind(reason)
        .bind(report_id)
        .bind(now)
        .bind(expires_at)
        .fetch_one(&*self.pool)
        .await?;

        Ok(id)
    }

    pub async fn check_user_status(&self, user_id: &str) -> Result<UserReputation, ApiError> {
        self.get_or_create_reputation(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))
    }

    pub async fn is_user_banned(&self, user_id: &str) -> Result<bool, ApiError> {
        let reputation = self.check_user_status(user_id).await?;
        Ok(reputation.is_banned)
    }

    pub async fn get_moderation_history(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, ApiError> {
        let actions =
            sqlx::query_as::<_, (i64, String, String, Option<i64>, i64, Option<i64>, bool)>(
                r#"
            SELECT id, action_type, reason, report_id, created_ts, expires_at, revoked
            FROM moderation_actions
            WHERE user_id = $1
            ORDER BY created_ts DESC
            LIMIT $2
            "#,
            )
            .bind(user_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let results: Vec<serde_json::Value> = actions
            .iter()
            .map(|action| {
                json!({
                    "id": action.0,
                    "action_type": action.1,
                    "reason": action.2,
                    "report_id": action.3,
                    "created_ts": action.4,
                    "expires_at": action.5,
                    "revoked": action.6
                })
            })
            .collect();

        Ok(results)
    }

    pub async fn get_top_reputation_users(
        &self,
        limit: i64,
    ) -> Result<Vec<UserReputation>, ApiError> {
        let users = sqlx::query_as::<_, UserReputation>(
            r#"
            SELECT * FROM user_reputations
            WHERE is_banned = FALSE
            ORDER BY reputation_score DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(users)
    }

    pub async fn get_low_reputation_users(
        &self,
        limit: i64,
    ) -> Result<Vec<UserReputation>, ApiError> {
        let users = sqlx::query_as::<_, UserReputation>(
            r#"
            SELECT * FROM user_reputations
            WHERE is_banned = FALSE
            ORDER BY reputation_score ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(users)
    }

    pub async fn get_reputation_stats(&self) -> Result<serde_json::Value, ApiError> {
        let total_users = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM user_reputations")
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let banned_users = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM user_reputations WHERE is_banned = TRUE",
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let avg_score =
            sqlx::query_scalar::<_, f64>("SELECT AVG(reputation_score) FROM user_reputations")
                .fetch_one(&*self.pool)
                .await
                .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(json!({
            "total_users": total_users,
            "banned_users": banned_users,
            "average_score": avg_score,
            "config": {
                "initial_score": self.config.initial_score,
                "auto_warn_threshold": self.config.auto_warn_threshold,
                "auto_ban_threshold": self.config.auto_ban_threshold
            }
        }))
    }
}
