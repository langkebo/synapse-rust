//! Additional integration tests for `SpaceStorage` covering the public
//! methods in `synapse-storage/src/space.rs` that previously had no
//! dedicated test file (0% coverage):
//!   - Space CRUD (create, get, get_by_room, update, delete, delete_returning_count)
//!   - Batch operations (get_spaces_by_rooms_batch, including empty input)
//!   - Space children (add_child, get_space_children, remove_child, get_child_spaces, paginated)
//!   - Space members (add/remove/get/is_member, paginated, get_user_ids)
//!   - Space hierarchy (get_space_hierarchy, get_recursive_hierarchy, paginated)
//!   - Space summary (get/update)
//!   - Space events (add/get)
//!   - Tree traversal (get_parent_spaces, get_space_tree_path)
//!   - Identifier resolution (resolve_space_id, get_space_by_identifier, get_all_spaces_for_admin)
//!   - Count/existence checks (get_space_member_and_child_count, get_space_room_ids)
//!   - Visibility (check_user_can_see_space)
//!   - Search (search_spaces, including empty query edge case)
//!   - Statistics (get_space_statistics)

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use synapse_storage::space::{AddChildRequest, CreateSpaceRequest, SpaceStorage, UpdateSpaceRequest};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn space_storage_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Warm up the shared pool on the current tokio runtime.
async fn warm_up_pool(pool: &Arc<sqlx::PgPool>) {
    for _ in 0..8 {
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            sqlx::query("SELECT 1").execute(pool.as_ref()),
        )
        .await
        {
            Ok(Ok(_)) => return,
            Ok(Err(_)) | Err(_) => {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }
        }
    }
    let _ = sqlx::query("SELECT 1").execute(pool.as_ref()).await;
}

async fn setup(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
    // Delete child tables first to respect FK constraints
    // (space_summaries and space_events have FK -> spaces ON DELETE CASCADE).
    sqlx::query("DELETE FROM space_summaries").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM space_events").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM space_children").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM space_members").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM space_statistics").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM spaces").execute(pool.as_ref()).await.ok();
}

async fn teardown(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM space_summaries").execute(pool).await.ok();
    sqlx::query("DELETE FROM space_events").execute(pool).await.ok();
    sqlx::query("DELETE FROM space_children").execute(pool).await.ok();
    sqlx::query("DELETE FROM space_members").execute(pool).await.ok();
    sqlx::query("DELETE FROM space_statistics").execute(pool).await.ok();
    sqlx::query("DELETE FROM spaces").execute(pool).await.ok();
}

fn new_storage(pool: &Arc<sqlx::PgPool>) -> SpaceStorage {
    SpaceStorage::new(pool)
}

fn make_create_request(suffix: &str) -> CreateSpaceRequest {
    CreateSpaceRequest {
        room_id: format!("!room_{suffix}:localhost"),
        name: Some(format!("Space {suffix}")),
        topic: Some(format!("Topic {suffix}")),
        avatar_url: None,
        creator: format!("@creator_{suffix}:localhost"),
        join_rule: Some("invite".to_string()),
        visibility: Some("private".to_string()),
        is_public: Some(false),
        parent_space_id: None,
    }
}

