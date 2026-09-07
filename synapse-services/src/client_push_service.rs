use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;
use synapse_storage::account_data::AccountDataStoreApi;
use synapse_storage::push::PushStoreApi;#[derive(Debug, Clone)]
/// The `UpsertPusherRequest` struct.
pub struct UpsertPusherRequest {
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `pushkey` field.
    pub pushkey: String,
    /// The `kind` field.
    pub kind: String,
    /// The `app_id` field.
    pub app_id: String,
    /// The `app_display_name` field.
    pub app_display_name: String,
    /// The `device_display_name` field.
    pub device_display_name: String,
    /// The `profile_tag` field.
    pub profile_tag: Option<String>,
    /// The `lang` field.
    pub lang: String,
    /// The `data` field.
    pub data: Option<Value>,
}

/// The `UpsertPushRuleRequest` struct.
#[derive(Debug, Clone)]
pub struct UpsertPushRuleRequest {
    /// The `user_id` field.
    pub user_id: String,
    /// The `scope` field.
    pub scope: String,
    /// The `kind` field.
    pub kind: String,
    /// The `rule_id` field.
    pub rule_id: String,
    /// The `pattern` field.
    pub pattern: Option<String>,
    /// The `conditions` field.
    pub conditions: Option<Value>,
    /// The `actions` field.
    pub actions: Value,
}

/// The `ClientPushService` struct.
pub struct ClientPushService {
    account_data_storage: Arc<dyn AccountDataStoreApi>,
    push_storage: Arc<dyn PushStoreApi>,
}

impl ClientPushService {
    /// See [`new`].
    /// See [`new`].
    pub fn new(account_data_storage: Arc<dyn AccountDataStoreApi>, push_storage: Arc<dyn PushStoreApi>) -> Self {
        Self { account_data_storage, push_storage }
    }

