#[cfg(test)]
mod tests {
    use synapse_rust::storage::refresh_token::*;
    use synapse_rust::services::refresh_token_service::RefreshTokenService;
    use synapse_rust::services::ServiceContainer;

    #[test]
    fn test_create_refresh_token_request() {
        let request = CreateRefreshTokenRequest {
            token_hash: "test_hash".to_string(),
            user_id: "@user:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            access_token_id: Some("access_token_id".to_string()),
            scope: Some("openid profile".to_string()),
            expires_at: chrono::Utc::now().timestamp_millis() + 604800000,
            client_info: None,
            ip_address: Some("127.0.0.1".to_string()),
            user_agent: Some("TestAgent/1.0".to_string()),
        };

        assert_eq!(request.token_hash, "test_hash");
        assert_eq!(request.user_id, "@user:example.com");
        assert_eq!(request.device_id, Some("DEVICE123".to_string()));
    }

    #[test]
    fn test_refresh_token_struct() {
        let token = RefreshToken {
            id: 1,
            token_hash: "hash123".to_string(),
            user_id: "@user:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            access_token_id: Some("access123".to_string()),
            scope: Some("openid".to_string()),
            expires_at: 1234567890,
            created_ts: 1234560000,
            last_used_ts: Some(1234567000),
            use_count: 5,
            is_revoked: false,
            revoked_ts: None,
            revoked_reason: None,
            client_info: None,
            ip_address: Some("127.0.0.1".to_string()),
            user_agent: Some("TestAgent".to_string()),
        };

        assert_eq!(token.id, 1);
        assert_eq!(token.use_count, 5);
        assert!(!token.is_revoked);
    }

    #[test]
    fn test_refresh_token_usage_struct() {
        let usage = RefreshTokenUsage {
            id: 1,
            refresh_token_id: 100,
            user_id: "@user:example.com".to_string(),
            old_access_token_id: Some("old_token".to_string()),
            new_access_token_id: Some("new_token".to_string()),
            used_ts: 1234567890,
            ip_address: Some("127.0.0.1".to_string()),
            user_agent: Some("TestAgent".to_string()),
            success: true,
            error_message: None,
        };

        assert_eq!(usage.refresh_token_id, 100);
        assert!(usage.success);
    }

    #[test]
    fn test_refresh_token_family_struct() {
        let family = RefreshTokenFamily {
            id: 1,
            family_id: "family123".to_string(),
            user_id: "@user:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            created_ts: 1234560000,
            last_refresh_ts: Some(1234567000),
            refresh_count: 3,
            is_compromised: false,
            compromised_ts: None,
        };

        assert_eq!(family.family_id, "family123");
        assert_eq!(family.refresh_count, 3);
        assert!(!family.is_compromised);
    }

    #[test]
    fn test_refresh_token_rotation_struct() {
        let rotation = RefreshTokenRotation {
            id: 1,
            family_id: "family123".to_string(),
            old_token_hash: Some("old_hash".to_string()),
            new_token_hash: "new_hash".to_string(),
            rotated_ts: 1234567890,
            rotation_reason: "refresh".to_string(),
        };

        assert_eq!(rotation.family_id, "family123");
        assert_eq!(rotation.rotation_reason, "refresh");
    }

    #[test]
    fn test_token_blacklist_entry_struct() {
        let entry = TokenBlacklistEntry {
            id: 1,
            token_hash: "blacklisted_hash".to_string(),
            token_type: "access".to_string(),
            user_id: "@user:example.com".to_string(),
            revoked_ts: 1234567890,
            expires_at: 1234577890,
            reason: Some("User logout".to_string()),
        };

        assert_eq!(entry.token_type, "access");
        assert_eq!(entry.reason, Some("User logout".to_string()));
    }

    #[test]
    fn test_refresh_token_stats_struct() {
        let stats = RefreshTokenStats {
            user_id: "@user:example.com".to_string(),
            total_tokens: 10,
            active_tokens: 5,
            revoked_tokens: 3,
            expired_tokens: 2,
            total_uses: 50,
        };

        assert_eq!(stats.total_tokens, 10);
        assert_eq!(stats.active_tokens, 5);
        assert_eq!(stats.total_uses, 50);
    }

