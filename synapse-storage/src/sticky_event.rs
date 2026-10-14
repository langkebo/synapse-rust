// Sticky Event Storage - MSC4354
// Stores is_sticky event metadata for rooms
// Following project field naming standards

use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

/// MSC4354's upper bound on a sticky duration: **1 hour**.
///
/// Mirrors upstream `synapse/api/constants.py::StickyEvent.MAX_DURATION`
/// (`Duration(hours=1)`), which is the value the reference implementation
/// enforces for `org.matrix.msc4354.sticky_duration_ms`.
pub const MAX_STICKY_DURATION_MS: i64 = 3_600_000;

/// A client-requested sticky duration was not acceptable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StickyDurationError {
    /// Zero or negative durations are meaningless: the pin would expire in the past.
    NotPositive(i64),
    /// Beyond [`MAX_STICKY_DURATION_MS`].
    TooLong {
        /// What the client asked for.
        requested: i64,
        /// The cap that was exceeded.
        max: i64,
    },
}

impl std::fmt::Display for StickyDurationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotPositive(requested) => {
                write!(f, "sticky_duration_ms must be greater than zero, got {requested}")
            }
            Self::TooLong { requested, max } => {
                write!(f, "sticky_duration_ms {requested} exceeds the MSC4354 maximum of {max}")
            }
        }
    }
}

impl std::error::Error for StickyDurationError {}

/// A sticky pin's expiry, as **resolved by the server**.
///
/// Both fields are derived once, at write time, and then persisted verbatim:
/// [`Self::duration_ms`] is what gets reported back to the client and
/// [`Self::expires_at`] is what the read paths compare against "now". Keeping
/// them in one value is the whole point of upstream changelog **20324** — when
/// the reported duration and the tracked expiry are computed independently they
/// drift, and clients see a pin that outlives (or dies before) the value they
/// were told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StickyExpiry {
    /// Requested sticky duration in milliseconds.
    pub duration_ms: i64,
    /// Absolute server-side expiry: `now_ms + duration_ms`.
    pub expires_at: i64,
}

/// Validate a client-requested sticky duration and resolve it into a
/// [`StickyExpiry`].
///
/// `Ok(None)` means the caller did not ask for an expiry, so the pin never
/// expires — that is the pre-20324 behaviour and stays the default.
///
/// The returned `expires_at` is computed from the *same* `now_ms` used for the
/// row's `created_ts`, so `expires_at - created_ts == duration_ms` holds exactly
/// and cannot drift from what clients are told.
pub fn resolve_sticky_expiry(now_ms: i64, requested: Option<i64>) -> Result<Option<StickyExpiry>, StickyDurationError> {
    let Some(duration_ms) = requested else {
        return Ok(None);
    };
    if duration_ms <= 0 {
        return Err(StickyDurationError::NotPositive(duration_ms));
    }
    if duration_ms > MAX_STICKY_DURATION_MS {
        return Err(StickyDurationError::TooLong { requested: duration_ms, max: MAX_STICKY_DURATION_MS });
    }
    Ok(Some(StickyExpiry { duration_ms, expires_at: now_ms.saturating_add(duration_ms) }))
}

/// Failure modes of [`StickyEventStorage::set_is_sticky_event`].
#[derive(Debug)]
pub enum SetStickyEventError {
    /// The requested duration was rejected (client error — surface as 400).
    Duration(StickyDurationError),
    /// Persistence failed (server error — surface as 500).
    Database(sqlx::Error),
}

impl std::fmt::Display for SetStickyEventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duration(e) => write!(f, "{e}"),
            Self::Database(e) => write!(f, "failed to persist sticky event: {e}"),
        }
    }
}

impl std::error::Error for SetStickyEventError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Duration(e) => Some(e),
            Self::Database(e) => Some(e),
        }
    }
}

impl From<sqlx::Error> for SetStickyEventError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}

/// The `StickyEventStorage` struct.
#[derive(Clone)]
pub struct StickyEventStorage {
    pool: Arc<PgPool>,
}

