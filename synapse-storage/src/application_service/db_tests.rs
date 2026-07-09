#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::env;

    async fn test_pool() -> Arc<sqlx::PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    /// Clean all application-service-related tables for rows matching the given suffix pattern.
    /// Deletes from child/reference tables first to avoid FK violations.
    async fn cleanup_with_suffix(pool: &sqlx::PgPool, suffix: &str) {
        let pattern = format!("%{suffix}");
        let _ = sqlx::query("DELETE FROM application_service_user_namespaces WHERE as_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM application_service_room_alias_namespaces WHERE as_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM application_service_room_namespaces WHERE as_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM application_service_statistics WHERE as_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ =
            sqlx::query("DELETE FROM application_service_state WHERE as_id LIKE $1").bind(&pattern).execute(pool).await;
        let _ = sqlx::query("DELETE FROM application_service_events WHERE as_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM application_service_transactions WHERE as_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ =
            sqlx::query("DELETE FROM application_service_users WHERE as_id LIKE $1").bind(&pattern).execute(pool).await;
        let _ = sqlx::query("DELETE FROM application_services WHERE as_id LIKE $1").bind(&pattern).execute(pool).await;
    }

    fn make_registration(
        as_id: &str,
        url: &str,
        as_token: &str,
        hs_token: &str,
        sender: &str,
    ) -> RegisterApplicationServiceRequest {
        RegisterApplicationServiceRequest {
            as_id: as_id.to_string(),
            url: url.to_string(),
            as_token: as_token.to_string(),
            hs_token: hs_token.to_string(),
            sender: sender.to_string(),
            description: Some("Integration test bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: Some(vec!["test".to_string()]),
            namespaces: Some(serde_json::json!({
                "users": [{"exclusive": true, "regex": "@_test_.*:example.com"}],
                "aliases": [],
                "rooms": []
            })),
            api_key: None,
            config: Some(serde_json::json!({"source": "db_test"})),
        }
    }

    // ---- register ----

    #[tokio::test]
    async fn test_register_creates_service() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        let svc = storage.register(req).await.expect("register should succeed");

        assert_eq!(svc.as_id, as_id);
        assert_eq!(svc.url, "http://localhost:9001");
        assert_eq!(svc.as_token, as_token);
        assert_eq!(svc.hs_token, hs_token);
        assert_eq!(svc.sender_localpart, sender);
        assert!(svc.is_enabled);
        assert!(!svc.is_rate_limited);
        assert_eq!(svc.protocols, vec!["test"]);
        assert_eq!(svc.description.as_deref(), Some("Integration test bridge"));

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_register_duplicate_as_id_fails() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req1 = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        storage.register(req1).await.expect("first register should succeed");

        let req2 = make_registration(&as_id, "http://localhost:9002", &as_token, &hs_token, &sender);
        let result = storage.register(req2).await;
        assert!(result.is_err(), "second register with same as_id should fail");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- upsert_registration ----

    #[tokio::test]
    async fn test_upsert_registration_inserts_new() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        let svc = storage.upsert_registration(req).await.expect("upsert_registration should succeed");

        assert_eq!(svc.as_id, as_id);
        assert!(svc.is_enabled);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_upsert_registration_updates_existing() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        // First upsert = insert
        let req1 = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        let svc1 = storage.upsert_registration(req1).await.expect("first upsert should succeed");
        assert_eq!(svc1.url, "http://localhost:9001");

        // Second upsert with different URL = update
        let req2 = RegisterApplicationServiceRequest {
            url: "http://localhost:9002".to_string(),
            ..make_registration(&as_id, "http://localhost:9002", &as_token, &hs_token, &sender)
        };
        let svc2 = storage.upsert_registration(req2).await.expect("second upsert should succeed");
        assert_eq!(svc2.as_id, as_id);
        assert_eq!(svc2.url, "http://localhost:9002");

        // Verify only one row exists
        let fetched = storage.get_by_id(&as_id).await.expect("get_by_id should succeed");
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().url, "http://localhost:9002");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- get_by_id ----

    #[tokio::test]
    async fn test_get_by_id_found() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        storage.register(req).await.expect("register should succeed");

        let found = storage.get_by_id(&as_id).await.expect("get_by_id should succeed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().as_id, as_id);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_by_id_not_found() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_nonexistent_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let found = storage.get_by_id(&as_id).await.expect("get_by_id should succeed");
        assert!(found.is_none());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- get_by_token ----

    #[tokio::test]
    async fn test_get_by_token_found() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        storage.register(req).await.expect("register should succeed");

        let found = storage.get_by_token(&as_token).await.expect("get_by_token should succeed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().as_id, as_id);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_by_token_not_found() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");

        cleanup_with_suffix(&pool, &suffix).await;

        let found = storage.get_by_token(&format!("bogus_token_{suffix}")).await.expect("get_by_token should succeed");
        assert!(found.is_none());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_by_token_ignores_disabled_service() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        storage.register(req).await.expect("register should succeed");

        // Disable the service
        storage
            .update(&as_id, &UpdateApplicationServiceRequest::new().is_enabled(false))
            .await
            .expect("update should succeed");

        let found = storage.get_by_token(&as_token).await.expect("get_by_token should succeed");
        assert!(found.is_none(), "disabled service should not be found by token");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- get_by_hs_token ----

    #[tokio::test]
    async fn test_get_by_hs_token_found() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        storage.register(req).await.expect("register should succeed");

        let found = storage.get_by_hs_token(&hs_token).await.expect("get_by_hs_token should succeed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().as_id, as_id);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_by_hs_token_not_found() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");

        cleanup_with_suffix(&pool, &suffix).await;

        let found =
            storage.get_by_hs_token(&format!("bogus_hs_{suffix}")).await.expect("get_by_hs_token should succeed");
        assert!(found.is_none());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- get_all_active ----

    #[tokio::test]
    async fn test_get_all_active_returns_only_enabled() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id1 = format!("as_active_{suffix}");
        let as_id2 = format!("as_inactive_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let req1 = make_registration(
            &as_id1,
            "http://localhost:9001",
            &format!("tok1_{suffix}"),
            &format!("hs1_{suffix}"),
            &format!("@bot1_{suffix}:t.example.com"),
        );
        storage.register(req1).await.expect("register active should succeed");

        let req2 = make_registration(
            &as_id2,
            "http://localhost:9002",
            &format!("tok2_{suffix}"),
            &format!("hs2_{suffix}"),
            &format!("@bot2_{suffix}:t.example.com"),
        );
        storage.register(req2).await.expect("register inactive should succeed");
        storage
            .update(&as_id2, &UpdateApplicationServiceRequest::new().is_enabled(false))
            .await
            .expect("disable should succeed");

        let active = storage.get_all_active().await.expect("get_all_active should succeed");
        let active_ids: Vec<_> = active.iter().map(|s| s.as_id.as_str()).collect();
        assert!(active_ids.contains(&as_id1.as_str()), "active list should contain enabled service");
        assert!(!active_ids.contains(&as_id2.as_str()), "active list should NOT contain disabled service");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_all_active_returns_empty_when_no_enabled() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_inactive_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(
            &as_id,
            "http://localhost:9001",
            &format!("tok_{suffix}"),
            &format!("hs_{suffix}"),
            &format!("@bot_{suffix}:t.example.com"),
        );
        storage.register(req).await.expect("register should succeed");
        storage
            .update(&as_id, &UpdateApplicationServiceRequest::new().is_enabled(false))
            .await
            .expect("disable should succeed");

        let active = storage.get_all_active().await.expect("get_all_active should succeed");
        // Filter out pre-existing data — only check our test rows
        let our_active: Vec<_> = active.into_iter().filter(|s| s.as_id.ends_with(&suffix)).collect();
        assert!(our_active.is_empty(), "no active services for our test suffix");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- update ----

    #[tokio::test]
    async fn test_update_modifies_fields() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:t.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        storage.register(req).await.expect("register should succeed");

        let update_req = UpdateApplicationServiceRequest::new()
            .url("http://localhost:9999")
            .description("Updated description")
            .is_rate_limited(true)
            .protocols(vec!["irc".to_string(), "matrix".to_string()])
            .api_key("new_key");

        let updated = storage.update(&as_id, &update_req).await.expect("update should succeed");
        assert_eq!(updated.url, "http://localhost:9999");
        assert_eq!(updated.description.as_deref(), Some("Updated description"));
        assert!(updated.is_rate_limited);
        assert_eq!(updated.protocols, vec!["irc", "matrix"]);
        assert_eq!(updated.api_key.as_deref(), Some("new_key"));

        // Verify persisted
        let fetched = storage.get_by_id(&as_id).await.expect("get_by_id should succeed");
        let fetched = fetched.unwrap();
        assert_eq!(fetched.url, "http://localhost:9999");
        assert_eq!(fetched.description.as_deref(), Some("Updated description"));

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- update_timestamp ----

    #[tokio::test]
    async fn test_update_timestamp_sets_updated_ts() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:t.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        let created = storage.register(req).await.expect("register should succeed");
        assert!(created.updated_ts.is_none(), "newly created should have no updated_ts");

        storage.update_timestamp(&as_id).await.expect("update_timestamp should succeed");

        let fetched = storage.get_by_id(&as_id).await.expect("get_by_id should succeed");
        let fetched = fetched.unwrap();
        assert!(fetched.updated_ts.is_some(), "should have updated_ts after update_timestamp");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- unregister ----

    #[tokio::test]
    async fn test_unregister_removes_service() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:t.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        storage.register(req).await.expect("register should succeed");

        // Verify it exists
        let found = storage.get_by_id(&as_id).await.expect("get_by_id should succeed");
        assert!(found.is_some(), "should exist before unregister");

        storage.unregister(&as_id).await.expect("unregister should succeed");

        let after = storage.get_by_id(&as_id).await.expect("get_by_id should succeed");
        assert!(after.is_none(), "should be gone after unregister");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- set_state / get_state / get_all_states ----

    #[tokio::test]
    async fn test_set_and_get_state() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        // Must register AS first due to FK constraint on application_service_state
        let req = make_registration(
            &as_id,
            "http://localhost:9001",
            &format!("tok_{suffix}"),
            &format!("hs_{suffix}"),
            &format!("@bot_{suffix}:t.example.com"),
        );
        storage.register(req).await.expect("register should succeed");

        let state =
            storage.set_state(&as_id, "config", "{\"theme\":\"dark\"}").await.expect("set_state should succeed");
        assert_eq!(state.as_id, as_id);
        assert_eq!(state.state_key, "config");
        assert_eq!(state.state_value, "{\"theme\":\"dark\"}");

        let fetched = storage.get_state(&as_id, "config").await.expect("get_state should succeed");
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().state_value, "{\"theme\":\"dark\"}");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_set_state_overwrites_existing() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(
            &as_id,
            "http://localhost:9001",
            &format!("tok_{suffix}"),
            &format!("hs_{suffix}"),
            &format!("@bot_{suffix}:t.example.com"),
        );
        storage.register(req).await.expect("register should succeed");

        storage.set_state(&as_id, "counter", "1").await.expect("first set_state should succeed");
        storage.set_state(&as_id, "counter", "2").await.expect("second set_state should succeed");

        let fetched = storage.get_state(&as_id, "counter").await.expect("get_state should succeed");
        assert_eq!(fetched.unwrap().state_value, "2");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_all_states_for_as_id() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(
            &as_id,
            "http://localhost:9001",
            &format!("tok_{suffix}"),
            &format!("hs_{suffix}"),
            &format!("@bot_{suffix}:t.example.com"),
        );
        storage.register(req).await.expect("register should succeed");

        storage.set_state(&as_id, "key1", "val1").await.expect("set_state key1 should succeed");
        storage.set_state(&as_id, "key2", "val2").await.expect("set_state key2 should succeed");

        let all = storage.get_all_states(&as_id).await.expect("get_all_states should succeed");
        assert_eq!(all.len(), 2);
        let keys: Vec<_> = all.iter().map(|s| s.state_key.as_str()).collect();
        assert!(keys.contains(&"key1"));
        assert!(keys.contains(&"key2"));

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_state_not_found() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let state = storage.get_state(&as_id, "nonexistent_key").await.expect("get_state should succeed");
        assert!(state.is_none());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- add_event / get_pending_events / count_pending_events / mark_event_processed ----

    #[tokio::test]
    async fn test_event_lifecycle() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let event_id = format!("ev_{suffix}");
        let room_id = format!("!room_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        // Add event
        let event = storage
            .add_event(
                &event_id,
                &as_id,
                &room_id,
                "m.room.message",
                "sender",
                serde_json::json!({"body": "hello"}),
                None,
            )
            .await
            .expect("add_event should succeed");
        assert_eq!(event.event_id, event_id);
        assert_eq!(event.as_id, as_id);
        assert_eq!(event.room_id, room_id);
        assert!(event.processed_ts.is_none(), "new event should be unprocessed");

        // Count pending
        let count = storage.count_pending_events(&as_id).await.expect("count_pending_events should succeed");
        assert_eq!(count, 1);

        // Get pending
        let pending = storage.get_pending_events(&as_id, 10).await.expect("get_pending_events should succeed");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event_id, event_id);

        // Mark processed
        storage.mark_event_processed(&event_id).await.expect("mark_event_processed should succeed");

        let count_after = storage.count_pending_events(&as_id).await.expect("count_pending_events should succeed");
        assert_eq!(count_after, 0, "no pending events after marking processed");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_pending_events_respects_limit() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let room_id = format!("!room_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        for i in 0..5 {
            storage
                .add_event(
                    &format!("ev_{suffix}_{i}"),
                    &as_id,
                    &room_id,
                    "m.room.message",
                    "sender",
                    serde_json::json!({"idx": i}),
                    None,
                )
                .await
                .expect("add_event should succeed");
        }

        let pending = storage.get_pending_events(&as_id, 3).await.expect("get_pending_events should succeed");
        assert_eq!(pending.len(), 3, "should respect limit of 3");

        let total = storage.count_pending_events(&as_id).await.expect("count_pending_events should succeed");
        assert_eq!(total, 5, "total should still be 5");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_mark_event_processed_idempotent() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let event_id = format!("ev_{suffix}");
        let room_id = format!("!room_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        storage
            .add_event(&event_id, &as_id, &room_id, "m.room.message", "sender", serde_json::json!({}), None)
            .await
            .expect("add_event should succeed");

        // Mark processed twice
        storage.mark_event_processed(&event_id).await.expect("first mark should succeed");
        storage.mark_event_processed(&event_id).await.expect("second mark should succeed (idempotent)");

        let count = storage.count_pending_events(&as_id).await.expect("count_pending_events should succeed");
        assert_eq!(count, 0);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- create_transaction / complete_transaction / fail_transaction / get_pending_transactions / count_pending_transactions ----

    #[tokio::test]
    async fn test_create_and_complete_transaction() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let txn_id = format!("txn_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let events = vec![serde_json::json!({"type": "m.room.message", "body": "hi"})];
        let txn =
            storage.create_transaction(&as_id, &txn_id, &events).await.expect("create_transaction should succeed");
        assert_eq!(txn.as_id, as_id);
        assert_eq!(txn.txn_id, txn_id);
        assert!(txn.completed_ts.is_none(), "new transaction should be incomplete");

        // Count pending
        let pending_count =
            storage.count_pending_transactions(&as_id).await.expect("count_pending_transactions should succeed");
        assert_eq!(pending_count, 1);

        // Get pending transactions
        let pending = storage.get_pending_transactions(&as_id).await.expect("get_pending_transactions should succeed");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].txn_id, txn_id);

        // Complete
        storage.complete_transaction(&as_id, &txn_id).await.expect("complete_transaction should succeed");

        let after_count =
            storage.count_pending_transactions(&as_id).await.expect("count_pending_transactions should succeed");
        assert_eq!(after_count, 0, "no pending after completion");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_fail_transaction_increments_retry() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let txn_id = format!("txn_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        storage.create_transaction(&as_id, &txn_id, &[]).await.expect("create_transaction should succeed");

        let failed = storage
            .fail_transaction(&as_id, &txn_id, "connection refused")
            .await
            .expect("fail_transaction should succeed");
        assert_eq!(failed.retry_count, 1);
        assert_eq!(failed.last_error.as_deref(), Some("connection refused"));

        let failed2 = storage.fail_transaction(&as_id, &txn_id, "timeout").await.expect("second fail should succeed");
        assert_eq!(failed2.retry_count, 2);
        assert_eq!(failed2.last_error.as_deref(), Some("timeout"));

        // Failed transaction is still pending (completed_ts still null)
        let pending_count =
            storage.count_pending_transactions(&as_id).await.expect("count_pending_transactions should succeed");
        assert_eq!(pending_count, 1, "failed txns are still pending");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- register_virtual_user / get_virtual_users ----

    #[tokio::test]
    async fn test_register_and_get_virtual_users() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let user1 = format!("@vu1_{suffix}:test.example.com");
        let user2 = format!("@vu2_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        // Must register AS first due to FK constraint on application_service_users
        let req = make_registration(
            &as_id,
            "http://localhost:9001",
            &format!("tok_{suffix}"),
            &format!("hs_{suffix}"),
            &format!("@bot_{suffix}:t.example.com"),
        );
        storage.register(req).await.expect("register should succeed");

        let vu1 = storage
            .register_virtual_user(&as_id, &user1, Some("VU One"), Some("mxc://avatar1"))
            .await
            .expect("register_virtual_user should succeed");
        assert_eq!(vu1.user_id, user1);
        assert_eq!(vu1.displayname.as_deref(), Some("VU One"));

        let vu2 = storage
            .register_virtual_user(&as_id, &user2, None, None)
            .await
            .expect("register_virtual_user should succeed");
        assert_eq!(vu2.user_id, user2);
        assert!(vu2.displayname.is_none());

        let users = storage.get_virtual_users(&as_id).await.expect("get_virtual_users should succeed");
        assert_eq!(users.len(), 2);
        let user_ids: Vec<_> = users.iter().map(|u| u.user_id.as_str()).collect();
        assert!(user_ids.contains(&user1.as_str()));
        assert!(user_ids.contains(&user2.as_str()));

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_register_virtual_user_upserts() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let user_id = format!("@vu_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(
            &as_id,
            "http://localhost:9001",
            &format!("tok_{suffix}"),
            &format!("hs_{suffix}"),
            &format!("@bot_{suffix}:t.example.com"),
        );
        storage.register(req).await.expect("register should succeed");

        let vu1 = storage
            .register_virtual_user(&as_id, &user_id, Some("Original"), None)
            .await
            .expect("first register should succeed");
        assert_eq!(vu1.displayname.as_deref(), Some("Original"));

        // Re-register with new displayname
        let vu2 = storage
            .register_virtual_user(&as_id, &user_id, Some("Updated"), Some("mxc://avatar"))
            .await
            .expect("second register should succeed (upsert)");
        assert_eq!(vu2.displayname.as_deref(), Some("Updated"));
        assert_eq!(vu2.avatar_url.as_deref(), Some("mxc://avatar"));

        let users = storage.get_virtual_users(&as_id).await.expect("get_virtual_users should succeed");
        assert_eq!(users.len(), 1, "only one row for the same user");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_virtual_users_empty_for_unknown_as_id() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_unknown_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let users = storage.get_virtual_users(&as_id).await.expect("get_virtual_users should succeed");
        assert!(users.is_empty());

        cleanup_with_suffix(&pool, &suffix).await;
    }
}
