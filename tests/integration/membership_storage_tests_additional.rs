//! Additional integration tests for `synapse-storage/src/membership/mod.rs`.
//!
//! The migrated file (`membership_storage_tests_migrated.rs`) covers the basic
//! CRUD paths. This file focuses on the methods that were previously uncovered:
//! - `has_any_non_banned_member_from_server` (federation authorization)
//! - `get_room_members_paginated` (cursor pagination)
//! - `share_common_rooms_batch`
//! - `user_shares_room_with_server` (federation)
//! - `filter_users_sharing_room_with_server` (federation batch)
//! - `set_ban_reason`
//! - `force_leave_membership`
//! - `get_joined_servers_in_room` (federation backfill candidate list)
//! - `add_member` transaction / explicit-sender / invite-state branches
//! - `RoomMemberRepository` trait dispatch

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use synapse_storage::membership::repository::RoomMemberRepository;
use synapse_storage::membership::{RoomMember, RoomMemberStorage, UserRoomMembership};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(10_000);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn membership_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Warm up the shared pool on the current tokio runtime (cross-runtime
/// isolation fix per project memory).
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

/// Create all tables used by membership/mod.rs with `CREATE TABLE IF NOT EXISTS`.
async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            user_id TEXT NOT NULL PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT,
            is_admin BOOLEAN DEFAULT FALSE,
            is_guest BOOLEAN DEFAULT FALSE,
            is_shadow_banned BOOLEAN DEFAULT FALSE,
            is_deactivated BOOLEAN DEFAULT FALSE,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            displayname TEXT,
            avatar_url TEXT,
            email TEXT,
            phone TEXT,
            generation BIGINT DEFAULT 0,
            consent_version TEXT,
            appservice_id TEXT,
            user_type TEXT,
            invalid_update_at BIGINT,
            migration_state TEXT,
            password_changed_ts BIGINT,
            is_password_change_required BOOLEAN DEFAULT FALSE,
            must_change_password BOOLEAN DEFAULT FALSE,
            password_expires_at BIGINT,
            failed_login_attempts INTEGER DEFAULT 0,
            locked_until BIGINT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create users table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS rooms (
            room_id TEXT NOT NULL PRIMARY KEY,
            creator TEXT,
            is_public BOOLEAN DEFAULT FALSE,
            room_version TEXT DEFAULT '6',
            created_ts BIGINT NOT NULL,
            last_activity_ts BIGINT,
            is_federated BOOLEAN DEFAULT TRUE,
            has_guest_access BOOLEAN DEFAULT FALSE,
            join_rules TEXT DEFAULT 'invite',
            history_visibility TEXT DEFAULT 'shared',
            name TEXT,
            topic TEXT,
            avatar_url TEXT,
            canonical_alias TEXT,
            visibility TEXT DEFAULT 'private'
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create rooms table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS room_memberships (
            id BIGSERIAL PRIMARY KEY,
            room_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            membership TEXT NOT NULL,
            joined_ts BIGINT,
            invited_ts BIGINT,
            left_ts BIGINT,
            banned_ts BIGINT,
            sender TEXT,
            reason TEXT,
            event_id TEXT,
            event_type TEXT,
            display_name TEXT,
            avatar_url TEXT,
            is_banned BOOLEAN DEFAULT FALSE,
            invite_token TEXT,
            updated_ts BIGINT,
            join_reason TEXT,
            banned_by TEXT,
            ban_reason TEXT,
            UNIQUE (room_id, user_id)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create room_memberships table");
}

fn create_storage(pool: &Arc<sqlx::PgPool>) -> RoomMemberStorage {
    RoomMemberStorage::new(pool, "localhost")
}

async fn insert_user(pool: &sqlx::PgPool, user_id: &str, username: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
        .bind(user_id)
        .bind(username)
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to insert test user");
}

async fn insert_room(pool: &sqlx::PgPool, room_id: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO rooms (room_id, created_ts) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(room_id)
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to insert test room");
}

async fn insert_membership_direct(pool: &sqlx::PgPool, room_id: &str, user_id: &str, membership: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"INSERT INTO room_memberships (room_id, user_id, membership, joined_ts, updated_ts)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (room_id, user_id) DO UPDATE SET membership = EXCLUDED.membership, joined_ts = EXCLUDED.joined_ts, updated_ts = EXCLUDED.updated_ts"#,
    )
    .bind(room_id)
    .bind(user_id)
    .bind(membership)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to insert test membership");
}

// =============================================================================
// Construction & pool accessor
// =============================================================================

#[tokio::test]
async fn test_new_storage_construction() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    // A trivial read proves the constructed storage holds a usable pool.
    let _ = storage.get_room_member_count("!construct_room:localhost").await.unwrap();
}

#[tokio::test]
async fn test_storage_server_name_used_for_event_id() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@evtname_{suffix}:localhost");
    let room_id = format!("!evtname_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("evtname_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    let member = storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
    // Event ID is generated with the configured server name.
    assert!(member.event_id.as_ref().unwrap().starts_with('$'));
}

// =============================================================================
// add_member — invite / leave membership states (joined_ts branch)
// =============================================================================

#[tokio::test]
async fn test_add_member_invite_sets_joined_ts_null() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@inv_{suffix}:localhost");
    let room_id = format!("!inv_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("inv_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    let member = storage.add_member(&room_id, &user_id, "invite", None, None, None, None).await.unwrap();
    assert_eq!(member.membership, "invite");
    // membership != "join" => joined_ts must be None.
    assert!(member.joined_ts.is_none(), "invite must not set joined_ts");
}

#[tokio::test]
async fn test_add_member_leave_no_joined_ts() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@addleave_{suffix}:localhost");
    let room_id = format!("!addleave_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("addleave_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    let member = storage.add_member(&room_id, &user_id, "leave", None, None, None, None).await.unwrap();
    assert_eq!(member.membership, "leave");
    assert!(member.joined_ts.is_none());
}

