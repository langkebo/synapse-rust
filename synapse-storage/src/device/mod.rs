use async_trait::async_trait;
use sqlx::{Pool, Postgres, Row};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

mod repository;
pub use repository::DeviceRepository;

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
    pub user_agent: Option<String>,
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
            r"
            INSERT INTO device_lists_stream (user_id, device_id, created_ts)
            VALUES ($1, $2, $3)
            RETURNING stream_id
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        use sqlx::Row;
        let stream_id: i64 = row.get("stream_id");

        sqlx::query(
            r"
            INSERT INTO device_lists_changes (user_id, device_id, change_type, stream_id, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            ",
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
        let _ = self.record_device_list_change(user_id, device_id, change_type).await;
    }

    async fn record_device_list_changes_batch(
        &self,
        user_id: &str,
        device_ids: &[String],
        change_type: &str,
    ) -> Result<(), sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(());
        }
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r"
            WITH inserted AS (
                INSERT INTO device_lists_stream (user_id, device_id, created_ts)
                SELECT $1, device_id, $2 FROM UNNEST($3::TEXT[]) AS device_id
                RETURNING stream_id, device_id
            )
            INSERT INTO device_lists_changes (user_id, device_id, change_type, stream_id, created_ts)
            SELECT $1, device_id, $4, stream_id, $2 FROM inserted
            ",
        )
        .bind(user_id)
        .bind(now)
        .bind(device_ids)
        .bind(change_type)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn record_device_list_changes_batch_best_effort(
        &self,
        user_id: &str,
        device_ids: &[String],
        change_type: &str,
    ) {
        let _ = self.record_device_list_changes_batch(user_id, device_ids, change_type).await;
    }

    pub(crate) async fn insert_device_list_change(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        change_type: &str,
        stream_id: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO device_lists_changes (user_id, device_id, change_type, stream_id, created_ts) VALUES ($1, $2, $3, $4, $4) ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(change_type)
        .bind(stream_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
    ) -> Result<HashSet<String>, sqlx::Error> {
        let members: Vec<String> = sqlx::query_scalar(
            r"
            SELECT member_user_id
            FROM lazy_loaded_members
            WHERE user_id = $1 AND device_id = $2 AND room_id = $3
            ",
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
            r"
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
            ",
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
            r"
            DELETE FROM lazy_loaded_members
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await
        .map(|result| result.rows_affected())
    }

    async fn delete_lazy_loaded_members_for_device(&self, user_id: &str, device_id: &str) -> Result<u64, sqlx::Error> {
        sqlx::query(
            r"
            DELETE FROM lazy_loaded_members
            WHERE user_id = $1 AND device_id = $2
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&*self.pool)
        .await
        .map(|result| result.rows_affected())
    }

    async fn delete_lazy_loaded_members_for_devices_batch(
        &self,
        user_id: &str,
        device_ids: &[String],
    ) -> Result<u64, sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(0);
        }
        sqlx::query(
            r"
            DELETE FROM lazy_loaded_members
            WHERE user_id = $1 AND device_id = ANY($2)
            ",
        )
        .bind(user_id)
        .bind(device_ids)
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
            r"
            DELETE FROM lazy_loaded_members
            WHERE user_id = $1 AND device_id = $2
            ",
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
            r"
            INSERT INTO devices (device_id, user_id, display_name, first_seen_ts, last_seen_ts, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, user_agent, appservice_id, ignored_user_list
            ",
        )
        .bind(device_id)
        .bind(user_id)
        .bind(display_name)
        .bind(now)
        .bind(now)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        let _ = self.delete_lazy_loaded_members_for_device(user_id, device_id).await;
        let _ = self.record_device_list_change(user_id, Some(device_id), "changed").await;

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
            r"
            INSERT INTO devices (device_id, user_id, display_name, first_seen_ts, last_seen_ts, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, user_agent, appservice_id, ignored_user_list
            ",
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
            r"
            INSERT INTO device_lists_stream (user_id, device_id, created_ts)
            VALUES ($1, $2, $3)
            RETURNING stream_id
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .fetch_one(&mut **tx)
        .await?;

        sqlx::query(
            r"
            INSERT INTO device_lists_changes (user_id, device_id, change_type, stream_id, created_ts)
            VALUES ($1, $2, 'changed', $3, $4)
            ",
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
            r"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, user_agent, appservice_id, ignored_user_list
            FROM devices WHERE device_id = $1
            ",
        )
        .bind(device_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_user_devices(&self, user_id: &str) -> Result<Vec<Device>, sqlx::Error> {
        sqlx::query_as::<_, Device>(
            r"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, user_agent, appservice_id, ignored_user_list
            FROM devices WHERE user_id = $1 ORDER BY last_seen_ts DESC
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn update_device_display_name(&self, device_id: &str, display_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE devices SET display_name = $1 WHERE device_id = $2
            ",
        )
        .bind(display_name)
        .bind(device_id)
        .execute(&*self.pool)
        .await?;

        if let Some(device) = self.get_device(device_id).await? {
            let _ = self.record_device_list_change(&device.user_id, Some(device_id), "changed").await;
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
            r"
            UPDATE devices
            SET display_name = $1
            WHERE device_id = $2 AND user_id = $3
            ",
        )
        .bind(display_name)
        .bind(device_id)
        .bind(user_id)
        .execute(&*self.pool)
        .await
        .map(|result| result.rows_affected())?;

        if rows_affected > 0 {
            let _ = self.record_device_list_change(user_id, Some(device_id), "changed").await;
        }

        Ok(rows_affected)
    }

    pub async fn update_device_last_seen(&self, device_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r"
            UPDATE devices SET last_seen_ts = $1 WHERE device_id = $2
            ",
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
            r"
            DELETE FROM devices WHERE device_id = $1
            ",
        )
        .bind(device_id)
        .execute(&*self.pool)
        .await;

        match result {
            Ok(res) => {
                if res.rows_affected() > 0 {
                    if let Some(device) = existing {
                        let _ = self.delete_lazy_loaded_members_for_device(&device.user_id, device_id).await;
                        let _ = self.record_device_list_change(&device.user_id, Some(device_id), "deleted").await;
                    }
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub async fn delete_user_device(&self, user_id: &str, device_id: &str) -> Result<u64, sqlx::Error> {
        let rows_affected = sqlx::query(
            r"
            DELETE FROM devices
            WHERE device_id = $1 AND user_id = $2
            ",
        )
        .bind(device_id)
        .bind(user_id)
        .execute(&*self.pool)
        .await
        .map(|result| result.rows_affected())?;

        if rows_affected > 0 {
            let _ = self.delete_lazy_loaded_members_for_device(user_id, device_id).await;
            let _ = self.record_device_list_change(user_id, Some(device_id), "deleted").await;
        }

        Ok(rows_affected)
    }

    pub async fn delete_user_devices(&self, user_id: &str) -> Result<(), sqlx::Error> {
        let device_ids: Vec<String> = sqlx::query_scalar(
            r"
            SELECT device_id FROM devices WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        let result = sqlx::query(
            r"
            DELETE FROM devices WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await;

        match result {
            Ok(res) => {
                if res.rows_affected() > 0 {
                    let _ = self.delete_lazy_loaded_members_for_user(user_id).await;
                    self.record_device_list_changes_batch_best_effort(user_id, &device_ids, "deleted").await;
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
            r"
            SELECT user_id, device_id FROM devices WHERE device_id = ANY($1)
            ",
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
                let _ = self.delete_lazy_loaded_members_for_device(&user_id, &device_id).await;
                let _ = self.record_device_list_change(&user_id, Some(&device_id), "deleted").await;
            }
        }

        Ok(rows_affected)
    }

    pub async fn delete_user_devices_batch(&self, user_id: &str, device_ids: &[String]) -> Result<u64, sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(0);
        }

        let rows_affected = sqlx::query("DELETE FROM devices WHERE user_id = $1 AND device_id = ANY($2)")
            .bind(user_id)
            .bind(device_ids)
            .execute(&*self.pool)
            .await
            .map(|result| result.rows_affected())?;

        if rows_affected > 0 {
            let _ = self.delete_lazy_loaded_members_for_devices_batch(user_id, device_ids).await;
            self.record_device_list_changes_batch_best_effort(user_id, device_ids, "deleted").await;
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

    pub async fn get_devices_batch(&self, device_ids: &[String]) -> Result<Vec<Device>, sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as::<_, Device>(
            r"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, user_agent, appservice_id, ignored_user_list
            FROM devices WHERE device_id = ANY($1)
            ORDER BY last_seen_ts DESC
            ",
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
            r"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, user_agent, appservice_id, ignored_user_list
            FROM devices WHERE user_id = ANY($1)
            ORDER BY user_id, last_seen_ts DESC
            ",
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
            r"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, user_agent, appservice_id, ignored_user_list
            FROM devices
            WHERE user_id = ANY($1) AND device_key IS NOT NULL
            ",
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

    pub async fn get_device_count(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            r"
            SELECT COUNT(*) FROM devices WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    pub async fn get_user_device(&self, user_id: &str, device_id: &str) -> Result<Option<Device>, sqlx::Error> {
        sqlx::query_as::<_, Device>(
            r"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, user_agent, appservice_id, ignored_user_list
            FROM devices WHERE user_id = $1 AND device_id = $2
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn filter_existing_users(&self, user_ids: &[String]) -> Result<Vec<String>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_scalar::<_, String>(
            r"
            SELECT DISTINCT user_id FROM devices WHERE user_id = ANY($1)
            ",
        )
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn update_devices_last_seen_batch(&self, device_ids: &[String]) -> Result<u64, sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(0);
        }

        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query(
            r"
            UPDATE devices SET last_seen_ts = $1 WHERE device_id = ANY($2)
            ",
        )
        .bind(now)
        .bind(device_ids)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Get the maximum stream ID from the device_lists_stream table.
    /// Returns 0 if the table is empty.
    pub async fn get_max_device_list_stream_id(&self) -> Result<i64, sqlx::Error> {
        let max_id: i64 = sqlx::query_scalar(
            r"
            SELECT COALESCE(MAX(stream_id), 0) FROM device_lists_stream
            ",
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(max_id)
    }

    pub async fn get_max_device_list_stream_id_for_user(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let max_id: i64 = sqlx::query_scalar(
            r"
            SELECT COALESCE(MAX(stream_id), 0)
            FROM device_lists_stream
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await?;

        Ok(max_id)
    }

    pub async fn has_device_list_updates_since(&self, since_stream_id: i64) -> Result<bool, sqlx::Error> {
        Ok(self.get_max_device_list_stream_id().await? > since_stream_id)
    }

    pub async fn get_device_list_changed_users_since(
        &self,
        since_stream_id: i64,
        exclude_user_id: &str,
    ) -> Result<(Vec<String>, i64), sqlx::Error> {
        let rows = sqlx::query(
            r"
            SELECT user_id, MAX(stream_id) AS max_id
            FROM device_lists_stream
            WHERE stream_id > $1
              AND user_id != $2
            GROUP BY user_id
            ORDER BY max_id ASC
            LIMIT 100
            ",
        )
        .bind(since_stream_id)
        .bind(exclude_user_id)
        .fetch_all(&*self.pool)
        .await?;

        let mut max_stream_id = since_stream_id;
        let changed = rows
            .iter()
            .map(|row| {
                let user_id: String = row.get("user_id");
                let stream_id: i64 = row.get("max_id");
                if stream_id > max_stream_id {
                    max_stream_id = stream_id;
                }
                user_id
            })
            .collect();

        Ok((changed, max_stream_id))
    }

    pub async fn get_device_list_left_users_since(
        &self,
        since_stream_id: i64,
        exclude_user_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query(
            r"
            SELECT DISTINCT dl.user_id
            FROM device_lists_stream dl
            LEFT JOIN room_memberships rm ON rm.user_id = dl.user_id
            WHERE dl.stream_id > $1
              AND dl.user_id != $2
              AND rm.user_id IS NULL
            ORDER BY dl.user_id
            LIMIT 100
            ",
        )
        .bind(since_stream_id)
        .bind(exclude_user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.iter().map(|row| row.get("user_id")).collect())
    }

    /// Get device lists changes with shared room permission checks
    pub async fn get_device_lists_since_with_shared_rooms(
        &self,
        since_stream_id: i64,
        exclude_user_id: &str,
    ) -> Result<(Vec<String>, Vec<String>), sqlx::Error> {
        let changed_rows = sqlx::query(
            r"
            SELECT DISTINCT dls.user_id
            FROM device_lists_stream dls
            INNER JOIN room_memberships rm1 ON rm1.user_id = dls.user_id AND rm1.membership = 'join'
            INNER JOIN room_memberships rm2 ON rm2.room_id = rm1.room_id AND rm2.user_id = $2 AND rm2.membership = 'join'
            WHERE dls.stream_id > $1
              AND dls.user_id != $2
            ORDER BY dls.user_id
            LIMIT 100
            ",
        )
        .bind(since_stream_id)
        .bind(exclude_user_id)
        .fetch_all(&*self.pool)
        .await?;

        let changed: Vec<String> = changed_rows.iter().map(|row| row.get("user_id")).collect();
        // We only persist current room membership, not a stream-aware history of
        // "shared room -> no shared room" transitions for the requesting user.
        // Returning `left` based only on current membership would leak isolated
        // users that never shared a room in the first place. Be conservative and
        // suppress `left` until we can derive it from a proper membership delta.
        let left = Vec::new();

        Ok((changed, left))
    }

    pub async fn get_device_list_changes(
        &self,
        since: i64,
        to: i64,
        user_ids: &[String],
    ) -> Result<Vec<(String, Option<String>, String, i64)>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as::<_, (String, Option<String>, String, i64)>(
            r"
            SELECT user_id, device_id, change_type, stream_id
            FROM device_lists_changes
            WHERE stream_id > $1
              AND stream_id <= $2
              AND user_id = ANY($3)
            ORDER BY stream_id ASC
            ",
        )
        .bind(since)
        .bind(to)
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_devices_by_user_device_pairs(
        &self,
        user_ids: &[&str],
        device_ids: &[&str],
    ) -> Result<Vec<(String, String, Option<String>, Option<i64>)>, sqlx::Error> {
        if user_ids.is_empty() || device_ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as::<_, (String, String, Option<String>, Option<i64>)>(
            r"
            SELECT user_id, device_id, display_name, last_seen_ts
            FROM devices
            WHERE (user_id, device_id) = ANY(SELECT * FROM UNNEST($1::text[], $2::text[]))
            ",
        )
        .bind(user_ids)
        .bind(device_ids)
        .fetch_all(&*self.pool)
        .await
    }

    /// Get the distinct user IDs whose device lists changed in the given stream
    /// range, excluding the requesting user.
    pub async fn get_device_list_changed_users(
        &self,
        from: i64,
        to: i64,
        exclude_user_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query(
            r"
            SELECT DISTINCT user_id
            FROM device_lists_stream
            WHERE stream_id > $1
              AND stream_id <= $2
              AND user_id != $3
            ORDER BY user_id
            LIMIT 100
            ",
        )
        .bind(from)
        .bind(to)
        .bind(exclude_user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.iter().map(|row| row.get("user_id")).collect())
    }

    /// Get the distinct user IDs who left (no room membership) in the given
    /// stream range, excluding the requesting user.
    pub async fn get_device_list_left_users(
        &self,
        from: i64,
        to: i64,
        exclude_user_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query(
            r"
            SELECT DISTINCT dl.user_id
            FROM device_lists_stream dl
            LEFT JOIN room_memberships rm ON rm.user_id = dl.user_id
            WHERE dl.stream_id > $1
              AND dl.stream_id <= $2
              AND dl.user_id != $3
              AND rm.user_id IS NULL
            ORDER BY dl.user_id
            LIMIT 100
            ",
        )
        .bind(from)
        .bind(to)
        .bind(exclude_user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.iter().map(|row| row.get("user_id")).collect())
    }
}

// ---------------------------------------------------------------------------
// DeviceRepository delegation
// ---------------------------------------------------------------------------

#[async_trait]
impl DeviceRepository for DeviceStorage {
    fn pool(&self) -> &Arc<Pool<Postgres>> {
        &self.pool
    }

    async fn create_device(
        &self,
        user_id: &str,
        device_id: &str,
        display_name: Option<&str>,
    ) -> Result<Device, sqlx::Error> {
        // CONCERN: Inherent DeviceStorage::create_device takes (device_id, user_id)
        // order -- swap arguments to match the actual storage method signature.
        self.create_device(device_id, user_id, display_name).await
    }

    async fn get_device(&self, user_id: &str, device_id: &str) -> Result<Option<Device>, sqlx::Error> {
        // CONCERN: Inherent DeviceStorage::get_device only accepts device_id.
        // Delegating to get_user_device(user_id, device_id) instead.
        self.get_user_device(user_id, device_id).await
    }

    async fn get_device_by_id(&self, device_id: &str) -> Result<Option<Device>, sqlx::Error> {
        self.get_device(device_id).await
    }

    async fn get_user_devices(&self, user_id: &str) -> Result<Vec<Device>, sqlx::Error> {
        self.get_user_devices(user_id).await
    }

    async fn update_device_display_name(
        &self,
        user_id: &str,
        device_id: &str,
        display_name: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let name = display_name.unwrap_or("");
        self.update_user_device_display_name(user_id, device_id, name).await.map(|_| ())
    }

    async fn update_device_last_seen(
        &self,
        device_id: &str,
        last_seen_ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        if last_seen_ip.is_some() {
            tracing::warn!("last_seen_ip parameter not yet supported by DeviceStorage; value discarded");
        }
        if user_agent.is_some() {
            tracing::warn!("user_agent parameter not yet supported by DeviceStorage; value discarded");
        }
        // CONCERN: DeviceStorage::update_device_last_seen only accepts
        // device_id; last_seen_ip and user_agent are not yet persisted by the
        // underlying storage.  When DeviceStorage is extended, these params
        // should be threaded through.
        self.update_device_last_seen(device_id).await
    }

    async fn delete_device(&self, user_id: &str, device_id: &str) -> Result<(), sqlx::Error> {
        // CONCERN: Inherent DeviceStorage::delete_device only takes device_id.
        // Delegating to delete_user_device(user_id, device_id) instead and
        // discarding the row-count return value to match the trait signature.
        self.delete_user_device(user_id, device_id).await.map(|_| ())
    }

    async fn delete_device_returning_count(&self, user_id: &str, device_id: &str) -> Result<u64, sqlx::Error> {
        self.delete_user_device(user_id, device_id).await
    }

    async fn delete_all_devices(&self, user_id: &str) -> Result<(), sqlx::Error> {
        self.delete_user_devices(user_id).await
    }

    async fn delete_devices_batch(&self, user_id: &str, device_ids: &[String]) -> Result<u64, sqlx::Error> {
        self.delete_user_devices_batch(user_id, device_ids).await
    }

    async fn get_device_keys_for_users(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, Vec<Device>>, sqlx::Error> {
        // CONCERN: Inherent DeviceStorage::get_device_keys_for_users returns
        // HashMap<String, HashMap<String, Device>> (user_id -> device_id -> Device).
        // Converting to the trait's expected HashMap<String, Vec<Device>>.
        let result = self.get_device_keys_for_users(user_ids).await?;
        Ok(result.into_iter().map(|(k, v)| (k, v.into_values().collect())).collect())
    }

    async fn get_device_count(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        self.get_device_count(user_id).await
    }

    async fn record_device_list_change(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        change_type: &str,
    ) -> Result<i64, sqlx::Error> {
        self.record_device_list_change(user_id, device_id, change_type).await
    }

    async fn insert_device_list_change(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        change_type: &str,
        stream_id: i64,
    ) -> Result<(), sqlx::Error> {
        self.insert_device_list_change(user_id, device_id, change_type, stream_id).await
    }

    // -- device list stream --

    async fn get_max_device_list_stream_id(&self) -> Result<i64, sqlx::Error> {
        self.get_max_device_list_stream_id().await
    }

    async fn get_max_device_list_stream_id_for_user(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        self.get_max_device_list_stream_id_for_user(user_id).await
    }

    async fn get_device_lists_since_with_shared_rooms(
        &self,
        since_stream_id: i64,
        exclude_user_id: &str,
    ) -> Result<(Vec<String>, Vec<String>), sqlx::Error> {
        self.get_device_lists_since_with_shared_rooms(since_stream_id, exclude_user_id).await
    }

    async fn has_device_list_updates_since(&self, since_stream_id: i64) -> Result<bool, sqlx::Error> {
        self.has_device_list_updates_since(since_stream_id).await
    }

    // -- lazy-loaded members --

    async fn get_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
    ) -> Result<std::collections::HashSet<String>, sqlx::Error> {
        self.get_lazy_loaded_members(user_id, device_id, room_id).await
    }

    async fn upsert_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        member_user_ids: &std::collections::HashSet<String>,
    ) -> Result<u64, sqlx::Error> {
        self.upsert_lazy_loaded_members(user_id, device_id, room_id, member_user_ids).await
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
            user_agent: None,
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
            user_agent: None,
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
            user_agent: None,
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
            user_agent: None,
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
            user_agent: None,
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
            user_agent: None,
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
            user_agent: None,
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
            user_agent: None,
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
            user_agent: None,
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
                tracing::warn!(
                    "Skipping device lazy-loaded-members test because test database is unavailable: {error}"
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
                user_agent TEXT,
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
            .upsert_lazy_loaded_members("@alice:localhost", "DEVICE123", "!room:localhost", &members)
            .await
            .expect("Failed to store lazy-loaded members");

        let before_delete = storage
            .get_lazy_loaded_members("@alice:localhost", "DEVICE123", "!room:localhost")
            .await
            .expect("Failed to fetch lazy-loaded members");
        assert_eq!(before_delete, members);

        let rows = storage.delete_user_device("@alice:localhost", "DEVICE123").await.expect("Failed to delete device");
        assert_eq!(rows, 1);

        let after_delete = storage
            .get_lazy_loaded_members("@alice:localhost", "DEVICE123", "!room:localhost")
            .await
            .expect("Failed to fetch lazy-loaded members after delete");
        assert!(after_delete.is_empty());
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn test_pool() -> Arc<Pool<Postgres>> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn ensure_test_user(pool: &Pool<Postgres>, user_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        let username = user_id
            .strip_prefix('@')
            .and_then(|u| u.split(':').next())
            .unwrap_or("testuser");
        sqlx::query(
            r#"INSERT INTO users (user_id, username, created_ts)
               VALUES ($1, $2, $3)
               ON CONFLICT (user_id) DO NOTHING"#,
        )
        .bind(user_id)
        .bind(username)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test user");
    }

    async fn ensure_test_device(pool: &Pool<Postgres>, user_id: &str, device_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"INSERT INTO devices (device_id, user_id, created_ts, first_seen_ts)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (device_id) DO NOTHING"#,
        )
        .bind(device_id)
        .bind(user_id)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test device");
    }

    #[tokio::test]
    async fn test_create_device_returns_valid_record() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let device_id = &format!("dev_create_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);
        let user_id = "@testuser:example.com";

        ensure_test_user(&pool, user_id).await;

        let device = storage
            .create_device(device_id, user_id, Some("Test Phone"))
            .await
            .expect("create_device should succeed");

        assert_eq!(device.device_id, device_id.as_str());
        assert_eq!(device.user_id, user_id);
        assert_eq!(device.display_name.as_deref(), Some("Test Phone"));
        assert!(device.created_ts > 0);
        assert!(device.first_seen_ts > 0);
    }

    #[tokio::test]
    async fn test_create_device_without_display_name() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let device_id = &format!("dev_nodesc_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);
        let user_id = "@nodesc:example.com";

        ensure_test_user(&pool, user_id).await;

        let device = storage
            .create_device(device_id, user_id, None)
            .await
            .expect("create_device without display name should succeed");

        assert_eq!(device.device_id, device_id.as_str());
        assert!(device.display_name.is_none());
    }

    #[tokio::test]
    async fn test_get_device_finds_created_device() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let device_id = &format!("dev_get_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);
        let user_id = "@getuser:example.com";

        ensure_test_user(&pool, user_id).await;
        storage
            .create_device(device_id, user_id, Some("Find Me"))
            .await
            .expect("create_device should succeed");

        let found = storage
            .get_device(device_id)
            .await
            .expect("get_device should succeed");

        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.device_id, device_id.as_str());
        assert_eq!(found.user_id, user_id);
        assert_eq!(found.display_name.as_deref(), Some("Find Me"));
    }

    #[tokio::test]
    async fn test_get_device_not_found_returns_none() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);

        let result = storage
            .get_device("nonexistent_device_id_12345")
            .await
            .expect("get_device should succeed");

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_user_devices_returns_all() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().simple().to_string().split_at(12).0.to_string();
        let user_id = format!("@multidev_{}:example.com", suffix);
        let d1 = format!("multi_1_{}", suffix);
        let d2 = format!("multi_2_{}", suffix);

        ensure_test_user(&pool, &user_id).await;
        // Clean up any leftover devices from previous runs
        let _ = storage.delete_user_devices(&user_id).await;
        storage.create_device(&d1, &user_id, Some("Phone")).await.expect("create d1");
        storage.create_device(&d2, &user_id, Some("Tablet")).await.expect("create d2");

        let devices = storage
            .get_user_devices(&user_id)
            .await
            .expect("get_user_devices should succeed");

        assert_eq!(devices.len(), 2);
        let ids: Vec<&str> = devices.iter().map(|d| d.device_id.as_str()).collect();
        assert!(ids.contains(&d1.as_str()));
        assert!(ids.contains(&d2.as_str()));
    }

    #[tokio::test]
    async fn test_get_user_devices_empty_for_new_user() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let user_id = "@nodevices:example.com";

        ensure_test_user(&pool, user_id).await;

        let devices = storage
            .get_user_devices(user_id)
            .await
            .expect("get_user_devices should succeed");

        assert!(devices.is_empty());
    }

    #[tokio::test]
    async fn test_update_device_display_name() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let device_id = &format!("dev_updname_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);
        let user_id = "@updatename:example.com";

        ensure_test_user(&pool, user_id).await;
        storage
            .create_device(device_id, user_id, Some("Old Name"))
            .await
            .expect("create_device");

        storage
            .update_device_display_name(device_id, "New Name")
            .await
            .expect("update_device_display_name should succeed");

        let updated = storage.get_device(device_id).await.expect("get_device").unwrap();
        assert_eq!(updated.display_name.as_deref(), Some("New Name"));
    }

    #[tokio::test]
    async fn test_update_user_device_display_name_returns_rows_affected() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let device_id = &format!("dev_upduser_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);
        let user_id = "@upduser:example.com";

        ensure_test_user(&pool, user_id).await;
        storage
            .create_device(device_id, user_id, None)
            .await
            .expect("create_device");

        let rows = storage
            .update_user_device_display_name(user_id, device_id, "Updated")
            .await
            .expect("update_user_device_display_name should succeed");

        assert_eq!(rows, 1);
    }

    #[tokio::test]
    async fn test_update_user_device_display_name_wrong_user_returns_zero() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let device_id = &format!("dev_wrongusr_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);
        let user_id = "@rightuser:example.com";

        ensure_test_user(&pool, user_id).await;
        storage
            .create_device(device_id, user_id, None)
            .await
            .expect("create_device");

        let rows = storage
            .update_user_device_display_name("@wronguser:example.com", device_id, "Nope")
            .await
            .expect("update should succeed but affect 0 rows");

        assert_eq!(rows, 0);
    }

    #[tokio::test]
    async fn test_update_device_last_seen() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let device_id = &format!("dev_seen_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);
        let user_id = "@seen:example.com";

        ensure_test_user(&pool, user_id).await;
        storage
            .create_device(device_id, user_id, None)
            .await
            .expect("create_device");

        storage
            .update_device_last_seen(device_id)
            .await
            .expect("update_device_last_seen should succeed");

        let updated = storage.get_device(device_id).await.expect("get_device").unwrap();
        assert!(updated.last_seen_ts.is_some());
        assert!(updated.last_seen_ts.unwrap() > 0);
    }

    #[tokio::test]
    async fn test_delete_user_device_returns_rows_affected() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let device_id = &format!("dev_del_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);
        let user_id = "@deleteusr:example.com";

        ensure_test_user(&pool, user_id).await;
        storage
            .create_device(device_id, user_id, None)
            .await
            .expect("create_device");

        let rows = storage
            .delete_user_device(user_id, device_id)
            .await
            .expect("delete_user_device should succeed");

        assert_eq!(rows, 1);

        let after = storage.get_device(device_id).await.expect("get_device");
        assert!(after.is_none());
    }

    #[tokio::test]
    async fn test_delete_user_device_nonexistent_returns_zero() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);

        let rows = storage
            .delete_user_device("@nobody:example.com", "no_such_device")
            .await
            .expect("delete should succeed but affect 0 rows");

        assert_eq!(rows, 0);
    }

    #[tokio::test]
    async fn test_device_exists_returns_true_for_created_device() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let device_id = &format!("dev_exists_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);
        let user_id = &format!("@exists_{}:example.com", uuid::Uuid::new_v4().simple().to_string().split_at(8).0);

        ensure_test_user(&pool, user_id).await;
        storage
            .create_device(device_id, user_id, None)
            .await
            .expect("create_device");

        assert!(storage.device_exists(device_id).await.expect("device_exists should succeed"));
    }

    #[tokio::test]
    async fn test_device_exists_returns_false_for_unknown_device() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);

        assert!(!storage
            .device_exists("no_such_device_xyz")
            .await
            .expect("device_exists should succeed"));
    }

    #[tokio::test]
    async fn test_get_device_count_counts_correctly() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().simple().to_string().split_at(12).0.to_string();
        let user_id = format!("@countusr_{}:example.com", suffix);

        ensure_test_user(&pool, &user_id).await;
        let _ = storage.delete_user_devices(&user_id).await;

        assert_eq!(
            storage.get_device_count(&user_id).await.expect("get_device_count"),
            0
        );

        let d1 = format!("cnt_1_{}", suffix);
        let d2 = format!("cnt_2_{}", suffix);
        storage.create_device(&d1, &user_id, None).await.expect("create d1");
        storage.create_device(&d2, &user_id, None).await.expect("create d2");

        assert_eq!(
            storage.get_device_count(&user_id).await.expect("get_device_count"),
            2
        );
    }

    #[tokio::test]
    async fn test_delete_user_devices_removes_all() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let user_id = "@purge:example.com";
        let d1 = &format!("purge_1_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);
        let d2 = &format!("purge_2_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);

        ensure_test_user(&pool, user_id).await;
        storage.create_device(d1, user_id, None).await.expect("create d1");
        storage.create_device(d2, user_id, None).await.expect("create d2");

        storage
            .delete_user_devices(user_id)
            .await
            .expect("delete_user_devices should succeed");

        assert_eq!(
            storage.get_device_count(user_id).await.expect("get_device_count"),
            0
        );
    }

    #[tokio::test]
    async fn test_get_user_device_finds_by_user_and_device() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let device_id = &format!("dev_userdev_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);
        let user_id = "@userdev:example.com";

        ensure_test_user(&pool, user_id).await;
        storage
            .create_device(device_id, user_id, Some("Specific"))
            .await
            .expect("create_device");

        let found = storage
            .get_user_device(user_id, device_id)
            .await
            .expect("get_user_device should succeed");

        assert!(found.is_some());
        assert_eq!(found.unwrap().device_id, device_id.as_str());
    }

    #[tokio::test]
    async fn test_get_user_device_returns_none_for_wrong_user() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let device_id = &format!("dev_wrongusr2_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);
        let user_id = "@owner:example.com";

        ensure_test_user(&pool, user_id).await;
        ensure_test_user(&pool, "@other:example.com").await;
        storage
            .create_device(device_id, user_id, None)
            .await
            .expect("create_device");

        let found = storage
            .get_user_device("@other:example.com", device_id)
            .await
            .expect("get_user_device should succeed");

        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_get_devices_batch_returns_requested_devices() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let user_id = "@batchusr:example.com";
        let d1 = &format!("batch_1_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);
        let d2 = &format!("batch_2_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);

        ensure_test_user(&pool, user_id).await;
        storage.create_device(d1, user_id, None).await.expect("create d1");
        storage.create_device(d2, user_id, None).await.expect("create d2");

        let devices = storage
            .get_devices_batch(&[d1.clone(), d2.clone(), "no_such".to_string()])
            .await
            .expect("get_devices_batch should succeed");

        assert_eq!(devices.len(), 2);
    }

    #[tokio::test]
    async fn test_get_devices_batch_empty_input_returns_empty() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);

        let devices = storage
            .get_devices_batch(&[])
            .await
            .expect("get_devices_batch with empty input should succeed");

        assert!(devices.is_empty());
    }

    #[tokio::test]
    async fn test_filter_existing_users_returns_only_users_with_devices() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().simple().to_string().split_at(12).0.to_string();
        let user_id = format!("@filterusr_{}:example.com", suffix);
        let no_device_user = format!("@nodevusr_{}:example.com", suffix);

        ensure_test_user(&pool, &user_id).await;
        ensure_test_user(&pool, &no_device_user).await;
        let _ = storage.delete_user_devices(&user_id).await;

        storage
            .create_device(&format!("filter_dev_{}", suffix), &user_id, None)
            .await
            .expect("create_device");

        let result = storage
            .filter_existing_users(&[user_id.clone(), no_device_user.clone()])
            .await
            .expect("filter_existing_users should succeed");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], user_id);
    }

    #[tokio::test]
    async fn test_filter_existing_users_empty_input_returns_empty() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);

        let result = storage
            .filter_existing_users(&[])
            .await
            .expect("filter_existing_users with empty input should succeed");

        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_get_max_device_list_stream_id_returns_zero_for_empty() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);

        let max_id = storage
            .get_max_device_list_stream_id()
            .await
            .expect("get_max_device_list_stream_id should succeed");

        assert!(max_id >= 0);
    }

    #[tokio::test]
    async fn test_device_list_stream_id_advances_after_create_device() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let user_id = "@streamusr:example.com";
        let device_id = &format!("stream_dev_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);

        ensure_test_user(&pool, user_id).await;

        let before = storage.get_max_device_list_stream_id().await.expect("get max before");

        storage
            .create_device(device_id, user_id, None)
            .await
            .expect("create_device");

        let after = storage.get_max_device_list_stream_id().await.expect("get max after");
        assert!(after > before);
    }

    #[tokio::test]
    async fn test_delete_device_by_id_cleans_up_record() {
        let pool = test_pool().await;
        let storage = DeviceStorage::new(&pool);
        let device_id = &format!("dev_delbyid_{}", uuid::Uuid::new_v4().simple().to_string().split_at(12).0);
        let user_id = "@delbyid:example.com";

        ensure_test_user(&pool, user_id).await;
        storage
            .create_device(device_id, user_id, None)
            .await
            .expect("create_device");

        storage
            .delete_device(device_id)
            .await
            .expect("delete_device should succeed");

        assert!(storage.get_device(device_id).await.expect("get_device").is_none());
    }
}
