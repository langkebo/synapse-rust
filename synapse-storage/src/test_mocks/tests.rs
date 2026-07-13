use super::*;
#[cfg(feature = "openclaw-routes")]
use crate::ai_connection::AiConnectionStoreApi;
#[cfg(feature = "burn-after-read")]
use crate::burn_after_read::BurnAfterReadStoreApi;
use crate::oidc_user_mapping::OidcUserMappingStoreApi;
use crate::room_summary::RoomSummaryStoreApi;
use crate::sliding_sync::SlidingSyncStoreApi;
use crate::user::UserStore;
use std::sync::Arc;

#[tokio::test]
async fn shared_fake_user_store_is_usable_via_trait_object() {
    let store: SharedFakeUserStore = shared_fake_user_store();
    let _trait_ref: Arc<dyn UserStore> = store.clone();
    assert!(!store.is_user_locked("@nobody:example.com").await.unwrap());
}

#[tokio::test]
async fn seed_locked_users_makes_lock_visible() {
    let store = shared_fake_user_store();
    seed_locked_users(
        &store,
        vec![crate::LockedUser {
            id: 1,
            user_id: "@bad:example.com".to_string(),
            reason: Some("spam".to_string()),
            locked_by: "@admin:example.com".to_string(),
            created_ts: 1_700_000_000_000,
            unlocked_ts: None,
            is_active: true,
        }],
    )
    .await;

    assert!(store.is_user_locked("@bad:example.com").await.unwrap());
    assert!(!store.is_user_locked("@innocent:example.com").await.unwrap());
}

// ── InMemoryRoomStore tests ──────────────────────────────────────

#[tokio::test]
async fn room_create_and_get() {
    let store = InMemoryRoomStore::new();
    store.create_room("!r:example.com", "@alice:example.com", "invite", "1", true).await.unwrap();
    let room = store.get_room("!r:example.com").await.unwrap().unwrap();
    assert_eq!(room.room_id, "!r:example.com");
    assert_eq!(room.creator_user_id.as_deref(), Some("@alice:example.com"));
    assert!(room.is_public);
}

#[tokio::test]
async fn room_not_found_returns_none() {
    let store = InMemoryRoomStore::new();
    assert!(store.get_room("!nonexistent:example.com").await.unwrap().is_none());
}

#[tokio::test]
async fn room_batch_fetch_filters_missing() {
    let store = InMemoryRoomStore::new();
    store.create_room("!a:example.com", "@u:example.com", "invite", "1", false).await.unwrap();
    let rooms = store.get_rooms_batch(&["!a:example.com".into(), "!b:example.com".into()]).await.unwrap();
    assert_eq!(rooms.len(), 1);
}

#[tokio::test]
async fn room_alias_round_trip() {
    let store = InMemoryRoomStore::new();
    store.create_room("!r:example.com", "@u:example.com", "invite", "1", false).await.unwrap();
    store.set_room_alias("!r:example.com", "#alias:example.com", "@u:example.com").await.unwrap();
    let room_id = store.get_room_by_alias("#alias:example.com").await.unwrap();
    assert_eq!(room_id.as_deref(), Some("!r:example.com"));
}

// ── InMemoryEventStore tests ─────────────────────────────────────

#[tokio::test]
async fn event_create_and_get() {
    let store = InMemoryEventStore::new();
    let params = crate::event::CreateEventParams {
        event_id: "$ev1:example.com".into(),
        room_id: "!r:example.com".into(),
        user_id: "@alice:example.com".into(),
        event_type: "m.room.message".into(),
        content: serde_json::json!({"body": "hello"}),
        state_key: None,
        origin_server_ts: 1_700_000_000_000,
        redacts: None,
    };
    let event = store.create_event(params).await.unwrap();
    assert_eq!(event.event_id, "$ev1:example.com");
    assert_eq!(event.event_type, "m.room.message");
}

#[tokio::test]
async fn event_find_missing_ids() {
    let store = InMemoryEventStore::new();
    let params = crate::event::CreateEventParams {
        event_id: "$ev1:example.com".into(),
        room_id: "!r:example.com".into(),
        user_id: "@alice:example.com".into(),
        event_type: "m.room.message".into(),
        content: serde_json::json!({}),
        state_key: None,
        origin_server_ts: 1_700_000_000_000,
        redacts: None,
    };
    store.create_event(params).await.unwrap();
    let missing = store.find_missing_event_ids(&["$ev1:example.com".into(), "$ev2:example.com".into()]).await.unwrap();
    assert_eq!(missing, vec!["$ev2:example.com"]);
}

#[tokio::test]
async fn event_redact_content_replaces_with_empty_json() {
    let store = InMemoryEventStore::new();
    let params = crate::event::CreateEventParams {
        event_id: "$ev1:example.com".into(),
        room_id: "!r:example.com".into(),
        user_id: "@alice:example.com".into(),
        event_type: "m.room.message".into(),
        content: serde_json::json!({"body": "secret"}),
        state_key: None,
        origin_server_ts: 1_700_000_000_000,
        redacts: None,
    };
    store.create_event(params).await.unwrap();
    store.redact_event_content("$ev1:example.com", None).await.unwrap();
    let redacted = store.get_event("$ev1:example.com").await.unwrap().unwrap();
    assert_eq!(redacted.content, serde_json::json!({}));
}

