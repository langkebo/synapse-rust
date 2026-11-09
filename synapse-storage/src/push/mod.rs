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
#[derive(Debug, Clone, sqlx::FromRow)]
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
    /// The `priority` field — position within this `(scope, kind)` group, ascending.
    /// Written by [`PushStorage::upsert_push_rule`], which renumbers the whole group
    /// on every positioned write; rows inserted before B-34 all carry the default `0`.
    pub priority: i64,
}

/// A `push_rules` row carrying its `scope`/`kind` classification, as returned by
/// [`PushStorage::get_all_push_rules`]. Unlike [`PushRuleRow`] (which is already
/// scoped by the caller's `scope`/`kind` arguments), this row is the input needed
/// to rebuild the full `GET /pushrules` document.
#[derive(Debug, Clone, sqlx::FromRow)]
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
    /// The `priority` field — position within this `(scope, kind)` group, ascending.
    /// See [`PushRuleRow::priority`].
    pub priority: i64,
}

/// Failure modes of a position-aware push-rule write
/// ([`PushStorage::upsert_push_rule`]).
///
/// The Matrix client-server API lets `PUT /pushrules/{scope}/{kind}/{rule_id}` name
/// a sibling via `before` / `after`. Both are client input, so "the anchor does not
/// exist" is a **400**, not a server fault — it must be distinguishable from a
/// database error instead of being flattened into `sqlx::Error`.
#[derive(Debug)]
pub enum PushRuleWriteError {
    /// `before` and `after` were both supplied; a rule cannot occupy two positions.
    ConflictingPosition,
    /// The rule named by `before` / `after` is not a rule of this `(user, scope, kind)`
    /// group. Server defaults live outside this table, so naming one lands here too —
    /// the spec forbids positioning a user rule relative to a default rule.
    AnchorNotFound {
        /// The `before` / `after` value the client sent.
        anchor: String,
    },
    /// The database rejected the write.
    Sqlx(sqlx::Error),
}

impl std::fmt::Display for PushRuleWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConflictingPosition => write!(f, "`before` and `after` cannot both be given"),
            Self::AnchorNotFound { anchor } => write!(f, "push rule `{anchor}` not found in this kind"),
            Self::Sqlx(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PushRuleWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlx(e) => Some(e),
            Self::ConflictingPosition | Self::AnchorNotFound { .. } => None,
        }
    }
}

impl From<sqlx::Error> for PushRuleWriteError {
    fn from(e: sqlx::Error) -> Self {
        Self::Sqlx(e)
    }
}

/// A `notifications` row as returned by [`PushStorage::get_notifications`].
#[derive(Debug, Clone, sqlx::FromRow)]
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

/// Filters and cursor for one page of a user's notification inbox
/// (`GET /_matrix/client/v3/notifications`).
///
/// Keyset pagination is `(ts, id)` descending: `(ts, id)` is unique per row and
/// both columns are non-null, so the cursor never skips or repeats a row when new
/// notifications arrive between two page reads.
#[derive(Debug, Clone, Default)]
pub struct NotificationPageQuery {
    /// Maximum number of rows to return. Callers that need `next_token` ask for
    /// `limit + 1` and drop the probe row themselves.
    pub limit: i64,
    /// `(ts, id)` of the last row of the previous page — strict `<` continuation.
    pub cursor: Option<(i64, i64)>,
    /// `only=room_id:<room>` filters. Empty means "any room".
    pub room_ids: Vec<String>,
    /// `only=type:<event type>` filters (matched against `events.event_type`,
    /// not against `notifications.notification_type`). Empty means "any type".
    pub event_types: Vec<String>,
    /// `only=sender:<user id>` filters. Empty means "any sender".
    pub senders: Vec<String>,
}

impl NotificationPageQuery {
    /// First page of `limit` rows with no filters.
    pub fn first(limit: i64) -> Self {
        Self { limit, ..Self::default() }
    }
}

