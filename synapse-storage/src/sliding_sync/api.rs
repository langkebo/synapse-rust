use super::models::*;
use super::repository::SlidingSyncStorage;
use async_trait::async_trait;

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait SlidingSyncStoreApi: Send + Sync {
    async fn create_or_update_token(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<SlidingSyncToken, sqlx::Error>;
    async fn get_token(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncToken>, sqlx::Error>;
    async fn validate_pos(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        pos: &str,
    ) -> Result<bool, sqlx::Error>;
    async fn save_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        sort: &[String],
        filters: Option<&SlidingSyncFilters>,
        room_subscription: Option<&serde_json::Value>,
        ranges: &[(u32, u32)],
    ) -> Result<SlidingSyncList, sqlx::Error>;
    async fn get_lists(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Vec<SlidingSyncList>, sqlx::Error>;
    async fn delete_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
    ) -> Result<(), sqlx::Error>;
    async fn upsert_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        list_key: Option<&str>,
        bump_stamp: i64,
        highlight_count: i32,
        notification_count: i32,
        is_dm: bool,
        is_encrypted: bool,
        is_tombstoned: bool,
        invited: bool,
        name: Option<&str>,
        avatar: Option<&str>,
        timestamp: i64,
    ) -> Result<SlidingSyncRoom, sqlx::Error>;
    async fn get_rooms_for_list(
        &self,
        query_params: SlidingSyncListQuery<'_>,
    ) -> Result<Vec<SlidingSyncRoom>, sqlx::Error>;
    async fn count_rooms_for_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        filters: Option<&SlidingSyncFilters>,
    ) -> Result<i64, sqlx::Error>;
    async fn get_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncRoom>, sqlx::Error>;
    async fn materialize_room_from_activity(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncRoom>, sqlx::Error>;
    async fn delete_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), sqlx::Error>;
    async fn update_notification_counts(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        highlight_count: i32,
        notification_count: i32,
    ) -> Result<(), sqlx::Error>;
    async fn bump_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        bump_stamp: i64,
    ) -> Result<(), sqlx::Error>;
    async fn cleanup_expired_tokens(&self) -> Result<u64, sqlx::Error>;
    async fn list_room_token_sync(
        &self,
        room_id: &str,
        limit: i64,
        from: Option<&RoomTokenSyncCursor>,
    ) -> Result<Vec<AdminRoomTokenSyncEntry>, sqlx::Error>;
    async fn count_room_token_sync(&self, room_id: &str) -> Result<i64, sqlx::Error>;
    async fn get_global_account_data(&self, user_id: &str) -> Result<serde_json::Value, sqlx::Error>;
    async fn get_room_account_data(&self, user_id: &str, room_ids: &[String])
        -> Result<serde_json::Value, sqlx::Error>;
    async fn get_receipts_for_rooms(&self, room_ids: &[String]) -> Result<serde_json::Value, sqlx::Error>;
    async fn delete_connection_data(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), sqlx::Error>;
}

#[async_trait]
impl SlidingSyncStoreApi for SlidingSyncStorage {
    async fn create_or_update_token(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<SlidingSyncToken, sqlx::Error> {
        self.create_or_update_token(user_id, device_id, conn_id).await
    }
    async fn get_token(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncToken>, sqlx::Error> {
        self.get_token(user_id, device_id, conn_id).await
    }
    async fn validate_pos(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        pos: &str,
    ) -> Result<bool, sqlx::Error> {
        self.validate_pos(user_id, device_id, conn_id, pos).await
    }
    async fn save_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        sort: &[String],
        filters: Option<&SlidingSyncFilters>,
        room_subscription: Option<&serde_json::Value>,
        ranges: &[(u32, u32)],
    ) -> Result<SlidingSyncList, sqlx::Error> {
        self.save_list(user_id, device_id, conn_id, list_key, sort, filters, room_subscription, ranges).await
    }
    async fn get_lists(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Vec<SlidingSyncList>, sqlx::Error> {
        self.get_lists(user_id, device_id, conn_id).await
    }
    async fn delete_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
    ) -> Result<(), sqlx::Error> {
        self.delete_list(user_id, device_id, conn_id, list_key).await
    }
    async fn upsert_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        list_key: Option<&str>,
        bump_stamp: i64,
        highlight_count: i32,
        notification_count: i32,
        is_dm: bool,
        is_encrypted: bool,
        is_tombstoned: bool,
        invited: bool,
        name: Option<&str>,
        avatar: Option<&str>,
        timestamp: i64,
    ) -> Result<SlidingSyncRoom, sqlx::Error> {
        self.upsert_room(
            user_id,
            device_id,
            room_id,
            conn_id,
            list_key,
            bump_stamp,
            highlight_count,
            notification_count,
            is_dm,
            is_encrypted,
            is_tombstoned,
            invited,
            name,
            avatar,
            timestamp,
        )
        .await
    }
    async fn get_rooms_for_list(
        &self,
        query_params: SlidingSyncListQuery<'_>,
    ) -> Result<Vec<SlidingSyncRoom>, sqlx::Error> {
        self.get_rooms_for_list(query_params).await
    }
    async fn count_rooms_for_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        filters: Option<&SlidingSyncFilters>,
    ) -> Result<i64, sqlx::Error> {
        self.count_rooms_for_list(user_id, device_id, conn_id, list_key, filters).await
    }
    async fn get_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncRoom>, sqlx::Error> {
        self.get_room(user_id, device_id, room_id, conn_id).await
    }
    async fn materialize_room_from_activity(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncRoom>, sqlx::Error> {
        self.materialize_room_from_activity(user_id, device_id, room_id, conn_id).await
    }
    async fn delete_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.delete_room(user_id, device_id, room_id, conn_id).await
    }
    async fn update_notification_counts(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        highlight_count: i32,
        notification_count: i32,
    ) -> Result<(), sqlx::Error> {
        self.update_notification_counts(user_id, device_id, room_id, conn_id, highlight_count, notification_count).await
    }
    async fn bump_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        bump_stamp: i64,
    ) -> Result<(), sqlx::Error> {
        self.bump_room(user_id, device_id, room_id, conn_id, bump_stamp).await
    }
    async fn cleanup_expired_tokens(&self) -> Result<u64, sqlx::Error> {
        self.cleanup_expired_tokens().await
    }
    async fn list_room_token_sync(
        &self,
        room_id: &str,
        limit: i64,
        from: Option<&RoomTokenSyncCursor>,
    ) -> Result<Vec<AdminRoomTokenSyncEntry>, sqlx::Error> {
        self.list_room_token_sync(room_id, limit, from).await
    }
    async fn count_room_token_sync(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        self.count_room_token_sync(room_id).await
    }
    async fn get_global_account_data(&self, user_id: &str) -> Result<serde_json::Value, sqlx::Error> {
        self.get_global_account_data(user_id).await
    }
    async fn get_room_account_data(
        &self,
        user_id: &str,
        room_ids: &[String],
    ) -> Result<serde_json::Value, sqlx::Error> {
        self.get_room_account_data(user_id, room_ids).await
    }
    async fn get_receipts_for_rooms(&self, room_ids: &[String]) -> Result<serde_json::Value, sqlx::Error> {
        self.get_receipts_for_rooms(room_ids).await
    }
    async fn delete_connection_data(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.delete_connection_data(user_id, device_id, conn_id).await
    }
}