// ── EventReader state event tests ────────────────────────────────

#[tokio::test]
async fn get_state_event_returns_matching_state_event() {
    let store = InMemoryEventStore::new();
    let params = crate::event::CreateEventParams {
        event_id: "$s1:example.com".into(),
        room_id: "!r:example.com".into(),
        user_id: "@alice:example.com".into(),
        event_type: "m.room.name".into(),
        content: serde_json::json!({"name": "Test Room"}),
        state_key: Some("".into()),
        origin_server_ts: 1_700_000_001_000,
        redacts: None,
    };
    store.create_event(params).await.unwrap();

    // Also create a non-state event (no state_key) — should be ignored.
    let non_state = crate::event::CreateEventParams {
        event_id: "$m1:example.com".into(),
        room_id: "!r:example.com".into(),
        user_id: "@alice:example.com".into(),
        event_type: "m.room.message".into(),
        content: serde_json::json!({"body": "hi"}),
        state_key: None,
        origin_server_ts: 1_700_000_002_000,
        redacts: None,
    };
    store.create_event(non_state).await.unwrap();

    let result = store.get_state_event("!r:example.com", "m.room.name", "").await.unwrap();
    assert!(result.is_some(), "should return state event for matching type+key");
    let ev = result.unwrap();
    assert_eq!(ev.event_id, "$s1:example.com");
    assert_eq!(ev.room_id, "!r:example.com");
    assert_eq!(ev.sender, "@alice:example.com");
    assert_eq!(ev.event_type.as_deref(), Some("m.room.name"));
}

#[tokio::test]
async fn get_state_event_returns_none_for_missing_state_key() {
    let store = InMemoryEventStore::new();
    let result = store.get_state_event("!r:example.com", "m.room.name", "").await.unwrap();
    assert!(result.is_none(), "should return None when no matching state event");
}

#[tokio::test]
async fn get_state_event_returns_none_for_non_matching_room() {
    let store = InMemoryEventStore::new();
    let params = crate::event::CreateEventParams {
        event_id: "$s1:example.com".into(),
        room_id: "!room_a:example.com".into(),
        user_id: "@alice:example.com".into(),
        event_type: "m.room.name".into(),
        content: serde_json::json!({"name": "A"}),
        state_key: Some("".into()),
        origin_server_ts: 1_700_000_000_000,
        redacts: None,
    };
    store.create_event(params).await.unwrap();

    let result = store.get_state_event("!room_b:example.com", "m.room.name", "").await.unwrap();
    assert!(result.is_none(), "should return None for different room");
}

// ── get_state_events_by_type tests ──────────────────────────────

#[tokio::test]
async fn get_state_events_by_type_returns_only_matching_type() {
    let store = InMemoryEventStore::new();
    // m.room.name state events (2 different state_keys)
    store
        .create_event(crate::event::CreateEventParams {
            event_id: "$n1:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.name".into(),
            content: serde_json::json!({"name": "First"}),
            state_key: Some("".into()),
            origin_server_ts: 1_700_000_001_000,
            redacts: None,
        })
        .await
        .unwrap();
    store
        .create_event(crate::event::CreateEventParams {
            event_id: "$n2:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.name".into(),
            content: serde_json::json!({"name": "Second"}),
            state_key: Some("alt".into()),
            origin_server_ts: 1_700_000_002_000,
            redacts: None,
        })
        .await
        .unwrap();
    // m.room.topic state event (different type, should be excluded)
    store
        .create_event(crate::event::CreateEventParams {
            event_id: "$t1:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.topic".into(),
            content: serde_json::json!({"topic": "Chat"}),
            state_key: Some("".into()),
            origin_server_ts: 1_700_000_003_000,
            redacts: None,
        })
        .await
        .unwrap();

    let results = store.get_state_events_by_type("!r:example.com", "m.room.name").await.unwrap();
    assert_eq!(results.len(), 2, "should return exactly 2 m.room.name events");
    let event_ids: Vec<&str> = results.iter().map(|e| e.event_id.as_str()).collect();
    assert!(event_ids.contains(&"$n1:example.com"));
    assert!(event_ids.contains(&"$n2:example.com"));
}

#[tokio::test]
async fn get_state_events_by_type_deduplicates_by_state_key() {
    let store = InMemoryEventStore::new();
    // Two m.room.name events with same state_key "" — only the latest should be returned.
    store
        .create_event(crate::event::CreateEventParams {
            event_id: "$old:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.name".into(),
            content: serde_json::json!({"name": "Old"}),
            state_key: Some("".into()),
            origin_server_ts: 1_700_000_001_000,
            redacts: None,
        })
        .await
        .unwrap();
    store
        .create_event(crate::event::CreateEventParams {
            event_id: "$new:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.name".into(),
            content: serde_json::json!({"name": "New"}),
            state_key: Some("".into()),
            origin_server_ts: 1_700_000_002_000,
            redacts: None,
        })
        .await
        .unwrap();

    let results = store.get_state_events_by_type("!r:example.com", "m.room.name").await.unwrap();
    assert_eq!(results.len(), 1, "should deduplicate by state_key, keeping latest");
    assert_eq!(results[0].event_id, "$new:example.com");
}

