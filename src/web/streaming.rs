use axum::{
    body::Bytes,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
};
use std::path::PathBuf;

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
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let data = std::fs::read(&path)?;
        let metadata = std::fs::metadata(&path)?;

        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            content_type
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
                .unwrap(),
        );
        headers.insert(
            header::CONTENT_LENGTH,
            metadata.len().to_string().parse().unwrap(),
        );
        if let Some(name) = filename {
            headers.insert(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", name)
                    .parse()
                    .unwrap(),
            );
        }

        Ok(Self {
            status: StatusCode::OK,
            headers,
            body: Bytes::from(data),
        })
    }

    pub fn bytes(data: Vec<u8>, content_type: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
        headers.insert(
            header::CONTENT_LENGTH,
            data.len().to_string().parse().unwrap(),
        );

        Ok(Self {
            status: StatusCode::OK,
            headers,
            body: Bytes::from(data),
        })
    }

    pub fn sse(event_name: String, data: String, id: Option<u64>) -> Self {
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
        headers.insert(header::CONTENT_TYPE, "text/event-stream".parse().unwrap());
        headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
        headers.insert(header::CONNECTION, "keep-alive".parse().unwrap());

        Self {
            status: StatusCode::OK,
            headers,
            body: Bytes::from(sse_data),
        }
    }

    pub fn chunked(items: Vec<String>, content_type: &str) -> Self {
        let combined = items.join("\n");

        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
        headers.insert(
            header::CONTENT_LENGTH,
            combined.len().to_string().parse().unwrap(),
        );

        Self {
            status: StatusCode::OK,
            headers,
            body: Bytes::from(combined),
        }
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
) -> Result<StreamingResponse, Box<dyn std::error::Error>> {
    StreamingResponse::file(path, filename, content_type)
}

pub async fn stream_sse(event_name: String, data: String, id: Option<u64>) -> StreamingResponse {
    StreamingResponse::sse(event_name, data, id)
}

pub async fn stream_json_chunked(items: Vec<serde_json::Value>) -> StreamingResponse {
    let json_strings: Vec<String> = items
        .into_iter()
        .map(|item| serde_json::to_string(&item).unwrap())
        .collect();
    StreamingResponse::chunked(json_strings, "application/json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sse_response() {
        let response =
            StreamingResponse::sse("test_event".to_string(), "test data".to_string(), Some(1));

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
        let response = StreamingResponse::bytes(data, "text/plain").unwrap();

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
        let response = StreamingResponse::chunked(items, "text/plain");

        let (parts, _) = response.into_response().into_parts();
        assert_eq!(parts.status, StatusCode::OK);
    }
}