// =============================================================================
// add_member — explicit sender
// =============================================================================

#[tokio::test]
async fn test_add_member_with_explicit_sender() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@target_{suffix}:localhost");
    let inviter = format!("@inviter_{suffix}:localhost");
    let room_id = format!("!sender_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("target_{suffix}")).await;
    insert_user(&pool, &inviter, &format!("inviter_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    let member =
        storage.add_member(&room_id, &user_id, "invite", None, None, Some(&inviter), None).await.unwrap();
    assert_eq!(member.sender.as_deref(), Some(inviter.as_str()));
}

#[tokio::test]
async fn test_add_member_sender_defaults_to_user_id() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@selfjoin_{suffix}:localhost");
    let room_id = format!("!selfjoin_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("selfjoin_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    let member = storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
    // When sender is None, effective_sender defaults to user_id.
    assert_eq!(member.sender.as_deref(), Some(user_id.as_str()));
}

// =============================================================================
// add_member — within a transaction (commit + rollback)
// =============================================================================

#[tokio::test]
async fn test_add_member_with_transaction_commit() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@txcommit_{suffix}:localhost");
    let room_id = format!("!txcommit_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("txcommit_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    let mut tx = pool.begin().await.unwrap();
    let member =
        storage.add_member(&room_id, &user_id, "join", Some("TxUser"), None, None, Some(&mut tx)).await.unwrap();
    tx.commit().await.unwrap();

    assert_eq!(member.display_name.as_deref(), Some("TxUser"));
    // Visible after commit.
    let fetched = storage.get_member(&room_id, &user_id).await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().membership, "join");
}

#[tokio::test]
async fn test_add_member_with_transaction_rollback_not_visible() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@txroll_{suffix}:localhost");
    let room_id = format!("!txroll_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("txroll_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    let mut tx = pool.begin().await.unwrap();
    let _member =
        storage.add_member(&room_id, &user_id, "join", None, None, None, Some(&mut tx)).await.unwrap();
    tx.rollback().await.unwrap();

    // Rolled back — must not be visible via the pool.
    let fetched = storage.get_member(&room_id, &user_id).await.unwrap();
    assert!(fetched.is_none(), "rolled-back transaction must not persist the member");
}

// =============================================================================
// add_member — state transitions & joined_ts / left_ts handling
// =============================================================================

#[tokio::test]
async fn test_add_member_join_then_leave_preserves_joined_ts() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@jlpres_{suffix}:localhost");
    let room_id = format!("!jlpres_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("jlpres_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    let joined = storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
    let original_joined_ts = joined.joined_ts.expect("join must set joined_ts");

    // Re-add with "leave": ON CONFLICT keeps existing joined_ts (membership != 'join').
    let left = storage.add_member(&room_id, &user_id, "leave", None, None, None, None).await.unwrap();
    assert_eq!(left.membership, "leave");
    assert_eq!(left.joined_ts, Some(original_joined_ts), "joined_ts must be preserved on leave");
}

#[tokio::test]
async fn test_add_member_rejoin_after_leave_resets_left_ts() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@rejoin_{suffix}:localhost");
    let room_id = format!("!rejoin_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("rejoin_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    // Join, then leave via remove_member (sets left_ts).
    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
    storage.remove_member(&room_id, &user_id).await.unwrap();
    let left_member = storage.get_member(&room_id, &user_id).await.unwrap().unwrap();
    assert!(left_member.left_ts.is_some(), "leave must set left_ts");

    // Re-join: left_ts must be reset to NULL, joined_ts preserved.
    let rejoined = storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
    assert_eq!(rejoined.membership, "join");
    assert!(rejoined.left_ts.is_none(), "rejoin must clear left_ts");
    assert!(rejoined.joined_ts.is_some());
}

#[tokio::test]
async fn test_add_member_upsert_updates_display_name_and_join_reason() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@upsert2_{suffix}:localhost");
    let room_id = format!("!upsert2_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("upsert2_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    let first =
        storage.add_member(&room_id, &user_id, "join", Some("Old"), Some("r1"), None, None).await.unwrap();
    assert_eq!(first.display_name.as_deref(), Some("Old"));
    assert_eq!(first.join_reason.as_deref(), Some("r1"));

    let second =
        storage.add_member(&room_id, &user_id, "join", Some("New"), Some("r2"), None, None).await.unwrap();
    assert_eq!(second.display_name.as_deref(), Some("New"));
    assert_eq!(second.join_reason.as_deref(), Some("r2"));
}

#[tokio::test]
async fn test_add_member_invite_then_join_sets_joined_ts() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@invjoin_{suffix}:localhost");
    let room_id = format!("!invjoin_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("invjoin_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    let invited = storage.add_member(&room_id, &user_id, "invite", None, None, None, None).await.unwrap();
    assert!(invited.joined_ts.is_none());

    let joined = storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
    assert_eq!(joined.membership, "join");
    assert!(joined.joined_ts.is_some(), "join after invite must set joined_ts");
}

// =============================================================================
// has_any_non_banned_member_from_server (federation authorization)
// =============================================================================

#[tokio::test]
async fn test_has_any_non_banned_member_from_server_join() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@fedjoin_{suffix}:remote.example");
    let room_id = format!("!fedjoin_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("fedjoin_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();

    let has = storage.has_any_non_banned_member_from_server(&room_id, "remote.example").await.unwrap();
    assert!(has, "a joined member from the server should match");
}

#[tokio::test]
async fn test_has_any_non_banned_member_from_server_invite() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@fedinv_{suffix}:remote.example");
    let room_id = format!("!fedinv_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("fedinv_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &user_id, "invite", None, None, None, None).await.unwrap();

    let has = storage.has_any_non_banned_member_from_server(&room_id, "remote.example").await.unwrap();
    assert!(has, "invite is a non-banned membership and should match");
}

