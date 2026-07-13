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

    async fn add_threepid(&self, _request: CreateThreepidRequest) -> Result<UserThreepid, ApiError> {
        Err(ApiError::internal("InMemoryThreepidStore does not support add_threepid"))
    }

    async fn verify_threepid(&self, _user_id: &str, _medium: &str, _address: &str) -> Result<bool, ApiError> {
        Err(ApiError::internal("InMemoryThreepidStore does not support verify_threepid"))
    }

    async fn create_validation_session(
        &self,
        _session_id: &str,
        _medium: &str,
        _address: &str,
        _client_secret: &str,
        _token: &str,
        _next_link: Option<&str>,
        _created_ts: i64,
        _expires_at: i64,
    ) -> Result<i64, ApiError> {
        Err(ApiError::internal("InMemoryThreepidStore does not support create_validation_session"))
    }

    async fn get_validation_session(
        &self,
        _session_id: &str,
        _client_secret: &str,
        _token: &str,
    ) -> Result<Option<ThreepidValidationSession>, ApiError> {
        Err(ApiError::internal("InMemoryThreepidStore does not support get_validation_session"))
    }

    async fn mark_validation_validated(&self, _id: i64) -> Result<(), ApiError> {
        Err(ApiError::internal("InMemoryThreepidStore does not support mark_validation_validated"))
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
