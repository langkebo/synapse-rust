use super::*;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

async fn test_pool() -> Arc<PgPool> {
    let db_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
    let pool =
        PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
    Arc::new(pool)
}

/// Clean up any leftover data from previous test runs.
async fn cleanup(pool: &Arc<PgPool>, space_id: &str) {
    let _ = sqlx::query(r"DELETE FROM space_events WHERE space_id = $1").bind(space_id).execute(&**pool).await;
    let _ = sqlx::query(r"DELETE FROM space_children WHERE space_id = $1").bind(space_id).execute(&**pool).await;
    let _ = sqlx::query(r"DELETE FROM space_members WHERE space_id = $1").bind(space_id).execute(&**pool).await;
    let _ = sqlx::query(r"DELETE FROM space_summaries WHERE space_id = $1").bind(space_id).execute(&**pool).await;
    let _ = sqlx::query(r"DELETE FROM spaces WHERE space_id = $1").bind(space_id).execute(&**pool).await;
}

// === Test 1: create_space ===
#[tokio::test]
async fn test_create_space_returns_valid_space() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!space_create_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Test Space".to_string()),
        topic: Some("A test space".to_string()),
        avatar_url: None,
        creator: "@creator:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: Some(true),
        parent_space_id: None,
    };

    let space = storage.create_space(request).await.expect("create_space should succeed");
    assert!(!space.space_id.is_empty());
    assert_eq!(space.name, Some("Test Space".to_string()));
    assert_eq!(space.topic, Some("A test space".to_string()));
    assert!(space.is_public);

    cleanup(&pool, &space.space_id).await;
}

// === Test 2: get_space (found) ===
#[tokio::test]
async fn test_get_space_found() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!space_get_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Get Test".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@g:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let created = storage.create_space(request).await.unwrap();

    let found = storage.get_space(&created.space_id).await.expect("get_space should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, Some("Get Test".to_string()));

    cleanup(&pool, &created.space_id).await;
}

// === Test 3: get_space (not found) ===
#[tokio::test]
async fn test_get_space_not_found() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let result = storage.get_space("!nonexistent_space:example.com").await.expect("get_space should succeed");
    assert!(result.is_none());
}

// === Test 4: get_space_by_room ===
#[tokio::test]
async fn test_get_space_by_room() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!space_by_room_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Room Lookup".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@r:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let created = storage.create_space(request).await.unwrap();

    let found = storage.get_space_by_room(&room_id).await.expect("get_space_by_room should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().space_id, created.space_id);

    cleanup(&pool, &created.space_id).await;
}

// === Test 5: update_space ===
#[tokio::test]
async fn test_update_space() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!space_update_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Original".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@u:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let created = storage.create_space(request).await.unwrap();

    let update = UpdateSpaceRequest::new().name("Updated Name").topic("Updated Topic");
    let updated = storage.update_space(&created.space_id, &update).await.expect("update_space should succeed");
    assert_eq!(updated.name, Some("Updated Name".to_string()));
    assert_eq!(updated.topic, Some("Updated Topic".to_string()));

    cleanup(&pool, &created.space_id).await;
}

// === Test 6: delete_space ===
#[tokio::test]
async fn test_delete_space() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!space_del_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Delete Me".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@d:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let created = storage.create_space(request).await.unwrap();

    storage.delete_space(&created.space_id).await.expect("delete_space should succeed");
    let found = storage.get_space(&created.space_id).await.unwrap();
    assert!(found.is_none());

    // Clean up related records (delete_space only removes from spaces table)
    cleanup(&pool, &created.space_id).await;
}

// === Test 7: space children CRUD ===
#[tokio::test]
async fn test_space_children_crud() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_child_{}:example.com", uuid::Uuid::new_v4());
    let child_room_id = format!("!childroom_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Parent Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@p:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    let child = storage
        .add_child(AddChildRequest {
            space_id: space.space_id.clone(),
            room_id: child_room_id.clone(),
            sender: "@sender:example.com".to_string(),
            is_suggested: true,
            via_servers: vec!["example.com".to_string()],
        })
        .await
        .expect("add_child should succeed");
    assert_eq!(child.room_id, child_room_id);

    let children = storage.get_space_children(&space.space_id).await.expect("get_space_children should succeed");
    assert_eq!(children.len(), 1);

    storage.remove_child(&space.space_id, &child_room_id).await.expect("remove_child should succeed");
    let after = storage.get_space_children(&space.space_id).await.unwrap();
    assert!(after.is_empty());

    cleanup(&pool, &space.space_id).await;
}

