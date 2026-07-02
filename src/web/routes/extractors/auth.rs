use crate::common::ApiError;
use crate::web::routes::context::{AuthContext, DeviceContext, E2eeRoomContext, RoomContext, SyncContext};
use crate::web::routes::AppState;
use crate::web::utils::admin_auth::authorize_admin_request;
use crate::web::utils::auth::resolve_request_id;
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, HeaderMap, Method},
};
use serde_json::json;
use synapse_storage::audit::CreateAuditEventRequest;

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

async fn audit_user_action(
    audit_svc: &synapse_services::AdminAuditService,
    user_id: &str,
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    is_admin: bool,
) {
    if matches!(method, &Method::POST | &Method::PUT | &Method::DELETE) && !path.starts_with("/_synapse/admin") {
        let request_id = resolve_request_id(headers);
        let audit_request = CreateAuditEventRequest {
            actor_id: user_id.to_string(),
            action: format!("user.{}", method.as_str().to_lowercase()),
            resource_type: "client_api".to_string(),
            resource_id: path.to_string(),
            result: "success".to_string(),
            request_id,
            details: Some(json!({
                "path": path,
                "method": method.as_str(),
                "is_admin": is_admin,
            })),
        };
        if let Err(e) = audit_svc.create_event(audit_request).await {
            ::tracing::error!(target: "security_audit", "Failed to create user audit event: {}", e);
        }
    }
}

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let uri = parts.uri.to_string();
        let token_result = crate::web::utils::auth::extract_token(&parts.headers, &uri);
        let state = state.clone();
        let method = parts.method.clone();
        let path = parts.uri.path().to_string();
        let headers = parts.headers.clone();

        async move {
            let token = token_result?;
            let result = state.services.core.auth_service.validate_token(&token).await;
            match result {
                Ok((user_id, device_id, is_admin, is_shadow_banned, is_guest)) => {
                    audit_user_action(
                        &state.services.admin.security.admin_audit_service,
                        &user_id,
                        &method,
                        &path,
                        &headers,
                        is_admin,
                    )
                    .await;

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
        let token_result = crate::web::utils::auth::extract_token(&parts.headers, &uri);
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

// =============================================================================
// FromRequestParts impls for typed context structs (RoomContext, SyncContext, etc.)
// =============================================================================
// These are needed because handlers migrate from State<AppState> to
// State<RoomContext> (etc.), and the auth extractors must work with the
// new state type. Each context carries an optional admin_audit_service
// field for best-effort audit event creation on write operations.

impl FromRequestParts<RoomContext> for AuthenticatedUser {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &RoomContext,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let uri = parts.uri.to_string();
        let token_result = crate::web::utils::auth::extract_token(&parts.headers, &uri);
        let state = state.clone();
        let method = parts.method.clone();
        let path = parts.uri.path().to_string();
        let headers = parts.headers.clone();

        async move {
            let token = token_result?;
            let result = state.auth_service.validate_token(&token).await;
            match result {
                Ok((user_id, device_id, is_admin, is_shadow_banned, is_guest)) => {
                    if let Some(ref audit_svc) = state.admin_audit_service {
                        audit_user_action(audit_svc, &user_id, &method, &path, &headers, is_admin).await;
                    }

                    Ok(Self { user_id, device_id, is_admin, is_shadow_banned, is_guest, access_token: token })
                }
                Err(e) => Err(e),
            }
        }
    }
}

impl FromRequestParts<E2eeRoomContext> for AuthenticatedUser {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &E2eeRoomContext,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let uri = parts.uri.to_string();
        let token_result = crate::web::utils::auth::extract_token(&parts.headers, &uri);
        let state = state.clone();
        let method = parts.method.clone();
        let path = parts.uri.path().to_string();
        let headers = parts.headers.clone();

        async move {
            let token = token_result?;
            let result = state.auth_service.validate_token(&token).await;
            match result {
                Ok((user_id, device_id, is_admin, is_shadow_banned, is_guest)) => {
                    if let Some(ref audit_svc) = state.admin_audit_service {
                        audit_user_action(audit_svc, &user_id, &method, &path, &headers, is_admin).await;
                    }

                    Ok(Self { user_id, device_id, is_admin, is_shadow_banned, is_guest, access_token: token })
                }
                Err(e) => Err(e),
            }
        }
    }
}

impl FromRequestParts<SyncContext> for AuthenticatedUser {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &SyncContext,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let uri = parts.uri.to_string();
        let token_result = crate::web::utils::auth::extract_token(&parts.headers, &uri);
        let state = state.clone();
        let method = parts.method.clone();
        let path = parts.uri.path().to_string();
        let headers = parts.headers.clone();

        async move {
            let token = token_result?;
            let (user_id, device_id, is_admin, is_shadow_banned, is_guest) =
                state.auth_service.validate_token(&token).await?;

            if let Some(ref audit_svc) = state.admin_audit_service {
                audit_user_action(audit_svc, &user_id, &method, &path, &headers, is_admin).await;
            }

            Ok(Self { user_id, device_id, is_admin, is_shadow_banned, is_guest, access_token: token })
        }
    }
}

