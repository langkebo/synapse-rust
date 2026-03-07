use crate::cache::CacheManager;
use crate::storage::beacon::{
    BeaconInfo, BeaconInfoWithLocations, BeaconLocation, BeaconStorage,
    CreateBeaconInfoParams, CreateBeaconLocationParams,
};
use std::sync::Arc;

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
        let beacon = self.storage.update_beacon_info(room_id, event_id, is_live, timeout).await?;

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
        let cache_key = format!("beacon:locations:{}:{}", beacon_info_id, limit.unwrap_or(100));

        if let Some(cached) = self.cache.get::<Vec<BeaconLocation>>(&cache_key).await? {
            return Ok(cached);
        }

        let locations = self.storage.get_beacon_locations(beacon_info_id, limit).await?;

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
        let beacon_with_locations = self.storage.get_beacon_with_locations(room_id, event_id).await?;
        Ok(beacon_with_locations)
    }

    pub async fn get_room_beacons(
        &self,
        room_id: &str,
        include_expired: bool,
    ) -> Result<Vec<BeaconInfoWithLocations>, Box<dyn std::error::Error + Send + Sync>> {
        let beacons = self.storage.get_room_beacons(room_id, include_expired).await?;
        Ok(beacons)
    }

    pub async fn cleanup_expired_beacons(&self) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let count = self.storage.cleanup_expired_beacons().await?;
        Ok(count)
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

        let accuracy = parts.iter()
            .find_map(|p| {
                if p.starts_with("u=") {
                    p.strip_prefix("u=")?.parse::<f64>().ok()
                } else {
                    None
                }
            });

        Some((lat, lon, accuracy))
    }

    pub fn format_geo_uri(lat: f64, lon: f64, accuracy: Option<f64>) -> String {
        match accuracy {
            Some(acc) => format!("geo:{},{},u={}", lat, lon, acc),
            None => format!("geo:{},{}", lat, lon),
        }
    }

    pub fn calculate_distance(
        lat1: f64,
        lon1: f64,
        lat2: f64,
        lon2: f64,
    ) -> f64 {
        let lat1_rad = lat1.to_radians();
        let lat2_rad = lat2.to_radians();
        let delta_lat = (lat2 - lat1).to_radians();
        let delta_lon = (lon2 - lon1).to_radians();

        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

        6371000.0 * c
    }

    pub async fn get_nearby_beacons(
        &self,
        room_id: &str,
        lat: f64,
        lon: f64,
        radius_meters: f64,
    ) -> Result<Vec<(BeaconInfo, BeaconLocation, f64)>, Box<dyn std::error::Error + Send + Sync>> {
        let beacons = self.storage.get_active_beacons(room_id).await?;
        let mut result = Vec::new();

        for beacon in beacons {
            if let Some(location) = self.storage.get_latest_location(&beacon.event_id).await? {
                if let Some((b_lat, b_lon, _)) = Self::parse_geo_uri(&location.uri) {
                    let distance = Self::calculate_distance(lat, lon, b_lat, b_lon);
                    if distance <= radius_meters {
                        result.push((beacon, location, distance));
                    }
                }
            }
        }

        result.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        Ok(result)
    }

    pub async fn get_location_history(
        &self,
        beacon_info_id: &str,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<BeaconLocation>, Box<dyn std::error::Error + Send + Sync>> {
        let all_locations = self.storage.get_beacon_locations(beacon_info_id, None).await?;

        let filtered: Vec<BeaconLocation> = all_locations
            .into_iter()
            .filter(|l| l.timestamp >= start_ts && l.timestamp <= end_ts)
            .collect();

        Ok(filtered)
    }

    pub async fn get_location_statistics(
        &self,
        beacon_info_id: &str,
    ) -> Result<LocationStatistics, Box<dyn std::error::Error + Send + Sync>> {
        let locations = self.storage.get_beacon_locations(beacon_info_id, None).await?;

        if locations.is_empty() {
            return Ok(LocationStatistics {
                total_locations: 0,
                first_ts: None,
                last_ts: None,
                avg_accuracy: None,
                total_distance: 0.0,
            });
        }

        let first_ts = locations.last().map(|l| l.timestamp);
        let last_ts = locations.first().map(|l| l.timestamp);

        let avg_accuracy = {
            let accuracies: Vec<f64> = locations
                .iter()
                .filter_map(|l| l.accuracy.map(|a| a as f64))
                .collect();
            if !accuracies.is_empty() {
                Some(accuracies.iter().sum::<f64>() / accuracies.len() as f64)
            } else {
                None
            }
        };

        let mut total_distance = 0.0;
        for window in locations.windows(2) {
            if let (Some((lat1, lon1, _)), Some((lat2, lon2, _))) = (
                Self::parse_geo_uri(&window[1].uri),
                Self::parse_geo_uri(&window[0].uri),
            ) {
                total_distance += Self::calculate_distance(lat1, lon1, lat2, lon2);
            }
        }

        Ok(LocationStatistics {
            total_locations: locations.len(),
            first_ts,
            last_ts,
            avg_accuracy,
            total_distance,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LocationStatistics {
    pub total_locations: usize,
    pub first_ts: Option<i64>,
    pub last_ts: Option<i64>,
    pub avg_accuracy: Option<f64>,
    pub total_distance: f64,
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

    #[test]
    fn test_format_geo_uri() {
        let uri = BeaconService::format_geo_uri(51.5008, 0.1247, Some(35.0));
        assert!(uri.starts_with("geo:"));
        assert!(uri.contains("51.5008"));
        assert!(uri.contains("0.1247"));
        assert!(uri.contains("u=35"));
    }

    #[test]
    fn test_format_geo_uri_without_accuracy() {
        let uri = BeaconService::format_geo_uri(51.5008, 0.1247, None);
        assert!(uri.starts_with("geo:"));
        assert!(!uri.contains("u="));
    }

    #[test]
    fn test_calculate_distance() {
        let distance = BeaconService::calculate_distance(51.5008, 0.1247, 51.5010, 0.1250);

        assert!(distance > 0.0);
        assert!(distance < 100.0);
    }

    #[test]
    fn test_calculate_distance_same_point() {
        let distance = BeaconService::calculate_distance(51.5008, 0.1247, 51.5008, 0.1247);
        assert!((distance - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_location_statistics() {
        let stats = LocationStatistics {
            total_locations: 10,
            first_ts: Some(1234567890000),
            last_ts: Some(1234567900000),
            avg_accuracy: Some(35.5),
            total_distance: 1500.0,
        };

        assert_eq!(stats.total_locations, 10);
        assert!(stats.avg_accuracy.is_some());
    }
}
