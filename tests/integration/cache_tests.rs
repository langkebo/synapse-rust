#[cfg(test)]
mod cache_integration_tests {
    use synapse_rust::auth::Claims;
    use synapse_rust::cache::{
        CacheConfig, CacheInvalidationConfig, CacheInvalidationMessage, CacheManager,
        InvalidationType, CACHE_INVALIDATION_CHANNEL, DEFAULT_LOCAL_CACHE_TTL_SECS,
        DEFAULT_REDIS_CACHE_TTL_SECS,
    };
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
            manager.set("test_key", &test_value, 60).await.unwrap();

            let result: Option<String> = manager.get("test_key").await.unwrap();
            assert_eq!(result, Some(test_value));
        });
    }

    #[test]
    fn test_cache_get_nonexistent() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            let result: Option<String> = manager.get("nonexistent").await.unwrap();
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
            manager.set("test_key", &test_value, 60).await.unwrap();
            assert!(manager.get::<String>("test_key").await.unwrap().is_some());

            manager.delete("test_key").await;
            assert!(manager.get::<String>("test_key").await.unwrap().is_none());
        });
    }

    #[test]
    fn test_invalidation_type_serialization() {
        let types = vec![
            InvalidationType::Key,
            InvalidationType::Pattern,
            InvalidationType::All,
            InvalidationType::Prefix,
        ];

        for t in types {
            let json = serde_json::to_string(&t).unwrap();
            let decoded: InvalidationType = serde_json::from_str(&json).unwrap();
            assert_eq!(t, decoded);
        }
    }

    #[test]
    fn test_cache_invalidation_message_serialization() {
        let msg = CacheInvalidationMessage::new(
            "test_key".to_string(),
            InvalidationType::Pattern,
            "instance-1".to_string(),
        )
        .with_reason("Test invalidation".to_string());

        let encoded = msg.encode().unwrap();
        let decoded = CacheInvalidationMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.key, "test_key");
        assert_eq!(decoded.invalidation_type, InvalidationType::Pattern);
        assert_eq!(decoded.sender_instance, "instance-1");
        assert_eq!(decoded.reason, Some("Test invalidation".to_string()));
    }

    #[test]
    fn test_cache_invalidation_config() {
        let config = CacheInvalidationConfig::default();

        assert!(config.enabled);
        assert_eq!(config.channel_name, CACHE_INVALIDATION_CHANNEL);
        assert_eq!(config.local_cache_ttl_secs, DEFAULT_LOCAL_CACHE_TTL_SECS);
        assert_eq!(config.redis_cache_ttl_secs, DEFAULT_REDIS_CACHE_TTL_SECS);
        assert!(config.instance_id.starts_with("instance-"));
    }

    #[test]
    fn test_cache_invalidation_config_custom() {
        let config = CacheInvalidationConfig {
            enabled: false,
            channel_name: "custom:channel".to_string(),
            local_cache_ttl_secs: 60,
            redis_cache_ttl_secs: 7200,
            instance_id: "custom-instance".to_string(),
            redis_url: "redis://custom:6379".to_string(),
        };

        assert!(!config.enabled);
        assert_eq!(config.channel_name, "custom:channel");
        assert_eq!(config.local_cache_ttl_secs, 60);
        assert_eq!(config.redis_cache_ttl_secs, 7200);
        assert_eq!(config.instance_id, "custom-instance");
        assert_eq!(config.redis_url, "redis://custom:6379");
    }

    #[test]
    fn test_local_cache_ttl() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            let ttl = manager.local_cache_ttl();
            assert_eq!(ttl, std::time::Duration::from_secs(DEFAULT_LOCAL_CACHE_TTL_SECS));
        });
    }

    #[test]
    fn test_invalidate_local_key() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            let test_value = "test_value".to_string();
            manager.set("test_key", &test_value, 60).await.unwrap();
            assert!(manager.get::<String>("test_key").await.unwrap().is_some());

            manager.invalidate_local_key("test_key");
            assert!(manager.get::<String>("test_key").await.unwrap().is_none());
        });
    }

    #[test]
    fn test_invalidate_local_pattern() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            manager.set("user:1:profile", &"data1".to_string(), 60).await.unwrap();
            manager.set("user:2:profile", &"data2".to_string(), 60).await.unwrap();
            manager.set("room:1:info", &"data3".to_string(), 60).await.unwrap();

            assert!(manager.get::<String>("user:1:profile").await.unwrap().is_some());
            assert!(manager.get::<String>("user:2:profile").await.unwrap().is_some());
            assert!(manager.get::<String>("room:1:info").await.unwrap().is_some());

            manager.invalidate_local_pattern("user:*");

            assert!(manager.get::<String>("user:1:profile").await.unwrap().is_none());
            assert!(manager.get::<String>("user:2:profile").await.unwrap().is_none());
            assert!(manager.get::<String>("room:1:info").await.unwrap().is_some());
        });
    }

    #[test]
    fn test_invalidate_local_all() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            manager.set("key1", &"value1".to_string(), 60).await.unwrap();
            manager.set("key2", &"value2".to_string(), 60).await.unwrap();
            manager.set("key3", &"value3".to_string(), 60).await.unwrap();

            assert!(manager.get::<String>("key1").await.unwrap().is_some());
            assert!(manager.get::<String>("key2").await.unwrap().is_some());
            assert!(manager.get::<String>("key3").await.unwrap().is_some());

            manager.invalidate_local_all();

            assert!(manager.get::<String>("key1").await.unwrap().is_none());
            assert!(manager.get::<String>("key2").await.unwrap().is_none());
            assert!(manager.get::<String>("key3").await.unwrap().is_none());
        });
    }

    #[test]
    fn test_handle_invalidation_message_key() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            manager.set("test_key", &"value".to_string(), 60).await.unwrap();
            assert!(manager.get::<String>("test_key").await.unwrap().is_some());

            let msg = CacheInvalidationMessage::new(
                "test_key".to_string(),
                InvalidationType::Key,
                "other-instance".to_string(),
            );

            manager.handle_invalidation_message(&msg).await;
            assert!(manager.get::<String>("test_key").await.unwrap().is_none());
        });
    }

    #[test]
    fn test_handle_invalidation_message_pattern() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            manager.set("user:1:data", &"value1".to_string(), 60).await.unwrap();
            manager.set("user:2:data", &"value2".to_string(), 60).await.unwrap();
            manager.set("room:1:data", &"value3".to_string(), 60).await.unwrap();

            let msg = CacheInvalidationMessage::new(
                "user:*".to_string(),
                InvalidationType::Pattern,
                "other-instance".to_string(),
            );

            manager.handle_invalidation_message(&msg).await;

            assert!(manager.get::<String>("user:1:data").await.unwrap().is_none());
            assert!(manager.get::<String>("user:2:data").await.unwrap().is_none());
            assert!(manager.get::<String>("room:1:data").await.unwrap().is_some());
        });
    }

    #[test]
    fn test_handle_invalidation_message_all() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            manager.set("key1", &"value1".to_string(), 60).await.unwrap();
            manager.set("key2", &"value2".to_string(), 60).await.unwrap();

            let msg = CacheInvalidationMessage::new(
                "*".to_string(),
                InvalidationType::All,
                "other-instance".to_string(),
            );

            manager.handle_invalidation_message(&msg).await;

            assert!(manager.get::<String>("key1").await.unwrap().is_none());
            assert!(manager.get::<String>("key2").await.unwrap().is_none());
        });
    }

    #[test]
    fn test_cache_consistency_scenario() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            manager.set("user:profile:123", &"John Doe".to_string(), 60).await.unwrap();

            let result: Option<String> = manager.get("user:profile:123").await.unwrap();
            assert_eq!(result, Some("John Doe".to_string()));

            manager.delete_with_invalidation("user:profile:123", InvalidationType::Key).await;

            let result: Option<String> = manager.get("user:profile:123").await.unwrap();
            assert!(result.is_none());
        });
    }

    #[test]
    fn test_multiple_invalidation_types() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            manager.set("session:abc", &"data1".to_string(), 60).await.unwrap();
            manager.set("session:def", &"data2".to_string(), 60).await.unwrap();
            manager.set("session:ghi", &"data3".to_string(), 60).await.unwrap();

            manager.delete_with_invalidation("session:*", InvalidationType::Pattern).await;

            assert!(manager.get::<String>("session:abc").await.unwrap().is_none());
            assert!(manager.get::<String>("session:def").await.unwrap().is_none());
            assert!(manager.get::<String>("session:ghi").await.unwrap().is_none());
        });
    }

    #[test]
    fn test_invalidation_message_timestamp() {
        let before = chrono::Utc::now().timestamp_millis();
        let msg = CacheInvalidationMessage::new(
            "test_key".to_string(),
            InvalidationType::Key,
            "instance-1".to_string(),
        );
        let after = chrono::Utc::now().timestamp_millis();

        assert!(msg.timestamp >= before);
        assert!(msg.timestamp <= after);
    }

    #[test]
    fn test_cache_manager_without_redis() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            assert!(manager.invalidation_manager().is_none());

            manager.set("test_key", &"test_value".to_string(), 60).await.unwrap();
            let result: Option<String> = manager.get("test_key").await.unwrap();
            assert_eq!(result, Some("test_value".to_string()));
        });
    }
}

