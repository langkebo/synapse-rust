//! DB-backed integration tests for `DirectoryStorage` (ARCH-06).
//!
//! These tests exercise the real PostgreSQL implementation of
//! `DirectoryStoreApi` against a migrated schema. They verify that the
//! `room_directory` metadata columns (added by migration
//! `20260810140000_add_directory_metadata_columns.sql`) decode correctly —
//! in particular that `member_count` (BIGINT) maps to Rust `i64` without a
//! `ColumnDecode` error.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use synapse_common::current_timestamp_millis;
use synapse_storage::directory::{DirectoryStorage, DirectoryStoreApi, RoomDirectoryEntry};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Insert a row into the `rooms` table to satisfy the
/// `fk_room_directory_room` foreign key constraint.
async fn insert_room(pool: &sqlx::PgPool, room_id: &str) {
    let now = current_timestamp_millis();
    sqlx::query("INSERT INTO rooms (room_id, created_ts) VALUES ($1, $2) ON CONFLICT (room_id) DO NOTHING")
        .bind(room_id)
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to insert test room");
}

fn create_storage(pool: &Arc<sqlx::PgPool>) -> DirectoryStorage {
    DirectoryStorage::new(pool)
}

/// Verifies the upsert -> list round-trip against PostgreSQL, with a
/// `member_count` value that exceeds `i32::MAX` to confirm the column is
/// `BIGINT` (not `INTEGER`) and decodes as `i64`.
#[tokio::test]
async fn test_directory_storage_upsert_and_list_roundtrip() {
    let pool = crate::require_test_pool().await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!dir_roundtrip_{suffix}:localhost");

    insert_room(&pool, &room_id).await;

    // Use a member_count that exceeds i32::MAX (2_147_483_647) to ensure the
    // column is BIGINT. With INTEGER this INSERT would fail with "integer out
    // of range", and the subsequent SELECT would fail to decode into i64.
    let entry = RoomDirectoryEntry {
        room_id: room_id.clone(),
        name: Some("Directory Test Room".to_string()),
        topic: Some("Testing upsert + list round-trip".to_string()),
        avatar_url: Some("mxc://localhost/avatar".to_string()),
        canonical_alias: Some("#dirtest:localhost".to_string()),
        join_rule: "public".to_string(),
        world_readable: true,
        guest_can_join: false,
        member_count: 3_000_000_000_i64,
    };

    storage.upsert_directory_entry(&entry).await.expect("upsert_directory_entry should succeed");

    let listed = storage.list_public_rooms(100, 0).await.expect("list_public_rooms should succeed");

    let found = listed
        .iter()
        .find(|e| e.room_id == room_id)
        .unwrap_or_else(|| panic!("room {room_id} should appear in public directory listing"));

    assert_eq!(found.name.as_deref(), Some("Directory Test Room"));
    assert_eq!(found.topic.as_deref(), Some("Testing upsert + list round-trip"));
    assert_eq!(found.avatar_url.as_deref(), Some("mxc://localhost/avatar"));
    assert_eq!(found.canonical_alias.as_deref(), Some("#dirtest:localhost"));
    assert_eq!(found.join_rule, "public");
    assert!(found.world_readable);
    assert!(!found.guest_can_join);
    // The critical assertion: member_count must decode as i64 with the full value.
    assert_eq!(found.member_count, 3_000_000_000_i64);

    // Cleanup
    storage.remove_from_directory(&room_id).await.expect("remove_from_directory should succeed");
    let after_remove = storage.list_public_rooms(100, 0).await.expect("list after remove should succeed");
    assert!(after_remove.iter().all(|e| e.room_id != room_id), "room should be removed from directory");
}

/// Verifies that a second upsert updates the existing row (ON CONFLICT path)
/// and that `member_count` reflects the new value.
#[tokio::test]
async fn test_directory_storage_upsert_updates_existing_row() {
    let pool = crate::require_test_pool().await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!dir_update_{suffix}:localhost");

    insert_room(&pool, &room_id).await;

    // Initial insert
    let mut entry = RoomDirectoryEntry::new(&room_id);
    entry.name = Some("Initial Name".to_string());
    entry.member_count = 42;
    storage.upsert_directory_entry(&entry).await.expect("first upsert should succeed");

    // Update via upsert
    entry.name = Some("Updated Name".to_string());
    entry.topic = Some("New topic".to_string());
    entry.member_count = 7_000_000_000_i64;
    storage.upsert_directory_entry(&entry).await.expect("second upsert should succeed");

    let listed = storage.list_public_rooms(100, 0).await.expect("list should succeed");
    let found = listed.iter().find(|e| e.room_id == room_id).expect("updated room should be listed");

    assert_eq!(found.name.as_deref(), Some("Updated Name"));
    assert_eq!(found.topic.as_deref(), Some("New topic"));
    assert_eq!(found.member_count, 7_000_000_000_i64);

    // Cleanup
    storage.remove_from_directory(&room_id).await.expect("cleanup should succeed");
}

/// Verifies that `search_public_rooms` filters by name/topic substring and
/// returns matching entries with correct `member_count` decoding.
#[tokio::test]
async fn test_directory_storage_search_public_rooms() {
    let pool = crate::require_test_pool().await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!dir_search_{suffix}:localhost");

    insert_room(&pool, &room_id).await;

    let entry = RoomDirectoryEntry {
        room_id: room_id.clone(),
        name: Some("UniqueSearchableRoomName".to_string()),
        topic: Some("Contains special keyword".to_string()),
        avatar_url: None,
        canonical_alias: None,
        join_rule: "public".to_string(),
        world_readable: false,
        guest_can_join: true,
        member_count: 5_000_000_000_i64,
    };
    storage.upsert_directory_entry(&entry).await.expect("upsert should succeed");

    // Search by name substring
    let by_name = storage.search_public_rooms("UniqueSearchable", 100).await.expect("search by name should succeed");
    let found_by_name = by_name.iter().find(|e| e.room_id == room_id).expect("should find room by name search");
    assert_eq!(found_by_name.member_count, 5_000_000_000_i64);
    assert!(found_by_name.guest_can_join);

    // Search by topic substring
    let by_topic = storage.search_public_rooms("special keyword", 100).await.expect("search by topic should succeed");
    assert!(by_topic.iter().any(|e| e.room_id == room_id), "should find room by topic search");

    // Search with non-matching filter
    let no_match =
        storage.search_public_rooms("zzz_no_match_zzz", 100).await.expect("non-matching search should succeed");
    assert!(no_match.iter().all(|e| e.room_id != room_id), "non-matching filter should not return room");

    // Cleanup
    storage.remove_from_directory(&room_id).await.expect("cleanup should succeed");
}