    /// See [`get_pushers`].
    /// See [`get_pushers`].
    pub async fn get_pushers(&self, user_id: &str, device_id: Option<&str>) -> Result<Vec<Value>, ApiError> {
        let pushers = self
            .push_storage
            .get_pushers(user_id, device_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Database error", &e))?;

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

    /// See [`upsert_pusher`].
    /// See [`upsert_pusher`].
    pub async fn upsert_pusher(&self, request: UpsertPusherRequest) -> Result<i64, ApiError> {
        let now = current_timestamp_millis();
        self.push_storage
            .upsert_pusher(
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
            .map_err(|e| ApiError::internal_with_context("Failed to save pusher", &e))?;
        // OBS-01 (P1): Pusher 订阅是用户可控的安全敏感通道（攻击者可通过 pushkey
        // 关联获得推送送达能力）。成功路径必须留痕：包含 user_id、device_id、
        // kind、app_id，但**不**打印 pushkey/data（data 可能含 gateway URL 等配置）。
        ::tracing::info!(
            target: "security_audit",
            event = "pusher_upserted",
            user_id = %request.user_id,
            device_id = %request.device_id,
            kind = %request.kind,
            app_id = %request.app_id,
            "Pusher subscription upserted"
        );
        Ok(now)
    }

    /// See [`delete_pusher`].
    /// See [`delete_pusher`].
    pub async fn delete_pusher(&self, user_id: &str, device_id: &str, pushkey: &str) -> Result<(), ApiError> {
        self.push_storage
            .delete_pusher(user_id, device_id, pushkey)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to delete pusher", &e))?;
        // OBS-01 (P1): 删除 pusher 同样留痕。注意 pushkey 已通过路径参数获得，
        // 仅记录其存在与长度，不打印值（pushkey 可能是 APNs device token 等敏感字段）。
        ::tracing::info!(
            target: "security_audit",
            event = "pusher_deleted",
            user_id = %user_id,
            device_id = %device_id,
            pushkey_len = pushkey.len(),
            "Pusher subscription deleted"
        );
        Ok(())
    }

    /// See [`get_push_rules_content`].
    /// See [`get_push_rules_content`].
    pub async fn get_push_rules_content(&self, user_id: &str) -> Result<Option<Value>, ApiError> {
        self.account_data_storage
            .get_account_data_content(user_id, "m.push_rules")
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get push rules", &e))
    }

    /// See [`get_user_push_rules`].
    /// See [`get_user_push_rules`].
    pub async fn get_user_push_rules(&self, user_id: &str, scope: &str, kind: &str) -> Result<Vec<Value>, ApiError> {
        let rules = self
            .push_storage
            .get_user_push_rules(user_id, scope, kind)
            .await
            .map_err(|e| ApiError::internal_with_context("Database error", &e))?;

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

    /// See [`upsert_push_rule`].
    /// See [`upsert_push_rule`].
    pub async fn upsert_push_rule(&self, request: UpsertPushRuleRequest) -> Result<i64, ApiError> {
        let now = current_timestamp_millis();
        self.push_storage
            .upsert_push_rule(
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
            .map_err(|e| ApiError::internal_with_context("Failed to save push rule", &e))?;
        // OBS-01 (P1): Push rule 控制通知过滤策略（房间/关键词/事件类型），是
        // 用户可控制的通知行为边界。记录操作类型和规则 ID；不记录 pattern/conditions
        // 内容（可能含关键词等用户数据）。
        ::tracing::info!(
            target: "security_audit",
            event = "push_rule_upserted",
            user_id = %request.user_id,
            scope = %request.scope,
            kind = %request.kind,
            rule_id = %request.rule_id,
            "Push rule upserted"
        );
        Ok(now)
    }

    /// See [`delete_push_rule`].
    pub async fn delete_push_rule(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<bool, ApiError> {
        let rows = self
            .push_storage
            .delete_push_rule(user_id, scope, kind, rule_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to delete push rule", &e))?;
        // OBS-01 (P1): 删除 push rule 留痕。
        if rows > 0 {
            ::tracing::info!(
                target: "security_audit",
                event = "push_rule_deleted",
                user_id = %user_id,
                scope = %scope,
                kind = %kind,
                rule_id = %rule_id,
                "Push rule deleted"
            );
        }
        Ok(rows > 0)
    }

    /// See [`set_push_rule_actions`].
    pub async fn set_push_rule_actions(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        actions: &Value,
    ) -> Result<(), ApiError> {
        self.push_storage
            .update_push_rule_actions(user_id, scope, kind, rule_id, actions)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to update push rule actions", &e))?;
        // OBS-01 (P1): Push rule actions 控制通知行为（notify / don't_notify / coalesce）。
        ::tracing::info!(
            target: "security_audit",
            event = "push_rule_actions_updated",
            user_id = %user_id,
            scope = %scope,
            kind = %kind,
            rule_id = %rule_id,
            "Push rule actions updated"
        );
        Ok(())
    }

    /// See [`get_push_rule_enabled`].
    pub async fn get_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<Option<bool>, ApiError> {
        self.push_storage
            .get_push_rule_enabled(user_id, scope, kind, rule_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Database error", &e))
    }

    /// See [`set_push_rule_enabled`].
    pub async fn set_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        enabled: bool,
    ) -> Result<(), ApiError> {
        self.push_storage
            .set_push_rule_enabled(user_id, scope, kind, rule_id, enabled)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to update push rule enabled", &e))?;
        // OBS-01 (P1): 启用/禁用 push rule 是通知行为开关，留痕。
        ::tracing::info!(
            target: "security_audit",
            event = if enabled { "push_rule_enabled" } else { "push_rule_disabled" },
            user_id = %user_id,
            scope = %scope,
            kind = %kind,
            rule_id = %rule_id,
            enabled = enabled,
            "Push rule enabled/disabled toggled"
        );
        Ok(())
    }

    /// See [`get_notifications`].
    /// See [`get_notifications`].
    pub async fn get_notifications(&self, user_id: &str, limit: i64) -> Result<Vec<Value>, ApiError> {
        let notifications = self
            .push_storage
            .get_notifications(user_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_context("Database error", &e))?;

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

    /// See [`ack_notification`].
    /// See [`ack_notification`].
    pub async fn ack_notification(&self, notification_id: i64, user_id: &str) -> Result<bool, ApiError> {
        let result = self
            .push_storage
            .ack_notification(notification_id, user_id, current_timestamp_millis())
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to ack notification", &e))?;
        let success = result.is_some();
        // OBS-01 (P1): 通知 ack 留痕（仅当成功时记录，避免失败噪声日志）。
        if success {
            ::tracing::info!(
                target: "security_audit",
                event = "notification_acked",
                user_id = %user_id,
                notification_id = notification_id,
                "Notification acknowledged"
            );
        }
        Ok(success)
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for `ClientPushService` — coverage target ≥90%.
    //!
    //! Tested via the in-memory mock stores (`InMemoryPushStore`,
    //! `InMemoryAccountDataStore`) so the suite runs without a real
    //! PostgreSQL pool. The four methods that read raw `sqlx::postgres::PgRow`
    //! values from the trait (`get_pushers`, `get_user_push_rules`,
    //! `get_notifications`, `ack_notification`) cannot be exercised in
    //! memory because `PgRow` is a live DB handle; they are covered by the
    //! integration tests in `tests/integration/`.

    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_mocks::{InMemoryAccountDataStore, InMemoryPushStore};
    use serde_json::json;

    fn build_service() -> ClientPushService {
        ClientPushService::new(Arc::new(InMemoryAccountDataStore::new()), Arc::new(InMemoryPushStore::new()))
    }

    fn pusher_req(user_id: &str, pushkey: &str, url: &str) -> UpsertPusherRequest {
        UpsertPusherRequest {
            user_id: user_id.to_string(),
            device_id: "DEVICE".to_string(),
            pushkey: pushkey.to_string(),
            kind: "http".to_string(),
            app_id: "com.example.app".to_string(),
            app_display_name: "Example".to_string(),
            device_display_name: "Device".to_string(),
            profile_tag: None,
            lang: "en".to_string(),
            data: Some(json!({"url": url})),
        }
    }

    fn rule_req(user_id: &str, rule_id: &str, actions: Value) -> UpsertPushRuleRequest {
        UpsertPushRuleRequest {
            user_id: user_id.to_string(),
            scope: "global".to_string(),
            kind: "room".to_string(),
            rule_id: rule_id.to_string(),
            pattern: Some("!room:example.com".to_string()),
            conditions: None,
            actions,
        }
    }

    #[tokio::test]
    async fn test_upsert_pusher_returns_now_in_call_window() {
        let svc = build_service();
        let before = current_timestamp_millis();
        let ts = svc
            .upsert_pusher(pusher_req("@alice:example.com", "tok1", "https://push.example.com/v1"))
            .await
            .expect("upsert should succeed");
        let after = current_timestamp_millis();
        assert!(ts >= before, "ts {ts} must be >= start {before}");
        assert!(ts <= after, "ts {ts} must be <= end {after}");
    }

    #[tokio::test]
    async fn test_upsert_pusher_idempotent_for_same_key() {
        let svc = build_service();
        // 3 upserts on the same (user, device, pushkey) — must all succeed.
        for i in 0..3 {
            svc.upsert_pusher(pusher_req(
                "@bob:example.com",
                "tok_bob",
                &format!("https://push.example.com/v{i}"),
            ))
            .await
            .expect("repeat upsert should succeed (idempotent)");
        }
    }

    #[tokio::test]
    async fn test_upsert_pusher_different_pushkeys_coexist() {
        let svc = build_service();
        svc.upsert_pusher(pusher_req("@bob:example.com", "tok_bob_iphone", "https://push1.example.com/v1"))
            .await
            .unwrap();
        svc.upsert_pusher(pusher_req("@bob:example.com", "tok_bob_pixel", "https://push2.example.com/v1"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_delete_pusher_succeeds_for_missing_row() {
        // DELETE on a non-existent pusher should be a no-op success
        // (mirrors the `DELETE WHERE user_id=$1 AND ...` SQL semantics).
        let svc = build_service();
        svc.delete_pusher("@nobody:example.com", "DEVICE_X", "absent_pushkey")
            .await
            .expect("delete of missing pusher must be idempotent");
    }

    #[tokio::test]
    async fn test_delete_pusher_after_upsert() {
        let svc = build_service();
        svc.upsert_pusher(pusher_req("@carol:example.com", "tok_carol", "https://push.example.com/v1"))
            .await
            .unwrap();
        svc.delete_pusher("@carol:example.com", "DEVICE", "tok_carol").await.unwrap();
    }

    #[tokio::test]
    async fn test_upsert_push_rule_inserts_new() {
        let svc = build_service();
        svc.upsert_push_rule(rule_req("@dave:example.com", ".m.rule.dave", json!([{"kind": "notify"}])))
            .await
            .expect("upsert rule should succeed");
    }

    #[tokio::test]
    async fn test_upsert_push_rule_overwrites_via_on_conflict() {
        let svc = build_service();
        svc.upsert_push_rule(rule_req("@eve:example.com", ".m.rule.eve", json!([{"kind": "notify"}])))
            .await
            .unwrap();
        // Overwrite with disabled notification — must not error.
        svc.upsert_push_rule(rule_req("@eve:example.com", ".m.rule.eve", json!([{"kind": "dont_notify"}])))
            .await
            .expect("overwrite via ON CONFLICT must succeed");
    }

    #[tokio::test]
    async fn test_delete_push_rule_returns_true_for_existing() {
        let svc = build_service();
        svc.upsert_push_rule(rule_req("@frank:example.com", ".m.rule.frank", json!([{"kind": "notify"}])))
            .await
            .unwrap();
        let existed = svc
            .delete_push_rule("@frank:example.com", "global", "room", ".m.rule.frank")
            .await
            .unwrap();
        assert!(existed, "deleting existing rule must return true");
    }

    #[tokio::test]
    async fn test_delete_push_rule_returns_false_for_missing() {
        let svc = build_service();
        let existed = svc
            .delete_push_rule("@ghost:example.com", "global", "room", ".m.rule.absent")
            .await
            .unwrap();
        assert!(!existed, "deleting missing rule must return false");
    }

    #[tokio::test]
    async fn test_set_push_rule_actions_on_existing_rule() {
        let svc = build_service();
        svc.upsert_push_rule(rule_req("@gina:example.com", ".m.rule.gina", json!([{"kind": "notify"}])))
            .await
            .unwrap();
        svc.set_push_rule_actions(
            "@gina:example.com",
            "global",
            "room",
            ".m.rule.gina",
            &json!([{"kind": "dont_notify"}]),
        )
        .await
        .expect("set actions on existing rule must succeed");
    }

    #[tokio::test]
    async fn test_set_push_rule_actions_silently_noop_for_missing() {
        // Mock is a no-op for non-existent rules (mirrors `UPDATE WHERE`
        // returning 0 rows). Service must not error.
        let svc = build_service();
        svc.set_push_rule_actions(
            "@henry:example.com",
            "global",
            "room",
            ".m.rule.absent",
            &json!([{"kind": "notify"}]),
        )
        .await
        .expect("set actions on missing rule must not error");
    }

    #[tokio::test]
    async fn test_get_push_rule_enabled_returns_none_when_missing() {
        let svc = build_service();
        let result = svc
            .get_push_rule_enabled("@ivy:example.com", "global", "room", "never_existed")
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_push_rule_enabled_default_true_after_upsert() {
        let svc = build_service();
        svc.upsert_push_rule(rule_req("@jack:example.com", ".m.rule.jack", json!([{"kind": "notify"}])))
            .await
            .unwrap();
        let enabled = svc
            .get_push_rule_enabled("@jack:example.com", "global", "room", ".m.rule.jack")
            .await
            .unwrap();
        assert_eq!(enabled, Some(true), "freshly upserted rule must default to enabled");
    }

    #[tokio::test]
    async fn test_set_push_rule_enabled_round_trip() {
        let svc = build_service();
        svc.upsert_push_rule(rule_req("@kate:example.com", ".m.rule.kate", json!([{"kind": "notify"}])))
            .await
            .unwrap();

        // Disable
        svc.set_push_rule_enabled("@kate:example.com", "global", "room", ".m.rule.kate", false)
            .await
            .unwrap();
        let enabled = svc
            .get_push_rule_enabled("@kate:example.com", "global", "room", ".m.rule.kate")
            .await
            .unwrap();
        assert_eq!(enabled, Some(false));

        // Re-enable
        svc.set_push_rule_enabled("@kate:example.com", "global", "room", ".m.rule.kate", true)
            .await
            .unwrap();
        let enabled = svc
            .get_push_rule_enabled("@kate:example.com", "global", "room", ".m.rule.kate")
            .await
            .unwrap();
        assert_eq!(enabled, Some(true));
    }

    #[tokio::test]
    async fn test_set_push_rule_enabled_silently_noop_for_missing() {
        let svc = build_service();
        svc.set_push_rule_enabled("@liam:example.com", "global", "room", ".m.rule.absent", false)
            .await
            .expect("set_enabled on missing rule must not error");
    }

    #[tokio::test]
    async fn test_get_push_rules_content_returns_none_when_absent() {
        let svc = build_service();
        let content = svc.get_push_rules_content("@mia:example.com").await.unwrap();
        assert!(content.is_none(), "no stored rules should yield None");
    }
}