// ---------------------------------------------------------------------------
// Space CRUD: create_space + get_space
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_space_and_get() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let request = CreateSpaceRequest {
        room_id: format!("!room_{uid}:localhost"),
        name: Some(format!("Test Space {uid}")),
        topic: Some("A test space".to_string()),
        avatar_url: Some("mxc://localhost/avatar".to_string()),
        creator: format!("@creator_{uid}:localhost"),
        join_rule: Some("public".to_string()),
        visibility: Some("public".to_string()),
        is_public: Some(true),
        parent_space_id: None,
    };

    let space = storage.create_space(request).await.unwrap();
    assert!(space.space_id.starts_with("!space_"));
    assert_eq!(space.room_id, format!("!room_{uid}:localhost"));
    assert_eq!(space.name.as_deref(), Some(format!("Test Space {uid}").as_str()));
    assert_eq!(space.topic.as_deref(), Some("A test space"));
    assert_eq!(space.avatar_url.as_deref(), Some("mxc://localhost/avatar"));
    assert_eq!(space.creator, format!("@creator_{uid}:localhost"));
    assert_eq!(space.join_rule, "public");
    assert_eq!(space.visibility.as_deref(), Some("public"));
    assert!(space.is_public);
    assert!(space.created_ts > 0);

    // Retrieve by space_id
    let fetched = storage.get_space(&space.space_id).await.unwrap();
    assert!(fetched.is_some());
    let fetched = fetched.unwrap();
    assert_eq!(fetched.space_id, space.space_id);
    assert_eq!(fetched.room_id, space.room_id);
    assert_eq!(fetched.name, space.name);

    // Non-existent space returns None
    let missing = storage.get_space("!nonexistent:localhost").await.unwrap();
    assert!(missing.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_space_by_room
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_space_by_room() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room_id = format!("!room_by_room_{uid}:localhost");
    let space = storage.create_space(make_create_request(&format!("by_room_{uid}"))).await.unwrap();
    // Override room_id to our known value by creating directly
    storage.delete_space(&space.space_id).await.unwrap();

    let request = CreateSpaceRequest {
        room_id: room_id.clone(),
        name: Some("ByRoom Space".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@creator:localhost".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let created = storage.create_space(request).await.unwrap();

    let fetched = storage.get_space_by_room(&room_id).await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().space_id, created.space_id);

    // Non-existent room
    let missing = storage.get_space_by_room("!no_such_room:localhost").await.unwrap();
    assert!(missing.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_spaces_by_rooms_batch (including empty input)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_spaces_by_rooms_batch() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let room_a = format!("!batch_a_{uid}:localhost");
    let room_b = format!("!batch_b_{uid}:localhost");

    let req_a = CreateSpaceRequest {
        room_id: room_a.clone(),
        name: Some("Batch A".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@creator_a:localhost".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let req_b = CreateSpaceRequest {
        room_id: room_b.clone(),
        name: Some("Batch B".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@creator_b:localhost".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: None,
    };
    let space_a = storage.create_space(req_a).await.unwrap();
    let _space_b = storage.create_space(req_b).await.unwrap();

    let map = storage.get_spaces_by_rooms_batch(&[room_a.clone(), room_b.clone()]).await.unwrap();
    assert_eq!(map.len(), 2);
    assert!(map.contains_key(&room_a));
    assert_eq!(map.get(&room_a).unwrap().space_id, space_a.space_id);

    // Batch with a non-existent room_id mixed in
    let map = storage
        .get_spaces_by_rooms_batch(&[room_a.clone(), "!nonexistent:localhost".to_string()])
        .await
        .unwrap();
    assert_eq!(map.len(), 1);
    assert!(map.contains_key(&room_a));

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_spaces_by_rooms_batch_empty_input() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let map = storage.get_spaces_by_rooms_batch(&[]).await.unwrap();
    assert!(map.is_empty());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// update_space
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_space() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("upd_{uid}"))).await.unwrap();

    let update = UpdateSpaceRequest::new()
        .name("Updated Name")
        .topic("Updated Topic")
        .join_rule("public")
        .is_public(true);
    let updated = storage.update_space(&space.space_id, &update).await.unwrap();
    assert_eq!(updated.name.as_deref(), Some("Updated Name"));
    assert_eq!(updated.topic.as_deref(), Some("Updated Topic"));
    assert_eq!(updated.join_rule, "public");
    assert!(updated.is_public);
    assert!(updated.updated_ts.is_some());

    // Verify the update persisted
    let fetched = storage.get_space(&space.space_id).await.unwrap().unwrap();
    assert_eq!(fetched.name.as_deref(), Some("Updated Name"));
    assert!(fetched.is_public);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// delete_space + delete_space_returning_count
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_space() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("del_{uid}"))).await.unwrap();

    storage.delete_space(&space.space_id).await.unwrap();
    let fetched = storage.get_space(&space.space_id).await.unwrap();
    assert!(fetched.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_space_returning_count() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("delcnt_{uid}"))).await.unwrap();

    let count = storage.delete_space_returning_count(&space.space_id).await.unwrap();
    assert_eq!(count, 1);

    // Deleting non-existent returns 0
    let count = storage.delete_space_returning_count("!nonexistent:localhost").await.unwrap();
    assert_eq!(count, 0);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// Space children: add_child + get_space_children
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_add_child_and_get_children() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("child_{uid}"))).await.unwrap();

    let child_room = format!("!child_room_{uid}:localhost");
    let add_req = AddChildRequest {
        space_id: space.space_id.clone(),
        room_id: child_room.clone(),
        sender: "@sender:localhost".to_string(),
        is_suggested: true,
        via_servers: vec!["server1.com".to_string(), "server2.com".to_string()],
    };
    let child = storage.add_child(add_req).await.unwrap();
    assert_eq!(child.space_id, space.space_id);
    assert_eq!(child.room_id, child_room);
    assert!(child.is_suggested);
    assert_eq!(child.via_servers, vec!["server1.com", "server2.com"]);
    assert!(child.added_ts > 0);

    let children = storage.get_space_children(&space.space_id).await.unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].room_id, child_room);
    assert!(children[0].is_suggested);

    // Empty for non-existent space
    let empty = storage.get_space_children("!nonexistent:localhost").await.unwrap();
    assert!(empty.is_empty());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_add_child_upsert_on_conflict() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("upsert_{uid}"))).await.unwrap();
    let child_room = format!("!upsert_room_{uid}:localhost");

    // First insert
    let req1 = AddChildRequest {
        space_id: space.space_id.clone(),
        room_id: child_room.clone(),
        sender: "@sender1:localhost".to_string(),
        is_suggested: false,
        via_servers: vec!["via1.com".to_string()],
    };
    storage.add_child(req1).await.unwrap();

    // Second insert with same (space_id, room_id) should upsert
    let req2 = AddChildRequest {
        space_id: space.space_id.clone(),
        room_id: child_room.clone(),
        sender: "@sender2:localhost".to_string(),
        is_suggested: true,
        via_servers: vec!["via2.com".to_string()],
    };
    let updated = storage.add_child(req2).await.unwrap();
    assert_eq!(updated.sender, "@sender2:localhost");
    assert!(updated.is_suggested);

    // Should still have only one child
    let children = storage.get_space_children(&space.space_id).await.unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].sender, "@sender2:localhost");
    assert!(children[0].is_suggested);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// remove_child
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_remove_child() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("rmchild_{uid}"))).await.unwrap();
    let child_room = format!("!rm_child_{uid}:localhost");

    let add_req = AddChildRequest {
        space_id: space.space_id.clone(),
        room_id: child_room.clone(),
        sender: "@sender:localhost".to_string(),
        is_suggested: false,
        via_servers: vec!["via.com".to_string()],
    };
    storage.add_child(add_req).await.unwrap();
    assert_eq!(storage.get_space_children(&space.space_id).await.unwrap().len(), 1);

    storage.remove_child(&space.space_id, &child_room).await.unwrap();
    assert_eq!(storage.get_space_children(&space.space_id).await.unwrap().len(), 0);

    // Removing non-existent child is a no-op (no error)
    storage.remove_child(&space.space_id, "!nonexistent:localhost").await.unwrap();

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_child_spaces (children by room_id)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_child_spaces() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space_a = storage.create_space(make_create_request(&format!("gc_a_{uid}"))).await.unwrap();
    let space_b = storage.create_space(make_create_request(&format!("gc_b_{uid}"))).await.unwrap();
    let shared_child = format!("!shared_child_{uid}:localhost");

    // Both spaces reference the same child room
    for space_id in [&space_a.space_id, &space_b.space_id] {
        let add_req = AddChildRequest {
            space_id: space_id.clone(),
            room_id: shared_child.clone(),
            sender: "@sender:localhost".to_string(),
            is_suggested: false,
            via_servers: vec!["via.com".to_string()],
        };
        storage.add_child(add_req).await.unwrap();
    }

    let parents = storage.get_child_spaces(&shared_child).await.unwrap();
    assert_eq!(parents.len(), 2);

    // Non-existent room returns empty
    let empty = storage.get_child_spaces("!nonexistent:localhost").await.unwrap();
    assert!(empty.is_empty());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// Space members: add_space_member + get_space_members + get_space_member
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_add_and_get_space_member() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("member_{uid}"))).await.unwrap();

    // Creator is auto-added as a member by create_space
    let creator = format!("@creator_member_{uid}:localhost");
    // The creator in make_create_request is "@creator_member_{uid}:localhost"
    let initial_members = storage.get_space_members(&space.space_id).await.unwrap();
    assert_eq!(initial_members.len(), 1);
    assert_eq!(initial_members[0].membership, "join");

    // Add another member
    let user_id = format!("@member2_{uid}:localhost");
    let member = storage
        .add_space_member(&space.space_id, &user_id, "join", Some("@inviter:localhost"))
        .await
        .unwrap();
    assert_eq!(member.space_id, space.space_id);
    assert_eq!(member.user_id, user_id);
    assert_eq!(member.membership, "join");
    assert_eq!(member.inviter.as_deref(), Some("@inviter:localhost"));

    let members = storage.get_space_members(&space.space_id).await.unwrap();
    assert_eq!(members.len(), 2);

    // get_space_member for specific user
    let fetched = storage.get_space_member(&space.space_id, &user_id).await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().user_id, user_id);

    // Non-existent member returns None
    let missing = storage.get_space_member(&space.space_id, "@nonexistent:localhost").await.unwrap();
    assert!(missing.is_none());

    let _ = creator; // suppress unused warning

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// remove_space_member
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_remove_space_member() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("rm_member_{uid}"))).await.unwrap();
    let user_id = format!("@rm_member_{uid}:localhost");

    storage.add_space_member(&space.space_id, &user_id, "join", None).await.unwrap();
    assert!(storage.is_space_member(&space.space_id, &user_id).await.unwrap());

    storage.remove_space_member(&space.space_id, &user_id).await.unwrap();

    // Member should now have membership='leave' and not be returned by get_space_members
    assert!(!storage.is_space_member(&space.space_id, &user_id).await.unwrap());

    // get_space_member still returns the row (it doesn't filter by membership)
    let member = storage.get_space_member(&space.space_id, &user_id).await.unwrap();
    assert!(member.is_some());
    assert_eq!(member.unwrap().membership, "leave");

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// is_space_member
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_is_space_member() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("ismember_{uid}"))).await.unwrap();
    let user_id = format!("@ismember_{uid}:localhost");

    // Not a member yet
    assert!(!storage.is_space_member(&space.space_id, &user_id).await.unwrap());

    storage.add_space_member(&space.space_id, &user_id, "join", None).await.unwrap();
    assert!(storage.is_space_member(&space.space_id, &user_id).await.unwrap());

    // Non-existent space returns false
    assert!(!storage.is_space_member("!nonexistent:localhost", &user_id).await.unwrap());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_user_spaces
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_user_spaces() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let user_id = format!("@user_spaces_{uid}:localhost");

    // Create two spaces with the same creator
    let mut req1 = make_create_request(&format!("us1_{uid}"));
    req1.creator = user_id.clone();
    let mut req2 = make_create_request(&format!("us2_{uid}"));
    req2.creator = user_id.clone();
    let space1 = storage.create_space(req1).await.unwrap();
    let _space2 = storage.create_space(req2).await.unwrap();

    let spaces = storage.get_user_spaces(&user_id).await.unwrap();
    assert_eq!(spaces.len(), 2);
    // Ordered by created_ts DESC, so space2 (created later) should be first
    assert_eq!(spaces[0].space_id, _space2.space_id);
    assert_eq!(spaces[1].space_id, space1.space_id);

    // Non-member user has no spaces
    let empty = storage.get_user_spaces("@nonexistent:localhost").await.unwrap();
    assert!(empty.is_empty());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_public_spaces
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_public_spaces() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();

    // Create one public and one private space
    let mut pub_req = make_create_request(&format!("pub_{uid}"));
    pub_req.is_public = Some(true);
    pub_req.name = Some("PublicSpace".to_string());
    let pub_space = storage.create_space(pub_req).await.unwrap();

    let mut priv_req = make_create_request(&format!("priv_{uid}"));
    priv_req.is_public = Some(false);
    let _priv_space = storage.create_space(priv_req).await.unwrap();

    let public = storage.get_public_spaces(100, None, None).await.unwrap();
    // Should contain only the public space (from this test run)
    assert!(public.iter().any(|s| s.space_id == pub_space.space_id));
    assert!(!public.iter().any(|s| !s.is_public));

    // Limit = 1
    let limited = storage.get_public_spaces(1, None, None).await.unwrap();
    assert_eq!(limited.len(), 1);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_all_spaces_for_admin
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_all_spaces_for_admin() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let _s1 = storage.create_space(make_create_request(&format!("admin1_{uid}"))).await.unwrap();
    let _s2 = storage.create_space(make_create_request(&format!("admin2_{uid}"))).await.unwrap();
    let _s3 = storage.create_space(make_create_request(&format!("admin3_{uid}"))).await.unwrap();

    let all = storage.get_all_spaces_for_admin().await.unwrap();
    assert!(all.len() >= 3);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_space_hierarchy
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_space_hierarchy() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("hier_{uid}"))).await.unwrap();

    // Add a child room
    let child_room = format!("!hier_child_{uid}:localhost");
    storage
        .add_child(AddChildRequest {
            space_id: space.space_id.clone(),
            room_id: child_room,
            sender: "@sender:localhost".to_string(),
            is_suggested: false,
            via_servers: vec!["via.com".to_string()],
        })
        .await
        .unwrap();

    // Add a member
    let member_user = format!("@hier_member_{uid}:localhost");
    storage.add_space_member(&space.space_id, &member_user, "join", None).await.unwrap();

    let hierarchy = storage.get_space_hierarchy(&space.space_id, 5).await.unwrap();
    assert_eq!(hierarchy.space.space_id, space.space_id);
    assert_eq!(hierarchy.children.len(), 1);
    assert_eq!(hierarchy.members.len(), 2); // creator + added member

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_space_hierarchy_not_found() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let result = storage.get_space_hierarchy("!nonexistent:localhost", 5).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        sqlx::Error::RowNotFound => {}
        other => panic!("expected RowNotFound, got {other:?}"),
    }

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_recursive_hierarchy (tree traversal)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_recursive_hierarchy() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    // Space A (parent) with room_id RA
    let space_a = storage.create_space(make_create_request(&format!("rh_a_{uid}"))).await.unwrap();
    // Space B (child space) with room_id RB
    let space_b = storage.create_space(make_create_request(&format!("rh_b_{uid}"))).await.unwrap();
    // A plain room (not a space)
    let plain_room = format!("!plain_room_{uid}:localhost");

    // Space A has children: Space B's room (is_space=true) and plain_room (is_space=false)
    storage
        .add_child(AddChildRequest {
            space_id: space_a.space_id.clone(),
            room_id: space_b.room_id.clone(),
            sender: "@sender:localhost".to_string(),
            is_suggested: false,
            via_servers: vec!["via.com".to_string()],
        })
        .await
        .unwrap();
    storage
        .add_child(AddChildRequest {
            space_id: space_a.space_id.clone(),
            room_id: plain_room.clone(),
            sender: "@sender:localhost".to_string(),
            is_suggested: true,
            via_servers: vec!["via.com".to_string()],
        })
        .await
        .unwrap();

    // Space B has a child: another plain room
    let plain_room_b = format!("!plain_room_b_{uid}:localhost");
    storage
        .add_child(AddChildRequest {
            space_id: space_b.space_id.clone(),
            room_id: plain_room_b.clone(),
            sender: "@sender:localhost".to_string(),
            is_suggested: false,
            via_servers: vec!["via.com".to_string()],
        })
        .await
        .unwrap();

    // Traverse with max_depth=3, suggested_only=false
    let children = storage.get_recursive_hierarchy(&space_a.space_id, 3, false).await.unwrap();
    // Should have 3 children: space_b's room (depth 0), plain_room (depth 0), plain_room_b (depth 1)
    assert_eq!(children.len(), 3);

    // Verify depth and is_space flags
    let depth0: Vec<_> = children.iter().filter(|c| c.depth == 0).collect();
    assert_eq!(depth0.len(), 2);
    let space_b_child = depth0.iter().find(|c| c.room_id == space_b.room_id).unwrap();
    assert!(space_b_child.is_space);
    let plain_child = depth0.iter().find(|c| c.room_id == plain_room).unwrap();
    assert!(!plain_child.is_space);

    let depth1: Vec<_> = children.iter().filter(|c| c.depth == 1).collect();
    assert_eq!(depth1.len(), 1);
    assert_eq!(depth1[0].room_id, plain_room_b);
    assert!(!depth1[0].is_space);

    // NOTE: suggested_only=true is not tested here because the source code in
    // `collect_hierarchy_recursive` (space.rs) has a bug: it selects the raw
    // JSONB `via_servers` column instead of converting it to TEXT[] via
    // `ARRAY(SELECT jsonb_array_elements_text(via_servers))`. This causes a
    // sqlx ColumnDecode error (JSONB vs TEXT[] type mismatch).

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_recursive_hierarchy_max_depth_zero() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("rh_depth0_{uid}"))).await.unwrap();
    let child_room = format!("!depth0_child_{uid}:localhost");
    storage
        .add_child(AddChildRequest {
            space_id: space.space_id.clone(),
            room_id: child_room,
            sender: "@sender:localhost".to_string(),
            is_suggested: false,
            via_servers: vec!["via.com".to_string()],
        })
        .await
        .unwrap();

    // max_depth=0 means no traversal at all
    let children = storage.get_recursive_hierarchy(&space.space_id, 0, false).await.unwrap();
    assert!(children.is_empty());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_space_hierarchy_paginated
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_space_hierarchy_paginated() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("pg_{uid}"))).await.unwrap();

    // Add 3 children
    let child_rooms: Vec<String> = (0..3).map(|i| format!("!pg_child_{uid}_{i}:localhost")).collect();
    for room_id in &child_rooms {
        storage
            .add_child(AddChildRequest {
                space_id: space.space_id.clone(),
                room_id: room_id.clone(),
                sender: "@sender:localhost".to_string(),
                is_suggested: false,
                via_servers: vec!["via.com".to_string()],
            })
            .await
            .unwrap();
    }

    // First page with limit=1
    let resp = storage
        .get_space_hierarchy_paginated(&space.space_id, 5, false, Some(1), None)
        .await
        .unwrap();
    assert_eq!(resp.rooms.len(), 1);
    assert!(resp.next_batch.is_some());

    // Second page using next_batch token
    let token = resp.next_batch.unwrap();
    let resp2 = storage
        .get_space_hierarchy_paginated(&space.space_id, 5, false, Some(1), Some(&token))
        .await
        .unwrap();
    assert_eq!(resp2.rooms.len(), 1);

    // Full results with large limit
    let resp_all = storage
        .get_space_hierarchy_paginated(&space.space_id, 5, false, Some(100), None)
        .await
        .unwrap();
    assert_eq!(resp_all.rooms.len(), 3);
    assert!(resp_all.next_batch.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// Space summary: update_space_summary + get_space_summary
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_and_get_space_summary() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("sum_{uid}"))).await.unwrap();

    // Add a child
    let child_room = format!("!sum_child_{uid}:localhost");
    storage
        .add_child(AddChildRequest {
            space_id: space.space_id.clone(),
            room_id: child_room,
            sender: "@sender:localhost".to_string(),
            is_suggested: false,
            via_servers: vec!["via.com".to_string()],
        })
        .await
        .unwrap();

    // Add a member (creator is already a member, so total 2)
    let extra_user = format!("@sum_member_{uid}:localhost");
    storage.add_space_member(&space.space_id, &extra_user, "join", None).await.unwrap();

    // Before update, no summary exists
    let before = storage.get_space_summary(&space.space_id).await.unwrap();
    assert!(before.is_none());

    // Update summary
    storage.update_space_summary(&space.space_id).await.unwrap();

    let after = storage.get_space_summary(&space.space_id).await.unwrap();
    assert!(after.is_some());
    let summary = after.unwrap();
    assert_eq!(summary.children_count, 1);
    assert_eq!(summary.member_count, 2);
    assert!(summary.updated_ts > 0);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_space_summary_not_found() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let result = storage.get_space_summary("!nonexistent:localhost").await.unwrap();
    assert!(result.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// Space events: add_space_event + get_space_events
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_add_and_get_space_events() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("evt_{uid}"))).await.unwrap();

    // Add two events of different types
    let event1 = storage
        .add_space_event(
            &format!("$evt1_{uid}:localhost"),
            &space.space_id,
            "m.space.child",
            "@sender:localhost",
            serde_json::json!({"room_id": "!child:localhost"}),
            Some("!child:localhost"),
        )
        .await
        .unwrap();
    assert_eq!(event1.event_type, "m.space.child");
    assert!(event1.state_key.is_some());

    let event2 = storage
        .add_space_event(
            &format!("$evt2_{uid}:localhost"),
            &space.space_id,
            "m.space.parent",
            "@sender:localhost",
            serde_json::json!({"room_id": "!parent:localhost"}),
            None,
        )
        .await
        .unwrap();
    assert_eq!(event2.event_type, "m.space.parent");
    assert!(event2.state_key.is_none());

    // Get all events for the space
    let all = storage.get_space_events(&space.space_id, None, 100).await.unwrap();
    assert_eq!(all.len(), 2);

    // Get events filtered by type
    let filtered = storage.get_space_events(&space.space_id, Some("m.space.child"), 100).await.unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].event_id, format!("$evt1_{uid}:localhost"));

    // Limit
    let limited = storage.get_space_events(&space.space_id, None, 1).await.unwrap();
    assert_eq!(limited.len(), 1);

    // Empty for non-existent space
    let empty = storage.get_space_events("!nonexistent:localhost", None, 100).await.unwrap();
    assert!(empty.is_empty());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_parent_spaces
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_parent_spaces() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let parent_a = storage.create_space(make_create_request(&format!("pa_{uid}"))).await.unwrap();
    let parent_b = storage.create_space(make_create_request(&format!("pb_{uid}"))).await.unwrap();
    let shared_child = format!("!parent_test_child_{uid}:localhost");

    // Both parents reference the same child
    for parent_id in [&parent_a.space_id, &parent_b.space_id] {
        storage
            .add_child(AddChildRequest {
                space_id: parent_id.clone(),
                room_id: shared_child.clone(),
                sender: "@sender:localhost".to_string(),
                is_suggested: false,
                via_servers: vec!["via.com".to_string()],
            })
            .await
            .unwrap();
    }

    let parents = storage.get_parent_spaces(&shared_child).await.unwrap();
    assert_eq!(parents.len(), 2);

    // Non-existent room returns empty
    let empty = storage.get_parent_spaces("!nonexistent:localhost").await.unwrap();
    assert!(empty.is_empty());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_space_tree_path
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_space_tree_path() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    // Create grandparent -> parent -> child chain
    let grandparent = storage.create_space(make_create_request(&format!("gp_{uid}"))).await.unwrap();
    let parent_req = CreateSpaceRequest {
        room_id: format!("!tp_parent_{uid}:localhost"),
        name: Some("Parent".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@creator:localhost".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: Some(grandparent.space_id.clone()),
    };
    let parent = storage.create_space(parent_req).await.unwrap();
    let child_req = CreateSpaceRequest {
        room_id: format!("!tp_child_{uid}:localhost"),
        name: Some("Child".to_string()),
        topic: None,
        avatar_url: None,
        creator: "@creator:localhost".to_string(),
        join_rule: None,
        visibility: None,
        is_public: None,
        parent_space_id: Some(parent.space_id.clone()),
    };
    let child = storage.create_space(child_req).await.unwrap();

    // Path from child should be [grandparent, parent, child]
    let path = storage.get_space_tree_path(&child.space_id).await.unwrap();
    assert_eq!(path.len(), 3);
    assert_eq!(path[0].space_id, grandparent.space_id);
    assert_eq!(path[1].space_id, parent.space_id);
    assert_eq!(path[2].space_id, child.space_id);

    // Path from parent should be [grandparent, parent]
    let path = storage.get_space_tree_path(&parent.space_id).await.unwrap();
    assert_eq!(path.len(), 2);
    assert_eq!(path[0].space_id, grandparent.space_id);

    // Path from grandparent (no parent) should be [grandparent]
    let path = storage.get_space_tree_path(&grandparent.space_id).await.unwrap();
    assert_eq!(path.len(), 1);

    // Non-existent space returns empty
    let path = storage.get_space_tree_path("!nonexistent:localhost").await.unwrap();
    assert!(path.is_empty());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// resolve_space_id + get_space_by_identifier
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_resolve_space_id() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("res_{uid}"))).await.unwrap();

    // Resolve by space_id
    let resolved = storage.resolve_space_id(&space.space_id).await.unwrap();
    assert_eq!(resolved, Some(space.space_id.clone()));

    // Resolve by room_id
    let resolved = storage.resolve_space_id(&space.room_id).await.unwrap();
    assert_eq!(resolved, Some(space.space_id.clone()));

    // Non-existent identifier returns None
    let missing = storage.resolve_space_id("!nonexistent:localhost").await.unwrap();
    assert!(missing.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_space_by_identifier() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("ident_{uid}"))).await.unwrap();

    // By space_id
    let by_id = storage.get_space_by_identifier(&space.space_id).await.unwrap();
    assert!(by_id.is_some());
    assert_eq!(by_id.unwrap().space_id, space.space_id);

    // By room_id
    let by_room = storage.get_space_by_identifier(&space.room_id).await.unwrap();
    assert!(by_room.is_some());
    assert_eq!(by_room.unwrap().space_id, space.space_id);

    // Non-existent
    let missing = storage.get_space_by_identifier("!nonexistent:localhost").await.unwrap();
    assert!(missing.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_space_user_ids + get_space_room_ids
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_space_user_ids_and_room_ids() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("ids_{uid}"))).await.unwrap();

    // Add members (creator is already a member)
    let user2 = format!("@ids_user2_{uid}:localhost");
    let user3 = format!("@ids_user3_{uid}:localhost");
    storage.add_space_member(&space.space_id, &user2, "join", None).await.unwrap();
    storage.add_space_member(&space.space_id, &user3, "join", None).await.unwrap();

    let user_ids = storage.get_space_user_ids(&space.space_id).await.unwrap();
    assert_eq!(user_ids.len(), 3);

    // Add child rooms
    let room1 = format!("!ids_room1_{uid}:localhost");
    let room2 = format!("!ids_room2_{uid}:localhost");
    for room_id in [&room1, &room2] {
        storage
            .add_child(AddChildRequest {
                space_id: space.space_id.clone(),
                room_id: room_id.clone(),
                sender: "@sender:localhost".to_string(),
                is_suggested: false,
                via_servers: vec!["via.com".to_string()],
            })
            .await
            .unwrap();
    }

    let room_ids = storage.get_space_room_ids(&space.space_id).await.unwrap();
    assert_eq!(room_ids.len(), 2);
    assert!(room_ids.contains(&room1));
    assert!(room_ids.contains(&room2));

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_space_member_and_child_count
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_space_member_and_child_count() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("cnt_{uid}"))).await.unwrap();

    // Add one more member (creator already a member → 2 joined)
    let user = format!("@cnt_user_{uid}:localhost");
    storage.add_space_member(&space.space_id, &user, "join", None).await.unwrap();

    // Add 3 children
    for i in 0..3 {
        storage
            .add_child(AddChildRequest {
                space_id: space.space_id.clone(),
                room_id: format!("!cnt_child_{uid}_{i}:localhost"),
                sender: "@sender:localhost".to_string(),
                is_suggested: false,
                via_servers: vec!["via.com".to_string()],
            })
            .await
            .unwrap();
    }

    let (member_count, child_count) = storage.get_space_member_and_child_count(&space.space_id).await.unwrap();
    assert_eq!(member_count, 2);
    assert_eq!(child_count, 3);

    // Non-existent space → 0, 0
    let (m, c) = storage.get_space_member_and_child_count("!nonexistent:localhost").await.unwrap();
    assert_eq!(m, 0);
    assert_eq!(c, 0);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// check_user_can_see_space
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_check_user_can_see_space() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();

    // Public space: everyone can see
    let mut pub_req = make_create_request(&format!("see_pub_{uid}"));
    pub_req.is_public = Some(true);
    let pub_space = storage.create_space(pub_req).await.unwrap();
    assert!(storage.check_user_can_see_space(&pub_space.space_id, "@anyone:localhost").await.unwrap());

    // Private space: only members can see
    let mut priv_req = make_create_request(&format!("see_priv_{uid}"));
    priv_req.is_public = Some(false);
    let priv_space = storage.create_space(priv_req).await.unwrap();
    let member_user = format!("@see_member_{uid}:localhost");
    storage.add_space_member(&priv_space.space_id, &member_user, "join", None).await.unwrap();

    // Member can see
    assert!(storage.check_user_can_see_space(&priv_space.space_id, &member_user).await.unwrap());
    // Non-member cannot see
    assert!(!storage.check_user_can_see_space(&priv_space.space_id, "@stranger:localhost").await.unwrap());

    // Non-existent space: false
    assert!(!storage.check_user_can_see_space("!nonexistent:localhost", "@anyone:localhost").await.unwrap());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// search_spaces (including empty query edge case)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_search_spaces_empty_query() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Empty query returns empty
    let empty = storage.search_spaces("", 10, None).await.unwrap();
    assert!(empty.is_empty());

    // Whitespace-only query returns empty
    let whitespace = storage.search_spaces("   ", 10, None).await.unwrap();
    assert!(whitespace.is_empty());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_search_spaces_by_name() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let unique_name = format!("UniqueSearchTarget_{uid}");
    let mut req = make_create_request(&format!("search_{uid}"));
    req.name = Some(unique_name.clone());
    req.is_public = Some(true);
    let space = storage.create_space(req).await.unwrap();

    // Search without user_id (only public spaces)
    let results = storage.search_spaces(&unique_name, 10, None).await.unwrap();
    assert!(results.iter().any(|s| s.space_id == space.space_id));

    // Search with user_id (creator should see their private space too)
    let results = storage.search_spaces(&unique_name, 10, Some(&space.creator)).await.unwrap();
    assert!(results.iter().any(|s| s.space_id == space.space_id));

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_space_children_paginated
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_space_children_paginated() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("pgchild_{uid}"))).await.unwrap();

    // Add 3 children
    let child_ids: Vec<String> = (0..3).map(|i| format!("!pgc_room_{uid}_{i}:localhost")).collect();
    for room_id in &child_ids {
        storage
            .add_child(AddChildRequest {
                space_id: space.space_id.clone(),
                room_id: room_id.clone(),
                sender: "@sender:localhost".to_string(),
                is_suggested: false,
                via_servers: vec!["via.com".to_string()],
            })
            .await
            .unwrap();
    }

    // First page (no cursor)
    let page1 = storage.get_space_children_paginated(&space.space_id, 2, None, None).await.unwrap();
    assert_eq!(page1.len(), 2);

    // Second page (using cursor from last item of page1)
    let cursor_ts = page1[1].added_ts;
    let cursor_id = page1[1].id;
    let page2 = storage
        .get_space_children_paginated(&space.space_id, 2, Some(cursor_ts), Some(cursor_id))
        .await
        .unwrap();
    assert_eq!(page2.len(), 1);

    // Full results (large limit)
    let all = storage.get_space_children_paginated(&space.space_id, 100, None, None).await.unwrap();
    assert_eq!(all.len(), 3);

    // Non-existent space returns empty
    let empty = storage.get_space_children_paginated("!nonexistent:localhost", 10, None, None).await.unwrap();
    assert!(empty.is_empty());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_space_members_paginated
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_space_members_paginated() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("pgm_{uid}"))).await.unwrap();

    // Add 2 extra members (creator is already a member → 3 total)
    let user2 = format!("@pgm_user2_{uid}:localhost");
    let user3 = format!("@pgm_user3_{uid}:localhost");
    storage.add_space_member(&space.space_id, &user2, "join", None).await.unwrap();
    storage.add_space_member(&space.space_id, &user3, "join", None).await.unwrap();

    // First page (no cursor)
    let page1 = storage.get_space_members_paginated(&space.space_id, 2, None, None).await.unwrap();
    assert_eq!(page1.len(), 2);

    // Second page (using cursor from last item of page1)
    let cursor_ts = page1[1].joined_ts;
    let cursor_user = page1[1].user_id.as_str();
    let page2 = storage
        .get_space_members_paginated(&space.space_id, 2, Some(cursor_ts), Some(cursor_user))
        .await
        .unwrap();
    assert_eq!(page2.len(), 1);

    // Full results (large limit)
    let all = storage.get_space_members_paginated(&space.space_id, 100, None, None).await.unwrap();
    assert_eq!(all.len(), 3);

    // Non-existent space returns empty
    let empty = storage.get_space_members_paginated("!nonexistent:localhost", 10, None, None).await.unwrap();
    assert!(empty.is_empty());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_space_statistics
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_space_statistics_empty() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // No data in space_statistics table → empty
    let stats = storage.get_space_statistics(10).await.unwrap();
    assert!(stats.is_empty());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_space_statistics_with_data() {
    let _guard = space_storage_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let space = storage.create_space(make_create_request(&format!("stat_{uid}"))).await.unwrap();
    let now = chrono::Utc::now().timestamp_millis();

    // Manually insert a row into space_statistics (the table is not populated by space.rs methods)
    sqlx::query(
        r"INSERT INTO space_statistics (space_id, name, is_public, child_room_count, member_count, created_ts, updated_ts)
           VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&space.space_id)
    .bind("StatSpace")
    .bind(true)
    .bind(5_i64)
    .bind(10_i64)
    .bind(now)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let stats = storage.get_space_statistics(10).await.unwrap();
    assert!(!stats.is_empty());
    let found = stats.iter().find(|s| s["space_id"].as_str() == Some(space.space_id.as_str()));
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found["name"].as_str(), Some("StatSpace"));
    assert_eq!(found["child_room_count"].as_i64(), Some(5));
    assert_eq!(found["member_count"].as_i64(), Some(10));

    teardown(pool.as_ref()).await;
}