#[cfg(test)]
mod cache_consistency_tests {
    use synapse_rust::cache::{
        CacheConfig, CacheInvalidationMessage, CacheManager, InvalidationType,
    };
    use tokio::runtime::Runtime;

    #[test]
    fn test_l1_l2_cache_consistency() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            manager.set("key1", &"value1".to_string(), 60).await.unwrap();

            let result1: Option<String> = manager.get("key1").await.unwrap();
            assert_eq!(result1, Some("value1".to_string()));

            manager.invalidate_local_key("key1");

            let result2: Option<String> = manager.get("key1").await.unwrap();
            assert!(result2.is_none());
        });
    }

    #[test]
    fn test_cache_invalidation_propagation() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            manager.set("shared_key", &"shared_value".to_string(), 60).await.unwrap();

            let msg = CacheInvalidationMessage::new(
                "shared_key".to_string(),
                InvalidationType::Key,
                "instance-2".to_string(),
            );

            manager.handle_invalidation_message(&msg).await;

            let result: Option<String> = manager.get("shared_key").await.unwrap();
            assert!(result.is_none());
        });
    }

    #[test]
    fn test_pattern_invalidation_consistency() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            for i in 0..5 {
                manager
                    .set(&format!("user:{}:profile", i), &format!("user_{}", i), 60)
                    .await
                    .unwrap();
            }

            manager.set("room:1:info", &"room_data".to_string(), 60).await.unwrap();

            let msg = CacheInvalidationMessage::new(
                "user:*".to_string(),
                InvalidationType::Pattern,
                "other-instance".to_string(),
            );
            manager.handle_invalidation_message(&msg).await;

            for i in 0..5 {
                let result: Option<String> = manager.get(&format!("user:{}:profile", i)).await.unwrap();
                assert!(result.is_none(), "User {} profile should be invalidated", i);
            }

            let room_result: Option<String> = manager.get("room:1:info").await.unwrap();
            assert!(room_result.is_some(), "Room data should not be affected");
        });
    }

    #[test]
    fn test_ttl_based_expiration() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig {
                max_capacity: 100,
                time_to_live: 1,
            };
            let manager = CacheManager::new(config);

            manager.set("short_lived", &"value".to_string(), 1).await.unwrap();

            let result1: Option<String> = manager.get("short_lived").await.unwrap();
            assert_eq!(result1, Some("value".to_string()));

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let result2: Option<String> = manager.get("short_lived").await.unwrap();
            assert!(result2.is_none(), "Cache entry should have expired");
        });
    }

    #[test]
    fn test_concurrent_cache_operations() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = std::sync::Arc::new(CacheManager::new(config));
            let mut handles = vec![];

            for i in 0..10 {
                let mgr = manager.clone();
                let handle = tokio::spawn(async move {
                    let key = format!("concurrent_key_{}", i);
                    mgr.set(&key, &format!("value_{}", i), 60).await.unwrap();
                    let result: Option<String> = mgr.get(&key).await.unwrap();
                    assert_eq!(result, Some(format!("value_{}", i)));
                });
                handles.push(handle);
            }

            for handle in handles {
                handle.await.unwrap();
            }

            for i in 0..10 {
                let key = format!("concurrent_key_{}", i);
                let result: Option<String> = manager.get(&key).await.unwrap();
                assert!(result.is_some());
            }
        });
    }

    #[test]
    fn test_cache_invalidation_with_reason() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            manager.set("user_session", &"session_data".to_string(), 60).await.unwrap();

            let msg = CacheInvalidationMessage::new(
                "user_session".to_string(),
                InvalidationType::Key,
                "auth-service".to_string(),
            )
            .with_reason("User logged out".to_string());

            manager.handle_invalidation_message(&msg).await;

            let result: Option<String> = manager.get("user_session").await.unwrap();
            assert!(result.is_none());
        });
    }

    #[test]
    fn test_prefix_invalidation() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = CacheConfig::default();
            let manager = CacheManager::new(config);

            manager.set("cache:user:1", &"data1".to_string(), 60).await.unwrap();
            manager.set("cache:user:2", &"data2".to_string(), 60).await.unwrap();
            manager.set("cache:room:1", &"data3".to_string(), 60).await.unwrap();
            manager.set("other:user:1", &"data4".to_string(), 60).await.unwrap();

            let msg = CacheInvalidationMessage::new(
                "cache:user:".to_string(),
                InvalidationType::Prefix,
                "other-instance".to_string(),
            );
            manager.handle_invalidation_message(&msg).await;

            assert!(manager.get::<String>("cache:user:1").await.unwrap().is_none());
            assert!(manager.get::<String>("cache:user:2").await.unwrap().is_none());
            assert!(manager.get::<String>("cache:room:1").await.unwrap().is_some());
            assert!(manager.get::<String>("other:user:1").await.unwrap().is_some());
        });
    }
}
