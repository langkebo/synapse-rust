// Note: update_device is split into update_device_display_name and
// update_device_last_seen matching the existing DeviceStorage API surface
// (update_user_device_display_name and update_device_last_seen respectively).

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use super::Device;

#[async_trait]
pub trait DeviceRepository: Send + Sync {
    /// Returns a reference to the database connection pool.
    fn pool(&self) -> &Arc<sqlx::PgPool>;

    async fn create_device(
        &self,
        user_id: &str,
        device_id: &str,
        display_name: Option<&str>,
    ) -> Result<Device, sqlx::Error>;

    async fn get_device(&self, user_id: &str, device_id: &str) -> Result<Option<Device>, sqlx::Error>;

    /// Get a device by device_id only (cross-user lookup for conflict detection).
    async fn get_device_by_id(&self, device_id: &str) -> Result<Option<Device>, sqlx::Error>;

    async fn get_user_devices(&self, user_id: &str) -> Result<Vec<Device>, sqlx::Error>;

    // update_device is split into update_device_display_name +
    // update_device_last_seen matching the existing DeviceStorage API surface.
    async fn update_device_display_name(
        &self,
        user_id: &str,
        device_id: &str,
        display_name: Option<&str>,
    ) -> Result<(), sqlx::Error>;

    async fn update_device_last_seen(
        &self,
        device_id: &str,
        last_seen_ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(), sqlx::Error>;

    async fn delete_device(&self, user_id: &str, device_id: &str) -> Result<(), sqlx::Error>;

    /// Delete a single device and return the count of rows affected.
    async fn delete_device_returning_count(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<u64, sqlx::Error>;

    async fn delete_all_devices(&self, user_id: &str) -> Result<(), sqlx::Error>;

    /// Delete multiple devices in a batch and return the count of rows affected.
    async fn delete_devices_batch(
        &self,
        user_id: &str,
        device_ids: &[String],
    ) -> Result<u64, sqlx::Error>;

    async fn get_device_keys_for_users(&self, user_ids: &[String])
        -> Result<HashMap<String, Vec<Device>>, sqlx::Error>;

    async fn get_device_count(&self, user_id: &str) -> Result<i64, sqlx::Error>;

    async fn record_device_list_change(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        change_type: &str,
    ) -> Result<i64, sqlx::Error>;

    // -- device list stream --

    async fn get_max_device_list_stream_id(&self) -> Result<i64, sqlx::Error>;

    async fn get_max_device_list_stream_id_for_user(&self, user_id: &str) -> Result<i64, sqlx::Error>;

    async fn get_device_lists_since_with_shared_rooms(
        &self,
        since_stream_id: i64,
        exclude_user_id: &str,
    ) -> Result<(Vec<String>, Vec<String>), sqlx::Error>;

    async fn has_device_list_updates_since(&self, since_stream_id: i64) -> Result<bool, sqlx::Error>;

    // -- lazy-loaded members --

    async fn get_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
    ) -> Result<std::collections::HashSet<String>, sqlx::Error>;

    async fn upsert_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        member_user_ids: &std::collections::HashSet<String>,
    ) -> Result<u64, sqlx::Error>;
}
