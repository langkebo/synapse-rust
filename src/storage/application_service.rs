pub use synapse_storage::application_service::*;

#[cfg(test)]
mod tests {
    use super::{ApplicationServiceStorage, Namespaces, NamespaceRule, UpdateApplicationServiceRequest};
    use sqlx::PgPool;
    use std::sync::Arc;

    #[test]
    fn root_application_service_storage_reexport_keeps_constructor_shape() {
        let _ctor: fn(&Arc<PgPool>) -> ApplicationServiceStorage = ApplicationServiceStorage::new;
    }

    #[test]
    fn root_application_service_update_builder_remains_accessible() {
        let request = UpdateApplicationServiceRequest::new()
            .url("http://new-url:8080")
            .description("Updated bridge service")
            .is_rate_limited(false)
            .is_enabled(true)
            .api_key("new-api-key")
            .config(serde_json::json!({
                "rate_limit": 100,
                "timeout": 30
            }));

        assert_eq!(request.url, Some("http://new-url:8080".to_string()));
        assert_eq!(request.description, Some("Updated bridge service".to_string()));
        assert_eq!(request.is_rate_limited, Some(false));
        assert_eq!(request.is_enabled, Some(true));
        assert_eq!(request.api_key, Some("new-api-key".to_string()));
        assert!(request.config.is_some());
    }

    #[test]
    fn root_application_service_namespace_types_remain_accessible() {
        let namespaces = Namespaces {
            users: vec![NamespaceRule {
                is_exclusive: true,
                regex: "@irc_.*:example.com".to_string(),
                group_id: None,
            }],
            aliases: vec![NamespaceRule {
                is_exclusive: false,
                regex: "#irc_.*:example.com".to_string(),
                group_id: None,
            }],
            rooms: vec![],
        };

        assert_eq!(namespaces.users.len(), 1);
        assert_eq!(namespaces.aliases.len(), 1);
        assert_eq!(namespaces.rooms.len(), 0);
    }
}