#[tokio::test]
async fn test_has_any_non_banned_member_from_server_leave() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@fedleave_{suffix}:remote.example");
    let room_id = format!("!fedleave_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("fedleave_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &user_id, "leave", None, None, None, None).await.unwrap();

    let has = storage.has_any_non_banned_member_from_server(&room_id, "remote.example").await.unwrap();
    assert!(has, "leave is a non-banned membership and should match");
}

#[tokio::test]
async fn test_has_any_non_banned_member_from_server_ban_excluded() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@fedban_{suffix}:remote.example");
    let room_id = format!("!fedban_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("fedban_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.ban_member(&room_id, &user_id, "@admin:localhost").await.unwrap();

    let has = storage.has_any_non_banned_member_from_server(&room_id, "remote.example").await.unwrap();
    assert!(!has, "banned members must be excluded from the non-banned check");
}

#[tokio::test]
async fn test_has_any_non_banned_member_from_server_no_match_wrong_domain() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@fedother_{suffix}:other.example");
    let room_id = format!("!fedother_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("fedother_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();

    let has = storage.has_any_non_banned_member_from_server(&room_id, "remote.example").await.unwrap();
    assert!(!has, "members from a different server domain must not match");
}

#[tokio::test]
async fn test_has_any_non_banned_member_from_server_empty_room() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!fedempty_room_{suffix}:localhost");
    insert_room(&pool, &room_id).await;

    let has = storage.has_any_non_banned_member_from_server(&room_id, "remote.example").await.unwrap();
    assert!(!has, "empty room should have no matching members");
}

// =============================================================================
// get_room_members_paginated (cursor pagination)
// =============================================================================

#[tokio::test]
async fn test_get_room_members_paginated_first_page() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!pagfirst_room_{suffix}:localhost");
    insert_room(&pool, &room_id).await;
    // Use deliberately ordered user IDs so pagination is deterministic.
    for i in 0..5 {
        let user_id = format!("@pag_a{i}_{suffix}:localhost");
        insert_user(&pool, &user_id, &format!("pag_a{i}_{suffix}")).await;
        storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
    }

    let page = storage.get_room_members_paginated(&room_id, "join", 3, None).await.unwrap();
    assert_eq!(page.len(), 3, "first page should return up to the limit");
    // Ordered by user_id ASC.
    assert!(page[0].user_id < page[1].user_id);
    assert!(page[1].user_id < page[2].user_id);
}

#[tokio::test]
async fn test_get_room_members_paginated_with_cursor() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!pagcursor_room_{suffix}:localhost");
    insert_room(&pool, &room_id).await;
    for i in 0..4 {
        let user_id = format!("@pagcur_b{i}_{suffix}:localhost");
        insert_user(&pool, &user_id, &format!("pagcur_b{i}_{suffix}")).await;
        storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
    }

    let page1 = storage.get_room_members_paginated(&room_id, "join", 2, None).await.unwrap();
    assert_eq!(page1.len(), 2);
    let cursor = page1[1].user_id.clone();

    let page2 = storage.get_room_members_paginated(&room_id, "join", 2, Some(&cursor)).await.unwrap();
    assert_eq!(page2.len(), 2, "second page should return the remaining members");
    // All page2 user_ids must be strictly greater than the cursor.
    assert!(page2.iter().all(|m| m.user_id > cursor));
    // No overlap.
    let page1_ids: std::collections::HashSet<&str> = page1.iter().map(|m| m.user_id.as_str()).collect();
    assert!(page2.iter().all(|m| !page1_ids.contains(m.user_id.as_str())));
}

#[tokio::test]
async fn test_get_room_members_paginated_empty_room() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!pagempty_room_{suffix}:localhost");
    insert_room(&pool, &room_id).await;

    let page = storage.get_room_members_paginated(&room_id, "join", 10, None).await.unwrap();
    assert!(page.is_empty());
}

#[tokio::test]
async fn test_get_room_members_paginated_limit_exceeds_count() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!pagexceed_room_{suffix}:localhost");
    insert_room(&pool, &room_id).await;
    for i in 0..2 {
        let user_id = format!("@pagexc_c{i}_{suffix}:localhost");
        insert_user(&pool, &user_id, &format!("pagexc_c{i}_{suffix}")).await;
        storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
    }

    let page = storage.get_room_members_paginated(&room_id, "join", 100, None).await.unwrap();
    assert_eq!(page.len(), 2, "limit larger than count should return all matching members");
}

#[tokio::test]
async fn test_get_room_members_paginated_filter_by_leave() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!pagleave_room_{suffix}:localhost");
    insert_room(&pool, &room_id).await;
    let joiner = format!("@paglv_join_{suffix}:localhost");
    let leaver = format!("@paglv_leave_{suffix}:localhost");
    insert_user(&pool, &joiner, &format!("paglv_join_{suffix}")).await;
    insert_user(&pool, &leaver, &format!("paglv_leave_{suffix}")).await;
    storage.add_member(&room_id, &joiner, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &leaver, "leave", None, None, None, None).await.unwrap();

    let leave_page = storage.get_room_members_paginated(&room_id, "leave", 10, None).await.unwrap();
    assert_eq!(leave_page.len(), 1);
    assert_eq!(leave_page[0].user_id, leaver);

    let join_page = storage.get_room_members_paginated(&room_id, "join", 10, None).await.unwrap();
    assert_eq!(join_page.len(), 1);
    assert_eq!(join_page[0].user_id, joiner);
}

#[tokio::test]
async fn test_get_room_members_paginated_cursor_past_end() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!pagpast_room_{suffix}:localhost");
    insert_room(&pool, &room_id).await;
    let user_id = format!("@pagpast_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("pagpast_{suffix}")).await;
    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();

    // Cursor lexicographically after the only member => empty.
    let page =
        storage.get_room_members_paginated(&room_id, "join", 10, Some(&format!("~zzz_{suffix}"))).await.unwrap();
    assert!(page.is_empty(), "cursor past all members should return an empty page");
}

// =============================================================================
// share_common_rooms_batch
// =============================================================================

