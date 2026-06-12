use crate::common::ApiError;
use crate::storage::UserStorage;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::instrument;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaCursor {
    pub created_ts: i64,
    pub media_id: String,
}

pub fn decode_media_cursor(cursor: Option<&str>) -> Option<MediaCursor> {
    let cursor = cursor?;
    let (created_ts, media_id) = cursor.split_once('|')?;
    let created_ts = created_ts.parse::<i64>().ok()?;
    if media_id.is_empty() {
        return None;
    }
    Some(MediaCursor { created_ts, media_id: media_id.to_owned() })
}

pub fn encode_media_cursor(cursor: &MediaCursor) -> String {
    format!("{}|{}", cursor.created_ts, cursor.media_id)
}

#[derive(Debug, Clone)]
pub struct AdminMediaInfo {
    pub media_id: String,
    pub content_type: Option<String>,
    pub file_name: Option<String>,
    pub size: i64,
    pub uploader_user_id: Option<String>,
    pub created_ts: i64,
    pub last_accessed_at: Option<i64>,
    pub quarantined: bool,
}

#[derive(Debug, Clone)]
pub struct AdminMediaPage {
    pub media: Vec<AdminMediaInfo>,
    pub next_batch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdminMediaQuotaSummary {
    pub total_size: i64,
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

pub struct AdminMediaService {
    pool: Arc<PgPool>,
    user_storage: UserStorage,
}

impl AdminMediaService {
    pub fn new(pool: Arc<PgPool>, user_storage: UserStorage) -> Self {
        Self { pool, user_storage }
    }

    #[instrument(skip(self))]
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
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let next_batch = if media.len() as i64 == limit {
            media.last().map(|row| {
                encode_media_cursor(&MediaCursor { created_ts: row.created_ts, media_id: row.media_id.clone() })
            })
        } else {
            None
        };

        Ok(AdminMediaPage { media: media.into_iter().map(map_media_row).collect(), next_batch })
    }

    #[instrument(skip(self))]
    pub async fn get_media_info(&self, media_id: &str) -> Result<Option<AdminMediaInfo>, ApiError> {
        let media: Option<AdminMediaRow> = sqlx::query_as::<_, AdminMediaRow>(
            r#"SELECT media_id, content_type, file_name, size, uploader_user_id, created_ts, last_accessed_at, quarantine_status
               FROM media_metadata WHERE media_id = $1"#,
        )
        .bind(media_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(media.map(map_media_row))
    }

    #[instrument(skip(self))]
    pub async fn delete_media(&self, media_id: &str) -> Result<(), ApiError> {
        let result = sqlx::query!("DELETE FROM media_metadata WHERE media_id = $1", media_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if result.rows_affected() == 0 {
            return Err(ApiError::not_found("Media not found".to_string()));
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_media_quota(&self) -> Result<AdminMediaQuotaSummary, ApiError> {
        let total_size = sqlx::query_scalar::<_, i64>("SELECT COALESCE(SUM(size), 0)::BIGINT FROM media_metadata")
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
        let total_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::BIGINT FROM media_metadata")
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(AdminMediaQuotaSummary { total_size, total_count })
    }

    #[instrument(skip(self))]
    pub async fn get_user_media(&self, identifier: &str) -> Result<(String, Vec<AdminMediaInfo>), ApiError> {
        let user = self
            .user_storage
            .get_user_by_identifier(identifier)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?
            .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

        let media: Vec<AdminMediaRow> = sqlx::query_as::<_, AdminMediaRow>(
            r#"SELECT media_id, content_type, file_name, size, uploader_user_id, created_ts,
               NULL::BIGINT AS last_accessed_at, NULL::TEXT AS quarantine_status
               FROM media_metadata WHERE uploader_user_id = $1 ORDER BY created_ts DESC"#,
        )
        .bind(&user.user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok((user.user_id, media.into_iter().map(map_media_row).collect()))
    }

    #[instrument(skip(self))]
    pub async fn delete_user_media(&self, identifier: &str) -> Result<u64, ApiError> {
        let user = self
            .user_storage
            .get_user_by_identifier(identifier)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?
            .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

        let result = sqlx::query!("DELETE FROM media_metadata WHERE uploader_user_id = $1", &user.user_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_media_cursor, encode_media_cursor, MediaCursor};

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
}
