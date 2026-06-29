// Note: update_device is split into update_device_display_name and
// update_device_last_seen matching the existing DeviceStorage API surface
// (update_user_device_display_name and update_device_last_seen respectively).

use async_trait::async_trait;
use std::collections::HashMap;

use super::Device;

#[async_trait]
pub trait DeviceRepository: Send + Sync {
    async fn create_device(
        &self,
        user_id: &str,
        device_id: &str,
        display_name: Option<&str>,
    ) -> Result<Device, sqlx::Error>;

    async fn get_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<Device>, sqlx::Error>;

    async fn get_user_devices(
        &self,
        user_id: &str,
    ) -> Result<Vec<Device>, sqlx::Error>;

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

    async fn delete_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), sqlx::Error>;

    async fn delete_all_devices(
        &self,
        user_id: &str,
    ) -> Result<(), sqlx::Error>;

    async fn get_device_keys_for_users(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, Vec<Device>>, sqlx::Error>;

    async fn get_device_count(
        &self,
        user_id: &str,
    ) -> Result<i64, sqlx::Error>;

    async fn record_device_list_change(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        change_type: &str,
    ) -> Result<i64, sqlx::Error>;
}
