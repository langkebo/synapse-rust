use super::backend::MediaStorageBackend;
use super::models::*;
use async_trait::async_trait;
use synapse_common::error::ApiError;

pub struct S3Backend {
    config: S3Config,
    _client: Option<()>,
}

impl S3Backend {
    pub fn new(config: S3Config) -> Self {
        Self { config, _client: None }
    }

    pub(crate) fn object_key(&self, media_id: &str) -> String {
        if let Some(ref prefix) = self.config.prefix {
            format!("{prefix}/{media_id}")
        } else {
            media_id.to_string()
        }
    }

    pub(crate) fn thumbnail_key(&self, media_id: &str, width: u32, height: u32, method: &str) -> String {
        let base = self.object_key(media_id);
        format!("thumbnails/{base}_{width}x{height}_{method}.jpg")
    }
}

#[async_trait]
impl MediaStorageBackend for S3Backend {
    async fn store(&self, media_id: &str, data: &[u8], content_type: &str) -> Result<(), ApiError> {
        let _key = self.object_key(media_id);

        tracing::info!(
            "S3 store: bucket={}, key={}, size={}, content_type={}",
            self.config.bucket,
            _key,
            data.len(),
            content_type
        );

        Err(ApiError::internal(
            "S3 backend not fully implemented. Please use filesystem backend or implement S3 client integration."
                .to_string(),
        ))
    }

    async fn retrieve(&self, media_id: &str) -> Result<Option<Vec<u8>>, ApiError> {
        let _key = self.object_key(media_id);

        tracing::info!("S3 retrieve: bucket={}, key={}", self.config.bucket, _key);

        Err(ApiError::internal("S3 backend not fully implemented".to_string()))
    }

    async fn delete(&self, media_id: &str) -> Result<bool, ApiError> {
        let _key = self.object_key(media_id);

        tracing::info!("S3 delete: bucket={}, key={}", self.config.bucket, _key);

        Err(ApiError::internal("S3 backend not fully implemented".to_string()))
    }

    async fn exists(&self, media_id: &str) -> Result<bool, ApiError> {
        let _key = self.object_key(media_id);

        tracing::info!("S3 exists: bucket={}, key={}", self.config.bucket, _key);

        Err(ApiError::internal("S3 backend not fully implemented".to_string()))
    }

    async fn get_size(&self, media_id: &str) -> Result<Option<u64>, ApiError> {
        let _key = self.object_key(media_id);

        tracing::info!("S3 get_size: bucket={}, key={}", self.config.bucket, _key);

        Err(ApiError::internal("S3 backend not fully implemented".to_string()))
    }

    async fn store_thumbnail(
        &self,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
        data: &[u8],
    ) -> Result<(), ApiError> {
        let _key = self.thumbnail_key(media_id, width, height, method);

        tracing::info!("S3 store_thumbnail: bucket={}, key={}, size={}", self.config.bucket, _key, data.len());

        Err(ApiError::internal("S3 backend not fully implemented".to_string()))
    }

    async fn retrieve_thumbnail(
        &self,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
    ) -> Result<Option<Vec<u8>>, ApiError> {
        let _key = self.thumbnail_key(media_id, width, height, method);

        tracing::info!("S3 retrieve_thumbnail: bucket={}, key={}", self.config.bucket, _key);

        Err(ApiError::internal("S3 backend not fully implemented".to_string()))
    }

    async fn delete_thumbnails(&self, media_id: &str) -> Result<u64, ApiError> {
        let _prefix = format!("thumbnails/{}/", self.object_key(media_id));

        tracing::info!("S3 delete_thumbnails: bucket={}, prefix={}", self.config.bucket, _prefix);

        Err(ApiError::internal("S3 backend not fully implemented".to_string()))
    }

    async fn get_stats(&self) -> Result<MediaStorageStats, ApiError> {
        Err(ApiError::internal("S3 backend not fully implemented".to_string()))
    }

    async fn health_check(&self) -> Result<bool, ApiError> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_backend(prefix: Option<&str>) -> S3Backend {
        S3Backend::new(S3Config {
            bucket: "test-bucket".into(),
            region: "us-east-1".into(),
            endpoint_url: None,
            access_key_id: "key".into(),
            secret_access_key: "secret".into(),
            prefix: prefix.map(|p| p.to_string()),
            use_path_style: false,
        })
    }

    #[test]
    fn object_key_without_prefix() {
        let backend = make_backend(None);
        assert_eq!(backend.object_key("abc123"), "abc123");
    }

    #[test]
    fn object_key_with_prefix() {
        let backend = make_backend(Some("media"));
        assert_eq!(backend.object_key("abc123"), "media/abc123");
    }

    #[test]
    fn object_key_nested_prefix() {
        let backend = make_backend(Some("uploads/2024"));
        assert_eq!(backend.object_key("xyz"), "uploads/2024/xyz");
    }

    #[test]
    fn thumbnail_key_builds_correctly() {
        let backend = make_backend(None);
        assert_eq!(backend.thumbnail_key("abc123", 100, 200, "crop"), "thumbnails/abc123_100x200_crop.jpg");
    }

    #[test]
    fn thumbnail_key_with_prefix() {
        let backend = make_backend(Some("media"));
        assert_eq!(backend.thumbnail_key("abc123", 50, 50, "scale"), "thumbnails/media/abc123_50x50_scale.jpg");
    }

    #[test]
    fn thumbnail_key_different_methods() {
        let backend = make_backend(None);
        let key = backend.thumbnail_key("img", 300, 200, "stretch");
        assert!(key.ends_with(".jpg"));
        assert!(key.starts_with("thumbnails/"));
        assert!(key.contains("_300x200_stretch"));
    }
}
