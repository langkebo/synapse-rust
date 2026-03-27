//! 自定义提取器模块
//! 
//! 从 routes/mod.rs 提取的通用提取器

mod pagination;

use crate::common::ApiError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

// ============== MatrixJson ==============

/// 自定义 JSON 提取器，提供友好的错误消息
pub struct MatrixJson<T>(pub T);

impl<S, T> axum::extract::FromRequest<S> for MatrixJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = ApiError;

    async fn from_request(
        req: axum::extract::Request, 
        state: &S
    ) -> Result<Self, Self::Rejection> {
        match axum::extract::Json::<T>::from_request(req, state).await {
            Ok(axum::extract::Json(value)) => Ok(MatrixJson(value)),
            Err(rejection) => {
                let message = match rejection {
                    axum::extract::rejection::JsonRejection::JsonDataError(e) => 
                        format!("Invalid JSON data: {}", e),
                    axum::extract::rejection::JsonRejection::JsonSyntaxError(e) => 
                        format!("JSON syntax error: {}", e),
                    axum::extract::rejection::JsonRejection::MissingJsonContentType(e) => 
                        format!("Missing Content-Type: application/json: {}", e),
                    _ => format!("JSON error: {}", rejection),
                };
                Err(ApiError::bad_request(message))
            }
        }
    }
}

// ============== RoomId ==============

/// Room ID 提取器
/// 格式: !room_id:domain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomId(pub String);

// ============== UserId ==============

/// User ID 提取器
/// 格式: @user_id:domain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserId(pub String);

impl UserId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
    
    pub fn parse(raw: &str) -> Result<Self, ApiError> {
        if raw.starts_with('@') {
            Ok(Self(raw.to_string()))
        } else {
            Err(ApiError::bad_request(
                format!("Invalid user ID format: {}", raw)
            ))
        }
    }
}

// ============== Pagination ==============

pub use pagination::Pagination;

// ============== DeviceId ==============

/// Device ID 提取器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceId(pub String);

impl DeviceId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
}

// ============== EventId ==============

/// Event ID 提取器
/// 格式: $event_id:domain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventId(pub String);

impl EventId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
}