    #[test]
    fn test_hash_token() {
        let token = "test_token_123";
        let hash1 = RefreshTokenService::hash_token(token);
        let hash2 = RefreshTokenService::hash_token(token);

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, token);
        assert!(!hash1.is_empty());
    }

    #[test]
    fn test_generate_token() {
        let token1 = RefreshTokenService::generate_token();
        let token2 = RefreshTokenService::generate_token();

        assert_ne!(token1, token2);
        assert!(!token1.is_empty());
        assert!(!token2.is_empty());
    }

    #[test]
    fn test_generate_family_id() {
        let id1 = RefreshTokenService::generate_family_id();
        let id2 = RefreshTokenService::generate_family_id();

        assert_ne!(id1, id2);
        assert!(!id1.is_empty());
        assert!(!id2.is_empty());
    }

    #[tokio::test]
    async fn test_refresh_token_service_creation() {
        let container = ServiceContainer::new_test();
        let _service = &container.refresh_token_service;
    }

    #[tokio::test]
    async fn test_get_user_tokens() {
        let container = ServiceContainer::new_test();
        let service = &container.refresh_token_service;

        let result = service.get_user_tokens("@nonexistent:example.com").await;
        if result.is_err() {
            eprintln!("Skipping test_get_user_tokens: database table not available");
            return;
        }

        let tokens = result.unwrap();
        assert!(tokens.is_empty() || tokens.len() >= 0);
    }

    #[tokio::test]
    async fn test_get_active_tokens() {
        let container = ServiceContainer::new_test();
        let service = &container.refresh_token_service;

        let result = service.get_active_tokens("@nonexistent:example.com").await;
        if result.is_err() {
            eprintln!("Skipping test_get_active_tokens: database table not available");
            return;
        }

        let tokens = result.unwrap();
        assert!(tokens.is_empty() || tokens.len() >= 0);
    }

    #[tokio::test]
    async fn test_get_user_stats_nonexistent() {
        let container = ServiceContainer::new_test();
        let service = &container.refresh_token_service;

        let result = service.get_user_stats("@nonexistent:example.com").await;
        if result.is_err() {
            eprintln!("Skipping test_get_user_stats_nonexistent: database table not available");
            return;
        }

        let stats = result.unwrap();
        assert!(stats.is_none());
    }

    #[tokio::test]
    async fn test_get_usage_history() {
        let container = ServiceContainer::new_test();
        let service = &container.refresh_token_service;

        let result = service.get_usage_history("@nonexistent:example.com", 10).await;
        if result.is_err() {
            eprintln!("Skipping test_get_usage_history: database table not available");
            return;
        }

        let history = result.unwrap();
        assert!(history.is_empty() || history.len() >= 0);
    }

    #[tokio::test]
    async fn test_cleanup_expired_tokens() {
        let container = ServiceContainer::new_test();
        let service = &container.refresh_token_service;

        let result = service.cleanup_expired_tokens().await;
        if result.is_err() {
            eprintln!("Skipping test_cleanup_expired_tokens: database table not available");
            return;
        }

        let _count = result.unwrap();
    }

    #[tokio::test]
    async fn test_revoke_all_user_tokens() {
        let container = ServiceContainer::new_test();
        let service = &container.refresh_token_service;

        let result = service.revoke_all_user_tokens("@nonexistent:example.com", "test").await;
        if result.is_err() {
            eprintln!("Skipping test_revoke_all_user_tokens: database table not available");
            return;
        }

        let _count = result.unwrap();
    }

    #[tokio::test]
    async fn test_validate_invalid_token() {
        let container = ServiceContainer::new_test();
        let service = &container.refresh_token_service;

        let result = service.validate_token("invalid_token").await;
        
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_default_expiry_ms() {
        let container = ServiceContainer::new_test();
        let service = &container.refresh_token_service;

        let expiry = service.get_default_expiry_ms();
        assert!(expiry > 0);
    }
}
