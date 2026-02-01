# 错误处理文档

> **版本**：1.0.0  
> **创建日期**：2026-01-28  
> **项目状态**：开发中  
> **参考文档**：[Synapse 官方文档](https://element-hq.github.io/synapse/latest/)、[Matrix 规范](https://spec.matrix.org/)

---

## 一、错误类型定义

### 1.1 ApiError 枚举

`ApiError` 是项目的统一错误类型，所有公共 API 都使用此错误类型。

```rust
# [derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    
    #[error("Unauthorized")]
    Unauthorized,
    
    #[error("Forbidden")]
    Forbidden,
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Conflict: {0}")]
    Conflict(String),
    
    #[error("Rate limited")]
    RateLimited,
    
    #[error("Internal error: {0}")]
    Internal(String),
    
    #[error("Database error: {0}")]
    Database(String),
    
    #[error("Cache error: {0}")]
    Cache(String),
    
    #[error("Authentication error: {0}")]
    Authentication(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
}
```

### 1.2 错误变体说明

| 错误变体 | HTTP 状态码 | Matrix 错误码 | 描述 |
|-----------|------------|---------------|------|
| `BadRequest` | 400 | M_BAD_JSON | 请求格式错误 |
| `Unauthorized` | 401 | M_UNAUTHORIZED | 未授权 |
| `Forbidden` | 403 | M_FORBIDDEN | 禁止访问 |
| `NotFound` | 404 | M_NOT_FOUND | 资源未找到 |
| `Conflict` | 409 | M_USER_IN_USE | 资源冲突 |
| `RateLimited` | 429 | M_LIMIT_EXCEEDED | 请求频率超限 |
| `Internal` | 500 | M_UNKNOWN | 内部错误 |
| `Database` | 500 | M_UNKNOWN | 数据库错误 |
| `Cache` | 500 | M_UNKNOWN | 缓存错误 |
| `Authentication` | 401 | M_UNKNOWN_TOKEN | 认证错误 |
| `Validation` | 400 | M_INVALID_PARAM | 验证错误 |

---

## 二、错误传播机制

### 2.1 From Trait 实现

`ApiError` 实现了 `From` trait，支持从其他错误类型自动转换。

```rust
impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        ApiError::Database(err.to_string())
    }
}

impl From<redis::RedisError> for ApiError {
    fn from(err: redis::RedisError) -> Self {
        ApiError::Cache(err.to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for ApiError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        ApiError::Authentication(err.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::BadRequest(err.to_string())
    }
}
```

### 2.2 错误传播

使用 `?` 操作符进行错误传播。

```rust
pub async fn get_user(&self, user_id: &str) -> Result<User, ApiError> {
    let user = self.user_storage.get_user(user_id).await?;
    Ok(user)
}
```

### 2.3 错误转换

自定义错误转换函数。

```rust
impl ApiError {
    pub fn internal<E: std::error::Error>(err: E) -> Self {
        ApiError::Internal(err.to_string())
    }
    
    pub fn not_found(resource: &str) -> Self {
        ApiError::NotFound(resource.to_string())
    }
    
    pub fn bad_request(msg: &str) -> Self {
        ApiError::BadRequest(msg.to_string())
    }
}
```

---

## 三、错误响应格式

### 3.1 Matrix 标准错误响应

Matrix 标准错误响应格式：

```json
{
  "errcode": "M_UNKNOWN",
  "error": "Unknown error"
}
```

### 3.2 错误响应实现

实现 `IntoResponse` trait，将 `ApiError` 转换为 HTTP 响应。

```rust
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, errcode, error) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "M_BAD_JSON", msg),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "M_UNAUTHORIZED", "Unauthorized"),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "M_FORBIDDEN", "Forbidden"),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "M_NOT_FOUND", msg),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, "M_USER_IN_USE", msg),
            ApiError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "M_LIMIT_EXCEEDED", "Rate limited"),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "M_UNKNOWN", msg),
            ApiError::Database(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "M_UNKNOWN", msg),
            ApiError::Cache(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "M_UNKNOWN", msg),
            ApiError::Authentication(msg) => (StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", msg),
            ApiError::Validation(msg) => (StatusCode::BAD_REQUEST, "M_INVALID_PARAM", msg),
        };
        
        let body = serde_json::json!({
            "errcode": errcode,
            "error": error
        });
        
        (status, Json(body)).into_response()
    }
}
```

### 3.3 错误响应示例

#### 3.3.1 Bad Request

```json
{
  "errcode": "M_BAD_JSON",
  "error": "Invalid JSON format"
}
```

#### 3.3.2 Unauthorized

```json
{
  "errcode": "M_UNAUTHORIZED",
  "error": "Unauthorized"
}
```

#### 3.3.3 Not Found

```json
{
  "errcode": "M_NOT_FOUND",
  "error": "User not found"
}
```

#### 3.3.4 Rate Limited

```json
{
  "errcode": "M_LIMIT_EXCEEDED",
  "error": "Rate limited"
}
```

---

## 四、日志记录规范

### 4.1 日志级别

| 级别 | 描述 | 使用场景 |
|------|------|----------|
| ERROR | 错误 | 需要立即处理的错误 |
| WARN | 警告 | 可能的问题，但不需要立即处理 |
| INFO | 信息 | 重要操作和状态变化 |
| DEBUG | 调试 | 详细的调试信息 |
| TRACE | 追踪 | 最详细的执行流程 |

### 4.2 错误日志

使用 `tracing` 库记录错误日志。

```rust
use tracing::{error, warn, info, debug, trace};

pub async fn get_user(&self, user_id: &str) -> Result<User, ApiError> {
    debug!("Getting user: {}", user_id);
    
    match self.user_storage.get_user(user_id).await {
        Ok(Some(user)) => {
            info!("User found: {}", user_id);
            Ok(user)
        }
        Ok(None) => {
            warn!("User not found: {}", user_id);
            Err(ApiError::not_found("User"))
        }
        Err(err) => {
            error!("Database error getting user {}: {}", user_id, err);
            Err(ApiError::from(err))
        }
    }
}
```

### 4.3 结构化日志

使用结构化日志记录上下文信息。

```rust
use tracing::{instrument, error};

# [instrument(skip(self))]
pub async fn get_user(&self, user_id: &str) -> Result<User, ApiError> {
    debug!(user_id, "Getting user");
    
    let user = self.user_storage.get_user(user_id).await
        .map_err(|err| {
            error!(error = %err, user_id, "Failed to get user");
            ApiError::from(err)
        })?;
    
    Ok(user)
}
```

### 4.4 错误上下文

记录错误上下文信息，便于调试。

```rust
use tracing::error;

pub async fn create_user(&self, username: &str, password: &str) -> Result<User, ApiError> {
    debug!(username, "Creating user");
    
    let password_hash = hash_password(password)
        .map_err(|err| {
            error!(error = %err, username, "Failed to hash password");
            ApiError::internal(err)
        })?;
    
    let user = self.user_storage.create_user(&user_id, username, Some(&password_hash), false).await
        .map_err(|err| {
            error!(error = %err, username, "Failed to create user");
            ApiError::from(err)
        })?;
    
    info!(user_id = user.user_id, username, "User created successfully");
    Ok(user)
}
```

---

## 五、错误处理最佳实践

### 5.1 早期返回

在函数开始时进行参数验证，早期返回错误。

```rust
pub async fn create_user(&self, username: &str, password: &str) -> Result<User, ApiError> {
    if username.is_empty() {
        return Err(ApiError::bad_request("Username cannot be empty"));
    }
    
    if password.len() < 8 {
        return Err(ApiError::bad_request("Password must be at least 8 characters"));
    }
    
    // 继续处理...
}
```

### 5.2 错误链

使用 `anyhow` 或 `eyre` 进行错误链，保留原始错误信息。

```rust
use anyhow::{Context, Result};

pub async fn create_user(&self, username: &str, password: &str) -> Result<User> {
    let password_hash = hash_password(password)
        .context("Failed to hash password")?;
    
    let user = self.user_storage.create_user(&user_id, username, Some(&password_hash), false).await
        .context("Failed to create user")?;
    
    Ok(user)
}
```

### 5.3 自定义错误

为特定场景定义自定义错误类型。

```rust
# [derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,
    
    #[error("Token expired")]
    TokenExpired,
    
    #[error("Token invalid")]
    TokenInvalid,
}

impl From<AuthError> for ApiError {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::InvalidCredentials => ApiError::Unauthorized,
            AuthError::TokenExpired => ApiError::Authentication("Token expired".to_string()),
            AuthError::TokenInvalid => ApiError::Authentication("Token invalid".to_string()),
        }
    }
}
```

### 5.4 错误恢复

对于可恢复的错误，尝试恢复操作。

```rust
pub async fn get_user_with_retry(&self, user_id: &str) -> Result<User, ApiError> {
    let mut retries = 3;
    
    loop {
        match self.user_storage.get_user(user_id).await {
            Ok(Some(user)) => return Ok(user),
            Ok(None) => return Err(ApiError::not_found("User")),
            Err(err) if retries > 0 => {
                warn!("Failed to get user {}, retrying... ({})", user_id, err);
                retries -= 1;
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
            Err(err) => {
                error!("Failed to get user {}: {}", user_id, err);
                return Err(ApiError::from(err));
            }
        }
    }
}
```

---

## 六、错误测试

### 6.1 单元测试

测试错误处理逻辑。

```rust
# [cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_conversion() {
        let err = sqlx::Error::RowNotFound;
        let api_err = ApiError::from(err);
        assert!(matches!(api_err, ApiError::Database(_)));
    }
    
    #[test]
    fn test_error_response() {
        let err = ApiError::not_found("User");
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
```

### 6.2 集成测试

测试 API 错误响应。

```rust
# [tokio::test]
async fn test_get_user_not_found() {
    let app = create_test_app();
    
    let response = app
        .oneshot(Request::builder()
            .uri("/_matrix/client/r0/users/@nonexistent:server.com")
            .body(Body::empty())
            .unwrap())
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    
    let body = hyper::body::to_bytes(response.into_body())
        .await
        .unwrap();
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(error["errcode"], "M_NOT_FOUND");
}
```

---

## 七、参考资料

- [Matrix 错误码规范](https://spec.matrix.org/v1.11/client-server-api/#standard-error-response)
- [thiserror 文档](https://docs.rs/thiserror/latest/thiserror/)
- [tracing 文档](https://docs.rs/tracing/latest/tracing/)
- [Axum 错误处理](https://docs.rs/axum/latest/axum/error_handling/)

---

## 八、变更日志

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.0.0 | 2026-01-28 | 初始版本，定义错误处理文档 |
