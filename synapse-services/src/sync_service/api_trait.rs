use async_trait::async_trait;
use synapse_common::ApiResult;

use super::types::SyncServiceRequest;
use super::SyncService;

/// The `SyncServiceApi` trait.
#[async_trait]
pub trait SyncServiceApi: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    /// See [`sync`].
    async fn sync(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        timeout: u64,
        full_state: bool,
        set_presence: &str,
        filter_id: Option<&str>,
        since: Option<&str>,
    ) -> ApiResult<serde_json::Value>;

    /// See [`sync_with_request`].
    async fn sync_with_request(&self, request: SyncServiceRequest<'_>) -> ApiResult<serde_json::Value>;

    /// See [`room_sync`].
    async fn room_sync(
        &self,
        user_id: &str,
        room_id: &str,
        timeout: u64,
        is_full_state: bool,
        since: Option<&str>,
    ) -> ApiResult<serde_json::Value>;

    /// See [`room_sync_with_timeout`].
    async fn room_sync_with_timeout(
        &self,
        user_id: &str,
        room_id: &str,
        timeout: u64,
        is_full_state: bool,
        since: Option<&str>,
    ) -> ApiResult<serde_json::Value>;

    /// See [`room_unread_counts`].
    async fn room_unread_counts(&self, room_id: &str, user_id: &str) -> ApiResult<(i64, i64)>;

    /// See [`get_events`].
    async fn get_events(&self, user_id: &str, from: &str, timeout: u64) -> ApiResult<serde_json::Value>;
}

#[async_trait]
impl SyncServiceApi for SyncService {
    async fn sync(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        timeout: u64,
        full_state: bool,
        set_presence: &str,
        filter_id: Option<&str>,
        since: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        self.sync(user_id, device_id, timeout, full_state, set_presence, filter_id, since).await
    }

    async fn sync_with_request(&self, request: SyncServiceRequest<'_>) -> ApiResult<serde_json::Value> {
        self.sync_with_request(request).await
    }

    async fn room_sync(
        &self,
        user_id: &str,
        room_id: &str,
        timeout: u64,
        is_full_state: bool,
        since: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        self.room_sync(user_id, room_id, timeout, is_full_state, since).await
    }

    async fn room_sync_with_timeout(
        &self,
        user_id: &str,
        room_id: &str,
        timeout: u64,
        is_full_state: bool,
        since: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        self.room_sync_with_timeout(user_id, room_id, timeout, is_full_state, since).await
    }

    async fn room_unread_counts(&self, room_id: &str, user_id: &str) -> ApiResult<(i64, i64)> {
        self.room_unread_counts(room_id, user_id).await
    }

    async fn get_events(&self, user_id: &str, from: &str, timeout: u64) -> ApiResult<serde_json::Value> {
        self.get_events(user_id, from, timeout).await
    }
}
