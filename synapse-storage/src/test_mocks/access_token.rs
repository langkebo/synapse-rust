use super::*;
use crate::token::{AccessToken, AccessTokenStoreApi};

/// In-memory test double for [`AccessTokenStoreApi`].
#[derive(Clone, Default)]
pub struct InMemoryAccessTokenStore {
    tokens: Arc<tokio::sync::RwLock<HashMap<i64, AccessToken>>>,
}

impl InMemoryAccessTokenStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn seed_token(&self, user_id: &str, token_id: i64, device_id: Option<&str>) {
        let mut tokens = self.tokens.write().await;
        tokens.insert(
            token_id,
            AccessToken {
                id: token_id,
                token_hash: format!("hash_{token_id}"),
                user_id: user_id.to_string(),
                device_id: device_id.map(|d| d.to_string()),
                created_ts: 1_700_000_000_000,
                expires_at: None,
                last_used_ts: None,
                user_agent: None,
                ip_address: None,
                is_revoked: false,
            },
        );
    }
}

#[async_trait::async_trait]
impl AccessTokenStoreApi for InMemoryAccessTokenStore {
    async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<AccessToken>, sqlx::Error> {
        let tokens = self.tokens.read().await;
        Ok(tokens.values().filter(|t| t.user_id == user_id).cloned().collect())
    }

    async fn delete_user_token_by_id(&self, user_id: &str, token_id: i64) -> Result<bool, sqlx::Error> {
        let mut tokens = self.tokens.write().await;
        if let Some(token) = tokens.get(&token_id) {
            if token.user_id == user_id {
                tokens.remove(&token_id);
                return Ok(true);
            }
        }
        Ok(false)
    }
}
