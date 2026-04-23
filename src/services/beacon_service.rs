use crate::cache::CacheManager;
use crate::storage::beacon::{
    BeaconInfo, BeaconInfoWithLocations, BeaconLocation, BeaconStorage, CreateBeaconInfoParams,
    CreateBeaconLocationParams,
};
use std::sync::Arc;

const BEACON_QUOTA_WINDOW_MS: i64 = 60_000;
const BEACON_MAX_PER_SENDER_PER_ROOM_WINDOW: i64 = 10;
const BEACON_MAX_PER_ROOM_WINDOW: i64 = 60;
const BEACON_ROOM_BACKPRESSURE_BUCKET_CAPACITY: u32 = 20;
const BEACON_ROOM_BACKPRESSURE_REFILL_PER_SEC: u32 = 5;

pub struct BeaconService {
    storage: BeaconStorage,
    cache: Arc<CacheManager>,
}

impl BeaconService {
    pub fn new(pool: Arc<sqlx::Pool<sqlx::Postgres>>, cache: Arc<CacheManager>) -> Self {
        Self {
            storage: BeaconStorage::new(pool),
            cache,
        }
    }

    pub async fn create_beacon(
        &self,
        params: CreateBeaconInfoParams,
    ) -> Result<BeaconInfo, Box<dyn std::error::Error + Send + Sync>> {
        let existing = self
            .storage
            .get_beacon_info_by_state_key(&params.room_id, &params.state_key)
            .await?;

        // Same sender/state_key in a room should have a single active beacon lifecycle.
        self.storage
            .deactivate_beacons_by_state_key(&params.room_id, &params.state_key)
            .await?;
        for old in existing {
            let old_cache_key = format!("beacon:info:{}", old.event_id);
            let _ = self.cache.delete(&old_cache_key).await;
        }

        let beacon = self.storage.create_beacon_info(params).await?;

        let cache_key = format!("beacon:info:{}", beacon.event_id);
        let _ = self.cache.set(&cache_key, &beacon, 60).await;

        let room_cache_key = format!("beacon:room:{}", beacon.room_id);
        let _ = self.cache.delete(&room_cache_key).await;

        Ok(beacon)
    }

    pub async fn get_beacon_info(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> Result<Option<BeaconInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let cache_key = format!("beacon:info:{}", event_id);

        if let Some(cached) = self.cache.get::<BeaconInfo>(&cache_key).await? {
            return Ok(Some(cached));
        }

        let beacon = self.storage.get_beacon_info(room_id, event_id).await?;

        if let Some(ref b) = beacon {
            let _ = self.cache.set(&cache_key, b, 60).await;
        }

        Ok(beacon)
    }

    pub async fn get_active_beacons(
        &self,
        room_id: &str,
    ) -> Result<Vec<BeaconInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let cache_key = format!("beacon:room:active:{}", room_id);

        if let Some(cached) = self.cache.get::<Vec<BeaconInfo>>(&cache_key).await? {
            return Ok(cached);
        }

        let beacons = self.storage.get_active_beacons(room_id).await?;

        let _ = self.cache.set(&cache_key, &beacons, 30).await;