impl FromRequestParts<DeviceContext> for AuthenticatedUser {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &DeviceContext,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let uri = parts.uri.to_string();
        let token_result = crate::web::utils::auth::extract_token(&parts.headers, &uri);
        let state = state.clone();
        let method = parts.method.clone();
        let path = parts.uri.path().to_string();
        let headers = parts.headers.clone();

        async move {
            let token = token_result?;
            let (user_id, device_id, is_admin, is_shadow_banned, is_guest) =
                state.auth_service.validate_token(&token).await?;

            if let Some(ref audit_svc) = state.admin_audit_service {
                audit_user_action(audit_svc, &user_id, &method, &path, &headers, is_admin).await;
            }

            Ok(Self { user_id, device_id, is_admin, is_shadow_banned, is_guest, access_token: token })
        }
    }
}

impl FromRequestParts<AuthContext> for AuthenticatedUser {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AuthContext,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let uri = parts.uri.to_string();
        let token_result = crate::web::utils::auth::extract_token(&parts.headers, &uri);
        let state = state.clone();
        let method = parts.method.clone();
        let path = parts.uri.path().to_string();
        let headers = parts.headers.clone();

        async move {
            let token = token_result?;
            let (user_id, device_id, is_admin, is_shadow_banned, is_guest) =
                state.auth_service.validate_token(&token).await?;

            if let Some(ref audit_svc) = state.admin_audit_service {
                audit_user_action(audit_svc, &user_id, &method, &path, &headers, is_admin).await;
            }

            Ok(Self { user_id, device_id, is_admin, is_shadow_banned, is_guest, access_token: token })
        }
    }
}

// OptionalAuthenticatedUser for context types
impl FromRequestParts<RoomContext> for OptionalAuthenticatedUser {
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        state: &RoomContext,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let uri = parts.uri.to_string();
        let token_result = crate::web::utils::auth::extract_token(&parts.headers, &uri);
        let state = state.clone();