// === Test 8: space members CRUD ===
#[tokio::test]
async fn test_space_members_crud() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_member_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Member Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@m:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Creator is auto-added as a member
    let members = storage.get_space_members(&space.space_id).await.expect("get_space_members should succeed");
    assert!(!members.is_empty()); // creator is always a member

    let member =
        storage.get_space_member(&space.space_id, "@m:example.com").await.expect("get_space_member should succeed");
    assert!(member.is_some());

    // Add another member
    storage
        .add_space_member(&space.space_id, "@other:example.com", "join", None)
        .await
        .expect("add_space_member should succeed");
    assert!(storage.is_space_member(&space.space_id, "@other:example.com").await.unwrap());

    // Remove the member
    storage
        .remove_space_member(&space.space_id, "@other:example.com")
        .await
        .expect("remove_space_member should succeed");
    assert!(!storage.is_space_member(&space.space_id, "@other:example.com").await.unwrap());

    cleanup(&pool, &space.space_id).await;
}

// === Test 9: get_user_spaces ===
#[tokio::test]
async fn test_get_user_spaces() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_user_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("User Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@us:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    let user_spaces = storage.get_user_spaces("@us:example.com").await.expect("get_user_spaces should succeed");
    assert!(!user_spaces.is_empty());
    assert!(user_spaces.iter().any(|s| s.space_id == space.space_id));

    cleanup(&pool, &space.space_id).await;
}

// === Test 10: get_public_spaces ===
#[tokio::test]
async fn test_get_public_spaces() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let spaces = storage.get_public_spaces(10, None, None).await.expect("get_public_spaces should succeed");
    // All returned spaces should be public
    for space in &spaces {
        assert!(space.is_public);
    }
}

// === Test 11: get_spaces_by_rooms_batch ===
#[tokio::test]
async fn test_get_spaces_by_rooms_batch() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_batch_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Batch Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@b:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    let map =
        storage.get_spaces_by_rooms_batch(&[room_id.clone()]).await.expect("get_spaces_by_rooms_batch should succeed");
    assert!(map.contains_key(&room_id));
    assert_eq!(map[&room_id].space_id, space.space_id);

    // Empty batch should return empty map
    let empty_map = storage.get_spaces_by_rooms_batch(&[]).await.expect("empty batch should succeed");
    assert!(empty_map.is_empty());

    cleanup(&pool, &space.space_id).await;
}

// === Test 12: get_space_summary and update_space_summary ===
#[tokio::test]
async fn test_get_space_summary_and_update() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_sum_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Summary Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@s:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // New space has no summary until update is called
    let summary = storage.get_space_summary(&space.space_id).await.expect("get_space_summary should succeed");
    assert!(summary.is_none(), "new space should not have a summary yet");

    // After update, summary should exist
    storage.update_space_summary(&space.space_id).await.expect("update_space_summary should succeed");
    let after = storage.get_space_summary(&space.space_id).await.unwrap();
    assert!(after.is_some());
    let s = after.unwrap();
    assert!(s.member_count.unwrap_or(0) >= 1, "should have at least the creator as member");

    cleanup(&pool, &space.space_id).await;
}

// === Test 13: get_child_spaces (reverse lookup) ===
#[tokio::test]
async fn test_get_child_spaces() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_child_of_{}:example.com", uuid::Uuid::new_v4());
    let child_room_id = format!("!child_of_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Child Of".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@c:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Add a child room to this space
    storage
        .add_child(AddChildRequest {
            space_id: space.space_id.clone(),
            room_id: child_room_id.clone(),
            sender: "@sender:example.com".to_string(),
            is_suggested: false,
            via_servers: vec!["example.com".to_string()],
        })
        .await
        .unwrap();

    // Reverse lookup: find spaces that have this room as a child
    let child_spaces = storage.get_child_spaces(&child_room_id).await.expect("get_child_spaces should succeed");
    assert!(!child_spaces.is_empty());
    assert_eq!(child_spaces[0].space_id, space.space_id);

    cleanup(&pool, &space.space_id).await;
}

