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
            .upsert_push_rule(
                &user_id,
                "global",
                "override",
                "rule1",
                &None,
                &None,
                &json!(["notify"]),
                None,
                None,
                now,
            )
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
                None,
                None,
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
                None,
                None,
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
            .upsert_push_rule(
                &user_id,
                "global",
                "override",
                "rule_actions",
                &None,
                &None,
                &json!(["notify"]),
                None,
                None,
                now,
            )
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
            .upsert_push_rule(
                &user_id,
                "global",
                "override",
                "rule_del",
                &None,
                &None,
                &json!(["notify"]),
                None,
                None,
                now,
            )
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
            .upsert_push_rule(
                &user_id,
                "global",
                "override",
                "rule_toggle",
                &None,
                &None,
                &json!(["notify"]),
                None,
                None,
                now,
            )
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
            .upsert_push_rule(&user_id, "global", "override", "r1", &None, &None, &json!(["notify"]), None, None, now)
            .await
            .expect("upsert r1");
        storage
            .upsert_push_rule(&user_id, "global", "content", "r2", &None, &None, &json!(["notify"]), None, None, now)
            .await
            .expect("upsert r2");
        storage
            .upsert_push_rule(
                &user_id,
                "devices/DEV1",
                "override",
                "r3",
                &None,
                &None,
                &json!(["notify"]),
                None,
                None,
                now,
            )
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

        let rows = storage
            .get_notifications(&user_id, &NotificationPageQuery::first(10))
            .await
            .expect("get_notifications should succeed");
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

        let rows = storage
            .get_notifications(&user_id, &NotificationPageQuery::first(3))
            .await
            .expect("get_notifications should succeed");
        assert_eq!(rows.len(), 3, "should respect limit");

        cleanup_notifications(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_notifications_empty_for_unknown_user() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@unknown");

        let rows = storage
            .get_notifications(&user_id, &NotificationPageQuery::first(10))
            .await
            .expect("get_notifications should succeed");
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

        let rows =
            storage.get_notifications(&user_id, &NotificationPageQuery::first(10)).await.expect("get_notifications");
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
        let rows =
            storage.get_notifications(&user_id, &NotificationPageQuery::first(10)).await.expect("get_notifications");
        assert_eq!(rows.len(), 1, "repeated attempts for one unread event must not duplicate");

        storage.ack_notification(rows[0].id, &user_id, now + 10).await.expect("ack_notification");
        storage
            .record_notification(&user_id, Some("$ev_dup"), Some("!room1:test.com"), "message", now + 11)
            .await
            .expect("record_notification after ack must succeed");
        let rows =
            storage.get_notifications(&user_id, &NotificationPageQuery::first(10)).await.expect("get_notifications");
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

        let rows =
            storage.get_notifications(&user_id, &NotificationPageQuery::first(10)).await.expect("get_notifications");
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
    /// B-34: `before` / `after` define the order within a kind; with neither, a
    /// **new** rule appends to the end of its kind while an **existing** rule keeps
    /// its position (a plain body update must not reorder the list).
    #[tokio::test]
    async fn test_upsert_push_rule_positions_rules_within_kind() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@pushorder");
        cleanup_push_rules(&pool, &user_id).await;

        async fn put(storage: &PushStorage, user_id: &str, rule_id: &str, before: Option<&str>, after: Option<&str>) {
            storage
                .upsert_push_rule(
                    user_id,
                    "global",
                    "override",
                    rule_id,
                    &None,
                    &None,
                    &json!(["notify"]),
                    before,
                    after,
                    current_timestamp_millis(),
                )
                .await
                .expect("positioned upsert should succeed");
        }

        async fn order(storage: &PushStorage, user_id: &str) -> Vec<String> {
            storage
                .get_user_push_rules(user_id, "global", "override")
                .await
                .expect("get_user_push_rules should succeed")
                .into_iter()
                .map(|r| r.rule_id)
                .collect()
        }

        // No anchor: each new rule appends, so the order is insertion order — not
        // `rule_id` order (the pre-B-34 behaviour this feature replaces).
        put(&storage, &user_id, "zzz", None, None).await;
        put(&storage, &user_id, "aaa", None, None).await;
        put(&storage, &user_id, "mmm", None, None).await;
        assert_eq!(order(&storage, &user_id).await, vec!["zzz", "aaa", "mmm"]);

        // `after` = next-less important → immediately after the anchor.
        put(&storage, &user_id, "d", None, Some("zzz")).await;
        assert_eq!(order(&storage, &user_id).await, vec!["zzz", "d", "aaa", "mmm"]);

        // `before` = next-most important → immediately before the anchor.
        put(&storage, &user_id, "e", Some("aaa"), None).await;
        assert_eq!(order(&storage, &user_id).await, vec!["zzz", "d", "e", "aaa", "mmm"]);

        // Updating an existing rule without anchors leaves its position untouched.
        put(&storage, &user_id, "aaa", None, None).await;
        assert_eq!(order(&storage, &user_id).await, vec!["zzz", "d", "e", "aaa", "mmm"]);

        // An existing rule may be moved: it is removed before the anchor is resolved.
        put(&storage, &user_id, "zzz", Some("mmm"), None).await;
        assert_eq!(order(&storage, &user_id).await, vec!["d", "e", "aaa", "zzz", "mmm"]);

        cleanup_push_rules(&pool, &user_id).await;
    }

    /// `before` + `after` together is client input we cannot honour → typed error
    /// (the service maps it to `400 M_INVALID_PARAM`), not a silent pick.
    #[tokio::test]
    async fn test_upsert_push_rule_rejects_conflicting_position() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@pushorder");
        cleanup_push_rules(&pool, &user_id).await;

        let err = storage
            .upsert_push_rule(
                &user_id,
                "global",
                "override",
                "conflict",
                &None,
                &None,
                &json!(["notify"]),
                Some("a"),
                Some("b"),
                current_timestamp_millis(),
            )
            .await
            .expect_err("both anchors must be rejected");
        assert!(matches!(err, PushRuleWriteError::ConflictingPosition), "unexpected error: {err}");
    }

    /// An unknown anchor — including a server default rule, which is not a row in
    /// this table — is a client error, not a database error.
    #[tokio::test]
    #[allow(clippy::panic, reason = "test-only: an unexpected error variant is reported via panic! in a match arm")]
    async fn test_upsert_push_rule_rejects_unknown_anchor() {
        let (_isolated, pool) = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@pushorder");
        cleanup_push_rules(&pool, &user_id).await;

        let err = storage
            .upsert_push_rule(
                &user_id,
                "global",
                "override",
                "orphan",
                &None,
                &None,
                &json!(["notify"]),
                Some(".m.rule.master"),
                None,
                current_timestamp_millis(),
            )
            .await
            .expect_err("unknown anchor must be rejected");
        match err {
            PushRuleWriteError::AnchorNotFound { anchor } => assert_eq!(anchor, ".m.rule.master"),
            other => panic!("expected AnchorNotFound, got {other}"),
        }

        // The rejected write must not leave a partial row behind.
        let rows = storage.get_user_push_rules(&user_id, "global", "override").await.expect("get_user_push_rules");
        assert!(rows.is_empty(), "a failed positioned write must not insert the rule");
    }
}
