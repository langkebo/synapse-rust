use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
#[cfg(test)]
use synapse_common::current_timestamp_millis;

// Push domain group — re-exports push_notification types under `push::`.
// Consumers should prefer `synapse_storage::push::PushNotificationStorage`
// over the flat `synapse_storage::PushNotificationStorage`.
pub use crate::push_notification::{
    CreateNotificationLogRequest, PushDevice, PushNotificationLog, PushNotificationQueue, PushNotificationStorage,
    QueueNotificationRequest, RegisterDeviceRequest, RoomNotification,
};

/// A `pushers` row as returned by [`PushStorage::get_pushers`].
///
/// Introduced in C33 so the storage layer stops leaking `sqlx::postgres::PgRow`
/// to callers that then had to decode columns by name at runtime.
#[derive(Debug, Clone)]
pub struct PusherRow {
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
    /// The `lang` field. Asserted non-null: `upsert_pusher` is the only writer and
    /// always binds it; the column's `DEFAULT 'en'` covers inserts that omit it.
    pub lang: String,
    /// The `data` field.
    pub data: Option<Value>,
    /// The `device_id` field.
    pub device_id: String,
}

/// A `push_rules` row as returned by [`PushStorage::get_user_push_rules`].
#[derive(Debug, Clone)]
pub struct PushRuleRow {
    /// The `rule_id` field.
    pub rule_id: String,
    /// The `pattern` field.
    pub pattern: Option<String>,
    /// The `conditions` field.
    pub conditions: Option<Value>,
    /// The `actions` field.
    pub actions: Option<Value>,
    /// The `is_enabled` field. Asserted non-null: written as a literal `true` by the
    /// INSERT and as a bound `bool` by `set_push_rule_enabled`.
    pub is_enabled: bool,
    /// The `is_default` field. Asserted non-null: the INSERT always writes literal `false`.
    pub is_default: bool,
}

/// A `push_rules` row carrying its `scope`/`kind` classification, as returned by
/// [`PushStorage::get_all_push_rules`]. Unlike [`PushRuleRow`] (which is already
/// scoped by the caller's `scope`/`kind` arguments), this row is the input needed
/// to rebuild the full `GET /pushrules` document.
#[derive(Debug, Clone)]
pub struct PushRuleScopedRow {
    /// The `scope` field (`global` or `device/<device_id>`).
    pub scope: String,
    /// The `kind` field (`override` / `content` / `room` / `sender` / `underride`).
    pub kind: String,
    /// The `rule_id` field.
    pub rule_id: String,
    /// The `pattern` field.
    pub pattern: Option<String>,
    /// The `conditions` field.
    pub conditions: Option<Value>,
    /// The `actions` field.
    pub actions: Option<Value>,
    /// The `is_enabled` field. Asserted non-null: written as a literal `true` by the
    /// INSERT and as a bound `bool` by `set_push_rule_enabled`.
    pub is_enabled: bool,
    /// The `is_default` field. Asserted non-null: the INSERT always writes literal `false`.
    pub is_default: bool,
}

/// A `notifications` row as returned by [`PushStorage::get_notifications`].
#[derive(Debug, Clone)]
pub struct NotificationRow {
    /// The `id` field.
    pub id: i64,
    /// The `event_id` field.
    pub event_id: Option<String>,
    /// The `room_id` field.
    pub room_id: Option<String>,
    /// The `ts` field.
    pub ts: i64,
    /// The `notification_type` field.
    pub notification_type: Option<String>,
    /// The `profile_tag` field — "the profile tag of the rule that matched this event"
    /// (Matrix `GET /_matrix/client/v3/notifications`). Written by the notification
    /// record layer; see D-68 for its current wiring status.
    pub profile_tag: Option<String>,
    /// The `is_read` field.
    pub is_read: Option<bool>,
}

/// Trait abstraction over [`PushStorage`] for testability and service wiring.
#[async_trait]
pub trait PushStoreApi: Send + Sync {
    /// See [`get_pushers`].
    async fn get_pushers(&self, user_id: &str, device_id: Option<&str>) -> Result<Vec<PusherRow>, sqlx::Error>;