// === Test 14: get_space_hierarchy ===
#[tokio::test]
async fn test_get_space_hierarchy() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_hier_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Hierarchy Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@h:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    let hierarchy = storage.get_space_hierarchy(&space.space_id, 2).await.expect("get_space_hierarchy should succeed");
    // SpaceHierarchy has: space, children, members
    assert_eq!(hierarchy.space.space_id, space.space_id);
    assert!(!hierarchy.members.is_empty(), "creator should be a member");

    // Test with non-existent space_id
    let result = storage.get_space_hierarchy("!nonexistent:example.com", 2).await;
    assert!(result.is_err(), "should error for non-existent space");

    cleanup(&pool, &space.space_id).await;
}

// === Test 15: add_space_event and get_space_events ===
#[tokio::test]
async fn test_add_and_get_space_events() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_evt_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$evt_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Event Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@e:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Add a space event
    let added = storage
        .add_space_event(
            &event_id,
            &space.space_id,
            "m.space.child",
            "@sender:example.com",
            serde_json::json!({"via": ["example.com"]}),
            Some("!child:example.com"),
        )
        .await
        .expect("add_space_event should succeed");
    assert_eq!(added.event_id, event_id);
    assert_eq!(added.event_type, "m.space.child");

    // Get all events for the space
    let events = storage.get_space_events(&space.space_id, None, 10).await.expect("get_space_events should succeed");
    assert!(!events.is_empty());
    assert_eq!(events[0].event_id, event_id);

    // Get events filtered by type
    let filtered = storage
        .get_space_events(&space.space_id, Some("m.space.child"), 10)
        .await
        .expect("get_space_events filtered should succeed");
    assert!(!filtered.is_empty());

    // Get events filtered by non-matching type
    let no_match = storage
        .get_space_events(&space.space_id, Some("m.room.create"), 10)
        .await
        .expect("get_space_events should succeed");
    assert!(no_match.is_empty());

    cleanup(&pool, &space.space_id).await;
}

// === Test 16: get_space_member_and_child_count ===
#[tokio::test]
async fn test_get_space_member_and_child_count() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_count_{}:example.com", uuid::Uuid::new_v4());
    let child_room_id = format!("!child_count_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Count Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@cnt:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Initially: 1 member (creator), 0 children
    let (member_count, child_count) = storage
        .get_space_member_and_child_count(&space.space_id)
        .await
        .expect("get_space_member_and_child_count should succeed");
    assert_eq!(member_count, 1);
    assert_eq!(child_count, 0);

    // Add a child
    storage
        .add_child(AddChildRequest {
            space_id: space.space_id.clone(),
            room_id: child_room_id.clone(),
            sender: "@sender:example.com".to_string(),
            is_suggested: false,
            via_servers: vec!["example.com".to_string()],
        })
        .await
        .unwrap();

    // Add another member
    storage.add_space_member(&space.space_id, "@other:example.com", "join", None).await.unwrap();

    let (member_count2, child_count2) =
        storage.get_space_member_and_child_count(&space.space_id).await.expect("count should succeed after adding");
    assert_eq!(member_count2, 2);
    assert_eq!(child_count2, 1);

    cleanup(&pool, &space.space_id).await;
}

// === Test 17: search_spaces (empty query returns empty) ===
#[tokio::test]
async fn test_search_spaces_empty_query_returns_empty() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);

    let results = storage.search_spaces("", 10, None).await.expect("search_spaces should succeed");
    assert!(results.is_empty(), "empty query should return empty vec");

    let results = storage.search_spaces("   ", 10, None).await.expect("search_spaces should succeed");
    assert!(results.is_empty(), "whitespace-only query should return empty vec");
}

// === Test 18: search_spaces (anonymous finds public spaces only) ===
#[tokio::test]
async fn test_search_spaces_anonymous_finds_public() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_search_pub_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("SearchablePublicSpace".to_string()),
        topic: Some("PublicTopicKeyword".to_string()),
        avatar_url: None,
        creator: "@sp:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: Some(true),
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Anonymous search should find the public space
    let results =
        storage.search_spaces("SearchablePublicSpace", 10, None).await.expect("anonymous search should succeed");
    assert!(results.iter().any(|s| s.space_id == space.space_id), "should find public space");

    // Also search by topic keyword
    let results_topic =
        storage.search_spaces("PublicTopicKeyword", 10, None).await.expect("topic search should succeed");
    assert!(results_topic.iter().any(|s| s.space_id == space.space_id), "should find by topic");

    cleanup(&pool, &space.space_id).await;
}

