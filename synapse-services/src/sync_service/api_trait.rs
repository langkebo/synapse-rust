use async_trait::async_trait;
use synapse_common::ApiResult;

use super::types::SyncServiceRequest;
use super::SyncService;

#[async_trait]
pub trait SyncServiceApi: Send + Sync {
    #[allow(clippy::too_many_arguments)]
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

    async fn sync_with_request(&self, request: SyncServiceRequest<'_>) -> ApiResult<serde_json::Value>;

    async fn room_sync(
        &self,
        user_id: &str,
        room_id: &str,
        timeout: u64,
        is_full_state: bool,
        since: Option<&str>,
    ) -> ApiResult<serde_json::Value>;

    async fn room_sync_with_timeout(
        &self,
        user_id: &str,
        room_id: &str,
        timeout: u64,
        is_full_state: bool,
        since: Option<&str>,
    ) -> ApiResult<serde_json::Value>;

    async fn room_unread_counts(&self, room_id: &str, user_id: &str) -> ApiResult<(i64, i64)>;

    async fn get_room_messages(
        &self,
        room_id: &str,
        user_id: &str,
        from: &str,
        limit: i64,
        dir: &str,
    ) -> ApiResult<serde_json::Value>;

    async fn get_public_rooms(&self, limit: i64, since: Option<&str>) -> ApiResult<serde_json::Value>;

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

    async fn get_room_messages(
        &self,
        room_id: &str,
        user_id: &str,
        from: &str,
        limit: i64,
        dir: &str,
    ) -> ApiResult<serde_json::Value> {
        self.get_room_messages(room_id, user_id, from, limit, dir).await
    }

    async fn get_public_rooms(&self, limit: i64, since: Option<&str>) -> ApiResult<serde_json::Value> {
        self.get_public_rooms(limit, since).await
    }

    async fn get_events(&self, user_id: &str, from: &str, timeout: u64) -> ApiResult<serde_json::Value> {
        self.get_events(user_id, from, timeout).await
    }
}
