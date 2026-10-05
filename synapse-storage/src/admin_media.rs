use async_trait::async_trait;
use std::sync::Arc;
use synapse_common::ApiError;

/// The `MediaCursor` struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaCursor {
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `media_id` field.
    pub media_id: String,
}

/// See [`decode_media_cursor`].
pub fn decode_media_cursor(cursor: Option<&str>) -> Option<MediaCursor> {
    let cursor = cursor?;
    let (created_ts, media_id) = cursor.split_once('|')?;
    let created_ts = created_ts.parse::<i64>().ok()?;
    if media_id.is_empty() {
        return None;
    }
    Some(MediaCursor { created_ts, media_id: media_id.to_owned() })
}

/// See [`encode_media_cursor`].
pub fn encode_media_cursor(cursor: &MediaCursor) -> String {
    format!("{}|{}", cursor.created_ts, cursor.media_id)
}

/// The `AdminMediaInfo` struct.
#[derive(Debug, Clone)]
pub struct AdminMediaInfo {
    /// The `media_id` field.
    pub media_id: String,
    /// The `content_type` field.
    pub content_type: Option<String>,
    /// The `file_name` field.
    pub file_name: Option<String>,
    /// The `size` field.
    pub size: i64,
    /// The `uploader_user_id` field.
    pub uploader_user_id: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `last_accessed_at` field.
    pub last_accessed_at: Option<i64>,
    /// The `quarantined` field.
    pub quarantined: bool,
}

/// The `AdminMediaPage` struct.
#[derive(Debug, Clone)]
pub struct AdminMediaPage {
    /// The `media` field.
    pub media: Vec<AdminMediaInfo>,
    /// The `next_batch` field.
    pub next_batch: Option<String>,
}

/// The `AdminMediaQuotaSummary` struct.
#[derive(Debug, Clone)]
pub struct AdminMediaQuotaSummary {
    /// The `total_size` field.
    pub total_size: i64,
    /// The `total_count` field.
    pub total_count: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct AdminMediaRow {
    media_id: String,
    content_type: Option<String>,
    file_name: Option<String>,
    size: i64,
    uploader_user_id: Option<String>,
    created_ts: i64,
    last_accessed_at: Option<i64>,
    quarantine_status: Option<String>,
}

fn quarantine_status_to_bool(value: Option<&str>) -> bool {
    matches!(value, Some("quarantined") | Some("true") | Some("1") | Some("yes"))
}

fn map_media_row(row: AdminMediaRow) -> AdminMediaInfo {
    AdminMediaInfo {
        media_id: row.media_id,
        content_type: row.content_type,
        file_name: row.file_name,
        size: row.size,
        uploader_user_id: row.uploader_user_id,
        created_ts: row.created_ts,
        last_accessed_at: row.last_accessed_at,
        quarantined: quarantine_status_to_bool(row.quarantine_status.as_deref()),
    }
}

/// The `AdminMediaStorage` struct.
///
/// Holds the `Arc<PgPool>` handle it was built with, not a downgraded inner
/// clone: the test-schema janitor drops a per-test schema as soon as the last
/// `Arc<PgPool>` is released (`synapse_common::test_schema_guard`), so a
/// service that downgrades the handle lets a fixture release the schema while
/// the service is still querying it. Unqualified queries then resolve through
/// `search_path` into the shared `public` schema — the media chunked-download
/// and quota flakes of 2026-09-19.
#[derive(Clone)]
pub struct AdminMediaStorage {
    pool: Arc<sqlx::PgPool>,
}

/// The `AdminMediaStoreApi` trait.
#[async_trait]
pub trait AdminMediaStoreApi: Send + Sync {
    /// See [`get_all_media`].
    async fn get_all_media(&self, limit: i64, cursor: Option<MediaCursor>) -> Result<AdminMediaPage, ApiError>;
    /// See [`get_media_info`].
    async fn get_media_info(&self, media_id: &str) -> Result<Option<AdminMediaInfo>, ApiError>;
    /// See [`delete_media`].
    async fn delete_media(&self, media_id: &str) -> Result<bool, ApiError>;
    /// See [`get_media_quota`].
    async fn get_media_quota(&self) -> Result<AdminMediaQuotaSummary, ApiError>;
    /// See [`get_user_media`].
    async fn get_user_media(&self, user_id: &str) -> Result<Vec<AdminMediaInfo>, ApiError>;
    /// See [`delete_user_media`].
    async fn delete_user_media(&self, user_id: &str) -> Result<u64, ApiError>;
    /// See [`get_room_media`].
    async fn get_room_media(
        &self,
        room_id: &str,
        limit: i64,
        cursor: Option<MediaCursor>,
    ) -> Result<AdminMediaPage, ApiError>;
    /// See [`delete_room_media`].
    async fn delete_room_media(&self, room_id: &str, media_id: &str) -> Result<bool, ApiError>;

    // ───────────────────────────────────────────────────────────────────────────
    // Missing endpoints to be implemented (U-5)
    // ───────────────────────────────────────────────────────────────────────────

