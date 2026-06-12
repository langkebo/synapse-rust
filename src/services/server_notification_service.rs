pub use synapse_services::server_notification_service::*;

#[cfg(test)]
mod tests {
    use super::ServerNotificationService;
    use crate::storage::server_notification::ServerNotificationStorage;
    use std::sync::Arc;

    #[test]
    fn root_server_notification_service_reexport_keeps_constructor_shape() {
        let _ctor: fn(Arc<ServerNotificationStorage>) -> ServerNotificationService = ServerNotificationService::new;
    }
}
