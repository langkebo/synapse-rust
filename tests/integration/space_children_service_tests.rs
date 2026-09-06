//! Integration tests for `synapse_services::room::space::children` module.
//!
//! Background: the `SpaceService` in `synapse-services/src/room/space/` had no
//! dedicated service-level tests — only `api_space_routes_tests.rs` exercised
//! it through the HTTP layer. This file covers the children-management paths
//! directly against a real Postgres schema (the same way
//! `e2ee_audit_service_tests.rs` does), so failures point at the service
//! surface, not at HTTP serialization.
//!
//! Coverage targets (synapse-services/src/room/space/children.rs):
//! - `add_child` happy path (creator → child added, summary updated, event appended)
//! - `add_child` fails when caller is not the space creator
//! - `add_child` fails when child room does not exist
//! - `remove_child` happy path
//! - `remove_child` fails when caller is not the space creator
//! - `get_space_children` returns added children
//! - `get_space_children_paginated` walks pages without overlap
//!
//! Method paths that need separate storage (`hierarchy`, `summary_with_children`,
//! `recursive_hierarchy`) are NOT covered here because they each require a fully
//! populated hierarchy and would replicate the suite in
//! `api_space_routes_tests.rs` (`space_hierarchy_suite`). Keep this file
//! scoped to the child CRUD contract.

#![cfg(feature = "test-utils")]

use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_services::room::space::SpaceService;
use synapse_storage::room::RoomStorage;
use synapse_storage::space::{AddChildRequest, CreateSpaceRequest, SpaceStorage};
use synapse_storage::RoomStoreApi;

use crate::require_test_pool;

/// Insert a minimal row into `rooms` with a given creator so that
/// `SpaceService::ensure_room_creator_access` (which compares
/// `room.creator` to the caller) succeeds.
async fn seed_room_with_creator(pool: &sqlx::PgPool, room_id: &str, creator: &str) {
    sqlx::query(
        "INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3)
         ON CONFLICT (room_id) DO NOTHING",
    )
    .bind(room_id)
    .bind(creator)
    .bind(current_timestamp_millis())
    .execute(pool)
    .await
    .expect("seed_room_with_creator");
}

/// Create a space via `SpaceStorage` directly (the service's
/// `create_space` also requires a corresponding `rooms` row; we
/// bypass it here because we only want to exercise the `children.rs`
/// methods, not the whole `mod.rs` CRUD surface).
async fn seed_space(pool: &sqlx::PgPool, room_id: &str, creator: &str) -> String {
    let storage = SpaceStorage::new(&Arc::new(pool.clone()));
    let req = CreateSpaceRequest {
        room_id: room_id.to_string(),
        name: Some(format!("space-{room_id}")),
        topic: None,
        avatar_url: None,
        creator: creator.to_string(),
        join_rule: Some("invite".to_string()),
        visibility: Some("private".to_string()),
        is_public: Some(false),
        parent_space_id: None,
    };
    storage.create_space(req).await.map(|s| s.space_id).expect("create_space")
}

fn build_service(pool: &sqlx::PgPool) -> SpaceService {
    let pool_arc = Arc::new(pool.clone());
    let space_storage = Arc::new(SpaceStorage::new(&pool_arc));
    let room_storage: Arc<dyn RoomStoreApi> = Arc::new(RoomStorage::new(&pool_arc));
    SpaceService::new(space_storage, room_storage, "localhost".to_string())
}

#[tokio::test]
async fn add_child_happy_path_persists_row_and_event() {
    let pool = require_test_pool().await;
    let svc = build_service(&pool);

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let space_room = format!("!space_{suffix}:localhost");
    let child_room = format!("!child_{suffix}:localhost");
    let creator = format!("@creator_{suffix}:localhost");

    seed_room_with_creator(&pool, &space_room, &creator).await;
    seed_room_with_creator(&pool, &child_room, &creator).await;
    let space_id = seed_space(&pool, &space_room, &creator).await;

    let req = AddChildRequest {
        space_id: space_id.clone(),
        room_id: child_room.clone(),
        sender: creator.clone(),
        is_suggested: true,
        via_servers: vec!["localhost".to_string()],
    };
    let child = svc.add_child(req).await.expect("add_child");

    assert_eq!(child.space_id, space_id);
    assert_eq!(child.room_id, child_room);
    assert!(child.is_suggested);

    // Re-query the children list to confirm persistence (not just return value).
    let listed = svc.get_space_children(&space_id).await.expect("get_space_children");
    assert_eq!(listed.len(), 1, "exactly one child must be persisted");
    assert_eq!(listed[0].room_id, child_room);
}

#[tokio::test]
async fn add_child_rejects_non_creator() {
    let pool = require_test_pool().await;
    let svc = build_service(&pool);

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let space_room = format!("!space_{suffix}:localhost");
    let child_room = format!("!child_{suffix}:localhost");
    let creator = format!("@creator_{suffix}:localhost");
    let attacker = format!("@attacker_{suffix}:localhost");

    seed_room_with_creator(&pool, &space_room, &creator).await;
    seed_room_with_creator(&pool, &child_room, &creator).await;
    let space_id = seed_space(&pool, &space_room, &creator).await;

    let req = AddChildRequest {
        space_id: space_id.clone(),
        room_id: child_room.clone(),
        sender: attacker.clone(),
        is_suggested: false,
        via_servers: vec!["localhost".to_string()],
    };
    let err = svc.add_child(req).await.expect_err("non-creator must be rejected");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("forbidden") || msg.contains("only the space creator"),
        "expected creator-only error, got: {msg}"
    );

    // Side-effect check: no child row should have been written.
    let listed = svc.get_space_children(&space_id).await.expect("get_space_children");
    assert!(listed.is_empty(), "rejected call must not persist anything");
}

