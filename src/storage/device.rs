use sqlx::{Pool, Postgres};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Device {
    pub device_id: String,
    pub user_id: String,
    pub display_name: Option<String>,
    pub device_key: Option<serde_json::Value>,
    pub last_seen_ts: Option<i64>,
    pub last_seen_ip: Option<String>,
    pub created_ts: i64,
    pub first_seen_ts: i64,
    pub appservice_id: Option<String>,
    pub ignored_user_list: Option<String>,
}

#[derive(Clone)]
pub struct DeviceStorage {
    pub pool: Arc<Pool<Postgres>>,
}

impl DeviceStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    async fn record_device_list_change(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        change_type: &str,
    ) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let row = sqlx::query(
            r#"
            INSERT INTO device_lists_stream (user_id, device_id, created_ts)
            VALUES ($1, $2, $3)
            RETURNING stream_id
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        use sqlx::Row;
        let stream_id: i64 = row.get("stream_id");

        sqlx::query(
            r#"
            INSERT INTO device_lists_changes (user_id, device_id, change_type, stream_id, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(change_type)
        .bind(stream_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(stream_id)
    }

    pub async fn record_device_list_change_best_effort(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        change_type: &str,
    ) {
        let _ = self
            .record_device_list_change(user_id, device_id, change_type)
            .await;
    }

    pub async fn get_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
    ) -> Result<HashSet<String>, sqlx::Error> {
        let members: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT member_user_id
            FROM lazy_loaded_members
            WHERE user_id = $1 AND device_id = $2 AND room_id = $3
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(members.into_iter().collect())
    }

    pub async fn upsert_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        member_user_ids: &HashSet<String>,
    ) -> Result<u64, sqlx::Error> {
        if member_user_ids.is_empty() {
            return Ok(0);
        }

        let now = chrono::Utc::now().timestamp_millis();
        let member_user_ids: Vec<&str> = member_user_ids.iter().map(String::as_str).collect();
        let result = sqlx::query(
            r#"
            INSERT INTO lazy_loaded_members (
                user_id,
                device_id,
                room_id,
                member_user_id,
                created_ts,
                updated_ts
            )
            SELECT $1, $2, $3, member_user_id, $4, $4
            FROM UNNEST($5::TEXT[]) AS member_user_id
            ON CONFLICT (user_id, device_id, room_id, member_user_id)
            DO UPDATE SET updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(room_id)
        .bind(now)
        .bind(&member_user_ids)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    async fn delete_lazy_loaded_members_for_user(&self, user_id: &str) -> Result<u64, sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM lazy_loaded_members
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await
        .map(|result| result.rows_affected())
    }

    async fn delete_lazy_loaded_members_for_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<u64, sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM lazy_loaded_members
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&*self.pool)
        .await
        .map(|result| result.rows_affected())
    }

    async fn delete_lazy_loaded_members_for_device_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: &str,
        device_id: &str,
    ) -> Result<u64, sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM lazy_loaded_members
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&mut **tx)
        .await
        .map(|result| result.rows_affected())
    }

    pub async fn create_device(
        &self,
        device_id: &str,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<Device, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let device = sqlx::query_as::<_, Device>(
            r#"
            INSERT INTO devices (device_id, user_id, display_name, first_seen_ts, last_seen_ts, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, appservice_id, ignored_user_list
            "#,
        )
        .bind(device_id)
        .bind(user_id)
        .bind(display_name)
        .bind(now)
        .bind(now)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        let _ = self
            .delete_lazy_loaded_members_for_device(user_id, device_id)
            .await;
        let _ = self
            .record_device_list_change(user_id, Some(device_id), "changed")
            .await;

        Ok(device)
    }

    /// Creates a new device in the database within a transaction.
    pub async fn create_device_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        device_id: &str,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<Device, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let device = sqlx::query_as::<_, Device>(
            r#"
            INSERT INTO devices (device_id, user_id, display_name, first_seen_ts, last_seen_ts, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, appservice_id, ignored_user_list
            "#,
        )
        .bind(device_id)
        .bind(user_id)
        .bind(display_name)
        .bind(now)
        .bind(now)
        .bind(now)
        .fetch_one(&mut **tx)
        .await?;

        let _ = Self::delete_lazy_loaded_members_for_device_tx(tx, user_id, device_id).await;

        let stream_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO device_lists_stream (user_id, device_id, created_ts)
            VALUES ($1, $2, $3)
            RETURNING stream_id
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .fetch_one(&mut **tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO device_lists_changes (user_id, device_id, change_type, stream_id, created_ts)
            VALUES ($1, $2, 'changed', $3, $4)
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(stream_id)
        .bind(now)
        .execute(&mut **tx)
        .await?;

        Ok(device)
    }

    pub async fn get_device(&self, device_id: &str) -> Result<Option<Device>, sqlx::Error> {
        sqlx::query_as::<_, Device>(
            r#"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, appservice_id, ignored_user_list
            FROM devices WHERE device_id = $1
            "#,
        )
        .bind(device_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_user_devices(&self, user_id: &str) -> Result<Vec<Device>, sqlx::Error> {
        sqlx::query_as::<_, Device>(
            r#"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, appservice_id, ignored_user_list
            FROM devices WHERE user_id = $1 ORDER BY last_seen_ts DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn update_device_display_name(
        &self,
        device_id: &str,
        display_name: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE devices SET display_name = $1 WHERE device_id = $2
            "#,
        )
        .bind(display_name)
        .bind(device_id)
        .execute(&*self.pool)
        .await?;

        if let Some(device) = self.get_device(device_id).await? {
            let _ = self
                .record_device_list_change(&device.user_id, Some(device_id), "changed")
                .await;
        }
        Ok(())
    }

    pub async fn update_user_device_display_name(
        &self,
        user_id: &str,
        device_id: &str,
        display_name: &str,
    ) -> Result<u64, sqlx::Error> {
        let rows_affected = sqlx::query(
            r#"
            UPDATE devices
            SET display_name = $1
            WHERE device_id = $2 AND user_id = $3
            "#,
        )
        .bind(display_name)
        .bind(device_id)
        .bind(user_id)
        .execute(&*self.pool)
        .await
        .map(|result| result.rows_affected())?;

        if rows_affected > 0 {
            let _ = self
                .record_device_list_change(user_id, Some(device_id), "changed")
                .await;
        }

        Ok(rows_affected)
    }

    pub async fn update_device_last_seen(&self, device_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            UPDATE devices SET last_seen_ts = $1 WHERE device_id = $2
            "#,
        )
        .bind(now)
        .bind(device_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_device(&self, device_id: &str) -> Result<(), sqlx::Error> {
        let existing = self.get_device(device_id).await?;
        let result = sqlx::query(
            r#"
            DELETE FROM devices WHERE device_id = $1
            "#,
        )
        .bind(device_id)
        .execute(&*self.pool)
        .await;

        match result {
            Ok(res) => {
                if res.rows_affected() > 0 {
                    if let Some(device) = existing {
                        let _ = self
                            .delete_lazy_loaded_members_for_device(&device.user_id, device_id)
                            .await;
                        let _ = self
                            .record_device_list_change(&device.user_id, Some(device_id), "deleted")
                            .await;
                    }
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub async fn delete_user_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let rows_affected = sqlx::query(
            r#"
            DELETE FROM devices
            WHERE device_id = $1 AND user_id = $2
            "#,
        )
        .bind(device_id)
        .bind(user_id)
        .execute(&*self.pool)
        .await
        .map(|result| result.rows_affected())?;

        if rows_affected > 0 {
            let _ = self
                .delete_lazy_loaded_members_for_device(user_id, device_id)
                .await;
            let _ = self
                .record_device_list_change(user_id, Some(device_id), "deleted")
                .await;
        }

        Ok(rows_affected)
    }

    pub async fn delete_user_devices(&self, user_id: &str) -> Result<(), sqlx::Error> {
        let device_ids: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT device_id FROM devices WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        let result = sqlx::query(
            r#"
            DELETE FROM devices WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await;

        match result {
            Ok(res) => {
                if res.rows_affected() > 0 {
                    let _ = self.delete_lazy_loaded_members_for_user(user_id).await;
                    for device_id in device_ids {
                        let _ = self
                            .record_device_list_change(user_id, Some(&device_id), "deleted")
                            .await;
                    }
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub async fn delete_devices_batch(&self, device_ids: &[String]) -> Result<u64, sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(0);
        }

        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT user_id, device_id FROM devices WHERE device_id = ANY($1)
            "#,
        )
        .bind(device_ids)
        .fetch_all(&*self.pool)
        .await?;

        let rows_affected = sqlx::query("DELETE FROM devices WHERE device_id = ANY($1)")
            .bind(device_ids)
            .execute(&*self.pool)
            .await
            .map(|result| result.rows_affected())?;

        if rows_affected > 0 {
            for (user_id, device_id) in rows {
                let _ = self
                    .delete_lazy_loaded_members_for_device(&user_id, &device_id)
                    .await;
                let _ = self
                    .record_device_list_change(&user_id, Some(&device_id), "deleted")
                    .await;
            }
        }

        Ok(rows_affected)
    }

    pub async fn delete_user_devices_batch(
        &self,
        user_id: &str,
        device_ids: &[String],
    ) -> Result<u64, sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(0);
        }

        let rows_affected =
            sqlx::query("DELETE FROM devices WHERE user_id = $1 AND device_id = ANY($2)")
                .bind(user_id)
                .bind(device_ids)
                .execute(&*self.pool)
                .await
                .map(|result| result.rows_affected())?;

        if rows_affected > 0 {
            for device_id in device_ids {
                let _ = self
                    .delete_lazy_loaded_members_for_device(user_id, device_id)
                    .await;
                let _ = self
                    .record_device_list_change(user_id, Some(device_id.as_str()), "deleted")
                    .await;
            }
        }

        Ok(rows_affected)
    }

    pub async fn device_exists(&self, device_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT 1 AS "exists" FROM devices WHERE device_id = $1 LIMIT 1
            "#,
        )
        .bind(device_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    pub async fn get_devices_batch(
        &self,
        device_ids: &[String],
    ) -> Result<Vec<Device>, sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as::<_, Device>(
            r#"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, appservice_id, ignored_user_list
            FROM devices WHERE device_id = ANY($1)
            ORDER BY last_seen_ts DESC
            "#,
        )
        .bind(device_ids)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_users_devices_batch(
        &self,
        user_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<Device>>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let devices: Vec<Device> = sqlx::query_as(
            r#"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, appservice_id, ignored_user_list
            FROM devices WHERE user_id = ANY($1)
            ORDER BY user_id, last_seen_ts DESC
            "#,
        )
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await?;

        let mut result: std::collections::HashMap<String, Vec<Device>> =
            user_ids.iter().map(|id| (id.clone(), Vec::new())).collect();

        for device in devices {
            if let Some(user_devices) = result.get_mut(&device.user_id) {
                user_devices.push(device);
            }
        }

        Ok(result)
    }

    pub async fn get_device_keys_for_users(
        &self,
        user_ids: &[String],
    ) -> Result<
        std::collections::HashMap<String, std::collections::HashMap<String, Device>>,
        sqlx::Error,
    > {
        if user_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let devices: Vec<Device> = sqlx::query_as(
            r#"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, appservice_id, ignored_user_list
            FROM devices 
            WHERE user_id = ANY($1) AND device_key IS NOT NULL
            "#,
        )
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await?;

        let mut result: std::collections::HashMap<
            String,
            std::collections::HashMap<String, Device>,
        > = user_ids
            .iter()
            .map(|id| (id.clone(), std::collections::HashMap::new()))
            .collect();

        for device in devices {
            if let Some(user_devices) = result.get_mut(&device.user_id) {
                user_devices.insert(device.device_id.clone(), device);
            }
        }

        Ok(result)
    }

    pub async fn update_devices_last_seen_batch(
        &self,
        device_ids: &[String],
    ) -> Result<u64, sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(0);
        }

        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query(
            r#"
            UPDATE devices SET last_seen_ts = $1 WHERE device_id = ANY($2)
            "#,
        )
        .bind(now)
        .bind(device_ids)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_device_struct() {
        let device = Device {
            device_id: "DEVICE123".to_string(),
            user_id: "@alice:example.com".to_string(),
            display_name: Some("iPhone 15".to_string()),
            device_key: Some(serde_json::json!({"key": "value"})),
            last_seen_ts: Some(1234567890000),
            last_seen_ip: Some("192.168.1.1".to_string()),
            created_ts: 1234567890000,
            first_seen_ts: 1234567890000,
            appservice_id: None,
            ignored_user_list: None,
        };

        assert_eq!(device.device_id, "DEVICE123");
        assert_eq!(device.user_id, "@alice:example.com");
        assert!(device.display_name.is_some());
    }

    #[test]
    fn test_device_with_minimal_fields() {
        let device = Device {
            device_id: "MINIMAL".to_string(),
            user_id: "@bob:example.com".to_string(),
            display_name: None,
            device_key: None,
            last_seen_ts: None,
            last_seen_ip: None,
            created_ts: 0,
            first_seen_ts: 0,
            appservice_id: None,
            ignored_user_list: None,
        };

        assert_eq!(device.device_id, "MINIMAL");
        assert!(device.display_name.is_none());
        assert!(device.device_key.is_none());
    }

    #[test]
    fn test_device_with_appservice() {
        let device = Device {
            device_id: "APPDEVICE".to_string(),
            user_id: "@bot:example.com".to_string(),
            display_name: Some("Bot Device".to_string()),
            device_key: None,
            last_seen_ts: Some(1234567890000),
            last_seen_ip: None,
            created_ts: 1234567890000,
            first_seen_ts: 1234567890000,
            appservice_id: Some("my_appservice".to_string()),
            ignored_user_list: None,
        };

        assert_eq!(device.appservice_id, Some("my_appservice".to_string()));
    }

    #[test]
    fn test_device_key_json() {
        let device = Device {
            device_id: "KEYDEVICE".to_string(),
            user_id: "@charlie:example.com".to_string(),
            display_name: None,
            device_key: Some(serde_json::json!({
                "algorithms": ["m.megolm.v1.aes-sha2"],
                "device_id": "KEYDEVICE",
                "keys": {
                    "ed25519:KEYDEVICE": "key_data"
                }
            })),
            last_seen_ts: None,
            last_seen_ip: None,
            created_ts: 0,
            first_seen_ts: 0,
            appservice_id: None,
            ignored_user_list: None,
        };

        assert!(device.device_key.is_some());
        let key = device.device_key.unwrap();
        assert!(key.get("algorithms").is_some());
    }

    #[test]
    fn test_device_with_display_name() {
        let device = Device {
            device_id: "TESTDEVICE".to_string(),
            user_id: "@alice:example.com".to_string(),
            display_name: Some("My Phone".to_string()),
            device_key: None,
            last_seen_ts: Some(1234567890),
            last_seen_ip: Some("192.168.1.1".to_string()),
            created_ts: 1234567800,
            first_seen_ts: 1234567800,
            appservice_id: None,
            ignored_user_list: None,
        };

        assert!(device.display_name.is_some());
        assert_eq!(device.display_name.unwrap(), "My Phone");
        assert!(device.last_seen_ts.is_some());
    }

    #[test]
    fn test_device_without_keys() {
        let device = Device {
            device_id: "NOKEYS".to_string(),
            user_id: "@bob:example.com".to_string(),
            display_name: None,
            device_key: None,
            last_seen_ts: None,
            last_seen_ip: None,
            created_ts: 0,
            first_seen_ts: 0,
            appservice_id: None,
            ignored_user_list: None,
        };

        assert!(device.device_key.is_none());
        assert!(device.last_seen_ts.is_none());
    }

    #[test]
    fn test_device_appservice_id() {
        let device = Device {
            device_id: "BRIDGE_DEVICE".to_string(),
            user_id: "@_irc_alice:example.com".to_string(),
            display_name: Some("IRC Bridge".to_string()),
            device_key: None,
            last_seen_ts: Some(1234567890),
            last_seen_ip: Some("10.0.0.1".to_string()),
            created_ts: 1234567800,
            first_seen_ts: 1234567800,
            appservice_id: Some("irc-bridge".to_string()),
            ignored_user_list: None,
        };

        assert!(device.appservice_id.is_some());
        assert_eq!(device.appservice_id.unwrap(), "irc-bridge");
    }

    #[test]
    fn test_device_ignored_user_list() {
        let device = Device {
            device_id: "IGNORED".to_string(),
            user_id: "@user:example.com".to_string(),
            display_name: None,
            device_key: None,
            last_seen_ts: None,
            last_seen_ip: None,
            created_ts: 0,
            first_seen_ts: 0,
            appservice_id: None,
            ignored_user_list: Some("[\"@baduser:example.com\"]".to_string()),
        };

        assert!(device.ignored_user_list.is_some());
    }

    #[test]
    fn test_device_ip_tracking() {
        let device = Device {
            device_id: "IPTRACK".to_string(),
            user_id: "@alice:example.com".to_string(),
            display_name: None,
            device_key: None,
            last_seen_ts: Some(1234567890),
            last_seen_ip: Some("203.0.113.1".to_string()),
            created_ts: 1234567800,
            first_seen_ts: 1234567800,
            appservice_id: None,
            ignored_user_list: None,
        };

        assert!(device.last_seen_ip.is_some());
        assert!(device.last_seen_ts.is_some());
    }

    #[tokio::test]
    async fn test_lazy_loaded_members_are_cleaned_up_with_device_lifecycle() {
        let pool = match crate::test_utils::prepare_empty_isolated_test_pool().await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!(
                    "Skipping device lazy-loaded-members test because test database is unavailable: {}",
                    error
                );
                return;
            }
        };

        sqlx::query(
            r#"
            CREATE TABLE users (
                user_id VARCHAR(255) PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*pool)
        .await
        .expect("Failed to create users table");

        sqlx::query(
            r#"
            CREATE TABLE devices (
                device_id VARCHAR(255) PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                display_name TEXT,
                device_key JSONB,
                last_seen_ts BIGINT,
                last_seen_ip TEXT,
                created_ts BIGINT NOT NULL,
                first_seen_ts BIGINT NOT NULL,
                appservice_id TEXT,
                ignored_user_list TEXT
            )
            "#,
        )
        .execute(&*pool)
        .await
        .expect("Failed to create devices table");

        sqlx::query(
            r#"
            CREATE TABLE device_lists_stream (
                stream_id BIGSERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255),
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*pool)
        .await
        .expect("Failed to create device_lists_stream table");

        sqlx::query(
            r#"
            CREATE TABLE device_lists_changes (
                id BIGSERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255),
                change_type TEXT NOT NULL,
                stream_id BIGINT NOT NULL,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*pool)
        .await
        .expect("Failed to create device_lists_changes table");

        sqlx::query(
            r#"
            CREATE TABLE lazy_loaded_members (
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                member_user_id TEXT NOT NULL,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                PRIMARY KEY (user_id, device_id, room_id, member_user_id)
            )
            "#,
        )
        .execute(&*pool)
        .await
        .expect("Failed to create lazy_loaded_members table");

        sqlx::query(
            r#"
            INSERT INTO users (user_id, username, created_ts)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind("@alice:localhost")
        .bind("alice")
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*pool)
        .await
        .expect("Failed to insert test user");

        let storage = DeviceStorage::new(&pool);
        storage
            .create_device("DEVICE123", "@alice:localhost", Some("Alice phone"))
            .await
            .expect("Failed to create device");

        let members = HashSet::from(["@alice:localhost".to_string(), "@bob:localhost".to_string()]);
        storage
            .upsert_lazy_loaded_members(
                "@alice:localhost",
                "DEVICE123",
                "!room:localhost",
                &members,
            )
            .await
            .expect("Failed to store lazy-loaded members");

        let before_delete = storage
            .get_lazy_loaded_members("@alice:localhost", "DEVICE123", "!room:localhost")
            .await
            .expect("Failed to fetch lazy-loaded members");
        assert_eq!(before_delete, members);

        let rows = storage
            .delete_user_device("@alice:localhost", "DEVICE123")
            .await
            .expect("Failed to delete device");
        assert_eq!(rows, 1);

        let after_delete = storage
            .get_lazy_loaded_members("@alice:localhost", "DEVICE123", "!room:localhost")
            .await
            .expect("Failed to fetch lazy-loaded members after delete");
        assert!(after_delete.is_empty());
    }
}
