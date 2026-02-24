use sqlx::{Pool, Postgres};
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

    pub async fn create_device(
        &self,
        device_id: &str,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<Device, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query_as::<_, Device>(
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
        .await
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
        Ok(())
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
        sqlx::query(
            r#"
            DELETE FROM devices WHERE device_id = $1
            "#,
        )
        .bind(device_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_user_devices(&self, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM devices WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_devices_batch(&self, device_ids: &[String]) -> Result<u64, sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(0);
        }

        sqlx::query("DELETE FROM devices WHERE device_id = ANY($1)")
            .bind(device_ids)
            .execute(&*self.pool)
            .await
            .map(|result| result.rows_affected())
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
    ) -> Result<std::collections::HashMap<String, std::collections::HashMap<String, Device>>, sqlx::Error> {
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

        let mut result: std::collections::HashMap<String, std::collections::HashMap<String, Device>> =
            user_ids.iter().map(|id| (id.clone(), std::collections::HashMap::new())).collect();

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
}