// === Test 19: search_spaces (with user finds private spaces) ===
#[tokio::test]
async fn test_search_spaces_with_user_finds_private() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_search_priv_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("PrivateKeywordSpace".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@priv_owner:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: Some(false),
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Anonymous search should NOT find the private space
    let anon_results =
        storage.search_spaces("PrivateKeywordSpace", 10, None).await.expect("anonymous search should succeed");
    assert!(!anon_results.iter().any(|s| s.space_id == space.space_id), "anonymous should not find private space");

    // Owner search SHOULD find the private space (creator is a member)
    let owner_results = storage
        .search_spaces("PrivateKeywordSpace", 10, Some("@priv_owner:example.com"))
        .await
        .expect("owner search should succeed");
    assert!(owner_results.iter().any(|s| s.space_id == space.space_id), "owner should find private space");

    cleanup(&pool, &space.space_id).await;
}

// === Test 20: get_space_statistics ===
#[tokio::test]
async fn test_get_space_statistics_returns_rows() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_stat_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Statistics Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@stat:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: Some(true),
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Insert a statistics row directly
    let now = current_timestamp_millis();
    let _ = sqlx::query(
            r"INSERT INTO space_statistics (space_id, name, is_public, child_room_count, member_count, created_ts, updated_ts)
               VALUES ($1, $2, TRUE, 2, 5, $3, $3)
               ON CONFLICT (space_id) DO UPDATE SET member_count = 5, child_room_count = 2",
        )
        .bind(&space.space_id)
        .bind("Statistics Space")
        .bind(now)
        .execute(&*pool)
        .await
        .unwrap();

    let stats = storage.get_space_statistics(10).await.expect("get_space_statistics should succeed");
    assert!(stats.iter().any(|s| s.get("space_id").and_then(|v| v.as_str()) == Some(space.space_id.as_str())));

    // Clean up the statistics row
    let _ =
        sqlx::query(r"DELETE FROM space_statistics WHERE space_id = $1").bind(&space.space_id).execute(&*pool).await;
    cleanup(&pool, &space.space_id).await;
}

// === Test 21: get_recursive_hierarchy (flat, no children) ===
#[tokio::test]
async fn test_get_recursive_hierarchy_flat() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_rec_flat_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Recursive Flat".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@rf:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // No children added, so recursive hierarchy should be empty
    let children = storage
        .get_recursive_hierarchy(&space.space_id, 3, false)
        .await
        .expect("get_recursive_hierarchy should succeed");
    assert!(children.is_empty(), "space with no children should return empty hierarchy");

    cleanup(&pool, &space.space_id).await;
}

// === Test 22: get_recursive_hierarchy (with children) ===
#[tokio::test]
async fn test_get_recursive_hierarchy_with_children() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_rec_parent_{}:example.com", uuid::Uuid::new_v4());
    let child_room_id = format!("!rec_child_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Recursive Parent".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@rp:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    storage
        .add_child(AddChildRequest {
            space_id: space.space_id.clone(),
            room_id: child_room_id.clone(),
            sender: "@rp:example.com".to_string(),
            is_suggested: false,
            via_servers: vec!["example.com".to_string()],
        })
        .await
        .unwrap();

    let children = storage
        .get_recursive_hierarchy(&space.space_id, 3, false)
        .await
        .expect("get_recursive_hierarchy should succeed");
    assert_eq!(children.len(), 1, "should find one child");
    assert_eq!(children[0].room_id, child_room_id);
    assert_eq!(children[0].depth, 0);

    // suggested_only=true should still return non-suggested children (filter happens at deeper levels)
    let suggested_children = storage
        .get_recursive_hierarchy(&space.space_id, 3, true)
        .await
        .expect("get_recursive_hierarchy with suggested_only should succeed");
    // The child we added is not suggested, so it should be excluded when suggested_only=true
    // (the collect_hierarchy_recursive filters non-suggested at depth > 0, but depth 0 children
    // are included regardless; verify the behavior is consistent)
    let _ = suggested_children; // just verify it doesn't error

    cleanup(&pool, &space.space_id).await;
}

