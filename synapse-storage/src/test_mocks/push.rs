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
/// Notifications are stored in a `Vec` since D-68 wired
/// [`PushStoreApi::record_notification`]: a fake that silently dropped the write
/// would make the "the push decision is recorded" behaviour untestable off-DB
/// (the same reason the C33 readers stopped being `unimplemented!()`).
#[derive(Clone, Debug, Default)]
pub struct InMemoryPushStore {
    #[allow(clippy::type_complexity)]
    pushers: Arc<RwLock<HashMap<(String, String, String), PusherEntry>>>,
    #[allow(clippy::type_complexity)]
    push_rules: Arc<RwLock<HashMap<(String, String, String, String), PushRuleEntry>>>,
    notifications: Arc<RwLock<Vec<NotificationRow>>>,
    next_notification_id: Arc<RwLock<i64>>,
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

    /// Mirrors the real `record_notification` (D-68): deduplicated per user while the
    /// row is unread, never deduplicated when `event_id` is absent, and it never
    /// overwrites `profile_tag` (this mock has no push-rule engine).
    async fn record_notification(
        &self,
        _user_id: &str,
        event_id: Option<&str>,
        room_id: Option<&str>,
        notification_type: &str,
        ts: i64,
    ) -> Result<(), sqlx::Error> {
        let mut notifications = self.notifications.write().await;
        if let Some(event_id) = event_id {
            let already_unread = notifications
                .iter()
                .any(|row| row.event_id.as_deref() == Some(event_id) && !row.is_read.unwrap_or(false));
            if already_unread {
                return Ok(());
            }
        }
        let mut next_id = self.next_notification_id.write().await;
        *next_id += 1;
        notifications.push(NotificationRow {
            id: *next_id,
            event_id: event_id.map(str::to_owned),
            room_id: room_id.map(str::to_owned),
            ts,
            notification_type: Some(notification_type.to_owned()),
            profile_tag: None,
            is_read: Some(false),
        });
        Ok(())
    }

    async fn get_notifications(&self, _user_id: &str, _limit: i64) -> Result<Vec<NotificationRow>, sqlx::Error> {
        let mut rows = self.notifications.read().await.clone();
        // Mirrors `ORDER BY ts DESC`. This mock has a single implicit user (the
        // fixture API never scopes notifications by user).
        rows.sort_by(|a, b| b.ts.cmp(&a.ts));
        Ok(rows)
    }

    async fn ack_notification(&self, id: i64, _user_id: &str, _now: i64) -> Result<Option<i64>, sqlx::Error> {
        let mut notifications = self.notifications.write().await;
        match notifications.iter_mut().find(|row| row.id == id) {
            Some(row) => {
                row.is_read = Some(true);
                Ok(Some(id))
            }
            None => Ok(None),
        }
    }
}