#[tokio::test]
async fn get_state_events_by_type_returns_empty_for_no_match() {
    let store = InMemoryEventStore::new();
    let results = store.get_state_events_by_type("!r:example.com", "m.room.name").await.unwrap();
    assert!(results.is_empty());
}

// ── get_state_events_at_or_before tests ─────────────────────────

#[tokio::test]
async fn get_state_events_at_or_before_filters_by_timestamp() {
    let store = InMemoryEventStore::new();
    // Event at t=1000
    store
        .create_event(crate::event::CreateEventParams {
            event_id: "$old:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.name".into(),
            content: serde_json::json!({"name": "Old"}),
            state_key: Some("".into()),
            origin_server_ts: 1000,
            redacts: None,
        })
        .await
        .unwrap();
    // Event at t=3000
    store
        .create_event(crate::event::CreateEventParams {
            event_id: "$new:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.name".into(),
            content: serde_json::json!({"name": "New"}),
            state_key: Some("".into()),
            origin_server_ts: 3000,
            redacts: None,
        })
        .await
        .unwrap();

    // Query at t=2000 — only the t=1000 event should be visible.
    let results = store.get_state_events_at_or_before("!r:example.com", 2000).await.unwrap();
    assert_eq!(results.len(), 1, "only the old event should be visible at t=2000");
    assert_eq!(results[0].event_id, "$old:example.com");
}

#[tokio::test]
async fn get_state_events_at_or_before_deduplicates_by_state_key() {
    let store = InMemoryEventStore::new();
    // Two events with same state_key, both at or before t=2000 — latest should win.
    store
        .create_event(crate::event::CreateEventParams {
            event_id: "$first:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.name".into(),
            content: serde_json::json!({"name": "First"}),
            state_key: Some("".into()),
            origin_server_ts: 1000,
            redacts: None,
        })
        .await
        .unwrap();
    store
        .create_event(crate::event::CreateEventParams {
            event_id: "$second:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.name".into(),
            content: serde_json::json!({"name": "Second"}),
            state_key: Some("".into()),
            origin_server_ts: 1500,
            redacts: None,
        })
        .await
        .unwrap();

    let results = store.get_state_events_at_or_before("!r:example.com", 2000).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].event_id, "$second:example.com");
}

// ── OidcUserMappingStore tests ──────────────────────────────────

#[tokio::test]
async fn oidc_insert_and_get_mapping() {
    let store = InMemoryOidcUserMappingStore::new();
    store.insert_mapping("https://idp.example.com", "sub-123", "@alice:example.com", 1_700_000_000_000).await.unwrap();
    let bound = store.get_bound_user_id("https://idp.example.com", "sub-123").await.unwrap();
    assert_eq!(bound.as_deref(), Some("@alice:example.com"));
}

#[tokio::test]
async fn oidc_get_none_for_unknown_subject() {
    let store = InMemoryOidcUserMappingStore::new();
    let bound = store.get_bound_user_id("https://idp.example.com", "unknown").await.unwrap();
    assert!(bound.is_none());
}

#[tokio::test]
async fn oidc_update_last_authenticated_increments_counter() {
    let store = InMemoryOidcUserMappingStore::new();
    store.insert_mapping("https://idp.example.com", "sub-123", "@alice:example.com", 1_700_000_000_000).await.unwrap();
    store.update_last_authenticated("https://idp.example.com", "sub-123", 1_700_000_010_000).await.unwrap();
    // Verify mapping still resolves correctly after update
    let bound = store.get_bound_user_id("https://idp.example.com", "sub-123").await.unwrap();
    assert_eq!(bound.as_deref(), Some("@alice:example.com"));
}

#[tokio::test]
async fn oidc_issuer_isolation() {
    let store = InMemoryOidcUserMappingStore::new();
    store
        .insert_mapping("https://idp-a.example.com", "sub-1", "@alice:a.example.com", 1_700_000_000_000)
        .await
        .unwrap();
    store
        .insert_mapping("https://idp-b.example.com", "sub-1", "@alice:b.example.com", 1_700_000_000_000)
        .await
        .unwrap();

    let a = store.get_bound_user_id("https://idp-a.example.com", "sub-1").await.unwrap();
    let b = store.get_bound_user_id("https://idp-b.example.com", "sub-1").await.unwrap();
    assert_eq!(a.as_deref(), Some("@alice:a.example.com"));
    assert_eq!(b.as_deref(), Some("@alice:b.example.com"));
}

// ── AiConnectionStore tests (behind openclaw-routes feature) ──────

#[cfg(feature = "openclaw-routes")]
#[tokio::test]
async fn ai_connection_create_and_get() {
    let store = InMemoryAiConnectionStore::new();
    let conn = crate::ai_connection::AiConnection {
        id: "conn-1".into(),
        user_id: "@alice:example.com".into(),
        provider: "openai".into(),
        config: Some(serde_json::json!({"api_key": "sk-test"})),
        is_active: true,
        created_ts: 1_700_000_000_000,
        updated_ts: None,
    };
    store.create_connection(&conn).await.unwrap();
    let got = store.get_connection("conn-1").await.unwrap().unwrap();
    assert_eq!(got.id, "conn-1");
    assert_eq!(got.provider, "openai");
    assert!(got.is_active);
}