/// Trait abstraction over [`PushStorage`] for testability and service wiring.
#[async_trait]
pub trait PushStoreApi: Send + Sync {
    /// See [`get_pushers`].
    async fn get_pushers(&self, user_id: &str, device_id: Option<&str>) -> Result<Vec<PusherRow>, sqlx::Error>;

    // reason: `upsert_pusher` 是协议/存储字段的直传签名：参数逐个来自调用点，打包成结构体只会把同一份字段列表抄两遍
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

    /// See [`PushStorage::delete_all_pushers_for_user`].
    ///
    /// Returns the number of rows removed. Used by account deactivation:
    /// `pushers` has **no foreign key** to `devices`, so deleting a user's
    /// devices does *not* remove their pushers — upstream Synapse has to delete
    /// them explicitly too (`deactivate_account.py`: "Most of the pushers will
    /// have been deleted when we logged out the associated devices above, but we
    /// still need to delete pushers not associated with devices, e.g. email
    /// pushers").
    async fn delete_all_pushers_for_user(&self, user_id: &str) -> Result<u64, sqlx::Error>;

    // reason: `upsert_push_rule` 是协议/存储字段的直传签名：参数逐个来自调用点，打包成结构体只会把同一份字段列表抄两遍
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
        before: Option<&str>,
        after: Option<&str>,
        now: i64,
    ) -> Result<(), PushRuleWriteError>;

    /// See [`delete_push_rule`].
    async fn delete_push_rule(&self, user_id: &str, scope: &str, kind: &str, rule_id: &str)
        -> Result<u64, sqlx::Error>;

    /// Count a user's push rules, excluding one `(scope, kind, rule_id)` triple.
    ///
    /// Backs the per-user push rule count limit (ELEMENTSEC-2026-1071). Excluding
    /// the target rule lets an *update* of an already-counted rule through while
    /// still capping the user's total at the limit.
    ///
    /// A dedicated `COUNT(*)` (served by `idx_push_rules_user`) is used rather
    /// than [`Self::get_all_push_rules`] because the latter would materialise
    /// every rule body on each write — the exact amplification the limit exists
    /// to prevent.
    async fn count_push_rules_excluding(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<i64, sqlx::Error>;

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
    async fn get_notifications(
        &self,
        user_id: &str,
        query: &NotificationPageQuery,
    ) -> Result<Vec<NotificationRow>, sqlx::Error>;

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
    // reason: `upsert_pusher` 是协议/存储字段的直传签名：参数逐个来自调用点，打包成结构体只会把同一份字段列表抄两遍
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

    /// Remove **every** pusher belonging to `user_id`, returning the row count.
    ///
    /// Deliberately not built on [`Self::delete_pusher`]: that one keys on
    /// `(user_id, device_id, pushkey)`, so it can only reach pushers whose device
    /// still exists *and* whose key the caller already knows. Deactivation needs
    /// the opposite ("everything this user has left"), which is why it takes a
    /// single statement rather than a read-then-delete loop.
    pub async fn delete_all_pushers_for_user(&self, user_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM pushers WHERE user_id = $1", user_id).execute(&*self.pool).await?;
        Ok(result.rows_affected())
    }

    /// Remove **every** push rule belonging to `user_id`, returning the row count.
    ///
    /// Counterpart to [`Self::delete_all_pushers_for_user`] and used by the same
    /// caller: [`Self::delete_push_rule`] keys on `(user_id, scope, kind, rule_id)`,
    /// so it cannot reach rules the caller does not name. Deactivation needs all of
    /// them — a deactivated account's rules are dead weight that nothing else clears.
    pub async fn delete_all_push_rules_for_user(&self, user_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM push_rules WHERE user_id = $1", user_id).execute(&*self.pool).await?;
        Ok(result.rows_affected())
    }

    // ── push_rules ───────────────────────────────────────────────────────

    /// See [`upsert_push_rule`].
    ///
    /// Positioned write for `PUT /_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}`:
    ///
    /// * `before` — the rule becomes the **next-most important** rule of its kind
    ///   (immediately before the anchor).
    /// * `after` — the rule becomes the **next-less important** rule (immediately
    ///   after the anchor).
    /// * neither — a *new* rule is appended to the end of its kind; an **existing**
    ///   rule keeps its current position, so a plain body update never reorders it.
    ///
    /// Anchors resolve only inside the same `(user, scope, kind)` group: server
    /// defaults are merged at read time and are not rows here, which is exactly why
    /// the spec forbids positioning relative to them — naming one yields
    /// [`PushRuleWriteError::AnchorNotFound`].
    ///
    /// The read-modify-write runs in one transaction. `SELECT … FOR UPDATE` locks the
    /// group's rows, so two concurrent writers to the same kind serialise instead of
    /// computing positions from the same stale list. A still-empty group has nothing
    /// to lock; its writers both land at index 0 and `rule_id` breaks the tie.
    // reason: `upsert_push_rule` 是协议/存储字段的直传签名：参数逐个来自调用点，打包成结构体只会把同一份字段列表抄两遍
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
        before: Option<&str>,
        after: Option<&str>,
        now: i64,
    ) -> Result<(), PushRuleWriteError> {
        if before.is_some() && after.is_some() {
            return Err(PushRuleWriteError::ConflictingPosition);
        }

        let mut tx = self.pool.begin().await?;

        let existing: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT rule_id, priority FROM push_rules
            WHERE user_id = $1 AND scope = $2 AND kind = $3
            ORDER BY priority ASC, rule_id ASC
            FOR UPDATE
            "#,
        )
        .bind(user_id)
        .bind(scope)
        .bind(kind)
        .fetch_all(&mut *tx)
        .await?;

        let mut order: Vec<String> = existing.into_iter().map(|(rule_id, _)| rule_id).collect();
        // Taking the rule out first lets `before`/`after` resolve against the list it
        // will actually be inserted into, and makes "anchor is the rule itself" a
        // not-found instead of a no-op (spec: anchors are *other* rules).
        let current_index = order.iter().position(|id| id == rule_id);
        if current_index.is_some() {
            order.retain(|id| id != rule_id);
        }

        let insert_at = match (before, after) {
            (Some(anchor), None) => order
                .iter()
                .position(|id| id == anchor)
                .ok_or_else(|| PushRuleWriteError::AnchorNotFound { anchor: anchor.to_string() })?,
            (None, Some(anchor)) => {
                order
                    .iter()
                    .position(|id| id == anchor)
                    .ok_or_else(|| PushRuleWriteError::AnchorNotFound { anchor: anchor.to_string() })?
                    + 1
            }
            (None, None) => current_index.unwrap_or(order.len()),
            (Some(_), Some(_)) => return Err(PushRuleWriteError::ConflictingPosition),
        };
        order.insert(insert_at, rule_id.to_string());

        // Body upsert only: `priority` is assigned by the renumbering statement below,
        // which needs a row to exist for every id in `order`. Keeping `ON CONFLICT`
        // limited to the body preserves the pre-B-34 update semantics.
        sqlx::query(
            r#"
            INSERT INTO push_rules (user_id, scope, kind, rule_id, pattern, conditions, actions,
             is_enabled, is_default, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, true, false, $8)
            ON CONFLICT (user_id, scope, kind, rule_id) DO UPDATE SET
             pattern = $5, conditions = $6, actions = $7
            "#,
        )
        .bind(user_id)
        .bind(scope)
        .bind(kind)
        .bind(rule_id)
        .bind(pattern.as_deref())
        .bind(conditions.as_ref())
        .bind(actions)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        // Renumber the whole group in one round trip: `unnest(…) WITH ORDINALITY`
        // turns the computed order into a `(rule_id, index)` relation and the UPDATE
        // joins it on the unique key. One statement instead of N keeps a group at the
        // per-user cap (10 000) from turning every insert into 10 000 round trips.
        sqlx::query(
            r#"
            UPDATE push_rules AS p
            SET priority = v.idx
            FROM (
                SELECT rule_id, ordinality - 1 AS idx
                FROM unnest($4::text[]) WITH ORDINALITY AS t(rule_id, ordinality)
            ) AS v
            WHERE p.user_id = $1 AND p.scope = $2 AND p.kind = $3 AND p.rule_id = v.rule_id
            "#,
        )
        .bind(user_id)
        .bind(scope)
        .bind(kind)
        .bind(&order)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
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

    /// See [`count_push_rules_excluding`].
    pub async fn count_push_rules_excluding(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!" FROM push_rules
            WHERE user_id = $1 AND NOT (scope = $2 AND kind = $3 AND rule_id = $4)
            "#,
            user_id,
            scope,
            kind,
            rule_id,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
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
    /// Results are ordered by `priority` ascending, `rule_id` breaking ties — the
    /// Matrix push-rule order *within* a kind. The spec ranks kinds
    /// override → content → room → sender → underride, and within a kind the client
    /// must see the order the user built with `before` / `after`
    /// ([`Self::upsert_push_rule`]). `rule_id` is the deterministic tiebreaker for
    /// rows that share a `priority` (all pre-B-34 rows default to `0`).
    ///
    /// Written with the runtime `query_as` form (not `query_as!`) because the read
    /// now selects `priority`, which the `.sqlx` offline cache does not know about.
    pub async fn get_user_push_rules(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
    ) -> Result<Vec<PushRuleRow>, sqlx::Error> {
        sqlx::query_as::<_, PushRuleRow>(
            r#"
            SELECT rule_id, pattern, conditions, actions, is_enabled, is_default, priority
            FROM push_rules
            WHERE user_id = $1 AND scope = $2 AND kind = $3
            ORDER BY priority ASC, rule_id ASC
            "#,
        )
        .bind(user_id)
        .bind(scope)
        .bind(kind)
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_all_push_rules`].
    ///
    /// Returns every rule for `user_id` across all scopes/kinds — the raw input for
    /// rebuilding `GET /pushrules`. Ordered `scope, kind, priority, rule_id` so the
    /// caller can group in a single pass and preserve the user-defined within-kind
    /// order.
    pub async fn get_all_push_rules(&self, user_id: &str) -> Result<Vec<PushRuleScopedRow>, sqlx::Error> {
        sqlx::query_as::<_, PushRuleScopedRow>(
            r#"
            SELECT scope, kind, rule_id, pattern, conditions, actions, is_enabled, is_default, priority
            FROM push_rules
            WHERE user_id = $1
            ORDER BY scope ASC, kind ASC, priority ASC, rule_id ASC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
    }

    // ── notifications ────────────────────────────────────────────────────

    /// See [`get_notifications`].
    ///
    /// Keyset pagination on `(ts, id)` descending plus the inbox filters from
    /// `GET /_matrix/client/v3/notifications`. `only=type:` and `only=sender:`
    /// are matched against the backing event (`events.event_type` /
    /// `events.sender`), not against `notifications.notification_type`: the
    /// former is "what the event is", the latter is "which push rule fired".
    ///
    /// Written with the runtime `query_as` form (not `query_as!`) so the SQL can
    /// carry `= ANY($n)` array filters without a new `.sqlx` offline-cache entry.
    pub async fn get_notifications(
        &self,
        user_id: &str,
        query: &NotificationPageQuery,
    ) -> Result<Vec<NotificationRow>, sqlx::Error> {
        let (cursor_ts, cursor_id) = match query.cursor {
            Some((ts, id)) => (Some(ts), Some(id)),
            None => (None, None),
        };

        sqlx::query_as::<_, NotificationRow>(
            r#"
            SELECT n.id, n.event_id, n.room_id, n.ts, n.notification_type, n.profile_tag, n.is_read
            FROM notifications n
            LEFT JOIN events e ON e.event_id = n.event_id
            WHERE n.user_id = $1
              AND ($2::bigint IS NULL OR n.ts < $2 OR (n.ts = $2 AND n.id < $3))
              AND (cardinality($4::text[]) = 0 OR n.room_id = ANY($4))
              AND (cardinality($5::text[]) = 0 OR e.event_type = ANY($5))
              AND (cardinality($6::text[]) = 0 OR e.sender = ANY($6))
            ORDER BY n.ts DESC, n.id DESC
            LIMIT $7
            "#,
        )
        .bind(user_id)
        .bind(cursor_ts)
        .bind(cursor_id)
        .bind(&query.room_ids)
        .bind(&query.event_types)
        .bind(&query.senders)
        .bind(query.limit)
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
        .await
        .map(|_| ())
        // The partial unique index `uq_notifications_user_event_unread` can fire
        // when two concurrent writers both pass the `NOT EXISTS` check (each sees
        // the pre-insert snapshot) and then both try to insert. A duplicate unread
        // row is exactly what the idempotency contract above forbids, so treat the
        // unique violation as success instead of surfacing a spurious 500.
        .or_else(|e| match &e {
            sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => Ok(()),
            _ => Err(e),
        })
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

    /// Records one `notifications` row per **local joined member** of `room_id`
    /// that this event should alert (D16).
    ///
    /// The notification endpoints read the `notifications` table. Before this
    /// method the only writer was [`record_notification`](Self::record_notification),
    /// called from `PushNotificationService::send_notification`, which returns
    /// early when the recipient has no pusher — so a user who never registered a
    /// pusher had a structurally empty inbox and `/notifications` could never
    /// return anything. Recording is therefore decoupled from delivery here.
    ///
    /// It is a single `INSERT ... SELECT` over the room's local membership, so the
    /// send path pays one round trip regardless of room size. Suppression covers
    /// the part of the push-rule model stored in `push_rules`:
    ///
    /// * the sender never notifies themselves;
    /// * remote members are skipped (`right(user_id, len(server)+1) = ':' || server`);
    /// * `m.ignored_user_list` hides the sender;
    /// * an enabled `dont_notify` rule whose id is `.m.rule.master` (whole account)
    ///   or a `room` rule targeting this room (matched on `pattern`, which is how
    ///   this server stores them, or on `rule_id`, which is how the spec names them)
    ///   suppresses the row.
    ///
    /// `highlight_user_ids` (`content.m.mentions.user_ids`) and `highlight_room`
    /// (`content.m.mentions.room`) select `notification_type = 'highlight'`
    /// instead of `'message'`. So do the body-text rules that `.m.mentions`
    /// alone misses: the default `.m.rule.contains_display_name` /
    /// `.m.rule.contains_user_name` (matched against `users.displayname` and the
    /// user id localpart) and any enabled user `content` keyword rule whose
    /// actions carry a `highlight` tweak. `body` is the plaintext
    /// `content.body`; encrypted events pass `""` and degrade to explicit
    /// mentions only.
    ///
    /// **Idempotent while unread**, like [`record_notification`](Self::record_notification):
    /// a member who already has an unread row for `event_id` is not inserted
    /// again, so a retried federation transaction cannot duplicate the inbox entry.
    /// The partial unique index `uq_notifications_user_event_unread` on
    /// `(user_id, event_id) WHERE COALESCE(is_read, FALSE) = FALSE AND event_id IS NOT NULL`
    /// backs this up between concurrent writers (`ON CONFLICT DO NOTHING`). The
    /// predicate here and in the index must stay textually identical — a mismatch
    /// makes PostgreSQL fail index inference with 42P10.
    ///
    /// Returns the number of rows inserted.
    pub async fn record_room_event_notifications(
        &self,
        room_id: &str,
        event_id: &str,
        sender: &str,
        highlight_user_ids: &[String],
        highlight_room: bool,
        ts: i64,
        local_server_name: &str,
        body: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            INSERT INTO notifications (user_id, event_id, room_id, ts, notification_type, is_read, created_ts)
            SELECT m.user_id, $2, $1, $6,
                   CASE WHEN m.user_id = ANY($4) OR $5 THEN 'highlight'
                        WHEN $8 <> '' AND (
                            -- .m.rule.contains_display_name（默认规则，未持久化）：
                            -- 正文包含收件人 displayname。
                            EXISTS (
                                SELECT 1 FROM users u
                                WHERE u.user_id = m.user_id
                                  AND u.displayname IS NOT NULL
                                  AND btrim(u.displayname) <> ''
                                  AND position(lower(btrim(u.displayname)) in lower($8)) > 0
                            )
                            -- .m.rule.contains_user_name（默认规则）：正文包含本
                            -- 地用户名（`@localpart:server` 的 localpart）。
                            OR position(lower(split_part(substr(m.user_id, 2), ':', 1)) in lower($8)) > 0
                            -- 用户自定义关键字规则（kind='content'）。
                            OR EXISTS (
                                SELECT 1 FROM push_rules pr
                                WHERE pr.user_id = m.user_id
                                  AND pr.scope = 'global'
                                  AND pr.kind = 'content'
                                  AND pr.is_enabled
                                  AND pr.pattern IS NOT NULL
                                  AND btrim(pr.pattern) <> ''
                                  AND pr.actions @> '[{"set_tweak":"highlight"}]'::jsonb
                                  AND position(lower(pr.pattern) in lower($8)) > 0
                            )
                        ) THEN 'highlight'
                        ELSE 'message' END,
                   FALSE, $6
            FROM room_memberships m
            WHERE m.room_id = $1
              AND m.membership = 'join'
              AND m.user_id <> $3
              AND right(m.user_id, length($7) + 1) = ':' || $7
              AND NOT EXISTS (
                  SELECT 1 FROM notifications n
                  WHERE n.user_id = m.user_id AND n.event_id = $2 AND COALESCE(n.is_read, FALSE) = FALSE
              )
              AND NOT EXISTS (
                  SELECT 1 FROM account_data ad
                  WHERE ad.user_id = m.user_id
                    AND ad.data_type = 'm.ignored_user_list'
                    AND ad.content -> 'ignored_users' ? $3
              )
              AND NOT EXISTS (
                  SELECT 1 FROM push_rules pr
                  WHERE pr.user_id = m.user_id
                    AND pr.scope = 'global'
                    AND pr.is_enabled
                    AND pr.actions ? 'dont_notify'
                    AND (
                        pr.rule_id = '.m.rule.master'
                        OR (
                            pr.kind = 'room'
                            AND (pr.pattern = $1 OR pr.rule_id = $1)
                        )
                    )
              )
            ON CONFLICT (user_id, event_id)
                WHERE COALESCE(is_read, FALSE) = FALSE AND event_id IS NOT NULL
                DO NOTHING
            "#,
        )
        .bind(room_id)
        .bind(event_id)
        .bind(sender)
        .bind(highlight_user_ids)
        .bind(highlight_room)
        .bind(ts)
        .bind(local_server_name)
        .bind(body)
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected())
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

    async fn delete_all_pushers_for_user(&self, user_id: &str) -> Result<u64, sqlx::Error> {
        self.delete_all_pushers_for_user(user_id).await
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
        before: Option<&str>,
        after: Option<&str>,
        now: i64,
    ) -> Result<(), PushRuleWriteError> {
        self.upsert_push_rule(user_id, scope, kind, rule_id, pattern, conditions, actions, before, after, now).await
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

    async fn count_push_rules_excluding(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<i64, sqlx::Error> {
        self.count_push_rules_excluding(user_id, scope, kind, rule_id).await
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

    async fn get_notifications(
        &self,
        user_id: &str,
        query: &NotificationPageQuery,
    ) -> Result<Vec<NotificationRow>, sqlx::Error> {
        self.get_notifications(user_id, query).await
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
mod db_tests;
