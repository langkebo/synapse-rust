use crate::common::ApiError;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct UpsertPusherRequest {
    pub user_id: String,
    pub device_id: String,
    pub pushkey: String,
    pub kind: String,
    pub app_id: String,
    pub app_display_name: String,
    pub device_display_name: String,
    pub profile_tag: Option<String>,
    pub lang: String,
    pub data: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct UpsertPushRuleRequest {
    pub user_id: String,
    pub scope: String,
    pub kind: String,
    pub rule_id: String,
    pub pattern: Option<String>,
    pub conditions: Option<Value>,
    pub actions: Value,
}

pub struct ClientPushService {
    pool: Arc<PgPool>,
}

impl ClientPushService {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    pub async fn get_pushers(&self, user_id: &str, device_id: Option<&str>) -> Result<Vec<Value>, ApiError> {
        let pushers = sqlx::query!(
            r#"
            SELECT pushkey AS "pushkey!", kind AS "kind!", app_id AS "app_id!", app_display_name AS "app_display_name!", device_display_name AS "device_display_name!",
                   profile_tag AS "profile_tag?", lang AS "lang!", data AS "data"
            FROM pushers
            WHERE user_id = $1 AND device_id IS NOT DISTINCT FROM $2
            ORDER BY created_ts DESC
            "#,
            user_id,
            device_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(pushers
            .iter()
            .map(|row| {
                json!({
                    "pushkey": row.pushkey,
                    "kind": row.kind,
                    "app_id": row.app_id,
                    "app_display_name": row.app_display_name,
                    "device_display_name": row.device_display_name,
                    "profile_tag": row.profile_tag,
                    "lang": row.lang,
                    "data": row.data.as_ref().unwrap_or(&json!({}))
                })
            })
            .collect())
    }

    pub async fn upsert_pusher(&self, request: UpsertPusherRequest) -> Result<i64, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query!(
            r"
            INSERT INTO pushers (user_id, device_id, pushkey, pushkey_ts, kind, app_id, app_display_name,
                                 device_display_name, profile_tag, lang, data, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (user_id, device_id, pushkey) DO UPDATE SET
                pushkey_ts = $4, kind = $5, app_id = $6, app_display_name = $7,
                device_display_name = $8, profile_tag = $9, lang = $10, data = $11, updated_ts = $13
            ",
            request.user_id,
            request.device_id,
            request.pushkey,
            now,
            request.kind,
            request.app_id,
            request.app_display_name,
            request.device_display_name,
            request.profile_tag,
            request.lang,
            request.data.as_ref(),
            now,
            now,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to save pusher", &e))?;
        Ok(now)
    }

    pub async fn delete_pusher(&self, user_id: &str, device_id: &str, pushkey: &str) -> Result<(), ApiError> {
        sqlx::query!(
            "DELETE FROM pushers WHERE user_id = $1 AND pushkey = $2 AND device_id = $3",
            user_id,
            pushkey,
            device_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to delete pusher", &e))?;
        Ok(())
    }

    pub async fn get_push_rules_content(&self, user_id: &str) -> Result<Option<Value>, ApiError> {
        sqlx::query_scalar!(r#"SELECT content AS "content!" FROM account_data WHERE user_id = $1 AND data_type = 'm.push_rules'"#, user_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get push rules", &e))
    }

    pub async fn get_user_push_rules(&self, user_id: &str, scope: &str, kind: &str) -> Result<Vec<Value>, ApiError> {
        let rules = sqlx::query!(
            r#"
            SELECT rule_id AS "rule_id!", pattern AS "pattern?", conditions AS "conditions", actions AS "actions", is_enabled AS "is_enabled!", is_default AS "is_default!"
            FROM push_rules
            WHERE user_id = $1 AND scope = $2 AND kind = $3
            ORDER BY priority DESC, created_ts ASC
            "#,
            user_id,
            scope,
            kind,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(rules
            .iter()
            .map(|row| {
                json!({
                    "rule_id": row.rule_id,
                    "default": row.is_default,
                    "enabled": row.is_enabled,
                    "pattern": row.pattern,
                    "conditions": row.conditions,
                    "actions": row.actions.as_ref().unwrap_or(&json!([])).clone()
                })
            })
            .collect())
    }

    pub async fn upsert_push_rule(&self, request: UpsertPushRuleRequest) -> Result<i64, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query!(
            r"
            INSERT INTO push_rules (user_id, scope, kind, rule_id, pattern, conditions, actions, is_enabled, is_default, priority_class, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, true, false, 5, $8)
            ON CONFLICT (user_id, scope, kind, rule_id) DO UPDATE SET
                pattern = $5, conditions = $6, actions = $7
            ",
            request.user_id,
            request.scope,
            request.kind,
            request.rule_id,
            request.pattern.as_deref(),
            request.conditions.as_ref(),
            &request.actions,
            now,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to save push rule", &e))?;
        Ok(now)
    }

    pub async fn delete_push_rule(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<bool, ApiError> {
        let result = sqlx::query!(
            "DELETE FROM push_rules WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $4",
            user_id,
            scope,
            kind,
            rule_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to delete push rule", &e))?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn set_push_rule_actions(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        actions: &Value,
    ) -> Result<(), ApiError> {
        sqlx::query!(
            "UPDATE push_rules SET actions = $4 WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $5",
            user_id,
            scope,
            kind,
            actions,
            rule_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update push rule actions", &e))?;
        Ok(())
    }

    pub async fn get_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<Option<bool>, ApiError> {
        let result = sqlx::query_scalar!(
            r#"SELECT is_enabled AS "is_enabled!" FROM push_rules WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $4"#,
            user_id,
            scope,
            kind,
            rule_id,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
        Ok(result)
    }

    pub async fn set_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        enabled: bool,
    ) -> Result<(), ApiError> {
        sqlx::query!(
            "UPDATE push_rules SET is_enabled = $4 WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $5",
            user_id,
            scope,
            kind,
            enabled,
            rule_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update push rule enabled", &e))?;
        Ok(())
    }

    pub async fn get_notifications(&self, user_id: &str, limit: i64) -> Result<Vec<Value>, ApiError> {
        let notifications = sqlx::query!(
            r#"
            SELECT id AS "id!", event_id AS "event_id?", room_id AS "room_id?", ts AS "ts?", notification_type AS "notification_type?", is_read AS "is_read?"
            FROM notifications
            WHERE user_id = $1
            ORDER BY ts DESC
            LIMIT $2
            "#,
            user_id,
            limit,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(notifications
            .iter()
            .map(|row| {
                json!({
                    "notification_id": row.id,
                    "event_id": row.event_id,
                    "room_id": row.room_id,
                    "ts": row.ts,
                    "profile_tag": row.notification_type,
                    "read": row.is_read.unwrap_or(false)
                })
            })
            .collect())
    }

    pub async fn ack_notification(&self, notification_id: i64, user_id: &str) -> Result<bool, ApiError> {
        let result = sqlx::query!(
            "UPDATE notifications SET is_read = true, updated_ts = $3 WHERE id = $1 AND user_id = $2 RETURNING id AS \"id!\"",
            notification_id,
            user_id,
            chrono::Utc::now().timestamp_millis(),
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to ack notification", &e))?;
        Ok(result.is_some())
    }
}
