#[cfg(test)]
mod tests {
    use synapse_rust::storage::application_service::{
        RegisterApplicationServiceRequest, NamespaceRule, Namespaces,
    };
    use synapse_rust::services::ServiceContainer;
    use std::sync::Arc;

    fn create_test_request() -> RegisterApplicationServiceRequest {
        RegisterApplicationServiceRequest {
            as_id: "test-bridge".to_string(),
            url: "http://localhost:9999".to_string(),
            as_token: "test_as_token_123".to_string(),
            hs_token: "test_hs_token_456".to_string(),
            sender: "@bridge-bot:test.com".to_string(),
            name: Some("Test Bridge".to_string()),
            description: Some("A test application service".to_string()),
            rate_limited: Some(false),
            protocols: Some(vec!["irc".to_string(), "matrix".to_string()]),
            namespaces: Some(serde_json::json!({
                "users": [{
                    "exclusive": true,
                    "regex": "@_irc_.*:test.com"
                }],
                "aliases": [{
                    "exclusive": true,
                    "regex": "#_irc_.*:test.com"
                }],
                "rooms": []
            })),
        }
    }

    #[test]
    fn test_namespace_rule_serialization() {
        let rule = NamespaceRule {
            exclusive: true,
            regex: "@_.*:example.com".to_string(),
            group_id: Some("group:example.com".to_string()),
        };

        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: NamespaceRule = serde_json::from_str(&json).unwrap();
        
        assert_eq!(rule.exclusive, deserialized.exclusive);
        assert_eq!(rule.regex, deserialized.regex);
        assert_eq!(rule.group_id, deserialized.group_id);
    }

    #[test]
    fn test_namespaces_serialization() {
        let namespaces = Namespaces {
            users: vec![NamespaceRule {
                exclusive: true,
                regex: "@_.*:example.com".to_string(),
                group_id: None,
            }],
            aliases: vec![NamespaceRule {
                exclusive: false,
                regex: "#_.*:example.com".to_string(),
                group_id: None,
            }],
            rooms: vec![],
        };

        let json = serde_json::to_string(&namespaces).unwrap();
        let deserialized: Namespaces = serde_json::from_str(&json).unwrap();
        
        assert_eq!(namespaces.users.len(), deserialized.users.len());
        assert_eq!(namespaces.aliases.len(), deserialized.aliases.len());
        assert_eq!(namespaces.rooms.len(), deserialized.rooms.len());
    }

    #[test]
    fn test_register_application_service_request() {
        let request = create_test_request();

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: RegisterApplicationServiceRequest = serde_json::from_str(&json).unwrap();
        
        assert_eq!(request.as_id, deserialized.as_id);
        assert_eq!(request.url, deserialized.url);
        assert_eq!(request.as_token, deserialized.as_token);
        assert_eq!(request.sender, deserialized.sender);
        assert_eq!(request.protocols.unwrap().len(), 2);
    }

    #[test]
    fn test_namespace_json_parsing() {
        let namespaces_json = serde_json::json!({
            "users": [{
                "exclusive": true,
                "regex": "@_irc_.*:test.com"
            }],
            "aliases": [{
                "exclusive": true,
                "regex": "#_irc_.*:test.com"
            }],
            "rooms": []
        });

        let users = namespaces_json.get("users").and_then(|v| v.as_array()).unwrap();
        assert_eq!(users.len(), 1);
        
        let first_user = &users[0];
        assert_eq!(first_user.get("exclusive").and_then(|e| e.as_bool()), Some(true));
        assert_eq!(first_user.get("regex").and_then(|r| r.as_str()), Some("@_irc_.*:test.com"));
    }

    #[test]
    fn test_user_id_namespace_matching() {
        let regex = "@_irc_.*:test.com";
        let user_id = "@_irc_john:test.com";
        
        let re = regex::Regex::new(regex).unwrap();
        assert!(re.is_match(user_id));
        
        let non_matching_user = "@john:test.com";
        assert!(!re.is_match(non_matching_user));
    }

    #[test]
    fn test_room_alias_namespace_matching() {
        let regex = "#_irc_.*:test.com";
        let alias = "#_irc_general:test.com";
        
        let re = regex::Regex::new(regex).unwrap();
        assert!(re.is_match(alias));
        
        let non_matching_alias = "#general:test.com";
        assert!(!re.is_match(non_matching_alias));
    }

    #[test]
    fn test_application_service_manager_creation() {
        let container = ServiceContainer::new_test();
        let manager = &container.app_service_manager;
        
        assert!(Arc::strong_count(manager) >= 1);
    }

