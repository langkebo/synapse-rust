// Sticky Event Storage - MSC4354
// Stores is_sticky event metadata for rooms
// Following project field naming standards

use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

/// 联邦补发游标（UG-7 catch-up：#20165 增量补发 / #20233 新成员补发）。
mod destinations;
pub use destinations::{PendingStickyDelivery, StickyDeliveryRow};

/// MSC4354's upper bound on a sticky duration: **1 hour**.
///
/// Mirrors upstream `synapse/api/constants.py::StickyEvent.MAX_DURATION`
/// (`Duration(hours=1)`), which is the value the reference implementation
/// enforces for `org.matrix.msc4354.sticky_duration_ms`.
pub const MAX_STICKY_DURATION_MS: i64 = 3_600_000;

/// The `unsigned` key a resolved sticky TTL is recorded under.
///
/// `unsigned` sits outside the signed bytes and the content hash, so writing it
/// after the event is persisted cannot invalidate the event's signature — which is
/// why both the immediate surfaces (`/send`, `/state`) and the delayed-event
/// dispatcher write here rather than into `content`.
pub const STICKY_TTL_UNSIGNED_FIELD: &str = "msc4354_sticky_duration_ttl_ms";

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
    /// # 单写者假设（C3，§19.5 批次 C）
    ///
    /// 唯一键 `(room_id, user_id, event_type)` 保证同一 owner key 与 `event_type` 的组合
    /// 恒为一行，因此**单写者**模型（本进程独占写 `room_sticky_events`）下同一用户不会
    /// 在 `/sync` 里看到重复条目。**已知边界**：同一 `event_type` 可以同时存在一条用户行
    /// 和多条房级行（不同 origin 各一行，`user_id` 是裸 server name）——去重由读路径的
    /// `DISTINCT ON (event_type)` 负责，见 [`Self::get_all_is_sticky_events`]。
    /// **仍未做**：若将来引入多写者 / 按房间分片（多进程各自写 sticky），唯一键只能约束
    /// 库内单行，**不保证**同一 sticky 事件不会经不同分片重复出现在多个 sync 响应中；
    /// 届时需要按分片位置去重或改用 per-room 广播抄本。当前部署形态下不需要。
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

        // 每次写入（含 `ON CONFLICT` 更新）都换一个新位点，联邦补发才能看出这行变过。
        sqlx::query!(
            r"
            INSERT INTO room_sticky_events
                (room_id, user_id, event_id, event_type, is_sticky, sticky_duration_ms, expires_at,
                 created_ts, updated_ts, revision)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9,
                    nextval('room_sticky_events_revision_seq'))
            ON CONFLICT (room_id, user_id, event_type)
            DO UPDATE SET event_id = EXCLUDED.event_id,
                          is_sticky = EXCLUDED.is_sticky,
                          sticky_duration_ms = EXCLUDED.sticky_duration_ms,
                          expires_at = EXCLUDED.expires_at,
                          updated_ts = EXCLUDED.updated_ts,
                          revision = nextval('room_sticky_events_revision_seq')
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
    ///
    /// # 可见性：「本人行 ∪ 房级行」
    ///
    /// 表里有两类 owner key：
    /// - `user_id LIKE '@%'` ⇒ **用户行**（本地用户自己设的置顶）；
    /// - 其他（裸 server name）⇒ **房级行**：远端置顶经
    ///   `handle_sticky_event_edu` 以 origin 服务器名落库，全房可见。
    ///
    /// 本读取对二者取并集，且**本人行优先** —— 同一个 `event_type` 上，用户自己的
    /// 置顶覆盖房级置顶。房级行之间（多个 origin 各写一行）按 `updated_ts` 取最新，
    /// 保证结果确定。旧实现只按 `user_id = 本人` 过滤，远端置顶即便落库也读不到。
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
            WHERE room_id = $1 AND event_type = $3 AND is_sticky = true
              AND (expires_at IS NULL OR expires_at > $4)
              AND (user_id = $2 OR user_id NOT LIKE '@%')
            ORDER BY (user_id = $2) DESC, updated_ts DESC, id DESC
            LIMIT 1
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
    ///
    /// 可见性同样是「本人行 ∪ 房级行」，每个 `event_type` **只出一条**：`DISTINCT ON`
    /// 配合 `ORDER BY event_type, (user_id = 本人) DESC, updated_ts DESC, id DESC`
    /// 让本人行优先、房级行取最新。若改成按 `user_id` 过滤（旧实现），远端置顶永远
    /// 不可见；若不加 `DISTINCT ON`，同一 `event_type` 会被用户行和房级行各报一次。
    pub async fn get_all_is_sticky_events(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Vec<StickyEvent>, sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query_as!(
            StickyEvent,
            r"
            SELECT DISTINCT ON (event_type)
                   room_id, user_id, event_id, event_type, is_sticky,
                   sticky_duration_ms, expires_at, created_ts, updated_ts
            FROM room_sticky_events
            WHERE room_id = $1 AND is_sticky = true
              AND (expires_at IS NULL OR expires_at > $3)
              AND (user_id = $2 OR user_id NOT LIKE '@%')
            ORDER BY event_type, (user_id = $2) DESC, updated_ts DESC, id DESC
            ",
            room_id,
            user_id,
            now,
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// Clear is_sticky event for a room, user, and event type
    ///
    /// 行**不删除**，只置 `is_sticky = false`：这就是联邦补发要用的墓碑 ——
    /// `room_sticky_events.revision` 随之递增，远端断连恢复后能拿到"已清除"这一变更，
    /// 否则那条置顶会永远留在对方界面上。
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
            SET is_sticky = false,
                updated_ts = $4,
                revision = nextval('room_sticky_events_revision_seq')
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

    /// Get the rooms among `room_ids` that carry sticky pins visible to `user_id`
    /// (for `/sync`).
    ///
    /// "Visible" means the same union the read paths use: the user's own rows plus
    /// the **room-level** rows federation writes (`user_id NOT LIKE '@%'`).
    ///
    /// `room_ids` is mandatory rather than optional on purpose: with room-level
    /// rows in scope, an unbounded `WHERE user_id = $me` scan would have to become
    /// "every room any peer ever pinned", turning a cheap prefetch into a full-table
    /// scan on a wide initial sync. Callers pass the rooms already in the response.
    ///
    /// Rooms whose only pins have already expired are omitted, so a client is
    /// never told to sync a room on account of a stale pin.
    pub async fn get_rooms_with_is_sticky_events(
        &self,
        room_ids: &[String],
        user_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let now = current_timestamp_millis();
        let rows: Vec<String> = sqlx::query_scalar!(
            r"
            SELECT DISTINCT room_id FROM room_sticky_events
            WHERE room_id = ANY($1::text[]) AND is_sticky = true
              AND (expires_at IS NULL OR expires_at > $2)
              AND (user_id = $3 OR user_id NOT LIKE '@%')
            ",
            room_ids,
            now,
            user_id,
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

// `sticky_event.rs` 是非 mod-rs 文件，子模块按 Rust 2018 目录模块解析到
// `sticky_event/<name>.rs` ⇒ 不要再写 `#[path]`：多出来的那行属性会把
// `#[cfg(test)] mod x;` 的相邻关系打断，使 `sqlx_query_census.py` 认不出
// 这些整份测试文件、把其中的运行时 SQL 错算进生产区（UG-7 catch-up 批实测 11 处）。
#[cfg(test)]
mod db_tests;

#[cfg(test)]
mod db_tests_room_level;

#[cfg(test)]
mod db_tests_destinations;
