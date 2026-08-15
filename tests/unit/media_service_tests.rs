#![cfg(test)]

use std::fs;
use tempfile::tempdir;
use tokio::runtime::Runtime;

use synapse_services::media_service::MediaService;

fn create_test_media_service() -> (MediaService, tempfile::TempDir) {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let media_path = temp_dir.path().to_str().expect("Invalid path");
    let media_service = MediaService::new(media_path, None, "test.local");
    (media_service, temp_dir)
}

// ---------------------------------------------------------------------------
// S3: Streaming media download tests
// ---------------------------------------------------------------------------

#[test]
fn test_get_media_file_path_returns_correct_path() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let (media_service, _temp_dir) = create_test_media_service();
        let content = create_test_image_data();

        let upload_result = media_service.upload_media("@alice:example.com", &content, "image/png", None).await;
        assert!(upload_result.is_ok(), "Upload should succeed");

        let media_id = upload_result.unwrap()["media_id"].as_str().unwrap().to_string();

        let file_path = media_service.get_media_file_path("test.local", &media_id).await;

        assert!(file_path.is_some(), "get_media_file_path should return Some for existing media");

        let path = file_path.unwrap();
        assert!(path.exists(), "File at returned path should exist");

        let file_content = std::fs::read(&path).unwrap();
        assert_eq!(file_content, content, "File content should match uploaded content");
    });
}

#[test]
fn test_get_media_file_path_returns_none_for_nonexistent() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let (media_service, _temp_dir) = create_test_media_service();

        let file_path = media_service.get_media_file_path("test.local", "nonexistent_media_id").await;

        assert!(file_path.is_none(), "get_media_file_path should return None for non-existent media");
    });
}

#[test]
fn test_get_media_file_path_validates_media_id() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let (media_service, _temp_dir) = create_test_media_service();

        // Invalid media_id with path traversal characters should return None
        let file_path = media_service.get_media_file_path("test.local", "../etc/passwd").await;
        assert!(file_path.is_none(), "get_media_file_path should reject path traversal in media_id");
    });
}

#[test]
fn test_get_media_file_path_large_file() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let (media_service, _temp_dir) = create_test_media_service();
        let content: Vec<u8> = vec![0xAB; 5 * 1024 * 1024]; // 5 MB

        let upload_result = media_service.upload_media("@alice:example.com", &content, "image/png", None).await;
        assert!(upload_result.is_ok(), "Upload should succeed");

        let media_id = upload_result.unwrap()["media_id"].as_str().unwrap().to_string();

        let file_path = media_service.get_media_file_path("test.local", &media_id).await;
        assert!(file_path.is_some(), "get_media_file_path should return Some for large file");

        let path = file_path.unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        assert_eq!(metadata.len(), 5 * 1024 * 1024, "File size should match uploaded content size");
    });
}

fn create_test_image_data() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00,
        0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00,
        0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x18,
        0xDD, 0x8D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

#[test]
fn test_media_service_creation() {
    let (_media_service, _temp_dir) = create_test_media_service();
    // media_path is private, skip assertion
}

#[test]
fn test_upload_media_png() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let (media_service, _temp_dir) = create_test_media_service();
        let content = create_test_image_data();

        let result = media_service.upload_media("@alice:example.com", &content, "image/png", None).await;

        assert!(result.is_ok(), "Failed to upload media");

        let metadata = result.unwrap();
        assert!(metadata.get("content_uri").is_some());
        assert!(metadata.get("media_id").is_some());

        let content_uri = metadata["content_uri"].as_str().unwrap();
        assert!(content_uri.starts_with("mxc://test.local/"));

        let media_id = metadata["media_id"].as_str().unwrap();
        assert!(!media_id.is_empty());
    });
}

#[test]
fn test_upload_media_jpeg() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let (media_service, _temp_dir) = create_test_media_service();
        let content = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00];

        let result = media_service.upload_media("@alice:example.com", &content, "image/jpeg", None).await;

        assert!(result.is_ok(), "Failed to upload JPEG media");

        let metadata = result.unwrap();
        let content_uri = metadata["content_uri"].as_str().unwrap();
        assert!(content_uri.starts_with("mxc://test.local/"));
    });
}

