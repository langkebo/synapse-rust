use super::*;

use serde_json::Value;

use crate::push::PushStoreApi;

/// Stored push-rule state for the in-memory mock, mirroring the mutable columns
/// of the `push_rules` table that the typed trait methods touch.
#[derive(Clone, Debug)]
#[allow(dead_code)] // `pattern`/`conditions` are only surfaced via raw-row readers, which are unimplemented.
struct PushRuleEntry {
    pattern: Option<String>,
    conditions: Option<Value>,
    actions: Value,
    is_enabled: bool,
}

/// Stored pusher state for the in-memory mock.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Fields are only surfaced via `get_pushers`, a raw-row reader that is unimplemented.
struct PusherEntry {
    kind: String,
    app_id: String,
    app_display_name: String,
    device_display_name: String,
    profile_tag: Option<String>,
    lang: String,
    data: Option<Value>,
    updated_ts: i64,
}

/// In-memory [`PushStoreApi`].
///
/// Faithfully implements the typed pusher/push-rule methods with `HashMap`
/// storage. The methods that return raw `sqlx::postgres::PgRow` values
/// (`get_pushers`, `get_user_push_rules`, `get_notifications`,
/// `ack_notification`) cannot be represented in memory and are left
/// `unimplemented!()`.
#[derive(Clone, Debug, Default)]
pub struct InMemoryPushStore {
    #[allow(clippy::type_complexity)]
    pushers: Arc<RwLock<HashMap<(String, String, String), PusherEntry>>>,
    #[allow(clippy::type_complexity)]
    push_rules: Arc<RwLock<HashMap<(String, String, String, String), PushRuleEntry>>>,
}

impl InMemoryPushStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl PushStoreApi for InMemoryPushStore {
    async fn get_pushers(
        &self,
        _user_id: &str,
        _device_id: Option<&str>,
    ) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
        unimplemented!("in-memory mock does not support raw-row method get_pushers")
    }

    #[allow(clippy::too_many_arguments)]
    async fn upsert_pusher(
        &self,
        user_id: &str,
        device_id: &str,
        pushkey: &str,
        kind: &str,
        app_id: &str,
        app_display_name: &str,
        device_display_name: &str,
        profile_tag: &Option<String>,
        lang: &str,
        data: &Option<Value>,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        self.pushers.write().await.insert(
            (user_id.to_string(), device_id.to_string(), pushkey.to_string()),
            PusherEntry {
                kind: kind.to_string(),
                app_id: app_id.to_string(),
                app_display_name: app_display_name.to_string(),
                device_display_name: device_display_name.to_string(),
                profile_tag: profile_tag.clone(),
                lang: lang.to_string(),
                data: data.clone(),
                updated_ts: now,
            },
        );
        Ok(())
    }

    async fn delete_pusher(&self, user_id: &str, device_id: &str, pushkey: &str) -> Result<(), sqlx::Error> {
        self.pushers.write().await.remove(&(user_id.to_string(), device_id.to_string(), pushkey.to_string()));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn upsert_push_rule(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        pattern: &Option<String>,
        conditions: &Option<Value>,
        actions: &Value,
        _now: i64,
    ) -> Result<(), sqlx::Error> {
        let key = (user_id.to_string(), scope.to_string(), kind.to_string(), rule_id.to_string());
        let mut rules = self.push_rules.write().await;
        match rules.get_mut(&key) {
            Some(existing) => {
                // ON CONFLICT DO UPDATE only touches pattern/conditions/actions.
                existing.pattern = pattern.clone();
                existing.conditions = conditions.clone();
                existing.actions = actions.clone();
            }
            None => {
                rules.insert(
                    key,
                    PushRuleEntry {
                        pattern: pattern.clone(),
                        conditions: conditions.clone(),
                        actions: actions.clone(),
                        is_enabled: true,
                    },
                );
            }
        }
        Ok(())
    }

    async fn delete_push_rule(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let removed = self
            .push_rules
            .write()
            .await
            .remove(&(user_id.to_string(), scope.to_string(), kind.to_string(), rule_id.to_string()))
            .is_some();
        Ok(if removed { 1 } else { 0 })
    }

    async fn update_push_rule_actions(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        actions: &Value,
    ) -> Result<(), sqlx::Error> {
        if let Some(entry) = self.push_rules.write().await.get_mut(&(
            user_id.to_string(),
            scope.to_string(),
            kind.to_string(),
            rule_id.to_string(),
        )) {
            entry.actions = actions.clone();
        }
        Ok(())
    }

    async fn get_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<Option<bool>, sqlx::Error> {
        Ok(self
            .push_rules
            .read()
            .await
            .get(&(user_id.to_string(), scope.to_string(), kind.to_string(), rule_id.to_string()))
            .map(|entry| entry.is_enabled))
    }

    async fn set_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        enabled: bool,
    ) -> Result<(), sqlx::Error> {
        if let Some(entry) = self.push_rules.write().await.get_mut(&(
            user_id.to_string(),
            scope.to_string(),
            kind.to_string(),
            rule_id.to_string(),
        )) {
            entry.is_enabled = enabled;
        }
        Ok(())
    }

    async fn get_user_push_rules(
        &self,
        _user_id: &str,
        _scope: &str,
        _kind: &str,
    ) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
        unimplemented!("in-memory mock does not support raw-row method get_user_push_rules")
    }

    async fn get_notifications(&self, _user_id: &str, _limit: i64) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
        unimplemented!("in-memory mock does not support raw-row method get_notifications")
    }

    async fn ack_notification(
        &self,
        _id: i64,
        _user_id: &str,
        _now: i64,
    ) -> Result<Option<sqlx::postgres::PgRow>, sqlx::Error> {
        unimplemented!("in-memory mock does not support raw-row method ack_notification")
    }
}
