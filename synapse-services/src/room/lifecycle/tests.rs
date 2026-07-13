#[cfg(test)]
mod tests {
    use super::super::service::{LifecycleService, LifecycleServiceConfig};
    use std::sync::Arc;
    use synapse_common::validation::Validator;
    use synapse_storage::test_mocks::{InMemoryEventStore, InMemoryMemberStore, InMemoryRoomStore};
    use synapse_storage::UserStore;

    use crate::UserService;

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
        LifecycleService::new(LifecycleServiceConfig {
            room_storage: Arc::new(room_store),
            member_storage: Arc::new(member_store),
            event_reader,
            event_writer,
            user_storage: user_store.clone(),
            user_service: Arc::new(UserService::new(user_store)),
            validator: test_validator(),
            server_name: "example.com".to_string(),
            room_summary_service: None,
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

    // ── migrate_room_content ───────────────────────────────────────────

    #[tokio::test]
    async fn migrate_room_content_creator_can_migrate() {
        let room_store = InMemoryRoomStore::new();
        let member_store = InMemoryMemberStore::new();
        seed_room_and_member(&room_store, &member_store, "@creator:example.com").await;
        // Create target room
        room_store.create_room("!target:example.com", "@creator:example.com", "invite", "10", false).await.unwrap();
        let svc = test_lifecycle_service(
            room_store,
            member_store,
            InMemoryEventStore::new(),
            Arc::new(synapse_storage::test_mocks::FakeUserStore::new()),
        );
        svc.migrate_room_content("!test:example.com", "!target:example.com", "@creator:example.com").await.unwrap();
    }

    #[tokio::test]
    async fn migrate_room_content_non_creator_is_forbidden() {
        let room_store = InMemoryRoomStore::new();
        let member_store = InMemoryMemberStore::new();
        seed_room_and_member(&room_store, &member_store, "@creator:example.com").await;
        room_store.create_room("!target:example.com", "@creator:example.com", "invite", "10", false).await.unwrap();
        let svc = test_lifecycle_service(
            room_store,
            member_store,
            InMemoryEventStore::new(),
            Arc::new(synapse_storage::test_mocks::FakeUserStore::new()),
        );
        let err = svc
            .migrate_room_content("!test:example.com", "!target:example.com", "@other:example.com")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Only room creator can migrate content"));
    }

    #[tokio::test]
    async fn migrate_room_content_nonexistent_target_errors() {
        let room_store = InMemoryRoomStore::new();
        let member_store = InMemoryMemberStore::new();
        seed_room_and_member(&room_store, &member_store, "@creator:example.com").await;
        let svc = test_lifecycle_service(
            room_store,
            member_store,
            InMemoryEventStore::new(),
            Arc::new(synapse_storage::test_mocks::FakeUserStore::new()),
        );
        let err = svc
            .migrate_room_content("!test:example.com", "!nonexistent:example.com", "@creator:example.com")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Target room not found"));
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
