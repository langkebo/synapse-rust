use serde_json::{json, Value};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;
use synapse_storage::push::{NotificationPageQuery, NotificationRow, PushRuleScopedRow, PushStoreApi};

/// Maximum UTF-8 byte length of a push rule ID. Mirrors upstream Synapse's
/// `push_rules.rule_id_length` (ELEMENTSEC-2026-1071).
const MAX_PUSH_RULE_ID_LEN: usize = 300;

/// Maximum combined UTF-8 byte size of a rule's serialised `conditions` +
/// `actions`. Mirrors upstream Synapse's `push_rules.rule_size`.
const MAX_PUSH_RULE_SIZE: usize = 1024;

/// Maximum number of push rules a single user may store. Mirrors upstream
/// Synapse's `push_rules.rule_count`.
const MAX_PUSH_RULES_PER_USER: i64 = 10_000;

/// Page size used by `GET /_matrix/client/v3/notifications` when the caller
/// omits `limit` (Matrix Client-Server API default).
pub const DEFAULT_NOTIFICATIONS_LIMIT: i64 = 50;

/// Upper bound on one notifications page. Requests above it are clamped rather
/// than rejected, matching upstream Synapse's behaviour for oversized limits.
pub const MAX_NOTIFICATIONS_LIMIT: i64 = 500;

/// One page of a user's notification inbox.
#[derive(Debug, Clone)]
pub struct NotificationsPage {
    /// The `notifications` array of the response body.
    pub notifications: Vec<Value>,
    /// Cursor for the next page: the `next_token` of the response. `None` means
    /// this is the last page — callers must not loop on an absent token.
    pub next_token: Option<String>,
}

/// The `only` filters of `GET /_matrix/client/v3/notifications`.
///
/// Only the three enumerable filters are supported. The remaining Matrix
/// filters (`highlight`, `is_encrypted`, `is_one_to_one`,
/// `contains_user_name:`) are rejected with 400 instead of being silently
/// ignored: answering with an unfiltered page would make a caller believe the
/// filter matched.
#[derive(Debug, Clone, Default)]
pub struct NotificationFilters {
    /// `only=room_id:<room id>` — repeated values are OR-ed.
    pub room_ids: Vec<String>,
    /// `only=type:<event type>` — repeated values are OR-ed.
    pub event_types: Vec<String>,
    /// `only=sender:<user id>` — repeated values are OR-ed.
    pub senders: Vec<String>,
}

impl NotificationFilters {
    /// Parses the repeatable `only` query parameter.
    ///
    /// # Errors
    ///
    /// Returns 400 `M_INVALID_PARAM` for a value without `:`, an empty value,
    /// or an unsupported filter key.
    pub fn from_only_params(only: &[String]) -> Result<Self, ApiError> {
        let mut filters = Self::default();
        for raw in only {
            let (key, value) = raw
                .split_once(':')
                .ok_or_else(|| ApiError::invalid_param(format!("`only` filter `{raw}` must be `<key>:<value>`")))?;
            if value.is_empty() {
                return Err(ApiError::invalid_param(format!("`only` filter `{key}` has an empty value")));
            }
            match key {
                "room_id" => filters.room_ids.push(value.to_owned()),
                "type" => filters.event_types.push(value.to_owned()),
                "sender" => filters.senders.push(value.to_owned()),
                other => {
                    return Err(ApiError::invalid_param(format!(
                        "unsupported `only` filter `{other}`; this server supports \
                         `room_id:`, `type:` and `sender:`"
                    )));
                }
            }
        }
        Ok(filters)
    }
}

/// Normalises the `limit` of a notifications request.
fn normalize_notifications_limit(limit: Option<i64>) -> Result<i64, ApiError> {
    match limit {
        None => Ok(DEFAULT_NOTIFICATIONS_LIMIT),
        Some(limit) if limit < 0 => Err(ApiError::invalid_param("`limit` must not be negative")),
        Some(limit) => Ok(limit.min(MAX_NOTIFICATIONS_LIMIT)),
    }
}

/// Parses a `from` cursor produced by [`NotificationsPage::next_token`].
fn parse_notification_cursor(from: &str) -> Result<(i64, i64), ApiError> {
    let invalid = || {
        ApiError::invalid_param(format!("invalid `from` token `{from}`; pass back the `next_token` of a previous page"))
    };
    let (ts, id) = from.split_once('_').ok_or_else(invalid)?;
    let ts = ts.parse::<i64>().map_err(|_| invalid())?;
    let id = id.parse::<i64>().map_err(|_| invalid())?;
    Ok((ts, id))
}