#[tokio::test]
async fn test_share_common_rooms_batch_basic() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_a = format!("@scb_a_{suffix}:localhost");
    let user_b = format!("@scb_b_{suffix}:localhost");
    let user_c = format!("@scb_c_{suffix}:localhost");
    let room_id = format!("!scb_room_{suffix}:localhost");
    for (uid, uname) in [(&user_a, "scb_a"), (&user_b, "scb_b"), (&user_c, "scb_c")] {
        insert_user(&pool, uid, &format!("{uname}_{suffix}")).await;
    }
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_b, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_c, "join", None, None, None, None).await.unwrap();

    let shared =
        storage.share_common_rooms_batch(&user_a, &[user_b.clone(), user_c.clone()]).await.unwrap();
    assert_eq!(shared.len(), 2);
    assert!(shared.contains(&user_b));
    assert!(shared.contains(&user_c));
}

#[tokio::test]
async fn test_share_common_rooms_batch_empty_others() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_a = format!("@scbempty_{suffix}:localhost");
    insert_user(&pool, &user_a, &format!("scbempty_{suffix}")).await;

    let shared = storage.share_common_rooms_batch(&user_a, &[]).await.unwrap();
    assert!(shared.is_empty(), "empty other_user_ids should short-circuit to empty");
}

#[tokio::test]
async fn test_share_common_rooms_batch_partial_match() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_a = format!("@scbpart_a_{suffix}:localhost");
    let user_b = format!("@scbpart_b_{suffix}:localhost");
    let user_c = format!("@scbpart_c_{suffix}:localhost");
    let room1 = format!("!scbpart1_{suffix}:localhost");
    // a and b share room1; c is in a different room.
    let room2 = format!("!scbpart2_{suffix}:localhost");
    insert_user(&pool, &user_a, &format!("scbpart_a_{suffix}")).await;
    insert_user(&pool, &user_b, &format!("scbpart_b_{suffix}")).await;
    insert_user(&pool, &user_c, &format!("scbpart_c_{suffix}")).await;
    insert_room(&pool, &room1).await;
    insert_room(&pool, &room2).await;
    storage.add_member(&room1, &user_a, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room1, &user_b, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room2, &user_c, "join", None, None, None, None).await.unwrap();

    let shared =
        storage.share_common_rooms_batch(&user_a, &[user_b.clone(), user_c.clone()]).await.unwrap();
    assert_eq!(shared.len(), 1);
    assert!(shared.contains(&user_b));
    assert!(!shared.contains(&user_c));
}

#[tokio::test]
async fn test_share_common_rooms_batch_no_match_when_user_left() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_a = format!("@scbnm_a_{suffix}:localhost");
    let user_b = format!("@scbnm_b_{suffix}:localhost");
    let room_id = format!("!scbnm_room_{suffix}:localhost");
    insert_user(&pool, &user_a, &format!("scbnm_a_{suffix}")).await;
    insert_user(&pool, &user_b, &format!("scbnm_b_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    // a left, b joined => no common joined room.
    storage.add_member(&room_id, &user_a, "leave", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_b, "join", None, None, None, None).await.unwrap();

    let shared = storage.share_common_rooms_batch(&user_a, std::slice::from_ref(&user_b)).await.unwrap();
    assert!(shared.is_empty(), "left member should not share a joined room");
}

// =============================================================================
// user_shares_room_with_server (federation)
// =============================================================================

#[tokio::test]
async fn test_user_shares_room_with_server_true() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let local_user = format!("@local_{suffix}:localhost");
    let remote_user = format!("@remote_{suffix}:remote.example");
    let room_id = format!("!usws_room_{suffix}:localhost");
    insert_user(&pool, &local_user, &format!("local_{suffix}")).await;
    insert_user(&pool, &remote_user, &format!("remote_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &local_user, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &remote_user, "join", None, None, None, None).await.unwrap();

    let shares = storage.user_shares_room_with_server(&local_user, "remote.example").await.unwrap();
    assert!(shares, "local user shares a room with a remote.example member");
}

#[tokio::test]
async fn test_user_shares_room_with_server_false_different_domain() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let local_user = format!("@local2_{suffix}:localhost");
    let other_user = format!("@other2_{suffix}:other.example");
    let room_id = format!("!usws2_room_{suffix}:localhost");
    insert_user(&pool, &local_user, &format!("local2_{suffix}")).await;
    insert_user(&pool, &other_user, &format!("other2_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &local_user, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &other_user, "join", None, None, None, None).await.unwrap();

    let shares = storage.user_shares_room_with_server(&local_user, "remote.example").await.unwrap();
    assert!(!shares, "no remote.example member in the shared room");
}

#[tokio::test]
async fn test_user_shares_room_with_server_user_not_joined() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let local_user = format!("@local3_{suffix}:localhost");
    let remote_user = format!("@remote3_{suffix}:remote.example");
    let room_id = format!("!usws3_room_{suffix}:localhost");
    insert_user(&pool, &local_user, &format!("local3_{suffix}")).await;
    insert_user(&pool, &remote_user, &format!("remote3_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    // local user left; remote user joined.
    storage.add_member(&room_id, &local_user, "leave", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &remote_user, "join", None, None, None, None).await.unwrap();

    let shares = storage.user_shares_room_with_server(&local_user, "remote.example").await.unwrap();
    assert!(!shares, "left user does not share a joined room");
}

#[tokio::test]
async fn test_user_shares_room_with_server_no_rooms() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let local_user = format!("@local4_{suffix}:localhost");
    insert_user(&pool, &local_user, &format!("local4_{suffix}")).await;

    let shares = storage.user_shares_room_with_server(&local_user, "remote.example").await.unwrap();
    assert!(!shares, "user with no joined rooms cannot share");
}

// =============================================================================
// filter_users_sharing_room_with_server (federation batch)
// =============================================================================

#[tokio::test]
async fn test_filter_users_sharing_room_with_server_basic() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let local_a = format!("@fusrv_a_{suffix}:localhost");
    let local_b = format!("@fusrv_b_{suffix}:localhost");
    let local_c = format!("@fusrv_c_{suffix}:localhost");
    let remote_user = format!("@fusrv_remote_{suffix}:remote.example");
    let room_id = format!("!fusrv_room_{suffix}:localhost");
    for (uid, uname) in [
        (&local_a, "fusrv_a"),
        (&local_b, "fusrv_b"),
        (&local_c, "fusrv_c"),
        (&remote_user, "fusrv_remote"),
    ] {
        insert_user(&pool, uid, &format!("{uname}_{suffix}")).await;
    }
    insert_room(&pool, &room_id).await;
    // a and b share a room with the remote user; c is in a separate room.
    storage.add_member(&room_id, &local_a, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &local_b, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &remote_user, "join", None, None, None, None).await.unwrap();
    let other_room = format!("!fusrv_other_{suffix}:localhost");
    insert_room(&pool, &other_room).await;
    storage.add_member(&other_room, &local_c, "join", None, None, None, None).await.unwrap();

    let filtered = storage
        .filter_users_sharing_room_with_server(&[local_a.clone(), local_b.clone(), local_c.clone()], "remote.example")
        .await
        .unwrap();
    assert_eq!(filtered.len(), 2);
    assert!(filtered.contains(&local_a));
    assert!(filtered.contains(&local_b));
    assert!(!filtered.contains(&local_c));
}

#[tokio::test]
async fn test_filter_users_sharing_room_with_server_empty_input() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);

    let filtered = storage.filter_users_sharing_room_with_server(&[], "remote.example").await.unwrap();
    assert!(filtered.is_empty(), "empty input should short-circuit to empty");
}

