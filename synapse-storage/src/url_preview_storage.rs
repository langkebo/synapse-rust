use async_trait::async_trait;
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
    pub expires_at: i64,
}

/// Trait abstraction over [`UrlPreviewStorage`] for testability.
#[async_trait]
pub trait UrlPreviewStoreApi: Send + Sync {
    async fn get_cached_preview(&self, url: &str, now_ts: i64) -> Result<Option<UrlPreviewCache>, sqlx::Error>;
    async fn save_preview(&self, preview: &UrlPreviewCache) -> Result<(), sqlx::Error>;
    async fn cleanup_expired_previews(&self, now_ts: i64) -> Result<u64, sqlx::Error>;
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
                   created_ts, expires_at
            FROM url_preview_cache
            WHERE url = $1 AND expires_at > $2
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
                created_ts, expires_at
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
                expires_at = EXCLUDED.expires_at
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
        .bind(preview.expires_at)
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    pub async fn cleanup_expired_previews(&self, now_ts: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM url_preview_cache
            WHERE expires_at <= $1
            "#,
        )
        .bind(now_ts)
        .execute(self.pool.as_ref())
        .await?;
        Ok(result.rows_affected())
    }
}

#[async_trait]
impl UrlPreviewStoreApi for UrlPreviewStorage {
    async fn get_cached_preview(&self, url: &str, now_ts: i64) -> Result<Option<UrlPreviewCache>, sqlx::Error> {
        self.get_cached_preview(url, now_ts).await
    }
    async fn save_preview(&self, preview: &UrlPreviewCache) -> Result<(), sqlx::Error> {
        self.save_preview(preview).await
    }
    async fn cleanup_expired_previews(&self, now_ts: i64) -> Result<u64, sqlx::Error> {
        self.cleanup_expired_previews(now_ts).await
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
            expires_at: 1700003600000,
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
        assert_eq!(preview.expires_at, 1700003600000);
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
            expires_at: 1700003600000,
        };
        assert!(preview.title.is_none());
        assert!(preview.description.is_none());
        assert!(preview.og_image_width.is_none());
    }

    #[test]
    fn test_url_preview_expiry_check() {
        let preview = sample_preview();
        // expires_at = 1700003600000
        // Before expiry
        assert!(1700000000000 < preview.expires_at);
        // After expiry
        assert!(1700004000000 > preview.expires_at);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    const BASE_TS: i64 = 1700000000000;
    const ONE_HOUR_MS: i64 = 3_600_000;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn cleanup_by_prefix(pool: &PgPool, prefix: &str) {
        let _ = sqlx::query("DELETE FROM url_preview_cache WHERE url LIKE $1")
            .bind(format!("{}%", prefix))
            .execute(pool)
            .await;
    }

    fn make_preview(url: &str, created_ts: i64, expires_at: i64) -> UrlPreviewCache {
        UrlPreviewCache {
            url: url.to_string(),
            title: Some("Test Title".to_string()),
            description: Some("A test description for preview.".to_string()),
            og_title: Some("OG Title".to_string()),
            og_image: Some("https://example.com/og.png".to_string()),
            og_image_width: Some(1200),
            og_image_height: Some(630),
            og_site_name: Some("Example".to_string()),
            og_type: Some("article".to_string()),
            created_ts,
            expires_at,
        }
    }

    // ---- Tests ----

    #[tokio::test]
    async fn test_save_and_get_preview() {
        let pool = test_pool().await;
        let storage = UrlPreviewStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string();
        let url = format!("https://example.com/test-save-{suffix}");
        let prefix = format!("https://example.com/test-save-{suffix}");

        // Cleanup before and after
        cleanup_by_prefix(&pool, &prefix).await;

        let preview = make_preview(&url, BASE_TS, BASE_TS + ONE_HOUR_MS);

        // Save
        storage.save_preview(&preview).await.expect("save_preview should succeed");

        // Retrieve with a timestamp between created_ts and expires_at
        let found = storage
            .get_cached_preview(&url, BASE_TS + 1000)
            .await
            .expect("get_cached_preview should succeed")
            .expect("preview should be found");

        assert_eq!(found.url, url);
        assert_eq!(found.title.as_deref(), Some("Test Title"));
        assert_eq!(found.description.as_deref(), Some("A test description for preview."));
        assert_eq!(found.og_title.as_deref(), Some("OG Title"));
        assert_eq!(found.og_image.as_deref(), Some("https://example.com/og.png"));
        assert_eq!(found.og_image_width, Some(1200));
        assert_eq!(found.og_image_height, Some(630));
        assert_eq!(found.og_site_name.as_deref(), Some("Example"));
        assert_eq!(found.og_type.as_deref(), Some("article"));
        assert_eq!(found.created_ts, BASE_TS);
        assert_eq!(found.expires_at, BASE_TS + ONE_HOUR_MS);

        cleanup_by_prefix(&pool, &prefix).await;
    }

    #[tokio::test]
    async fn test_get_preview_not_found() {
        let pool = test_pool().await;
        let storage = UrlPreviewStorage::new(&pool);
        let nonexistent_url = format!("https://example.com/nonexistent-{}", uuid::Uuid::new_v4());

        let result = storage
            .get_cached_preview(&nonexistent_url, BASE_TS)
            .await
            .expect("get_cached_preview should succeed for missing URL");

        assert!(result.is_none(), "nonexistent URL should return None");
    }

    #[tokio::test]
    async fn test_get_expired_preview_returns_none() {
        let pool = test_pool().await;
        let storage = UrlPreviewStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string();
        let url = format!("https://example.com/test-expired-{suffix}");
        let prefix = url.clone();

        cleanup_by_prefix(&pool, &prefix).await;

        // Save a preview with expires_at in the past relative to our lookup time
        let preview = make_preview(&url, BASE_TS, BASE_TS + ONE_HOUR_MS);
        storage.save_preview(&preview).await.expect("save_preview should succeed");

        // Query with now_ts after expires_at — should return None
        let found = storage
            .get_cached_preview(&url, BASE_TS + ONE_HOUR_MS + 1)
            .await
            .expect("get_cached_preview should succeed");

        assert!(found.is_none(), "expired preview should return None");

        cleanup_by_prefix(&pool, &prefix).await;
    }

    #[tokio::test]
    async fn test_update_preview_via_upsert() {
        let pool = test_pool().await;
        let storage = UrlPreviewStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string();
        let url = format!("https://example.com/test-upsert-{suffix}");
        let prefix = url.clone();

        cleanup_by_prefix(&pool, &prefix).await;

        // Insert initial preview
        let preview1 = make_preview(&url, BASE_TS, BASE_TS + ONE_HOUR_MS);
        storage.save_preview(&preview1).await.expect("first save should succeed");

        // Update with new data — same URL, different fields
        let preview2 = UrlPreviewCache {
            url: url.clone(),
            title: Some("Updated Title".to_string()),
            description: None,
            og_title: Some("Updated OG".to_string()),
            og_image: None,
            og_image_width: None,
            og_image_height: None,
            og_site_name: None,
            og_type: None,
            created_ts: BASE_TS + 5000,
            expires_at: BASE_TS + 2 * ONE_HOUR_MS,
        };
        storage.save_preview(&preview2).await.expect("second save (upsert) should succeed");

        // Retrieve and verify updated fields
        let found = storage
            .get_cached_preview(&url, BASE_TS + ONE_HOUR_MS)
            .await
            .expect("get_cached_preview should succeed")
            .expect("updated preview should be found");

        assert_eq!(found.title.as_deref(), Some("Updated Title"));
        assert!(found.description.is_none(), "description should be updated to None");
        assert_eq!(found.og_title.as_deref(), Some("Updated OG"));
        assert!(found.og_image.is_none(), "og_image should be updated to None");
        assert_eq!(found.created_ts, BASE_TS + 5000);
        assert_eq!(found.expires_at, BASE_TS + 2 * ONE_HOUR_MS);

        // Verify only one row exists for this URL
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*)::bigint FROM url_preview_cache WHERE url = $1")
            .bind(&url)
            .fetch_one(pool.as_ref())
            .await
            .expect("count query should succeed");
        assert_eq!(count.0, 1, "upsert should not create duplicate rows");

        cleanup_by_prefix(&pool, &prefix).await;
    }

    #[tokio::test]
    async fn test_cleanup_expired_previews() {
        let pool = test_pool().await;
        let storage = UrlPreviewStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string();
        let prefix = format!("https://example.com/test-cleanup-{suffix}");

        cleanup_by_prefix(&pool, &prefix).await;

        let url_alive = format!("{prefix}/alive");
        let url_dead1 = format!("{prefix}/dead1");
        let url_dead2 = format!("{prefix}/dead2");

        // Preview that is still valid (expires far in future)
        let alive = make_preview(&url_alive, BASE_TS, BASE_TS + 10 * ONE_HOUR_MS);
        storage.save_preview(&alive).await.expect("alive save should succeed");

        // Preview that expires at cutoff boundary (exclusive for get, inclusive for cleanup)
        // expires_at = cleanup_ts means it IS expired for cleanup purposes
        let dead1 = make_preview(&url_dead1, BASE_TS, BASE_TS + ONE_HOUR_MS);
        storage.save_preview(&dead1).await.expect("dead1 save should succeed");

        // Preview that expired long ago
        let dead2 = make_preview(&url_dead2, BASE_TS - 2 * ONE_HOUR_MS, BASE_TS - ONE_HOUR_MS);
        storage.save_preview(&dead2).await.expect("dead2 save should succeed");

        // Cleanup with cutoff at BASE_TS + ONE_HOUR_MS
        // dead1 has expires_at = BASE_TS + ONE_HOUR_MS (<= cutoff, gets deleted)
        // dead2 has expires_at = BASE_TS - ONE_HOUR_MS (< cutoff, gets deleted)
        // alive has expires_at = BASE_TS + 10 * ONE_HOUR_MS (> cutoff, stays)
        let deleted = storage.cleanup_expired_previews(BASE_TS + ONE_HOUR_MS).await.expect("cleanup should succeed");
        assert!(deleted >= 2, "should delete at least 2 expired previews");

        // Verify alive preview still exists
        let found_alive =
            storage.get_cached_preview(&url_alive, BASE_TS + 5 * ONE_HOUR_MS).await.expect("get should succeed");
        assert!(found_alive.is_some(), "non-expired preview should survive cleanup");

        // Verify dead previews are gone
        let found_dead1 =
            storage.get_cached_preview(&url_dead1, BASE_TS + ONE_HOUR_MS + 1).await.expect("get should succeed");
        assert!(found_dead1.is_none(), "expired preview should be deleted");

        let found_dead2 = storage.get_cached_preview(&url_dead2, BASE_TS).await.expect("get should succeed");
        assert!(found_dead2.is_none(), "expired preview should be deleted");

        cleanup_by_prefix(&pool, &prefix).await;
    }

    #[tokio::test]
    async fn test_round_trip_all_option_fields_none() {
        let pool = test_pool().await;
        let storage = UrlPreviewStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string();
        let url = format!("https://example.com/test-minimal-{suffix}");
        let prefix = url.clone();

        cleanup_by_prefix(&pool, &prefix).await;

        let minimal = UrlPreviewCache {
            url: url.clone(),
            title: None,
            description: None,
            og_title: None,
            og_image: None,
            og_image_width: None,
            og_image_height: None,
            og_site_name: None,
            og_type: None,
            created_ts: BASE_TS,
            expires_at: BASE_TS + ONE_HOUR_MS,
        };
        storage.save_preview(&minimal).await.expect("save should succeed");

        let found = storage
            .get_cached_preview(&url, BASE_TS + 1000)
            .await
            .expect("get should succeed")
            .expect("preview should be found");

        assert_eq!(found.url, url);
        assert!(found.title.is_none());
        assert!(found.description.is_none());
        assert!(found.og_title.is_none());
        assert!(found.og_image.is_none());
        assert!(found.og_image_width.is_none());
        assert!(found.og_image_height.is_none());
        assert!(found.og_site_name.is_none());
        assert!(found.og_type.is_none());
        assert_eq!(found.created_ts, BASE_TS);
        assert_eq!(found.expires_at, BASE_TS + ONE_HOUR_MS);

        cleanup_by_prefix(&pool, &prefix).await;
    }
}
