#[cfg(test)]
mod cache_integration_tests {
    use synapse_rust::auth::Claims;
    use synapse_rust::cache::{CacheConfig, CacheManager};
    use tokio::runtime::Runtime;

    fn create_test_claims() -> Claims {
        Claims {
            sub: "test_subject".to_string(),
            user_id: "@test:example.com".to_string(),
            admin: false,
            device_id: Some("DEVICE123".to_string()),
            exp: 1234567890,
            iat: 1234567890,
        }
    }

    #[test]
    fn test_cache_set_and_get_token() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            let claims = create_test_claims();
            manager.set_token("test_token", &claims, 3600).await;

            let result = manager.get_token("test_token").await;
            assert!(result.is_some());
            assert_eq!(result.unwrap().user_id, "@test:example.com");
        });
    }

    #[test]
    fn test_cache_delete_token() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            let claims = create_test_claims();
            manager.set_token("test_token", &claims, 3600).await;
            assert!(manager.get_token("test_token").await.is_some());

            manager.delete_token("test_token").await;
            assert!(manager.get_token("test_token").await.is_none());
        });
    }

    #[test]
    fn test_cache_set_and_get_generic() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            let test_value = "test_value".to_string();
            manager.set("test_key", &test_value, 60).await;

            let result: Option<String> = manager.get("test_key").await;
            assert_eq!(result, Some(test_value));
        });
    }

    #[test]
    fn test_cache_get_nonexistent() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            let result: Option<String> = manager.get("nonexistent").await;
            assert!(result.is_none());
        });
    }

    #[test]
    fn test_cache_delete() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            let test_value = "test_value".to_string();
            manager.set("test_key", &test_value, 60).await;
            assert!(manager.get::<String>("test_key").await.is_some());

            manager.delete("test_key").await;
            assert!(manager.get::<String>("test_key").await.is_none());
        });
    }
}