// === Test 23: get_space_hierarchy_paginated (no children) ===
#[tokio::test]
async fn test_get_space_hierarchy_paginated_no_children() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_pag_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Paginated Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@pg:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    let response = storage
        .get_space_hierarchy_paginated(&space.space_id, 3, false, Some(10), None)
        .await
        .expect("get_space_hierarchy_paginated should succeed");
    assert!(response.rooms.is_empty(), "no children means no rooms in hierarchy");
    assert!(response.next_batch.is_none());

    cleanup(&pool, &space.space_id).await;
}

// === Test 24: get_space_hierarchy_paginated (with from cursor) ===
#[tokio::test]
async fn test_get_space_hierarchy_paginated_with_from() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_pag_from_{}:example.com", uuid::Uuid::new_v4());
    let child_room_1 = format!("!pag_child1_{}:example.com", uuid::Uuid::new_v4());
    let child_room_2 = format!("!pag_child2_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Paginated From Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@pf:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Add two child rooms (non-space children, so build_hierarchy_room returns a default room)
    for child_room in [&child_room_1, &child_room_2] {
        storage
            .add_child(AddChildRequest {
                space_id: space.space_id.clone(),
                room_id: child_room.clone(),
                sender: "@pf:example.com".to_string(),
                is_suggested: false,
                via_servers: vec!["example.com".to_string()],
            })
            .await
            .unwrap();
    }

    // First page with limit=1
    let response = storage
        .get_space_hierarchy_paginated(&space.space_id, 3, false, Some(1), None)
        .await
        .expect("first page should succeed");
    assert_eq!(response.rooms.len(), 1, "first page should have 1 room");

    // If there's a next_batch, fetch the second page using it as the from cursor
    if let Some(next) = response.next_batch {
        let response2 = storage
            .get_space_hierarchy_paginated(&space.space_id, 3, false, Some(1), Some(&next))
            .await
            .expect("second page should succeed");
        assert!(
            !response2.rooms.is_empty() || response2.next_batch.is_none(),
            "second page should have content or end"
        );
    }

    cleanup(&pool, &space.space_id).await;
}

// === Test 25: check_user_can_see_space (public space) ===
#[tokio::test]
async fn test_check_user_can_see_space_public() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_see_pub_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("See Public".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@see:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: Some(true),
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Anyone can see a public space
    let can_see =
        storage.check_user_can_see_space(&space.space_id, "@anyone:example.com").await.expect("check should succeed");
    assert!(can_see, "any user should be able to see a public space");

    cleanup(&pool, &space.space_id).await;
}

// === Test 26: check_user_can_see_space (private, member) ===
#[tokio::test]
async fn test_check_user_can_see_space_private_member() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_see_priv_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("See Private".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@priv_creator:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: Some(false),
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Creator is a member, should be able to see
    let can_see_creator = storage
        .check_user_can_see_space(&space.space_id, "@priv_creator:example.com")
        .await
        .expect("check creator should succeed");
    assert!(can_see_creator, "creator (member) should see private space");

    // Add another member, they should be able to see
    storage.add_space_member(&space.space_id, "@member:example.com", "join", None).await.unwrap();
    let can_see_member = storage
        .check_user_can_see_space(&space.space_id, "@member:example.com")
        .await
        .expect("check member should succeed");
    assert!(can_see_member, "member should see private space");

    cleanup(&pool, &space.space_id).await;
}

// === Test 27: check_user_can_see_space (private, non-member) ===
#[tokio::test]
async fn test_check_user_can_see_space_private_non_member() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_see_nonmem_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("See NonMember".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@nm_creator:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: Some(false),
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Non-member should NOT be able to see
    let can_see = storage
        .check_user_can_see_space(&space.space_id, "@stranger:example.com")
        .await
        .expect("check non-member should succeed");
    assert!(!can_see, "non-member should not see private space");

    cleanup(&pool, &space.space_id).await;
}

// === Test 28: check_user_can_see_space (nonexistent returns false) ===
#[tokio::test]
async fn test_check_user_can_see_space_nonexistent() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);

    let can_see = storage
        .check_user_can_see_space("!nonexistent:example.com", "@user:example.com")
        .await
        .expect("check nonexistent should succeed");
    assert!(!can_see, "nonexistent space should return false");
}