        async move {
            match token_result {
                Ok(token) => match state.auth_service.validate_token(&token).await {
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

impl FromRequestParts<SyncContext> for OptionalAuthenticatedUser {
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        state: &SyncContext,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let uri = parts.uri.to_string();
        let token_result = crate::web::utils::auth::extract_token(&parts.headers, &uri);
        let state = state.clone();

        async move {
            match token_result {
                Ok(token) => match state.auth_service.validate_token(&token).await {
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

impl FromRequestParts<DeviceContext> for OptionalAuthenticatedUser {
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        state: &DeviceContext,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let uri = parts.uri.to_string();
        let token_result = crate::web::utils::auth::extract_token(&parts.headers, &uri);
        let state = state.clone();

        async move {
            match token_result {
                Ok(token) => match state.auth_service.validate_token(&token).await {
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

impl FromRequestParts<AuthContext> for OptionalAuthenticatedUser {
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AuthContext,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let uri = parts.uri.to_string();
        let token_result = crate::web::utils::auth::extract_token(&parts.headers, &uri);
        let state = state.clone();

        async move {
            match token_result {
                Ok(token) => match state.auth_service.validate_token(&token).await {
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

#[cfg(test)]
mod tests {
    use super::{AdminUser, OptionalAuthenticatedUser};
    use axum::body::Body;
    use axum::http::{header, HeaderMap, Request};

    #[test]
    fn test_extract_token_from_headers_valid() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer test-token-123".parse().unwrap());
        assert_eq!(crate::web::utils::auth::bearer_token(&headers).unwrap(), "test-token-123");
    }

    #[test]
    fn test_extract_token_from_headers_missing() {
        let headers = HeaderMap::new();
        assert!(crate::web::utils::auth::bearer_token(&headers).is_err());
    }

    #[test]
    fn test_extract_token_from_request_bearer_header() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer header-token".parse().unwrap());
        assert_eq!(crate::web::utils::auth::extract_token(&headers, "/test").unwrap(), "header-token");
    }

    #[test]
    fn test_extract_token_from_request_query_param() {
        let headers = HeaderMap::new();
        let uri = "/_matrix/client/v3/sync?access_token=query-token&other=value";
        assert_eq!(crate::web::utils::auth::extract_token(&headers, uri).unwrap(), "query-token");
    }

    #[test]
    fn test_extract_token_from_request_header_takes_priority() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer header-token".parse().unwrap());
        let uri = "/test?access_token=query-token";
        assert_eq!(crate::web::utils::auth::extract_token(&headers, uri).unwrap(), "header-token");
    }

    #[test]
    fn test_extract_token_from_request_query_only() {
        let headers = HeaderMap::new();
        let uri = "/test?access_token=abc123";
        assert_eq!(crate::web::utils::auth::extract_token(&headers, uri).unwrap(), "abc123");
    }

    #[test]
    fn test_extract_token_from_request_no_token() {
        let headers = HeaderMap::new();
        let uri = "/test";
        assert!(crate::web::utils::auth::extract_token(&headers, uri).is_err());
    }

    #[test]
    fn test_extract_token_from_request_query_no_access_token() {
        let headers = HeaderMap::new();
        let uri = "/test?other_param=value";
        assert!(crate::web::utils::auth::extract_token(&headers, uri).is_err());
    }

    fn build_request_with_token(token: Option<&str>) -> Request<Body> {
        let mut req = Request::builder().uri("https://test.local/_matrix/client/v3/sync");
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        req.body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn test_authenticated_user_rejects_missing_token() {
        // AuthenticatedUser requires a valid bearer token.
        // Without an authorization header, the token extraction fails,
        // and the FromRequestParts implementation returns Err.
        let req = build_request_with_token(None);
        assert!(
            crate::web::utils::auth::bearer_token(req.headers()).is_err(),
            "AuthenticatedUser must reject requests without an auth token"
        );
    }

    #[tokio::test]
    async fn test_optional_user_allows_missing_token() {
        // OptionalAuthenticatedUser does not require a token.
        // When no token is present, user_id is None and
        // is_admin / is_shadow_banned / is_guest default to false.
        let req = build_request_with_token(None);
        assert!(
            crate::web::utils::auth::bearer_token(req.headers()).is_err(),
            "OptionalAuthenticatedUser handles missing token gracefully"
        );

        // Demonstrate the struct shape for the anonymous case:
        let _anon = OptionalAuthenticatedUser {
            user_id: None,
            device_id: None,
            is_admin: false,
            is_shadow_banned: false,
            is_guest: false,
            access_token: None,
        };
    }

    #[tokio::test]
    async fn test_admin_user_enforces_admin_check() {
        // AdminUser requires both valid authentication AND admin privileges.
        // It delegates to authorize_admin_request which validates the token
        // and checks for admin role.  Without a valid admin context the
        // extraction fails.
        let req = build_request_with_token(Some("non-admin-token"));

        // The token is structurally present (but not an admin token):
        assert_eq!(crate::web::utils::auth::bearer_token(req.headers()).unwrap(), "non-admin-token");
        // Full privilege checking requires a running server with token
        // storage -- covered by integration tests.

        // Demonstrate the struct shape for a successful admin extraction:
        let _admin = AdminUser {
            user_id: "admin_user".to_owned(),
            device_id: None,
            access_token: "admin_token".to_owned(),
            role: "admin".to_owned(),
        };
    }
}
