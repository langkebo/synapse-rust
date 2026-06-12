#[cfg(feature = "widgets")]
pub use synapse_services::widget_service::*;

#[cfg(test)]
#[cfg(feature = "widgets")]
mod tests {
    use super::*;

    #[test]
    fn test_create_widget_request() {
        let request = CreateWidgetRequest {
            room_id: Some("!room:example.com".to_string()),
            widget_type: "customwidget".to_string(),
            url: "https://example.com/widget".to_string(),
            name: "My Widget".to_string(),
            data: Some(serde_json::json!({"key": "value"})),
        };

        assert_eq!(request.widget_type, "customwidget");
        assert!(request.room_id.is_some());
    }

    #[test]
    fn test_update_widget_request() {
        let request =
            UpdateWidgetRequest { url: Some("https://example.com/new-widget".to_string()), name: None, data: None };

        assert!(request.url.is_some());
        assert!(request.name.is_none());
    }

    #[test]
    fn test_set_permission_request() {
        let request = SetPermissionRequest {
            user_id: "@user:example.com".to_string(),
            permissions: vec!["read".to_string(), "write".to_string()],
        };

        assert_eq!(request.permissions.len(), 2);
    }

    #[test]
    fn test_create_session_request() {
        let request = CreateSessionRequest {
            widget_id: "widget_123".to_string(),
            device_id: Some("DEVICE123".to_string()),
            expires_in_ms: Some(3_600_000),
        };

        assert_eq!(request.widget_id, "widget_123");
        assert!(request.expires_in_ms.is_some());
    }

    #[test]
    fn test_widget_response() {
        let widget = Widget {
            id: 1,
            widget_id: "widget_123".to_string(),
            room_id: Some("!room:example.com".to_string()),
            user_id: "@user:example.com".to_string(),
            widget_type: "customwidget".to_string(),
            url: "https://example.com/widget".to_string(),
            name: "My Widget".to_string(),
            data: serde_json::json!({}),
            created_ts: 1234567890000,
            updated_ts: None,
            is_active: true,
        };

        let response = WidgetResponse { widget };
        assert_eq!(response.widget.widget_id, "widget_123");
    }
}
