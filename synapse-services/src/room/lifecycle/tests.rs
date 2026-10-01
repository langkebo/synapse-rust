// Module-level allow: 这个 file 命名为 tests.rs 是 Rust 测试模块的
// 惯用约定（integration-style 单文件测试）。`mod tests { ... }` 是
// 项目里的标准内部单元测试结构。rename 文件会破坏测试运行路径，
// 因此用 allow 锁住 clippy::module_inception lint。
#![allow(clippy::module_inception)]

#[cfg(test)]
mod tests {
    use super::super::service::{LifecycleService, LifecycleServiceConfig};
    use crate::room::CreateRoomConfig;
    use std::sync::Arc;
    use synapse_cache::{CacheConfig, CacheManager};
    use synapse_common::validation::Validator;
    use synapse_storage::test_mocks::{InMemoryEventStore, InMemoryMemberStore, InMemoryRoomStore};
    use synapse_storage::UserStore;

    fn test_validator() -> Arc<Validator> {
        Arc::new(Validator::new().expect("Validator::new should succeed"))
    }

    fn test_lifecycle_service(
        room_store: InMemoryRoomStore,
        member_store: InMemoryMemberStore,
        event_store: InMemoryEventStore,
        user_store: Arc<dyn UserStore>,
    ) -> LifecycleService {
        let event_reader: Arc<dyn synapse_storage::event::EventReader> = Arc::new(event_store.clone());
        let event_writer: Arc<dyn synapse_storage::event::EventWriter> = Arc::new(event_store.clone());
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        LifecycleService::new(LifecycleServiceConfig {
            room_storage: Arc::new(room_store),
            member_storage: Arc::new(member_store),
            event_reader,
            event_writer,
            user_storage: user_store.clone(),
            validator: test_validator(),
            server_name: "example.com".to_string(),
            room_summary_service: None,
            cache,
            app_service_manager: None,
            policy_service: None,
        })
    }

    // ── get_tombstone_event ─────────────────────────────────────────

