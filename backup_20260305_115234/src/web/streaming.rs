use axum::{
    body::Bytes,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StreamingError {
    #[error("File read error: {0}")]
    FileReadError(String),
    #[error("Metadata error: {0}")]
    MetadataError(String),
    #[error("Header parse error: {0}")]
    HeaderParseError(String),
    #[error("Invalid content length")]
    InvalidContentLength,
    #[error("SSE construction error: {0}")]
    SseError(String),
}

pub const CHUNK_SIZE: usize = 64 * 1024;

pub struct StreamingResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl StreamingResponse {
    pub fn file(
        path: PathBuf,
        filename: Option<String>,
        content_type: Option<&str>,
    ) -> Result<Self, StreamingError> {
        let data =
            std::fs::read(&path).map_err(|e| StreamingError::FileReadError(e.to_string()))?;
        let metadata =
            std::fs::metadata(&path).map_err(|e| StreamingError::MetadataError(e.to_string()))?;

        let mut headers = HeaderMap::new();
        let content_type_value = content_type
            .unwrap_or_else(|| {
                if path.extension().and_then(|e| e.to_str()) == Some("mp4") {
                    "video/mp4"
                } else if path.extension().and_then(|e| e.to_str()) == Some("mp3") {
                    "audio/mpeg"
                } else if path.extension().and_then(|e| e.to_str()) == Some("png") {
                    "image/png"
                } else if path.extension().and_then(|e| e.to_str()) == Some("jpg")
                    || path.extension().and_then(|e| e.to_str()) == Some("jpeg")
                {
                    "image/jpeg"
                } else {
                    "application/octet-stream"
                }
            })
            .parse()
            .map_err(|_| StreamingError::HeaderParseError("Invalid content-type".to_string()))?;
        headers.insert(header::CONTENT_TYPE, content_type_value);

        let content_length = metadata
            .len()
            .to_string()
            .parse()
            .map_err(|_| StreamingError::InvalidContentLength)?;
        headers.insert(header::CONTENT_LENGTH, content_length);

        if let Some(name) = filename {
            let disposition = format!("attachment; filename=\"{}\"", name)
                .parse()
                .map_err(|_| {
                    StreamingError::HeaderParseError("Invalid content-disposition".to_string())
                })?;
            headers.insert(header::CONTENT_DISPOSITION, disposition);
        }

        Ok(Self {
            status: StatusCode::OK,
            headers,
            body: Bytes::from(data),
        })
    }

    pub fn bytes(data: Vec<u8>, content_type: &str) -> Result<Self, StreamingError> {
        let mut headers = HeaderMap::new();
        let content_type_header = content_type
            .parse()
            .map_err(|_| StreamingError::HeaderParseError("Invalid content-type".to_string()))?;
        headers.insert(header::CONTENT_TYPE, content_type_header);

        let content_length = data
            .len()
            .to_string()
            .parse()
            .map_err(|_| StreamingError::InvalidContentLength)?;
        headers.insert(header::CONTENT_LENGTH, content_length);

        Ok(Self {
            status: StatusCode::OK,
            headers,
            body: Bytes::from(data),
        })
    }

    pub fn sse(event_name: String, data: String, id: Option<u64>) -> Result<Self, StreamingError> {
        let mut sse_data = String::new();
        if let Some(event_id) = id {
            sse_data.push_str(&format!("id: {}\n", event_id));
        }
        sse_data.push_str(&format!("event: {}\n", event_name));
        for line in data.lines() {
            sse_data.push_str(&format!("data: {}\n", line));
        }
        sse_data.push('\n');

        let mut headers = HeaderMap::new();
        let content_type = "text/event-stream"
            .parse()
            .map_err(|_| StreamingError::HeaderParseError("Invalid content-type".to_string()))?;
        headers.insert(header::CONTENT_TYPE, content_type);

        let cache_control = "no-cache"
            .parse()
            .map_err(|_| StreamingError::HeaderParseError("Invalid cache-control".to_string()))?;
        headers.insert(header::CACHE_CONTROL, cache_control);

        let connection = "keep-alive"
            .parse()
            .map_err(|_| StreamingError::HeaderParseError("Invalid connection".to_string()))?;
        headers.insert(header::CONNECTION, connection);

        Ok(Self {
            status: StatusCode::OK,
            headers,
            body: Bytes::from(sse_data),
        })
    }

    pub fn chunked(items: Vec<String>, content_type: &str) -> Result<Self, StreamingError> {
        let combined = items.join("\n");

        let mut headers = HeaderMap::new();
        let content_type_header = content_type
            .parse()
            .map_err(|_| StreamingError::HeaderParseError("Invalid content-type".to_string()))?;
        headers.insert(header::CONTENT_TYPE, content_type_header);

        let content_length = combined
            .len()
            .to_string()
            .parse()
            .map_err(|_| StreamingError::InvalidContentLength)?;
        headers.insert(header::CONTENT_LENGTH, content_length);

        Ok(Self {
            status: StatusCode::OK,
            headers,
            body: Bytes::from(combined),
        })
    }
}

impl IntoResponse for StreamingResponse {
    fn into_response(self) -> axum::response::Response {
        (self.status, self.headers, self.body).into_response()
    }
}

pub async fn stream_file(
    path: PathBuf,
    filename: Option<String>,
    content_type: Option<&str>,
) -> Result<StreamingResponse, StreamingError> {
    StreamingResponse::file(path, filename, content_type)
}

pub async fn stream_sse(
    event_name: String,
    data: String,
    id: Option<u64>,
) -> Result<StreamingResponse, StreamingError> {
    StreamingResponse::sse(event_name, data, id)
}

pub async fn stream_json_chunked(
    items: Vec<serde_json::Value>,
) -> Result<StreamingResponse, StreamingError> {
    let json_strings: Vec<String> = items
        .into_iter()
        .map(|item| {
            serde_json::to_string(&item).map_err(|e| StreamingError::SseError(e.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    StreamingResponse::chunked(json_strings, "application/json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header;

    #[tokio::test]
    async fn test_sse_response() {
        let response_result =
            StreamingResponse::sse("test_event".to_string(), "test data".to_string(), Some(1));
        assert!(response_result.is_ok(), "SSE response should succeed");
        let response = response_result.unwrap();

        let (parts, _) = response.into_response().into_parts();
        assert_eq!(parts.status, StatusCode::OK);
        assert_eq!(
            parts.headers.get(header::CONTENT_TYPE).unwrap(),
            "text/event-stream"
        );
        assert_eq!(
            parts.headers.get(header::CACHE_CONTROL).unwrap(),
            "no-cache"
        );
    }

    #[tokio::test]
    async fn test_bytes_response() {
        let data = b"Hello, World!".to_vec();
        let response_result = StreamingResponse::bytes(data, "text/plain");
        assert!(response_result.is_ok(), "Bytes response should succeed");
        let response = response_result.unwrap();

        let (parts, _) = response.into_response().into_parts();
        assert_eq!(parts.status, StatusCode::OK);
        assert_eq!(
            parts.headers.get(header::CONTENT_TYPE).unwrap(),
            "text/plain"
        );
    }

    #[tokio::test]
    async fn test_chunked_response() {
        let items = vec!["item1".to_string(), "item2".to_string()];
        let response_result = StreamingResponse::chunked(items, "text/plain");
        assert!(response_result.is_ok(), "Chunked response should succeed");
        let response = response_result.unwrap();

        let (parts, _) = response.into_response().into_parts();
        assert_eq!(parts.status, StatusCode::OK);
    }
}
