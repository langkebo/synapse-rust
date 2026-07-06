use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BeaconInfo {
    pub id: i64,
    pub room_id: String,
    pub event_id: String,
    pub state_key: String,
    pub sender: String,
    pub description: Option<String>,
    pub timeout: i64,
    pub is_live: bool,
    pub asset_type: String,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBeaconInfoParams {
    pub room_id: String,
    pub event_id: String,
    pub state_key: String,
    pub sender: String,
    pub description: Option<String>,
    pub timeout: i64,
    pub is_live: bool,
    pub asset_type: String,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BeaconLocation {
    pub id: i64,
    pub room_id: String,
    pub event_id: String,
    pub beacon_info_id: String,
    pub sender: String,
    pub uri: String,
    pub description: Option<String>,
    pub timestamp: i64,
    pub accuracy: Option<i64>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBeaconLocationParams {
    pub room_id: String,
    pub event_id: String,
    pub beacon_info_id: String,
    pub sender: String,
    pub uri: String,
    pub description: Option<String>,
    pub timestamp: i64,
    pub accuracy: Option<i64>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconInfoWithLocations {
    pub beacon_info: BeaconInfo,
    pub locations: Vec<BeaconLocation>,
}

/// Trait abstraction over [`BeaconStorage`] for testability and service wiring.
#[async_trait]
pub trait BeaconStoreApi {
    async fn create_beacon_info(&self, params: CreateBeaconInfoParams) -> Result<BeaconInfo, sqlx::Error>;

    async fn deactivate_beacons_by_state_key(&self, room_id: &str, state_key: &str) -> Result<u64, sqlx::Error>;

    async fn get_beacon_info(&self, room_id: &str, event_id: &str) -> Result<Option<BeaconInfo>, sqlx::Error>;

    async fn get_beacon_info_by_state_key(
        &self,
        room_id: &str,
        state_key: &str,
    ) -> Result<Vec<BeaconInfo>, sqlx::Error>;

    async fn get_active_beacons(&self, room_id: &str) -> Result<Vec<BeaconInfo>, sqlx::Error>;

    async fn update_beacon_info(
        &self,
        room_id: &str,
        event_id: &str,
        is_live: bool,
        timeout: Option<i64>,
    ) -> Result<Option<BeaconInfo>, sqlx::Error>;

    async fn delete_beacon_info(&self, room_id: &str, event_id: &str) -> Result<bool, sqlx::Error>;

    async fn create_beacon_location(&self, params: CreateBeaconLocationParams) -> Result<BeaconLocation, sqlx::Error>;

    async fn get_beacon_locations(
        &self,
        beacon_info_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<BeaconLocation>, sqlx::Error>;

    async fn get_beacon_locations_batch(
        &self,
        beacon_info_ids: &[String],
        limit: Option<i64>,
    ) -> Result<std::collections::HashMap<String, Vec<BeaconLocation>>, sqlx::Error>;

    async fn get_latest_location(&self, beacon_info_id: &str) -> Result<Option<BeaconLocation>, sqlx::Error>;

    async fn count_locations_in_room_since(&self, room_id: &str, since_ts: i64) -> Result<i64, sqlx::Error>;

    async fn count_locations_in_room_by_sender_since(
        &self,
        room_id: &str,
        sender: &str,
        since_ts: i64,
    ) -> Result<i64, sqlx::Error>;

    async fn get_joined_member_count(&self, room_id: &str) -> Result<i64, sqlx::Error>;

    async fn get_beacon_with_locations(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> Result<Option<BeaconInfoWithLocations>, sqlx::Error>;

    async fn cleanup_expired_beacons(&self) -> Result<u64, sqlx::Error>;

    async fn get_room_beacons(
        &self,
        room_id: &str,
        include_expired: bool,
    ) -> Result<Vec<BeaconInfoWithLocations>, sqlx::Error>;
}

pub struct BeaconStorage {
    pool: Arc<Pool<Postgres>>,
}

impl BeaconStorage {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    pub async fn create_beacon_info(&self, params: CreateBeaconInfoParams) -> Result<BeaconInfo, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = if params.timeout > 0 { Some(params.created_ts + params.timeout) } else { None };

        let row = sqlx::query_as::<_, BeaconInfo>(
            r#"
            INSERT INTO beacon_info (
                room_id, event_id, state_key, sender, description,
                timeout, is_live, asset_type, created_ts, updated_ts, expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            "#,
        )
        .bind(&params.room_id)
        .bind(&params.event_id)
        .bind(&params.state_key)
        .bind(&params.sender)
        .bind(&params.description)
        .bind(params.timeout)
        .bind(params.is_live)
        .bind(&params.asset_type)
        .bind(params.created_ts)
        .bind(now)
        .bind(expires_at)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn deactivate_beacons_by_state_key(&self, room_id: &str, state_key: &str) -> Result<u64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query(
            r#"
            UPDATE beacon_info
            SET is_live = false, updated_ts = $3
            WHERE room_id = $1 AND state_key = $2 AND is_live = true
            "#,
        )
        .bind(room_id)
        .bind(state_key)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn get_beacon_info(&self, room_id: &str, event_id: &str) -> Result<Option<BeaconInfo>, sqlx::Error> {
        let row = sqlx::query_as::<_, BeaconInfo>(
            r#"
            SELECT id, room_id, event_id, state_key, sender, description, timeout, is_live, asset_type, created_ts, updated_ts, expires_at FROM beacon_info
            WHERE room_id = $1 AND event_id = $2
            "#,
        )
        .bind(room_id)
        .bind(event_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_beacon_info_by_state_key(
        &self,
        room_id: &str,
        state_key: &str,
    ) -> Result<Vec<BeaconInfo>, sqlx::Error> {
        let rows = sqlx::query_as::<_, BeaconInfo>(
            r#"
            SELECT id, room_id, event_id, state_key, sender, description, timeout, is_live, asset_type, created_ts, updated_ts, expires_at FROM beacon_info
            WHERE room_id = $1 AND state_key = $2
            ORDER BY created_ts DESC
            "#,
        )
        .bind(room_id)
        .bind(state_key)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_active_beacons(&self, room_id: &str) -> Result<Vec<BeaconInfo>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let rows = sqlx::query_as::<_, BeaconInfo>(
            r#"
            SELECT id, room_id, event_id, state_key, sender, description, timeout, is_live, asset_type, created_ts, updated_ts, expires_at FROM beacon_info
            WHERE room_id = $1
              AND is_live = true
              AND (expires_at IS NULL OR expires_at > $2)
            ORDER BY created_ts DESC
            "#,
        )
        .bind(room_id)
        .bind(now)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn update_beacon_info(
        &self,
        room_id: &str,
        event_id: &str,
        is_live: bool,
        timeout: Option<i64>,
    ) -> Result<Option<BeaconInfo>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let expires_at = timeout.map(|t| now + t);

        let row = sqlx::query_as::<_, BeaconInfo>(
            r#"
            UPDATE beacon_info
            SET is_live = $3, timeout = COALESCE($4, timeout),
                expires_at = COALESCE($5, expires_at), updated_ts = $6
            WHERE room_id = $1 AND event_id = $2
            RETURNING *
            "#,
        )
        .bind(room_id)
        .bind(event_id)
        .bind(is_live)
        .bind(timeout)
        .bind(expires_at)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_beacon_info(&self, room_id: &str, event_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM beacon_info
            WHERE room_id = $1 AND event_id = $2
            "#,
        )
        .bind(room_id)
        .bind(event_id)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn create_beacon_location(
        &self,
        params: CreateBeaconLocationParams,
    ) -> Result<BeaconLocation, sqlx::Error> {
        let row = sqlx::query_as::<_, BeaconLocation>(
            r#"
            INSERT INTO beacon_locations (
                room_id, event_id, beacon_info_id, sender, uri,
                description, timestamp, accuracy, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            "#,
        )
        .bind(&params.room_id)
        .bind(&params.event_id)
        .bind(&params.beacon_info_id)
        .bind(&params.sender)
        .bind(&params.uri)
        .bind(&params.description)
        .bind(params.timestamp)
        .bind(params.accuracy)
        .bind(params.created_ts)
        .fetch_one(&*self.pool)
        .await?;

        if params.timestamp > 0 {
            self.update_beacon_expiry(&params.beacon_info_id, params.timestamp).await?;
        }

        Ok(row)
    }

    async fn update_beacon_expiry(&self, beacon_info_id: &str, location_ts: i64) -> Result<(), sqlx::Error> {
        let beacon_info = sqlx::query_as::<_, BeaconInfo>("SELECT id, room_id, event_id, state_key, sender, description, timeout, is_live, asset_type, created_ts, updated_ts, expires_at FROM beacon_info WHERE event_id = $1")
            .bind(beacon_info_id)
            .fetch_optional(&*self.pool)
            .await?;

        if let Some(info) = beacon_info {
            if info.timeout > 0 {
                let new_expiry = location_ts + info.timeout;
                sqlx::query(
                    r#"
                    UPDATE beacon_info
                    SET expires_at = $2, updated_ts = $3
                    WHERE event_id = $1
                    "#,
                )
                .bind(beacon_info_id)
                .bind(new_expiry)
                .bind(chrono::Utc::now().timestamp_millis())
                .execute(&*self.pool)
                .await?;
            }
        }

        Ok(())
    }

    pub async fn get_beacon_locations(
        &self,
        beacon_info_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<BeaconLocation>, sqlx::Error> {
        let limit = limit.unwrap_or(100);

        let rows = sqlx::query_as::<_, BeaconLocation>(
            r#"
            SELECT id, room_id, event_id, beacon_info_id, sender, uri, description, timestamp, accuracy, created_ts FROM beacon_locations
            WHERE beacon_info_id = $1
            ORDER BY timestamp DESC
            LIMIT $2
            "#,
        )
        .bind(beacon_info_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// Batch variant of [`get_beacon_locations`] that fetches locations for
    /// multiple beacon info ids in a single query, respecting the same
    /// per-beacon limit via a window function.
    pub async fn get_beacon_locations_batch(
        &self,
        beacon_info_ids: &[String],
        limit: Option<i64>,
    ) -> Result<std::collections::HashMap<String, Vec<BeaconLocation>>, sqlx::Error> {
        if beacon_info_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let limit = limit.unwrap_or(100);

        let rows = sqlx::query_as::<_, BeaconLocation>(
            r#"
            SELECT id, room_id, event_id, beacon_info_id, sender, uri, description, timestamp, accuracy, created_ts
            FROM (
                SELECT id, room_id, event_id, beacon_info_id, sender, uri, description, timestamp, accuracy, created_ts,
                       ROW_NUMBER() OVER (PARTITION BY beacon_info_id ORDER BY timestamp DESC) AS rn
                FROM beacon_locations
                WHERE beacon_info_id = ANY($1)
            ) t
            WHERE rn <= $2
            "#,
        )
        .bind(beacon_info_ids)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        let mut result: std::collections::HashMap<String, Vec<BeaconLocation>> =
            beacon_info_ids.iter().map(|id| (id.clone(), Vec::new())).collect();

        for location in rows {
            if let Some(locations) = result.get_mut(&location.beacon_info_id) {
                locations.push(location);
            }
        }

        Ok(result)
    }

    pub async fn get_latest_location(&self, beacon_info_id: &str) -> Result<Option<BeaconLocation>, sqlx::Error> {
        let row = sqlx::query_as::<_, BeaconLocation>(
            r#"
            SELECT id, room_id, event_id, beacon_info_id, sender, uri, description, timestamp, accuracy, created_ts FROM beacon_locations
            WHERE beacon_info_id = $1
            ORDER BY timestamp DESC
            LIMIT 1
            "#,
        )
        .bind(beacon_info_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn count_locations_in_room_since(&self, room_id: &str, since_ts: i64) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM beacon_locations
            WHERE room_id = $1 AND created_ts >= $2
            "#,
        )
        .bind(room_id)
        .bind(since_ts)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn count_locations_in_room_by_sender_since(
        &self,
        room_id: &str,
        sender: &str,
        since_ts: i64,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM beacon_locations
            WHERE room_id = $1 AND sender = $2 AND created_ts >= $3
            "#,
        )
        .bind(room_id)
        .bind(sender)
        .bind(since_ts)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_joined_member_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(COUNT(*), 0)
            FROM room_memberships
            WHERE room_id = $1 AND membership = 'join'
            "#,
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_beacon_with_locations(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> Result<Option<BeaconInfoWithLocations>, sqlx::Error> {
        let beacon_info = self.get_beacon_info(room_id, event_id).await?;

        if let Some(info) = beacon_info {
            let locations = self.get_beacon_locations(&info.event_id, None).await?;
            Ok(Some(BeaconInfoWithLocations { beacon_info: info, locations }))
        } else {
            Ok(None)
        }
    }

    pub async fn cleanup_expired_beacons(&self) -> Result<u64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r#"
            DELETE FROM beacon_info
            WHERE expires_at IS NOT NULL AND expires_at < $1
            "#,
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn get_room_beacons(
        &self,
        room_id: &str,
        include_expired: bool,
    ) -> Result<Vec<BeaconInfoWithLocations>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let beacon_infos = if include_expired {
            sqlx::query_as::<_, BeaconInfo>(
                r#"
                SELECT id, room_id, event_id, state_key, sender, description, timeout, is_live, asset_type, created_ts, updated_ts, expires_at FROM beacon_info
                WHERE room_id = $1
                ORDER BY created_ts DESC
                "#,
            )
            .bind(room_id)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as::<_, BeaconInfo>(
                r#"
                SELECT id, room_id, event_id, state_key, sender, description, timeout, is_live, asset_type, created_ts, updated_ts, expires_at FROM beacon_info
                WHERE room_id = $1
                  AND (expires_at IS NULL OR expires_at > $2)
                ORDER BY created_ts DESC
                "#,
            )
            .bind(room_id)
            .bind(now)
            .fetch_all(&*self.pool)
            .await?
        };

        let beacon_info_ids: Vec<String> = beacon_infos.iter().map(|info| info.event_id.clone()).collect();
        let locations_map = self.get_beacon_locations_batch(&beacon_info_ids, None).await?;

        let result = beacon_infos
            .into_iter()
            .map(|info| {
                let locations = locations_map.get(&info.event_id).cloned().unwrap_or_default();
                BeaconInfoWithLocations { beacon_info: info, locations }
            })
            .collect();

        Ok(result)
    }
}

#[async_trait]
impl BeaconStoreApi for BeaconStorage {
    async fn create_beacon_info(&self, params: CreateBeaconInfoParams) -> Result<BeaconInfo, sqlx::Error> {
        self.create_beacon_info(params).await
    }

    async fn deactivate_beacons_by_state_key(&self, room_id: &str, state_key: &str) -> Result<u64, sqlx::Error> {
        self.deactivate_beacons_by_state_key(room_id, state_key).await
    }

    async fn get_beacon_info(&self, room_id: &str, event_id: &str) -> Result<Option<BeaconInfo>, sqlx::Error> {
        self.get_beacon_info(room_id, event_id).await
    }

    async fn get_beacon_info_by_state_key(
        &self,
        room_id: &str,
        state_key: &str,
    ) -> Result<Vec<BeaconInfo>, sqlx::Error> {
        self.get_beacon_info_by_state_key(room_id, state_key).await
    }

    async fn get_active_beacons(&self, room_id: &str) -> Result<Vec<BeaconInfo>, sqlx::Error> {
        self.get_active_beacons(room_id).await
    }

    async fn update_beacon_info(
        &self,
        room_id: &str,
        event_id: &str,
        is_live: bool,
        timeout: Option<i64>,
    ) -> Result<Option<BeaconInfo>, sqlx::Error> {
        self.update_beacon_info(room_id, event_id, is_live, timeout).await
    }

    async fn delete_beacon_info(&self, room_id: &str, event_id: &str) -> Result<bool, sqlx::Error> {
        self.delete_beacon_info(room_id, event_id).await
    }

    async fn create_beacon_location(&self, params: CreateBeaconLocationParams) -> Result<BeaconLocation, sqlx::Error> {
        self.create_beacon_location(params).await
    }

    async fn get_beacon_locations(
        &self,
        beacon_info_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<BeaconLocation>, sqlx::Error> {
        self.get_beacon_locations(beacon_info_id, limit).await
    }

    async fn get_beacon_locations_batch(
        &self,
        beacon_info_ids: &[String],
        limit: Option<i64>,
    ) -> Result<std::collections::HashMap<String, Vec<BeaconLocation>>, sqlx::Error> {
        self.get_beacon_locations_batch(beacon_info_ids, limit).await
    }

    async fn get_latest_location(&self, beacon_info_id: &str) -> Result<Option<BeaconLocation>, sqlx::Error> {
        self.get_latest_location(beacon_info_id).await
    }

    async fn count_locations_in_room_since(&self, room_id: &str, since_ts: i64) -> Result<i64, sqlx::Error> {
        self.count_locations_in_room_since(room_id, since_ts).await
    }

    async fn count_locations_in_room_by_sender_since(
        &self,
        room_id: &str,
        sender: &str,
        since_ts: i64,
    ) -> Result<i64, sqlx::Error> {
        self.count_locations_in_room_by_sender_since(room_id, sender, since_ts).await
    }

    async fn get_joined_member_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        self.get_joined_member_count(room_id).await
    }

    async fn get_beacon_with_locations(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> Result<Option<BeaconInfoWithLocations>, sqlx::Error> {
        self.get_beacon_with_locations(room_id, event_id).await
    }

    async fn cleanup_expired_beacons(&self) -> Result<u64, sqlx::Error> {
        self.cleanup_expired_beacons().await
    }

    async fn get_room_beacons(
        &self,
        room_id: &str,
        include_expired: bool,
    ) -> Result<Vec<BeaconInfoWithLocations>, sqlx::Error> {
        self.get_room_beacons(room_id, include_expired).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beacon_info_struct() {
        let beacon = BeaconInfo {
            id: 1,
            room_id: "!room:example.com".to_string(),
            event_id: "$beacon_info_1".to_string(),
            state_key: "@alice:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            description: Some("Alice's location".to_string()),
            timeout: 3_600_000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: 1234567890000,
            updated_ts: Some(1234567890000),
            expires_at: Some(1234571490000),
        };

        assert_eq!(beacon.room_id, "!room:example.com");
        assert_eq!(beacon.timeout, 3_600_000);
        assert!(beacon.is_live);
    }

    #[test]
    fn test_beacon_location_struct() {
        let location = BeaconLocation {
            id: 1,
            room_id: "!room:example.com".to_string(),
            event_id: "$beacon_1".to_string(),
            beacon_info_id: "$beacon_info_1".to_string(),
            sender: "@alice:example.com".to_string(),
            uri: "geo:51.5008,0.1247;u=35".to_string(),
            description: Some("London".to_string()),
            timestamp: 1234567890000,
            accuracy: Some(35),
            created_ts: 1234567890000,
        };

        assert_eq!(location.uri, "geo:51.5008,0.1247;u=35");
        assert_eq!(location.accuracy, Some(35));
    }

    #[test]
    fn test_create_beacon_info_params() {
        let params = CreateBeaconInfoParams {
            room_id: "!room:example.com".to_string(),
            event_id: "$beacon_info_1".to_string(),
            state_key: "@alice:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            description: Some("Alice's location".to_string()),
            timeout: 3_600_000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: 1234567890000,
        };

        assert_eq!(params.timeout, 3_600_000);
        assert_eq!(params.asset_type, "m.self");
    }

    #[test]
    fn test_create_beacon_location_params() {
        let params = CreateBeaconLocationParams {
            room_id: "!room:example.com".to_string(),
            event_id: "$beacon_1".to_string(),
            beacon_info_id: "$beacon_info_1".to_string(),
            sender: "@alice:example.com".to_string(),
            uri: "geo:51.5008,0.1247;u=35".to_string(),
            description: Some("London".to_string()),
            timestamp: 1234567890000,
            accuracy: Some(35),
            created_ts: 1234567890000,
        };

        assert!(params.uri.starts_with("geo:"));
        assert!(params.accuracy.is_some());
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn test_pool() -> Arc<Pool<Postgres>> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    fn unique_room_id() -> String {
        format!("!db_test_{}:example.com", uuid::Uuid::new_v4())
    }

    fn unique_event_id() -> String {
        format!("$db_beacon_{}", uuid::Uuid::new_v4())
    }

    /// Create a beacon_info record with default fields, returning the created record.
    async fn seed_beacon_info(storage: &BeaconStorage, room_id: &str, event_id: &str, state_key: &str) -> BeaconInfo {
        storage
            .create_beacon_info(CreateBeaconInfoParams {
                room_id: room_id.to_string(),
                event_id: event_id.to_string(),
                state_key: state_key.to_string(),
                sender: "@alice:example.com".to_string(),
                description: Some("Seed beacon".to_string()),
                timeout: 3_600_000,
                is_live: true,
                asset_type: "m.self".to_string(),
                created_ts: 1700000000000,
            })
            .await
            .expect("seed_beacon_info should succeed")
    }

    // --- BeaconInfo CRUD tests ---

    #[tokio::test]
    async fn test_create_and_get_beacon_info() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));
        let room_id = unique_room_id();
        let event_id = unique_event_id();
        let created_ts: i64 = 1700000000000;
        let timeout: i64 = 3_600_000;

        let params = CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: event_id.clone(),
            state_key: "@alice:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            description: Some("Test beacon".to_string()),
            timeout,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts,
        };

        let created = storage.create_beacon_info(params).await.expect("create_beacon_info should succeed");

        assert_eq!(created.room_id, room_id);
        assert_eq!(created.event_id, event_id);
        assert_eq!(created.sender, "@alice:example.com");
        assert_eq!(created.description.as_deref(), Some("Test beacon"));
        assert_eq!(created.timeout, timeout);
        assert!(created.is_live);
        assert_eq!(created.asset_type, "m.self");
        assert_eq!(created.created_ts, created_ts);
        assert_eq!(created.expires_at, Some(created_ts + timeout));
        assert!(created.id > 0);

        let fetched = storage
            .get_beacon_info(&room_id, &event_id)
            .await
            .expect("get_beacon_info query should succeed")
            .expect("beacon_info should be found");

        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.event_id, event_id);
    }

    #[tokio::test]
    async fn test_get_beacon_info_not_found() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));

        let result =
            storage.get_beacon_info("!nonexistent:example.com", "$nonexistent").await.expect("query should succeed");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_beacon_info_by_state_key_ordering() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));
        let room_id = unique_room_id();
        let state_key = format!("@alice_{}:example.com", uuid::Uuid::new_v4());

        // Create two beacons with same state_key, different created_ts
        storage
            .create_beacon_info(CreateBeaconInfoParams {
                room_id: room_id.clone(),
                event_id: format!("$older_{}", uuid::Uuid::new_v4()),
                state_key: state_key.clone(),
                sender: "@alice:example.com".to_string(),
                description: None,
                timeout: 3_600_000,
                is_live: true,
                asset_type: "m.self".to_string(),
                created_ts: 1000,
            })
            .await
            .unwrap();

        storage
            .create_beacon_info(CreateBeaconInfoParams {
                room_id: room_id.clone(),
                event_id: format!("$newer_{}", uuid::Uuid::new_v4()),
                state_key: state_key.clone(),
                sender: "@alice:example.com".to_string(),
                description: None,
                timeout: 3_600_000,
                is_live: true,
                asset_type: "m.self".to_string(),
                created_ts: 2000,
            })
            .await
            .unwrap();

        let results = storage.get_beacon_info_by_state_key(&room_id, &state_key).await.expect("query should succeed");

        assert_eq!(results.len(), 2);
        // ORDER BY created_ts DESC — newest first
        assert_eq!(results[0].created_ts, 2000);
        assert_eq!(results[1].created_ts, 1000);
    }

    #[tokio::test]
    async fn test_get_active_beacons_filters_correctly() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));
        let room_id = unique_room_id();
        let far_future = chrono::Utc::now().timestamp_millis() + 86_400_000; // 1 day from now

        // Active: is_live=true, expires_at in the future
        storage
            .create_beacon_info(CreateBeaconInfoParams {
                room_id: room_id.clone(),
                event_id: format!("$active_{}", uuid::Uuid::new_v4()),
                state_key: "@active:example.com".to_string(),
                sender: "@alice:example.com".to_string(),
                description: None,
                timeout: 86_400_000,
                is_live: true,
                asset_type: "m.self".to_string(),
                created_ts: far_future - 86_400_000,
            })
            .await
            .unwrap();

        // Expired: is_live=true but expires_at in the past
        storage
            .create_beacon_info(CreateBeaconInfoParams {
                room_id: room_id.clone(),
                event_id: format!("$expired_{}", uuid::Uuid::new_v4()),
                state_key: "@expired:example.com".to_string(),
                sender: "@alice:example.com".to_string(),
                description: None,
                timeout: 1,
                is_live: true,
                asset_type: "m.self".to_string(),
                created_ts: 1000, // expires_at = 1001, well in the past
            })
            .await
            .unwrap();

        // Not live: is_live=false
        storage
            .create_beacon_info(CreateBeaconInfoParams {
                room_id: room_id.clone(),
                event_id: format!("$inactive_{}", uuid::Uuid::new_v4()),
                state_key: "@inactive:example.com".to_string(),
                sender: "@alice:example.com".to_string(),
                description: None,
                timeout: 3_600_000,
                is_live: false,
                asset_type: "m.self".to_string(),
                created_ts: 2000,
            })
            .await
            .unwrap();

        let active = storage.get_active_beacons(&room_id).await.expect("query should succeed");
        assert_eq!(active.len(), 1);
        assert!(active[0].event_id.contains("active"));
    }

    #[tokio::test]
    async fn test_deactivate_beacons_by_state_key() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));
        let room_id = unique_room_id();
        let state_key = format!("@bob_{}:example.com", uuid::Uuid::new_v4());

        let info = seed_beacon_info(&storage, &room_id, &format!("$evt_{}", uuid::Uuid::new_v4()), &state_key).await;
        assert!(info.is_live);

        let deactivated =
            storage.deactivate_beacons_by_state_key(&room_id, &state_key).await.expect("deactivate should succeed");

        assert_eq!(deactivated, 1, "should deactivate exactly 1 beacon");

        // Second call should affect 0 rows since it's already deactivated
        let second = storage
            .deactivate_beacons_by_state_key(&room_id, &state_key)
            .await
            .expect("second deactivate should succeed");

        assert_eq!(second, 0, "already deactivated, should return 0");

        let fetched = storage.get_beacon_info(&room_id, &info.event_id).await.unwrap().unwrap();
        assert!(!fetched.is_live, "beacon should no longer be live");
    }

    #[tokio::test]
    async fn test_update_beacon_info_collesce_behavior() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));
        let room_id = unique_room_id();
        let event_id = unique_event_id();

        let created = seed_beacon_info(&storage, &room_id, &event_id, "@charlie:example.com").await;
        assert!(created.is_live);
        assert_eq!(created.timeout, 3_600_000);

        // Update with new timeout — expires_at should be recalculated
        let updated = storage
            .update_beacon_info(&room_id, &event_id, true, Some(7_200_000))
            .await
            .expect("update should succeed")
            .expect("should return a record");

        assert_eq!(updated.timeout, 7_200_000);
        assert!(updated.is_live);
        assert!(updated.expires_at.is_some());

        // Update with is_live=false, None timeout — timeout/expires_at should be COALESCEd (keep old values)
        let updated2 = storage
            .update_beacon_info(&room_id, &event_id, false, None)
            .await
            .expect("update should succeed")
            .expect("should return a record");

        assert!(!updated2.is_live);
        assert_eq!(updated2.timeout, 7_200_000, "timeout should be preserved via COALESCE");
    }

    #[tokio::test]
    async fn test_delete_beacon_info() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));
        let room_id = unique_room_id();
        let event_id = unique_event_id();

        seed_beacon_info(&storage, &room_id, &event_id, "@dave:example.com").await;

        let deleted = storage.delete_beacon_info(&room_id, &event_id).await.expect("delete should succeed");

        assert!(deleted, "should return true when row is deleted");

        // Verify it's gone
        let fetched = storage.get_beacon_info(&room_id, &event_id).await.unwrap();
        assert!(fetched.is_none());

        // Deleting again should return false
        let deleted_again =
            storage.delete_beacon_info(&room_id, &event_id).await.expect("second delete should succeed");

        assert!(!deleted_again, "should return false when no row exists");
    }

    // --- BeaconLocation tests ---

    #[tokio::test]
    async fn test_create_and_get_beacon_locations() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));
        let room_id = unique_room_id();
        let beacon_event_id = unique_event_id();

        // Need beacon_info first — create_beacon_location calls update_beacon_expiry internally
        seed_beacon_info(&storage, &room_id, &beacon_event_id, "@alice:example.com").await;

        // Create two locations with different timestamps
        let loc1 = storage
            .create_beacon_location(CreateBeaconLocationParams {
                room_id: room_id.clone(),
                event_id: format!("$loc1_{}", uuid::Uuid::new_v4()),
                beacon_info_id: beacon_event_id.clone(),
                sender: "@alice:example.com".to_string(),
                uri: "geo:51.5008,0.1247;u=35".to_string(),
                description: Some("London".to_string()),
                timestamp: 2000,
                accuracy: Some(35),
                created_ts: 2000,
            })
            .await
            .expect("first location should be created");

        let loc2 = storage
            .create_beacon_location(CreateBeaconLocationParams {
                room_id: room_id.clone(),
                event_id: format!("$loc2_{}", uuid::Uuid::new_v4()),
                beacon_info_id: beacon_event_id.clone(),
                sender: "@alice:example.com".to_string(),
                uri: "geo:48.8566,2.3522;u=50".to_string(),
                description: Some("Paris".to_string()),
                timestamp: 3000,
                accuracy: Some(50),
                created_ts: 3000,
            })
            .await
            .expect("second location should be created");

        assert_eq!(loc1.uri, "geo:51.5008,0.1247;u=35");
        assert_eq!(loc2.uri, "geo:48.8566,2.3522;u=50");

        // get_beacon_locations — ORDER BY timestamp DESC
        let locations = storage.get_beacon_locations(&beacon_event_id, Some(10)).await.expect("query should succeed");
        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0].timestamp, 3000, "newest first");
        assert_eq!(locations[1].timestamp, 2000);

        // Limit
        let limited = storage.get_beacon_locations(&beacon_event_id, Some(1)).await.expect("query should succeed");
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].timestamp, 3000);
    }

    #[tokio::test]
    async fn test_get_latest_location() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));
        let room_id = unique_room_id();
        let beacon_event_id = unique_event_id();

        seed_beacon_info(&storage, &room_id, &beacon_event_id, "@alice:example.com").await;

        storage
            .create_beacon_location(CreateBeaconLocationParams {
                room_id: room_id.clone(),
                event_id: format!("$loc_old_{}", uuid::Uuid::new_v4()),
                beacon_info_id: beacon_event_id.clone(),
                sender: "@alice:example.com".to_string(),
                uri: "geo:51.5008,0.1247;u=35".to_string(),
                description: None,
                timestamp: 1000,
                accuracy: None,
                created_ts: 1000,
            })
            .await
            .unwrap();

        storage
            .create_beacon_location(CreateBeaconLocationParams {
                room_id: room_id.clone(),
                event_id: format!("$loc_new_{}", uuid::Uuid::new_v4()),
                beacon_info_id: beacon_event_id.clone(),
                sender: "@alice:example.com".to_string(),
                uri: "geo:40.7128,-74.0060;u=10".to_string(),
                description: None,
                timestamp: 5000,
                accuracy: None,
                created_ts: 5000,
            })
            .await
            .unwrap();

        let latest = storage
            .get_latest_location(&beacon_event_id)
            .await
            .expect("query should succeed")
            .expect("should find a location");
        assert_eq!(latest.timestamp, 5000);
        assert_eq!(latest.uri, "geo:40.7128,-74.0060;u=10");

        // Nonexistent beacon_info_id
        let none = storage.get_latest_location("$nonexistent").await.expect("query should succeed");
        assert!(none.is_none());
    }

    // --- Counting tests ---

    #[tokio::test]
    async fn test_count_locations_in_room_since() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));
        let room_id = unique_room_id();
        let beacon_event_id = unique_event_id();

        seed_beacon_info(&storage, &room_id, &beacon_event_id, "@alice:example.com").await;

        // Create 3 locations
        for i in 0..3 {
            storage
                .create_beacon_location(CreateBeaconLocationParams {
                    room_id: room_id.clone(),
                    event_id: format!("$loc_{}_{}", i, uuid::Uuid::new_v4()),
                    beacon_info_id: beacon_event_id.clone(),
                    sender: "@alice:example.com".to_string(),
                    uri: format!("geo:{},{};u=10", 51.0 + i as f64, 0.0),
                    description: None,
                    timestamp: (i + 1) * 1000,
                    accuracy: None,
                    created_ts: (i + 1) * 1000,
                })
                .await
                .unwrap();
        }

        // All 3 since ts = 0
        let count_all = storage.count_locations_in_room_since(&room_id, 0).await.expect("query should succeed");
        assert_eq!(count_all, 3);

        // Only 2 since ts = 2000
        let count_since = storage.count_locations_in_room_since(&room_id, 2000).await.expect("query should succeed");
        assert_eq!(count_since, 2);

        // None since ts = 9999
        let count_none = storage.count_locations_in_room_since(&room_id, 9999).await.expect("query should succeed");
        assert_eq!(count_none, 0);
    }

    #[tokio::test]
    async fn test_count_locations_in_room_by_sender_since() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));
        let room_id = unique_room_id();
        let beacon_event_id = unique_event_id();

        seed_beacon_info(&storage, &room_id, &beacon_event_id, "@alice:example.com").await;

        // Alice: 2 locations
        for i in 0..2 {
            storage
                .create_beacon_location(CreateBeaconLocationParams {
                    room_id: room_id.clone(),
                    event_id: format!("$alice_{}_{}", i, uuid::Uuid::new_v4()),
                    beacon_info_id: beacon_event_id.clone(),
                    sender: "@alice:example.com".to_string(),
                    uri: format!("geo:{},{};u=10", 51.0 + i as f64, 0.0),
                    description: None,
                    timestamp: (i + 1) * 1000,
                    accuracy: None,
                    created_ts: (i + 1) * 1000,
                })
                .await
                .unwrap();
        }

        // Bob: 1 location
        storage
            .create_beacon_location(CreateBeaconLocationParams {
                room_id: room_id.clone(),
                event_id: format!("$bob_{}", uuid::Uuid::new_v4()),
                beacon_info_id: beacon_event_id.clone(),
                sender: "@bob:example.com".to_string(),
                uri: "geo:48.8566,2.3522;u=50".to_string(),
                description: None,
                timestamp: 1500,
                accuracy: None,
                created_ts: 1500,
            })
            .await
            .unwrap();

        let alice_count = storage
            .count_locations_in_room_by_sender_since(&room_id, "@alice:example.com", 0)
            .await
            .expect("query should succeed");
        assert_eq!(alice_count, 2);

        let bob_count = storage
            .count_locations_in_room_by_sender_since(&room_id, "@bob:example.com", 0)
            .await
            .expect("query should succeed");
        assert_eq!(bob_count, 1);

        // No locations for unknown sender
        let unknown_count = storage
            .count_locations_in_room_by_sender_since(&room_id, "@unknown:example.com", 0)
            .await
            .expect("query should succeed");
        assert_eq!(unknown_count, 0);
    }

    // --- Combined / aggregate tests ---

    #[tokio::test]
    async fn test_get_beacon_with_locations() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));
        let room_id = unique_room_id();
        let beacon_event_id = unique_event_id();

        seed_beacon_info(&storage, &room_id, &beacon_event_id, "@alice:example.com").await;

        storage
            .create_beacon_location(CreateBeaconLocationParams {
                room_id: room_id.clone(),
                event_id: format!("$loc_{}", uuid::Uuid::new_v4()),
                beacon_info_id: beacon_event_id.clone(),
                sender: "@alice:example.com".to_string(),
                uri: "geo:51.5008,0.1247;u=35".to_string(),
                description: None,
                timestamp: 5000,
                accuracy: None,
                created_ts: 5000,
            })
            .await
            .unwrap();

        let combined = storage
            .get_beacon_with_locations(&room_id, &beacon_event_id)
            .await
            .expect("query should succeed")
            .expect("should find beacon with locations");

        assert_eq!(combined.beacon_info.event_id, beacon_event_id);
        assert_eq!(combined.locations.len(), 1);
        assert_eq!(combined.locations[0].timestamp, 5000);

        // Nonexistent
        let none =
            storage.get_beacon_with_locations("!nonexistent", "$nonexistent").await.expect("query should succeed");
        assert!(none.is_none());
    }

    #[tokio::test]
    async fn test_cleanup_expired_beacons() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));
        let room_id = unique_room_id();

        // Create an expired beacon: created_ts=1, timeout=1 => expires_at=2 (well in the past)
        storage
            .create_beacon_info(CreateBeaconInfoParams {
                room_id: room_id.clone(),
                event_id: format!("$expired_{}", uuid::Uuid::new_v4()),
                state_key: "@expired:example.com".to_string(),
                sender: "@alice:example.com".to_string(),
                description: None,
                timeout: 1,
                is_live: true,
                asset_type: "m.self".to_string(),
                created_ts: 1,
            })
            .await
            .unwrap();

        // Create a non-expired beacon: expires_at=null (timeout=0)
        storage
            .create_beacon_info(CreateBeaconInfoParams {
                room_id: room_id.clone(),
                event_id: format!("$forever_{}", uuid::Uuid::new_v4()),
                state_key: "@forever:example.com".to_string(),
                sender: "@alice:example.com".to_string(),
                description: None,
                timeout: 0, // no expiry
                is_live: true,
                asset_type: "m.self".to_string(),
                created_ts: 1,
            })
            .await
            .unwrap();

        // Create a non-expired beacon: expires_at far in the future
        let far_future = chrono::Utc::now().timestamp_millis() + 86_400_000;
        storage
            .create_beacon_info(CreateBeaconInfoParams {
                room_id: room_id.clone(),
                event_id: format!("$future_{}", uuid::Uuid::new_v4()),
                state_key: "@future:example.com".to_string(),
                sender: "@alice:example.com".to_string(),
                description: None,
                timeout: 86_400_000,
                is_live: true,
                asset_type: "m.self".to_string(),
                created_ts: far_future - 86_400_000,
            })
            .await
            .unwrap();

        // cleanup_expired_beacons is global — delete WHERE expires_at < now
        let deleted = storage.cleanup_expired_beacons().await.expect("cleanup should succeed");
        assert!(deleted >= 1, "should delete at least the expired beacon we just created");
    }

    #[tokio::test]
    async fn test_get_room_beacons_with_and_without_expired() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));
        let room_id = unique_room_id();
        let now = chrono::Utc::now().timestamp_millis();
        let one_day = 86_400_000i64;

        let active_event_id = format!("$active_{}", uuid::Uuid::new_v4());
        let expired_event_id = format!("$expired_{}", uuid::Uuid::new_v4());

        // Active beacon (expires in 1 day from now)
        storage
            .create_beacon_info(CreateBeaconInfoParams {
                room_id: room_id.clone(),
                event_id: active_event_id.clone(),
                state_key: "@active:example.com".to_string(),
                sender: "@alice:example.com".to_string(),
                description: None,
                timeout: one_day,
                is_live: true,
                asset_type: "m.self".to_string(),
                created_ts: now,
            })
            .await
            .unwrap();

        // Expired beacon (expires_at = created_ts + timeout = 1 + 1 = 2, well in the past)
        storage
            .create_beacon_info(CreateBeaconInfoParams {
                room_id: room_id.clone(),
                event_id: expired_event_id.clone(),
                state_key: "@expired:example.com".to_string(),
                sender: "@alice:example.com".to_string(),
                description: None,
                timeout: 1,
                is_live: true,
                asset_type: "m.self".to_string(),
                created_ts: 1,
            })
            .await
            .unwrap();

        // Without expired: only active
        let active_beacons = storage.get_room_beacons(&room_id, false).await.expect("query should succeed");
        assert_eq!(active_beacons.len(), 1);
        assert_eq!(active_beacons[0].beacon_info.event_id, active_event_id);

        // With expired: both
        let all_beacons = storage.get_room_beacons(&room_id, true).await.expect("query should succeed");
        assert_eq!(all_beacons.len(), 2);
    }

    #[tokio::test]
    async fn test_get_beacon_locations_batch() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));
        let room_id = unique_room_id();
        let beacon1_id = unique_event_id();
        let beacon2_id = unique_event_id();

        seed_beacon_info(&storage, &room_id, &beacon1_id, "@alice:example.com").await;
        seed_beacon_info(&storage, &room_id, &beacon2_id, "@bob:example.com").await;

        // Locations for beacon1
        for i in 0..2 {
            storage
                .create_beacon_location(CreateBeaconLocationParams {
                    room_id: room_id.clone(),
                    event_id: format!("$b1_loc_{}_{}", i, uuid::Uuid::new_v4()),
                    beacon_info_id: beacon1_id.clone(),
                    sender: "@alice:example.com".to_string(),
                    uri: format!("geo:{},{};u=10", 51.0 + i as f64, 0.0),
                    description: None,
                    timestamp: (i + 1) * 1000,
                    accuracy: None,
                    created_ts: (i + 1) * 1000,
                })
                .await
                .unwrap();
        }

        // Locations for beacon2
        for i in 0..3 {
            storage
                .create_beacon_location(CreateBeaconLocationParams {
                    room_id: room_id.clone(),
                    event_id: format!("$b2_loc_{}_{}", i, uuid::Uuid::new_v4()),
                    beacon_info_id: beacon2_id.clone(),
                    sender: "@bob:example.com".to_string(),
                    uri: format!("geo:{},{};u=10", 48.0 + i as f64, 2.0),
                    description: None,
                    timestamp: (i + 1) * 500,
                    accuracy: None,
                    created_ts: (i + 1) * 500,
                })
                .await
                .unwrap();
        }

        let ids = vec![beacon1_id.clone(), beacon2_id.clone()];
        let batch = storage.get_beacon_locations_batch(&ids, Some(5)).await.expect("batch query should succeed");

        assert_eq!(batch.len(), 2, "should return entries for both beacon_info_ids");
        assert_eq!(batch.get(&beacon1_id).map(|v| v.len()), Some(2));
        assert_eq!(batch.get(&beacon2_id).map(|v| v.len()), Some(3));

        // Verify limit works
        let limited = storage.get_beacon_locations_batch(&ids, Some(1)).await.expect("batch with limit should succeed");

        assert_eq!(limited.get(&beacon1_id).map(|v| v.len()), Some(1));
        assert_eq!(limited.get(&beacon2_id).map(|v| v.len()), Some(1));

        // Empty input returns empty map
        let empty = storage.get_beacon_locations_batch(&[], Some(10)).await.expect("empty batch should succeed");

        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn test_create_beacon_info_zero_timeout_no_expiry() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));
        let room_id = unique_room_id();
        let event_id = unique_event_id();

        let created = storage
            .create_beacon_info(CreateBeaconInfoParams {
                room_id: room_id.clone(),
                event_id: event_id.clone(),
                state_key: "@forever:example.com".to_string(),
                sender: "@alice:example.com".to_string(),
                description: None,
                timeout: 0,
                is_live: true,
                asset_type: "m.self".to_string(),
                created_ts: 1700000000000,
            })
            .await
            .expect("create with timeout=0 should succeed");

        assert_eq!(created.timeout, 0);
        assert!(created.expires_at.is_none(), "expires_at should be None when timeout is 0");
    }

    #[tokio::test]
    async fn test_update_beacon_info_nonexistent_returns_none() {
        let pool = test_pool().await;
        let storage = BeaconStorage::new(Arc::clone(&pool));

        let result = storage
            .update_beacon_info("!nonexistent", "$nonexistent", false, None)
            .await
            .expect("query should succeed");

        assert!(result.is_none(), "updating nonexistent beacon should return None");
    }
}