    #[tokio::test]
    async fn get_tombstone_event_returns_none_when_no_state_events() {
        let svc = test_lifecycle_service(
            InMemoryRoomStore::new(),
            InMemoryMemberStore::new(),
            InMemoryEventStore::new(),
            Arc::new(synapse_storage::test_mocks::FakeUserStore::new()),
        );
        let result = svc.get_tombstone_event("!room:example.com").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_tombstone_event_returns_none_when_no_tombstone() {
        let event_store = InMemoryEventStore::new();
        // Pre-seed a non-tombstone state event
        event_store
            .create_event(synapse_storage::CreateEventParams {
                event_id: "$ev1:example.com".to_string(),
                room_id: "!room:example.com".to_string(),
                user_id: "@user:example.com".to_string(),
                event_type: "m.room.name".to_string(),
                content: serde_json::json!({"name": "Test"}),
                state_key: Some("".to_string()),
                origin_server_ts: 1000,
                redacts: None,
            })
            .await
            .unwrap();
        let svc = test_lifecycle_service(
            InMemoryRoomStore::new(),
            InMemoryMemberStore::new(),
            event_store,
            Arc::new(synapse_storage::test_mocks::FakeUserStore::new()),
        );
        let result = svc.get_tombstone_event("!room:example.com").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_tombstone_event_finds_existing_tombstone() {
        let event_store = InMemoryEventStore::new();
        event_store
            .create_event(synapse_storage::CreateEventParams {
                event_id: "$tomb:example.com".to_string(),
                room_id: "!room:example.com".to_string(),
                user_id: "@user:example.com".to_string(),
                event_type: "m.room.tombstone".to_string(),
                content: serde_json::json!({"body": "Room upgraded", "replacement_room": "!new:example.com"}),
                state_key: Some("".to_string()),
                origin_server_ts: 2000,
                redacts: None,
            })
            .await
            .unwrap();
        let svc = test_lifecycle_service(
            InMemoryRoomStore::new(),
            InMemoryMemberStore::new(),
            event_store,
            Arc::new(synapse_storage::test_mocks::FakeUserStore::new()),
        );
        let result = svc.get_tombstone_event("!room:example.com").await.unwrap();
        let tombstone = result.expect("should find tombstone event");
        assert_eq!(tombstone["type"], "m.room.tombstone");
        assert_eq!(tombstone["content"]["body"], "Room upgraded");
    }

    // ── is_room_upgrade_allowed ──────────────────────────────────────

    async fn seed_room_and_member(room_store: &InMemoryRoomStore, member_store: &InMemoryMemberStore, creator: &str) {
        room_store.create_room("!test:example.com", creator, "invite", "10", false).await.unwrap();
        member_store.add_member("!test:example.com", creator, "join", None).await.unwrap();
    }

    #[tokio::test]
    async fn is_room_upgrade_allowed_creator_can_upgrade() {
        let room_store = InMemoryRoomStore::new();
        let member_store = InMemoryMemberStore::new();
        seed_room_and_member(&room_store, &member_store, "@creator:example.com").await;
        let svc = test_lifecycle_service(
            room_store,
            member_store,
            InMemoryEventStore::new(),
            Arc::new(synapse_storage::test_mocks::FakeUserStore::new()),
        );
        let allowed = svc.is_room_upgrade_allowed("!test:example.com", "@creator:example.com").await.unwrap();
        assert!(allowed);
    }

    #[tokio::test]
    async fn is_room_upgrade_allowed_non_creator_cannot_upgrade() {
        let room_store = InMemoryRoomStore::new();
        let member_store = InMemoryMemberStore::new();
        seed_room_and_member(&room_store, &member_store, "@creator:example.com").await;
        member_store.add_member("!test:example.com", "@other:example.com", "join", None).await.unwrap();
        let svc = test_lifecycle_service(
            room_store,
            member_store,
            InMemoryEventStore::new(),
            Arc::new(synapse_storage::test_mocks::FakeUserStore::new()),
        );
        let allowed = svc.is_room_upgrade_allowed("!test:example.com", "@other:example.com").await.unwrap();
        assert!(!allowed);
    }

    #[tokio::test]
    async fn is_room_upgrade_allowed_non_member_cannot_upgrade() {
        let room_store = InMemoryRoomStore::new();
        let member_store = InMemoryMemberStore::new();
        seed_room_and_member(&room_store, &member_store, "@creator:example.com").await;
        let svc = test_lifecycle_service(
            room_store,
            member_store,
            InMemoryEventStore::new(),
            Arc::new(synapse_storage::test_mocks::FakeUserStore::new()),
        );
        let allowed = svc.is_room_upgrade_allowed("!test:example.com", "@outsider:example.com").await.unwrap();
        assert!(!allowed);
    }

    #[tokio::test]
    async fn is_room_upgrade_allowed_nonexistent_room_errors() {
        let svc = test_lifecycle_service(
            InMemoryRoomStore::new(),
            InMemoryMemberStore::new(),
            InMemoryEventStore::new(),
            Arc::new(synapse_storage::test_mocks::FakeUserStore::new()),
        );
        let err = svc.is_room_upgrade_allowed("!nonexistent:example.com", "@user:example.com").await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    // ── determine_join_rule ────────────────────────────────────────────

    #[test]
    fn determine_join_rule_defaults_to_invite() {
        assert_eq!(LifecycleService::determine_join_rule(None), "invite");
        assert_eq!(LifecycleService::determine_join_rule(Some("private_chat")), "invite");
    }

    #[test]
    fn determine_join_rule_public_chat_returns_public() {
        assert_eq!(LifecycleService::determine_join_rule(Some("public_chat")), "public");
    }

    // ── is_public_visibility ───────────────────────────────────────────

    #[test]
    fn is_public_visibility_defaults_to_private() {
        assert!(!LifecycleService::is_public_visibility(None));
        assert!(!LifecycleService::is_public_visibility(Some("private")));
    }

    #[test]
    fn is_public_visibility_explicit_public_is_true() {
        assert!(LifecycleService::is_public_visibility(Some("public")));
    }

    // ── format_room_alias ──────────────────────────────────────────────

    #[test]
    fn format_room_alias_returns_formatted_alias() {
        let svc = test_lifecycle_service(
            InMemoryRoomStore::new(),
            InMemoryMemberStore::new(),
            InMemoryEventStore::new(),
            Arc::new(synapse_storage::test_mocks::FakeUserStore::new()),
        );
        let alias = svc.format_room_alias(Some("testroom"));
        assert_eq!(alias, Some("#testroom:example.com".to_string()));
    }

    #[test]
    fn format_room_alias_returns_none_when_no_alias() {
        let svc = test_lifecycle_service(
            InMemoryRoomStore::new(),
            InMemoryMemberStore::new(),
            InMemoryEventStore::new(),
            Arc::new(synapse_storage::test_mocks::FakeUserStore::new()),
        );
        assert_eq!(svc.format_room_alias(None), None);
    }

    // ── build_room_response ────────────────────────────────────────────

    #[test]
    fn build_room_response_includes_room_id_and_alias() {
        let response = LifecycleService::build_room_response("!room:example.com", Some("#alias:example.com"));
        assert_eq!(response["room_id"], "!room:example.com");
        assert_eq!(response["room_alias"], "#alias:example.com");
    }

    #[test]
    fn build_room_response_without_alias() {
        let response = LifecycleService::build_room_response("!room:example.com", None);
        assert_eq!(response["room_id"], "!room:example.com");
        assert!(response["room_alias"].is_null());
    }

    // ── create_room 埋点（room_operations_total）────────────────────────

    /// `create_room` 的埋点包装必须把**失败**也记进 `room_operations_total`：
    /// `room_operations_total{operation="create",outcome="error"}` 正是
    /// `RoomCreationFailureRate` 告警读的标签组合。
    ///
    /// 用「不可创建的房间版本」（999）让内层在**不碰数据库**的情况下确定性失败，
    /// 因此这个用例可以跑在纯 mock 服务上。断言用**增量**而非绝对值：全局
    /// `ServerMetrics` 句柄是整个测试二进制共享的，别的用例可能已经装过。
    #[tokio::test]
    async fn create_room_records_failure_outcome_with_labels() {
        use synapse_common::metrics::MetricsCollector;
        use synapse_common::server_metrics::{global_server_metrics, install_global_server_metrics, ServerMetrics};

        if global_server_metrics().is_none() {
            install_global_server_metrics(Arc::new(ServerMetrics::new(Arc::new(MetricsCollector::new()))));
        }
        let Some(metrics) = global_server_metrics() else { return };

        let svc = test_lifecycle_service(
            InMemoryRoomStore::new(),
            InMemoryMemberStore::new(),
            InMemoryEventStore::new(),
            Arc::new(synapse_storage::test_mocks::FakeUserStore::new()),
        );

        let config = CreateRoomConfig { room_version: Some("999".to_string()), ..Default::default() };
        let error = svc
            .create_room("@alice:example.com", config)
            .await
            .expect_err("room version 999 cannot be created, so create_room must fail");

        let labels = ["create", "error", "999", "private", error.code_str()];
        let counter = metrics
            .room_operations_total
            .get_counter(&labels)
            .unwrap_or_else(|| panic!("expected room_operations_total{labels:?} to be recorded"));
        assert!(counter.get() >= 1, "failed create must increment the labeled counter");
    }
}
