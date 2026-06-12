pub use synapse_services::media_service::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thumbnail_method_from_str() {
        assert_eq!(ThumbnailMethod::from_str("crop").unwrap(), ThumbnailMethod::Crop);
        assert_eq!(ThumbnailMethod::from_str("CROP").unwrap(), ThumbnailMethod::Crop);
        assert_eq!(ThumbnailMethod::from_str("scale").unwrap(), ThumbnailMethod::Scale);
        assert_eq!(ThumbnailMethod::from_str("SCALE").unwrap(), ThumbnailMethod::Scale);
        assert!(ThumbnailMethod::from_str("invalid").is_err());
    }

    #[test]
    fn test_thumbnail_config_default() {
        let config = ThumbnailSettings::default();
        assert_eq!(config.width, 800);
        assert_eq!(config.height, 600);
        assert_eq!(config.method, ThumbnailMethod::Scale);
        assert_eq!(config.quality, 80);
    }

    #[test]
    fn test_thumbnail_method_equality() {
        assert_eq!(ThumbnailMethod::Crop, ThumbnailMethod::Crop);
        assert_eq!(ThumbnailMethod::Scale, ThumbnailMethod::Scale);
        assert_ne!(ThumbnailMethod::Crop, ThumbnailMethod::Scale);
    }

    #[test]
    fn test_media_service_creation() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();

        let service = MediaService::new(media_path, None, "test.server");

        assert!(service.media_path.exists());
        assert!(service.thumbnail_path.exists());
        assert!(service.task_queue.is_none());
        assert_eq!(service.default_thumbnail_configs.len(), 5);
    }

    #[test]
    fn test_media_service_task_queue_field() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();

        let service = MediaService::new(media_path, None, "test.server");
        assert!(service.task_queue.is_none());
    }

    #[test]
    fn test_get_extension_from_content_type() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();
        let _service = MediaService::new(media_path, None, "test.server");

        assert_eq!(MediaService::get_extension_from_content_type("image/jpeg"), "jpg");
        assert_eq!(MediaService::get_extension_from_content_type("image/png"), "png");
        assert_eq!(MediaService::get_extension_from_content_type("image/gif"), "gif");
        assert_eq!(MediaService::get_extension_from_content_type("application/pdf"), "pdf");
        assert_eq!(MediaService::get_extension_from_content_type("text/plain"), "txt");
        assert_eq!(MediaService::get_extension_from_content_type("unknown/type"), "bin");
        assert_eq!(MediaService::get_extension_from_content_type(""), "bin");
    }

    #[test]
    fn test_thumbnail_method_error_message() {
        let result = ThumbnailMethod::from_str("invalid_method");
        assert!(result.is_err());

        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("Invalid thumbnail method"));
        assert!(error_msg.contains("invalid_method"));
    }

    #[test]
    fn test_thumbnail_config_custom() {
        let config = ThumbnailSettings { width: 1024, height: 768, method: ThumbnailMethod::Crop, quality: 90 };

        assert_eq!(config.width, 1024);
        assert_eq!(config.height, 768);
        assert_eq!(config.method, ThumbnailMethod::Crop);
        assert_eq!(config.quality, 90);
    }

    #[test]
    fn test_media_service_default_thumbnail_configs() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();
        let service = MediaService::new(media_path, None, "test.server");

        let configs = &service.default_thumbnail_configs;

        assert_eq!(configs.len(), 5);

        assert_eq!(configs[0].width, 32);
        assert_eq!(configs[0].height, 32);
        assert_eq!(configs[0].method, ThumbnailMethod::Crop);

        assert_eq!(configs[4].width, 800);
        assert_eq!(configs[4].height, 600);
        assert_eq!(configs[4].method, ThumbnailMethod::Scale);
    }

    #[test]
    fn test_thumbnail_method_case_insensitive() {
        assert!(ThumbnailMethod::from_str("crop").is_ok());
        assert!(ThumbnailMethod::from_str("CROP").is_ok());
        assert!(ThumbnailMethod::from_str("Crop").is_ok());
        assert!(ThumbnailMethod::from_str("CrOp").is_ok());

        assert!(ThumbnailMethod::from_str("scale").is_ok());
        assert!(ThumbnailMethod::from_str("SCALE").is_ok());
        assert!(ThumbnailMethod::from_str("Scale").is_ok());
        assert!(ThumbnailMethod::from_str("ScAlE").is_ok());
    }

    #[test]
    fn test_thumbnail_config_quality_range() {
        let valid_qualities: Vec<u8> = vec![1, 50, 80, 100];
        for quality in valid_qualities {
            let config = ThumbnailSettings { width: 100, height: 100, method: ThumbnailMethod::Scale, quality };
            assert!(config.quality > 0);
        }
    }

    #[test]
    fn test_thumbnail_config_dimension_boundaries() {
        let config = ThumbnailSettings { width: 1, height: 1, method: ThumbnailMethod::Scale, quality: 80 };
        assert_eq!(config.width, 1);
        assert_eq!(config.height, 1);

        let config_large =
            ThumbnailSettings { width: 10000, height: 10000, method: ThumbnailMethod::Crop, quality: 80 };
        assert_eq!(config_large.width, 10000);
        assert_eq!(config_large.height, 10000);
    }

    #[tokio::test]
    async fn test_get_thumbnail_configurations() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();
        let service = MediaService::new(media_path, None, "test.server");

        let configs = service.get_thumbnail_configurations();
        assert_eq!(configs.len(), 5);
    }

    #[tokio::test]
    async fn test_async_content_type_validation() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();
        let _service = MediaService::new(media_path, None, "test.server");

        let test_cases = vec![
            ("image/jpeg", "jpg"),
            ("image/png", "png"),
            ("image/gif", "gif"),
            ("application/pdf", "pdf"),
            ("text/plain", "txt"),
            ("", "bin"),
            ("unknown/type", "bin"),
        ];

        for (content_type, expected_ext) in test_cases {
            let ext = MediaService::get_extension_from_content_type(content_type);
            assert_eq!(ext, expected_ext, "Failed for content type: {content_type}");
        }
    }

    #[tokio::test]
    async fn test_async_upload_different_types() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();
        let service = MediaService::new(media_path, None, "test.server");

        let content = b"test content";

        let result_jpeg = service.upload_media("@user:example.com", content, "image/jpeg", None).await;
        assert!(result_jpeg.is_ok());

        let result_png = service.upload_media("@user:example.com", content, "image/png", None).await;
        assert!(result_png.is_ok());

        let result_pdf = service.upload_media("@user:example.com", content, "application/pdf", None).await;
        assert!(result_pdf.is_ok());
    }

    #[tokio::test]
    async fn test_async_cleanup_empty_directory() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();
        let service = MediaService::new(media_path, None, "test.server");

        let result = service.cleanup_old_thumbnails(30).await;
        assert!(result.is_ok());
        let deleted = result.unwrap();
        assert_eq!(deleted, 0);
    }

    #[tokio::test]
    async fn test_async_preview_url_metadata() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();
        let service = MediaService::new(media_path, None, "test.server");

        let url = "https://example.com/test";
        let ts = 1234567890i64;

        let result = service.preview_url(url, ts);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["url"], url);
    }
}
