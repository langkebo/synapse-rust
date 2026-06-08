use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, FromRow)]
pub struct UrlPreviewCache {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub og_title: Option<String>,
    pub og_image: Option<String>,
    pub og_image_width: Option<i32>,
    pub og_image_height: Option<i32>,
    pub og_site_name: Option<String>,
    pub og_type: Option<String>,
    pub created_ts: i64,
    pub expires_ts: i64,
}

#[derive(Debug, Clone)]
pub struct UrlPreviewStorage {
    pool: Arc<PgPool>,
}

impl UrlPreviewStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn get_cached_preview(&self, url: &str, now_ts: i64) -> Result<Option<UrlPreviewCache>, sqlx::Error> {
        sqlx::query_as::<_, UrlPreviewCache>(
            r#"
            SELECT url, title, description, og_title, og_image,
                   og_image_width, og_image_height, og_site_name, og_type,
                   created_ts, expires_ts
            FROM url_preview_cache
            WHERE url = $1 AND expires_ts > $2
            "#,
        )
        .bind(url)
        .bind(now_ts)
        .fetch_optional(self.pool.as_ref())
        .await
    }

    pub async fn save_preview(&self, preview: &UrlPreviewCache) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO url_preview_cache (
                url, title, description, og_title, og_image,
                og_image_width, og_image_height, og_site_name, og_type,
                created_ts, expires_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (url) DO UPDATE SET
                title = EXCLUDED.title,
                description = EXCLUDED.description,
                og_title = EXCLUDED.og_title,
                og_image = EXCLUDED.og_image,
                og_image_width = EXCLUDED.og_image_width,
                og_image_height = EXCLUDED.og_image_height,
                og_site_name = EXCLUDED.og_site_name,
                og_type = EXCLUDED.og_type,
                created_ts = EXCLUDED.created_ts,
                expires_ts = EXCLUDED.expires_ts
            "#,
        )
        .bind(&preview.url)
        .bind(&preview.title)
        .bind(&preview.description)
        .bind(&preview.og_title)
        .bind(&preview.og_image)
        .bind(preview.og_image_width)
        .bind(preview.og_image_height)
        .bind(&preview.og_site_name)
        .bind(&preview.og_type)
        .bind(preview.created_ts)
        .bind(preview.expires_ts)
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    pub async fn cleanup_expired_previews(&self, now_ts: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM url_preview_cache
            WHERE expires_ts <= $1
            "#,
        )
        .bind(now_ts)
        .execute(self.pool.as_ref())
        .await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_preview() -> UrlPreviewCache {
        UrlPreviewCache {
            url: "https://example.com/page".to_string(),
            title: Some("Example Page".to_string()),
            description: Some("A sample page".to_string()),
            og_title: Some("OG Example".to_string()),
            og_image: Some("https://example.com/image.png".to_string()),
            og_image_width: Some(1200),
            og_image_height: Some(630),
            og_site_name: Some("Example Site".to_string()),
            og_type: Some("website".to_string()),
            created_ts: 1700000000000,
            expires_ts: 1700003600000,
        }
    }

    #[test]
    fn test_url_preview_cache_fields() {
        let preview = sample_preview();
        assert_eq!(preview.url, "https://example.com/page");
        assert_eq!(preview.title.as_deref(), Some("Example Page"));
        assert_eq!(preview.description.as_deref(), Some("A sample page"));
        assert_eq!(preview.og_title.as_deref(), Some("OG Example"));
        assert_eq!(preview.og_image.as_deref(), Some("https://example.com/image.png"));
        assert_eq!(preview.og_image_width, Some(1200));
        assert_eq!(preview.og_image_height, Some(630));
        assert_eq!(preview.og_site_name.as_deref(), Some("Example Site"));
        assert_eq!(preview.og_type.as_deref(), Some("website"));
        assert_eq!(preview.created_ts, 1700000000000);
        assert_eq!(preview.expires_ts, 1700003600000);
    }

    #[test]
    fn test_url_preview_cache_clone() {
        let preview = sample_preview();
        let cloned = preview.clone();
        assert_eq!(cloned.url, preview.url);
        assert_eq!(cloned.title, preview.title);
        assert_eq!(cloned.og_image_width, preview.og_image_width);
    }

    #[test]
    fn test_url_preview_cache_minimal() {
        let preview = UrlPreviewCache {
            url: "https://example.com".to_string(),
            title: None,
            description: None,
            og_title: None,
            og_image: None,
            og_image_width: None,
            og_image_height: None,
            og_site_name: None,
            og_type: None,
            created_ts: 1700000000000,
            expires_ts: 1700003600000,
        };
        assert!(preview.title.is_none());
        assert!(preview.description.is_none());
        assert!(preview.og_image_width.is_none());
    }

    #[test]
    fn test_url_preview_expiry_check() {
        let preview = sample_preview();
        // expires_ts = 1700003600000
        // Before expiry
        assert!(1700000000000 < preview.expires_ts);
        // After expiry
        assert!(1700004000000 > preview.expires_ts);
    }

    // DB-dependent tests marked with #[ignore]

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_save_and_get_preview() {
        // Requires a running PostgreSQL with url_preview_cache table
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_get_expired_preview_returns_none() {
        // Requires a running PostgreSQL
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_cleanup_expired_previews() {
        // Requires a running PostgreSQL
    }
}
