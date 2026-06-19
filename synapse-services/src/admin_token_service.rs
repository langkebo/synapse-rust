use crate::registration_token_service::RegistrationTokenService;
use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::refresh_token::RefreshTokenStorage;
use synapse_storage::registration_token::{
    CreateRegistrationTokenRequest, RegistrationToken, UpdateRegistrationTokenRequest,
};
use synapse_storage::token::AccessTokenStorage;
use tracing::instrument;

#[derive(Debug, Clone)]
pub struct AdminAccessTokenInfo {
    pub id: i64,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
    pub is_revoked: bool,
}

#[derive(Debug, Clone)]
pub struct AdminRefreshTokenInfo {
    pub id: i64,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
    pub is_revoked: bool,
}

pub struct AdminTokenService {
    token_storage: AccessTokenStorage,
    refresh_token_storage: Arc<RefreshTokenStorage>,
    registration_token_service: Arc<RegistrationTokenService>,
}

impl AdminTokenService {
    pub fn new(
        token_storage: AccessTokenStorage,
        refresh_token_storage: Arc<RefreshTokenStorage>,
        registration_token_service: Arc<RegistrationTokenService>,
    ) -> Self {
        Self { token_storage, refresh_token_storage, registration_token_service }
    }

    #[instrument(skip(self))]
    pub async fn create_registration_token(
        &self,
        token: Option<String>,
        max_uses: i32,
        expires_at: Option<i64>,
        created_by: &str,
    ) -> Result<RegistrationToken, ApiError> {
        self.registration_token_service
            .create_token(CreateRegistrationTokenRequest {
                token,
                token_type: None,
                description: None,
                max_uses: Some(max_uses),
                expires_at,
                created_by: Some(created_by.to_owned()),
                allowed_email_domains: None,
                allowed_user_ids: None,
                auto_join_rooms: None,
                display_name: None,
                email: None,
            })
            .await
    }

    #[instrument(skip(self))]
    pub async fn get_registration_token(&self, token: &str) -> Result<Option<RegistrationToken>, ApiError> {
        self.registration_token_service.get_token(token).await
    }

    #[instrument(skip(self))]
    pub async fn delete_registration_token(&self, token: &str) -> Result<(), ApiError> {
        let existing = self
            .registration_token_service
            .get_token(token)
            .await?
            .ok_or_else(|| ApiError::not_found("Token not found".to_string()))?;

        self.registration_token_service.delete_token(existing.id).await
    }

    #[instrument(skip(self))]
    pub async fn update_registration_token(
        &self,
        token: &str,
        max_uses: Option<i32>,
        expires_at: Option<i64>,
    ) -> Result<RegistrationToken, ApiError> {
        let existing = self
            .registration_token_service
            .get_token(token)
            .await?
            .ok_or_else(|| ApiError::not_found("Token not found".to_string()))?;

        self.registration_token_service
            .update_token(
                existing.id,
                UpdateRegistrationTokenRequest { description: None, max_uses, is_enabled: None, expires_at },
            )
            .await
    }

    #[instrument(skip(self))]
    pub async fn get_user_access_tokens(&self, user_id: &str) -> Result<Vec<AdminAccessTokenInfo>, ApiError> {
        let tokens = self
            .token_storage
            .get_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(tokens
            .into_iter()
            .map(|token| AdminAccessTokenInfo {
                id: token.id,
                device_id: token.device_id,
                created_ts: token.created_ts,
                expires_at: token.expires_at,
                is_revoked: token.is_revoked,
            })
            .collect())
    }

    #[instrument(skip(self))]
    pub async fn delete_user_access_token(&self, user_id: &str, token_id: i64) -> Result<(), ApiError> {
        let deleted = self
            .token_storage
            .delete_user_token_by_id(user_id, token_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if !deleted {
            return Err(ApiError::not_found("Token not found".to_string()));
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_user_refresh_tokens(&self, user_id: &str) -> Result<Vec<AdminRefreshTokenInfo>, ApiError> {
        let tokens = self
            .refresh_token_storage
            .get_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(tokens
            .into_iter()
            .map(|token| AdminRefreshTokenInfo {
                id: token.id,
                device_id: token.device_id,
                created_ts: token.created_ts,
                expires_at: token.expires_at,
                is_revoked: token.is_revoked,
            })
            .collect())
    }

    #[instrument(skip(self))]
    pub async fn delete_refresh_token(&self, user_id: &str, token_id: i64) -> Result<(), ApiError> {
        let token = self
            .refresh_token_storage
            .get_token_by_id(token_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?
            .ok_or_else(|| ApiError::not_found("Refresh token not found".to_string()))?;

        if token.user_id != user_id {
            return Err(ApiError::not_found("Refresh token not found".to_string()));
        }

        self.refresh_token_storage
            .delete_token(&token.token_hash)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(())
    }
}