#[test]
fn test_upload_media_with_filename() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let (media_service, _temp_dir) = create_test_media_service();
        let content = create_test_image_data();

        let result =
            media_service.upload_media("@alice:example.com", &content, "image/png", Some("test_image.png")).await;

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
        let (media_service, _temp_dir) = create_test_media_service();
        let content = create_test_image_data();

        let result = media_service.upload_media("@alice:example.com", &content, "image/png", None).await;

        assert!(result.is_ok());

        let metadata = result.unwrap();
        let media_id = metadata["media_id"].as_str().unwrap();

        // The file is stored as media_id.png in the media directory.
        // Use get_media_file_path to find the actual file.
        let file_path = media_service.get_media_file_path("test.local", media_id).await;
        assert!(file_path.is_some(), "Media file should be created on disk");

        let file_content = fs::read(file_path.unwrap()).expect("Failed to read file");
        assert_eq!(file_content, content);
    });
}

#[test]
fn test_get_media_success() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let (media_service, _temp_dir) = create_test_media_service();
        let content = create_test_image_data();

        let upload_result =
            media_service.upload_media("@alice:example.com", &content, "image/png", None).await.unwrap();

        let media_id = upload_result["media_id"].as_str().unwrap();

        let retrieved_content = media_service.get_media("example.com", media_id).await;

        assert!(retrieved_content.is_some(), "Should retrieve uploaded media");

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

        let upload_result =
            media_service.upload_media("@alice:example.com", &content, "image/png", None).await.unwrap();

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

        let result = media_service.download_media("example.com", "nonexistent_id").await;

        assert!(result.is_err(), "Should return error for non-existent media");

        let error = result.unwrap_err();
        assert!(error.is_not_found());
    });
}

#[test]
fn test_get_thumbnail_success() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let (media_service, _temp_dir) = create_test_media_service();
        let content = create_test_image_data();

        let upload_result =
            media_service.upload_media("@alice:example.com", &content, "image/png", None).await.unwrap();

        let media_id = upload_result["media_id"].as_str().unwrap();

        let result = media_service.get_thumbnail("example.com", media_id, 100, 100, "scale").await;

        assert!(result.is_ok(), "Should get thumbnail");

        assert_eq!(result.unwrap(), content);
    });
}

#[test]
fn test_get_thumbnail_not_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let (media_service, _temp_dir) = create_test_media_service();

        let result = media_service.get_thumbnail("example.com", "nonexistent_id", 100, 100, "scale").await;

        assert!(result.is_err(), "Should return error for non-existent media");

        let error = result.unwrap_err();
        assert!(error.is_not_found());
    });
}

#[test]
fn test_get_media_metadata_success() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let (media_service, _temp_dir) = create_test_media_service();
        let content = create_test_image_data();

        let upload_result =
            media_service.upload_media("@alice:example.com", &content, "image/png", None).await.unwrap();

        let media_id = upload_result["media_id"].as_str().unwrap();

        let result = media_service.get_media_metadata("example.com", media_id).await;

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

        let result = media_service.get_media_metadata("example.com", "nonexistent_id").await;

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

        let result1 = media_service.upload_media("@alice:example.com", &content1, "image/png", None).await;
        let result2 = media_service.upload_media("@alice:example.com", &content2, "image/jpeg", None).await;
        let result3 = media_service.upload_media("@alice:example.com", &content3, "image/gif", None).await;

        assert!(result1.is_ok());
        assert!(result2.is_ok());
        assert!(result3.is_ok());

        let metadata1 = result1.unwrap();
        let media_id1 = metadata1["media_id"].as_str().unwrap();
        let metadata2 = result2.unwrap();
        let media_id2 = metadata2["media_id"].as_str().unwrap();
        let metadata3 = result3.unwrap();
        let media_id3 = metadata3["media_id"].as_str().unwrap();

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
fn test_upload_empty_content() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let (media_service, _temp_dir) = create_test_media_service();
        let content: Vec<u8> = vec![];

        let result = media_service.upload_media("@alice:example.com", &content, "image/png", None).await;

        assert!(result.is_ok(), "Should upload empty content");

        let metadata = result.unwrap();
        assert!(metadata.get("content_uri").is_some());
        assert!(metadata.get("media_id").is_some());
    });
}

#[test]
fn test_upload_large_content() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let (media_service, _temp_dir) = create_test_media_service();
        let content: Vec<u8> = vec![0xFF; 1024 * 1024];

        let result = media_service.upload_media("@alice:example.com", &content, "image/png", None).await;

        assert!(result.is_ok(), "Should upload large content");

        let metadata = result.unwrap();
        let media_id = metadata["media_id"].as_str().unwrap();

        // Verify the file was stored with the correct size.
        let file_path = media_service.get_media_file_path("test.local", media_id).await;
        assert!(file_path.is_some(), "Large media file should be created on disk");

        let file_metadata = std::fs::metadata(file_path.unwrap()).expect("Failed to read file metadata");
        assert_eq!(file_metadata.len(), 1024 * 1024);
    });
}
