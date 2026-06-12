#[cfg(feature = "burn-after-read")]
pub use synapse_services::burn_after_read_service::*;

#[cfg(test)]
#[cfg(feature = "burn-after-read")]
mod tests {
    use super::*;

    #[test]
    fn test_burn_settings_struct() {
        let settings = BurnSettings { is_enabled: true, burn_after_ms: 60_000 };
        assert!(settings.is_enabled);
        assert_eq!(settings.burn_after_ms, 60_000);
    }

    #[test]
    fn test_burn_event_struct() {
        let event = BurnEvent {
            id: 1,
            event_id: "$event1".to_string(),
            room_id: "!room:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            created_ts: 1234567890,
            delete_ts: 1234567950,
        };
        assert_eq!(event.id, 1);
        assert_eq!(event.event_id, "$event1");
    }

    #[test]
    fn test_burn_stats_default() {
        let stats = BurnStats::default();
        assert_eq!(stats.total_burned, 0);
        assert_eq!(stats.total_pending, 0);
        assert_eq!(stats.rooms_enabled, 0);
    }

    #[test]
    fn test_burn_stats_custom() {
        let stats = BurnStats { total_burned: 10, total_pending: 3, rooms_enabled: 2 };
        assert_eq!(stats.total_burned, 10);
        assert_eq!(stats.total_pending, 3);
        assert_eq!(stats.rooms_enabled, 2);
    }
}
