pub use synapse_services::application_service::*;

#[cfg(test)]
mod tests {
    use super::{ApplicationServiceManager, NamespacesInfo};
    use crate::storage::application_service::{
        ApplicationServiceNamespace, ApplicationServiceStorage, UpdateApplicationServiceRequest,
    };
    use crate::storage::EventStorage;
    use std::sync::Arc;

    #[test]
    fn root_application_service_manager_reexport_keeps_constructor_shape() {
        let _ctor: fn(Arc<ApplicationServiceStorage>, Arc<EventStorage>, String) -> ApplicationServiceManager =
            ApplicationServiceManager::new;
    }

    #[test]
    fn root_application_service_service_reexports_keep_request_types() {
        let request = UpdateApplicationServiceRequest::new()
            .url("http://new-url:8080")
            .description("New Description")
            .is_rate_limited(true)
            .is_enabled(true);

        assert_eq!(request.url, Some("http://new-url:8080".to_string()));
        assert_eq!(request.description, Some("New Description".to_string()));
        assert_eq!(request.is_rate_limited, Some(true));
        assert_eq!(request.is_enabled, Some(true));
    }

    #[test]
    fn root_application_service_namespaces_info_remains_accessible() {
        let namespace = ApplicationServiceNamespace {
            id: 1,
            as_id: "test-as".to_string(),
            namespace_pattern: "@_.*:example.com".to_string(),
            is_exclusive: true,
            regex: "@_.*:example.com".to_string(),
            created_ts: 1_234_567_890,
        };
        let info = NamespacesInfo { users: vec![namespace.clone()], aliases: vec![], rooms: vec![namespace] };

        assert_eq!(info.users.len(), 1);
        assert_eq!(info.rooms.len(), 1);
        assert!(info.aliases.is_empty());
    }
}