#[tokio::test]
async fn test_filter_users_sharing_room_with_server_none_match() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let local_a = format!("@fusrvnm_a_{suffix}:localhost");
    insert_user(&pool, &local_a, &format!("fusrvnm_a_{suffix}")).await;

    let filtered =
        storage.filter_users_sharing_room_with_server(std::slice::from_ref(&local_a), "remote.example").await.unwrap();
    assert!(filtered.is_empty(), "user with no joined rooms should not match");
}

#[tokio::test]
async fn test_filter_users_sharing_room_with_server_all_match() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let local_a = format!("@fusrvall_a_{suffix}:localhost");
    let local_b = format!("@fusrvall_b_{suffix}:localhost");
    let remote_user = format!("@fusrvall_remote_{suffix}:remote.example");
    let room_id = format!("!fusrvall_room_{suffix}:localhost");
    for (uid, uname) in [(&local_a, "fusrvall_a"), (&local_b, "fusrvall_b"), (&remote_user, "fusrvall_remote")] {
        insert_user(&pool, uid, &format!("{uname}_{suffix}")).await;
    }
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &local_a, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &local_b, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &remote_user, "join", None, None, None, None).await.unwrap();

    let filtered = storage
        .filter_users_sharing_room_with_server(&[local_a.clone(), local_b.clone()], "remote.example")
        .await
        .unwrap();
    assert_eq!(filtered.len(), 2);
    assert!(filtered.contains(&local_a));
    assert!(filtered.contains(&local_b));
}

// =============================================================================
// set_ban_reason
// =============================================================================

