use super::*;

use serde_json::Value;

use crate::push::{NotificationRow, PushRuleRow, PushStoreApi, PusherRow};

/// Stored push-rule state for the in-memory mock, mirroring the mutable columns
/// of the `push_rules` table that the typed trait methods touch.
#[derive(Clone, Debug)]
struct PushRuleEntry {
    pattern: Option<String>,
    conditions: Option<Value>,
    actions: Value,
    is_enabled: bool,
    /// Mirrors the table's `is_default`; the INSERT always writes `false`.
    is_default: bool,
}

/// Stored pusher state for the in-memory mock.
#[derive(Clone, Debug)]
#[allow(dead_code)] // `updated_ts` mirrors the table but `get_pushers` (typed) does not surface it.
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
/// Faithfully implements the pusher/push-rule methods with `HashMap` storage,
/// including the two typed readers (`get_pushers`, `get_user_push_rules`) —
/// since C33 they return `PusherRow` / `PushRuleRow` instead of a raw
/// `sqlx::postgres::PgRow`, so they no longer have to be `unimplemented!()`.
///
/// Notifications are **not** stored by this mock (there is no notification
/// fixture API); `get_notifications` therefore reports an empty list and
/// `ack_notification` reports "nothing acked", which is the faithful answer for
/// a fake with no notifications rather than a panic.
#[derive(Clone, Debug, Default)]
pub struct InMemoryPushStore {
    #[allow(clippy::type_complexity)]
    pushers: Arc<RwLock<HashMap<(String, String, String), PusherEntry>>>,
    #[allow(clippy::type_complexity)]
    push_rules: Arc<RwLock<HashMap<(String, String, String, String), PushRuleEntry>>>,
}

impl InMemoryPushStore {
    /// See [`new`].
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl PushStoreApi for InMemoryPushStore {
    async fn get_pushers(&self, user_id: &str, device_id: Option<&str>) -> Result<Vec<PusherRow>, sqlx::Error> {
        // Mirrors `WHERE user_id = $1 AND device_id IS NOT DISTINCT FROM $2`:
        // `device_id` is part of the key and non-null, so `None` matches nothing.
        let Some(device_id) = device_id else {
            return Ok(Vec::new());
        };
        let pushers = self.pushers.read().await;
        let mut rows: Vec<PusherRow> = pushers
            .iter()
            .filter(|((user, device, _), _)| user == user_id && device == device_id)
            .map(|((_, _, pushkey), entry)| PusherRow {
                pushkey: pushkey.clone(),
                kind: entry.kind.clone(),
                app_id: entry.app_id.clone(),
                app_display_name: entry.app_display_name.clone(),
                device_display_name: entry.device_display_name.clone(),
                profile_tag: entry.profile_tag.clone(),
                lang: entry.lang.clone(),
                data: entry.data.clone(),
                device_id: device_id.to_string(),
            })
            .collect();
        // Mirrors `ORDER BY created_ts DESC, pushkey ASC` (the mock keeps no
        // created_ts; ordering by pushkey keeps the result deterministic).
        rows.sort_by(|a, b| a.pushkey.cmp(&b.pushkey));
        Ok(rows)
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
                        is_default: false,
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
        user_id: &str,
        scope: &str,
        kind: &str,
    ) -> Result<Vec<PushRuleRow>, sqlx::Error> {
        let rules = self.push_rules.read().await;
        let mut rows: Vec<PushRuleRow> = rules
            .iter()
            .filter(|((user, rule_scope, rule_kind, _), _)| user == user_id && rule_scope == scope && rule_kind == kind)
            .map(|((_, _, _, rule_id), entry)| PushRuleRow {
                rule_id: rule_id.clone(),
                pattern: entry.pattern.clone(),
                conditions: entry.conditions.clone(),
                actions: Some(entry.actions.clone()),
                is_enabled: entry.is_enabled,
                is_default: entry.is_default,
            })
            .collect();
        // Mirrors `ORDER BY rule_id ASC`.
        rows.sort_by(|a, b| a.rule_id.cmp(&b.rule_id));
        Ok(rows)
    }

    async fn get_notifications(&self, _user_id: &str, _limit: i64) -> Result<Vec<NotificationRow>, sqlx::Error> {
        Ok(Vec::new())
    }

    async fn ack_notification(&self, _id: i64, _user_id: &str, _now: i64) -> Result<Option<i64>, sqlx::Error> {
        Ok(None)
    }
}