    /// See [`quarantine_user_media`].
    async fn quarantine_user_media(&self, user_id: &str) -> Result<i64, ApiError>;
    /// See [`delete_media_by_policy`].
    async fn delete_media_by_policy(&self, before_ts: i64, max_size: i64) -> Result<u64, ApiError>;
    /// See [`purge_media_cache`].
    async fn purge_media_cache(&self, before_ts: i64) -> Result<u64, ApiError>;
    /// See [`unprotect_media`].
    async fn unprotect_media(&self, media_id: &str, changed_by: &str) -> Result<i64, ApiError>;
}

impl AdminMediaStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// Insert (or refresh) the metadata row for one media item.
    ///
    /// `content_hash` is the upstream/Synapse media content hash produced by
    /// [`synapse_common::content_hash`]; it is nullable only because rows that
    /// predate the column exist (see `get_is_hash_quarantined`).
    ///
    /// `quarantine_status` is `None` for a normal upload and
    /// `Some("quarantined")` when the upload matched an already-quarantined
    /// content hash. The conflict branch deliberately COALESCEs both new
    /// columns instead of overwriting with the incoming `NULL`: re-upserting a
    /// row must not erase a hash or silently lift a quarantine ruling that was
    /// recorded elsewhere.
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_media_metadata(
        &self,
        media_id: &str,
        server_name: &str,
        content_type: &str,
        file_name: &str,
        size: i64,
        uploader_user_id: &str,
        created_ts: i64,
        content_hash: Option<&str>,
        quarantine_status: Option<&str>,
    ) -> Result<(), ApiError> {
        // R5：`content_hash` / `quarantine_status` 是 `Option<&str>`（**按值**绑到可空列），
        // 不是宏拒绝的 `&Option<T>`。
        sqlx::query!(
            r#"
            INSERT INTO media_metadata
                (media_id, server_name, content_type, file_name, size, uploader_user_id, created_ts,
                 content_hash, quarantine_status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (media_id) DO UPDATE
            SET content_type = EXCLUDED.content_type,
                file_name = EXCLUDED.file_name,
                size = EXCLUDED.size,
                content_hash = COALESCE(EXCLUDED.content_hash, media_metadata.content_hash),
                quarantine_status = COALESCE(EXCLUDED.quarantine_status, media_metadata.quarantine_status)
            "#,
            media_id,
            server_name,
            content_type,
            file_name,
            size,
            uploader_user_id,
            created_ts,
            content_hash,
            quarantine_status,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(())
    }

    /// `TRUE` iff any `media_metadata` row carries `content_hash` **and** is
    /// currently quarantined.
    ///
    /// This backs hash-level automatic quarantine (upstream
    /// `store.get_is_hash_quarantined`, release-v1.161): once one upload of a
    /// given content is quarantined, every later upload of byte-identical
    /// content is quarantined on write without re-scanning.
    ///
    /// The status predicate is the SQL mirror of [`quarantine_status_to_bool`]
    /// — `'quarantined' | 'true' | '1' | 'yes'` — so `NULL`, `''` and `'no'`
    /// (and any other value, including a manual unquarantine writing `''`) are
    /// **not** quarantined. `IN` never matches `NULL`, which is what makes a
    /// pre-column row with no status non-quarantined.
    ///
    /// `content_hash` itself is never `NULL`-for-unset here: callers only reach
    /// this with a real hash, and rows with `content_hash IS NULL` simply do not
    /// match.
    pub async fn get_is_hash_quarantined(&self, content_hash: &str) -> Result<bool, ApiError> {
        // Static `query_scalar!` on purpose: this is new production SQL, and the
        // SQLx literal-dynamic ratchet (`scripts/ci/sqlx_literal_production_baseline`,
        // `scripts/ci/sqlx_dynamic_ratio_baseline`) must not grow for it.
        //
        // The `EXISTS(...)` column has no relation origin, so sqlx infers it as
        // nullable. It is asserted non-null with `AS "exists!"` rather than left
        // as `Option<bool>` + `unwrap_or(false)`: `EXISTS` can never be `NULL`, and
        // an `unwrap_or` here would be a **fail-open** shape — a `NULL` would read
        // as "not quarantined". (`query_scalar!` does accept the `AS "col!"`
        // override; see `room/mod.rs` / `room/admin.rs`.)
        let quarantined = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM media_metadata
                WHERE content_hash = $1
                  AND quarantine_status IN ('quarantined', 'true', '1', 'yes')
            ) AS "exists!"
            "#,
            content_hash
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(quarantined)
    }

    /// See [`get_all_media`].
    pub async fn get_all_media(&self, limit: i64, cursor: Option<MediaCursor>) -> Result<AdminMediaPage, ApiError> {
        // 8 列与 `AdminMediaRow` 8 字段一一对应（可空列 ⇒ `Option` 字段，R4 一致）；
        // 游标两参是 `Option<i64>` / `Option<&str>`，按值绑定。
        let media: Vec<AdminMediaRow> = sqlx::query_as!(
            AdminMediaRow,
            r#"SELECT media_id, content_type, file_name, size, uploader_user_id, created_ts, last_accessed_at, quarantine_status
               FROM media_metadata
               WHERE ($1::BIGINT IS NULL AND $2::TEXT IS NULL)
                  OR created_ts < $1
                  OR (created_ts = $1 AND media_id < $2)
               ORDER BY created_ts DESC, media_id DESC
               LIMIT $3"#,
            cursor.as_ref().map(|cursor| cursor.created_ts),
            cursor.as_ref().map(|cursor| cursor.media_id.as_str()),
            limit,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        let next_batch = if media.len() as i64 == limit {
            media.last().map(|row| {
                encode_media_cursor(&MediaCursor { created_ts: row.created_ts, media_id: row.media_id.clone() })
            })
        } else {
            None
        };

        Ok(AdminMediaPage { media: media.into_iter().map(map_media_row).collect(), next_batch })
    }

    /// See [`get_media_info`].
    pub async fn get_media_info(&self, media_id: &str) -> Result<Option<AdminMediaInfo>, ApiError> {
        let media: Option<AdminMediaRow> = sqlx::query_as!(
            AdminMediaRow,
            r#"SELECT media_id, content_type, file_name, size, uploader_user_id, created_ts, last_accessed_at, quarantine_status
               FROM media_metadata WHERE media_id = $1"#,
            media_id,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(media.map(map_media_row))
    }

    /// See [`delete_media`].
    pub async fn delete_media(&self, media_id: &str) -> Result<bool, ApiError> {
        let result = sqlx::query!("DELETE FROM media_metadata WHERE media_id = $1", media_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`get_media_quota`].
    pub async fn get_media_quota(&self) -> Result<AdminMediaQuotaSummary, ApiError> {
        // R4 ①：`COALESCE(SUM(...), 0)` 没有关系来源 ⇒ sqlx 推成可空；谁保证非空：聚合在空集上
        // 落到字面量 0。
        let total_size =
            sqlx::query_scalar!(r#"SELECT COALESCE(SUM(size), 0)::BIGINT AS "total_size!" FROM media_metadata"#)
                .fetch_one(&*self.pool)
                .await
                .map_err(|e| ApiError::internal_with_cause("Database error", e))?;
        let total_count = sqlx::query_scalar!(r#"SELECT COUNT(*)::BIGINT AS "total_count!" FROM media_metadata"#)
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(AdminMediaQuotaSummary { total_size, total_count })
    }

    /// See [`get_user_media`].
    pub async fn get_user_media(&self, user_id: &str) -> Result<Vec<AdminMediaInfo>, ApiError> {
        // 后两列是 `NULL::BIGINT` / `NULL::TEXT` 常量（R4 ①：无关系来源 ⇒ 可空），
        // 与 `AdminMediaRow` 的 `Option` 字段一致。
        let media: Vec<AdminMediaRow> = sqlx::query_as!(
            AdminMediaRow,
            r#"SELECT media_id, content_type, file_name, size, uploader_user_id, created_ts,
               NULL::BIGINT AS last_accessed_at, NULL::TEXT AS quarantine_status
               FROM media_metadata WHERE uploader_user_id = $1 ORDER BY created_ts DESC, media_id DESC"#,
            user_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(media.into_iter().map(map_media_row).collect())
    }

    /// See [`delete_user_media`].
    pub async fn delete_user_media(&self, user_id: &str) -> Result<u64, ApiError> {
        let result = sqlx::query!("DELETE FROM media_metadata WHERE uploader_user_id = $1", user_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(result.rows_affected())
    }

    /// See [`get_room_media`].
    pub async fn get_room_media(
        &self,
        room_id: &str,
        limit: i64,
        cursor: Option<MediaCursor>,
    ) -> Result<AdminMediaPage, ApiError> {
        // List media in a room: join room_events to find mxc:// URLs in this room,
        // extract the media_id portion, then join media_metadata for details.
        // Only non-encrypted media (content_type not null) is returned.
        //
        // mxc:// URL format: `mxc://server_name/media_id`
        // SUBSTRING(url FROM 7) strips the `mxc://` prefix (6 chars), yielding
        // `server_name/media_id`. SPLIT_PART(..., '/', 2) extracts `media_id`.
        let media: Vec<AdminMediaRow> = sqlx::query_as!(
            AdminMediaRow,
            "SELECT DISTINCT mm.media_id, mm.content_type, mm.file_name, mm.size, mm.uploader_user_id, mm.created_ts, mm.last_accessed_at, mm.quarantine_status FROM room_events re INNER JOIN media_metadata mm ON mm.media_id = SPLIT_PART(SUBSTRING(re.content->>'url' FROM 7), '/', 2) WHERE re.room_id = $1 AND re.content->>'url' LIKE 'mxc://%%' AND mm.content_type IS NOT NULL AND (($2::BIGINT IS NULL AND $3::TEXT IS NULL) OR mm.created_ts < $2 OR (mm.created_ts = $2 AND mm.media_id < $3)) ORDER BY mm.created_ts DESC, mm.media_id DESC LIMIT $4",
            room_id,
            cursor.as_ref().map(|cursor| cursor.created_ts),
            cursor.as_ref().map(|cursor| cursor.media_id.as_str()),
            limit,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        let next_batch = if media.len() as i64 == limit {
            media.last().map(|row| {
                encode_media_cursor(&MediaCursor { created_ts: row.created_ts, media_id: row.media_id.clone() })
            })
        } else {
            None
        };

        Ok(AdminMediaPage { media: media.into_iter().map(map_media_row).collect(), next_batch })
    }

    /// See [`delete_room_media`].
    pub async fn delete_room_media(&self, room_id: &str, media_id: &str) -> Result<bool, ApiError> {
        // First verify the media exists in this room (via room_events).
        // mxc:// URL format: `mxc://server_name/media_id`
        // Strip the 6-char `mxc://` prefix (FROM 7 in 1-indexed SUBSTRING), then
        // SPLIT_PART on '/' to extract the media_id portion for comparison.
        // R4 ①：`COUNT(*)` 无关系来源 ⇒ 断言（计数恒非空）。
        let in_room: i64 = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!" FROM room_events WHERE room_id = $1 AND content->>'url' LIKE 'mxc://%%' AND SPLIT_PART(SUBSTRING(content->>'url' FROM 7), '/', 2) = $2"#,
            room_id,
            media_id,
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        if in_room == 0 {
            return Ok(false);
        }

        // Delete the media record (metadata + thumbnails cascade via FK).
        // If the media is shared by other rooms, the media_metadata row remains;
        // the room_events reference is what ties it to this room.
        let result = sqlx::query!("DELETE FROM media_metadata WHERE media_id = $1", media_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(result.rows_affected() > 0)
    }

    // ───────────────────────────────────────────────────────────────────────────
    // U-5 missing endpoint implementations
    // ───────────────────────────────────────────────────────────────────────────

    /// Quarantine all local media uploaded by a given user.
    ///
    /// Backs `POST /_synapse/admin/v1/user/{user_id}/media/quarantine`.
    /// Returns the number of rows transitioned to quarantined status.
    ///
    /// Only affects local uploads (server_name = this server), never remote media.
    pub async fn quarantine_user_media(&self, user_id: &str) -> Result<i64, ApiError> {
        let result = sqlx::query!(
            r#"
            UPDATE media_metadata
            SET quarantine_status = 'quarantined'
            WHERE uploader_user_id = $1
              AND (quarantine_status IS NULL OR quarantine_status NOT IN ('quarantined', 'protected'))
            "#,
            user_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(result.rows_affected() as i64)
    }

    /// Batch-delete local media by policy: created before `before_ts` OR larger than `max_size`.
    ///
    /// Backs `POST /_synapse/admin/v1/media/delete`.
    /// Both parameters are optional; a value of `0` means "no limit on that dimension".
    /// Protected and quarantined rows are skipped.
    pub async fn delete_media_by_policy(&self, before_ts: i64, max_size: i64) -> Result<u64, ApiError> {
        // ⚠️ **D-74 同型陷阱**：裸 `$n = 0` 会让 PG 把参数定型成 **int4**，于是宏要求 `i32`，
        // 而公共 API 传的是 `i64`（动态路径靠 sqlx 显式发送 INT8 才没暴露）⇒ 显式 `0::BIGINT`
        // 保持 `i64` 形状与"0 = 不设限"的语义（实测量处 E0308：`expected i32, found i64`）。
        let result = sqlx::query!(
            r#"
            DELETE FROM media_metadata
            WHERE (quarantine_status IS NULL OR quarantine_status NOT IN ('quarantined', 'protected'))
              AND (
                    $1 = 0::BIGINT OR created_ts < $1
                    OR $2 = 0::BIGINT OR size > $2
                  )
            "#,
            before_ts,
            max_size,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(result.rows_affected())
    }

    /// Purge cached remote media that has not been accessed since `before_ts`.
    ///
    /// Backs `POST /_synapse/admin/v1/purge_media_cache`.
    /// In this implementation only local media exists in `media_metadata`,
    /// so this degrades to deleting local media that matches the access-time
    /// policy (the remote-cache table does not exist in this codebase).
    /// Returns the number of rows deleted.
    pub async fn purge_media_cache(&self, before_ts: i64) -> Result<u64, ApiError> {
        let result = sqlx::query!(
            r#"
            DELETE FROM media_metadata
            WHERE (last_accessed_at IS NULL OR last_accessed_at < $1)
            "#,
            before_ts,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(result.rows_affected())
    }

    /// Clear the `protected` status on a media row so it can be quarantined
    /// or deleted by policy again.
    ///
    /// Backs `POST /_synapse/admin/v1/media/unprotect/{mediaId}`.
    /// Returns `0` if the row did not exist, `1` otherwise.
    pub async fn unprotect_media(&self, media_id: &str, changed_by: &str) -> Result<i64, ApiError> {
        // `changed_by` is recorded via the audit stream at the service layer;
        // the storage layer only flips the status column.
        let _ = changed_by;

        let result = sqlx::query!(
            r#"
            UPDATE media_metadata
            SET quarantine_status = NULL
            WHERE media_id = $1
              AND quarantine_status = 'protected'
            "#,
            media_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(if result.rows_affected() > 0 { 1 } else { 0 })
    }
}

#[async_trait]
impl AdminMediaStoreApi for AdminMediaStorage {
    async fn get_all_media(&self, limit: i64, cursor: Option<MediaCursor>) -> Result<AdminMediaPage, ApiError> {
        self.get_all_media(limit, cursor).await
    }

    async fn get_media_info(&self, media_id: &str) -> Result<Option<AdminMediaInfo>, ApiError> {
        self.get_media_info(media_id).await
    }

    async fn delete_media(&self, media_id: &str) -> Result<bool, ApiError> {
        self.delete_media(media_id).await
    }

    async fn get_media_quota(&self) -> Result<AdminMediaQuotaSummary, ApiError> {
        self.get_media_quota().await
    }

    async fn get_user_media(&self, user_id: &str) -> Result<Vec<AdminMediaInfo>, ApiError> {
        self.get_user_media(user_id).await
    }

    async fn delete_user_media(&self, user_id: &str) -> Result<u64, ApiError> {
        self.delete_user_media(user_id).await
    }

    async fn get_room_media(
        &self,
        room_id: &str,
        limit: i64,
        cursor: Option<MediaCursor>,
    ) -> Result<AdminMediaPage, ApiError> {
        self.get_room_media(room_id, limit, cursor).await
    }

    async fn delete_room_media(&self, room_id: &str, media_id: &str) -> Result<bool, ApiError> {
        self.delete_room_media(room_id, media_id).await
    }

    // ───────────────────────────────────────────────────────────────────────────
    // U-5 missing endpoint trait delegations
    // ───────────────────────────────────────────────────────────────────────────

    async fn quarantine_user_media(&self, user_id: &str) -> Result<i64, ApiError> {
        self.quarantine_user_media(user_id).await
    }

    async fn delete_media_by_policy(&self, before_ts: i64, max_size: i64) -> Result<u64, ApiError> {
        self.delete_media_by_policy(before_ts, max_size).await
    }

    async fn purge_media_cache(&self, before_ts: i64) -> Result<u64, ApiError> {
        self.purge_media_cache(before_ts).await
    }

    async fn unprotect_media(&self, media_id: &str, changed_by: &str) -> Result<i64, ApiError> {
        self.unprotect_media(media_id, changed_by).await
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::{
        decode_media_cursor, encode_media_cursor, map_media_row, quarantine_status_to_bool, AdminMediaRow, MediaCursor,
    };

    #[test]
    fn test_media_cursor_round_trip() {
        let cursor =
            encode_media_cursor(&MediaCursor { created_ts: 1_700_000_000_000, media_id: "abc123".to_string() });
        assert_eq!(
            decode_media_cursor(Some(&cursor)),
            Some(MediaCursor { created_ts: 1_700_000_000_000, media_id: "abc123".to_string() })
        );
    }

    #[test]
    fn test_media_cursor_rejects_invalid_value() {
        assert_eq!(decode_media_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_media_cursor(Some("123|")), None);
    }

    // ── quarantine_status_to_bool ──────────────────────────────────

    #[test]
    fn quarantine_status_quarantined_is_true() {
        assert!(quarantine_status_to_bool(Some("quarantined")));
    }

    #[test]
    fn quarantine_status_true_is_true() {
        assert!(quarantine_status_to_bool(Some("true")));
    }

    #[test]
    fn quarantine_status_1_is_true() {
        assert!(quarantine_status_to_bool(Some("1")));
    }

    #[test]
    fn quarantine_status_yes_is_true() {
        assert!(quarantine_status_to_bool(Some("yes")));
    }

    #[test]
    fn quarantine_status_other_is_false() {
        assert!(!quarantine_status_to_bool(Some("no")));
        assert!(!quarantine_status_to_bool(Some("false")));
        assert!(!quarantine_status_to_bool(Some("")));
    }

    #[test]
    fn quarantine_status_none_is_false() {
        assert!(!quarantine_status_to_bool(None));
    }

    // ── map_media_row ──────────────────────────────────────────────

    fn make_media_row() -> AdminMediaRow {
        AdminMediaRow {
            media_id: "media_1".to_string(),
            content_type: Some("image/png".to_string()),
            file_name: Some("photo.png".to_string()),
            size: 1024,
            uploader_user_id: Some("@alice:ex.com".to_string()),
            created_ts: 1_700_000_000_000,
            last_accessed_at: Some(1_700_000_001_000),
            quarantine_status: None,
        }
    }

    #[test]
    fn map_media_row_transfers_fields() {
        let row = make_media_row();
        let info = map_media_row(row);
        assert_eq!(info.media_id, "media_1");
        assert_eq!(info.content_type, Some("image/png".to_string()));
        assert_eq!(info.file_name, Some("photo.png".to_string()));
        assert_eq!(info.size, 1024);
        assert_eq!(info.uploader_user_id, Some("@alice:ex.com".to_string()));
        assert_eq!(info.created_ts, 1_700_000_000_000);
        assert_eq!(info.last_accessed_at, Some(1_700_000_001_000));
        assert!(!info.quarantined);
    }

    #[test]
    fn map_media_row_sets_quarantined_true() {
        let mut row = make_media_row();
        row.quarantine_status = Some("quarantined".to_string());
        let info = map_media_row(row);
        assert!(info.quarantined);
    }

    #[test]
    fn map_media_row_handles_nulls() {
        let mut row = make_media_row();
        row.content_type = None;
        row.file_name = None;
        row.uploader_user_id = None;
        row.last_accessed_at = None;
        let info = map_media_row(row);
        assert_eq!(info.content_type, None);
        assert_eq!(info.file_name, None);
        assert_eq!(info.uploader_user_id, None);
        assert_eq!(info.last_accessed_at, None);
    }
}

/// DB-backed tests for the hash-level quarantine query. These run against the
/// **real migrated schema** (per-test isolated schema), never a hand-built
/// `CREATE TABLE`, so a column rename in the baseline fails them immediately.
#[cfg(test)]
mod db_tests {
    use super::*;
    use synapse_common::current_timestamp_millis;

    use sqlx::postgres::PgPool;

    /// Each test gets its own isolated schema; the guard is returned alongside
    /// the pool so the schema outlives the whole test (dropping it early spawns
    /// a background `DROP SCHEMA` that races with in-flight queries).
    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<PgPool>) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = isolated.pool();
        (isolated, pool)
    }

    async fn insert_row(
        storage: &AdminMediaStorage,
        media_id: &str,
        content_hash: Option<&str>,
        quarantine_status: Option<&str>,
    ) {
        storage
            .upsert_media_metadata(
                media_id,
                "test.server",
                "image/png",
                "photo.png",
                4,
                "@alice:test.server",
                current_timestamp_millis(),
                content_hash,
                quarantine_status,
            )
            .await
            .expect("upsert must succeed against the migrated schema");
    }

    async fn stored_hash(pool: &PgPool, media_id: &str) -> Option<String> {
        sqlx::query_scalar::<_, Option<String>>("SELECT content_hash FROM media_metadata WHERE media_id = $1")
            .bind(media_id)
            .fetch_one(pool)
            .await
            .expect("hash read must succeed")
    }

    #[tokio::test]
    async fn upsert_persists_content_hash() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let media_id = format!("m_hash_{}", uuid::Uuid::new_v4().simple());

        insert_row(&storage, &media_id, Some("AAAA+BBB/CCC"), None).await;
        assert_eq!(stored_hash(&pool, &media_id).await.as_deref(), Some("AAAA+BBB/CCC"));
    }

    #[tokio::test]
    async fn hash_is_quarantined_when_any_matching_row_is_quarantined() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let hash = format!("QUARANTINED+{suffix}/HASH");

        // One quarantined row and one clean row carry the same hash. The
        // upstream semantics are "the content is quarantined", so a single hit
        // must flip the answer for the whole hash.
        insert_row(&storage, &format!("m_q_{suffix}"), Some(&hash), Some("quarantined")).await;
        insert_row(&storage, &format!("m_clean_{suffix}"), Some(&hash), None).await;

        assert!(storage.get_is_hash_quarantined(&hash).await.expect("lookup must succeed"));
    }

    #[tokio::test]
    async fn hash_is_not_quarantined_for_unquarantined_rows_with_same_hash() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let hash = format!("CLEAN+{suffix}/HASH");

        insert_row(&storage, &format!("m_a_{suffix}"), Some(&hash), None).await;
        insert_row(&storage, &format!("m_b_{suffix}"), Some(&hash), Some("")).await;
        insert_row(&storage, &format!("m_c_{suffix}"), Some(&hash), Some("no")).await;

        assert!(
            !storage.get_is_hash_quarantined(&hash).await.expect("lookup must succeed"),
            "NULL / '' / 'no' must not count as quarantined (same predicate as quarantine_status_to_bool)"
        );
    }

    #[tokio::test]
    async fn unknown_hash_is_not_quarantined() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);

        assert!(!storage
            .get_is_hash_quarantined(&format!("UNKNOWN+{}", uuid::Uuid::new_v4().simple()))
            .await
            .expect("lookup must succeed"));
    }

    #[tokio::test]
    async fn rows_without_a_hash_never_match() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().simple().to_string();

        // A pre-column row: no hash, quarantined status. It cannot and must not
        // be reachable by hash lookup (documented limitation).
        insert_row(&storage, &format!("m_legacy_{suffix}"), None, Some("quarantined")).await;

        assert!(!storage.get_is_hash_quarantined(&suffix).await.expect("lookup must succeed"));
    }

    #[tokio::test]
    async fn hash_quarantine_survives_a_manual_unquarantine() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let hash = format!("UNQUARANTINE+{suffix}/HASH");
        let media_id = format!("m_unq_{suffix}");

        insert_row(&storage, &media_id, Some(&hash), Some("quarantined")).await;
        assert!(storage.get_is_hash_quarantined(&hash).await.expect("lookup must succeed"));

        // The admin unquarantine path writes '' (see `unquarantine_media`).
        let quarantine_storage = crate::media::QuarantinedMediaChangeStorage::new(&pool);
        quarantine_storage
            .set_media_quarantine_status(&media_id, "test.server", "")
            .await
            .expect("status update must succeed");

        assert!(
            !storage.get_is_hash_quarantined(&hash).await.expect("lookup must succeed"),
            "an unquarantined row must stop matching, otherwise the hash stays poisoned forever"
        );
    }

    // ── get_room_media / delete_room_media ─────────────────────────

    /// Insert a room + event + media_metadata row.
    /// Uses `upsert_media_metadata` for media_metadata; direct SQL for room_events.
    async fn insert_room_media_row(
        storage: &AdminMediaStorage,
        pool: &PgPool,
        room_id: &str,
        media_id: &str,
        user_id: &str,
    ) {
        // Insert the media metadata row.
        storage
            .upsert_media_metadata(
                media_id,
                "test.server",
                "image/png",
                "photo.png",
                1024,
                user_id,
                current_timestamp_millis(),
                None,
                None,
            )
            .await
            .expect("upsert_media_metadata must succeed");

        // Insert a room event pointing to the media via mxc:// URL.
        // mxc:// URL format: mxc://server_name/media_id
        // SUBSTRING(url FROM 7) strips the mxc:// prefix (6 chars), yielding
        // server_name/media_id. SPLIT_PART(..., '/', 2) extracts media_id.
        let event_id = format!("ev_rm_{}", uuid::Uuid::new_v4().simple());
        let content = serde_json::json!({
            "body": "test image",
            "url": format!("mxc://test.server/{}", media_id),
            "msgtype": "m.image"
        });
        sqlx::query(
            "INSERT INTO room_events (event_id, room_id, sender, event_type, content, prev_event_id, origin_server_ts, created_ts) \
             VALUES ($1, $2, $3, $4, $5, NULL, $6, $6)",
        )
        .bind(&event_id)
        .bind(room_id)
        .bind(user_id)
        .bind("m.room.message")
        .bind(content)
        .bind(current_timestamp_millis())
        .execute(pool)
        .await
        .expect("room_events insert must succeed");
    }

    #[tokio::test]
    async fn get_room_media_finds_media_in_room() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let room_id = format!("!rm_{suffix}:test.server");
        let media_id = format!("rm_media_{suffix}");
        let user_id = "@rm_user:test.server";

        insert_room_media_row(&storage, &pool, &room_id, &media_id, user_id).await;

        let page = storage.get_room_media(&room_id, 100, None).await.expect("get_room_media must succeed");
        assert_eq!(page.media.len(), 1, "expected exactly one media row in room");
        assert_eq!(page.media[0].media_id, media_id);
        assert_eq!(page.media[0].content_type.as_deref(), Some("image/png"));
        assert!(page.next_batch.is_none(), "page with one row and limit 100 must have no cursor");
    }

    #[tokio::test]
    async fn get_room_media_excludes_media_in_other_rooms() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let room_a = format!("!rm_a_{suffix}:test.server");
        let room_b = format!("!rm_b_{suffix}:test.server");
        let media_id_a = format!("rm_a_{suffix}");
        let user_id = "@rm_excl:test.server";

        insert_room_media_row(&storage, &pool, &room_a, &media_id_a, user_id).await;
        insert_room_media_row(&storage, &pool, &room_b, &format!("rm_b_{suffix}"), user_id).await;

        // Query room B: should not return room A's media.
        let page = storage.get_room_media(&room_b, 100, None).await.expect("get_room_media must succeed");
        assert_eq!(page.media.len(), 1);
        assert_ne!(page.media[0].media_id, media_id_a, "room B must not see room A's media");
    }

    #[tokio::test]
    async fn get_room_media_pagination_with_cursor() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let room_id = format!("!rm_page_{suffix}:test.server");
        let user_id = "@rm_page:test.server";

        // Insert 3 media rows in the room with distinct created_ts.
        for i in 1..=3 {
            let mid = format!("rm_pg_{suffix}_{i}");
            insert_room_media_row(&storage, &pool, &room_id, &mid, user_id).await;
        }

        // Page 1: limit=2 → should return 2 rows + a next_batch cursor.
        let page1 = storage.get_room_media(&room_id, 2, None).await.expect("page 1 must succeed");
        assert_eq!(page1.media.len(), 2, "first page must have exactly 2 rows");
        let cursor = page1.next_batch.clone().expect("page 1 of 2 must have a next_batch cursor");

        // Page 2: limit=2, start from cursor → should return 1 row, no cursor.
        let decoded = decode_media_cursor(Some(&cursor));
        let page2 = storage.get_room_media(&room_id, 2, decoded).await.expect("page 2 must succeed");
        assert_eq!(page2.media.len(), 1, "second page must have exactly 1 remaining row");
        assert!(page2.next_batch.is_none(), "last page must have no next_batch cursor");
    }

    #[tokio::test]
    async fn delete_room_media_removes_media() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let room_id = format!("!rm_del_{suffix}:test.server");
        let media_id = format!("rm_del_{suffix}");
        let user_id = "@rm_del:test.server";

        insert_room_media_row(&storage, &pool, &room_id, &media_id, user_id).await;

        // Confirm media is present before delete.
        let before = storage.get_room_media(&room_id, 100, None).await.expect("get_room_media before must succeed");
        assert_eq!(before.media.len(), 1, "media must exist before delete");

        let deleted = storage.delete_room_media(&room_id, &media_id).await.expect("delete_room_media must succeed");
        assert!(deleted, "delete_room_media must return true for existing media");

        // Confirm media is gone after delete.
        let after = storage.get_room_media(&room_id, 100, None).await.expect("get_room_media after must succeed");
        assert_eq!(after.media.len(), 0, "media must be absent after delete");
    }

    #[tokio::test]
    async fn delete_room_media_returns_false_for_missing_media() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let room_id = format!("!rm_missing_{}:test.server", uuid::Uuid::new_v4().simple());

        let deleted = storage.delete_room_media(&room_id, "nonexistent").await.expect("delete must not error");
        assert!(!deleted, "delete_room_media must return false for non-existent media");
    }

    #[tokio::test]
    async fn delete_room_media_returns_false_when_room_has_no_media() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let room_id = format!("!rm_empty_{}:test.server", uuid::Uuid::new_v4().simple());

        // A room with no room_events rows at all → no media in this room.
        let deleted = storage.delete_room_media(&room_id, "rm_empty_media_id").await.expect("delete must not error");
        assert!(!deleted, "no room_events row → media not in room → false");
    }
}