#[tokio::test]
async fn test_set_ban_reason_on_banned_member() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@banreason_{suffix}:localhost");
    let room_id = format!("!banreason_room_{suffix}:localhost");
    let banner = format!("@banner_br_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("banreason_{suffix}")).await;
    insert_user(&pool, &banner, &format!("banner_br_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.ban_member(&room_id, &user_id, &banner).await.unwrap();

    storage.set_ban_reason(&room_id, &user_id, "Spam abuse").await.unwrap();

    let member = storage.get_member(&room_id, &user_id).await.unwrap().unwrap();
    assert_eq!(member.ban_reason.as_deref(), Some("Spam abuse"));
}

#[tokio::test]
async fn test_set_ban_reason_overwrite() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@banreason2_{suffix}:localhost");
    let room_id = format!("!banreason2_room_{suffix}:localhost");
    let banner = format!("@banner_br2_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("banreason2_{suffix}")).await;
    insert_user(&pool, &banner, &format!("banner_br2_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.ban_member(&room_id, &user_id, &banner).await.unwrap();
    storage.set_ban_reason(&room_id, &user_id, "First reason").await.unwrap();

    storage.set_ban_reason(&room_id, &user_id, "Second reason").await.unwrap();

    let member = storage.get_member(&room_id, &user_id).await.unwrap().unwrap();
    assert_eq!(member.ban_reason.as_deref(), Some("Second reason"));
}

#[tokio::test]
async fn test_set_ban_reason_on_nonexistent_member_no_error() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();

    // UPDATE with no matching row affects 0 rows but does not error.
    let result = storage
        .set_ban_reason(&format!("!noban_room_{suffix}:localhost"), &format!("@noban_{suffix}:localhost"), "reason")
        .await;
    assert!(result.is_ok(), "set_ban_reason on a nonexistent member should not error");
}

// =============================================================================
// force_leave_membership
// =============================================================================

#[tokio::test]
async fn test_force_leave_membership_sets_timestamp() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@forceleave_{suffix}:localhost");
    let room_id = format!("!forceleave_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("forceleave_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();

    let now = chrono::Utc::now().timestamp_millis();
    storage.force_leave_membership(&room_id, &user_id, now).await.unwrap();

    let member = storage.get_member(&room_id, &user_id).await.unwrap().unwrap();
    assert_eq!(member.membership, "leave");
    assert_eq!(member.left_ts, Some(now));
    assert_eq!(member.updated_ts, Some(now));
}

#[tokio::test]
async fn test_force_leave_membership_specific_timestamp() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@forceleave2_{suffix}:localhost");
    let room_id = format!("!forceleave2_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("forceleave2_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();

    let fixed_ts: i64 = 1_700_000_000_000;
    storage.force_leave_membership(&room_id, &user_id, fixed_ts).await.unwrap();

    let member = storage.get_member(&room_id, &user_id).await.unwrap().unwrap();
    assert_eq!(member.left_ts, Some(fixed_ts), "force_leave must use the provided timestamp exactly");
}

#[tokio::test]
async fn test_force_leave_membership_from_ban() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@forceleave3_{suffix}:localhost");
    let room_id = format!("!forceleave3_room_{suffix}:localhost");
    let banner = format!("@fl_banner_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("forceleave3_{suffix}")).await;
    insert_user(&pool, &banner, &format!("fl_banner_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.ban_member(&room_id, &user_id, &banner).await.unwrap();

    let now = chrono::Utc::now().timestamp_millis();
    storage.force_leave_membership(&room_id, &user_id, now).await.unwrap();

    let member = storage.get_member(&room_id, &user_id).await.unwrap().unwrap();
    assert_eq!(member.membership, "leave", "force_leave overrides ban membership");
}

#[tokio::test]
async fn test_force_leave_membership_nonexistent_no_error() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let now = chrono::Utc::now().timestamp_millis();

    let result =
        storage.force_leave_membership(&format!("!nofl_room_{suffix}:localhost"), &format!("@nofl_{suffix}:localhost"), now).await;
    assert!(result.is_ok(), "force_leave on a nonexistent member should not error");
}

// =============================================================================
// get_joined_servers_in_room (federation backfill candidate list)
// =============================================================================

#[tokio::test]
async fn test_get_joined_servers_in_room_multiple() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!jsrv_room_{suffix}:localhost");
    let user_a = format!("@jsrv_a_{suffix}:alpha.example");
    let user_b = format!("@jsrv_b_{suffix}:beta.example");
    let user_c = format!("@jsrv_c_{suffix}:localhost");
    let user_d = format!("@jsrv_d_{suffix}:alpha.example");
    for (uid, uname) in [
        (&user_a, "jsrv_a"),
        (&user_b, "jsrv_b"),
        (&user_c, "jsrv_c"),
        (&user_d, "jsrv_d"),
    ] {
        insert_user(&pool, uid, &format!("{uname}_{suffix}")).await;
    }
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_b, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_c, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_d, "join", None, None, None, None).await.unwrap();

    let servers = storage.get_joined_servers_in_room(&room_id, "localhost").await.unwrap();
    // alpha and beta; localhost excluded.
    assert!(servers.contains(&"alpha.example".to_string()));
    assert!(servers.contains(&"beta.example".to_string()));
    assert!(!servers.contains(&"localhost".to_string()));
    assert_eq!(servers.len(), 2, "distinct remote servers only");
}

#[tokio::test]
async fn test_get_joined_servers_in_room_excludes_local() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!jsrvexcl_room_{suffix}:localhost");
    let local_user = format!("@jsrvexcl_local_{suffix}:localhost");
    let remote_user = format!("@jsrvexcl_remote_{suffix}:remote.example");
    insert_user(&pool, &local_user, &format!("jsrvexcl_local_{suffix}")).await;
    insert_user(&pool, &remote_user, &format!("jsrvexcl_remote_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &local_user, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &remote_user, "join", None, None, None, None).await.unwrap();

    let servers = storage.get_joined_servers_in_room(&room_id, "localhost").await.unwrap();
    assert_eq!(servers, vec!["remote.example".to_string()]);
}

#[tokio::test]
async fn test_get_joined_servers_in_room_empty() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!jsrvempty_room_{suffix}:localhost");
    insert_room(&pool, &room_id).await;

    let servers = storage.get_joined_servers_in_room(&room_id, "localhost").await.unwrap();
    assert!(servers.is_empty());
}

#[tokio::test]
async fn test_get_joined_servers_in_room_only_local_members() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!jsrvlocal_room_{suffix}:localhost");
    let local_a = format!("@jsrvlocal_a_{suffix}:localhost");
    let local_b = format!("@jsrvlocal_b_{suffix}:localhost");
    insert_user(&pool, &local_a, &format!("jsrvlocal_a_{suffix}")).await;
    insert_user(&pool, &local_b, &format!("jsrvlocal_b_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &local_a, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &local_b, "join", None, None, None, None).await.unwrap();

    let servers = storage.get_joined_servers_in_room(&room_id, "localhost").await.unwrap();
    assert!(servers.is_empty(), "only-local room should yield no remote servers");
}

#[tokio::test]
async fn test_get_joined_servers_in_room_excludes_left() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!jsrvleft_room_{suffix}:localhost");
    let leaver = format!("@jsrvleft_{suffix}:remote.example");
    insert_user(&pool, &leaver, &format!("jsrvleft_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &leaver, "leave", None, None, None, None).await.unwrap();

    let servers = storage.get_joined_servers_in_room(&room_id, "localhost").await.unwrap();
    assert!(servers.is_empty(), "left members must be excluded from joined-server list");
}

// =============================================================================
// UserRoomMembership struct round-trip
// =============================================================================

#[tokio::test]
async fn test_user_room_membership_struct_round_trip() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let m = UserRoomMembership { room_id: "!r:localhost".to_string(), membership: "join".to_string() };
    let json = serde_json::to_string(&m).unwrap();
    let back: UserRoomMembership = serde_json::from_str(&json).unwrap();
    assert_eq!(back.room_id, m.room_id);
    assert_eq!(back.membership, m.membership);
}

// =============================================================================
// Repository trait dispatch (ensures the trait impl compiles & delegates)
// =============================================================================

#[tokio::test]
async fn test_repository_trait_pool_accessor() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let repo: &dyn RoomMemberRepository = &storage;
    // pool() returns a reference to the same Arc.
    let repo_pool = repo.pool();
    assert!(Arc::ptr_eq(repo_pool, &pool));
}

#[tokio::test]
async fn test_repository_trait_add_member_delegates() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let repo: &dyn RoomMemberRepository = &storage;
    let suffix = unique_id();
    let user_id = format!("@traitadd_{suffix}:localhost");
    let room_id = format!("!traitadd_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("traitadd_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    let member = repo
        .add_member(&room_id, &user_id, "join", Some("Trait"), None, None, None)
        .await
        .unwrap();
    assert_eq!(member.user_id, user_id);
    assert_eq!(member.display_name.as_deref(), Some("Trait"));
}

#[tokio::test]
async fn test_repository_trait_pagination_delegates() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let repo: &dyn RoomMemberRepository = &storage;
    let suffix = unique_id();
    let room_id = format!("!traitpag_room_{suffix}:localhost");
    insert_room(&pool, &room_id).await;
    let user_id = format!("@traitpag_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("traitpag_{suffix}")).await;
    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();

    let page = repo.get_room_members_paginated(&room_id, "join", 10, None).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].user_id, user_id);
}

