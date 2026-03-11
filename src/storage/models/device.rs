use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
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

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct DehydratedDevice {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub device_data: Option<serde_json::Value>,
    pub time_of_dehydration: i64,
    pub is_restored: bool,
    pub restored_by_device_id: Option<String>,
    pub created_ts: i64,
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
            user_agent: Some("Mozilla/5.0".to_string()),
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
    fn test_dehydrated_device() {
        let dehydrated = DehydratedDevice {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEHYDRATED123".to_string(),
            device_data: Some(serde_json::json!({"key": "encrypted_data"})),
            time_of_dehydration: 1234567890000,
            is_restored: false,
            restored_by_device_id: None,
            created_ts: 1234567890000,
        };

        assert_eq!(dehydrated.device_id, "DEHYDRATED123");
        assert!(!dehydrated.is_restored);
    }
}
