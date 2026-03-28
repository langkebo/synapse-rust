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
            Ok(axum::extract::Json(value)) => Ok(MatrixJson(value)),
            Err(rejection) => {
                let message = match rejection {
                    JsonRejection::JsonDataError(e) => format!("Invalid JSON data: {}", e),
                    JsonRejection::JsonSyntaxError(e) => format!("JSON syntax error: {}", e),
                    JsonRejection::MissingJsonContentType(e) => {
                        format!("Missing Content-Type: application/json: {}", e)
                    }
                    _ => format!("JSON error: {}", rejection),
                };
                Err(ApiError::bad_request(message))
            }
        }
    }
}