#[cfg(feature = "openclaw-routes")]
#[tokio::test]
async fn ai_connection_get_none_for_unknown() {
    let store = InMemoryAiConnectionStore::new();
    assert!(store.get_connection("nonexistent").await.unwrap().is_none());
}

#[cfg(feature = "openclaw-routes")]
#[tokio::test]
async fn ai_connection_list_by_user() {
    let store = InMemoryAiConnectionStore::new();
    store
        .create_connection(&crate::ai_connection::AiConnection {
            id: "c1".into(),
            user_id: "@alice:example.com".into(),
            provider: "openai".into(),
            config: None,
            is_active: true,
            created_ts: 1000,
            updated_ts: None,
        })
        .await
        .unwrap();
    store
        .create_connection(&crate::ai_connection::AiConnection {
            id: "c2".into(),
            user_id: "@bob:example.com".into(),
            provider: "anthropic".into(),
            config: None,
            is_active: true,
            created_ts: 2000,
            updated_ts: None,
        })
        .await
        .unwrap();
    store
        .create_connection(&crate::ai_connection::AiConnection {
            id: "c3".into(),
            user_id: "@alice:example.com".into(),
            provider: "siliconflow".into(),
            config: None,
            is_active: false,
            created_ts: 3000,
            updated_ts: None,
        })
        .await
        .unwrap();

    let alice_conns = store.get_user_connections("@alice:example.com").await.unwrap();
    assert_eq!(alice_conns.len(), 2);
    assert_eq!(alice_conns[0].id, "c3"); // newest first
    assert_eq!(alice_conns[1].id, "c1");
}

#[cfg(feature = "openclaw-routes")]
#[tokio::test]
async fn ai_connection_filter_by_provider() {
    let store = InMemoryAiConnectionStore::new();
    store
        .create_connection(&crate::ai_connection::AiConnection {
            id: "c1".into(),
            user_id: "@alice:example.com".into(),
            provider: "openai".into(),
            config: None,
            is_active: true,
            created_ts: 1000,
            updated_ts: None,
        })
        .await
        .unwrap();
    store
        .create_connection(&crate::ai_connection::AiConnection {
            id: "c2".into(),
            user_id: "@alice:example.com".into(),
            provider: "openai".into(),
            config: None,
            is_active: false,
            created_ts: 2000,
            updated_ts: None,
        })
        .await
        .unwrap();

    let result = store.get_user_provider_connection("@alice:example.com", "openai").await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().id, "c1"); // only active one

    let result = store.get_user_provider_connection("@alice:example.com", "anthropic").await.unwrap();
    assert!(result.is_none());
}

#[cfg(feature = "openclaw-routes")]
#[tokio::test]
async fn ai_connection_update_status() {
    let store = InMemoryAiConnectionStore::new();
    store
        .create_connection(&crate::ai_connection::AiConnection {
            id: "c1".into(),
            user_id: "@alice:example.com".into(),
            provider: "openai".into(),
            config: None,
            is_active: true,
            created_ts: 1000,
            updated_ts: None,
        })
        .await
        .unwrap();

    store.update_connection_status("c1", false).await.unwrap();
    let conn = store.get_connection("c1").await.unwrap().unwrap();
    assert!(!conn.is_active);
}

#[cfg(feature = "openclaw-routes")]
#[tokio::test]
async fn ai_connection_delete() {
    let store = InMemoryAiConnectionStore::new();
    store
        .create_connection(&crate::ai_connection::AiConnection {
            id: "c1".into(),
            user_id: "@alice:example.com".into(),
            provider: "openai".into(),
            config: None,
            is_active: true,
            created_ts: 1000,
            updated_ts: None,
        })
        .await
        .unwrap();

    store.delete_connection("c1").await.unwrap();
    assert!(store.get_connection("c1").await.unwrap().is_none());
}

// ── InMemoryRateLimitStore tests ───────────────────────────────────

#[tokio::test]
async fn rate_limit_upsert_and_get() {
    let store = InMemoryRateLimitStore::new();
    store.upsert_user_rate_limit("@alice:example.com", 10.0, 5).await.unwrap();
    let record = store.get_user_rate_limit("@alice:example.com").await.unwrap().unwrap();
    assert_eq!(record.messages_per_second, Some(10.0));
    assert_eq!(record.burst_count, Some(5));
}

#[tokio::test]
async fn rate_limit_get_none_for_unknown() {
    let store = InMemoryRateLimitStore::new();
    assert!(store.get_user_rate_limit("@unknown:example.com").await.unwrap().is_none());
}

#[tokio::test]
async fn rate_limit_upsert_overwrites_existing() {
    let store = InMemoryRateLimitStore::new();
    store.upsert_user_rate_limit("@alice:example.com", 10.0, 5).await.unwrap();
    store.upsert_user_rate_limit("@alice:example.com", 20.0, 10).await.unwrap();
    let record = store.get_user_rate_limit("@alice:example.com").await.unwrap().unwrap();
    assert_eq!(record.messages_per_second, Some(20.0));
    assert_eq!(record.burst_count, Some(10));
}