    #[allow(clippy::too_many_arguments)]
    /// See [`upsert_pusher`].
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
    ) -> Result<(), sqlx::Error>;

    /// See [`delete_pusher`].
    async fn delete_pusher(&self, user_id: &str, device_id: &str, pushkey: &str) -> Result<(), sqlx::Error>;

    #[allow(clippy::too_many_arguments)]
    /// See [`upsert_push_rule`].
    async fn upsert_push_rule(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        pattern: &Option<String>,
        conditions: &Option<Value>,
        actions: &Value,
        now: i64,
    ) -> Result<(), sqlx::Error>;

    /// See [`delete_push_rule`].
    async fn delete_push_rule(&self, user_id: &str, scope: &str, kind: &str, rule_id: &str)
        -> Result<u64, sqlx::Error>;

    /// See [`update_push_rule_actions`].
    async fn update_push_rule_actions(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        actions: &Value,
    ) -> Result<(), sqlx::Error>;

    /// See [`get_push_rule_enabled`].
    async fn get_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<Option<bool>, sqlx::Error>;

    /// See [`set_push_rule_enabled`].
    async fn set_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        enabled: bool,
    ) -> Result<(), sqlx::Error>;

    /// See [`get_user_push_rules`].
    async fn get_user_push_rules(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
    ) -> Result<Vec<PushRuleRow>, sqlx::Error>;

    /// See [`get_all_push_rules`].
    async fn get_all_push_rules(&self, user_id: &str) -> Result<Vec<PushRuleScopedRow>, sqlx::Error>;

    /// See [`get_notifications`].
    async fn get_notifications(&self, user_id: &str, limit: i64) -> Result<Vec<NotificationRow>, sqlx::Error>;

    /// See [`record_notification`].
    async fn record_notification(
        &self,
        user_id: &str,
        event_id: Option<&str>,
        room_id: Option<&str>,
        notification_type: &str,
        ts: i64,
    ) -> Result<(), sqlx::Error>;

    /// See [`ack_notification`].
    async fn ack_notification(&self, id: i64, user_id: &str, now: i64) -> Result<Option<i64>, sqlx::Error>;
}

/// The `PushStorage` struct.
#[derive(Clone)]
pub struct PushStorage {
    pool: Arc<sqlx::PgPool>,
}

impl PushStorage {
    /// See [`new`].
    pub fn new(pool: Arc<sqlx::PgPool>) -> Self {
        Self { pool }
    }

    // ── pushers ──────────────────────────────────────────────────────────

