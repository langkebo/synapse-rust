use crate::routes::context::FederationContext;
use crate::routes::extractors::{MediaId, ServerName};
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
};
use serde_json::Value;
use synapse_common::*;

fn validate_federation_media_server_name(ctx: &FederationContext, server_name: &str) -> Result<(), ApiError> {
    if server_name != ctx.server_name {
        return Err(ApiError::not_found("Media is not hosted on this server".to_string()));
    }

    Ok(())
}

fn parse_federation_query_i64(params: &Value, key: &str, default: i64) -> Result<i64, ApiError> {
    match params.get(key) {
        Some(Value::Number(value)) => {
            value.as_i64().ok_or_else(|| ApiError::bad_request(format!("Invalid '{key}' parameter")))
        }
        Some(Value::String(value)) => {
            value.parse::<i64>().map_err(|_| ApiError::bad_request(format!("Invalid '{key}' parameter")))
        }
        Some(_) => Err(ApiError::bad_request(format!("Invalid '{key}' parameter"))),
        None => Ok(default),
    }
}

/// See [`media_download`].
pub(super) async fn media_download(
    State(ctx): State<FederationContext>,
    Path((server_name, media_id)): Path<(ServerName, MediaId)>,
) -> Result<impl IntoResponse, ApiError> {
    validate_federation_media_server_name(&ctx, &server_name)?;

    let content = ctx.media_service.download_media(&server_name, &media_id).await?;
    let content_type = federation_guess_content_type(&media_id, &content).to_string();
    let headers = federation_media_response_headers(content_type, content.len());

    Ok((headers, content))
}

/// See [`media_thumbnail`].
pub(super) async fn media_thumbnail(
    State(ctx): State<FederationContext>,
    Path((server_name, media_id)): Path<(ServerName, MediaId)>,
    Query(params): Query<Value>,
) -> Result<impl IntoResponse, ApiError> {
    validate_federation_media_server_name(&ctx, &server_name)?;

    let width = parse_federation_query_i64(&params, "width", 100)?;
    let height = parse_federation_query_i64(&params, "height", 100)?;
    let method = params.get("method").and_then(|v| v.as_str()).unwrap_or("scale");

    const MAX_FEDERATION_THUMBNAIL_DIMENSION: i64 = 4096;
    if width < 1
        || height < 1
        || width > MAX_FEDERATION_THUMBNAIL_DIMENSION
        || height > MAX_FEDERATION_THUMBNAIL_DIMENSION
    {
        return Err(ApiError::bad_request(format!(
            "Thumbnail dimensions must be between 1 and {MAX_FEDERATION_THUMBNAIL_DIMENSION}"
        )));
    }

    let content = ctx.media_service.get_thumbnail(&server_name, &media_id, width as u32, height as u32, method).await?;
    let content_type = federation_guess_content_type(&media_id, &content).to_string();
    let headers = federation_media_response_headers(content_type, content.len());

    Ok((headers, content))
}

fn federation_media_response_headers(content_type: String, content_length: usize) -> [(String, String); 2] {
    [("Content-Type".to_string(), content_type), ("Content-Length".to_string(), content_length.to_string())]
}

fn federation_guess_content_type(filename: &str, data: &[u8]) -> &'static str {
    if let Some(kind) = infer::get(data) {
        return kind.mime_type();
    }

    let lower = filename.to_ascii_lowercase();

    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".mp4") {
        "video/mp4"
    } else if lower.ends_with(".webm") {
        "video/webm"
    } else if lower.ends_with(".ogg") {
        "audio/ogg"
    } else if lower.ends_with(".mp3") {
        "audio/mpeg"
    } else if lower.ends_with(".wav") {
        "audio/wav"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_federation_query_i64_with_integer_value() {
        let params = json!({ "width": 200 });
        assert_eq!(parse_federation_query_i64(&params, "width", 100).unwrap(), 200);
    }

    #[test]
    fn test_parse_federation_query_i64_with_string_value() {
        let params = json!({ "width": "300" });
        assert_eq!(parse_federation_query_i64(&params, "width", 100).unwrap(), 300);
    }

    #[test]
    fn test_parse_federation_query_i64_with_default_when_missing() {
        let params = json!({});
        assert_eq!(parse_federation_query_i64(&params, "width", 100).unwrap(), 100);
    }

    #[test]
    fn test_parse_federation_query_i64_rejects_non_number() {
        let params = json!({ "width": [1, 2] });
        let result = parse_federation_query_i64(&params, "width", 100);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().http_status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_parse_federation_query_i64_rejects_invalid_string() {
        let params = json!({ "width": "abc" });
        let result = parse_federation_query_i64(&params, "width", 100);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().http_status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_federation_media_response_headers_structure() {
        let headers = federation_media_response_headers("image/png".to_string(), 1024);
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].0, "Content-Type");
        assert_eq!(headers[0].1, "image/png");
        assert_eq!(headers[1].0, "Content-Length");
        assert_eq!(headers[1].1, "1024");
    }

    #[test]
    fn test_federation_guess_content_type_from_infer() {
        // PNG magic bytes: 89 50 4E 47
        let data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(federation_guess_content_type("anything.bin", &data), "image/png");
    }

    #[test]
    fn test_federation_guess_content_type_from_extension_jpeg() {
        // infer library might not detect JPEG from partial bytes, so the
        // extension fallback should handle it.
        let data = [0xFF, 0xD8, 0xFF]; // partial JPEG
        assert_eq!(federation_guess_content_type("photo.jpg", &data), "image/jpeg");
    }

    #[test]
    fn test_federation_guess_content_type_from_extension_png() {
        let data = [0x00; 1]; // no infer match
        assert_eq!(federation_guess_content_type("icon.png", &data), "image/png");
    }

    #[test]
    fn test_federation_guess_content_type_from_extension_svg() {
        let data = [0x00; 1];
        assert_eq!(federation_guess_content_type("drawing.svg", &data), "image/svg+xml");
    }

    #[test]
    fn test_federation_guess_content_type_from_extension_webm() {
        let data = [0x00; 1];
        assert_eq!(federation_guess_content_type("video.webm", &data), "video/webm");
    }

    #[test]
    fn test_federation_guess_content_type_unknown_extension() {
        let data = [0x00; 1];
        assert_eq!(federation_guess_content_type("file.xyz", &data), "application/octet-stream");
    }

    #[test]
    fn test_federation_guess_content_type_no_extension() {
        let data = [0x00; 1];
        assert_eq!(federation_guess_content_type("file", &data), "application/octet-stream");
    }

    #[test]
    fn test_federation_guess_content_type_uppercase_extension() {
        let data = [0x00; 1];
        assert_eq!(federation_guess_content_type("ICON.PNG", &data), "image/png");
        assert_eq!(federation_guess_content_type("VIDEO.MP4", &data), "video/mp4");
    }
}
