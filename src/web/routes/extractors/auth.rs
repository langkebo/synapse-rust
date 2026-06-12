use crate::common::ApiError;
use crate::storage::CreateAuditEventRequest;
use crate::web::routes::AppState;
use crate::web::utils::admin_auth::authorize_admin_request;
use crate::web::utils::auth::resolve_request_id;
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, HeaderMap, Method},
};
use serde_json::json;

#[derive(Clone)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub device_id: Option<String>,
    pub is_admin: bool,
    pub is_shadow_banned: bool,
    pub is_guest: bool,
    pub access_token: String,
}

#[derive(Clone)]
pub struct OptionalAuthenticatedUser {
    pub user_id: Option<String>,
    pub device_id: Option<String>,
    pub is_admin: bool,
    pub is_shadow_banned: bool,
    pub is_guest: bool,
    pub access_token: Option<String>,
}

#[derive(Clone)]
pub struct AdminUser {
    pub user_id: String,
    pub device_id: Option<String>,
    pub access_token: String,
    pub role: String,
}

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let uri = parts.uri.to_string();
        let token_result = extract_token_from_request(&parts.headers, &uri);
        let state = state.clone();
        let method = parts.method.clone();
        let path = parts.uri.path().to_string();
        let headers = parts.headers.clone();

        async move {
            let token = token_result?;
            let result = state.services.core.auth_service.validate_token(&token).await;
            match result {
                Ok((user_id, device_id, is_admin, is_shadow_banned, is_guest)) => {
                    // 对敏感写操作记录审计日志 (非管理员路径)
                    if matches!(method, Method::POST | Method::PUT | Method::DELETE)
                        && !path.starts_with("/_synapse/admin")
                    {
                        let request_id = resolve_request_id(&headers);

                        let audit_request = CreateAuditEventRequest {
                            actor_id: user_id.clone(),
                            action: format!("user.{}", method.as_str().to_lowercase()),
                            resource_type: "client_api".to_string(),
                            resource_id: path.clone(),
                            result: "success".to_string(), // 在提取器中目前只能记录尝试/成功，真正的执行结果在 handler
                            request_id,
                            details: Some(json!({
                                "path": path,
                                "method": method.as_str(),
                                "is_admin": is_admin,
                            })),
                        };

                        if let Err(e) = state.services.admin.admin_audit_service.create_event(audit_request).await {
                            ::tracing::error!(target: "security_audit", "Failed to create user audit event: {}", e);
                        }
                    }

                    Ok(Self { user_id, device_id, is_admin, is_shadow_banned, is_guest, access_token: token })
                }
                Err(e) => Err(e),
            }
        }
    }
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let state = state.clone();
        let headers = parts.headers.clone();
        let method = parts.method.clone();
        let path = parts.uri.path().to_string();

        async move {
            let admin = authorize_admin_request(&headers, &method, &path, &state).await?;
            Ok(Self {
                user_id: admin.user_id,
                device_id: admin.device_id,
                access_token: admin.access_token,
                role: admin.role,
            })
        }
    }
}

impl FromRequestParts<AppState> for OptionalAuthenticatedUser {
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let uri = parts.uri.to_string();
        let token_result = extract_token_from_request(&parts.headers, &uri);
        let state = state.clone();

        async move {
            match token_result {
                Ok(token) => match state.services.core.auth_service.validate_token(&token).await {
                    Ok((user_id, device_id, is_admin, is_shadow_banned, is_guest)) => Ok(Self {
                        user_id: Some(user_id),
                        device_id,
                        is_admin,
                        is_shadow_banned,
                        is_guest,
                        access_token: Some(token),
                    }),
                    Err(_) => Ok(Self {
                        user_id: None,
                        device_id: None,
                        is_admin: false,
                        is_shadow_banned: false,
                        is_guest: false,
                        access_token: None,
                    }),
                },
                Err(_) => Ok(Self {
                    user_id: None,
                    device_id: None,
                    is_admin: false,
                    is_shadow_banned: false,
                    is_guest: false,
                    access_token: None,
                }),
            }
        }
    }
}

pub trait AuthExtractor {
    fn extract_token(&self, uri: &str) -> Result<String, ApiError>;
}

impl AuthExtractor for HeaderMap {
    fn extract_token(&self, uri: &str) -> Result<String, ApiError> {
        extract_token_from_request(self, uri)
    }
}

pub(crate) fn extract_token_from_request(headers: &HeaderMap, uri: &str) -> Result<String, ApiError> {
    match crate::web::utils::auth::bearer_token(headers) {
        Ok(token) => Ok(token),
        Err(header_err) => {
            if let Some(query) = uri.split('?').nth(1) {
                for pair in query.split('&') {
                    if let Some(value) = pair.strip_prefix("access_token=") {
                        return Ok(value.to_string());
                    }
                }
            }
            Err(header_err)
        }
    }
}

pub(crate) fn extract_token_from_headers(headers: &HeaderMap) -> Result<String, ApiError> {
    crate::web::utils::auth::bearer_token(headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn test_extract_token_from_headers_valid() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer test-token-123".parse().unwrap());
        assert_eq!(extract_token_from_headers(&headers).unwrap(), "test-token-123");
    }

    #[test]
    fn test_extract_token_from_headers_missing() {
        let headers = HeaderMap::new();
        assert!(extract_token_from_headers(&headers).is_err());
    }

    #[test]
    fn test_extract_token_from_request_bearer_header() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer header-token".parse().unwrap());
        assert_eq!(extract_token_from_request(&headers, "/test").unwrap(), "header-token");
    }

    #[test]
    fn test_extract_token_from_request_query_param() {
        let headers = HeaderMap::new();
        let uri = "/_matrix/client/v3/sync?access_token=query-token&other=value";
        assert_eq!(extract_token_from_request(&headers, uri).unwrap(), "query-token");
    }

    #[test]
    fn test_extract_token_from_request_header_takes_priority() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer header-token".parse().unwrap());
        let uri = "/test?access_token=query-token";
        assert_eq!(extract_token_from_request(&headers, uri).unwrap(), "header-token");
    }

    #[test]
    fn test_extract_token_from_request_query_only() {
        let headers = HeaderMap::new();
        let uri = "/test?access_token=abc123";
        assert_eq!(extract_token_from_request(&headers, uri).unwrap(), "abc123");
    }

    #[test]
    fn test_extract_token_from_request_no_token() {
        let headers = HeaderMap::new();
        let uri = "/test";
        assert!(extract_token_from_request(&headers, uri).is_err());
    }

    #[test]
    fn test_extract_token_from_request_query_no_access_token() {
        let headers = HeaderMap::new();
        let uri = "/test?other_param=value";
        assert!(extract_token_from_request(&headers, uri).is_err());
    }
}