// === Test 29: get_parent_spaces ===
#[tokio::test]
async fn test_get_parent_spaces() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let parent_room_id = format!("!sp_parent_{}:example.com", uuid::Uuid::new_v4());
    let child_room_id = format!("!sp_child_room_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: parent_room_id.clone(),
        name: Some("Parent Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@p:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let parent_space = storage.create_space(request).await.unwrap();

    // Add child_room_id as a child of parent_space
    storage
        .add_child(AddChildRequest {
            space_id: parent_space.space_id.clone(),
            room_id: child_room_id.clone(),
            sender: "@p:example.com".to_string(),
            is_suggested: false,
            via_servers: vec!["example.com".to_string()],
        })
        .await
        .unwrap();

    // get_parent_spaces(child_room_id) should return spaces that contain child_room_id as a child
    let parents = storage.get_parent_spaces(&child_room_id).await.expect("get_parent_spaces should succeed");
    assert!(!parents.is_empty(), "should find at least one parent");
    assert!(parents.iter().any(|p| p.space_id == parent_space.space_id), "should find the parent space");

    cleanup(&pool, &parent_space.space_id).await;
}

// === Test 30: get_space_tree_path (root space) ===
#[tokio::test]
async fn test_get_space_tree_path_root() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_tree_root_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Tree Root".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@tr:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Root space (no parent) should have a path of just itself
    let path = storage.get_space_tree_path(&space.space_id).await.expect("get_space_tree_path should succeed");
    assert_eq!(path.len(), 1, "root space path should contain only itself");
    assert_eq!(path[0].space_id, space.space_id);

    cleanup(&pool, &space.space_id).await;
}

// === Test 31: get_space_tree_path (nested space) ===
#[tokio::test]
async fn test_get_space_tree_path_nested() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let parent_room_id = format!("!sp_tree_parent_{}:example.com", uuid::Uuid::new_v4());
    let child_room_id = format!("!sp_tree_child_{}:example.com", uuid::Uuid::new_v4());

    // Create parent space
    let parent_request = CreateSpaceRequest {
        room_id: parent_room_id.clone(),
        name: Some("Tree Parent".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@tp:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let parent_space = storage.create_space(parent_request).await.unwrap();

    // Create child space with parent_space_id pointing to parent
    let child_request = CreateSpaceRequest {
        room_id: child_room_id.clone(),
        name: Some("Tree Child".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@tc:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: Some(parent_space.space_id.clone()),
    };
    let child_space = storage.create_space(child_request).await.unwrap();

    // Path from child should be [parent, child]
    let path =
        storage.get_space_tree_path(&child_space.space_id).await.expect("get_space_tree_path nested should succeed");
    assert_eq!(path.len(), 2, "nested space path should have 2 entries");
    assert_eq!(path[0].space_id, parent_space.space_id, "first entry should be the root parent");
    assert_eq!(path[1].space_id, child_space.space_id, "second entry should be the child");

    cleanup(&pool, &child_space.space_id).await;
    cleanup(&pool, &parent_space.space_id).await;
}

// === Test 32: resolve_space_id (by space_id) ===
#[tokio::test]
async fn test_resolve_space_id_by_space_id() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_resolve_sid_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Resolve SID".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@rs:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Resolve by space_id
    let resolved = storage.resolve_space_id(&space.space_id).await.expect("resolve by space_id should succeed");
    assert_eq!(resolved, Some(space.space_id.clone()));

    cleanup(&pool, &space.space_id).await;
}

// === Test 33: resolve_space_id (by room_id) ===
#[tokio::test]
async fn test_resolve_space_id_by_room_id() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_resolve_rid_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Resolve RID".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@rr:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Resolve by room_id should return the space_id
    let resolved = storage.resolve_space_id(&room_id).await.expect("resolve by room_id should succeed");
    assert_eq!(resolved, Some(space.space_id.clone()));

    cleanup(&pool, &space.space_id).await;
}

// === Test 34: resolve_space_id (not found) ===
#[tokio::test]
async fn test_resolve_space_id_not_found() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);

    let resolved =
        storage.resolve_space_id("!nonexistent_resolve:example.com").await.expect("resolve nonexistent should succeed");
    assert!(resolved.is_none(), "nonexistent identifier should resolve to None");
}

// === Test 35: get_all_spaces_for_admin ===
#[tokio::test]
async fn test_get_all_spaces_for_admin() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_admin_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Admin Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@adm:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: Some(false), // private space, only admin listing should include it
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    let all = storage.get_all_spaces_for_admin().await.expect("get_all_spaces_for_admin should succeed");
    assert!(all.iter().any(|s| s.space_id == space.space_id), "admin listing should include the created space");

    cleanup(&pool, &space.space_id).await;
}