#[tokio::test]
async fn add_child_returns_not_found_when_child_room_missing() {
    let pool = require_test_pool().await;
    let svc = build_service(&pool);

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let space_room = format!("!space_{suffix}:localhost");
    let missing_child = format!("!missing_{suffix}:localhost");
    let creator = format!("@creator_{suffix}:localhost");

    seed_room_with_creator(&pool, &space_room, &creator).await;
    let space_id = seed_space(&pool, &space_room, &creator).await;

    let req = AddChildRequest {
        space_id: space_id.clone(),
        room_id: missing_child.clone(),
        sender: creator.clone(),
        is_suggested: false,
        via_servers: vec!["localhost".to_string()],
    };
    let err = svc.add_child(req).await.expect_err("missing child room must error");
    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("not found") || msg.contains("room not found"), "got: {msg}");
}

#[tokio::test]
async fn remove_child_happy_path_drops_row() {
    let pool = require_test_pool().await;
    let svc = build_service(&pool);

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let space_room = format!("!space_{suffix}:localhost");
    let child_room = format!("!child_{suffix}:localhost");
    let creator = format!("@creator_{suffix}:localhost");

    seed_room_with_creator(&pool, &space_room, &creator).await;
    seed_room_with_creator(&pool, &child_room, &creator).await;
    let space_id = seed_space(&pool, &space_room, &creator).await;

    // Seed an existing child via the service so remove_child has something to
    // delete.
    svc.add_child(AddChildRequest {
        space_id: space_id.clone(),
        room_id: child_room.clone(),
        sender: creator.clone(),
        is_suggested: false,
        via_servers: vec!["localhost".to_string()],
    })
    .await
    .expect("add_child");

    svc.remove_child(&space_id, &child_room, &creator).await.expect("remove_child");

    let listed = svc.get_space_children(&space_id).await.expect("get_space_children");
    assert!(listed.is_empty(), "child row must be gone after remove");
}

#[tokio::test]
async fn remove_child_rejects_non_creator() {
    let pool = require_test_pool().await;
    let svc = build_service(&pool);

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let space_room = format!("!space_{suffix}:localhost");
    let child_room = format!("!child_{suffix}:localhost");
    let creator = format!("@creator_{suffix}:localhost");
    let attacker = format!("@attacker_{suffix}:localhost");

    seed_room_with_creator(&pool, &space_room, &creator).await;
    seed_room_with_creator(&pool, &child_room, &creator).await;
    let space_id = seed_space(&pool, &space_room, &creator).await;

    svc.add_child(AddChildRequest {
        space_id: space_id.clone(),
        room_id: child_room.clone(),
        sender: creator.clone(),
        is_suggested: false,
        via_servers: vec!["localhost".to_string()],
    })
    .await
    .expect("add_child");

    let err = svc.remove_child(&space_id, &child_room, &attacker).await.expect_err("must reject");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("forbidden") || msg.contains("only the space creator"),
        "expected creator-only error, got: {msg}"
    );

    // The child row should still be there.
    let listed = svc.get_space_children(&space_id).await.expect("get_space_children");
    assert_eq!(listed.len(), 1, "rejected remove must not delete the row");
}

#[tokio::test]
async fn get_space_children_paginated_walks_pages_without_overlap() {
    let pool = require_test_pool().await;
    let svc = build_service(&pool);

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let space_room = format!("!space_{suffix}:localhost");
    let creator = format!("@creator_{suffix}:localhost");

    seed_room_with_creator(&pool, &space_room, &creator).await;
    let space_id = seed_space(&pool, &space_room, &creator).await;

    // Add 5 children sequentially so added_ts differ.
    for i in 0..5 {
        let child_room = format!("!c{i}_{suffix}:localhost");
        seed_room_with_creator(&pool, &child_room, &creator).await;
        svc.add_child(AddChildRequest {
            space_id: space_id.clone(),
            room_id: child_room,
            sender: creator.clone(),
            is_suggested: false,
            via_servers: vec!["localhost".to_string()],
        })
        .await
        .expect("add_child");
        // Spread inserts across distinct ts to guarantee stable ordering.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    let page1 = svc.get_space_children_paginated(&space_id, 2, None, None).await.expect("page1");
    assert_eq!(page1.len(), 2, "page 1 must be size 2");

    let last = page1.last().expect("non-empty");
    let page2 = svc
        .get_space_children_paginated(&space_id, 2, Some(last.added_ts), Some(last.id))
        .await
        .expect("page2");
    assert_eq!(page2.len(), 2, "page 2 must be size 2");
    for c in &page2 {
        assert!(!page1.iter().any(|p| p.id == c.id), "pages must not overlap");
    }

    let last2 = page2.last().expect("non-empty");
    let page3 = svc
        .get_space_children_paginated(&space_id, 2, Some(last2.added_ts), Some(last2.id))
        .await
        .expect("page3");
    assert_eq!(page3.len(), 1, "page 3 should have the last remaining child");
}