    /// See [`get_pushers`].
    pub async fn get_pushers(&self, user_id: &str, device_id: Option<&str>) -> Result<Vec<PusherRow>, sqlx::Error> {
        sqlx::query_as!(
            PusherRow,
            r#"
            SELECT pushkey, kind, app_id, app_display_name, device_display_name,
                   profile_tag, lang AS "lang!", data, device_id
            FROM pushers WHERE user_id = $1 AND device_id IS NOT DISTINCT FROM $2
            ORDER BY created_ts DESC, pushkey ASC
            "#,
            user_id,
            device_id,
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`upsert_pusher`].
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_pusher(
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
        sqlx::query!(
            r#"
            INSERT INTO pushers (user_id, device_id, pushkey, pushkey_ts, kind, app_id, app_display_name,
             device_display_name, profile_tag, lang, data, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (user_id, device_id, pushkey) DO UPDATE SET
             pushkey_ts = $4, kind = $5, app_id = $6, app_display_name = $7,
             device_display_name = $8, profile_tag = $9, lang = $10, data = $11, updated_ts = $13
            "#,
            user_id,
            device_id,
            pushkey,
            now,
            kind,
            app_id,
            app_display_name,
            device_display_name,
            profile_tag.as_deref(),
            lang,
            data.as_ref(),
            now,
            now,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`delete_pusher`].
    pub async fn delete_pusher(&self, user_id: &str, device_id: &str, pushkey: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM pushers WHERE user_id = $1 AND pushkey = $2 AND device_id = $3",
            user_id,
            pushkey,
            device_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    // ── push_rules ───────────────────────────────────────────────────────

    /// See [`upsert_push_rule`].
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_push_rule(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        pattern: &Option<String>,
        conditions: &Option<Value>,
        actions: &Value,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO push_rules (user_id, scope, kind, rule_id, pattern, conditions, actions,
             is_enabled, is_default, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, true, false, $8)
            ON CONFLICT (user_id, scope, kind, rule_id) DO UPDATE SET
             pattern = $5, conditions = $6, actions = $7
            "#,
            user_id,
            scope,
            kind,
            rule_id,
            pattern.as_deref(),
            conditions.as_ref(),
            actions,
            now,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`delete_push_rule`].
    pub async fn delete_push_rule(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            "DELETE FROM push_rules WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $4",
            user_id,
            scope,
            kind,
            rule_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// See [`update_push_rule_actions`].
    pub async fn update_push_rule_actions(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        actions: &Value,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE push_rules SET actions = $4 WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $5",
            user_id,
            scope,
            kind,
            actions,
            rule_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`get_push_rule_enabled`].
    pub async fn get_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<Option<bool>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT is_enabled AS "is_enabled!" FROM push_rules
               WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $4"#,
            user_id,
            scope,
            kind,
            rule_id,
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`set_push_rule_enabled`].
    pub async fn set_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        enabled: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE push_rules SET is_enabled = $4 WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $5",
            user_id,
            scope,
            kind,
            enabled,
            rule_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`get_user_push_rules`].
    ///
    /// Results are ordered by `rule_id` ascending — the Matrix push-rule order
    /// *within* a kind (the spec ranks kinds
    /// override → content → room → sender → underride, and within a kind the
    /// client must see a deterministic, rule-ID order). The previous
    /// `ORDER BY priority DESC, created_ts ASC` was meaningless: no writer ever
    /// set `priority` (every row carried the column default `0`), so the order
    /// silently degenerated to insertion time.
    pub async fn get_user_push_rules(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
    ) -> Result<Vec<PushRuleRow>, sqlx::Error> {
        sqlx::query_as!(
            PushRuleRow,
            r#"
            SELECT rule_id, pattern, conditions, actions,
                   is_enabled AS "is_enabled!", is_default AS "is_default!"
            FROM push_rules
            WHERE user_id = $1 AND scope = $2 AND kind = $3
            ORDER BY rule_id ASC
            "#,
            user_id,
            scope,
            kind,
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_all_push_rules`].
    ///
    /// Returns every rule for `user_id` across all scopes/kinds — the raw input for
    /// rebuilding `GET /pushrules`. Ordered `scope, kind, rule_id` so the caller can
    /// group in a single pass with a deterministic within-kind order.
    pub async fn get_all_push_rules(&self, user_id: &str) -> Result<Vec<PushRuleScopedRow>, sqlx::Error> {
        sqlx::query_as!(
            PushRuleScopedRow,
            r#"
            SELECT scope, kind, rule_id, pattern, conditions, actions,
                   is_enabled AS "is_enabled!", is_default AS "is_default!"
            FROM push_rules
            WHERE user_id = $1
            ORDER BY scope ASC, kind ASC, rule_id ASC
            "#,
            user_id,
        )
        .fetch_all(&*self.pool)
        .await
    }

    // ── notifications ────────────────────────────────────────────────────

    /// See [`get_notifications`].
    pub async fn get_notifications(&self, user_id: &str, limit: i64) -> Result<Vec<NotificationRow>, sqlx::Error> {
        sqlx::query_as!(
            NotificationRow,
            r#"
            SELECT id, event_id, room_id, ts, notification_type, profile_tag, is_read
            FROM notifications WHERE user_id = $1 ORDER BY ts DESC LIMIT $2
            "#,
            user_id,
            limit,
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// Record one notification for `user_id` — the **only production writer** of the
    /// `notifications` table (D-68).
    ///
    /// Called by `PushNotificationService::send_notification` *after* the server has
    /// decided to push, so `GET /_matrix/client/v3/notifications` and
    /// `GET /_matrix/client/v3/rooms/{room_id}/notifications` read rows a real code
    /// path produced instead of structurally empty results.
    ///
    /// **Idempotent while unread**: if `event_id` is known and the user already has an
    /// unread row for it, nothing is inserted — repeated attempts for one event do not
    /// stack up in the inbox. After `ack_notification` the event may notify again.
    /// `event_id IS NULL` (server-initiated push with no backing event) is never
    /// deduplicated, otherwise every event-less push would be swallowed by the first.
    pub async fn record_notification(
        &self,
        user_id: &str,
        event_id: Option<&str>,
        room_id: Option<&str>,
        notification_type: &str,
        ts: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO notifications (user_id, event_id, room_id, ts, notification_type, is_read, created_ts)
            SELECT $1, $2, $3, $4, $5, FALSE, $4
            WHERE $2::text IS NULL
               OR NOT EXISTS (
                   SELECT 1 FROM notifications
                   WHERE user_id = $1 AND event_id = $2 AND COALESCE(is_read, FALSE) = FALSE
               )
            "#,
            user_id,
            event_id,
            room_id,
            ts,
            notification_type,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`ack_notification`].
    pub async fn ack_notification(&self, id: i64, user_id: &str, now: i64) -> Result<Option<i64>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"
            UPDATE notifications SET is_read = true, updated_ts = $3
            WHERE id = $1 AND user_id = $2 RETURNING id
            "#,
            id,
            user_id,
            now,
        )
        .fetch_optional(&*self.pool)
        .await
    }
}

#[async_trait]
impl PushStoreApi for PushStorage {
    async fn get_pushers(&self, user_id: &str, device_id: Option<&str>) -> Result<Vec<PusherRow>, sqlx::Error> {
        self.get_pushers(user_id, device_id).await
    }

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
        self.upsert_pusher(
            user_id,
            device_id,
            pushkey,
            kind,
            app_id,
            app_display_name,
            device_display_name,
            profile_tag,
            lang,
            data,
            now,
        )
        .await
    }

    async fn delete_pusher(&self, user_id: &str, device_id: &str, pushkey: &str) -> Result<(), sqlx::Error> {
        self.delete_pusher(user_id, device_id, pushkey).await
    }

    async fn upsert_push_rule(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        pattern: &Option<String>,
        conditions: &Option<Value>,
        actions: &Value,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        self.upsert_push_rule(user_id, scope, kind, rule_id, pattern, conditions, actions, now).await
    }

    async fn delete_push_rule(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<u64, sqlx::Error> {
        self.delete_push_rule(user_id, scope, kind, rule_id).await
    }

    async fn update_push_rule_actions(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        actions: &Value,
    ) -> Result<(), sqlx::Error> {
        self.update_push_rule_actions(user_id, scope, kind, rule_id, actions).await
    }

    async fn get_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<Option<bool>, sqlx::Error> {
        self.get_push_rule_enabled(user_id, scope, kind, rule_id).await
    }

    async fn set_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        enabled: bool,
    ) -> Result<(), sqlx::Error> {
        self.set_push_rule_enabled(user_id, scope, kind, rule_id, enabled).await
    }

    async fn get_user_push_rules(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
    ) -> Result<Vec<PushRuleRow>, sqlx::Error> {
        self.get_user_push_rules(user_id, scope, kind).await
    }

    async fn get_all_push_rules(&self, user_id: &str) -> Result<Vec<PushRuleScopedRow>, sqlx::Error> {
        self.get_all_push_rules(user_id).await
    }

    async fn get_notifications(&self, user_id: &str, limit: i64) -> Result<Vec<NotificationRow>, sqlx::Error> {
        self.get_notifications(user_id, limit).await
    }

    async fn record_notification(
        &self,
        user_id: &str,
        event_id: Option<&str>,
        room_id: Option<&str>,
        notification_type: &str,
        ts: i64,
    ) -> Result<(), sqlx::Error> {
        self.record_notification(user_id, event_id, room_id, notification_type, ts).await
    }

    async fn ack_notification(&self, id: i64, user_id: &str, now: i64) -> Result<Option<i64>, sqlx::Error> {
        self.ack_notification(id, user_id, now).await
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    /// 每个测试一个从迁移 baseline 克隆出来的独立 schema（返回 guard 与 pool）。
    ///
    /// 2026-09-21：原先用共享 `public` 池。共享池的两个问题：测试结果取决于环境里
    /// `public` 的残渣（本地 `public` 落后于迁移 baseline 时会直接 42P01），且并行测试
    /// 互相影响。按铁律 7 消除状态共享：per-test schema 由模板克隆，表一定存在、行数从 0 开始。
    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<sqlx::PgPool>) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated test pool");
        let pool = isolated.pool();
        (isolated, pool)
    }

    fn unique_user_id(prefix: &str) -> String {
        format!("{}_pushtest_{}:test.com", prefix, uuid::Uuid::new_v4())
    }

    // ── helpers ─────────────────────────────────────────────────────────

    async fn cleanup_pushers(pool: &sqlx::PgPool, user_id: &str) {
        let _ = sqlx::query("DELETE FROM pushers WHERE user_id = $1").bind(user_id).execute(pool).await;
    }

    async fn cleanup_push_rules(pool: &sqlx::PgPool, user_id: &str) {
        let _ = sqlx::query("DELETE FROM push_rules WHERE user_id = $1").bind(user_id).execute(pool).await;
    }

    async fn cleanup_notifications(pool: &sqlx::PgPool, user_id: &str) {
        let _ = sqlx::query("DELETE FROM notifications WHERE user_id = $1").bind(user_id).execute(pool).await;
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_notification(
        pool: &sqlx::PgPool,
        user_id: &str,
        event_id: &str,
        room_id: &str,
        ts: i64,
        notification_type: &str,
        is_read: bool,
        created_ts: i64,
    ) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO notifications (user_id, event_id, room_id, ts, notification_type, is_read, created_ts) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        )
        .bind(user_id)
        .bind(event_id)
        .bind(room_id)
        .bind(ts)
        .bind(notification_type)
        .bind(is_read)
        .bind(created_ts)
        .fetch_one(pool)
        .await
        .expect("failed to insert test notification")
    }

    // ── pushers tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_upsert_and_get_pushers_with_device_id() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let device_id = "device1";
        let pushkey = "pushkey1";
        let now = current_timestamp_millis();

        cleanup_pushers(&pool, &user_id).await;

        storage
            .upsert_pusher(
                &user_id,
                device_id,
                pushkey,
                "http",
                "app.id",
                "My App",
                "My Device",
                &None,
                "en",
                &None,
                now,
            )
            .await
            .expect("upsert_pusher should succeed");

        let rows = storage.get_pushers(&user_id, Some(device_id)).await.expect("get_pushers should succeed");
        assert!(!rows.is_empty(), "should return at least one pusher");

        let row = &rows[0];
        assert_eq!(row.pushkey, "pushkey1");
        assert_eq!(row.device_id, device_id);

        cleanup_pushers(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_pushers_filters_by_specific_device_id() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");

        cleanup_pushers(&pool, &user_id).await;

        let now = current_timestamp_millis();
        storage
            .upsert_pusher(&user_id, "d1", "pk1", "http", "app.id", "App", "Dev", &None, "en", &None, now)
            .await
            .expect("upsert d1 should succeed");
        storage
            .upsert_pusher(&user_id, "d2", "pk2", "http", "app.id", "App", "Dev", &None, "en", &None, now)
            .await
            .expect("upsert d2 should succeed");

        // Filtering by a specific device_id should return only that pusher
        let rows_d1 =
            storage.get_pushers(&user_id, Some("d1")).await.expect("get_pushers with Some(d1) should succeed");
        assert_eq!(rows_d1.len(), 1, "should return exactly one pusher for d1");
        assert_eq!(rows_d1[0].device_id, "d1");

        let rows_d2 =
            storage.get_pushers(&user_id, Some("d2")).await.expect("get_pushers with Some(d2) should succeed");
        assert_eq!(rows_d2.len(), 1, "should return exactly one pusher for d2");
        assert_eq!(rows_d2[0].device_id, "d2");

        cleanup_pushers(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_pushers_with_none_device_id() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");

        cleanup_pushers(&pool, &user_id).await;

        let now = current_timestamp_millis();
        storage
            .upsert_pusher(&user_id, "d1", "pk1", "http", "app.id", "App", "Dev", &None, "en", &None, now)
            .await
            .expect("upsert should succeed");

        // When device_id is None, it binds as SQL NULL, and
        // `device_id IS NOT DISTINCT FROM NULL` only matches rows
        // where device_id IS NULL. Since the column is NOT NULL,
        // this currently returns zero rows.
        let rows = storage.get_pushers(&user_id, None).await.expect("get_pushers with None should succeed");
        assert!(rows.is_empty(), "None device_id currently returns empty result set");

        cleanup_pushers(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_pushers_returns_empty_for_unknown_user() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@unknown");

        let rows = storage.get_pushers(&user_id, None).await.expect("get_pushers should succeed");
        assert!(rows.is_empty(), "unknown user should have no pushers");
    }

    #[tokio::test]
    async fn test_upsert_pusher_updates_existing() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let device_id = "dev_update";
        let pushkey = "pk_update";
        let now = current_timestamp_millis();

        cleanup_pushers(&pool, &user_id).await;

        // Insert initial
        storage
            .upsert_pusher(&user_id, device_id, pushkey, "http", "app.id", "App1", "Dev1", &None, "en", &None, now)
            .await
            .expect("first upsert should succeed");

        // Update with new app_display_name
        let now2 = now + 1000;
        storage
            .upsert_pusher(&user_id, device_id, pushkey, "http", "app.id", "App2", "Dev2", &None, "en", &None, now2)
            .await
            .expect("second upsert should succeed");

        let rows = storage.get_pushers(&user_id, Some(device_id)).await.expect("get_pushers should succeed");
        assert_eq!(rows.len(), 1, "should still have exactly one pusher after upsert");
        assert_eq!(rows[0].app_display_name, "App2");

        cleanup_pushers(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_delete_pusher() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let device_id = "dev_del";
        let pushkey = "pk_del";
        let now = current_timestamp_millis();

        cleanup_pushers(&pool, &user_id).await;

        storage
            .upsert_pusher(&user_id, device_id, pushkey, "http", "app.id", "App", "Dev", &None, "en", &None, now)
            .await
            .expect("upsert should succeed");

        storage.delete_pusher(&user_id, device_id, pushkey).await.expect("delete_pusher should succeed");

        let rows = storage.get_pushers(&user_id, Some(device_id)).await.expect("get_pushers should succeed");
        assert!(rows.is_empty(), "pusher should be deleted");

        cleanup_pushers(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_delete_pusher_nonexistent_does_not_error() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");

        storage
            .delete_pusher(&user_id, "no_dev", "no_pk")
            .await
            .expect("delete_pusher on non-existent row should not error");
    }

    // ── push_rules tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_upsert_and_get_push_rule() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = current_timestamp_millis();

        cleanup_push_rules(&pool, &user_id).await;

        storage
            .upsert_push_rule(&user_id, "global", "override", "rule1", &None, &None, &json!(["notify"]), now)
            .await
            .expect("upsert_push_rule should succeed");

        let rows = storage
            .get_user_push_rules(&user_id, "global", "override")
            .await
            .expect("get_user_push_rules should succeed");
        assert!(!rows.is_empty(), "should return at least one rule");

        let enabled = storage
            .get_push_rule_enabled(&user_id, "global", "override", "rule1")
            .await
            .expect("get_push_rule_enabled should succeed");
        assert_eq!(enabled, Some(true), "new rule should be enabled by default");

        cleanup_push_rules(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_user_push_rules_returns_empty_for_no_rules() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");

        let rows = storage
            .get_user_push_rules(&user_id, "global", "override")
            .await
            .expect("get_user_push_rules should succeed");
        assert!(rows.is_empty(), "unknown user should have no push rules");
    }

    #[tokio::test]
    async fn test_upsert_push_rule_updates_existing() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = current_timestamp_millis();

        cleanup_push_rules(&pool, &user_id).await;

        storage
            .upsert_push_rule(
                &user_id,
                "global",
                "override",
                "rule2",
                &Some("original".to_string()),
                &None,
                &json!(["notify"]),
                now,
            )
            .await
            .expect("first upsert should succeed");

        storage
            .upsert_push_rule(
                &user_id,
                "global",
                "override",
                "rule2",
                &Some("updated".to_string()),
                &None,
                &json!(["dont_notify"]),
                now,
            )
            .await
            .expect("second upsert should succeed");

        let rows = storage
            .get_user_push_rules(&user_id, "global", "override")
            .await
            .expect("get_user_push_rules should succeed");
        assert_eq!(rows.len(), 1, "should still have exactly one rule after upsert");
        assert_eq!(rows[0].pattern.as_deref(), Some("updated"));

        cleanup_push_rules(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_update_push_rule_actions() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = current_timestamp_millis();

        cleanup_push_rules(&pool, &user_id).await;

        storage
            .upsert_push_rule(&user_id, "global", "override", "rule_actions", &None, &None, &json!(["notify"]), now)
            .await
            .expect("upsert should succeed");

        storage
            .update_push_rule_actions(
                &user_id,
                "global",
                "override",
                "rule_actions",
                &json!(["dont_notify", {"set_tweak": "highlight"}]),
            )
            .await
            .expect("update_push_rule_actions should succeed");

        let rows = storage
            .get_user_push_rules(&user_id, "global", "override")
            .await
            .expect("get_user_push_rules should succeed");
        assert!(!rows.is_empty());
        let actions = rows[0].actions.as_ref().expect("actions are stored as JSONB");
        assert!(actions.as_array().is_some_and(|a| a.len() >= 2));

        cleanup_push_rules(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_delete_push_rule() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = current_timestamp_millis();

        cleanup_push_rules(&pool, &user_id).await;

        storage
            .upsert_push_rule(&user_id, "global", "override", "rule_del", &None, &None, &json!(["notify"]), now)
            .await
            .expect("upsert should succeed");

        let affected = storage
            .delete_push_rule(&user_id, "global", "override", "rule_del")
            .await
            .expect("delete_push_rule should succeed");
        assert_eq!(affected, 1, "should delete exactly one row");

        let affected2 = storage
            .delete_push_rule(&user_id, "global", "override", "rule_del")
            .await
            .expect("second delete should succeed");
        assert_eq!(affected2, 0, "second delete should affect zero rows");

        cleanup_push_rules(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_push_rule_enabled_returns_none_for_nonexistent() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");

        let enabled = storage
            .get_push_rule_enabled(&user_id, "global", "override", "no_such_rule")
            .await
            .expect("get_push_rule_enabled should succeed");
        assert_eq!(enabled, None, "non-existent rule should return None");
    }

    #[tokio::test]
    async fn test_set_push_rule_enabled() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = current_timestamp_millis();

        cleanup_push_rules(&pool, &user_id).await;

        storage
            .upsert_push_rule(&user_id, "global", "override", "rule_toggle", &None, &None, &json!(["notify"]), now)
            .await
            .expect("upsert should succeed");

        storage
            .set_push_rule_enabled(&user_id, "global", "override", "rule_toggle", false)
            .await
            .expect("set_push_rule_enabled should succeed");

        let enabled = storage
            .get_push_rule_enabled(&user_id, "global", "override", "rule_toggle")
            .await
            .expect("get_push_rule_enabled should succeed");
        assert_eq!(enabled, Some(false), "rule should be disabled");

        storage
            .set_push_rule_enabled(&user_id, "global", "override", "rule_toggle", true)
            .await
            .expect("re-enable should succeed");

        let enabled = storage
            .get_push_rule_enabled(&user_id, "global", "override", "rule_toggle")
            .await
            .expect("get_push_rule_enabled should succeed");
        assert_eq!(enabled, Some(true), "rule should be re-enabled");

        cleanup_push_rules(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_user_push_rules_scoped_by_scope_and_kind() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = current_timestamp_millis();

        cleanup_push_rules(&pool, &user_id).await;

        // Create rules in different scope/kind combinations
        storage
            .upsert_push_rule(&user_id, "global", "override", "r1", &None, &None, &json!(["notify"]), now)
            .await
            .expect("upsert r1");
        storage
            .upsert_push_rule(&user_id, "global", "content", "r2", &None, &None, &json!(["notify"]), now)
            .await
            .expect("upsert r2");
        storage
            .upsert_push_rule(&user_id, "devices/DEV1", "override", "r3", &None, &None, &json!(["notify"]), now)
            .await
            .expect("upsert r3");

        let global_override =
            storage.get_user_push_rules(&user_id, "global", "override").await.expect("get global override");
        assert_eq!(global_override.len(), 1, "should return only global/override rules");
        assert_eq!(global_override[0].rule_id, "r1");

        let global_content =
            storage.get_user_push_rules(&user_id, "global", "content").await.expect("get global content");
        assert_eq!(global_content.len(), 1);
        assert_eq!(global_content[0].rule_id, "r2");

        let device_override =
            storage.get_user_push_rules(&user_id, "devices/DEV1", "override").await.expect("get device override");
        assert_eq!(device_override.len(), 1);
        assert_eq!(device_override[0].rule_id, "r3");

        cleanup_push_rules(&pool, &user_id).await;
    }

    // ── notifications tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_notifications_returns_rows() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = current_timestamp_millis();

        cleanup_notifications(&pool, &user_id).await;

        insert_notification(&pool, &user_id, "$ev1", "!room1:test.com", now, "message", false, now).await;
        insert_notification(&pool, &user_id, "$ev2", "!room1:test.com", now + 1, "invite", false, now).await;

        let rows = storage.get_notifications(&user_id, 10).await.expect("get_notifications should succeed");
        assert_eq!(rows.len(), 2, "should return both notifications");
        // Results ordered by ts DESC, so newest (ev2) first
        assert_eq!(rows[0].event_id.as_deref(), Some("$ev2"));

        cleanup_notifications(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_notifications_respects_limit() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = current_timestamp_millis();

        cleanup_notifications(&pool, &user_id).await;

        for i in 0..5 {
            insert_notification(&pool, &user_id, &format!("$ev{i}"), "!room1:test.com", now + i, "message", false, now)
                .await;
        }

        let rows = storage.get_notifications(&user_id, 3).await.expect("get_notifications should succeed");
        assert_eq!(rows.len(), 3, "should respect limit");

        cleanup_notifications(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_notifications_empty_for_unknown_user() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@unknown");

        let rows = storage.get_notifications(&user_id, 10).await.expect("get_notifications should succeed");
        assert!(rows.is_empty(), "unknown user should have no notifications");
    }

    /// D-68：`record_notification` 必须真的写进 `notifications`，并可从
    /// `get_notifications` 读回（此前该表在生产路径上**没有任何写入者**，
    /// 三个已注册端点恒为空）。
    #[tokio::test]
    async fn test_record_notification_inserts_a_readable_row() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = current_timestamp_millis();

        storage
            .record_notification(&user_id, Some("$ev_record"), Some("!room1:test.com"), "message", now)
            .await
            .expect("record_notification must succeed");

        let rows = storage.get_notifications(&user_id, 10).await.expect("get_notifications");
        assert_eq!(rows.len(), 1, "the recorded notification must be readable");
        assert_eq!(rows[0].event_id.as_deref(), Some("$ev_record"));
        assert_eq!(rows[0].room_id.as_deref(), Some("!room1:test.com"));
        assert_eq!(rows[0].notification_type.as_deref(), Some("message"));
        assert_eq!(rows[0].is_read, Some(false), "a freshly recorded notification is unread");
        assert_eq!(rows[0].ts, now);

        cleanup_notifications(&pool, &user_id).await;
    }

    /// 同一 event 在被 ack 之前重复触发（同一次推送的多次尝试）**不得**累积重复行；
    /// ack 之后允许重新成为一条新通知。
    #[tokio::test]
    async fn test_record_notification_is_idempotent_until_acked() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = current_timestamp_millis();

        for attempt in 0..3 {
            storage
                .record_notification(&user_id, Some("$ev_dup"), Some("!room1:test.com"), "message", now + attempt)
                .await
                .expect("record_notification must succeed");
        }
        let rows = storage.get_notifications(&user_id, 10).await.expect("get_notifications");
        assert_eq!(rows.len(), 1, "repeated attempts for one unread event must not duplicate");

        storage.ack_notification(rows[0].id, &user_id, now + 10).await.expect("ack_notification");
        storage
            .record_notification(&user_id, Some("$ev_dup"), Some("!room1:test.com"), "message", now + 11)
            .await
            .expect("record_notification after ack must succeed");
        let rows = storage.get_notifications(&user_id, 10).await.expect("get_notifications");
        assert_eq!(rows.len(), 2, "after ack the event may notify again");

        cleanup_notifications(&pool, &user_id).await;
    }

    /// `event_id` 缺失（服务端主动推送）时**不做**去重：两次通知各自成行，
    /// 否则所有无 event 的推送都会被第一条吞掉。
    #[tokio::test]
    async fn test_record_notification_without_event_id_never_dedups() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = current_timestamp_millis();

        storage
            .record_notification(&user_id, None, None, "message", now)
            .await
            .expect("record_notification must succeed");
        storage
            .record_notification(&user_id, None, None, "message", now + 1)
            .await
            .expect("record_notification must succeed");

        let rows = storage.get_notifications(&user_id, 10).await.expect("get_notifications");
        assert_eq!(rows.len(), 2, "notifications without an event_id must never be merged");

        cleanup_notifications(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_ack_notification_marks_read_and_returns_id() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = current_timestamp_millis();

        cleanup_notifications(&pool, &user_id).await;

        let nid = insert_notification(&pool, &user_id, "$ev_ack", "!room1:test.com", now, "message", false, now).await;

        let result =
            storage.ack_notification(nid, &user_id, now + 1000).await.expect("ack_notification should succeed");
        assert!(result.is_some(), "should return the acknowledged notification id");
        assert_eq!(result.unwrap(), nid);

        cleanup_notifications(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_ack_notification_returns_none_for_wrong_user() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let other_user = unique_user_id("@other");
        let now = current_timestamp_millis();

        cleanup_notifications(&pool, &user_id).await;
        cleanup_notifications(&pool, &other_user).await;

        let nid =
            insert_notification(&pool, &user_id, "$ev_wrong", "!room1:test.com", now, "message", false, now).await;

        let result =
            storage.ack_notification(nid, &other_user, now + 1000).await.expect("ack_notification should succeed");
        assert!(result.is_none(), "other user should not be able to ack this notification");

        cleanup_notifications(&pool, &user_id).await;
        cleanup_notifications(&pool, &other_user).await;
    }

    #[tokio::test]
    async fn test_ack_notification_returns_none_for_nonexistent_id() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");

        let result = storage
            .ack_notification(99999999, &user_id, current_timestamp_millis())
            .await
            .expect("ack_notification should succeed");
        assert!(result.is_none(), "non-existent notification id should return None");
    }
    /// The spec orders push rules *within a kind* deterministically; this
    /// implementation uses `rule_id` ascending. Regression guard for the old
    /// `ORDER BY priority DESC, created_ts ASC`, which degenerated to insertion
    /// order because no writer ever set `priority`.
    #[tokio::test]
    async fn test_get_user_push_rules_orders_by_rule_id_not_insertion_time() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@pushorder");
        cleanup_push_rules(&pool, &user_id).await;

        // Insert deliberately OUT of rule_id order, with descending created_ts,
        // so an insertion-time sort would produce a different answer.
        let base = current_timestamp_millis();
        for (idx, rule_id) in [".m.rule.zzz", ".m.rule.aaa", ".m.rule.mmm"].into_iter().enumerate() {
            storage
                .upsert_push_rule(
                    &user_id,
                    "global",
                    "override",
                    rule_id,
                    &None,
                    &None,
                    &json!(["notify"]),
                    base + (10 - idx as i64),
                )
                .await
                .expect("upsert should succeed");
        }

        let rows = storage
            .get_user_push_rules(&user_id, "global", "override")
            .await
            .expect("get_user_push_rules should succeed");
        let got: Vec<String> = rows.iter().map(|r| r.rule_id.clone()).collect();
        assert_eq!(
            got,
            vec![".m.rule.aaa", ".m.rule.mmm", ".m.rule.zzz"],
            "rules must come back in rule_id order regardless of insertion time"
        );

        cleanup_push_rules(&pool, &user_id).await;
    }
}
