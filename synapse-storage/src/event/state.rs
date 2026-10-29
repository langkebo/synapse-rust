use super::models::*;

/// Shared SELECT column list for StateEvent outer queries.
/// Uses COALESCE wrappers so null columns don't break deserialization.
///
/// `pub(crate)` 只为 D14-2 的守卫（`schema_const_guard_tests`）：它要在这条常量被 `format!`
/// 拼进 SQL 的**同一个文本**上验证"能被 PG prepare + 输出列名等于消费结构体字段名"。
/// 常量本身仍是"只有一份"，没有复制（详见 §8.6 与模块文档）。
pub(crate) const STATE_EVENT_OUTER_COLS: &str =
    "event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key, \
     COALESCE(unsigned, '{}'::jsonb) as unsigned, \
     COALESCE(is_redacted, false) as is_redacted, \
     COALESCE(origin_server_ts, 0) as origin_server_ts, \
     depth, NULL::BIGINT as processed_at, not_before, status, origin, user_id, stream_ordering, \
     prev_events, auth_events, signatures, hashes";

/// Shared column list for StateEvent inner DISTINCT ON subqueries.
/// Raw columns (no COALESCE) — the outer query wraps them.
///
/// `pub(crate)`：同 `STATE_EVENT_OUTER_COLS`，为 D14-2 守卫。
pub(crate) const STATE_EVENT_INNER_COLS: &str =
    "event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key, \
     unsigned, is_redacted, origin_server_ts, depth, not_before, status, origin, user_id, stream_ordering, \
     prev_events, auth_events, signatures, hashes";

/// Defensive upper bound on the rows returned by the un-windowed batch state
/// queries (`get_state_events_batch`, `get_membership_state_keys_since_batch`).
///
/// These queries back `/sync` and are intentionally un-windowed, so a *small*
/// `LIMIT` would silently truncate sync state (a correctness regression). The
/// cap here is therefore set far above any realistic room-state volume purely
/// to bound memory on pathological input; hitting it is logged as a warning so
/// operators can spot the anomaly.
const STATE_BATCH_MAX_ROWS: usize = 50_000;

/// Temporary escape hatch for deployments that still have rooms without a
/// resolved-state record (A4/A1, 2026-10-30).
///
/// Current-state reads are **fail-closed** by default: a room with no
/// `state_groups` row yields an error instead of silently deriving a state whose
/// order is the sender's clock (`origin_server_ts`), which federated events can
/// contradict. Two supported ways to make a room readable — the write path seeds
/// a record on the room's first state event, and
/// `scripts/migration/backfill_state_groups.py` does it in bulk for pre-existing
/// rooms. This variable is the third: set it (any value) to keep the old
/// timestamp derivation while the backfill runs.
/// `pub(crate)` 只为 `event::db_tests` 的 A1 用例引用同一个字面量（避免名字漂移）。
pub(crate) const STATE_TS_FALLBACK_ENV: &str = "SYNAPSE_ALLOW_STATE_TS_FALLBACK";

/// Whether the timestamp derivation is allowed for a room without a record.
fn state_ts_fallback_allowed() -> bool {
    std::env::var_os(STATE_TS_FALLBACK_ENV).is_some()
}

/// The fail-closed error for a current-state read of a room with no record.
fn missing_state_group(room_id: &str, method: &str) -> sqlx::Error {
    sqlx::Error::Protocol(format!(
        "Room {room_id} has no state group record, so its current state has no DAG-ordered source \
         ({method}); the write path seeds one on the room's first state event, pre-existing rooms \
         need `scripts/migration/backfill_state_groups.py`, and {STATE_TS_FALLBACK_ENV}=1 restores \
         the origin_server_ts derivation (A4/A1)"
    ))
}

