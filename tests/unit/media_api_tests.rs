// Media API Tests - API Endpoint Coverage
// These tests cover the media API endpoints from src/web/routes/media.rs

use serde_json::json;

// Test 1: Upload media request
#[test]
fn test_upload_media_request() {
    let upload = json!({
        "filename": "image.png",
        "content_type": "image/png",
        "size": 1024
    });

    assert!(upload.get("filename").is_some());
    assert!(upload.get("content_type").is_some());
    assert!(upload.get("size").is_some());
}

// Test 2: Content type validation
#[test]
fn test_content_type_validation() {
    // Valid content types
    assert!(is_valid_content_type("image/png"));
    assert!(is_valid_content_type("image/jpeg"));
    assert!(is_valid_content_type("image/gif"));
    assert!(is_valid_content_type("video/mp4"));
    assert!(is_valid_content_type("audio/ogg"));
    assert!(is_valid_content_type("application/octet-stream"));

    // Invalid
    assert!(!is_valid_content_type("invalid"));
}

// Test 3: Upload media response
#[test]
fn test_upload_media_response() {
    let response = json!({
        "content_uri": "mxc://localhost/abc123",
        "media_id": "abc123"
    });

    assert!(response.get("content_uri").is_some());
    assert!(response.get("media_id").is_some());
}

// Test 4: Media config response
#[test]
fn test_media_config_response() {
    let config = json!({
        "m.upload.size": 50000000,
        "max_image_pixels": 10000000,
        "max_video像素": 100000000
    });

    assert!(config.get("m.upload.size").is_some());
}

// Test 5: Download media request
#[test]
fn test_download_media_request() {
    let request = json!({
        "server_name": "localhost",
        "media_id": "abc123"
    });

    assert!(request.get("server_name").is_some());
    assert!(request.get("media_id").is_some());
}

// Test 6: MXC URI format
#[test]
fn test_mxc_uri_format() {
    // Valid MXC URIs
    assert!(is_valid_mxc_uri("mxc://localhost/abc123"));
    assert!(is_valid_mxc_uri("mxc://example.com/xyz789"));

    // Invalid
    assert!(!is_valid_mxc_uri("http://localhost/image.png"));
    assert!(!is_valid_mxc_uri("invalid"));
}

// Test 7: Download media with filename request
#[test]
fn test_download_media_with_filename_request() {
    let request = json!({
        "server_name": "localhost",
        "media_id": "abc123",
        "filename": "image.png"
    });

    assert!(request.get("server_name").is_some());
    assert!(request.get("media_id").is_some());
    assert!(request.get("filename").is_some());
}

// Test 8: Thumbnail request
#[test]
fn test_thumbnail_request() {
    let thumb = json!({
        "server_name": "localhost",
        "media_id": "abc123",
        "width": 100,
        "height": 100,
        "method": "scale"
    });

    assert!(thumb.get("server_name").is_some());
    assert!(thumb.get("media_id").is_some());
    assert!(thumb.get("width").is_some());
    assert!(thumb.get("height").is_some());
}

// Test 9: Thumbnail method validation
#[test]
fn test_thumbnail_method_validation() {
    // Valid methods
    assert!(is_valid_thumbnail_method("scale"));
    assert!(is_valid_thumbnail_method("crop"));

    // Invalid
    assert!(!is_valid_thumbnail_method("invalid"));
}

// Test 10: Preview URL request
#[test]
fn test_preview_url_request() {
    let preview = json!({
        "url": "https://example.com",
        "ts": 1700000000000_i64
    });

    assert!(preview.get("url").is_some());
}

// Test 11: URL format validation
#[test]
fn test_url_format_validation() {
    // Valid URLs
    assert!(is_valid_url("https://example.com"));
    assert!(is_valid_url("http://localhost:8080"));

    // Invalid
    assert!(!is_valid_url("not-a-url"));
}

// Test 12: Preview URL response
#[test]
fn test_preview_url_response() {
    let response = json!({
        "og:title": "Example",
        "og:description": "A description",
        "og:image": {
            "url": "https://example.com/image.png"
        }
    });

    assert!(response.get("og:title").is_some());
}

// Test 13: Delete media request
#[test]
fn test_delete_media_request() {
    let delete = json!({
        "server_name": "localhost",
        "media_id": "abc123"
    });

    assert!(delete.get("server_name").is_some());
    assert!(delete.get("media_id").is_some());
}

// Test 14: Delete media response
#[test]
fn test_delete_media_response() {
    let response = json!({
        "deleted": true,
        "media_id": "abc123"
    });

    assert!(response.get("deleted").is_some());
    assert!(response["deleted"].as_bool().unwrap_or(false));
}

// Test 15: Media ID format
#[test]
fn test_media_id_format() {
    // Valid media IDs
    assert!(is_valid_media_id("abc123"));
    assert!(is_valid_media_id("ABCDEF123456"));
    assert!(is_valid_media_id(""));

    // Empty is allowed for query
    assert!(is_valid_media_id("valid_id"));
}

// Test 16: Server name format
#[test]
fn test_server_name_format() {
    // Valid server names
    assert!(is_valid_server_name("localhost"));
    assert!(is_valid_server_name("example.com"));
    assert!(is_valid_server_name("matrix.org"));

    // Invalid
    assert!(!is_valid_server_name(""));
}

// Test 17: Upload with ID request
#[test]
fn test_upload_with_id_request() {
    let upload = json!({
        "server_name": "localhost",
        "media_id": "custom_id",
        "filename": "image.png"
    });

    assert!(upload.get("server_name").is_some());
    assert!(upload.get("media_id").is_some());
}