#[tokio::test]
async fn rate_limit_delete_removes_record() {
    let store = InMemoryRateLimitStore::new();
    store.upsert_user_rate_limit("@alice:example.com", 10.0, 5).await.unwrap();
    store.delete_user_rate_limit("@alice:example.com").await.unwrap();
    assert!(store.get_user_rate_limit("@alice:example.com").await.unwrap().is_none());
}

// ── InMemoryMemberStore tests ────────────────────────────────────

#[tokio::test]
async fn member_join_and_query() {
    let store = InMemoryMemberStore::new();
    store.add_member("!r:example.com", "@alice:example.com", "join", Some("Alice")).await.unwrap();
    assert!(store.is_member("!r:example.com", "@alice:example.com").await.unwrap());
    assert_eq!(
        store.get_membership_state("!r:example.com", "@alice:example.com").await.unwrap().as_deref(),
        Some("join")
    );
}

#[tokio::test]
async fn member_ban_updates_state() {
    let store = InMemoryMemberStore::new();
    store.add_member("!r:example.com", "@bad:example.com", "join", None).await.unwrap();
    store.ban_member("!r:example.com", "@bad:example.com", "@admin:example.com").await.unwrap();
    let member = store.get_member("!r:example.com", "@bad:example.com").await.unwrap().unwrap();
    assert_eq!(member.membership, "ban");
    assert_eq!(member.banned_by.as_deref(), Some("@admin:example.com"));
}

#[tokio::test]
async fn member_joined_rooms_lists_only_joined() {
    let store = InMemoryMemberStore::new();
    store.add_member("!r1:example.com", "@alice:example.com", "join", None).await.unwrap();
    store.add_member("!r2:example.com", "@alice:example.com", "leave", None).await.unwrap();
    store.add_member("!r3:example.com", "@alice:example.com", "join", None).await.unwrap();
    let rooms = store.get_joined_rooms("@alice:example.com").await.unwrap();
    assert_eq!(rooms.len(), 2);
    assert!(rooms.contains(&"!r1:example.com".to_string()));
    assert!(rooms.contains(&"!r3:example.com".to_string()));
}

// ── InMemoryRoomTagStore tests ─────────────────────────────────────

#[tokio::test]
async fn room_tag_add_and_list() {
    let store = InMemoryRoomTagStore::new();
    store.add_tag("@alice:example.com", "!r:example.com", "favourite", Some(0.5)).await.unwrap();
    let tags = store.get_tags("@alice:example.com", "!r:example.com").await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].tag, "favourite");
}

#[tokio::test]
async fn room_tag_upsert_replaces_order() {
    let store = InMemoryRoomTagStore::new();
    store.add_tag("@alice:example.com", "!r:example.com", "favourite", Some(0.5)).await.unwrap();
    store.add_tag("@alice:example.com", "!r:example.com", "favourite", Some(0.9)).await.unwrap();
    let tags = store.get_tags("@alice:example.com", "!r:example.com").await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].order, Some(0.9));
}

#[tokio::test]
async fn room_tag_remove() {
    let store = InMemoryRoomTagStore::new();
    store.add_tag("@alice:example.com", "!r:example.com", "favourite", None).await.unwrap();
    store.add_tag("@alice:example.com", "!r:example.com", "u.lowpriority", None).await.unwrap();
    store.remove_tag("@alice:example.com", "!r:example.com", "favourite").await.unwrap();
    let tags = store.get_tags("@alice:example.com", "!r:example.com").await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].tag, "u.lowpriority");
}

#[tokio::test]
async fn room_tag_get_all_tags_across_rooms() {
    let store = InMemoryRoomTagStore::new();
    store.add_tag("@alice:example.com", "!r1:example.com", "favourite", None).await.unwrap();
    store.add_tag("@alice:example.com", "!r2:example.com", "u.lowpriority", None).await.unwrap();
    let tags = store.get_all_tags("@alice:example.com").await.unwrap();
    assert_eq!(tags.len(), 2);
}

#[tokio::test]
async fn room_tag_empty_for_unknown_user() {
    let store = InMemoryRoomTagStore::new();
    let tags = store.get_tags("@unknown:example.com", "!r:example.com").await.unwrap();
    assert!(tags.is_empty());
}

// ── InMemoryBurnAfterReadStore ───────────────────────────────────────

#[cfg(feature = "burn-after-read")]
#[derive(Clone, Default)]
pub struct InMemoryBurnAfterReadStore {
    settings: std::sync::Arc<
        tokio::sync::RwLock<std::collections::HashMap<(String, String), crate::burn_after_read::BurnSettingsRow>>,
    >,
    pending: std::sync::Arc<tokio::sync::RwLock<Vec<crate::burn_after_read::BurnPendingRow>>>,
    logs: std::sync::Arc<tokio::sync::RwLock<Vec<crate::burn_after_read::BurnLogRow>>>,
    defaults: std::sync::Arc<
        tokio::sync::RwLock<std::collections::HashMap<String, crate::burn_after_read::BurnUserDefaultsRow>>,
    >,
    next_id: std::sync::Arc<std::sync::atomic::AtomicI64>,
}

