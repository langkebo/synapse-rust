// Module-level allow: 这个 file 命名为 tests.rs 是 Rust 测试模块的
// 惯用约定（integration-style 单文件测试）。`mod tests { ... }` 是
// 项目里的标准内部单元测试结构。rename 文件会破坏测试运行路径，
// 因此用 allow 锁住 clippy::module_inception lint。
#![allow(clippy::module_inception)]

#[cfg(test)]
mod tests {
    use super::super::creation_graph::CreationGraph;
    use super::super::service::{LifecycleService, LifecycleServiceConfig};
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
        test_lifecycle_service_with_gate(
            room_store,
            member_store,
            event_store,
            user_store,
            Arc::new(crate::test_mocks::FakeEventAdmissionGate::new()),
        )
    }

    /// Like [`test_lifecycle_service`], but with a caller-supplied admission
    /// gate so a test can prove the creation sequence actually consults it.
    fn test_lifecycle_service_with_gate(
        room_store: InMemoryRoomStore,
        member_store: InMemoryMemberStore,
        event_store: InMemoryEventStore,
        user_store: Arc<dyn UserStore>,
        event_admission_gate: Arc<dyn crate::module_service::EventAdmissionGate>,
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
            event_admission_gate,
        })
    }

    // ── event admission gate on the room-creation sequence (D-1) ───────

    /// 铁律 8 红探针：房间创建事件序列必须真的过门禁。
    ///
    /// 用拒绝型门禁调用 [`LifecycleService::write_creation_event`]，必须得到
    /// 403（而不是静默把事件写进去），且事件存储里一行都不能有。把
    /// `write_creation_event` 里的 `consult_event_admission` 调用删掉，这个
    /// 断言立刻变红——它就是这条接线的证伪器。
    #[tokio::test]
    async fn write_creation_event_is_refused_by_denying_gate_and_persists_nothing() {
        let event_store = InMemoryEventStore::new();
        let svc = test_lifecycle_service_with_gate(
            InMemoryRoomStore::new(),
            InMemoryMemberStore::new(),
            event_store.clone(),
            Arc::new(synapse_storage::test_mocks::FakeUserStore::new()),
            Arc::new(crate::test_mocks::FakeEventAdmissionGate::denying("room creation is closed")),
        );

        let mut graph = CreationGraph::new("11");
        let err = svc
            .write_creation_event(
                &mut graph,
                None,
                "!room:example.com",
                "@alice:example.com",
                "m.room.name",
                Some(""),
                serde_json::json!({"name": "Blocked"}),
                1000,
                true,
                None,
            )
            .await
            .expect_err("a refusing rule must fail the creation sequence");

        assert!(err.is_forbidden(), "expected 403, got: {err:?}");
        assert!(
            err.message().contains("room creation is closed"),
            "the rule's reason must surface, got: {}",
            err.message()
        );
        let written = event_store.get_room_events("!room:example.com", 10).await.unwrap();
        assert!(written.is_empty(), "a refused creation event must not be persisted, found: {written:?}");
    }

    /// `allow_modification = false` 只挡「改写」，不挡「拒绝」。
    ///
    /// create 事件的 content hash 就是 v12 的 room_id（MSC4291），所以它不允许被
    /// 改写；但它仍然必须能被规则拒绝——否则就等于给攻击者留了一条绕开门禁的写路径。
    #[tokio::test]
    async fn create_event_is_still_refused_when_modification_is_disallowed() {
        let event_store = InMemoryEventStore::new();
        let svc = test_lifecycle_service_with_gate(
            InMemoryRoomStore::new(),
            InMemoryMemberStore::new(),
            event_store.clone(),
            Arc::new(synapse_storage::test_mocks::FakeUserStore::new()),
            Arc::new(crate::test_mocks::FakeEventAdmissionGate::denying("create denied")),
        );

        let mut graph = CreationGraph::new("11");
        let err = svc
            .write_creation_event(
                &mut graph,
                None,
                "!room:example.com",
                "@alice:example.com",
                "m.room.create",
                Some(""),
                serde_json::json!({"creator": "@alice:example.com"}),
                1000,
                false,
                None,
            )
            .await
            .expect_err("a refusing rule must fail the create event too");

        assert!(err.is_forbidden(), "expected 403, got: {err:?}");
        let written = event_store.get_room_events("!room:example.com", 10).await.unwrap();
        assert!(written.is_empty(), "a refused create event must not be persisted, found: {written:?}");
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
}
