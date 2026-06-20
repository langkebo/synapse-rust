pub use synapse_storage::server_notification::{
    decode_server_notification_cursor, encode_server_notification_cursor, CreateNotificationRequest,
    CreateTemplateRequest, NotificationDeliveryLog, NotificationTemplate, NotificationWithStatus,
    ScheduledNotification, ServerNotification, ServerNotificationCursor, ServerNotificationStorage,
    UserNotificationStatus,
};

// NOTE: Tests moved to synapse-storage crate.
