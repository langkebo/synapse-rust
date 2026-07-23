use super::*;
use synapse_common::current_timestamp_millis;

use std::sync::atomic::{AtomicI64, Ordering};

use crate::openid_token::{CreateOpenIdTokenRequest, OpenIdToken, OpenIdTokenStoreApi};

/// In-memory [`OpenIdTokenStoreApi`] backed by a `HashMap` keyed on the token
/// string. Mirrors the production `is_valid` / `expires_at` semantics so that
/// `get_token` / `validate_token` filter correctly.
#[derive(Clone, Debug, Default)]
pub struct InMemoryOpenIdTokenStore {
    tokens: Arc<RwLock<HashMap<String, OpenIdToken>>>,
    next_id: Arc<AtomicI64>,
}

impl InMemoryOpenIdTokenStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl OpenIdTokenStoreApi for InMemoryOpenIdTokenStore {
    async fn create_token(&self, request: CreateOpenIdTokenRequest) -> Result<OpenIdToken, ApiError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let token = OpenIdToken {
            id,
            token: request.token.clone(),
            user_id: request.user_id,
            device_id: request.device_id,
            created_ts: current_timestamp_millis(),
            expires_at: request.expires_at,
            is_valid: true,
        };
        self.tokens.write().await.insert(request.token, token.clone());
        Ok(token)
    }

    async fn get_token(&self, token: &str) -> Result<Option<OpenIdToken>, ApiError> {
        Ok(self.tokens.read().await.get(token).filter(|t| t.is_valid).cloned())
    }

    async fn validate_token(&self, token: &str) -> Result<Option<OpenIdToken>, ApiError> {
        let now = current_timestamp_millis();
        Ok(self.tokens.read().await.get(token).filter(|t| t.is_valid && t.expires_at > now).cloned())
    }

    async fn revoke_token(&self, token: &str) -> Result<bool, ApiError> {
        let mut tokens = self.tokens.write().await;
        match tokens.get_mut(token) {
            Some(t) => {
                t.is_valid = false;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn revoke_user_tokens(&self, user_id: &str) -> Result<u64, ApiError> {
        let mut count = 0u64;
        let mut tokens = self.tokens.write().await;
        for t in tokens.values_mut() {
            if t.user_id == user_id && t.is_valid {
                t.is_valid = false;
                count += 1;
            }
        }
        Ok(count)
    }

    async fn cleanup_expired_tokens(&self) -> Result<u64, ApiError> {
        let now = current_timestamp_millis();
        let mut tokens = self.tokens.write().await;
        let before = tokens.len();
        tokens.retain(|_, t| t.expires_at >= now && t.is_valid);
        Ok((before - tokens.len()) as u64)
    }

    async fn get_tokens_by_user(&self, user_id: &str) -> Result<Vec<OpenIdToken>, ApiError> {
        let mut tokens: Vec<_> = self.tokens.read().await.values().filter(|t| t.user_id == user_id).cloned().collect();
        tokens.sort_by(|a, b| b.created_ts.cmp(&a.created_ts));
        Ok(tokens)
    }
}