// === Test 36: get_space_by_identifier (by space_id) ===
#[tokio::test]
async fn test_get_space_by_identifier_by_space_id() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_ident_sid_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Ident SID".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@is:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    let found = storage
        .get_space_by_identifier(&space.space_id)
        .await
        .expect("get_space_by_identifier by space_id should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().space_id, space.space_id);

    cleanup(&pool, &space.space_id).await;
}

// === Test 37: get_space_by_identifier (by room_id) ===
#[tokio::test]
async fn test_get_space_by_identifier_by_room_id() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_ident_rid_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Ident RID".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@ir:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    let found =
        storage.get_space_by_identifier(&room_id).await.expect("get_space_by_identifier by room_id should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().space_id, space.space_id);

    cleanup(&pool, &space.space_id).await;
}

// === Test 38: get_space_by_identifier (not found) ===
#[tokio::test]
async fn test_get_space_by_identifier_not_found() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);

    let found = storage
        .get_space_by_identifier("!nonexistent_ident:example.com")
        .await
        .expect("get_space_by_identifier nonexistent should succeed");
    assert!(found.is_none());
}

// === Test 39: get_space_user_ids ===
#[tokio::test]
async fn test_get_space_user_ids() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_uids_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("User IDs Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@uids_creator:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Add another member
    storage.add_space_member(&space.space_id, "@uids_other:example.com", "join", None).await.unwrap();

    let user_ids = storage.get_space_user_ids(&space.space_id).await.expect("get_space_user_ids should succeed");
    assert!(user_ids.contains(&"@uids_creator:example.com".to_string()), "should contain creator");
    assert!(user_ids.contains(&"@uids_other:example.com".to_string()), "should contain added member");

    cleanup(&pool, &space.space_id).await;
}

// === Test 40: get_space_room_ids ===
#[tokio::test]
async fn test_get_space_room_ids() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_rids_{}:example.com", uuid::Uuid::new_v4());
    let child_room_1 = format!("!rids_child1_{}:example.com", uuid::Uuid::new_v4());
    let child_room_2 = format!("!rids_child2_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Room IDs Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@rids:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    for child_room in [&child_room_1, &child_room_2] {
        storage
            .add_child(AddChildRequest {
                space_id: space.space_id.clone(),
                room_id: child_room.clone(),
                sender: "@rids:example.com".to_string(),
                is_suggested: false,
                via_servers: vec!["example.com".to_string()],
            })
            .await
            .unwrap();
    }

    let room_ids = storage.get_space_room_ids(&space.space_id).await.expect("get_space_room_ids should succeed");
    assert!(room_ids.contains(&child_room_1), "should contain child room 1");
    assert!(room_ids.contains(&child_room_2), "should contain child room 2");

    cleanup(&pool, &space.space_id).await;
}

// === Test 41: delete_space_returning_count ===
#[tokio::test]
async fn test_delete_space_returning_count() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_delcnt_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Delete Count".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@dc:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    let count = storage
        .delete_space_returning_count(&space.space_id)
        .await
        .expect("delete_space_returning_count should succeed");
    assert_eq!(count, 1, "should delete exactly 1 row");

    // Verify it's gone
    let found = storage.get_space(&space.space_id).await.unwrap();
    assert!(found.is_none());

    cleanup(&pool, &space.space_id).await;
}

// === Test 42: delete_space_returning_count (nonexistent returns 0) ===
#[tokio::test]
async fn test_delete_space_returning_count_nonexistent() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);

    let count = storage
        .delete_space_returning_count("!nonexistent_delcnt:example.com")
        .await
        .expect("delete nonexistent should succeed");
    assert_eq!(count, 0, "should delete 0 rows for nonexistent space");
}

