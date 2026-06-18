use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::account_data::AccountDataStorage;
use synapse_storage::push::PushStorage;

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
    account_data_storage: AccountDataStorage,
    pool: Arc<PgPool>,
}

impl ClientPushService {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { account_data_storage: AccountDataStorage::new(&pool), pool }
    }

    pub async fn get_pushers(&self, user_id: &str, device_id: Option<&str>) -> Result<Vec<Value>, ApiError> {
        let pushers = PushStorage::get_pushers(&self.pool, user_id, device_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(pushers
            .iter()
            .map(|row| {
                let data = row.try_get::<Option<Value>, _>("data").ok().flatten().unwrap_or_else(|| json!({}));
                json!({
                    "pushkey": row.get::<String, _>("pushkey"),
                    "kind": row.get::<String, _>("kind"),
                    "app_id": row.get::<String, _>("app_id"),
                    "app_display_name": row.get::<String, _>("app_display_name"),
                    "device_display_name": row.get::<String, _>("device_display_name"),
                    "profile_tag": row.try_get::<Option<String>, _>("profile_tag").ok().flatten(),
                    "lang": row.get::<String, _>("lang"),
                    "data": data
                })
            })
            .collect())
    }

    pub async fn upsert_pusher(&self, request: UpsertPusherRequest) -> Result<i64, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        PushStorage::upsert_pusher(
            &self.pool,
            &request.user_id,
            &request.device_id,
            &request.pushkey,
            &request.kind,
            &request.app_id,
            &request.app_display_name,
            &request.device_display_name,
            &request.profile_tag,
            &request.lang,
            &request.data,
            now,
        )
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to save pusher", &e))?;
        Ok(now)
    }

    pub async fn delete_pusher(&self, user_id: &str, device_id: &str, pushkey: &str) -> Result<(), ApiError> {
        PushStorage::delete_pusher(&self.pool, user_id, device_id, pushkey)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete pusher", &e))?;
        Ok(())
    }

    pub async fn get_push_rules_content(&self, user_id: &str) -> Result<Option<Value>, ApiError> {
        self.account_data_storage
            .get_account_data_content(user_id, "m.push_rules")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get push rules", &e))
    }

    pub async fn get_user_push_rules(&self, user_id: &str, scope: &str, kind: &str) -> Result<Vec<Value>, ApiError> {
        let rules = PushStorage::get_user_push_rules(&self.pool, user_id, scope, kind)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(rules
            .iter()
            .map(|row| {
                let actions = row.try_get::<Option<Value>, _>("actions").ok().flatten().unwrap_or_else(|| json!([]));
                json!({
                    "rule_id": row.get::<String, _>("rule_id"),
                    "default": row.get::<bool, _>("is_default"),
                    "enabled": row.get::<bool, _>("is_enabled"),
                    "pattern": row.try_get::<Option<String>, _>("pattern").ok().flatten(),
                    "conditions": row.try_get::<Option<Value>, _>("conditions").ok().flatten(),
                    "actions": actions
                })
            })
            .collect())
    }

    pub async fn upsert_push_rule(&self, request: UpsertPushRuleRequest) -> Result<i64, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        PushStorage::upsert_push_rule(
            &self.pool,
            &request.user_id,
            &request.scope,
            &request.kind,
            &request.rule_id,
            &request.pattern,
            &request.conditions,
            &request.actions,
            now,
        )
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
        let rows = PushStorage::delete_push_rule(&self.pool, user_id, scope, kind, rule_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete push rule", &e))?;
        Ok(rows > 0)
    }

    pub async fn set_push_rule_actions(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        actions: &Value,
    ) -> Result<(), ApiError> {
        PushStorage::update_push_rule_actions(&self.pool, user_id, scope, kind, rule_id, actions)
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
        PushStorage::get_push_rule_enabled(&self.pool, user_id, scope, kind, rule_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))
    }

    pub async fn set_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        enabled: bool,
    ) -> Result<(), ApiError> {
        PushStorage::set_push_rule_enabled(&self.pool, user_id, scope, kind, rule_id, enabled)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update push rule enabled", &e))?;
        Ok(())
    }

    pub async fn get_notifications(&self, user_id: &str, limit: i64) -> Result<Vec<Value>, ApiError> {
        let notifications = PushStorage::get_notifications(&self.pool, user_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(notifications
            .iter()
            .map(|row| {
                json!({
                    "notification_id": row.get::<i64, _>("id"),
                    "event_id": row.try_get::<Option<String>, _>("event_id").ok().flatten(),
                    "room_id": row.try_get::<Option<String>, _>("room_id").ok().flatten(),
                    "ts": row.try_get::<Option<i64>, _>("ts").ok().flatten(),
                    "profile_tag": row.try_get::<Option<String>, _>("notification_type").ok().flatten(),
                    "read": row.try_get::<Option<bool>, _>("is_read").ok().flatten().unwrap_or(false)
                })
            })
            .collect())
    }

    pub async fn ack_notification(&self, notification_id: i64, user_id: &str) -> Result<bool, ApiError> {
        let result =
            PushStorage::ack_notification(&self.pool, notification_id, user_id, chrono::Utc::now().timestamp_millis())
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to ack notification", &e))?;
        Ok(result.is_some())
    }
}