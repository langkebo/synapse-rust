use axum::{
    extract::{rejection::QueryRejection, FromRequestParts},
    http::{request::Parts, Uri},
};
use synapse_common::ApiError;

/// Drop-in replacement for `axum::extract::Query` in extractor position.
///
/// Handlers keep destructuring `MatrixQuery(params)` exactly like they did with
/// `Query(params)`, and deref forwards field access to the inner value. The only
/// behavioural difference is the rejection: a query string that cannot be
/// deserialized is mapped onto the unified Matrix error envelope (`400`
/// `application/json` with an `errcode`) instead of Axum's plain-text `400`.
pub struct MatrixQuery<T>(pub T);

impl<T> std::ops::Deref for MatrixQuery<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> std::ops::DerefMut for MatrixQuery<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

/// Map an Axum [`QueryRejection`] onto the Matrix error body.
///
///   M_MISSING_PARAM — a required query parameter is absent
///   M_INVALID_PARAM — the query string is present but cannot be deserialized
///                     (wrong type, duplicate field, unsupported variant, …)
fn map_query_rejection(rejection: QueryRejection) -> ApiError {
    let detail = rejection.to_string();
    // `serde` phrases an absent required field as ``missing field `<name>` ``;
    // every other deserialization failure means a parameter *is* present but
    // its value cannot be interpreted.
    if detail.contains("missing field") {
        ApiError::missing_param(format!("Missing required query parameter: {detail}"))
    } else {
        ApiError::invalid_param(format!("Invalid query parameter: {detail}"))
    }
}

impl<T> MatrixQuery<T>
where
    T: serde::de::DeserializeOwned,
{
    /// Attempts to construct a [`MatrixQuery`] from a reference to a [`Uri`].
    ///
    /// Equivalent to [`axum::extract::Query::try_from_uri`] but returns the
    /// Matrix error envelope on failure.
    pub fn try_from_uri(value: &Uri) -> Result<Self, ApiError> {
        axum::extract::Query::<T>::try_from_uri(value)
            .map(|axum::extract::Query(value)| Self(value))
            .map_err(map_query_rejection)
    }
}

impl<S, T> FromRequestParts<S> for MatrixQuery<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Query::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Query(value)) => Ok(Self(value)),
            Err(rejection) => Err(map_query_rejection(rejection)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MatrixQuery;
    use axum::{
        body::Body,
        extract::FromRequestParts,
        http::{Request, StatusCode},
    };
    use serde::Deserialize;
    use synapse_common::MatrixErrorCode;

    #[derive(Debug, Deserialize, PartialEq)]
    struct SampleQuery {
        count: u32,
    }

    async fn extract_query(uri: &str) -> Result<MatrixQuery<SampleQuery>, synapse_common::ApiError> {
        let request = Request::builder().uri(uri).body(Body::empty()).unwrap();
        let (mut parts, _body) = request.into_parts();
        MatrixQuery::<SampleQuery>::from_request_parts(&mut parts, &()).await
    }

    #[tokio::test]
    async fn matrix_query_extracts_valid_query_string() {
        let query = extract_query("/_matrix/test?count=7").await.unwrap();

        assert_eq!(query.0, SampleQuery { count: 7 });
    }

    #[tokio::test]
    async fn matrix_query_reports_absent_parameter_as_missing_param() {
        let error = match extract_query("/_matrix/test").await {
            Ok(_) => panic!("expected missing-parameter rejection"),
            Err(error) => error,
        };

        assert_eq!(error.http_status(), StatusCode::BAD_REQUEST);
        assert_eq!(error.code, MatrixErrorCode::MissingParam);
        assert!(error.message().contains("Missing required query parameter"));
        assert!(error.message().contains("missing field"));
    }

    #[tokio::test]
    async fn matrix_query_reports_unparsable_parameter_as_invalid_param() {
        let error = match extract_query("/_matrix/test?count=not-a-number").await {
            Ok(_) => panic!("expected invalid-parameter rejection"),
            Err(error) => error,
        };

        assert_eq!(error.http_status(), StatusCode::BAD_REQUEST);
        assert_eq!(error.code, MatrixErrorCode::InvalidParam);
        assert!(error.message().contains("Invalid query parameter"));
        assert!(error.message().contains("invalid digit found in string"));
    }

    #[test]
    fn matrix_query_try_from_uri_matches_extractor_mapping() {
        let uri = "/_matrix/test?count=abc".parse().unwrap();
        let error = match MatrixQuery::<SampleQuery>::try_from_uri(&uri) {
            Ok(_) => panic!("expected invalid-parameter rejection"),
            Err(error) => error,
        };

        assert_eq!(error.code, MatrixErrorCode::InvalidParam);
    }
}
