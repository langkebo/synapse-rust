pub use synapse_storage::server_notification::*;

#[cfg(test)]
mod cursor_tests {
    use super::{decode_server_notification_cursor, encode_server_notification_cursor, ServerNotificationCursor};

    #[test]
    fn server_notification_cursor_round_trip() {
        let cursor = ServerNotificationCursor { created_ts: 1_700_000_000_000, id: 7 };
        let encoded = encode_server_notification_cursor(&cursor);
        assert_eq!(decode_server_notification_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn server_notification_cursor_rejects_invalid_value() {
        assert_eq!(decode_server_notification_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_server_notification_cursor(Some("123|")), None);
    }
}

#[cfg(test)]
mod tests {
    use super::{CreateNotificationRequest, ServerNotification, ServerNotificationStorage};
    use sqlx::PgPool;
    use std::sync::Arc;

    #[test]
    fn root_server_notification_storage_reexport_keeps_constructor_shape() {
        let _ctor: fn(&Arc<PgPool>) -> ServerNotificationStorage = ServerNotificationStorage::new;
    }

    #[test]
    fn root_server_notification_request_types_remain_accessible() {
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
    fn root_server_notification_type_remains_accessible() {
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
            created_ts: 1_700_000_000_000,
            updated_ts: 1_700_000_000_000,
        };
        assert_eq!(notification.title, "Test");
        assert!(notification.is_enabled);
    }
}