    #[tokio::test]
    async fn test_register_application_service() {
        let container = ServiceContainer::new_test();
        let manager = &container.app_service_manager;
        
        let request = RegisterApplicationServiceRequest {
            as_id: format!("test-bridge-{}", uuid::Uuid::new_v4()),
            url: "http://localhost:9999".to_string(),
            as_token: format!("token_{}", uuid::Uuid::new_v4()),
            hs_token: format!("hs_token_{}", uuid::Uuid::new_v4()),
            sender: "@bridge-bot:test.com".to_string(),
            name: Some("Test Bridge".to_string()),
            description: Some("A test application service".to_string()),
            rate_limited: Some(false),
            protocols: Some(vec!["irc".to_string()]),
            namespaces: Some(serde_json::json!({
                "users": [{"exclusive": true, "regex": "@_irc_.*:test.com"}],
                "aliases": [],
                "rooms": []
            })),
        };

        let result = manager.register(request).await;
        assert!(result.is_ok());
        
        let service = result.unwrap();
        assert!(!service.as_id.is_empty());
        assert!(service.is_enabled);
    }

    #[tokio::test]
    async fn test_get_nonexistent_service() {
        let container = ServiceContainer::new_test();
        let manager = &container.app_service_manager;
        
        let result = manager.get("nonexistent-service").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_by_token_nonexistent() {
        let container = ServiceContainer::new_test();
        let manager = &container.app_service_manager;
        
        let result = manager.get_by_token("nonexistent_token").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_query_user_namespace() {
        let container = ServiceContainer::new_test();
        let manager = &container.app_service_manager;
        
        let unique_id = uuid::Uuid::new_v4();
        let request = RegisterApplicationServiceRequest {
            as_id: format!("irc-bridge-{}", unique_id),
            url: "http://localhost:9999".to_string(),
            as_token: format!("as_token_{}", unique_id),
            hs_token: format!("hs_token_{}", unique_id),
            sender: "@irc-bot:test.com".to_string(),
            name: Some("IRC Bridge".to_string()),
            description: None,
            rate_limited: None,
            protocols: None,
            namespaces: Some(serde_json::json!({
                "users": [{"exclusive": true, "regex": format!("@_irc{}_.*:test.com", unique_id)}],
                "aliases": [],
                "rooms": []
            })),
        };

        let _service = manager.register(request).await.unwrap();
        
        let user_id = format!("@_irc{}_john:test.com", unique_id);
        let result = manager.query_user(&user_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_set_and_get_state() {
        let container = ServiceContainer::new_test();
        let manager = &container.app_service_manager;
        
        let unique_id = uuid::Uuid::new_v4();
        let request = RegisterApplicationServiceRequest {
            as_id: format!("state-test-{}", unique_id),
            url: "http://localhost:9999".to_string(),
            as_token: format!("token_{}", unique_id),
            hs_token: format!("hs_{}", unique_id),
            sender: "@bot:test.com".to_string(),
            name: None,
            description: None,
            rate_limited: None,
            protocols: None,
            namespaces: None,
        };

        let service = manager.register(request).await.unwrap();
        
        let state = manager.set_state(&service.as_id, "last_sync", "2024-01-01T00:00:00Z").await;
        assert!(state.is_ok());
        
        let retrieved = manager.get_state(&service.as_id, "last_sync").await;
        assert!(retrieved.is_ok());
        assert!(retrieved.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_get_all_states() {
        let container = ServiceContainer::new_test();
        let manager = &container.app_service_manager;
        
        let unique_id = uuid::Uuid::new_v4();
        let request = RegisterApplicationServiceRequest {
            as_id: format!("states-test-{}", unique_id),
            url: "http://localhost:9999".to_string(),
            as_token: format!("token_{}", unique_id),
            hs_token: format!("hs_{}", unique_id),
            sender: "@bot:test.com".to_string(),
            name: None,
            description: None,
            rate_limited: None,
            protocols: None,
            namespaces: None,
        };

        let service = manager.register(request).await.unwrap();
        
        let _ = manager.set_state(&service.as_id, "key1", "value1").await;
        let _ = manager.set_state(&service.as_id, "key2", "value2").await;
        
        let states = manager.get_all_states(&service.as_id).await;
        assert!(states.is_ok());
        let states = states.unwrap();
        assert_eq!(states.len(), 2);
    }

    #[tokio::test]
    async fn test_get_statistics() {
        let container = ServiceContainer::new_test();
        let manager = &container.app_service_manager;
        
        let stats = manager.get_statistics().await;
        assert!(stats.is_ok());
    }

    #[tokio::test]
    async fn test_unregister_service() {
        let container = ServiceContainer::new_test();
        let manager = &container.app_service_manager;
        
        let unique_id = uuid::Uuid::new_v4();
        let request = RegisterApplicationServiceRequest {
            as_id: format!("unregister-test-{}", unique_id),
            url: "http://localhost:9999".to_string(),
            as_token: format!("token_{}", unique_id),
            hs_token: format!("hs_{}", unique_id),
            sender: "@bot:test.com".to_string(),
            name: None,
            description: None,
            rate_limited: None,
            protocols: None,
            namespaces: None,
        };

        let service = manager.register(request).await.unwrap();
        
        let unregister_result = manager.unregister(&service.as_id).await;
        assert!(unregister_result.is_ok());
        
        let get_result = manager.get(&service.as_id).await;
        assert!(get_result.is_ok());
        assert!(get_result.unwrap().is_none());
    }
}
