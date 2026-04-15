use crate::common::ApiError;
use crate::web::routes::AppState;
use crate::web::utils::admin_auth::authorize_admin_request;
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, HeaderMap},
};

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
        let token_result = extract_token_from_headers(&parts.headers);
        let state = state.clone();

        async move {
            let token = token_result?;
            let result = state.services.auth_service.validate_token(&token).await;
            match result {
                Ok((user_id, device_id, is_admin, is_shadow_banned, is_guest)) => {
                    Ok(AuthenticatedUser {
                        user_id,
                        device_id,
                        is_admin,
                        is_shadow_banned,
                        is_guest,
                        access_token: token,
                    })
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
            Ok(AdminUser {
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
        let token_result = extract_token_from_headers(&parts.headers);
        let state = state.clone();

        async move {
            match token_result {
                Ok(token) => match state.services.auth_service.validate_token(&token).await {
                    Ok((user_id, device_id, is_admin, is_shadow_banned, is_guest)) => {
                        Ok(OptionalAuthenticatedUser {
                            user_id: Some(user_id),
                            device_id,
                            is_admin,
                            is_shadow_banned,
                            is_guest,
                            access_token: Some(token),
                        })
                    }
                    Err(_) => Ok(OptionalAuthenticatedUser {
                        user_id: None,
                        device_id: None,
                        is_admin: false,
                        is_shadow_banned: false,
                        is_guest: false,
                        access_token: None,
                    }),
                },
                Err(_) => Ok(OptionalAuthenticatedUser {
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
    fn extract_token(&self) -> Result<String, ApiError>;
}

impl AuthExtractor for HeaderMap {
    fn extract_token(&self) -> Result<String, ApiError> {
        extract_token_from_headers(self)
    }
}

pub(crate) fn extract_token_from_headers(headers: &HeaderMap) -> Result<String, ApiError> {
    crate::web::utils::auth::bearer_token(headers)
}