#[cfg(feature = "burn-after-read")]
impl InMemoryBurnAfterReadStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(feature = "burn-after-read")]
#[async_trait::async_trait]
impl crate::burn_after_read::BurnAfterReadStoreApi for InMemoryBurnAfterReadStore {
    async fn get_settings(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> Result<Option<crate::burn_after_read::BurnSettingsRow>, sqlx::Error> {
        Ok(self.settings.read().await.get(&(user_id.to_string(), room_id.to_string())).cloned())
    }

    async fn set_settings(
        &self,
        user_id: &str,
        room_id: &str,
        is_enabled: bool,
        burn_after_ms: i64,
    ) -> Result<crate::burn_after_read::BurnSettingsRow, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let row = crate::burn_after_read::BurnSettingsRow {
            user_id: user_id.to_string(),
            room_id: room_id.to_string(),
            is_enabled,
            burn_after_ms,
            created_ts: now,
            updated_ts: Some(now),
        };
        self.settings.write().await.insert((user_id.to_string(), room_id.to_string()), row.clone());
        Ok(row)
    }

    async fn schedule_burn(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
        delete_ts: i64,
    ) -> Result<crate::burn_after_read::BurnPendingRow, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let row = crate::burn_after_read::BurnPendingRow {
            id,
            user_id: user_id.to_string(),
            room_id: room_id.to_string(),
            event_id: event_id.to_string(),
            created_ts: chrono::Utc::now().timestamp_millis(),
            delete_ts,
            is_processed: false,
        };
        self.pending.write().await.push(row.clone());
        Ok(row)
    }

    async fn cancel_burn(&self, user_id: &str, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        let mut pending = self.pending.write().await;
        for p in pending.iter_mut() {
            if p.user_id == user_id && p.room_id == room_id && p.event_id == event_id && !p.is_processed {
                p.is_processed = true;
            }
        }
        Ok(())
    }

    async fn get_pending_burns(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> Result<Vec<crate::burn_after_read::BurnPendingRow>, sqlx::Error> {
        Ok(self
            .pending
            .read()
            .await
            .iter()
            .filter(|p| p.user_id == user_id && p.room_id == room_id && !p.is_processed)
            .cloned()
            .collect())
    }

    async fn get_expired_burns(&self, now_ms: i64) -> Result<Vec<crate::burn_after_read::BurnPendingRow>, sqlx::Error> {
        Ok(self.pending.read().await.iter().filter(|p| p.delete_ts <= now_ms && !p.is_processed).cloned().collect())
    }

    async fn mark_burn_processed(&self, id: i64) -> Result<(), sqlx::Error> {
        let mut pending = self.pending.write().await;
        for p in pending.iter_mut() {
            if p.id == id {
                p.is_processed = true;
            }
        }
        Ok(())
    }

    async fn log_burned_event(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
        burned_ts: i64,
    ) -> Result<(), sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.logs.write().await.push(crate::burn_after_read::BurnLogRow {
            id,
            user_id: user_id.to_string(),
            room_id: room_id.to_string(),
            event_id: event_id.to_string(),
            burned_ts,
        });
        Ok(())
    }

    async fn get_user_stats(&self, user_id: &str) -> Result<crate::burn_after_read::BurnStatsRow, sqlx::Error> {
        let pending = self.pending.read().await;
        let logs = self.logs.read().await;
        let settings = self.settings.read().await;
        Ok(crate::burn_after_read::BurnStatsRow {
            total_burned: logs.iter().filter(|l| l.user_id == user_id).count() as i64,
            total_pending: pending.iter().filter(|p| p.user_id == user_id && !p.is_processed).count() as i64,
            rooms_enabled: settings.iter().filter(|((uid, _), s)| uid == user_id && s.is_enabled).count() as i64,
        })
    }

    async fn get_user_default(
        &self,
        user_id: &str,
    ) -> Result<Option<crate::burn_after_read::BurnUserDefaultsRow>, sqlx::Error> {
        Ok(self.defaults.read().await.get(user_id).cloned())
    }

    async fn set_user_default(&self, user_id: &str, default_burn_ms: i64) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        self.defaults.write().await.insert(
            user_id.to_string(),
            crate::burn_after_read::BurnUserDefaultsRow {
                user_id: user_id.to_string(),
                default_burn_ms,
                created_ts: now,
                updated_ts: Some(now),
            },
        );
        Ok(())
    }
}

#[cfg(feature = "burn-after-read")]
#[tokio::test]
async fn burn_after_read_settings_round_trip() {
    let store = InMemoryBurnAfterReadStore::new();
    let result = store.get_settings("@alice:example.com", "!room:example.com").await.unwrap();
    assert!(result.is_none());

    let row = store.set_settings("@alice:example.com", "!room:example.com", true, 60_000).await.unwrap();
    assert!(row.is_enabled);
    assert_eq!(row.burn_after_ms, 60_000);

    let fetched = store.get_settings("@alice:example.com", "!room:example.com").await.unwrap().unwrap();
    assert!(fetched.is_enabled);
}

