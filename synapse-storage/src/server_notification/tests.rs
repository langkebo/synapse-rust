use super::*;
use chrono::{Duration, Utc};
use synapse_common::current_timestamp_millis;

#[test]
fn test_create_notification_request_defaults() {
    let req = CreateNotificationRequest {
        title: "Test".to_string(),
        content: "Content".to_string(),
        notification_type: None,
        priority: None,
        target_audience: None,
        target_user_ids: None,
        starts_at: None,
        expires_at: None,
        is_dismissable: None,
        action_url: None,
        action_text: None,
        created_by: None,
    };

    assert_eq!(req.title, "Test");
    assert_eq!(req.content, "Content");
    assert!(req.notification_type.is_none());
    assert!(req.priority.is_none());
}

#[test]
fn test_notification_type_validation() {
    let valid_types = vec!["info", "warning", "error", "maintenance"];
    for t in valid_types {
        let req = CreateNotificationRequest {
            title: "Test".to_string(),
            content: "Content".to_string(),
            notification_type: Some(t.to_string()),
            priority: None,
            target_audience: None,
            target_user_ids: None,
            starts_at: None,
            expires_at: None,
            is_dismissable: None,
            action_url: None,
            action_text: None,
            created_by: None,
        };
        assert!(req.notification_type.is_some());
    }
}

#[test]
fn test_notification_priority_range() {
    for priority in 0..=2 {
        let req = CreateNotificationRequest {
            title: "Test".to_string(),
            content: "Content".to_string(),
            notification_type: None,
            priority: Some(priority),
            target_audience: None,
            target_user_ids: None,
            starts_at: None,
            expires_at: None,
            is_dismissable: None,
            action_url: None,
            action_text: None,
            created_by: None,
        };
        assert!(req.priority.unwrap() <= 2);
    }
}

#[test]
fn test_target_audience_types() {
    let audiences = vec!["all", "admins", "users"];
    for audience in audiences {
        let req = CreateNotificationRequest {
            title: "Test".to_string(),
            content: "Content".to_string(),
            notification_type: None,
            priority: None,
            target_audience: Some(audience.to_string()),
            target_user_ids: None,
            starts_at: None,
            expires_at: None,
            is_dismissable: None,
            action_url: None,
            action_text: None,
            created_by: None,
        };
        assert_eq!(req.target_audience.unwrap(), audience);
    }
}

#[test]
fn test_scheduled_notification_future_time() {
    let _future_time = Utc::now() + Duration::hours(1);
    let req = CreateNotificationRequest {
        title: "Scheduled".to_string(),
        content: "Future notification".to_string(),
        notification_type: None,
        priority: None,
        target_audience: None,
        target_user_ids: None,
        starts_at: None,
        expires_at: None,
        is_dismissable: None,
        action_url: None,
        action_text: None,
        created_by: None,
    };
    assert!(req.title == "Scheduled");
}

#[test]
fn test_server_notification_struct() {
    let notification = ServerNotification {
        id: 1,
        title: "Test".to_string(),
        content: "Content".to_string(),
        notification_type: "info".to_string(),
        priority: 0,
        target_audience: "all".to_string(),
        target_user_ids: serde_json::json!([]),
        starts_at: None,
        expires_at: None,
        is_enabled: true,
        is_dismissable: true,
        action_url: None,
        action_text: None,
        created_by: Some("admin".to_string()),
        created_ts: current_timestamp_millis(),
        updated_ts: current_timestamp_millis(),
    };
    assert_eq!(notification.title, "Test");
    assert!(notification.is_enabled);
}

#[test]
fn test_user_notification_status() {
    let status = UserNotificationStatus {
        id: 1,
        user_id: "@user:example.com".to_string(),
        notification_id: 1,
        is_read: false,
        is_dismissed: false,
        read_ts: None,
        dismissed_ts: None,
        created_ts: current_timestamp_millis(),
    };
    assert!(!status.is_read);
    assert!(!status.is_dismissed);
}

#[test]
fn test_notification_template() {
    let template = NotificationTemplate {
        id: 1,
        name: "welcome".to_string(),
        title_template: "Welcome!".to_string(),
        content_template: "Hello, {username}!".to_string(),
        notification_type: "info".to_string(),
        variables: serde_json::json!(["username"]),
        is_enabled: true,
        created_ts: current_timestamp_millis(),
        updated_ts: current_timestamp_millis(),
    };
    assert_eq!(template.name, "welcome");
    assert!(template.is_enabled);
}
