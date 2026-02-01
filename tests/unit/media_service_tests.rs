#[cfg(test)]
mod media_service_tests {
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use tokio::runtime::Runtime;

    use synapse_rust::services::media_service::MediaService;

    fn create_test_media_service() -> (MediaService, tempfile::TempDir) {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let media_path = temp_dir.path().to_str().expect("Invalid path");
        let media_service = MediaService::new(media_path);
        (media_service, temp_dir)
    }

    fn create_test_image_data() -> Vec<u8> {
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
            0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
            0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00,
            0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
            0x00, 0x03, 0x00, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB4, 0x00, 0x00, 0x00,
            0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ]
    }

    #[test]
    fn test_media_service_creation() {
        let (media_service, _temp_dir) = create_test_media_service();
        assert_eq!(
            media_service.media_path,
            PathBuf::from(_temp_dir.path())
        );
    }

    #[test]
    fn test_upload_media_png() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (media_service, _temp_dir) = create_test_media_service();
            let content = create_test_image_data();

            let result = media_service
                .upload_media("@alice:example.com", &content, "image/png", None)
                .await;

            assert!(result.is_ok(), "Failed to upload media");

            let metadata = result.unwrap();
            assert!(metadata.get("content_uri").is_some());
            assert!(metadata.get("content_type").is_some());
            assert!(metadata.get("size").is_some());
            assert!(metadata.get("media_id").is_some());

            let content_uri = metadata["content_uri"].as_str().unwrap();
            assert!(content_uri.starts_with("/_matrix/media/v3/download/"));
            assert!(content_uri.ends_with(".png"));

            let size = metadata["size"].as_i64().unwrap();
            assert_eq!(size, content.len() as i64);
        });
    }

    #[test]
    fn test_upload_media_jpeg() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (media_service, _temp_dir) = create_test_media_service();
            let content = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00];

            let result = media_service
                .upload_media("@alice:example.com", &content, "image/jpeg", None)
                .await;

            assert!(result.is_ok(), "Failed to upload JPEG media");

            let metadata = result.unwrap();
            let content_uri = metadata["content_uri"].as_str().unwrap();
            assert!(content_uri.ends_with(".jpg"));
        });
    }

    #[test]
    fn test_upload_media_with_filename() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (media_service, _temp_dir) = create_test_media_service();
            let content = create_test_image_data();

            let result = media_service
                .upload_media(
                    "@alice:example.com",
                    &content,
                    "image/png",
                    Some("test_image.png"),
                )
                .await;

            assert!(result.is_ok(), "Failed to upload media with filename");

            let metadata = result.unwrap();
            let media_id = metadata["media_id"].as_str().unwrap();
            assert!(!media_id.is_empty());
        });
    }

    #[test]
    fn test_upload_media_creates_file() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (media_service, temp_dir) = create_test_media_service();
            let content = create_test_image_data();

            let result = media_service
                .upload_media("@alice:example.com", &content, "image/png", None)
                .await;

            assert!(result.is_ok());

            let metadata = result.unwrap();
            let content_uri = metadata["content_uri"].as_str().unwrap();
            let filename = content_uri.split('/').last().unwrap();

            let file_path = temp_dir.path().join(filename);
            assert!(
                file_path.exists(),
                "Media file should be created on disk"
            );

            let file_content = fs::read(&file_path).expect("Failed to read file");
            assert_eq!(file_content, content);
        });
    }

    #[test]
    fn test_get_media_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (media_service, _temp_dir) = create_test_media_service();
            let content = create_test_image_data();

            let upload_result = media_service
                .upload_media("@alice:example.com", &content, "image/png", None)
                .await
                .unwrap();

            let media_id = upload_result["media_id"].as_str().unwrap();

            let retrieved_content = media_service
                .get_media("example.com", media_id)
                .await;

            assert!(
                retrieved_content.is_some(),
                "Should retrieve uploaded media"
            );

            assert_eq!(retrieved_content.unwrap(), content);
        });
    }

    #[test]
    fn test_get_media_not_found() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (media_service, _temp_dir) = create_test_media_service();

            let result = media_service.get_media("example.com", "nonexistent_id").await;

            assert!(result.is_none(), "Should return None for non-existent media");
        });
    }

    #[test]
    fn test_download_media_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (media_service, _temp_dir) = create_test_media_service();
            let content = create_test_image_data();

            let upload_result = media_service
                .upload_media("@alice:example.com", &content, "image/png", None)
                .await
                .unwrap();

            let media_id = upload_result["media_id"].as_str().unwrap();

            let result = media_service.download_media("example.com", media_id).await;

            assert!(result.is_ok(), "Should download uploaded media");

            assert_eq!(result.unwrap(), content);
        });
    }

    #[test]
    fn test_download_media_not_found() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (media_service, _temp_dir) = create_test_media_service();

            let result = media_service
                .download_media("example.com", "nonexistent_id")
                .await;

            assert!(result.is_err(), "Should return error for non-existent media");

            let error = result.unwrap_err();
            assert_eq!(error.status_code(), 404);
        });
    }

    #[test]
    fn test_get_thumbnail_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (media_service, _temp_dir) = create_test_media_service();
            let content = create_test_image_data();

            let upload_result = media_service
                .upload_media("@alice:example.com", &content, "image/png", None)
                .await
                .unwrap();

            let media_id = upload_result["media_id"].as_str().unwrap();

            let result = media_service
                .get_thumbnail("example.com", media_id, 100, 100, "scale")
                .await;

            assert!(result.is_ok(), "Should get thumbnail");

            assert_eq!(result.unwrap(), content);
        });
    }

    #[test]
    fn test_get_thumbnail_not_found() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (media_service, _temp_dir) = create_test_media_service();

            let result = media_service
                .get_thumbnail("example.com", "nonexistent_id", 100, 100, "scale")
                .await;

            assert!(result.is_err(), "Should return error for non-existent media");

            let error = result.unwrap_err();
            assert_eq!(error.status_code(), 404);
        });
    }

    #[test]
    fn test_get_media_metadata_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (media_service, _temp_dir) = create_test_media_service();
            let content = create_test_image_data();

            let upload_result = media_service
                .upload_media("@alice:example.com", &content, "image/png", None)
                .await
                .unwrap();

            let media_id = upload_result["media_id"].as_str().unwrap();

            let result = media_service
                .get_media_metadata("example.com", media_id)
                .await;

            assert!(result.is_some(), "Should get media metadata");

            let metadata = result.unwrap();
            assert_eq!(metadata["media_id"], media_id);
            assert!(metadata.get("content_uri").is_some());
            assert!(metadata.get("filename").is_some());
        });
    }

    #[test]
    fn test_get_media_metadata_not_found() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (media_service, _temp_dir) = create_test_media_service();

            let result = media_service
                .get_media_metadata("example.com", "nonexistent_id")
                .await;

            assert!(result.is_none(), "Should return None for non-existent media");
        });
    }

    #[test]
    fn test_upload_multiple_media() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (media_service, _temp_dir) = create_test_media_service();

            let content1 = create_test_image_data();
            let content2 = vec![0xFF, 0xD8, 0xFF, 0xE0];
            let content3 = vec![0x47, 0x49, 0x46, 0x38, 0x39, 0x61];

            let result1 = media_service
                .upload_media("@alice:example.com", &content1, "image/png", None)
                .await;
            let result2 = media_service
                .upload_media("@alice:example.com", &content2, "image/jpeg", None)
                .await;
            let result3 = media_service
                .upload_media("@alice:example.com", &content3, "image/gif", None)
                .await;

            assert!(result1.is_ok());
            assert!(result2.is_ok());
            assert!(result3.is_ok());

            let media_id1 = result1.unwrap()["media_id"].as_str().unwrap();
            let media_id2 = result2.unwrap()["media_id"].as_str().unwrap();
            let media_id3 = result3.unwrap()["media_id"].as_str().unwrap();

            assert_ne!(media_id1, media_id2);
            assert_ne!(media_id2, media_id3);
            assert_ne!(media_id1, media_id3);

            let retrieved1 = media_service.get_media("example.com", media_id1).await;
            let retrieved2 = media_service.get_media("example.com", media_id2).await;
            let retrieved3 = media_service.get_media("example.com", media_id3).await;

            assert_eq!(retrieved1.unwrap(), content1);
            assert_eq!(retrieved2.unwrap(), content2);
            assert_eq!(retrieved3.unwrap(), content3);
        });
    }

    #[test]
    fn test_get_extension_from_content_type() {
        let media_service = MediaService::new("/tmp/test");

        assert_eq!(
            media_service.get_extension_from_content_type("image/png"),
            "png"
        );
        assert_eq!(
            media_service.get_extension_from_content_type("image/jpeg"),
            "jpg"
        );
        assert_eq!(
            media_service.get_extension_from_content_type("image/gif"),
            "gif"
        );
        assert_eq!(
            media_service.get_extension_from_content_type("image/webp"),
            "webp"
        );
        assert_eq!(
            media_service.get_extension_from_content_type("video/mp4"),
            "mp4"
        );
        assert_eq!(
            media_service.get_extension_from_content_type("video/webm"),
            "webm"
        );
        assert_eq!(
            media_service.get_extension_from_content_type("audio/mpeg"),
            "mp3"
        );
        assert_eq!(
            media_service.get_extension_from_content_type("audio/ogg"),
            "ogg"
        );
        assert_eq!(
            media_service.get_extension_from_content_type("unknown/type"),
            "bin"
        );
    }

    #[test]
    fn test_upload_empty_content() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (media_service, _temp_dir) = create_test_media_service();
            let content: Vec<u8> = vec![];

            let result = media_service
                .upload_media("@alice:example.com", &content, "image/png", None)
                .await;

            assert!(result.is_ok(), "Should upload empty content");

            let metadata = result.unwrap();
            let size = metadata["size"].as_i64().unwrap();
            assert_eq!(size, 0);
        });
    }

    #[test]
    fn test_upload_large_content() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (media_service, _temp_dir) = create_test_media_service();
            let content: Vec<u8> = vec![0xFF; 1024 * 1024];

            let result = media_service
                .upload_media("@alice:example.com", &content, "image/png", None)
                .await;

            assert!(result.is_ok(), "Should upload large content");

            let metadata = result.unwrap();
            let size = metadata["size"].as_i64().unwrap();
            assert_eq!(size, 1024 * 1024);
        });
    }
}