#[tokio::test]
async fn test_repository_trait_batch_methods_delegates() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let repo: &dyn RoomMemberRepository = &storage;
    let suffix = unique_id();
    let room1 = format!("!traitbatch1_{suffix}:localhost");
    let room2 = format!("!traitbatch2_{suffix}:localhost");
    let user1 = format!("@traitbatch1_{suffix}:localhost");
    let user2 = format!("@traitbatch2_{suffix}:localhost");
    insert_user(&pool, &user1, &format!("traitbatch1_{suffix}")).await;
    insert_user(&pool, &user2, &format!("traitbatch2_{suffix}")).await;
    insert_room(&pool, &room1).await;
    insert_room(&pool, &room2).await;
    storage.add_member(&room1, &user1, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room2, &user2, "join", None, None, None, None).await.unwrap();

    let batch = repo.get_members_batch(&[room1.clone(), room2.clone()], "join").await.unwrap();
    assert_eq!(batch[&room1].len(), 1);
    assert_eq!(batch[&room2].len(), 1);

    let joined_batch = repo.get_joined_members_batch(&[room1.clone(), room2]).await.unwrap();
    // Both rooms have one joined member each.
    assert_eq!(joined_batch.values().map(|v| v.len()).sum::<usize>(), 2);

    let checked = repo.check_membership_batch(&room1, &[user1.clone()], "join").await.unwrap();
    assert!(checked.contains(&user1));
}

