#[cfg(feature = "beacons")]
pub use synapse_services::beacon_service::*;

#[cfg(test)]
#[cfg(feature = "beacons")]
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