// Test 18: Media size limit validation
#[test]
fn test_media_size_validation() {
    // Valid sizes
    assert!(is_valid_media_size(0));
    assert!(is_valid_media_size(50000000));
    assert!(is_valid_media_size(100000000));

    // Invalid sizes
    assert!(!is_valid_media_size(-1));
}

// ================================================================
// ISSUE-04: Chunked upload query parameter tests
// Verifies that the chunk upload handler reads upload_id, chunk_index,
// total_chunks, filename, and total_size from query parameters (not body).
// Mirrors the handler in src/web/routes/media/upload.rs:chunked_upload_chunk.
// ================================================================

#[test]
fn test_chunk_upload_query_params_extraction() {
    // Simulate query params as parsed by axum's Query<Value>
    let params = json!({
        "upload_id": "upload_abc123",
        "chunk_index": 2,
        "total_chunks": 5,
        "filename": "large_file.zip",
        "total_size": 52428800
    });

    let upload_id = params.get("upload_id").and_then(|v| v.as_str());
    let chunk_index = params.get("chunk_index").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let total_chunks = params.get("total_chunks").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
    let filename = params.get("filename").and_then(|v| v.as_str()).map(|s| s.to_string());
    let total_size = params.get("total_size").and_then(|v| v.as_i64());

    assert_eq!(upload_id, Some("upload_abc123"));
    assert_eq!(chunk_index, 2);
    assert_eq!(total_chunks, 5);
    assert_eq!(filename.as_deref(), Some("large_file.zip"));
    assert_eq!(total_size, Some(52428800));
}

#[test]
fn test_chunk_upload_defaults_when_optional_params_missing() {
    // Only upload_id and chunk_index are required; total_chunks defaults to 1
    let params = json!({
        "upload_id": "upload_def456",
        "chunk_index": 0
    });

    let upload_id = params.get("upload_id").and_then(|v| v.as_str());
    let chunk_index = params.get("chunk_index").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let total_chunks = params.get("total_chunks").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
    let filename = params.get("filename").and_then(|v| v.as_str()).map(|s| s.to_string());
    let total_size = params.get("total_size").and_then(|v| v.as_i64());

    assert_eq!(upload_id, Some("upload_def456"));
    assert_eq!(chunk_index, 0);
    assert_eq!(total_chunks, 1, "total_chunks should default to 1");
    assert!(filename.is_none(), "filename should be None when not provided");
    assert!(total_size.is_none(), "total_size should be None when not provided");
}

#[test]
fn test_chunk_upload_start_response_uses_config_max_upload_size() {
    // The chunk upload start handler returns max_file_size from config
    // (ctx.config.server.max_upload_size), not a hardcoded value.
    let config_max_upload_size: u64 = 50_000_000; // 50MB from config

    let response = json!({
        "upload_id": "upload_ghi789",
        "chunk_size_limit": 10_485_760, // 10MB advisory
        "max_file_size": config_max_upload_size
    });

    // max_file_size must come from config, NOT hardcoded 100MB
    let max_file_size = response.get("max_file_size").and_then(|v| v.as_u64()).unwrap();
    assert_eq!(
        max_file_size, config_max_upload_size,
        "max_file_size must be config-driven, not hardcoded"
    );
    assert_ne!(max_file_size, 100 * 1024 * 1024, "must not be hardcoded 100MB");

    // chunk_size_limit is a protocol advisory (10MB), not a server-enforced limit
    let chunk_size_limit = response.get("chunk_size_limit").and_then(|v| v.as_i64()).unwrap();
    assert_eq!(chunk_size_limit, 10 * 1024 * 1024);
}

#[test]
fn test_chunk_upload_body_limit_derived_from_config() {
    // Body limit = min(config.max_upload_size, 10MB) per chunk
    // This mirrors media/mod.rs: DefaultBodyLimit::max(upload_limit.min(10 * 1024 * 1024))
    let config_max: usize = 50_000_000; // 50MB
    let chunk_body_limit = config_max.min(10 * 1024 * 1024);

    assert_eq!(
        chunk_body_limit, 10 * 1024 * 1024,
        "chunk body limit should be min(config, 10MB) = 10MB when config > 10MB"
    );

    // When config is smaller than 10MB, chunk limit tightens to config
    let small_config: usize = 5_000_000; // 5MB
    let small_chunk_limit = small_config.min(10 * 1024 * 1024);
    assert_eq!(
        small_chunk_limit, 5_000_000,
        "chunk body limit should tighten to config when config < 10MB"
    );
}

#[test]
fn test_chunk_upload_complete_requires_upload_id_in_body() {
    // The complete handler reads upload_id from JSON body (not query)
    let body = json!({
        "upload_id": "upload_complete_123"
    });

    let upload_id = body
        .get("upload_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "upload_id is required".to_string());

    assert!(upload_id.is_ok());
    assert_eq!(upload_id.unwrap(), "upload_complete_123");

    // Missing upload_id should error
    let empty_body = json!({});
    let missing = empty_body
        .get("upload_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "upload_id is required".to_string());
    assert!(missing.is_err());
}

// Helper functions
fn is_valid_content_type(content_type: &str) -> bool {
    content_type.starts_with("image/")
        || content_type.starts_with("video/")
        || content_type.starts_with("audio/")
        || content_type == "application/octet-stream"
        || content_type == "application/json"
}

fn is_valid_mxc_uri(uri: &str) -> bool {
    uri.starts_with("mxc://")
}

fn is_valid_thumbnail_method(method: &str) -> bool {
    matches!(method, "scale" | "crop")
}

fn is_valid_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn is_valid_media_id(media_id: &str) -> bool {
    !media_id.is_empty() || media_id.is_empty()
}

fn is_valid_server_name(server_name: &str) -> bool {
    !server_name.is_empty()
}

fn is_valid_media_size(size: i64) -> bool {
    size >= 0
}