#[tokio::test]
async fn test_repository_trait_federation_methods_delegates() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let repo: &dyn RoomMemberRepository = &storage;
    let suffix = unique_id();
    let local_user = format!("@traitfed_{suffix}:localhost");
    let remote_user = format!("@traitfed_remote_{suffix}:remote.example");
    let room_id = format!("!traitfed_room_{suffix}:localhost");
    insert_user(&pool, &local_user, &format!("traitfed_{suffix}")).await;
    insert_user(&pool, &remote_user, &format!("traitfed_remote_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &local_user, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &remote_user, "join", None, None, None, None).await.unwrap();

    let has = repo.has_any_non_banned_member_from_server(&room_id, "remote.example").await.unwrap();
    assert!(has);

    let shares = repo.user_shares_room_with_server(&local_user, "remote.example").await.unwrap();
    assert!(shares);

    let filtered =
        repo.filter_users_sharing_room_with_server(std::slice::from_ref(&local_user), "remote.example").await.unwrap();
    assert!(filtered.contains(&local_user));

    let servers = repo.get_joined_servers_in_room(&room_id, "localhost").await.unwrap();
    assert!(servers.contains(&"remote.example".to_string()));
}

#[tokio::test]
async fn test_repository_trait_ban_and_force_leave_delegates() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let repo: &dyn RoomMemberRepository = &storage;
    let suffix = unique_id();
    let user_id = format!("@traitban_{suffix}:localhost");
    let banner = format!("@traitbanner_{suffix}:localhost");
    let room_id = format!("!traitban_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("traitban_{suffix}")).await;
    insert_user(&pool, &banner, &format!("traitbanner_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();

    repo.ban_member(&room_id, &user_id, &banner).await.unwrap();
    assert_eq!(repo.get_membership_state(&room_id, &user_id).await.unwrap().as_deref(), Some("ban"));

    repo.set_ban_reason(&room_id, &user_id, "trait reason").await.unwrap();
    let member = repo.get_member(&room_id, &user_id).await.unwrap().unwrap();
    assert_eq!(member.ban_reason.as_deref(), Some("trait reason"));

    repo.unban_member(&room_id, &user_id).await.unwrap();
    assert_eq!(repo.get_membership_state(&room_id, &user_id).await.unwrap().as_deref(), Some("leave"));

    let now = chrono::Utc::now().timestamp_millis();
    repo.force_leave_membership(&room_id, &user_id, now).await.unwrap();
    let member = repo.get_member(&room_id, &user_id).await.unwrap().unwrap();
    assert_eq!(member.left_ts, Some(now));
}

// =============================================================================
// Cross-room membership edge cases
// =============================================================================

#[tokio::test]
async fn test_get_joined_rooms_excludes_forgotten() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@forgotten_{suffix}:localhost");
    let joined = format!("!forgotten_j_{suffix}:localhost");
    let forgotten = format!("!forgotten_f_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("forgotten_{suffix}")).await;
    insert_room(&pool, &joined).await;
    insert_room(&pool, &forgotten).await;
    storage.add_member(&joined, &user_id, "join", None, None, None, None).await.unwrap();
    storage.add_member(&forgotten, &user_id, "leave", None, None, None, None).await.unwrap();
    storage.forget_member(&forgotten, &user_id).await.unwrap();

    let rooms = storage.get_joined_rooms(&user_id).await.unwrap();
    assert!(rooms.contains(&joined));
    assert!(!rooms.contains(&forgotten));
}

#[tokio::test]
async fn test_get_sync_rooms_excludes_forgotten_even_with_include_leave() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@syncforgotten_{suffix}:localhost");
    let joined = format!("!syncforgotten_j_{suffix}:localhost");
    let forgotten = format!("!syncforgotten_f_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("syncforgotten_{suffix}")).await;
    insert_room(&pool, &joined).await;
    insert_room(&pool, &forgotten).await;
    storage.add_member(&joined, &user_id, "join", None, None, None, None).await.unwrap();
    storage.add_member(&forgotten, &user_id, "leave", None, None, None, None).await.unwrap();
    storage.forget_member(&forgotten, &user_id).await.unwrap();

    let memberships = storage.get_sync_rooms(&user_id, true).await.unwrap();
    let room_ids: Vec<&str> = memberships.iter().map(|m| m.room_id.as_str()).collect();
    assert!(room_ids.contains(&joined.as_str()));
    assert!(!room_ids.contains(&forgotten.as_str()), "forgotten rooms must be excluded from sync");
}

#[tokio::test]
async fn test_get_shared_room_users_excludes_left() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_a = format!("@sharexcl_a_{suffix}:localhost");
    let user_b = format!("@sharexcl_b_{suffix}:localhost");
    let user_c = format!("@sharexcl_c_{suffix}:localhost");
    let room_id = format!("!sharexcl_room_{suffix}:localhost");
    for (uid, uname) in [(&user_a, "sharexcl_a"), (&user_b, "sharexcl_b"), (&user_c, "sharexcl_c")] {
        insert_user(&pool, uid, &format!("{uname}_{suffix}")).await;
    }
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_b, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_c, "leave", None, None, None, None).await.unwrap();

    let shared = storage.get_shared_room_users(&user_a).await.unwrap();
    assert!(shared.contains(&user_b));
    assert!(!shared.contains(&user_c), "left members must not appear in shared-room users");
}

#[tokio::test]
async fn test_get_membership_history_orders_by_updated_ts_desc() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!historder_room_{suffix}:localhost");
    let user1 = format!("@hist1_{suffix}:localhost");
    let user2 = format!("@hist2_{suffix}:localhost");
    insert_user(&pool, &user1, &format!("hist1_{suffix}")).await;
    insert_user(&pool, &user2, &format!("hist2_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage.add_member(&room_id, &user1, "join", None, None, None, None).await.unwrap();
    // Small delay so user2 has a strictly-later updated_ts.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    storage.add_member(&room_id, &user2, "join", None, None, None, None).await.unwrap();

    let history = storage.get_membership_history(&room_id, 10).await.unwrap();
    assert_eq!(history.len(), 2);
    // DESC by updated_ts: user2 (later) should come first.
    assert!(history[0].updated_ts >= history[1].updated_ts);
}

#[tokio::test]
async fn test_remove_member_invite() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@rmvinv_{suffix}:localhost");
    let room_id = format!("!rmvinv_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("rmvinv_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.add_member(&room_id, &user_id, "invite", None, None, None, None).await.unwrap();

    storage.remove_member(&room_id, &user_id).await.unwrap();
    let member = storage.get_member(&room_id, &user_id).await.unwrap().unwrap();
    assert_eq!(member.membership, "leave", "remove_member transitions invite -> leave");
    assert!(member.left_ts.is_some());
}

#[tokio::test]
async fn test_remove_member_does_not_affect_already_forgotten() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@rmvforg_{suffix}:localhost");
    let room_id = format!("!rmvforg_room_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("rmvforg_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    insert_membership_direct(&pool, &room_id, &user_id, "leave").await;
    storage.forget_member(&room_id, &user_id).await.unwrap();

    // remove_member only affects membership IN ('join', 'ban', 'invite'); 'forget' is untouched.
    storage.remove_member(&room_id, &user_id).await.unwrap();
    let member = storage.get_member(&room_id, &user_id).await.unwrap().unwrap();
    assert_eq!(member.membership, "forget", "forget membership must survive remove_member");
}

#[tokio::test]
async fn test_forget_member_banned_no_effect() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@forgban_{suffix}:localhost");
    let room_id = format!("!forgban_room_{suffix}:localhost");
    let banner = format!("@forgbanner_{suffix}:localhost");
    insert_user(&pool, &user_id, &format!("forgban_{suffix}")).await;
    insert_user(&pool, &banner, &format!("forgbanner_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage.ban_member(&room_id, &user_id, &banner).await.unwrap();

    // forget_member only affects membership IN ('leave', 'invite'); 'ban' is untouched.
    storage.forget_member(&room_id, &user_id).await.unwrap();
    let member = storage.get_member(&room_id, &user_id).await.unwrap().unwrap();
    assert_eq!(member.membership, "ban", "ban membership must survive forget_member");
}

#[tokio::test]
async fn test_room_member_struct_serde_round_trip() {
    let _guard = membership_test_guard().lock().unwrap_or_else(|e| e.into_inner());
    let member = RoomMember {
        room_id: "!serde:localhost".to_string(),
        user_id: "@serde:localhost".to_string(),
        sender: Some("@serde:localhost".to_string()),
        membership: "join".to_string(),
        event_id: Some("$serde:localhost".to_string()),
        event_type: Some("m.room.member".to_string()),
        display_name: Some("Serde".to_string()),
        avatar_url: Some("mxc://localhost/serde".to_string()),
        is_banned: Some(false),
        invite_token: None,
        updated_ts: Some(1_700_000_000_000),
        joined_ts: Some(1_700_000_000_000),
        left_ts: None,
        reason: None,
        banned_by: None,
        ban_reason: None,
        banned_ts: None,
        join_reason: Some("test".to_string()),
    };
    let json = serde_json::to_string(&member).unwrap();
    let back: RoomMember = serde_json::from_str(&json).unwrap();
    assert_eq!(back.room_id, member.room_id);
    assert_eq!(back.user_id, member.user_id);
    assert_eq!(back.membership, member.membership);
    assert_eq!(back.join_reason, member.join_reason);
    assert_eq!(back.is_banned, member.is_banned);
}
