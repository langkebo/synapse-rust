use crate::common::ApiError;
use axum::extract::rejection::JsonRejection;

pub struct MatrixJson<T>(pub T);

impl<S, T> axum::extract::FromRequest<S> for MatrixJson<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned + Send,
{
    type Rejection = ApiError;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Json::<T>::from_request(req, state).await {
            Ok(axum::extract::Json(value)) => Ok(Self(value)),
            Err(rejection) => {
                let message = match rejection {
                    JsonRejection::JsonDataError(e) => format!("Invalid JSON data: {e}"),
                    JsonRejection::JsonSyntaxError(e) => format!("JSON syntax error: {e}"),
                    JsonRejection::MissingJsonContentType(e) => {
                        format!("Missing Content-Type: application/json: {e}")
                    }
                    _ => format!("JSON error: {rejection}"),
                };
                Err(ApiError::bad_request(message))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MatrixJson;
    use crate::common::ApiError;
    use axum::{
        body::Body,
        extract::FromRequest,
        http::{header::CONTENT_TYPE, Request, StatusCode},
    };
    use futures::stream;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct SamplePayload {
        count: u32,
    }

    async fn extract_payload(request: Request<Body>) -> Result<MatrixJson<SamplePayload>, ApiError> {
        MatrixJson::<SamplePayload>::from_request(request, &()).await
    }

    #[tokio::test]
    async fn matrix_json_extracts_valid_json_payload() {
        let request = Request::builder()
            .uri("/_matrix/test")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"count": 7}"#))
            .unwrap();

        let payload = extract_payload(request).await.unwrap();

        assert_eq!(payload.0, SamplePayload { count: 7 });
    }

    #[tokio::test]
    async fn matrix_json_reports_json_syntax_errors_as_bad_request() {
        let request = Request::builder()
            .uri("/_matrix/test")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"count": }"#))
            .unwrap();

        let error = match extract_payload(request).await {
            Ok(_) => panic!("expected syntax error"),
            Err(error) => error,
        };

        assert_eq!(error.http_status(), StatusCode::BAD_REQUEST);
        assert!(error.message().contains("JSON syntax error"));
    }

    #[tokio::test]
    async fn matrix_json_reports_json_data_errors_as_bad_request() {
        let request = Request::builder()
            .uri("/_matrix/test")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"count":"oops"}"#))
            .unwrap();

        let error = match extract_payload(request).await {
            Ok(_) => panic!("expected data error"),
            Err(error) => error,
        };

        assert_eq!(error.http_status(), StatusCode::BAD_REQUEST);
        assert!(error.message().contains("Invalid JSON data"));
    }

    #[tokio::test]
    async fn matrix_json_requires_application_json_content_type() {
        let request = Request::builder().uri("/_matrix/test").body(Body::from(r#"{"count": 1}"#)).unwrap();

        let error = match extract_payload(request).await {
            Ok(_) => panic!("expected missing content-type error"),
            Err(error) => error,
        };

        assert_eq!(error.http_status(), StatusCode::BAD_REQUEST);
        assert!(error.message().contains("Missing Content-Type: application/json"));
    }

    #[tokio::test]
    async fn matrix_json_reports_body_read_failures_as_generic_json_errors() {
        let request = Request::builder()
            .uri("/_matrix/test")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from_stream(stream::once(async {
                Err::<bytes::Bytes, std::io::Error>(std::io::Error::other("broken body stream"))
            })))
            .unwrap();

        let error = match extract_payload(request).await {
            Ok(_) => panic!("expected body read failure"),
            Err(error) => error,
        };

        assert_eq!(error.http_status(), StatusCode::BAD_REQUEST);
        assert!(error.message().contains("JSON error"));
        assert!(error.message().contains("broken body stream"));
    }
}