impl EventStorage {
    /// The room's **resolved-state group**, if its state has ever been resolved.
    ///
    /// `state_groups` is the resolved-state record maintained by
    /// `synapse_services::room::state_resolution`: the newest group for a room is
    /// its current state once a conflict has been resolved (MSC4297 v2.1).
    /// `None` means the room has no resolved record yet — current-state reads are
    /// then fail-closed (see [`missing_state_group`]) unless
    /// [`STATE_TS_FALLBACK_ENV`] is set.
    async fn current_state_group_id(&self, room_id: &str) -> Result<Option<i64>, sqlx::Error> {
        sqlx::query_scalar!(r#"SELECT id FROM state_groups WHERE room_id = $1 ORDER BY id DESC LIMIT 1"#, room_id,)
            .fetch_optional(&*self.pool)
            .await
    }

    /// The state events of one resolved-state group.
    ///
    /// `event_type` / `state_key` narrow the result to a single key (the
    /// `get_state_event` shape); `None` returns the whole group.
    async fn state_events_of_group(
        &self,
        state_group_id: i64,
        event_type: Option<&str>,
        state_key: Option<&str>,
    ) -> Result<Vec<StateEvent>, sqlx::Error> {
        sqlx::query_as!(
            StateEvent,
            r#"
            SELECT e.event_id, e.room_id, COALESCE(e.sender, e.user_id) AS "sender!", e.event_type,
                   e.content, e.state_key,
                   COALESCE(e.unsigned, '{}'::jsonb) AS unsigned,
                   COALESCE(e.is_redacted, false) AS is_redacted,
                   COALESCE(e.origin_server_ts, 0) AS "origin_server_ts!",
                   e.depth, NULL::BIGINT AS processed_ts, e.not_before, e.status, e.origin,
                   e.user_id, e.stream_ordering, e.prev_events, e.auth_events, e.signatures, e.hashes
            FROM events e
            JOIN state_group_state sgs ON sgs.event_id = e.event_id
            WHERE sgs.state_group_id = $1
              AND ($2::text IS NULL OR sgs.event_type = $2)
              AND ($3::text IS NULL OR sgs.state_key = $3)
            ORDER BY COALESCE(e.origin_server_ts, 0) DESC, e.event_id ASC
            "#,
            state_group_id,
            event_type,
            state_key,
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_state_event`].
    pub async fn get_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<StateEvent>, sqlx::Error> {
        if let Some(state_group_id) = self.current_state_group_id(room_id).await? {
            return Ok(self
                .state_events_of_group(state_group_id, Some(event_type), Some(state_key))
                .await?
                .into_iter()
                .next());
        }

        // A4/A1: fail closed. A room without a record has no DAG-ordered current
        // state; deriving one from `origin_server_ts` (the sender's clock) can
        // name the losing branch. The seeding write path and the backfill script
        // are the fixes; the env var is the temporary escape hatch.
        if !state_ts_fallback_allowed() {
            return Err(missing_state_group(room_id, "get_state_event"));
        }
        tracing::warn!(
            room_id,
            method = "get_state_event",
            env = STATE_TS_FALLBACK_ENV,
            "room has no resolved-state record; deriving current state from origin_server_ts"
        );
        self.derive_state_event(room_id, event_type, state_key).await
    }

    /// The `origin_server_ts` derivation for **one** key — the pre-A4 current-state
    /// source. Only reachable through the [`STATE_TS_FALLBACK_ENV`] escape hatch.
    async fn derive_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<StateEvent>, sqlx::Error> {
        sqlx::query_as::<_, StateEvent>(&format!(
            "SELECT {STATE_EVENT_OUTER_COLS} \
             FROM events \
             WHERE room_id = $1 \
               AND event_type = $2 \
               AND state_key = $3 \
               AND state_key IS NOT NULL \
             ORDER BY origin_server_ts DESC \
             LIMIT 1"
        ))
        .bind(room_id)
        .bind(event_type)
        .bind(state_key)
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`get_state_events`].
    pub async fn get_state_events(&self, room_id: &str) -> Result<Vec<StateEvent>, sqlx::Error> {
        if let Some(state_group_id) = self.current_state_group_id(room_id).await? {
            return self.state_events_of_group(state_group_id, None, None).await;
        }

        // A4/A1: fail closed, same as [`Self::get_state_event`].
        if !state_ts_fallback_allowed() {
            return Err(missing_state_group(room_id, "get_state_events"));
        }
        tracing::warn!(
            room_id,
            method = "get_state_events",
            env = STATE_TS_FALLBACK_ENV,
            "room has no resolved-state record; deriving current state from origin_server_ts"
        );
        self.derive_state_from_event_log(room_id).await
    }

    /// The room's current state **derived from the event log alone**
    /// (`DISTINCT ON (event_type, state_key) … origin_server_ts DESC`).
    ///
    /// This is *not* the room's current state whenever a resolved-state record
    /// exists — `origin_server_ts` is the sender's clock, and a federated state
    /// event may arrive late with an older stamp and still win the DAG. Two
    /// callers may use it:
    ///
    /// * the write path's one-time seeding (`seed_from_current_state`), whose
    ///   whole purpose is to freeze this derivation into a record before the read
    ///   path is switched over to it;
    /// * the [`STATE_TS_FALLBACK_ENV`] escape hatch, while a deployment backfills
    ///   its pre-existing rooms.
    ///
    /// Every other caller must go through [`Self::get_state_events`], which
    /// serves the record and fails closed when there is none.
    pub async fn derive_state_from_event_log(&self, room_id: &str) -> Result<Vec<StateEvent>, sqlx::Error> {
        sqlx::query_as::<_, StateEvent>(&format!(
            "SELECT {STATE_EVENT_OUTER_COLS} \
             FROM ( \
                 SELECT DISTINCT ON (event_type, state_key) \
                        {STATE_EVENT_INNER_COLS} \
                 FROM events \
                 WHERE room_id = $1 \
                   AND state_key IS NOT NULL \
                 ORDER BY event_type, state_key, origin_server_ts DESC \
             ) s \
             ORDER BY origin_server_ts DESC"
        ))
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_state_events_at_or_before`].
    pub async fn get_state_events_at_or_before(
        &self,
        room_id: &str,
        origin_server_ts: i64,
    ) -> Result<Vec<StateEvent>, sqlx::Error> {
        sqlx::query_as::<_, StateEvent>(&format!(
            "SELECT {STATE_EVENT_OUTER_COLS} \
             FROM ( \
                 SELECT DISTINCT ON (event_type, state_key) \
                        {STATE_EVENT_INNER_COLS} \
                 FROM events \
                 WHERE room_id = $1 \
                   AND state_key IS NOT NULL \
                   AND origin_server_ts <= $2 \
                 ORDER BY event_type, state_key, origin_server_ts DESC, event_id DESC \
             ) s \
             ORDER BY origin_server_ts DESC, event_id ASC"
        ))
        .bind(room_id)
        .bind(origin_server_ts)
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_state_events_by_type`].
    pub async fn get_state_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
    ) -> Result<Vec<StateEvent>, sqlx::Error> {
        // A resolved-state record is the room's current state for **every** key it
        // covers, so a type-scoped read must serve it too — otherwise a conflicted
        // singleton type (`m.room.join_rules`, `m.room.encryption`, …) could still
        // be read from the losing branch through this method.
        //
        // Deliberately **not** part of the A4/A1 fail-closed set: this method stays
        // derivation-based for a room with no record, because the sync/join paths
        // call it for rooms the record has not covered yet. Registered as the
        // remaining half of P1-1 in `docs/audit/REMAINING_PROBLEMS_AND_OPTIMIZATION_2026-10-29.md`.
        if let Some(state_group_id) = self.current_state_group_id(room_id).await? {
            return self.state_events_of_group(state_group_id, Some(event_type), None).await;
        }

        sqlx::query_as::<_, StateEvent>(&format!(
            "SELECT {STATE_EVENT_OUTER_COLS} \
             FROM ( \
                 SELECT DISTINCT ON (state_key) \
                        {STATE_EVENT_INNER_COLS} \
                 FROM events \
                 WHERE room_id = $1 \
                   AND event_type = $2 \
                   AND state_key IS NOT NULL \
                 ORDER BY state_key, origin_server_ts DESC \
                 LIMIT 5000 \
             ) s \
             ORDER BY origin_server_ts DESC"
        ))
        .bind(room_id)
        .bind(event_type)
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_state_events_by_type_batch`].
    pub async fn get_state_events_by_type_batch(
        &self,
        room_ids: &[String],
        event_type: &str,
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let events: Vec<StateEvent> = sqlx::query_as(&format!(
            "SELECT {STATE_EVENT_OUTER_COLS} \
             FROM ( \
                 SELECT DISTINCT ON (room_id, state_key) \
                        {STATE_EVENT_INNER_COLS} \
                 FROM events \
                 WHERE room_id = ANY($1) \
                   AND event_type = $2 \
                   AND state_key IS NOT NULL \
                 ORDER BY room_id, state_key, origin_server_ts DESC \
             ) s \
             ORDER BY room_id, origin_server_ts DESC \
             LIMIT 50000"
        ))
        .bind(room_ids)
        .bind(event_type)
        .fetch_all(&*self.pool)
        .await?;

        Ok(Self::group_state_events(room_ids, events))
    }

    /// See [`get_state_events_since_batch`].
    pub async fn get_state_events_since_batch(
        &self,
        room_ids: &[String],
        since: SinceFilter,
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let col = since.column();
        let events: Vec<StateEvent> = sqlx::query_as(&format!(
            "SELECT {STATE_EVENT_OUTER_COLS} \
             FROM ( \
                 SELECT DISTINCT ON (room_id, event_type, state_key) \
                        {STATE_EVENT_INNER_COLS} \
                 FROM events \
                 WHERE room_id = ANY($1) \
                   AND state_key IS NOT NULL \
                   AND {col} > $2 \
                 ORDER BY room_id, event_type, state_key, {col} DESC \
             ) s \
             ORDER BY room_id, {col} DESC \
             LIMIT 50000"
        ))
        .bind(room_ids)
        .bind(since.value())
        .fetch_all(&*self.pool)
        .await?;

        Ok(Self::group_state_events(room_ids, events))
    }

    /// See [`get_state_change_timestamps_batch`].
    pub async fn get_state_change_timestamps_batch(
        &self,
        room_ids: &[String],
        since: SinceFilter,
    ) -> Result<std::collections::HashMap<String, i64>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let col = since.column();
        let rows: Vec<(String, i64)> = sqlx::query_as(&format!(
            "SELECT room_id, MAX({col}) AS latest_ts \
             FROM events \
             WHERE room_id = ANY($1) \
               AND state_key IS NOT NULL \
               AND {col} > $2 \
             GROUP BY room_id"
        ))
        .bind(room_ids)
        .bind(since.value())
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    /// See [`get_state_events_batch`].
    pub async fn get_state_events_batch(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let events: Vec<StateEvent> = sqlx::query_as(&format!(
            "SELECT {STATE_EVENT_OUTER_COLS} \
             FROM ( \
                 SELECT DISTINCT ON (room_id, event_type, state_key) \
                        {STATE_EVENT_INNER_COLS} \
                 FROM events \
                 WHERE room_id = ANY($1) \
                   AND state_key IS NOT NULL \
                 ORDER BY room_id, event_type, state_key, origin_server_ts DESC \
             ) s \
             ORDER BY room_id, origin_server_ts DESC \
             LIMIT {STATE_BATCH_MAX_ROWS}"
        ))
        .bind(room_ids)
        .fetch_all(&*self.pool)
        .await?;

        if events.len() >= STATE_BATCH_MAX_ROWS {
            tracing::warn!(
                room_count = room_ids.len(),
                returned = events.len(),
                cap = STATE_BATCH_MAX_ROWS,
                "get_state_events_batch hit the defensive row cap; sync state may be truncated"
            );
        }

        let mut result: std::collections::HashMap<String, Vec<StateEvent>> =
            std::collections::HashMap::with_capacity(room_ids.len());
        for id in room_ids {
            result.insert(id.clone(), Vec::new());
        }

        for event in events {
            if let Some(room_events) = result.get_mut(&event.room_id) {
                room_events.push(event);
            }
        }

        Ok(result)
    }

    // -----------------------------------------------------------------------
    // Membership state keys
    // -----------------------------------------------------------------------

    /// See [`get_membership_state_keys_since_batch`].
    pub async fn get_membership_state_keys_since_batch(
        &self,
        room_ids: &[String],
        since: SinceFilter,
    ) -> Result<std::collections::HashMap<String, std::collections::HashSet<String>>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let col = since.column();
        let rows: Vec<(String, String)> = sqlx::query_as(&format!(
            "SELECT room_id, state_key \
             FROM ( \
                 SELECT DISTINCT ON (room_id, state_key) \
                        room_id, state_key \
                 FROM events \
                 WHERE room_id = ANY($1) \
                   AND {col} > $2 \
                   AND event_type = 'm.room.member' \
                   AND state_key IS NOT NULL \
                 ORDER BY room_id, state_key, {col} DESC \
             ) recent_membership \
             LIMIT {STATE_BATCH_MAX_ROWS}"
        ))
        .bind(room_ids)
        .bind(since.value())
        .fetch_all(&*self.pool)
        .await?;

        if rows.len() >= STATE_BATCH_MAX_ROWS {
            tracing::warn!(
                room_count = room_ids.len(),
                returned = rows.len(),
                cap = STATE_BATCH_MAX_ROWS,
                "get_membership_state_keys_since_batch hit the defensive row cap; sync membership keys may be truncated"
            );
        }

        let mut result: std::collections::HashMap<String, std::collections::HashSet<String>> =
            room_ids.iter().map(|room_id| (room_id.clone(), std::collections::HashSet::new())).collect();

        for (room_id, state_key) in rows {
            result.entry(room_id).or_default().insert(state_key);
        }

        Ok(result)
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn group_state_events(
        room_ids: &[String],
        events: Vec<StateEvent>,
    ) -> std::collections::HashMap<String, Vec<StateEvent>> {
        let mut result: std::collections::HashMap<String, Vec<StateEvent>> =
            std::collections::HashMap::with_capacity(room_ids.len());
        for id in room_ids {
            result.insert(id.clone(), Vec::new());
        }

        for event in events {
            result.entry(event.room_id.clone()).or_default().push(event);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_state_event(event_id: &str, room_id: &str, event_type: &str, state_key: &str) -> StateEvent {
        StateEvent {
            event_id: event_id.into(),
            room_id: room_id.into(),
            sender: "@alice:ex.com".into(),
            event_type: Some(event_type.into()),
            content: json!({"membership": "join"}),
            state_key: Some(state_key.into()),
            unsigned: None,
            is_redacted: Some(false),
            origin_server_ts: 1000,
            depth: Some(1),
            processed_ts: None,
            not_before: None,
            status: None,
            origin: Some("self".into()),
            user_id: Some("@alice:ex.com".into()),
            stream_ordering: Some(1),
            prev_events: None,
            auth_events: None,
            signatures: None,
            hashes: None,
        }
    }

    #[test]
    fn group_state_events_basic() {
        let room_ids = vec!["!a:ex.com".into(), "!b:ex.com".into()];
        let events = vec![
            make_state_event("$1", "!a:ex.com", "m.room.name", ""),
            make_state_event("$2", "!b:ex.com", "m.room.topic", ""),
        ];
        let result = EventStorage::group_state_events(&room_ids, events);
        assert_eq!(result.get("!a:ex.com").unwrap().len(), 1);
        assert_eq!(result.get("!b:ex.com").unwrap().len(), 1);
    }

    #[test]
    fn group_state_events_empty() {
        let result = EventStorage::group_state_events(&[], vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn group_state_events_initializes_empty_vecs_for_all_rooms() {
        let room_ids = vec!["!a:ex.com".into(), "!b:ex.com".into()];
        let events = vec![make_state_event("$1", "!a:ex.com", "m.room.name", "")];
        let result = EventStorage::group_state_events(&room_ids, events);
        assert_eq!(result.get("!a:ex.com").unwrap().len(), 1);
        assert_eq!(result.get("!b:ex.com").unwrap().len(), 0);
    }
}
