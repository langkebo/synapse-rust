#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_rule_serialization() {
        let rule = NamespaceRule {
            is_exclusive: true,
            regex: "@_.*:example.com".to_string(),
            group_id: Some("group:example.com".to_string()),
        };

        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: NamespaceRule = serde_json::from_str(&json).unwrap();

        assert_eq!(rule.is_exclusive, deserialized.is_exclusive);
        assert_eq!(rule.regex, deserialized.regex);
        assert_eq!(rule.group_id, deserialized.group_id);
    }

    #[test]
    fn test_namespaces_serialization() {
        let namespaces = Namespaces {
            users: vec![NamespaceRule { is_exclusive: true, regex: "@_.*:example.com".to_string(), group_id: None }],
            aliases: vec![NamespaceRule { is_exclusive: false, regex: "#_.*:example.com".to_string(), group_id: None }],
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
        let request = RegisterApplicationServiceRequest {
            as_id: "irc-bridge".to_string(),
            url: "http://localhost:9999".to_string(),
            as_token: "secret_token".to_string(),
            hs_token: "hs_secret".to_string(),
            sender: "@irc-bot:example.com".to_string(),
            description: Some("IRC to Matrix bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: Some(vec!["irc".to_string()]),
            namespaces: Some(serde_json::json!({
                "users": [{"exclusive": true, "regex": "@_irc_.*:example.com"}],
                "aliases": [{"exclusive": true, "regex": "#_irc_.*:example.com"}],
                "rooms": []
            })),
            api_key: None,
            config: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: RegisterApplicationServiceRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request.as_id, deserialized.as_id);
        assert_eq!(request.url, deserialized.url);
        assert_eq!(request.as_token, deserialized.as_token);
        assert_eq!(request.sender, deserialized.sender);
        assert_eq!(request.protocols.unwrap().len(), 1);
    }

    #[test]
    fn test_update_request_builder_chains_all_fields() {
        let req = UpdateApplicationServiceRequest::new()
            .url("https://new-url.example.com")
            .description("Updated bridge")
            .is_rate_limited(true)
            .protocols(vec!["irc".to_string(), "matrix".to_string()])
            .is_enabled(false)
            .api_key("new_api_key")
            .config(serde_json::json!({"feature": "enabled"}));

        assert_eq!(req.url.as_deref(), Some("https://new-url.example.com"));
        assert_eq!(req.description.as_deref(), Some("Updated bridge"));
        assert_eq!(req.is_rate_limited, Some(true));
        assert_eq!(req.protocols.as_ref().map(|p| p.len()), Some(2));
        assert_eq!(req.is_enabled, Some(false));
        assert_eq!(req.api_key.as_deref(), Some("new_api_key"));
        assert!(req.config.is_some());
    }

    #[test]
    fn test_update_request_builder_optional_fields_none_by_default() {
        let req = UpdateApplicationServiceRequest::new();
        assert!(req.url.is_none());
        assert!(req.description.is_none());
        assert!(req.is_rate_limited.is_none());
        assert!(req.protocols.is_none());
        assert!(req.is_enabled.is_none());
        assert!(req.api_key.is_none());
        assert!(req.config.is_none());
    }

    #[test]
    fn test_update_request_builder_partial_chain() {
        let req = UpdateApplicationServiceRequest::new().url("https://partial.example.com").is_enabled(true);

        assert_eq!(req.url.as_deref(), Some("https://partial.example.com"));
        assert_eq!(req.is_enabled, Some(true));
        // Other fields should still be None
        assert!(req.description.is_none());
        assert!(req.is_rate_limited.is_none());
        assert!(req.protocols.is_none());
        assert!(req.api_key.is_none());
        assert!(req.config.is_none());
    }

    #[test]
    fn test_update_request_serde_roundtrip() {
        let req = UpdateApplicationServiceRequest {
            url: Some("https://test.example.com".to_string()),
            description: Some("A test update".to_string()),
            is_rate_limited: Some(true),
            protocols: Some(vec!["test".to_string()]),
            is_enabled: Some(true),
            api_key: Some("key123".to_string()),
            config: Some(serde_json::json!({"k": "v"})),
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: UpdateApplicationServiceRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.url, req.url);
        assert_eq!(deserialized.description, req.description);
        assert_eq!(deserialized.is_rate_limited, req.is_rate_limited);
        assert_eq!(deserialized.protocols, req.protocols);
        assert_eq!(deserialized.is_enabled, req.is_enabled);
        assert_eq!(deserialized.api_key, req.api_key);
    }

    #[test]
    fn test_namespace_rule_without_group_id() {
        let rule = NamespaceRule { is_exclusive: false, regex: "@test:example.com".to_string(), group_id: None };
        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: NamespaceRule = serde_json::from_str(&json).unwrap();
        assert!(!deserialized.is_exclusive);
        assert_eq!(deserialized.group_id, None);
    }

    #[test]
    fn test_application_service_state_serde() {
        let state = ApplicationServiceState {
            as_id: "my_service".to_string(),
            state_key: "config".to_string(),
            state_value: "{\"key\":\"value\"}".to_string(),
            updated_ts: 1700000000000,
        };
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: ApplicationServiceState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.as_id, state.as_id);
        assert_eq!(deserialized.state_key, state.state_key);
        assert_eq!(deserialized.updated_ts, state.updated_ts);
    }

    #[test]
    fn test_application_service_namespace_serde() {
        let ns = ApplicationServiceNamespace {
            id: 1,
            as_id: "irc-bridge".to_string(),
            namespace_pattern: "@_irc_.*:example.com".to_string(),
            is_exclusive: true,
            regex: "@_irc_.*:example.com".to_string(),
            created_ts: 1700000000000,
        };
        let json = serde_json::to_string(&ns).unwrap();
        let deserialized: ApplicationServiceNamespace = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.as_id, ns.as_id);
        assert_eq!(deserialized.is_exclusive, ns.is_exclusive);
        assert_eq!(deserialized.namespace_pattern, ns.namespace_pattern);
    }

    #[test]
    fn test_application_service_user_serde() {
        let user = ApplicationServiceUser {
            as_id: "irc-bridge".to_string(),
            user_id: "@_irc_alice:example.com".to_string(),
            displayname: Some("Alice (IRC)".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            created_ts: 1700000000000,
        };
        let json = serde_json::to_string(&user).unwrap();
        let deserialized: ApplicationServiceUser = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.user_id, user.user_id);
        assert_eq!(deserialized.displayname, user.displayname);
        assert_eq!(deserialized.avatar_url, user.avatar_url);
    }
}