/// Serialises one inbox row as the Matrix notification object.
fn notification_json(row: &NotificationRow) -> Value {
    json!({
        "notification_id": row.id,
        "event_id": row.event_id,
        "room_id": row.room_id,
        "ts": row.ts,
        // D-62 修法①：`profile_tag` 取自同名真列（规范语义："命中的推送规则的
        // profile tag"），`notification_type` 作为**独立键**并列暴露 —— 信息是
        // 旧形状的超集，不再用错了语义的键承载通知类型。
        "profile_tag": row.profile_tag,
        "notification_type": row.notification_type,
        "read": row.is_read.unwrap_or(false)
    })
}

/// Combined UTF-8 byte size of a rule's serialised `conditions` + `actions`,
/// the quantity bounded by [`MAX_PUSH_RULE_SIZE`].
///
/// Mirrors upstream Synapse's `_push_rule_size_for_limits`: the length of the
/// compact JSON encoding, not a pretty-printed one. An absent (`None`)
/// `conditions` value contributes zero bytes.
fn push_rule_size(conditions: Option<&Value>, actions: &Value) -> usize {
    let conditions_len = conditions.map(|c| c.to_string().len()).unwrap_or(0);
    conditions_len + actions.to_string().len()
}

/// Enforce the per-rule `rule_id` length and `conditions` + `actions` size
/// caps, returning a 413 `M_TOO_LARGE` on violation.
fn ensure_push_rule_within_limits(rule_id: &str, conditions: Option<&Value>, actions: &Value) -> Result<(), ApiError> {
    if rule_id.len() > MAX_PUSH_RULE_ID_LEN {
        return Err(ApiError::too_large(format!(
            "Push rule ID length {} exceeds the limit of {MAX_PUSH_RULE_ID_LEN}",
            rule_id.len()
        )));
    }
    let size = push_rule_size(conditions, actions);
    if size > MAX_PUSH_RULE_SIZE {
        return Err(ApiError::too_large(format!("Push rule size {size} exceeds the limit of {MAX_PUSH_RULE_SIZE}")));
    }
    Ok(())
}

/// Map one `push_rules` row to its Matrix JSON representation.
///
/// Shared by the per-kind reader and the full-document builder so the two
/// cannot drift apart.
fn rule_to_json(
    rule_id: String,
    is_default: bool,
    is_enabled: bool,
    pattern: Option<String>,
    conditions: Option<Value>,
    actions: Option<Value>,
) -> Value {
    json!({
        "rule_id": rule_id,
        "default": is_default,
        "enabled": is_enabled,
        "pattern": pattern,
        "conditions": conditions,
        "actions": actions.unwrap_or_else(|| json!([]))
    })
}

/// Rebuild the `GET /pushrules` document from persisted rows.
///
/// Shape: `{"global": {<kind>: [rule, ...]}, "devices": {<device_id>: {<kind>: [rule, ...]}}}`.
/// The rows arrive pre-sorted (`scope, kind, rule_id`), so a single pass preserves
/// the deterministic within-kind order the spec requires.
///
/// Only `global` and `device/<device_id>` scopes are recognised — the same
/// constraint the HTTP layer enforces (`get_push_rules_scope` rejects anything
/// else). Rows with any other `scope` are skipped rather than inventing a bucket.
///
/// Shared with the sync service, which emits the same document as the synced
/// `m.push_rules` account-data event.
pub(crate) fn rules_to_content(rows: Vec<PushRuleScopedRow>) -> Value {
    let mut content = json!({"global": {}, "devices": {}});
    for row in rows {
        let bucket = if row.scope == "global" {
            content["global"].as_object_mut()
        } else if let Some(device_id) = row.scope.strip_prefix("device/") {
            content["devices"]
                .as_object_mut()
                .and_then(|devices| devices.entry(device_id.to_string()).or_insert_with(|| json!({})).as_object_mut())
        } else {
            continue;
        };
        let Some(bucket) = bucket else { continue };
        let rule = rule_to_json(row.rule_id, row.is_default, row.is_enabled, row.pattern, row.conditions, row.actions);
        if let Some(rules) = bucket.entry(row.kind).or_insert_with(|| json!([])).as_array_mut() {
            rules.push(rule);
        }
    }
    content
}