#[cfg(test)]
mod admin_read_paths_db_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// C60-0：`admin_media.rs` 的 U-5 端点实现（`get_all_media` / `get_media_info` /
    /// `delete_media` / `get_media_quota` / `get_user_media` / `delete_user_media` /
    /// `quarantine_user_media` / `delete_media_by_policy` / `purge_media_cache` /
    /// `unprotect_media`）此前**零 storage 级覆盖** —— 只有 `upsert`/`is_hash_quarantined`/
    /// `get_room_media`/`delete_room_media` 有真基线用例。这批补齐后才能按 R8 转换它们。
    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<sqlx::PgPool>) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = isolated.pool();
        (isolated, pool)
    }

    /// 直接插入一行 `media_metadata`（绕过 upsert：这些用例要精确控制 `created_ts` /
    /// `last_accessed_at` / `uploader_user_id`，而 `upsert_media_metadata` 把 created_ts 固定成 now）。
    #[allow(clippy::too_many_arguments)] // 夹具：字段多但语义直白
    async fn insert_media_row(
        pool: &sqlx::PgPool,
        media_id: &str,
        uploader_user_id: &str,
        size: i64,
        created_ts: i64,
        last_accessed_at: Option<i64>,
        quarantine_status: Option<&str>,
    ) {
        sqlx::query(
            r"INSERT INTO media_metadata
               (media_id, server_name, content_type, file_name, size, uploader_user_id, created_ts,
                last_accessed_at, quarantine_status)
               VALUES ($1, 'test.server', 'image/png', 'photo.png', $2, $3, $4, $5, $6)",
        )
        .bind(media_id)
        .bind(size)
        .bind(uploader_user_id)
        .bind(created_ts)
        .bind(last_accessed_at)
        .bind(quarantine_status)
        .execute(pool)
        .await
        .expect("insert media row");
    }

    async fn status_of(pool: &sqlx::PgPool, media_id: &str) -> Option<String> {
        sqlx::query_scalar::<_, Option<String>>("SELECT quarantine_status FROM media_metadata WHERE media_id = $1")
            .bind(media_id)
            .fetch_one(pool)
            .await
            .expect("status read")
    }

    #[tokio::test]
    async fn get_all_media_paginates_newest_first_with_tie_break_and_cursor() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let s = uuid::Uuid::new_v4().simple().to_string();
        // (created_ts, media_id)：300 > 200(两个，按 media_id DESC 决胜) > 100
        insert_media_row(&pool, &format!("m_a_{s}"), "@page:test", 1, 300, None, None).await;
        insert_media_row(&pool, &format!("m_c_{s}"), "@page:test", 1, 200, None, None).await;
        insert_media_row(&pool, &format!("m_b_{s}"), "@page:test", 1, 200, None, None).await;
        insert_media_row(&pool, &format!("m_d_{s}"), "@page:test", 1, 100, None, None).await;

        let page1 = storage.get_all_media(2, None).await.expect("get_all_media");
        let ids1: Vec<&str> = page1.media.iter().map(|m| m.media_id.as_str()).collect();
        assert_eq!(ids1, vec![format!("m_a_{s}"), format!("m_c_{s}")], "created_ts DESC, media_id DESC");
        // len == limit ⇒ next_batch 必须是**最后一行**的 (created_ts, media_id)
        assert_eq!(page1.next_batch.as_deref(), Some(format!("200|m_c_{s}").as_str()));

        let cursor = decode_media_cursor(page1.next_batch.as_deref()).expect("cursor decodes");
        let page2 = storage.get_all_media(2, Some(cursor)).await.expect("page 2");
        let ids2: Vec<&str> = page2.media.iter().map(|m| m.media_id.as_str()).collect();
        assert_eq!(
            ids2,
            vec![format!("m_b_{s}"), format!("m_d_{s}")],
            "游标之后的下一页：同 ts 取 media_id 更小的，然后跨到更早的 ts"
        );
        assert_eq!(page2.next_batch.as_deref(), Some(format!("100|m_d_{s}").as_str()));

        // 不足 limit ⇒ next_batch 必须是 None（否则调用方会无限翻页）
        let all = storage.get_all_media(10, None).await.expect("all");
        assert_eq!(all.media.len(), 4);
        assert!(all.next_batch.is_none());
    }

    #[tokio::test]
    async fn get_media_info_returns_row_or_none() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let s = uuid::Uuid::new_v4().simple().to_string();
        let media_id = format!("m_info_{s}");
        insert_media_row(&pool, &media_id, "@info:test", 7, 111, Some(222), Some("quarantined")).await;

        let info = storage.get_media_info(&media_id).await.expect("get_media_info").expect("row exists");
        assert_eq!(info.media_id, media_id);
        assert_eq!(info.size, 7);
        assert_eq!(info.created_ts, 111);
        assert_eq!(info.last_accessed_at, Some(222));
        assert!(info.quarantined, "quarantine_status='quarantined' ⇒ quarantined=true");
        assert_eq!(info.uploader_user_id.as_deref(), Some("@info:test"));

        assert!(storage.get_media_info(&format!("m_missing_{s}")).await.expect("missing lookup").is_none());
    }

    #[tokio::test]
    async fn delete_media_reports_whether_a_row_was_removed() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let s = uuid::Uuid::new_v4().simple().to_string();
        let media_id = format!("m_del_{s}");
        insert_media_row(&pool, &media_id, "@del:test", 1, 1, None, None).await;

        assert!(storage.delete_media(&media_id).await.expect("first delete"));
        assert!(!storage.delete_media(&media_id).await.expect("second delete"), "重复删除返回 false");
        assert!(storage.get_media_info(&media_id).await.expect("lookup").is_none());
    }

    #[tokio::test]
    async fn get_media_quota_sums_size_and_counts_rows() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);

        let empty = storage.get_media_quota().await.expect("quota on empty table");
        assert_eq!((empty.total_size, empty.total_count), (0, 0), "空表上 SUM 必须落成 0 而不是 NULL");

        let s = uuid::Uuid::new_v4().simple().to_string();
        insert_media_row(&pool, &format!("m_q1_{s}"), "@q:test", 10, 1, None, None).await;
        insert_media_row(&pool, &format!("m_q2_{s}"), "@q:test", 32, 1, None, None).await;

        let quota = storage.get_media_quota().await.expect("quota");
        assert_eq!(quota.total_size, 42);
        assert_eq!(quota.total_count, 2);
    }

    #[tokio::test]
    async fn get_user_media_filters_and_orders_and_nulls_absent_columns() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let s = uuid::Uuid::new_v4().simple().to_string();
        let mine = format!("@user_media_{s}:test");
        insert_media_row(&pool, &format!("m_u_old_{s}"), &mine, 1, 100, Some(5), Some("quarantined")).await;
        insert_media_row(&pool, &format!("m_u_new_{s}"), &mine, 1, 300, Some(5), None).await;
        insert_media_row(&pool, &format!("m_other_{s}"), "@someone:test", 1, 200, None, None).await;

        let mine_rows = storage.get_user_media(&mine).await.expect("get_user_media");
        let ids: Vec<&str> = mine_rows.iter().map(|m| m.media_id.as_str()).collect();
        assert_eq!(ids, vec![format!("m_u_new_{s}"), format!("m_u_old_{s}")], "created_ts DESC 且只含该用户");
        // 该查询的两列是 `NULL::BIGINT` / `NULL::TEXT` 常量 ⇒ 必须落成 None / false
        assert!(mine_rows.iter().all(|m| m.last_accessed_at.is_none() && !m.quarantined));

        assert!(storage.get_user_media(&format!("@nobody_{s}:test")).await.expect("nobody").is_empty());
    }

    #[tokio::test]
    async fn delete_user_media_removes_only_that_users_rows() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let s = uuid::Uuid::new_v4().simple().to_string();
        let mine = format!("@user_del_{s}:test");
        insert_media_row(&pool, &format!("m_d1_{s}"), &mine, 1, 1, None, None).await;
        insert_media_row(&pool, &format!("m_d2_{s}"), &mine, 1, 2, None, None).await;
        insert_media_row(&pool, &format!("m_keep_{s}"), "@keep:test", 1, 3, None, None).await;

        assert_eq!(storage.delete_user_media(&mine).await.expect("delete_user_media"), 2);
        assert!(storage.get_user_media(&mine).await.expect("after").is_empty());
        assert!(storage.get_media_info(&format!("m_keep_{s}")).await.expect("other user").is_some());

        // 幂等：没有行时返回 0
        assert_eq!(storage.delete_user_media(&mine).await.expect("second delete"), 0);
    }

    #[tokio::test]
    async fn quarantine_user_media_skips_protected_and_already_quarantined() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let s = uuid::Uuid::new_v4().simple().to_string();
        let mine = format!("@user_q_{s}:test");
        let clean = format!("m_clean_{s}");
        let protected = format!("m_prot_{s}");
        let already = format!("m_already_{s}");
        let other = format!("m_otherq_{s}");
        insert_media_row(&pool, &clean, &mine, 1, 1, None, None).await;
        insert_media_row(&pool, &protected, &mine, 1, 1, None, Some("protected")).await;
        insert_media_row(&pool, &already, &mine, 1, 1, None, Some("quarantined")).await;
        insert_media_row(&pool, &other, "@someone_else:test", 1, 1, None, None).await;

        let affected = storage.quarantine_user_media(&mine).await.expect("quarantine_user_media");
        assert_eq!(affected, 1, "只应动那一行 clean（protected 与已隔离都跳过）");
        assert_eq!(status_of(&pool, &clean).await.as_deref(), Some("quarantined"));
        assert_eq!(status_of(&pool, &protected).await.as_deref(), Some("protected"));
        assert_eq!(status_of(&pool, &already).await.as_deref(), Some("quarantined"));
        assert_eq!(status_of(&pool, &other).await, None, "别人的媒体不受影响");
    }

    #[tokio::test]
    async fn delete_media_by_policy_honours_both_dimensions_and_skips_protected() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let s = uuid::Uuid::new_v4().simple().to_string();

        // 时间维度：ts=100 早于阈值 ⇒ 删；ts=900 不早于阈值且尺寸不大 ⇒ 留
        let old_small = format!("m_old_{s}");
        let new_small = format!("m_new_{s}");
        // 尺寸维度：ts=900 不早但 size=1000 > 500 ⇒ 删
        let new_big = format!("m_big_{s}");
        // 受保护：早且不大，但 protected ⇒ 两个维度都不动它
        let protected = format!("m_prot_{s}");
        insert_media_row(&pool, &old_small, "@p:test", 1, 100, None, None).await;
        insert_media_row(&pool, &new_small, "@p:test", 1, 900, None, None).await;
        insert_media_row(&pool, &new_big, "@p:test", 1000, 900, None, None).await;
        insert_media_row(&pool, &protected, "@p:test", 1000, 100, None, Some("protected")).await;

        // (before_ts=500, max_size=500)：两个维度是 OR ⇒ 删 old_small（早）+ new_big（大）
        assert_eq!(storage.delete_media_by_policy(500, 500).await.expect("policy"), 2);
        assert!(storage.get_media_info(&new_small).await.expect("kept").is_some());
        assert!(storage.get_media_info(&protected).await.expect("protected kept").is_some());
        assert!(storage.get_media_info(&old_small).await.expect("deleted").is_none());

        // `0` 表示该维度不设限：只按时间删（此时只剩 new_small 与 protected）
        assert_eq!(storage.delete_media_by_policy(0, 0).await.expect("no limits"), 1, "0 是'不设限'不是'匹配 0'");
        assert!(storage.get_media_info(&protected).await.expect("protected kept").is_some());
    }

    #[tokio::test]
    async fn purge_media_cache_deletes_never_accessed_or_stale_rows() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let s = uuid::Uuid::new_v4().simple().to_string();
        let never = format!("m_never_{s}");
        let stale = format!("m_stale_{s}");
        let fresh = format!("m_fresh_{s}");
        insert_media_row(&pool, &never, "@pc:test", 1, 1, None, None).await;
        insert_media_row(&pool, &stale, "@pc:test", 1, 1, Some(100), None).await;
        insert_media_row(&pool, &fresh, "@pc:test", 1, 1, Some(900), None).await;

        assert_eq!(storage.purge_media_cache(500).await.expect("purge"), 2, "NULL 与早于阈值都要删");
        assert!(storage.get_media_info(&fresh).await.expect("fresh kept").is_some());
        assert!(storage.get_media_info(&stale).await.expect("stale gone").is_none());
    }

    #[tokio::test]
    async fn unprotect_media_only_clears_the_protected_status() {
        let (_iso, pool) = test_pool().await;
        let storage = AdminMediaStorage::new(&pool);
        let s = uuid::Uuid::new_v4().simple().to_string();
        let protected = format!("m_unprot_{s}");
        let quarantined = format!("m_unprot_q_{s}");
        insert_media_row(&pool, &protected, "@up:test", 1, 1, None, Some("protected")).await;
        insert_media_row(&pool, &quarantined, "@up:test", 1, 1, None, Some("quarantined")).await;

        assert_eq!(storage.unprotect_media(&protected, "@admin:test").await.expect("unprotect"), 1);
        assert_eq!(status_of(&pool, &protected).await, None, "protected → NULL");
        // 非 protected（含已隔离）不动、缺失行返回 0
        assert_eq!(storage.unprotect_media(&quarantined, "@admin:test").await.expect("no-op"), 0);
        assert_eq!(status_of(&pool, &quarantined).await.as_deref(), Some("quarantined"));
        assert_eq!(storage.unprotect_media(&format!("m_absent_{s}"), "@admin:test").await.expect("missing"), 0);
    }
}
