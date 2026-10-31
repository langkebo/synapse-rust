use axum::{
    extract::{rejection::PathRejection, FromRequestParts, OptionalFromRequestParts},
    http::request::Parts,
};
use synapse_common::ApiError;

use super::log_rejection;

/// Drop-in replacement for `axum::extract::Path` in extractor position.
///
/// Handlers keep destructuring `MatrixPath(params)` exactly like they did with
/// `Path(params)`, and deref forwards field access to the inner value. The only
/// behavioural difference is the rejection: path parameters that cannot be
/// deserialized are mapped onto the unified Matrix error envelope (`400`
/// `application/json` with an `errcode`) instead of Axum's plain-text `400`.
pub struct MatrixPath<T>(pub T);

impl<T> std::ops::Deref for MatrixPath<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> std::ops::DerefMut for MatrixPath<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

/// Map an Axum [`PathRejection`] onto the Matrix error body.
///
///   M_INVALID_PARAM — the captured segment exists but cannot be deserialized
///                     (wrong type, wrong arity, invalid UTF-8, …)
///   M_INTERNAL      — the route matched but Axum's path-parameter extension is
///                     missing (a router wiring bug, not a client error)
///
/// The rejection's own text (which names the expected type and the captured
/// value) goes to the log only — see [`log_rejection`](super::log_rejection).
fn map_path_rejection(rejection: PathRejection) -> ApiError {
    match rejection {
        PathRejection::FailedToDeserializePathParams(detail) => {
            log_rejection(detail, ApiError::invalid_param("Invalid path parameter"))
        }
        PathRejection::MissingPathParams(detail) => {
            log_rejection(detail, ApiError::internal("Missing path parameters for matched route"))
        }
        other => log_rejection(other, ApiError::invalid_param("Invalid path parameter")),
    }
}

impl<S, T> FromRequestParts<S> for MatrixPath<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned + Send,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match <axum::extract::Path<T> as FromRequestParts<S>>::from_request_parts(parts, state).await {
            Ok(axum::extract::Path(value)) => Ok(Self(value)),
            Err(rejection) => Err(map_path_rejection(rejection)),
        }
    }
}

/// Mirrors Axum's `Option<Path<T>>` semantics (no path parameters in the route
/// ⇒ `None`) while keeping Matrix-shaped errors for captured-but-invalid
/// segments.
impl<S, T> OptionalFromRequestParts<S> for MatrixPath<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned + Send + 'static,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Option<Self>, Self::Rejection> {
        match <axum::extract::Path<T> as OptionalFromRequestParts<S>>::from_request_parts(parts, state).await {
            Ok(Some(axum::extract::Path(value))) => Ok(Some(Self(value))),
            Ok(None) => Ok(None),
            Err(rejection) => Err(map_path_rejection(rejection)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MatrixPath;
    use axum::{
        body::Body,
        http::{header::CONTENT_TYPE, Request, StatusCode},
        routing::get,
        Router,
    };
    use serde_json::Value;
    use tower::ServiceExt;

    async fn echo_number(MatrixPath(number): MatrixPath<u32>) -> String {
        number.to_string()
    }

    async fn optional_number(number: Option<MatrixPath<u32>>) -> String {
        number.map(|MatrixPath(number)| number.to_string()).unwrap_or_else(|| "none".to_string())
    }

    async fn send(app: Router, uri: &str) -> axum::response::Response {
        app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap()).await.unwrap()
    }

    #[tokio::test]
    async fn matrix_path_extracts_valid_segment() {
        let app = Router::new().route("/numbers/{number}", get(echo_number));

        let response = send(app, "/numbers/42").await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"42");
    }

    #[tokio::test]
    async fn matrix_path_reports_unparsable_segment_as_invalid_param_json() {
        let app = Router::new().route("/numbers/{number}", get(echo_number));

        let response = send(app, "/numbers/not-a-number").await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let content_type = response.headers().get(CONTENT_TYPE).and_then(|value| value.to_str().ok()).unwrap_or("");
        assert!(content_type.contains("application/json"), "expected application/json, got: {content_type}");

        let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
        let json: Value = serde_json::from_slice(&body).expect("body should be valid JSON");
        assert_eq!(json["errcode"], "M_INVALID_PARAM");
        assert_eq!(json["error"], "Invalid path parameter", "serde detail must not leak: {json}");
    }

    #[tokio::test]
    async fn matrix_path_is_none_when_route_has_no_parameters() {
        let app = Router::new().route("/numbers", get(optional_number));

        let response = send(app, "/numbers").await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"none");
    }

    #[tokio::test]
    async fn matrix_path_is_some_when_route_has_parameters() {
        let app = Router::new().route("/numbers/{number}", get(optional_number));

        let response = send(app, "/numbers/7").await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"7");
    }
}