impl StickyEventStorage {
    /// See [`new`].
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Set is_sticky event metadata for a room.
    ///
    /// `sticky_duration_ms` is the optional MSC4354 duration
    /// (`org.matrix.msc4354.sticky_duration_ms`). `None` means "never expires"
    /// and is stored as SQL NULL in both `sticky_duration_ms` and `expires_at`.
    ///
    /// # Errors
    ///
    /// Returns [`SetStickyEventError::Duration`] when the duration is not a
    /// positive number of milliseconds, or exceeds [`MAX_STICKY_DURATION_MS`].
    pub async fn set_is_sticky_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        event_type: &str,
        is_sticky: bool,
        sticky_duration_ms: Option<i64>,
    ) -> Result<(), SetStickyEventError> {
        let now = current_timestamp_millis();
        let expiry = resolve_sticky_expiry(now, sticky_duration_ms).map_err(SetStickyEventError::Duration)?;
        let (duration_ms, expires_at) = match expiry {
            Some(e) => (Some(e.duration_ms), Some(e.expires_at)),
            None => (None, None),
        };

        sqlx::query!(
            r"
            INSERT INTO room_sticky_events
                (room_id, user_id, event_id, event_type, is_sticky, sticky_duration_ms, expires_at, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (room_id, user_id, event_type)
            DO UPDATE SET event_id = EXCLUDED.event_id,
                          is_sticky = EXCLUDED.is_sticky,
                          sticky_duration_ms = EXCLUDED.sticky_duration_ms,
                          expires_at = EXCLUDED.expires_at,
                          updated_ts = EXCLUDED.updated_ts
            ",
            room_id,
            user_id,
            event_id,
            event_type,
            is_sticky,
            duration_ms,
            expires_at,
            now,
            now,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Get is_sticky event for a room and user by event type.
    ///
    /// Pins whose `expires_at` has passed are **not** returned: this predicate —
    /// not the stored `is_sticky` flag — is what "the server tracks".
    pub async fn get_is_sticky_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
    ) -> Result<Option<StickyEvent>, sqlx::Error> {
        let now = current_timestamp_millis();
        // `StickyEvent` 的字段与投影列一一对应 ⇒ 直接 `query_as!`（无需元组/手工映射）。
        sqlx::query_as!(
            StickyEvent,
            r"
            SELECT room_id, user_id, event_id, event_type, is_sticky,
                   sticky_duration_ms, expires_at, created_ts, updated_ts
            FROM room_sticky_events
            WHERE room_id = $1 AND user_id = $2 AND event_type = $3 AND is_sticky = true
              AND (expires_at IS NULL OR expires_at > $4)
            ",
            room_id,
            user_id,
            event_type,
            now,
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// Get all is_sticky events for a room and user.
    ///
    /// Expired pins are filtered out, same as [`Self::get_is_sticky_event`].
    pub async fn get_all_is_sticky_events(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Vec<StickyEvent>, sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query_as!(
            StickyEvent,
            r"
            SELECT room_id, user_id, event_id, event_type, is_sticky,
                   sticky_duration_ms, expires_at, created_ts, updated_ts
            FROM room_sticky_events
            WHERE room_id = $1 AND user_id = $2 AND is_sticky = true
              AND (expires_at IS NULL OR expires_at > $3)
            ORDER BY event_type
            ",
            room_id,
            user_id,
            now,
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// Clear is_sticky event for a room, user, and event type
    pub async fn clear_is_sticky_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query!(
            r"
            UPDATE room_sticky_events
            SET is_sticky = false, updated_ts = $4
            WHERE room_id = $1 AND user_id = $2 AND event_type = $3
            ",
            room_id,
            user_id,
            event_type,
            now,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Get rooms with is_sticky events for a user (for sync).
    ///
    /// Rooms whose only pins have already expired are omitted, so a client is
    /// never told to sync a room on account of a stale pin.
    pub async fn get_rooms_with_is_sticky_events(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let now = current_timestamp_millis();
        let rows: Vec<String> = sqlx::query_scalar!(
            r"
            SELECT DISTINCT room_id FROM room_sticky_events
            WHERE user_id = $1 AND is_sticky = true
              AND (expires_at IS NULL OR expires_at > $2)
            ",
            user_id,
            now,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }
}

/// Sticky Event - Following project field naming standards
/// - created_ts: NOT NULL, milliseconds timestamp
/// - updated_ts: NOT NULL (or NULLABLE if appropriate), milliseconds timestamp
/// - sticky_duration_ms / expires_at: NULL together (never expires) or both set
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StickyEvent {
    /// The `room_id` field.
    pub room_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `is_sticky` field.
    pub is_sticky: bool,
    /// Client-requested sticky duration in milliseconds, or `None` for "never expires".
    pub sticky_duration_ms: Option<i64>,
    /// Absolute server-side expiry (`created_ts + sticky_duration_ms`), or `None`.
    ///
    /// This is the value the read paths compare against "now" **and** the value
    /// reported to clients, so the two can never disagree (upstream 20324).
    pub expires_at: Option<i64>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_sticky_event_struct() {
        let event = StickyEvent {
            room_id: "!room:localhost".to_string(),
            user_id: "@user:localhost".to_string(),
            event_id: "$event:localhost".to_string(),
            event_type: "m.room.message".to_string(),
            is_sticky: true,
            sticky_duration_ms: None,
            expires_at: None,
            created_ts: 1700000000000i64,
            updated_ts: 1700000000000i64,
        };

        assert_eq!(event.room_id, "!room:localhost");
        assert!(event.is_sticky);
        assert_eq!(event.expires_at, None, "no requested duration must mean 'never expires'");
    }

    #[test]
    fn test_event_type_validation() {
        let valid_types = vec!["m.room.message", "m.room.topic", "m.room.avatar"];

        for event_type in valid_types {
            assert!(event_type.starts_with("m.") || event_type.starts_with("com."));
        }
    }

    // --- `resolve_sticky_expiry`: pure, no DB ---

    #[test]
    fn test_resolve_sticky_expiry_absent_means_missing_expiry() {
        assert_eq!(resolve_sticky_expiry(1_000, None).expect("no duration is valid"), None);
    }

    #[test]
    fn test_resolve_sticky_expiry_is_created_ts_plus_duration() {
        // The whole point of 20324: the expiry the server tracks is *derived from
        // the same instant* as the row's created_ts, so the reported duration and
        // the tracked deadline cannot drift apart.
        let resolved = resolve_sticky_expiry(1_000, Some(60_000)).expect("valid duration").expect("some expiry");
        assert_eq!(resolved.duration_ms, 60_000);
        assert_eq!(resolved.expires_at, 61_000);
        assert_eq!(resolved.expires_at - 1_000, resolved.duration_ms);
    }

    #[test]
    fn test_resolve_sticky_expiry_accepts_the_maximum() {
        let resolved = resolve_sticky_expiry(0, Some(MAX_STICKY_DURATION_MS)).expect("the cap itself is allowed");
        assert_eq!(resolved.expect("some expiry").expires_at, MAX_STICKY_DURATION_MS);
    }

    #[test]
    fn test_resolve_sticky_expiry_rejects_non_positive() {
        for requested in [0_i64, -1, i64::MIN] {
            assert_eq!(
                resolve_sticky_expiry(0, Some(requested)),
                Err(StickyDurationError::NotPositive(requested)),
                "{requested} ms must be rejected"
            );
        }
    }

    #[test]
    fn test_resolve_sticky_expiry_rejects_beyond_the_msc4354_cap() {
        let requested = MAX_STICKY_DURATION_MS + 1;
        assert_eq!(
            resolve_sticky_expiry(0, Some(requested)),
            Err(StickyDurationError::TooLong { requested, max: MAX_STICKY_DURATION_MS }),
        );
    }

    #[test]
    fn test_resolve_sticky_expiry_saturates_instead_of_wrapping() {
        // A near-i64::MAX "now" plus a valid duration must not wrap into a
        // negative expiry (which would make the pin live forever).
        let resolved = resolve_sticky_expiry(i64::MAX - 10, Some(60_000)).expect("valid duration");
        assert_eq!(resolved.expect("some expiry").expires_at, i64::MAX);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;

    /// Shared `public` is deliberately replaced by a per-test schema here:
    /// the delete helpers are keyed on `(room_id, user_id)`, but shared `public` still let
    /// a sibling purge that row between this test's write and its read.
    ///
    /// Eliminating the shared state removes the race instead of serialising around it
    /// (AGENTS.md rule 7). The guard is returned with the pool so the schema outlives
    /// the whole test — dropping it early spawns a background `DROP SCHEMA`.
    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<PgPool>) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = isolated.pool();
        (isolated, pool)
    }

    /// Insert a minimal user row to satisfy the FK from room_sticky_events.user_id.
    async fn ensure_test_user(pool: &PgPool, user_id: &str) {
        let now = current_timestamp_millis();
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
        sqlx::query(
            r#"INSERT INTO users (user_id, username, created_ts)
               VALUES ($1, $2, $3)
               ON CONFLICT (user_id) DO NOTHING"#,
        )
        .bind(user_id)
        .bind(username)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test user");
    }

    /// Insert a minimal room row to satisfy the FK from room_sticky_events.room_id.
    async fn ensure_test_room(pool: &PgPool, room_id: &str) {
        let now = current_timestamp_millis();
        sqlx::query(
            r#"INSERT INTO rooms (room_id, created_ts)
               VALUES ($1, $2)
               ON CONFLICT (room_id) DO NOTHING"#,
        )
        .bind(room_id)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test room");
    }

    /// Delete sticky events for a given room+user combo (idempotent cleanup).
    async fn cleanup_sticky(pool: &PgPool, room_id: &str, user_id: &str) {
        sqlx::query("DELETE FROM room_sticky_events WHERE room_id = $1 AND user_id = $2")
            .bind(room_id)
            .bind(user_id)
            .execute(pool)
            .await
            .expect("failed to cleanup sticky events");
    }

    /// Insert a minimal `events` row (NOT NULL columns only) so that a redaction
    /// has a real row to act on.
    async fn ensure_test_event(pool: &PgPool, room_id: &str, sender: &str, event_id: &str, event_type: &str) {
        let now = current_timestamp_millis();
        sqlx::query(
            r#"INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts)
               VALUES ($1, $2, $3, $4, '{}'::jsonb, $5)
               ON CONFLICT (event_id) DO NOTHING"#,
        )
        .bind(event_id)
        .bind(room_id)
        .bind(sender)
        .bind(event_type)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test event");
    }

    // --- Tests ---

    #[tokio::test]
    async fn test_set_and_get_sticky_event() {
        let (_isolated, pool) = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_get_{suffix}:test.com");
        let user_id = &format!("@user_get_{suffix}:test.com");
        let event_id = &format!("$event_get_{suffix}:test.com");
        let event_type = "m.room.message";

        // Cleanup at start (handles leftover from previous aborted run)
        cleanup_sticky(&pool, room_id, user_id).await;
        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;

        storage
            .set_is_sticky_event(room_id, user_id, event_id, event_type, true, None)
            .await
            .expect("set should succeed");

        let found = storage
            .get_is_sticky_event(room_id, user_id, event_type)
            .await
            .expect("get should succeed")
            .expect("sticky event should be found");

        assert_eq!(found.room_id, *room_id);
        assert_eq!(found.user_id, *user_id);
        assert_eq!(found.event_id, *event_id);
        assert_eq!(found.event_type, event_type);
        assert!(found.is_sticky);
        assert!(found.created_ts > 0);
        assert!(found.updated_ts > 0);

        // Cleanup at end
        cleanup_sticky(&pool, room_id, user_id).await;
    }

    #[tokio::test]
    async fn test_get_sticky_event_not_found() {
        let (_isolated, pool) = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_nf_{suffix}:test.com");
        let user_id = &format!("@user_nf_{suffix}:test.com");

        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;

        let result = storage.get_is_sticky_event(room_id, user_id, "m.room.topic").await.expect("query should succeed");

        assert!(result.is_none(), "non-existent event should return None");

        cleanup_sticky(&pool, room_id, user_id).await;
    }

    #[tokio::test]
    async fn test_update_sticky_event() {
        let (_isolated, pool) = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_upd_{suffix}:test.com");
        let user_id = &format!("@user_upd_{suffix}:test.com");
        let event_id_a = &format!("$event_a_{suffix}:test.com");
        let event_id_b = &format!("$event_b_{suffix}:test.com");
        let event_type = "m.room.message";

        cleanup_sticky(&pool, room_id, user_id).await;
        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;

        // Set initial sticky event
        storage
            .set_is_sticky_event(room_id, user_id, event_id_a, event_type, true, None)
            .await
            .expect("first set should succeed");

        // Update with a different event_id (same room/user/type triggers ON CONFLICT DO UPDATE)
        storage
            .set_is_sticky_event(room_id, user_id, event_id_b, event_type, true, None)
            .await
            .expect("second set should succeed");

        let found = storage
            .get_is_sticky_event(room_id, user_id, event_type)
            .await
            .expect("get should succeed")
            .expect("updated sticky event should be found");

        assert_eq!(found.event_id, *event_id_b, "event_id should be updated to the new value");

        cleanup_sticky(&pool, room_id, user_id).await;
    }

    #[tokio::test]
    async fn test_clear_sticky_event() {
        let (_isolated, pool) = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_clr_{suffix}:test.com");
        let user_id = &format!("@user_clr_{suffix}:test.com");
        let event_id = &format!("$event_clr_{suffix}:test.com");
        let event_type = "m.room.message";

        cleanup_sticky(&pool, room_id, user_id).await;
        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;

        // Set and verify
        storage
            .set_is_sticky_event(room_id, user_id, event_id, event_type, true, None)
            .await
            .expect("set should succeed");
        assert!(storage.get_is_sticky_event(room_id, user_id, event_type).await.unwrap().is_some());

        // Clear it
        storage.clear_is_sticky_event(room_id, user_id, event_type).await.expect("clear should succeed");

        // Should no longer be returned (is_sticky = false)
        let result = storage.get_is_sticky_event(room_id, user_id, event_type).await.expect("query should succeed");
        assert!(result.is_none(), "cleared event should not be returned");

        cleanup_sticky(&pool, room_id, user_id).await;
    }

    #[tokio::test]
    async fn test_get_all_sticky_events() {
        let (_isolated, pool) = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_all_{suffix}:test.com");
        let user_id = &format!("@user_all_{suffix}:test.com");
        let event_id1 = &format!("$ev1_{suffix}:test.com");
        let event_id2 = &format!("$ev2_{suffix}:test.com");

        cleanup_sticky(&pool, room_id, user_id).await;
        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;

        storage
            .set_is_sticky_event(room_id, user_id, event_id1, "m.room.message", true, None)
            .await
            .expect("set 1 should succeed");
        storage
            .set_is_sticky_event(room_id, user_id, event_id2, "m.room.topic", true, None)
            .await
            .expect("set 2 should succeed");

        let all = storage.get_all_is_sticky_events(room_id, user_id).await.expect("get_all should succeed");

        assert_eq!(all.len(), 2, "should return both sticky events");
        assert!(all.iter().any(|e| e.event_type == "m.room.message"));
        assert!(all.iter().any(|e| e.event_type == "m.room.topic"));
        assert!(all.iter().all(|e| e.is_sticky));

        cleanup_sticky(&pool, room_id, user_id).await;
    }

    #[tokio::test]
    async fn test_get_rooms_with_sticky_events() {
        let (_isolated, pool) = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let user_id = &format!("@user_rooms_{suffix}:test.com");
        let room_a = &format!("!room_a_{suffix}:test.com");
        let room_b = &format!("!room_b_{suffix}:test.com");
        let event_id = &format!("$ev_rooms_{suffix}:test.com");

        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_a).await;
        ensure_test_room(&pool, room_b).await;

        // Cleanup both room+user combos
        cleanup_sticky(&pool, room_a, user_id).await;
        cleanup_sticky(&pool, room_b, user_id).await;

        storage
            .set_is_sticky_event(room_a, user_id, event_id, "m.room.message", true, None)
            .await
            .expect("set room_a should succeed");
        storage
            .set_is_sticky_event(room_b, user_id, event_id, "m.room.topic", true, None)
            .await
            .expect("set room_b should succeed");

        let rooms = storage.get_rooms_with_is_sticky_events(user_id).await.expect("get_rooms should succeed");

        assert_eq!(rooms.len(), 2, "should return both rooms");
        assert!(rooms.contains(&room_a.to_string()));
        assert!(rooms.contains(&room_b.to_string()));

        cleanup_sticky(&pool, room_a, user_id).await;
        cleanup_sticky(&pool, room_b, user_id).await;
    }

    #[tokio::test]
    async fn test_sticky_event_round_trip() {
        let (_isolated, pool) = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_rt_{suffix}:test.com");
        let user_id = &format!("@user_rt_{suffix}:test.com");
        let event_id = &format!("$ev_rt_{suffix}:test.com");
        let event_type = "m.room.message";

        cleanup_sticky(&pool, room_id, user_id).await;
        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;

        // Set -> Get -> Clear -> Get (should be gone) -> Set again -> Get (should be back)
        storage
            .set_is_sticky_event(room_id, user_id, event_id, event_type, true, None)
            .await
            .expect("set 1 should succeed");

        let found = storage
            .get_is_sticky_event(room_id, user_id, event_type)
            .await
            .unwrap()
            .expect("should be found after set 1");
        assert_eq!(found.event_id, *event_id);

        storage.clear_is_sticky_event(room_id, user_id, event_type).await.expect("clear should succeed");

        let after_clear = storage.get_is_sticky_event(room_id, user_id, event_type).await.unwrap();
        assert!(after_clear.is_none(), "should be gone after clear");

        // Re-set (same room/user/type — row still exists with is_sticky=false, ON CONFLICT DO UPDATE flips it back)
        storage
            .set_is_sticky_event(room_id, user_id, event_id, event_type, true, None)
            .await
            .expect("set 2 should succeed");

        let found_again = storage
            .get_is_sticky_event(room_id, user_id, event_type)
            .await
            .unwrap()
            .expect("should be found after set 2");
        assert_eq!(found_again.event_id, *event_id);
        assert!(found_again.is_sticky);

        cleanup_sticky(&pool, room_id, user_id).await;
    }

    /// MSC4354 Sticky Events (upstream #20323): **redacting an event removes its
    /// stickiness**, even when the redaction lands long after the event was
    /// persisted. Before the fix the pin survived the redaction, so sync /
    /// sliding-sync / `GET /sticky_events` kept advertising an event whose
    /// content had been wiped.
    ///
    /// This drives the real redaction entry point (`EventStorage::redact_event_content`)
    /// rather than a hand-written `UPDATE room_sticky_events`, because the whole
    /// point of the fix is that *that* function owns the cleanup — a test that
    /// deletes the row itself would pass even with the fix reverted.
    #[tokio::test]
    async fn test_redacting_an_event_removes_its_stickiness() {
        let (_isolated, pool) = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let events = crate::event::EventStorage::new(&pool, "test.localhost".to_string());

        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_unstick_{suffix}:test.com");
        let user_id = &format!("@user_unstick_{suffix}:test.com");
        let pinned_event = &format!("$pinned_{suffix}:test.com");
        let untouched_event = &format!("$untouched_{suffix}:test.com");

        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;
        ensure_test_event(&pool, room_id, user_id, pinned_event, "m.room.message").await;
        ensure_test_event(&pool, room_id, user_id, untouched_event, "m.room.topic").await;
        cleanup_sticky(&pool, room_id, user_id).await;

        storage
            .set_is_sticky_event(room_id, user_id, pinned_event, "m.room.message", true, None)
            .await
            .expect("pinning the first event should succeed");
        storage
            .set_is_sticky_event(room_id, user_id, untouched_event, "m.room.topic", true, None)
            .await
            .expect("pinning the second event should succeed");
        assert_eq!(
            storage.get_all_is_sticky_events(room_id, user_id).await.unwrap().len(),
            2,
            "both events must start out pinned, otherwise the assertion below is vacuous"
        );

        events.redact_event_content(pinned_event, None).await.expect("redaction should succeed");

        assert!(
            storage
                .get_is_sticky_event(room_id, user_id, "m.room.message")
                .await
                .expect("single read should succeed")
                .is_none(),
            "MSC4354 #20323: the redacted event must not be reported as sticky"
        );

        let remaining = storage.get_all_is_sticky_events(room_id, user_id).await.expect("bulk read should succeed");
        assert!(
            remaining.iter().all(|e| e.event_id != *pinned_event),
            "the redacted event must not survive in the bulk read either: {remaining:?}"
        );
        // Control: redacting one event must not un-stick the others — without
        // this, a mutation that deletes *every* sticky row would look correct.
        assert_eq!(remaining.len(), 1, "the untouched pin must survive: {remaining:?}");
        assert_eq!(remaining[0].event_id, *untouched_event);

        cleanup_sticky(&pool, room_id, user_id).await;
    }

    /// Force a pin's tracked expiry without waiting for wall-clock time to pass.
    ///
    /// Deliberately writes `expires_at` (the tracked column) rather than
    /// `is_sticky`: the point of these tests is that the *expiry* is what
    /// retires a pin, so they must not be satisfiable by flipping the flag.
    async fn force_expiry(pool: &PgPool, room_id: &str, user_id: &str, event_type: &str, expires_at: i64) {
        sqlx::query(
            "UPDATE room_sticky_events SET expires_at = $4 WHERE room_id = $1 AND user_id = $2 AND event_type = $3",
        )
        .bind(room_id)
        .bind(user_id)
        .bind(event_type)
        .bind(expires_at)
        .execute(pool)
        .await
        .expect("forcing the expiry should succeed");
    }

    /// MSC4354 / upstream **20324**: a pin that has reached its tracked expiry
    /// must stop being reported — on *every* read path, including the sync one.
    #[tokio::test]
    async fn test_expired_pin_is_not_reported_on_any_read_path() {
        let (_isolated, pool) = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_exp_{suffix}:test.com");
        let user_id = &format!("@user_exp_{suffix}:test.com");
        let event_id = &format!("$ev_exp_{suffix}:test.com");
        let event_type = "m.room.message";

        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;
        cleanup_sticky(&pool, room_id, user_id).await;

        storage
            .set_is_sticky_event(room_id, user_id, event_id, event_type, true, Some(60_000))
            .await
            .expect("setting a pin with a duration should succeed");

        // Control: while the expiry is in the future the pin *is* reported.
        assert!(
            storage.get_is_sticky_event(room_id, user_id, event_type).await.expect("read should succeed").is_some(),
            "precondition: an unexpired pin must be reported, otherwise the assertion below is vacuous"
        );
        assert_eq!(
            storage.get_rooms_with_is_sticky_events(user_id).await.expect("rooms read should succeed"),
            vec![room_id.to_string()],
            "precondition: the room must be listed while the pin is live"
        );

        // Move the tracked expiry into the past.
        force_expiry(&pool, room_id, user_id, event_type, current_timestamp_millis() - 1).await;

        assert!(
            storage
                .get_is_sticky_event(room_id, user_id, event_type)
                .await
                .expect("single read should succeed")
                .is_none(),
            "20324: an expired pin must not be reported"
        );
        assert!(
            storage.get_all_is_sticky_events(room_id, user_id).await.expect("bulk read should succeed").is_empty(),
            "20324: the bulk read (used by /sync) must drop expired pins too"
        );
        assert!(
            storage.get_rooms_with_is_sticky_events(user_id).await.expect("rooms read should succeed").is_empty(),
            "20324: a room whose only pin expired must not be advertised as sticky"
        );

        // The row itself is *not* deleted — expiry is a read-side predicate, so a
        // later re-pin of the same (room, user, type) still updates in place.
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM room_sticky_events WHERE room_id = $1")
            .bind(room_id)
            .fetch_one(&*pool)
            .await
            .expect("counting rows should succeed");
        assert_eq!(rows, 1, "expiry must not delete the row");

        cleanup_sticky(&pool, room_id, user_id).await;
    }

    /// The 20324 invariant proper: the expiry **reported** to clients is exactly
    /// the expiry the server **tracks**. Falsified by any implementation that
    /// recomputes one of the two independently (e.g. reporting
    /// `now + duration` at read time while tracking `created_ts + duration`).
    #[tokio::test]
    async fn test_reported_expiry_is_the_tracked_expiry() {
        let (_isolated, pool) = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_20324_{suffix}:test.com");
        let user_id = &format!("@user_20324_{suffix}:test.com");
        let event_id = &format!("$ev_20324_{suffix}:test.com");
        let event_type = "m.room.message";
        let duration_ms = 120_000_i64;

        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;
        cleanup_sticky(&pool, room_id, user_id).await;

        storage
            .set_is_sticky_event(room_id, user_id, event_id, event_type, true, Some(duration_ms))
            .await
            .expect("setting a pin with a duration should succeed");

        let reported = storage
            .get_is_sticky_event(room_id, user_id, event_type)
            .await
            .expect("read should succeed")
            .expect("pin should be present");

        let reported_expiry = reported.expires_at.expect("a pin set with a duration must carry an expiry");
        assert_eq!(reported.sticky_duration_ms, Some(duration_ms), "the requested duration must round-trip");
        assert_eq!(
            reported_expiry,
            reported.created_ts + duration_ms,
            "the reported expiry must be created_ts + requested duration"
        );

        // Now read what the *tracker* would compare against, straight from the
        // column: if these ever differ, reported != tracked and clients are lied to.
        let tracked: Option<i64> =
            sqlx::query_scalar("SELECT expires_at FROM room_sticky_events WHERE room_id = $1 AND user_id = $2")
                .bind(room_id)
                .bind(user_id)
                .fetch_one(&*pool)
                .await
                .expect("reading the tracked expiry should succeed");
        assert_eq!(
            Some(reported_expiry),
            tracked,
            "20324: the expiry sent to clients must be the same value the server expires on"
        );

        // Boundary: the predicate is strict (`expires_at > now`), so a pin whose
        // tracked expiry is exactly "now" is already retired — a client is never
        // shown a pin past the deadline it was told.
        force_expiry(&pool, room_id, user_id, event_type, current_timestamp_millis()).await;
        assert!(
            storage.get_is_sticky_event(room_id, user_id, event_type).await.expect("read should succeed").is_none(),
            "the pin must retire at its reported expiry, not one tick later"
        );

        cleanup_sticky(&pool, room_id, user_id).await;
    }

    /// A duration outside MSC4354's range is a client error and must not be
    /// silently clamped into a pin that lives longer (or shorter) than asked.
    #[tokio::test]
    async fn test_set_rejects_out_of_range_duration() {
        let (_isolated, pool) = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_bad_{suffix}:test.com");
        let user_id = &format!("@user_bad_{suffix}:test.com");
        let event_id = &format!("$ev_bad_{suffix}:test.com");

        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;
        cleanup_sticky(&pool, room_id, user_id).await;

        let err = storage
            .set_is_sticky_event(room_id, user_id, event_id, "m.room.message", true, Some(0))
            .await
            .expect_err("zero must be rejected");
        assert!(matches!(err, SetStickyEventError::Duration(StickyDurationError::NotPositive(0))), "got {err:?}");

        let err = storage
            .set_is_sticky_event(room_id, user_id, event_id, "m.room.message", true, Some(MAX_STICKY_DURATION_MS + 1))
            .await
            .expect_err("over the cap must be rejected");
        assert!(matches!(err, SetStickyEventError::Duration(StickyDurationError::TooLong { .. })), "got {err:?}");

        assert!(
            storage.get_all_is_sticky_events(room_id, user_id).await.expect("read should succeed").is_empty(),
            "a rejected set must not leave any pin behind"
        );

        cleanup_sticky(&pool, room_id, user_id).await;
    }
}
