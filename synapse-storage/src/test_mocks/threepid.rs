use super::*;

#[derive(Clone, Default)]
pub struct InMemoryThreepidStore {
    threepids: Arc<tokio::sync::RwLock<Vec<UserThreepid>>>,
    next_id: Arc<tokio::sync::RwLock<i64>>,
}

impl InMemoryThreepidStore {
    pub fn new() -> Self {
        Self {
            threepids: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            next_id: Arc::new(tokio::sync::RwLock::new(1)),
        }
    }
}

#[async_trait::async_trait]
impl ThreepidStoreApi for InMemoryThreepidStore {
    async fn get_verified_threepid_by_address(
        &self,
        medium: &str,
        address: &str,
    ) -> Result<Option<UserThreepid>, ApiError> {
        Ok(self
            .threepids
            .read()
            .await
            .iter()
            .find(|t| t.medium == medium && t.address == address && t.is_verified)
            .cloned())
    }

    async fn get_threepids_by_user(&self, user_id: &str) -> Result<Vec<UserThreepid>, ApiError> {
        Ok(self.threepids.read().await.iter().filter(|t| t.user_id == user_id).cloned().collect())
    }

    async fn add_verified_threepid(
        &self,
        user_id: &str,
        medium: &str,
        address: &str,
        validated_at: i64,
        added_ts: i64,
    ) -> Result<u64, ApiError> {
        let mut threepids = self.threepids.write().await;
        let id = *self.next_id.read().await;
        *self.next_id.write().await = id + 1;
        threepids.push(UserThreepid {
            id,
            user_id: user_id.to_string(),
            medium: medium.to_string(),
            address: address.to_string(),
            validated_at: Some(validated_at),
            added_ts,
            is_verified: true,
            verification_token: None,
            verification_expires_at: None,
        });
        Ok(1)
    }

    async fn remove_threepid(&self, user_id: &str, medium: &str, address: &str) -> Result<bool, ApiError> {
        let mut threepids = self.threepids.write().await;
        let before = threepids.len();
        threepids.retain(|t| !(t.user_id == user_id && t.medium == medium && t.address == address));
        Ok(threepids.len() < before)
    }
}

impl InMemoryThreepidStore {
    /// Seed a verified threepid for tests.
    pub async fn seed_threepid(&self, user_id: &str, medium: &str, address: &str) {
        let mut threepids = self.threepids.write().await;
        let id = threepids.len() as i64 + 1;
        threepids.push(UserThreepid {
            id,
            user_id: user_id.to_string(),
            medium: medium.to_string(),
            address: address.to_string(),
            validated_at: Some(chrono::Utc::now().timestamp_millis()),
            added_ts: chrono::Utc::now().timestamp_millis(),
            is_verified: true,
            verification_token: None,
            verification_expires_at: None,
        });
    }
}

// =============================================================================
// InMemoryAccessTokenStore
// =============================================================================