#[cfg(feature = "burn-after-read")]
#[tokio::test]
async fn burn_after_read_schedule_and_expire() {
    let store = InMemoryBurnAfterReadStore::new();
    let now = chrono::Utc::now().timestamp_millis();

    store.schedule_burn("@alice:example.com", "!room:example.com", "$ev1", now + 60_000).await.unwrap();
    store.schedule_burn("@alice:example.com", "!room:example.com", "$ev2", now - 60_000).await.unwrap();

    let pending = store.get_pending_burns("@alice:example.com", "!room:example.com").await.unwrap();
    assert_eq!(pending.len(), 2);

    let expired = store.get_expired_burns(now).await.unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].event_id, "$ev2");
}

#[cfg(feature = "burn-after-read")]
#[tokio::test]
async fn burn_after_read_cancel_removes_from_pending() {
    let store = InMemoryBurnAfterReadStore::new();
    let now = chrono::Utc::now().timestamp_millis();

    store.schedule_burn("@alice:example.com", "!room:example.com", "$ev1", now + 60_000).await.unwrap();

    store.cancel_burn("@alice:example.com", "!room:example.com", "$ev1").await.unwrap();

    let pending = store.get_pending_burns("@alice:example.com", "!room:example.com").await.unwrap();
    assert!(pending.is_empty());
}

#[cfg(feature = "burn-after-read")]
#[tokio::test]
async fn burn_after_read_user_default() {
    let store = InMemoryBurnAfterReadStore::new();
    let result = store.get_user_default("@alice:example.com").await.unwrap();
    assert!(result.is_none());

    store.set_user_default("@alice:example.com", 30_000).await.unwrap();

    let fetched = store.get_user_default("@alice:example.com").await.unwrap().unwrap();
    assert_eq!(fetched.default_burn_ms, 30_000);
}

// ── InMemoryRoomSummaryStore tests ────────────────────────────────

#[tokio::test]
async fn room_summary_create_and_get() {
    let store = InMemoryRoomSummaryStore::new();
    let summary = store
        .create_summary(crate::room_summary::CreateRoomSummaryRequest {
            room_id: "!r:example.com".into(),
            name: Some("Test Room".into()),
            topic: None,
            room_type: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: None,
            history_visibility: None,
            guest_access: None,
            is_direct: None,
            is_space: None,
        })
        .await
        .unwrap();
    assert_eq!(summary.room_id, "!r:example.com");
    assert_eq!(summary.name.as_deref(), Some("Test Room"));

    let fetched = store.get_summary("!r:example.com").await.unwrap().unwrap();
    assert_eq!(fetched.room_id, "!r:example.com");
}

#[tokio::test]
async fn room_summary_add_and_get_members() {
    let store = InMemoryRoomSummaryStore::new();
    store
        .create_summary(crate::room_summary::CreateRoomSummaryRequest {
            room_id: "!r:example.com".into(),
            name: None,
            topic: None,
            room_type: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: None,
            history_visibility: None,
            guest_access: None,
            is_direct: None,
            is_space: None,
        })
        .await
        .unwrap();

    let member = store
        .add_member(crate::room_summary::CreateSummaryMemberRequest {
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            display_name: Some("Alice".into()),
            avatar_url: None,
            membership: "join".into(),
            is_hero: Some(true),
            last_active_ts: None,
        })
        .await
        .unwrap();
    assert_eq!(member.user_id, "@alice:example.com");
    assert!(member.is_hero);

    let members = store.get_members("!r:example.com").await.unwrap();
    assert_eq!(members.len(), 1);
}

#[tokio::test]
async fn room_summary_state_round_trip() {
    let store = InMemoryRoomSummaryStore::new();
    let state =
        store.set_state("!r:example.com", "m.room.name", "", None, serde_json::json!({"name": "Lobby"})).await.unwrap();
    assert_eq!(state.event_type, "m.room.name");

    let fetched = store.get_state("!r:example.com", "m.room.name", "").await.unwrap().unwrap();
    assert_eq!(fetched.content, serde_json::json!({"name": "Lobby"}));
}

#[tokio::test]
async fn room_summary_stats_update_and_get() {
    let store = InMemoryRoomSummaryStore::new();
    let stats = store.update_stats("!r:example.com", 10, 2, 8, 1, 1024).await.unwrap();
    assert_eq!(stats.total_events, 10);

    let fetched = store.get_stats("!r:example.com").await.unwrap().unwrap();
    assert_eq!(fetched.total_messages, 8);
}

#[tokio::test]
async fn room_summary_queue_lifecycle() {
    let store = InMemoryRoomSummaryStore::new();
    store.queue_update("!r:example.com", "$ev1", "m.room.message", None, 10).await.unwrap();
    store.queue_update("!r:example.com", "$ev2", "m.room.member", Some("@a:ex.com"), 5).await.unwrap();

    let pending = store.get_pending_updates(10).await.unwrap();
    assert_eq!(pending.len(), 2);
    // Highest priority first
    assert_eq!(pending[0].event_id, "$ev1");
    assert_eq!(pending[0].priority, 10);

    store.mark_update_processed(pending[0].id).await.unwrap();
    let remaining = store.get_pending_updates(10).await.unwrap();
    assert_eq!(remaining.len(), 1);
}

