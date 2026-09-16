use std::collections::HashMap;
use std::sync::Arc;

use crate::login_token::{LoginToken, LoginTokenStoreApi};
use synapse_common::current_timestamp_millis;
use tokio::sync::RwLock;

/// A pending token: `(user_id, device_id, expires_at)`.
type TokenEntry = (String, Option<String>, i64);

/// In-memory [`LoginTokenStoreApi`] mirroring the `login_tokens` single-use
/// semantics (a token is removed on the first consume, expired tokens are
/// dropped instead of returned).
pub struct InMemoryLoginTokenStore {
    tokens: Arc<RwLock<HashMap<String, TokenEntry>>>,
}

impl Default for InMemoryLoginTokenStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryLoginTokenStore {
    /// See [`new`].
    pub fn new() -> Self {
        Self { tokens: Arc::new(RwLock::new(HashMap::new())) }
    }
}

#[async_trait::async_trait]
impl LoginTokenStoreApi for InMemoryLoginTokenStore {
    async fn create_login_token(
        &self,
        token: &str,
        user_id: &str,
        device_id: Option<&str>,
        expires_at: i64,
    ) -> Result<(), sqlx::Error> {
        self.tokens
            .write()
            .await
            .insert(token.to_string(), (user_id.to_string(), device_id.map(str::to_string), expires_at));
        Ok(())
    }

    async fn consume_login_token(&self, token: &str) -> Result<Option<LoginToken>, sqlx::Error> {
        let now = current_timestamp_millis();
        let removed = self.tokens.write().await.remove(token);
        Ok(removed.filter(|(_, _, expires_at)| *expires_at > now).map(|(user_id, device_id, _)| LoginToken {
            id: 0,
            token: token.to_string(),
            user_id,
            device_id,
            created_ts: 0,
            expires_at: 0,
        }))
    }

    async fn cleanup_expired_tokens(&self, _now_ts: i64) -> Result<u64, sqlx::Error> {
        Ok(0)
    }
}
