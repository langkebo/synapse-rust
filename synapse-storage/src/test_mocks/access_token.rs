use super::*;
use crate::token::{AccessToken, AccessTokenStoreApi};
use std::collections::HashSet;

/// In-memory test double for [`AccessTokenStoreApi`].
#[derive(Clone, Default)]
pub struct InMemoryAccessTokenStore {
    tokens: Arc<tokio::sync::RwLock<HashMap<i64, AccessToken>>>,
    /// Token hashes that have been blacklisted.
    blacklist: Arc<tokio::sync::RwLock<HashSet<String>>>,
    /// Monotonic ID counter for `create_token`.
    next_id: Arc<std::sync::atomic::AtomicI64>,
}

impl InMemoryAccessTokenStore {
    /// See [`new`].
    pub fn new() -> Self {
        Self::default()
    }

    /// See [`seed_token`].
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

    fn hash_token(token: &str) -> String {
        synapse_common::crypto::hash_token(token)
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

    async fn cleanup_expired_tokens(&self) -> Result<u64, sqlx::Error> {
        // In-memory store has no expiration concept
        Ok(0)
    }

    async fn create_token(
        &self,
        token: &str,
        user_id: &str,
        device_id: Option<&str>,
        expires_at: Option<i64>,
    ) -> Result<AccessToken, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let token_hash = Self::hash_token(token);
        let access_token = AccessToken {
            id,
            token_hash,
            user_id: user_id.to_string(),
            device_id: device_id.map(|d| d.to_string()),
            created_ts: synapse_common::current_timestamp_millis(),
            expires_at,
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: false,
        };
        let mut tokens = self.tokens.write().await;
        tokens.insert(id, access_token.clone());
        Ok(access_token)
    }

    async fn delete_token(&self, token: &str) -> Result<(), sqlx::Error> {
        let hash = Self::hash_token(token);
        let mut tokens = self.tokens.write().await;
        for t in tokens.values_mut() {
            if t.token_hash == hash {
                t.is_revoked = true;
            }
        }
        Ok(())
    }

    async fn delete_user_tokens(&self, user_id: &str) -> Result<(), sqlx::Error> {
        let mut tokens = self.tokens.write().await;
        for t in tokens.values_mut() {
            if t.user_id == user_id {
                t.is_revoked = true;
            }
        }
        Ok(())
    }

    async fn delete_device_tokens(&self, device_id: &str) -> Result<(), sqlx::Error> {
        let mut tokens = self.tokens.write().await;
        for t in tokens.values_mut() {
            if t.device_id.as_deref() == Some(device_id) {
                t.is_revoked = true;
            }
        }
        Ok(())
    }

    async fn delete_user_device_tokens(&self, user_id: &str, device_id: &str) -> Result<(), sqlx::Error> {
        let mut tokens = self.tokens.write().await;
        for t in tokens.values_mut() {
            if t.user_id == user_id && t.device_id.as_deref() == Some(device_id) {
                t.is_revoked = true;
            }
        }
        Ok(())
    }

    async fn delete_user_tokens_except_device(&self, user_id: &str, device_id: &str) -> Result<(), sqlx::Error> {
        let mut tokens = self.tokens.write().await;
        for t in tokens.values_mut() {
            if t.user_id == user_id && t.device_id.as_deref() != Some(device_id) {
                t.is_revoked = true;
            }
        }
        Ok(())
    }

    async fn is_token_revoked(&self, token: &str) -> Result<bool, sqlx::Error> {
        let hash = Self::hash_token(token);
        let tokens = self.tokens.read().await;
        Ok(tokens.values().any(|t| t.token_hash == hash && t.is_revoked))
    }

    async fn add_to_blacklist(&self, token: &str, _user_id: &str, _reason: Option<&str>) -> Result<(), sqlx::Error> {
        let hash = Self::hash_token(token);
        let mut blacklist = self.blacklist.write().await;
        blacklist.insert(hash);
        Ok(())
    }

    async fn add_hash_to_blacklist(
        &self,
        token_hash: &str,
        _user_id: &str,
        _reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let mut blacklist = self.blacklist.write().await;
        blacklist.insert(token_hash.to_string());
        Ok(())
    }

    async fn is_in_blacklist(&self, token: &str) -> Result<bool, sqlx::Error> {
        let hash = Self::hash_token(token);
        let blacklist = self.blacklist.read().await;
        Ok(blacklist.contains(&hash))
    }
}