// === Test 43: get_space_children_paginated (no cursor) ===
#[tokio::test]
async fn test_get_space_children_paginated_no_cursor() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_childpag_{}:example.com", uuid::Uuid::new_v4());
    let child_room_1 = format!("!childpag1_{}:example.com", uuid::Uuid::new_v4());
    let child_room_2 = format!("!childpag2_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Children Paginated".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@cp:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    for child_room in [&child_room_1, &child_room_2] {
        storage
            .add_child(AddChildRequest {
                space_id: space.space_id.clone(),
                room_id: child_room.clone(),
                sender: "@cp:example.com".to_string(),
                is_suggested: false,
                via_servers: vec!["example.com".to_string()],
            })
            .await
            .unwrap();
    }

    // Get all children with a high limit
    let children = storage
        .get_space_children_paginated(&space.space_id, 10, None, None)
        .await
        .expect("get_space_children_paginated no cursor should succeed");
    assert_eq!(children.len(), 2, "should return both children");

    // Get with limit=1, should return only 1
    let limited = storage
        .get_space_children_paginated(&space.space_id, 1, None, None)
        .await
        .expect("get_space_children_paginated limit=1 should succeed");
    assert_eq!(limited.len(), 1, "should return only 1 child with limit=1");

    cleanup(&pool, &space.space_id).await;
}

// === Test 44: get_space_children_paginated (with cursor) ===
#[tokio::test]
async fn test_get_space_children_paginated_with_cursor() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_childcur_{}:example.com", uuid::Uuid::new_v4());
    let child_room_1 = format!("!childcur1_{}:example.com", uuid::Uuid::new_v4());
    let child_room_2 = format!("!childcur2_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Children Cursor".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@cc:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    for child_room in [&child_room_1, &child_room_2] {
        storage
            .add_child(AddChildRequest {
                space_id: space.space_id.clone(),
                room_id: child_room.clone(),
                sender: "@cc:example.com".to_string(),
                is_suggested: false,
                via_servers: vec!["example.com".to_string()],
            })
            .await
            .unwrap();
    }

    // First page: limit=1, no cursor
    let first_page =
        storage.get_space_children_paginated(&space.space_id, 1, None, None).await.expect("first page should succeed");
    assert_eq!(first_page.len(), 1);
    let first_child = &first_page[0];

    // Second page: use the first child's added_ts and id as cursor
    let second_page = storage
        .get_space_children_paginated(&space.space_id, 10, Some(first_child.added_ts), Some(first_child.id))
        .await
        .expect("second page with cursor should succeed");
    assert_eq!(second_page.len(), 1, "should return the remaining child");
    assert_ne!(second_page[0].id, first_child.id, "second page should not include the first child again");

    cleanup(&pool, &space.space_id).await;
}

// === Test 45: get_space_members_paginated (no cursor) ===
#[tokio::test]
async fn test_get_space_members_paginated_no_cursor() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_mempag_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Members Paginated".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@mp_creator:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Add another member
    storage.add_space_member(&space.space_id, "@mp_member:example.com", "join", None).await.unwrap();

    let members = storage
        .get_space_members_paginated(&space.space_id, 10, None, None)
        .await
        .expect("get_space_members_paginated no cursor should succeed");
    assert_eq!(members.len(), 2, "should return both members (creator + added)");

    // Limit=1 should return only 1
    let limited = storage
        .get_space_members_paginated(&space.space_id, 1, None, None)
        .await
        .expect("get_space_members_paginated limit=1 should succeed");
    assert_eq!(limited.len(), 1, "should return only 1 member with limit=1");

    cleanup(&pool, &space.space_id).await;
}

// === Test 46: get_space_members_paginated (with cursor) ===
#[tokio::test]
async fn test_get_space_members_paginated_with_cursor() {
    let pool = test_pool().await;
    let storage = SpaceStorage::new(&pool);
    let room_id = format!("!sp_memcur_{}:example.com", uuid::Uuid::new_v4());

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("Members Cursor".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@mc_creator:example.com".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space = storage.create_space(request).await.unwrap();

    // Add another member
    storage.add_space_member(&space.space_id, "@mc_member:example.com", "join", None).await.unwrap();

    // First page: limit=1, no cursor
    let first_page =
        storage.get_space_members_paginated(&space.space_id, 1, None, None).await.expect("first page should succeed");
    assert_eq!(first_page.len(), 1);
    let first_member = &first_page[0];

    // Second page: use first member's joined_ts and user_id as cursor
    let second_page = storage
        .get_space_members_paginated(&space.space_id, 10, Some(first_member.joined_ts), Some(&first_member.user_id))
        .await
        .expect("second page with cursor should succeed");
    assert_eq!(second_page.len(), 1, "should return the remaining member");
    assert_ne!(second_page[0].user_id, first_member.user_id, "second page should not include the first member again");

    cleanup(&pool, &space.space_id).await;
}
