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
}

impl AdminMediaStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`upsert_media_metadata`].
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
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO media_metadata (media_id, server_name, content_type, file_name, size, uploader_user_id, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (media_id) DO UPDATE
            SET content_type = EXCLUDED.content_type,
                file_name = EXCLUDED.file_name,
                size = EXCLUDED.size
            "#,
        )
        .bind(media_id)
        .bind(server_name)
        .bind(content_type)
        .bind(file_name)
        .bind(size)
        .bind(uploader_user_id)
        .bind(created_ts)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(())
    }

    /// See [`get_all_media`].
    pub async fn get_all_media(&self, limit: i64, cursor: Option<MediaCursor>) -> Result<AdminMediaPage, ApiError> {
        let media: Vec<AdminMediaRow> = sqlx::query_as::<_, AdminMediaRow>(
            r#"SELECT media_id, content_type, file_name, size, uploader_user_id, created_ts, last_accessed_at, quarantine_status
               FROM media_metadata
               WHERE ($1::BIGINT IS NULL AND $2::TEXT IS NULL)
                  OR created_ts < $1
                  OR (created_ts = $1 AND media_id < $2)
               ORDER BY created_ts DESC, media_id DESC
               LIMIT $3"#,
        )
        .bind(cursor.as_ref().map(|cursor| cursor.created_ts))
        .bind(cursor.as_ref().map(|cursor| cursor.media_id.as_str()))
        .bind(limit)
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
        let media: Option<AdminMediaRow> = sqlx::query_as::<_, AdminMediaRow>(
            r#"SELECT media_id, content_type, file_name, size, uploader_user_id, created_ts, last_accessed_at, quarantine_status
               FROM media_metadata WHERE media_id = $1"#,
        )
        .bind(media_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(media.map(map_media_row))
    }

    /// See [`delete_media`].
    pub async fn delete_media(&self, media_id: &str) -> Result<bool, ApiError> {
        let result = sqlx::query("DELETE FROM media_metadata WHERE media_id = $1")
            .bind(media_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`get_media_quota`].
    pub async fn get_media_quota(&self) -> Result<AdminMediaQuotaSummary, ApiError> {
        let total_size = sqlx::query_scalar::<_, i64>("SELECT COALESCE(SUM(size), 0)::BIGINT FROM media_metadata")
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;
        let total_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::BIGINT FROM media_metadata")
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(AdminMediaQuotaSummary { total_size, total_count })
    }

    /// See [`get_user_media`].
    pub async fn get_user_media(&self, user_id: &str) -> Result<Vec<AdminMediaInfo>, ApiError> {
        let media: Vec<AdminMediaRow> = sqlx::query_as::<_, AdminMediaRow>(
            r#"SELECT media_id, content_type, file_name, size, uploader_user_id, created_ts,
               NULL::BIGINT AS last_accessed_at, NULL::TEXT AS quarantine_status
               FROM media_metadata WHERE uploader_user_id = $1 ORDER BY created_ts DESC"#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(media.into_iter().map(map_media_row).collect())
    }

    /// See [`delete_user_media`].
    pub async fn delete_user_media(&self, user_id: &str) -> Result<u64, ApiError> {
        let result = sqlx::query("DELETE FROM media_metadata WHERE uploader_user_id = $1")
            .bind(user_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(result.rows_affected())
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
