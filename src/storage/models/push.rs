use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct PushDevice {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub push_kind: String,
    pub app_id: String,
    pub app_display_name: Option<String>,
    pub device_display_name: Option<String>,
    pub profile_tag: Option<String>,
    pub pushkey: String,
    pub lang: String,
    pub data: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct PushRule {
    pub id: i64,
    pub user_id: String,
    pub scope: String,
    pub rule_id: String,
    pub kind: String,
    pub priority_class: i32,
    pub priority: i32,
    pub conditions: Option<serde_json::Value>,
    pub actions: Option<serde_json::Value>,
    pub pattern: Option<String>,
    pub is_default: bool,
    pub is_enabled: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Pusher {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub pushkey: String,
    pub pushkey_ts: i64,
    pub kind: String,
    pub app_id: String,
    pub app_display_name: String,
    pub device_display_name: String,
    pub profile_tag: Option<String>,
    pub lang: String,
    pub data: Option<serde_json::Value>,
    pub last_updated_ts: Option<i64>,
    pub created_ts: i64,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct PushNotificationQueue {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub event_id: String,
    pub room_id: String,
    pub notification_type: String,
    pub content: Option<serde_json::Value>,
    pub is_processed: bool,
    pub processed_at: Option<i64>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct PushNotificationLog {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub pushkey: String,
    pub status: String,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub last_attempt_at: Option<i64>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct PushConfig {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub config_type: String,
    pub config_data: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Notification {
    pub id: i64,
    pub user_id: String,
    pub event_id: Option<String>,
    pub room_id: Option<String>,
    pub ts: i64,
    pub notification_type: String,
    pub profile_tag: Option<String>,
    pub is_read: bool,
    // 注意: read 字段已移除（与 is_read 冗余）
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_device() {
        let device = PushDevice {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            push_kind: "http".to_string(),
            app_id: "com.example.app".to_string(),
            app_display_name: Some("Example App".to_string()),
            device_display_name: Some("iPhone 15".to_string()),
            profile_tag: None,
            pushkey: "pushkey_abc123".to_string(),
            lang: "en".to_string(),
            data: Some(serde_json::json!({"url": "https://push.example.com"})),
            created_ts: 1234567890000,
            updated_ts: None,
            is_enabled: true,
        };

        assert_eq!(device.push_kind, "http");
        assert!(device.is_enabled);
    }

    #[test]
    fn test_push_rule() {
        let rule = PushRule {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            scope: "global".to_string(),
            rule_id: ".m.rule.contains_user_name".to_string(),
            kind: "override".to_string(),
            priority_class: 5,
            priority: 0,
            conditions: Some(serde_json::json!([{"kind": "contains_display_name"}])),
            actions: Some(serde_json::json!(["notify", {"set_tweak": "sound", "value": "default"}])),
            pattern: None,
            is_default: true,
            is_enabled: true,
            created_ts: 1234567890000,
            updated_ts: None,
        };

        assert_eq!(rule.kind, "override");
        assert!(rule.is_default);
    }

    #[test]
    fn test_pusher() {
        let pusher = Pusher {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            pushkey: "pushkey_abc123".to_string(),
            pushkey_ts: 1234567890000,
            kind: "http".to_string(),
            app_id: "com.example.app".to_string(),
            app_display_name: "Example App".to_string(),
            device_display_name: "iPhone 15".to_string(),
            profile_tag: None,
            lang: "en".to_string(),
            data: Some(serde_json::json!({"url": "https://push.example.com"})),
            last_updated_ts: Some(1234567900000),
            created_ts: 1234567890000,
            is_enabled: true,
        };

        assert_eq!(pusher.kind, "http");
        assert!(pusher.is_enabled);
    }

    #[test]
    fn test_notification() {
        let notification = Notification {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            event_id: Some("$event:example.com".to_string()),
            room_id: Some("!room:example.com".to_string()),
            ts: 1234567890000,
            notification_type: "message".to_string(),
            profile_tag: None,
            is_read: false,
            // 注意: read 字段已移除
            created_ts: 1234567890000,
            updated_ts: None,
        };

        assert_eq!(notification.notification_type, "message");
        assert!(!notification.is_read);
    }
}