#[derive(Debug, Clone)]
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
    push_storage: Arc<dyn PushStoreApi>,
}

impl ClientPushService {
    /// See [`new`].
    pub fn new(push_storage: Arc<dyn PushStoreApi>) -> Self {
        Self { push_storage }
    }

    /// See [`get_pushers`].
    pub async fn get_pushers(&self, user_id: &str, device_id: Option<&str>) -> Result<Vec<Value>, ApiError> {
        let pushers = self
            .push_storage
            .get_pushers(user_id, device_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(pushers
            .into_iter()
            .map(|row| {
                let data = row.data.unwrap_or_else(|| json!({}));
                json!({
                    "pushkey": row.pushkey,
                    "kind": row.kind,
                    "app_id": row.app_id,
                    "app_display_name": row.app_display_name,
                    "device_display_name": row.device_display_name,
                    "profile_tag": row.profile_tag,
                    "lang": row.lang,
                    "data": data
                })
            })
            .collect())
    }

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
            .map_err(|e| ApiError::internal_with_cause("Failed to save pusher", e))?;
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
    pub async fn delete_pusher(&self, user_id: &str, device_id: &str, pushkey: &str) -> Result<(), ApiError> {
        self.push_storage
            .delete_pusher(user_id, device_id, pushkey)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete pusher", e))?;
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
    ///
    /// The `push_rules` table is the single authority for a user's rules: both the
    /// per-rule CRUD endpoints and this full-document reader operate on it. An empty
    /// table yields `None` so callers fall back to the spec defaults.
    pub async fn get_push_rules_content(&self, user_id: &str) -> Result<Option<Value>, ApiError> {
        let rows = self
            .push_storage
            .get_all_push_rules(user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get push rules", e))?;
        if rows.is_empty() {
            return Ok(None);
        }
        Ok(Some(rules_to_content(rows)))
    }

    /// See [`get_user_push_rules`].
    pub async fn get_user_push_rules(&self, user_id: &str, scope: &str, kind: &str) -> Result<Vec<Value>, ApiError> {
        let rules = self
            .push_storage
            .get_user_push_rules(user_id, scope, kind)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(rules
            .into_iter()
            .map(|row| {
                rule_to_json(row.rule_id, row.is_default, row.is_enabled, row.pattern, row.conditions, row.actions)
            })
            .collect())
    }

    /// See [`upsert_push_rule`].
    pub async fn upsert_push_rule(&self, request: UpsertPushRuleRequest) -> Result<i64, ApiError> {
        // ELEMENTSEC-2026-1071: bound rule count and per-rule size before touching
        // storage, so an abusive client cannot exhaust disk/memory. The cheap
        // in-memory checks run first; the per-user count (one indexed `COUNT(*)`)
        // runs last so it is only paid when the request is otherwise well-formed.
        ensure_push_rule_within_limits(&request.rule_id, request.conditions.as_ref(), &request.actions)?;
        let existing = self
            .push_storage
            .count_push_rules_excluding(&request.user_id, &request.scope, &request.kind, &request.rule_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to count push rules", e))?;
        if existing >= MAX_PUSH_RULES_PER_USER {
            return Err(ApiError::too_large(format!(
                "Push rule count {existing} exceeds the limit of {MAX_PUSH_RULES_PER_USER}"
            )));
        }
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
            .map_err(|e| ApiError::internal_with_cause("Failed to save push rule", e))?;
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
            .map_err(|e| ApiError::internal_with_cause("Failed to delete push rule", e))?;
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
        // ELEMENTSEC-2026-1071: replacing a rule's actions changes its total size,
        // so re-check the cap against the stored conditions. A missing rule is a
        // silent no-op (unchanged behaviour), hence the check is skipped for it.
        let rules = self
            .push_storage
            .get_user_push_rules(user_id, scope, kind)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get push rules", e))?;
        if let Some(row) = rules.iter().find(|row| row.rule_id == rule_id) {
            ensure_push_rule_within_limits(rule_id, row.conditions.as_ref(), actions)?;
        }
        self.push_storage
            .update_push_rule_actions(user_id, scope, kind, rule_id, actions)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to update push rule actions", e))?;
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
            .map_err(|e| ApiError::internal_with_cause("Database error", e))
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
            .map_err(|e| ApiError::internal_with_cause("Failed to update push rule enabled", e))?;
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

    /// One page of `GET /_matrix/client/v3/notifications`.
    ///
    /// `from` is the opaque `next_token` of a previous page; an absent or empty
    /// value starts at the newest row. `filters` comes from
    /// [`NotificationFilters::from_only_params`].
    ///
    /// The response cursor is derived from one extra probe row: the store is
    /// asked for `limit + 1` rows and the extra row is dropped, so `next_token`
    /// is `Some` exactly when another page exists. A caller that loops until it
    /// sees `null` therefore never misses rows, and never loops forever.
    pub async fn get_notifications_page(
        &self,
        user_id: &str,
        limit: Option<i64>,
        from: Option<&str>,
        filters: NotificationFilters,
    ) -> Result<NotificationsPage, ApiError> {
        let limit = normalize_notifications_limit(limit)?;
        let cursor = match from {
            Some(from) if !from.is_empty() => Some(parse_notification_cursor(from)?),
            _ => None,
        };

        let query = NotificationPageQuery {
            // Clamped to <= MAX_NOTIFICATIONS_LIMIT above, so `+ 1` cannot overflow.
            limit: limit + 1,
            cursor,
            room_ids: filters.room_ids,
            event_types: filters.event_types,
            senders: filters.senders,
        };

        let mut rows = self
            .push_storage
            .get_notifications(user_id, &query)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        let has_more = i64::try_from(rows.len()).unwrap_or(i64::MAX) > limit;
        rows.truncate(usize::try_from(limit).unwrap_or(0));
        let next_token = if has_more { rows.last().map(|row| format!("{}_{}", row.ts, row.id)) } else { None };

        Ok(NotificationsPage { notifications: rows.iter().map(notification_json).collect(), next_token })
    }

    /// See [`get_notifications_page`].
    ///
    /// Convenience wrapper for internal callers that want the first page with no
    /// filters and do not paginate.
    pub async fn get_notifications(&self, user_id: &str, limit: i64) -> Result<Vec<Value>, ApiError> {
        Ok(self.get_notifications_page(user_id, Some(limit), None, NotificationFilters::default()).await?.notifications)
    }

    /// See [`ack_notification`].
    pub async fn ack_notification(&self, notification_id: i64, user_id: &str) -> Result<bool, ApiError> {
        let result = self
            .push_storage
            .ack_notification(notification_id, user_id, current_timestamp_millis())
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to ack notification", e))?;
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
    //! Tested via the in-memory mock store (`InMemoryPushStore`) so the suite runs
    //! without a real PostgreSQL pool. The four methods that read raw
    //! `sqlx::postgres::PgRow` values from the trait (`get_pushers`, `get_user_push_rules`)
    //! are now typed rows (`PusherRow` / `PushRuleRow`) and **are** exercised here;
    //! `get_notifications` / `ack_notification` are exercised against a real
    //! database by the storage-level `db_tests` and the integration suite.

    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_mocks::InMemoryPushStore;
    use serde_json::json;
    use synapse_common::MatrixErrorCode;

    fn build_service() -> ClientPushService {
        ClientPushService::new(Arc::new(InMemoryPushStore::new()))
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
            svc.upsert_pusher(pusher_req("@bob:example.com", "tok_bob", &format!("https://push.example.com/v{i}")))
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
        svc.upsert_pusher(pusher_req("@carol:example.com", "tok_carol", "https://push.example.com/v1")).await.unwrap();
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
        svc.upsert_push_rule(rule_req("@eve:example.com", ".m.rule.eve", json!([{"kind": "notify"}]))).await.unwrap();
        // Overwrite with disabled notification — must not error.
        svc.upsert_push_rule(rule_req("@eve:example.com", ".m.rule.eve", json!([{"kind": "dont_notify"}])))
            .await
            .expect("overwrite via ON CONFLICT must succeed");
    }

    #[tokio::test]
    async fn test_upsert_push_rule_rejects_oversized_rule_id() {
        // ELEMENTSEC-2026-1071: a rule ID longer than 300 bytes is rejected.
        let svc = build_service();
        let long_id = "a".repeat(MAX_PUSH_RULE_ID_LEN + 1);
        let err = svc
            .upsert_push_rule(rule_req("@mallory:example.com", &long_id, json!([{"kind": "notify"}])))
            .await
            .expect_err("oversized rule ID must be rejected");
        assert!(err.code_is(MatrixErrorCode::TooLarge), "must be 413 M_TOO_LARGE, got {err:?}");
    }

    #[tokio::test]
    async fn test_upsert_push_rule_accepts_rule_id_at_limit() {
        let svc = build_service();
        let max_id = "a".repeat(MAX_PUSH_RULE_ID_LEN);
        svc.upsert_push_rule(rule_req("@mallory:example.com", &max_id, json!([{"kind": "notify"}])))
            .await
            .expect("rule ID exactly at the limit must be accepted");
    }

    #[tokio::test]
    async fn test_upsert_push_rule_rejects_oversized_rule() {
        // `actions` alone exceeding the 1024-byte combined cap must be rejected.
        let svc = build_service();
        let fat_actions = json!([{"kind": "notify", "blob": "x".repeat(MAX_PUSH_RULE_SIZE)}]);
        let err = svc
            .upsert_push_rule(rule_req("@mallory:example.com", ".m.rule.fat", fat_actions))
            .await
            .expect_err("oversized rule must be rejected");
        assert!(err.code_is(MatrixErrorCode::TooLarge), "must be 413 M_TOO_LARGE, got {err:?}");
    }

    #[tokio::test]
    async fn test_upsert_push_rule_accepts_rule_at_size_limit() {
        let svc = build_service();
        // Pad to exactly the limit: the serialised action list plus its two
        // enclosing brackets must total `MAX_PUSH_RULE_SIZE` bytes.
        let pad = "x".repeat(MAX_PUSH_RULE_SIZE - r#"[{"kind":"notify","blob":""}]"#.len());
        let actions = json!([{"kind": "notify", "blob": pad}]);
        assert_eq!(push_rule_size(None, &actions), MAX_PUSH_RULE_SIZE);
        svc.upsert_push_rule(rule_req("@mallory:example.com", ".m.rule.at_limit", actions))
            .await
            .expect("rule exactly at the size limit must be accepted");
    }

    #[tokio::test]
    async fn test_upsert_push_rule_rejects_rule_at_count_limit() {
        // ELEMENTSEC-2026-1071: a user at the per-user cap cannot add another rule.
        let svc = build_service();
        let user = "@quentin:example.com";
        for i in 0..MAX_PUSH_RULES_PER_USER {
            svc.upsert_push_rule(rule_req(user, &format!(".m.rule.{i}"), json!([{"kind": "notify"}])))
                .await
                .expect("seeding rules below the cap must succeed");
        }
        let err = svc
            .upsert_push_rule(rule_req(user, ".m.rule.one_too_many", json!([{"kind": "notify"}])))
            .await
            .expect_err("adding a rule past the cap must be rejected");
        assert!(err.code_is(MatrixErrorCode::TooLarge), "must be 413 M_TOO_LARGE, got {err:?}");
    }

    #[tokio::test]
    async fn test_upsert_push_rule_allows_updating_existing_rule_at_count_limit() {
        // The rule being written is excluded from the count, so re-saving an
        // existing rule (an update, not an insert) stays allowed at the cap.
        let svc = build_service();
        let user = "@rhea:example.com";
        for i in 0..MAX_PUSH_RULES_PER_USER {
            svc.upsert_push_rule(rule_req(user, &format!(".m.rule.{i}"), json!([{"kind": "notify"}])))
                .await
                .expect("seeding rules below the cap must succeed");
        }
        svc.upsert_push_rule(rule_req(user, ".m.rule.0", json!([{"kind": "dont_notify"}])))
            .await
            .expect("updating an existing rule at the cap must succeed");
    }

    #[tokio::test]
    async fn test_set_push_rule_actions_rejects_oversized_actions() {
        let svc = build_service();
        svc.upsert_push_rule(rule_req("@tina:example.com", ".m.rule.tina", json!([{"kind": "notify"}]))).await.unwrap();
        let fat_actions = json!([{"kind": "notify", "blob": "x".repeat(MAX_PUSH_RULE_SIZE)}]);
        let err = svc
            .set_push_rule_actions("@tina:example.com", "global", "room", ".m.rule.tina", &fat_actions)
            .await
            .expect_err("oversized actions must be rejected");
        assert!(err.code_is(MatrixErrorCode::TooLarge), "must be 413 M_TOO_LARGE, got {err:?}");
    }

    #[tokio::test]
    async fn test_delete_push_rule_returns_true_for_existing() {
        let svc = build_service();
        svc.upsert_push_rule(rule_req("@frank:example.com", ".m.rule.frank", json!([{"kind": "notify"}])))
            .await
            .unwrap();
        let existed = svc.delete_push_rule("@frank:example.com", "global", "room", ".m.rule.frank").await.unwrap();
        assert!(existed, "deleting existing rule must return true");
    }

    #[tokio::test]
    async fn test_delete_push_rule_returns_false_for_missing() {
        let svc = build_service();
        let existed = svc.delete_push_rule("@ghost:example.com", "global", "room", ".m.rule.absent").await.unwrap();
        assert!(!existed, "deleting missing rule must return false");
    }

    #[tokio::test]
    async fn test_set_push_rule_actions_on_existing_rule() {
        let svc = build_service();
        svc.upsert_push_rule(rule_req("@gina:example.com", ".m.rule.gina", json!([{"kind": "notify"}]))).await.unwrap();
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
        let result = svc.get_push_rule_enabled("@ivy:example.com", "global", "room", "never_existed").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_push_rule_enabled_default_true_after_upsert() {
        let svc = build_service();
        svc.upsert_push_rule(rule_req("@jack:example.com", ".m.rule.jack", json!([{"kind": "notify"}]))).await.unwrap();
        let enabled = svc.get_push_rule_enabled("@jack:example.com", "global", "room", ".m.rule.jack").await.unwrap();
        assert_eq!(enabled, Some(true), "freshly upserted rule must default to enabled");
    }

    #[tokio::test]
    async fn test_set_push_rule_enabled_round_trip() {
        let svc = build_service();
        svc.upsert_push_rule(rule_req("@kate:example.com", ".m.rule.kate", json!([{"kind": "notify"}]))).await.unwrap();

        // Disable
        svc.set_push_rule_enabled("@kate:example.com", "global", "room", ".m.rule.kate", false).await.unwrap();
        let enabled = svc.get_push_rule_enabled("@kate:example.com", "global", "room", ".m.rule.kate").await.unwrap();
        assert_eq!(enabled, Some(false));

        // Re-enable
        svc.set_push_rule_enabled("@kate:example.com", "global", "room", ".m.rule.kate", true).await.unwrap();
        let enabled = svc.get_push_rule_enabled("@kate:example.com", "global", "room", ".m.rule.kate").await.unwrap();
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
        assert!(content.is_none(), "an empty push_rules table should yield None (caller falls back to defaults)");
    }

    /// The full-document reader and the per-rule CRUD share one authority (the
    /// `push_rules` table): a rule written through `upsert_push_rule` is visible in
    /// the `GET /pushrules` document, grouped by scope and kind.
    #[tokio::test]
    async fn test_get_push_rules_content_rebuilds_document_from_table() {
        let svc = build_service();
        svc.upsert_push_rule(rule_req("@ross:example.com", ".m.rule.ross", json!([{"kind": "notify"}])))
            .await
            .expect("upsert global rule");
        let mut device_rule = rule_req("@ross:example.com", ".m.rule.ross.device", json!([{"kind": "dont_notify"}]));
        device_rule.scope = "device/DEVICE".to_string();
        svc.upsert_push_rule(device_rule).await.expect("upsert device rule");

        let content = svc.get_push_rules_content("@ross:example.com").await.expect("content").expect("some content");
        assert_eq!(
            content,
            json!({
                "global": {
                    "room": [{
                        "rule_id": ".m.rule.ross",
                        "default": false,
                        "enabled": true,
                        "pattern": "!room:example.com",
                        "conditions": null,
                        "actions": [{"kind": "notify"}]
                    }]
                },
                "devices": {
                    "DEVICE": {
                        "room": [{
                            "rule_id": ".m.rule.ross.device",
                            "default": false,
                            "enabled": true,
                            "pattern": "!room:example.com",
                            "conditions": null,
                            "actions": [{"kind": "dont_notify"}]
                        }]
                    }
                }
            })
        );
    }

    // ── C33: typed-row readers (previously `PgRow` ⇒ mock had to `unimplemented!()`) ──

    /// `get_pushers` 现在返回 `PusherRow`，因此这个映射**第一次**能在纯内存里被测到。
    /// 断言整份 JSON：这就是响应形状的锁定点。
    #[tokio::test]
    async fn test_get_pushers_maps_typed_row_to_json() {
        let svc = build_service();
        let mut request = pusher_req("@nina:example.com", "pk1", "https://push.example.com/v1");
        request.profile_tag = Some("iphone".to_string());
        svc.upsert_pusher(request).await.expect("upsert should succeed");

        let pushers = svc.get_pushers("@nina:example.com", Some("DEVICE")).await.expect("get_pushers");
        assert_eq!(
            pushers,
            vec![json!({
                "pushkey": "pk1",
                "kind": "http",
                "app_id": "com.example.app",
                "app_display_name": "Example",
                "device_display_name": "Device",
                "profile_tag": "iphone",
                "lang": "en",
                "data": {"url": "https://push.example.com/v1"}
            })]
        );
    }

    /// 镜像 SQL 的 `device_id IS NOT DISTINCT FROM $2`：该列 NOT NULL，故 `None` 匹配不到任何行。
    #[tokio::test]
    async fn test_get_pushers_with_none_device_id_matches_nothing() {
        let svc = build_service();
        svc.upsert_pusher(pusher_req("@omar:example.com", "pk2", "https://push.example.com/v2")).await.unwrap();
        let pushers = svc.get_pushers("@omar:example.com", None).await.expect("get_pushers with None");
        assert!(pushers.is_empty(), "device_id is NOT NULL in the table, so NULL matches nothing");
    }

    /// 同上：`get_user_push_rules` 的列 → JSON 映射首次可测。
    #[tokio::test]
    async fn test_get_user_push_rules_maps_typed_row_to_json() {
        let svc = build_service();
        svc.upsert_push_rule(rule_req("@pia:example.com", ".m.rule.pia", json!([{"kind": "notify"}])))
            .await
            .expect("upsert rule should succeed");

        let rules = svc.get_user_push_rules("@pia:example.com", "global", "room").await.expect("get rules");
        assert_eq!(
            rules,
            vec![json!({
                "rule_id": ".m.rule.pia",
                "default": false,
                "enabled": true,
                "pattern": "!room:example.com",
                "conditions": null,
                "actions": [{"kind": "notify"}]
            })]
        );
    }

    /// 内存 mock **不存**通知 ⇒ 空列表是"这个 fake 里没有通知"的忠实答案（而不是 panic）。
    #[tokio::test]
    async fn test_get_notifications_empty_without_a_notification_store() {
        let svc = build_service();
        let notifications = svc.get_notifications("@quinn:example.com", 10).await.expect("get_notifications");
        assert!(notifications.is_empty());
        assert!(!svc.ack_notification(4242, "@quinn:example.com").await.expect("ack_notification"));
    }

    // ── B-7: `/notifications` 分页与 `only` 过滤 ────────────────────────────

    fn build_service_with_store() -> (ClientPushService, Arc<InMemoryPushStore>) {
        let store = Arc::new(InMemoryPushStore::new());
        (ClientPushService::new(store.clone()), store)
    }

    async fn seed_three(store: &InMemoryPushStore) {
        for (event_id, room_id, ts) in [
            ("$e1", "!r1:example.com", 1_000_i64),
            ("$e2", "!r2:example.com", 2_000),
            ("$e3", "!r1:example.com", 3_000),
        ] {
            store
                .record_notification("@quinn:example.com", Some(event_id), Some(room_id), "highlight", ts)
                .await
                .expect("seed notification");
        }
    }

    #[test]
    fn test_from_only_params_parses_supported_filters() {
        let filters = NotificationFilters::from_only_params(&[
            "room_id:!r1:example.com".to_string(),
            "room_id:!r2:example.com".to_string(),
            "type:m.room.message".to_string(),
            "sender:@alice:example.com".to_string(),
        ])
        .expect("supported filters must parse");
        assert_eq!(filters.room_ids, vec!["!r1:example.com", "!r2:example.com"]);
        assert_eq!(filters.event_types, vec!["m.room.message"]);
        assert_eq!(filters.senders, vec!["@alice:example.com"]);
    }

    #[test]
    fn test_from_only_params_rejects_missing_colon() {
        let err = NotificationFilters::from_only_params(&["highlight".to_string()])
            .expect_err("a filter without `:` must be rejected");
        assert!(err.code_is(MatrixErrorCode::InvalidParam), "must be 400 M_INVALID_PARAM, got {err:?}");
    }

    #[test]
    fn test_from_only_params_rejects_empty_value() {
        let err = NotificationFilters::from_only_params(&["type:".to_string()])
            .expect_err("an empty filter value must be rejected");
        assert!(err.code_is(MatrixErrorCode::InvalidParam), "must be 400 M_INVALID_PARAM, got {err:?}");
    }

    /// 未实现的过滤器必须 400，而不是静默返回未过滤的一页。
    #[test]
    fn test_from_only_params_rejects_unsupported_key() {
        let err = NotificationFilters::from_only_params(&["highlight:true".to_string()])
            .expect_err("unsupported filters must be rejected, not ignored");
        assert!(err.code_is(MatrixErrorCode::InvalidParam), "must be 400 M_INVALID_PARAM, got {err:?}");
    }

    #[tokio::test]
    async fn test_notifications_page_reports_next_token_only_when_more_rows_exist() {
        let (svc, store) = build_service_with_store();
        seed_three(&store).await;

        let first = svc
            .get_notifications_page("@quinn:example.com", Some(2), None, NotificationFilters::default())
            .await
            .unwrap();
        assert_eq!(first.notifications.len(), 2);
        assert_eq!(first.notifications[0]["event_id"], "$e3");
        assert_eq!(first.notifications[1]["event_id"], "$e2");
        // 游标是页内最后一行，而不是"还有更多"之外的东西。
        assert_eq!(first.next_token.as_deref(), Some("2000_2"));

        let second = svc
            .get_notifications_page(
                "@quinn:example.com",
                Some(2),
                first.next_token.as_deref(),
                NotificationFilters::default(),
            )
            .await
            .unwrap();
        assert_eq!(second.notifications.len(), 1);
        assert_eq!(second.notifications[0]["event_id"], "$e1");
        assert_eq!(second.next_token, None, "last page must not advertise a cursor");
    }

    #[tokio::test]
    async fn test_notifications_page_exact_page_size_has_no_next_token() {
        let (svc, store) = build_service_with_store();
        seed_three(&store).await;

        let page = svc
            .get_notifications_page("@quinn:example.com", Some(3), None, NotificationFilters::default())
            .await
            .unwrap();
        assert_eq!(page.notifications.len(), 3);
        assert_eq!(page.next_token, None, "limit == row count must not fabricate a next page");
    }

    #[tokio::test]
    async fn test_notifications_page_filters_by_room_id() {
        let (svc, store) = build_service_with_store();
        seed_three(&store).await;

        let filters = NotificationFilters::from_only_params(&["room_id:!r1:example.com".to_string()]).unwrap();
        let page = svc.get_notifications_page("@quinn:example.com", Some(10), None, filters).await.unwrap();
        let event_ids: Vec<_> = page.notifications.iter().map(|n| n["event_id"].as_str().unwrap()).collect();
        assert_eq!(event_ids, vec!["$e3", "$e1"]);
        assert_eq!(page.next_token, None);
    }

    #[tokio::test]
    async fn test_notifications_page_rejects_invalid_cursor() {
        let svc = build_service();
        let err = svc
            .get_notifications_page(
                "@quinn:example.com",
                Some(10),
                Some("not-a-cursor"),
                NotificationFilters::default(),
            )
            .await
            .expect_err("a malformed `from` must be rejected");
        assert!(err.code_is(MatrixErrorCode::InvalidParam), "must be 400 M_INVALID_PARAM, got {err:?}");
    }

    #[tokio::test]
    async fn test_notifications_page_rejects_negative_limit() {
        let svc = build_service();
        let err = svc
            .get_notifications_page("@quinn:example.com", Some(-1), None, NotificationFilters::default())
            .await
            .expect_err("a negative `limit` must be rejected");
        assert!(err.code_is(MatrixErrorCode::InvalidParam), "must be 400 M_INVALID_PARAM, got {err:?}");
    }

    #[tokio::test]
    async fn test_notifications_page_empty_from_is_first_page() {
        let (svc, store) = build_service_with_store();
        seed_three(&store).await;
        let page = svc
            .get_notifications_page("@quinn:example.com", Some(2), Some(""), NotificationFilters::default())
            .await
            .unwrap();
        assert_eq!(page.notifications[0]["event_id"], "$e3");
        assert_eq!(page.next_token.as_deref(), Some("2000_2"));
    }
}