        Ok(beacons)
    }

    pub async fn update_beacon_liveness(
        &self,
        room_id: &str,
        event_id: &str,
        is_live: bool,
        timeout: Option<i64>,
    ) -> Result<Option<BeaconInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let beacon = self
            .storage
            .update_beacon_info(room_id, event_id, is_live, timeout)
            .await?;

        if let Some(ref b) = beacon {
            let cache_key = format!("beacon:info:{}", event_id);
            let _ = self.cache.set(&cache_key, b, 60).await;

            let room_cache_key = format!("beacon:room:active:{}", room_id);
            let _ = self.cache.delete(&room_cache_key).await;
        }

        Ok(beacon)
    }

    pub async fn delete_beacon(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let deleted = self.storage.delete_beacon_info(room_id, event_id).await?;

        if deleted {
            let cache_key = format!("beacon:info:{}", event_id);
            let _ = self.cache.delete(&cache_key).await;

            let room_cache_key = format!("beacon:room:active:{}", room_id);
            let _ = self.cache.delete(&room_cache_key).await;
        }

        Ok(deleted)
    }

    pub async fn report_location(
        &self,
        params: CreateBeaconLocationParams,
    ) -> Result<BeaconLocation, Box<dyn std::error::Error + Send + Sync>> {
        let location = self.storage.create_beacon_location(params.clone()).await?;

        let cache_key = format!("beacon:location:latest:{}", params.beacon_info_id);
        let _ = self.cache.set(&cache_key, &location, 30).await;

        let locations_cache_key = format!("beacon:locations:{}", params.beacon_info_id);
        let _ = self.cache.delete(&locations_cache_key).await;

        Ok(location)
    }

    pub async fn get_beacon_locations(
        &self,
        beacon_info_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<BeaconLocation>, Box<dyn std::error::Error + Send + Sync>> {
        let cache_key = format!(
            "beacon:locations:{}:{}",
            beacon_info_id,
            limit.unwrap_or(100)
        );

        if let Some(cached) = self.cache.get::<Vec<BeaconLocation>>(&cache_key).await? {
            return Ok(cached);
        }

        let locations = self
            .storage
            .get_beacon_locations(beacon_info_id, limit)
            .await?;

        let _ = self.cache.set(&cache_key, &locations, 30).await;

        Ok(locations)
    }

    pub async fn get_latest_location(
        &self,
        beacon_info_id: &str,
    ) -> Result<Option<BeaconLocation>, Box<dyn std::error::Error + Send + Sync>> {
        let cache_key = format!("beacon:location:latest:{}", beacon_info_id);

        if let Some(cached) = self.cache.get::<BeaconLocation>(&cache_key).await? {
            return Ok(Some(cached));
        }

        let location = self.storage.get_latest_location(beacon_info_id).await?;

        if let Some(ref l) = location {
            let _ = self.cache.set(&cache_key, l, 30).await;
        }

        Ok(location)
    }

    pub async fn get_beacon_with_locations(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> Result<Option<BeaconInfoWithLocations>, Box<dyn std::error::Error + Send + Sync>> {
        let beacon_with_locations = self
            .storage
            .get_beacon_with_locations(room_id, event_id)
            .await?;
        Ok(beacon_with_locations)
    }

    pub async fn get_room_beacons(
        &self,
        room_id: &str,
        include_expired: bool,
    ) -> Result<Vec<BeaconInfoWithLocations>, Box<dyn std::error::Error + Send + Sync>> {
        let beacons = self
            .storage
            .get_room_beacons(room_id, include_expired)
            .await?;
        Ok(beacons)
    }

    pub async fn cleanup_expired_beacons(
        &self,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let count = self.storage.cleanup_expired_beacons().await?;
        Ok(count)
    }

    pub async fn check_location_quota(
        &self,
        room_id: &str,
        sender: &str,
        now_ts: i64,
    ) -> Result<Option<u64>, Box<dyn std::error::Error + Send + Sync>> {
        let since_ts = now_ts.saturating_sub(BEACON_QUOTA_WINDOW_MS);
        let sender_count = self
            .storage
            .count_locations_in_room_by_sender_since(room_id, sender, since_ts)
            .await?;
        if sender_count >= BEACON_MAX_PER_SENDER_PER_ROOM_WINDOW {
            return Ok(Some(1000));
        }

        let room_count = self
            .storage
            .count_locations_in_room_since(room_id, since_ts)
            .await?;
        if room_count >= BEACON_MAX_PER_ROOM_WINDOW {
            return Ok(Some(1000));
        }

        Ok(None)
    }

    pub async fn check_room_backpressure(
        &self,
        room_id: &str,
        _now_ts: i64,
    ) -> Result<Option<u64>, Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("rl:beacon:room_backpressure:{}", room_id);
        let decision = self
            .cache
            .rate_limit_token_bucket_take(
                &key,
                BEACON_ROOM_BACKPRESSURE_REFILL_PER_SEC,
                BEACON_ROOM_BACKPRESSURE_BUCKET_CAPACITY,
            )
            .await?;

        if decision.allowed {
            return Ok(None);
        }

        Ok(Some((decision.retry_after_seconds.max(1)) * 1000))
    }

    pub fn parse_geo_uri(uri: &str) -> Option<(f64, f64, Option<f64>)> {
        if !uri.starts_with("geo:") {
            return None;
        }

        let coords_part = uri.strip_prefix("geo:")?;
        let parts: Vec<&str> = coords_part.split(';').collect();

        if parts.is_empty() {
            return None;
        }

        let coords: Vec<&str> = parts[0].split(',').collect();
        if coords.len() < 2 {
            return None;
        }

        let lat = coords[0].parse::<f64>().ok()?;
        let lon = coords[1].parse::<f64>().ok()?;

        let accuracy = parts.iter().find_map(|p| {
            if p.starts_with("u=") {
                p.strip_prefix("u=")?.parse::<f64>().ok()
            } else {
                None
            }
        });

        Some((lat, lon, accuracy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_geo_uri() {
        let uri = "geo:51.5008,0.1247;u=35";
        let result = BeaconService::parse_geo_uri(uri);

        assert!(result.is_some());
        let (lat, lon, accuracy) = result.unwrap();
        assert!((lat - 51.5008).abs() < 0.0001);
        assert!((lon - 0.1247).abs() < 0.0001);
        assert_eq!(accuracy, Some(35.0));
    }

    #[test]
    fn test_parse_geo_uri_without_accuracy() {
        let uri = "geo:51.5008,0.1247";
        let result = BeaconService::parse_geo_uri(uri);

        assert!(result.is_some());
        let (lat, lon, accuracy) = result.unwrap();
        assert!((lat - 51.5008).abs() < 0.0001);
        assert!((lon - 0.1247).abs() < 0.0001);
        assert!(accuracy.is_none());
    }

    #[test]
    fn test_parse_geo_uri_invalid() {
        assert!(BeaconService::parse_geo_uri("invalid").is_none());
        assert!(BeaconService::parse_geo_uri("geo:invalid").is_none());
    }
}
