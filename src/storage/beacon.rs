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
    pub updated_ts: i64,
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

pub struct BeaconStorage {
    pool: Arc<Pool<Postgres>>,
}

impl BeaconStorage {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    pub async fn create_beacon_info(
        &self,
        params: CreateBeaconInfoParams,
    ) -> Result<BeaconInfo, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = if params.timeout > 0 {
            Some(params.created_ts + params.timeout)
        } else {
            None
        };

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

    pub async fn deactivate_beacons_by_state_key(
        &self,
        room_id: &str,
        state_key: &str,
    ) -> Result<u64, sqlx::Error> {
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

    pub async fn get_beacon_info(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> Result<Option<BeaconInfo>, sqlx::Error> {
        let row = sqlx::query_as::<_, BeaconInfo>(
            r#"
            SELECT * FROM beacon_info
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
            SELECT * FROM beacon_info
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
            SELECT * FROM beacon_info
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

    pub async fn delete_beacon_info(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> Result<bool, sqlx::Error> {
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
            self.update_beacon_expiry(&params.beacon_info_id, params.timestamp)
                .await?;
        }

        Ok(row)
    }

    async fn update_beacon_expiry(
        &self,
        beacon_info_id: &str,
        location_ts: i64,
    ) -> Result<(), sqlx::Error> {
        let beacon_info =
            sqlx::query_as::<_, BeaconInfo>("SELECT * FROM beacon_info WHERE event_id = $1")
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
            SELECT * FROM beacon_locations
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

    pub async fn get_latest_location(
        &self,
        beacon_info_id: &str,
    ) -> Result<Option<BeaconLocation>, sqlx::Error> {
        let row = sqlx::query_as::<_, BeaconLocation>(
            r#"
            SELECT * FROM beacon_locations
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

    pub async fn count_locations_in_room_since(
        &self,
        room_id: &str,
        since_ts: i64,
    ) -> Result<i64, sqlx::Error> {
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
            Ok(Some(BeaconInfoWithLocations {
                beacon_info: info,
                locations,
            }))
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
                SELECT * FROM beacon_info
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
                SELECT * FROM beacon_info
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

        let mut result = Vec::new();
        for info in beacon_infos {
            let locations = self.get_beacon_locations(&info.event_id, None).await?;
            result.push(BeaconInfoWithLocations {
                beacon_info: info,
                locations,
            });
        }

        Ok(result)
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
            timeout: 3600000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
            expires_at: Some(1234571490000),
        };

        assert_eq!(beacon.room_id, "!room:example.com");
        assert_eq!(beacon.timeout, 3600000);
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
            timeout: 3600000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: 1234567890000,
        };

        assert_eq!(params.timeout, 3600000);
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
