use crate::registration_token_service::RegistrationTokenApi;
use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::refresh_token::RefreshTokenStoreApi;
use synapse_storage::registration_token::{
    CreateRegistrationTokenRequest, RegistrationToken, UpdateRegistrationTokenRequest,
};
use synapse_storage::token::AccessTokenStoreApi;
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
    token_storage: Arc<dyn AccessTokenStoreApi>,
    refresh_token_storage: Arc<dyn RefreshTokenStoreApi>,
    registration_token_service: Arc<dyn RegistrationTokenApi>,
}

impl AdminTokenService {
    pub fn new(
        token_storage: Arc<dyn AccessTokenStoreApi>,
        refresh_token_storage: Arc<dyn RefreshTokenStoreApi>,
        registration_token_service: Arc<dyn RegistrationTokenApi>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::InMemoryRegistrationTokenService;
    use synapse_storage::test_mocks::{InMemoryAccessTokenStore, InMemoryRefreshTokenStore};

    fn test_service() -> AdminTokenService {
        AdminTokenService::new(
            Arc::new(InMemoryAccessTokenStore::new()),
            Arc::new(InMemoryRefreshTokenStore::new()),
            Arc::new(InMemoryRegistrationTokenService::new()),
        )
    }

    fn test_service_with(
        token_store: InMemoryAccessTokenStore,
        refresh_store: InMemoryRefreshTokenStore,
        reg_svc: InMemoryRegistrationTokenService,
    ) -> AdminTokenService {
        AdminTokenService::new(Arc::new(token_store), Arc::new(refresh_store), Arc::new(reg_svc))
    }

    // ── create_registration_token ──────────────────────────────────────

    #[tokio::test]
    async fn create_registration_token_returns_created_token() {
        let svc = test_service();
        let result = svc.create_registration_token(None, 10, None, "@admin:example.com").await.unwrap();
        assert_eq!(result.max_uses, 10);
        assert!(result.token.starts_with("auto_token_"));
    }

    #[tokio::test]
    async fn create_registration_token_with_custom_token() {
        let svc = test_service();
        let result =
            svc.create_registration_token(Some("custom_token".into()), 5, None, "@admin:example.com").await.unwrap();
        assert_eq!(result.token, "custom_token");
        assert_eq!(result.max_uses, 5);
    }

    // ── get_registration_token ────────────────────────────────────────

    #[tokio::test]
    async fn get_registration_token_returns_none_for_unknown() {
        let svc = test_service();
        assert!(svc.get_registration_token("unknown").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_registration_token_returns_created_token() {
        let svc = test_service();
        let created =
            svc.create_registration_token(Some("find_me".into()), 3, None, "@admin:example.com").await.unwrap();
        let found = svc.get_registration_token("find_me").await.unwrap();
        assert_eq!(found.unwrap().id, created.id);
    }

    // ── delete_registration_token ─────────────────────────────────────

    #[tokio::test]
    async fn delete_registration_token_errors_for_unknown() {
        let svc = test_service();
        let err = svc.delete_registration_token("unknown").await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn delete_registration_token_succeeds_for_existing() {
        let svc = test_service();
        svc.create_registration_token(Some("to_delete".into()), 1, None, "@admin:example.com").await.unwrap();
        assert!(svc.delete_registration_token("to_delete").await.is_ok());
        assert!(svc.get_registration_token("to_delete").await.unwrap().is_none());
    }

    // ── update_registration_token ──────────────────────────────────────

    #[tokio::test]
    async fn update_registration_token_updates_max_uses() {
        let svc = test_service();
        svc.create_registration_token(Some("updatable".into()), 1, None, "@admin:example.com").await.unwrap();
        let updated = svc.update_registration_token("updatable", Some(20), None).await.unwrap();
        assert_eq!(updated.max_uses, 20);
    }

    #[tokio::test]
    async fn update_registration_token_errors_for_unknown() {
        let svc = test_service();
        let err = svc.update_registration_token("unknown", Some(10), None).await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    // ── get_user_access_tokens ─────────────────────────────────────────

    #[tokio::test]
    async fn get_user_access_tokens_returns_seeded() {
        let token_store = InMemoryAccessTokenStore::new();
        token_store.seed_token("@alice:example.com", 1, Some("DEV1")).await;
        token_store.seed_token("@alice:example.com", 2, None).await;
        let svc =
            test_service_with(token_store, InMemoryRefreshTokenStore::new(), InMemoryRegistrationTokenService::new());

        let tokens = svc.get_user_access_tokens("@alice:example.com").await.unwrap();
        assert_eq!(tokens.len(), 2);
        let has_dev1 = tokens.iter().any(|t| t.device_id.as_deref() == Some("DEV1"));
        assert!(has_dev1);
    }

    #[tokio::test]
    async fn get_user_access_tokens_returns_empty_for_unknown() {
        let svc = test_service();
        let tokens = svc.get_user_access_tokens("@unknown:example.com").await.unwrap();
        assert!(tokens.is_empty());
    }

    // ── delete_user_access_token ───────────────────────────────────────

    #[tokio::test]
    async fn delete_user_access_token_succeeds_for_owned() {
        let token_store = InMemoryAccessTokenStore::new();
        token_store.seed_token("@alice:example.com", 1, None).await;
        let svc =
            test_service_with(token_store, InMemoryRefreshTokenStore::new(), InMemoryRegistrationTokenService::new());

        svc.delete_user_access_token("@alice:example.com", 1).await.unwrap();
        assert!(svc.get_user_access_tokens("@alice:example.com").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_user_access_token_errors_when_not_found() {
        let svc = test_service();
        let err = svc.delete_user_access_token("@alice:example.com", 999).await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    // ── get_user_refresh_tokens ────────────────────────────────────────

    #[tokio::test]
    async fn get_user_refresh_tokens_returns_seeded() {
        let refresh_store = InMemoryRefreshTokenStore::new();
        refresh_store.seed_token("@alice:example.com", 1, "hash_1", Some("DEV1")).await;
        refresh_store.seed_token("@alice:example.com", 2, "hash_2", None).await;
        let svc =
            test_service_with(InMemoryAccessTokenStore::new(), refresh_store, InMemoryRegistrationTokenService::new());

        let tokens = svc.get_user_refresh_tokens("@alice:example.com").await.unwrap();
        assert_eq!(tokens.len(), 2);
    }

    #[tokio::test]
    async fn get_user_refresh_tokens_returns_empty_for_unknown() {
        let svc = test_service();
        let tokens = svc.get_user_refresh_tokens("@unknown:example.com").await.unwrap();
        assert!(tokens.is_empty());
    }

    // ── delete_refresh_token ───────────────────────────────────────────

    #[tokio::test]
    async fn delete_refresh_token_succeeds_for_owner() {
        let refresh_store = InMemoryRefreshTokenStore::new();
        refresh_store.seed_token("@alice:example.com", 1, "hash_1", None).await;
        let svc =
            test_service_with(InMemoryAccessTokenStore::new(), refresh_store, InMemoryRegistrationTokenService::new());

        svc.delete_refresh_token("@alice:example.com", 1).await.unwrap();
    }

    #[tokio::test]
    async fn delete_refresh_token_errors_for_wrong_user() {
        let refresh_store = InMemoryRefreshTokenStore::new();
        refresh_store.seed_token("@alice:example.com", 1, "hash_1", None).await;
        let svc =
            test_service_with(InMemoryAccessTokenStore::new(), refresh_store, InMemoryRegistrationTokenService::new());

        let err = svc.delete_refresh_token("@bob:example.com", 1).await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