// ── InMemorySlidingSyncStore tests ────────────────────────────────

#[tokio::test]
async fn sliding_sync_token_create_and_get() {
    let store = InMemorySlidingSyncStore::new();
    let token = store.create_or_update_token("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap();
    assert_eq!(token.user_id, "@alice:ex.com");
    assert!(token.token.starts_with("sst_"));

    let fetched = store.get_token("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap().unwrap();
    assert_eq!(fetched.id, token.id);

    assert!(store.validate_pos("@alice:ex.com", "DEV1", Some("conn1"), &token.pos.to_string()).await.unwrap());
    assert!(!store.validate_pos("@alice:ex.com", "DEV1", Some("conn1"), "bad_pos").await.unwrap());
}

#[tokio::test]
async fn sliding_sync_list_save_and_get() {
    let store = InMemorySlidingSyncStore::new();
    let list = store
        .save_list("@alice:ex.com", "DEV1", Some("conn1"), "my_list", &["by_name".to_string()], None, None, &[(0, 10)])
        .await
        .unwrap();
    assert_eq!(list.list_key, "my_list");

    let lists = store.get_lists("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap();
    assert_eq!(lists.len(), 1);

    store.delete_list("@alice:ex.com", "DEV1", Some("conn1"), "my_list").await.unwrap();
    let empty = store.get_lists("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap();
    assert!(empty.is_empty());
}

#[tokio::test]
async fn sliding_sync_room_upsert_and_get() {
    let store = InMemorySlidingSyncStore::new();
    let room = store
        .upsert_room(
            "@alice:ex.com",
            "DEV1",
            "!r:ex.com",
            Some("conn1"),
            Some("my_list"),
            100,
            0,
            2,
            true,
            false,
            false,
            false,
            Some("Test Room"),
            None,
            200,
        )
        .await
        .unwrap();
    assert_eq!(room.room_id, "!r:ex.com");
    assert!(room.is_dm);

    let fetched = store.get_room("@alice:ex.com", "DEV1", "!r:ex.com", Some("conn1")).await.unwrap().unwrap();
    assert_eq!(fetched.bump_stamp, Some(100));
    assert_eq!(fetched.notification_count, 2);
}

#[tokio::test]
async fn sliding_sync_rooms_for_list() {
    let store = InMemorySlidingSyncStore::new();
    store
        .upsert_room(
            "@alice:ex.com",
            "DEV1",
            "!r:ex.com",
            Some("conn1"),
            Some("my_list"),
            200,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("Room A"),
            None,
            200,
        )
        .await
        .unwrap();
    store
        .upsert_room(
            "@alice:ex.com",
            "DEV1",
            "!r2:ex.com",
            Some("conn1"),
            Some("my_list"),
            100,
            0,
            0,
            false,
            false,
            false,
            false,
            Some("Room B"),
            None,
            100,
        )
        .await
        .unwrap();

    let rooms = store
        .get_rooms_for_list(crate::sliding_sync::SlidingSyncListQuery {
            user_id: "@alice:ex.com",
            device_id: "DEV1",
            conn_id: Some("conn1"),
            list_key: "my_list",
            start: 0,
            end: 10,
            filters: None,
        })
        .await
        .unwrap();
    assert_eq!(rooms.len(), 2);
    // Higher bump_stamp first
    assert_eq!(rooms[0].room_id, "!r:ex.com");
}

#[tokio::test]
async fn sliding_sync_notification_counts_and_bump() {
    let store = InMemorySlidingSyncStore::new();
    store
        .upsert_room(
            "@alice:ex.com",
            "DEV1",
            "!r:ex.com",
            Some("conn1"),
            Some("my_list"),
            0,
            0,
            0,
            false,
            false,
            false,
            false,
            None,
            None,
            0,
        )
        .await
        .unwrap();

    store.update_notification_counts("@alice:ex.com", "DEV1", "!r:ex.com", Some("conn1"), 5, 10).await.unwrap();
    store.bump_room("@alice:ex.com", "DEV1", "!r:ex.com", Some("conn1"), 42).await.unwrap();

    let room = store.get_room("@alice:ex.com", "DEV1", "!r:ex.com", Some("conn1")).await.unwrap().unwrap();
    assert_eq!(room.highlight_count, 5);
    assert_eq!(room.notification_count, 10);
    assert_eq!(room.bump_stamp, Some(42));
}

#[tokio::test]
async fn sliding_sync_token_cleanup() {
    let store = InMemorySlidingSyncStore::new();
    store.create_or_update_token("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap();
    // Token has a future expiry, so cleanup should remove none
    let removed = store.cleanup_expired_tokens().await.unwrap();
    assert_eq!(removed, 0);
    assert!(store.get_token("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap().is_some());
}

#[tokio::test]
async fn sliding_sync_delete_connection_data() {
    let store = InMemorySlidingSyncStore::new();
    store.create_or_update_token("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap();
    store.save_list("@alice:ex.com", "DEV1", Some("conn1"), "l1", &[], None, None, &[(0, 5)]).await.unwrap();

    store.delete_connection_data("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap();

    assert!(store.get_token("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap().is_none());
    assert!(store.get_lists("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap().is_empty());
}
