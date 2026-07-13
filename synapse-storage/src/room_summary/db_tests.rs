#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::env;
use std::sync::Arc;

async fn test_pool() -> Arc<PgPool> {
    let db_url = env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
    let pool =
        PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
    Arc::new(pool)
}

async fn ensure_test_user(pool: &PgPool, user_id: &str) {
    let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
    sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .execute(pool)
        .await
        .ok();
}

async fn ensure_test_room(pool: &PgPool, room_id: &str) {
    sqlx::query(
            "INSERT INTO rooms (room_id, room_version, is_public, creator, created_ts) VALUES ($1, '1', false, '@test:localhost', EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (room_id) DO NOTHING",
        )
        .bind(room_id)
        .execute(pool)
        .await
        .ok();
}

/// Delete test data from all room-summary tables in FK-safe order.
async fn cleanup_summary_data(pool: &PgPool, suffix: &str) {
    let pattern = format!("%{suffix}");
    // state depends on rooms
    let _ = sqlx::query("DELETE FROM room_summary_state WHERE room_id LIKE $1").bind(&pattern).execute(pool).await;
    // members depends on rooms + users
    let _ = sqlx::query("DELETE FROM room_summary_members WHERE room_id LIKE $1").bind(&pattern).execute(pool).await;
    // stats depends on rooms
    let _ = sqlx::query("DELETE FROM room_summary_stats WHERE room_id LIKE $1").bind(&pattern).execute(pool).await;
    // queue depends on rooms
    let _ =
        sqlx::query("DELETE FROM room_summary_update_queue WHERE room_id LIKE $1").bind(&pattern).execute(pool).await;
    // summaries depends on rooms
    let _ = sqlx::query("DELETE FROM room_summaries WHERE room_id LIKE $1").bind(&pattern).execute(pool).await;
    // FK-parents: clean test rooms and users last
    let _ = sqlx::query("DELETE FROM rooms WHERE room_id LIKE $1").bind(&pattern).execute(pool).await;
    let _ = sqlx::query("DELETE FROM users WHERE user_id LIKE $1").bind(&pattern).execute(pool).await;
}

fn make_suffix() -> String {
    uuid::Uuid::new_v4().to_string().replace('-', "")
}

// ── create_summary ──────────────────────────────────────────────

#[tokio::test]
async fn test_create_summary_with_all_fields() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_cs_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    let request = CreateRoomSummaryRequest {
        room_id: room_id.clone(),
        room_type: Some("m.space".to_string()),
        name: Some("My Test Room".to_string()),
        topic: Some("Testing room summary".to_string()),
        avatar_url: Some("mxc://test.org/avatar".to_string()),
        canonical_alias: Some("#myroom:localhost".to_string()),
        join_rule: Some("invite".to_string()),
        history_visibility: Some("joined".to_string()),
        guest_access: Some("can_join".to_string()),
        is_direct: Some(false),
        is_space: Some(true),
    };

    let summary = storage.create_summary(request).await.unwrap();

    assert!(summary.id.is_some());
    assert_eq!(summary.room_id, room_id);
    assert_eq!(summary.room_type.as_deref(), Some("m.space"));
    assert_eq!(summary.name.as_deref(), Some("My Test Room"));
    assert_eq!(summary.topic.as_deref(), Some("Testing room summary"));
    assert_eq!(summary.avatar_url.as_deref(), Some("mxc://test.org/avatar"));
    assert_eq!(summary.canonical_alias.as_deref(), Some("#myroom:localhost"));
    assert_eq!(summary.join_rule, "invite");
    assert_eq!(summary.history_visibility, "joined");
    assert_eq!(summary.guest_access, "can_join");
    assert!(!summary.is_direct);
    assert!(summary.is_space);
    assert!(!summary.is_encrypted);
    assert_eq!(summary.member_count, Some(0));
    assert_eq!(summary.joined_member_count, Some(0));
    assert_eq!(summary.invited_member_count, Some(0));
    assert!(summary.created_ts.is_some());
    assert_eq!(summary.updated_ts, summary.created_ts);

    cleanup_summary_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_create_summary_default_values() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_csd_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    let request = CreateRoomSummaryRequest {
        room_id: room_id.clone(),
        room_type: None,
        name: None,
        topic: None,
        avatar_url: None,
        canonical_alias: None,
        join_rule: None,
        history_visibility: None,
        guest_access: None,
        is_direct: None,
        is_space: None,
    };

    let summary = storage.create_summary(request).await.unwrap();

    assert_eq!(summary.room_id, room_id);
    assert_eq!(summary.join_rule, "invite");
    assert_eq!(summary.history_visibility, "shared");
    assert_eq!(summary.guest_access, "forbidden");
    assert!(!summary.is_direct);
    assert!(!summary.is_space);
    assert_eq!(summary.unread_notifications, 0);
    assert_eq!(summary.unread_highlight, 0);

    cleanup_summary_data(&pool, &suffix).await;
}

// ── get_summary ─────────────────────────────────────────────────

#[tokio::test]
async fn test_get_summary_found() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_gs_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: Some("Found Me".to_string()),
            topic: None,
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

    let found = storage.get_summary(&room_id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().name.as_deref(), Some("Found Me"));

    cleanup_summary_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_summary_not_found() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let nonexistent = format!("!nonexistent_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;

    let storage = RoomSummaryStorage::new(&pool);
    let result = storage.get_summary(&nonexistent).await.unwrap();
    assert!(result.is_none());

    cleanup_summary_data(&pool, &suffix).await;
}

// ── update_summary ──────────────────────────────────────────────

#[tokio::test]
async fn test_update_summary_updates_fields() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_us_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: Some("Original".to_string()),
            topic: None,
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

    let updated = storage
        .update_summary(
            &room_id,
            UpdateRoomSummaryRequest {
                name: Some("Updated Name".to_string()),
                topic: Some("Updated Topic".to_string()),
                is_encrypted: Some(true),
                last_event_ts: Some(1_700_000_000_000i64),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(updated.name.as_deref(), Some("Updated Name"));
    assert_eq!(updated.topic.as_deref(), Some("Updated Topic"));
    assert!(updated.is_encrypted);
    assert_eq!(updated.last_event_ts, Some(1_700_000_000_000i64));

    cleanup_summary_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_update_summary_keeps_unchanged_fields() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_usk_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: Some("m.room".to_string()),
            name: Some("Keep".to_string()),
            topic: Some("Keep topic".to_string()),
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

    let updated = storage
        .update_summary(&room_id, UpdateRoomSummaryRequest { name: Some("New Name".to_string()), ..Default::default() })
        .await
        .unwrap();

    assert_eq!(updated.name.as_deref(), Some("New Name"));
    // unchanged fields should stay
    assert_eq!(updated.topic.as_deref(), Some("Keep topic"));
    assert_eq!(updated.room_type.as_deref(), Some("m.room"));

    cleanup_summary_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_update_summary_not_found() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_usnf_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;

    let storage = RoomSummaryStorage::new(&pool);
    let result = storage
        .update_summary(&room_id, UpdateRoomSummaryRequest { name: Some("Ghost".to_string()), ..Default::default() })
        .await;

    assert!(result.is_err());

    cleanup_summary_data(&pool, &suffix).await;
}

// ── set_canonical_alias ─────────────────────────────────────────

#[tokio::test]
async fn test_set_canonical_alias_sets_and_clears() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_sca_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: None,
            topic: None,
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

    // Set alias
    let summary = storage.set_canonical_alias(&room_id, Some("#testalias:localhost")).await.unwrap();
    assert_eq!(summary.canonical_alias.as_deref(), Some("#testalias:localhost"));

    // Clear alias
    let summary = storage.set_canonical_alias(&room_id, None).await.unwrap();
    assert!(summary.canonical_alias.is_none());

    cleanup_summary_data(&pool, &suffix).await;
}

// ── delete_summary ──────────────────────────────────────────────

#[tokio::test]
async fn test_delete_summary_removes_record() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_ds_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: None,
            topic: None,
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

    storage.delete_summary(&room_id).await.unwrap();
    let result = storage.get_summary(&room_id).await.unwrap();
    assert!(result.is_none());

    // Idempotent — second delete should not error
    storage.delete_summary(&room_id).await.unwrap();

    cleanup_summary_data(&pool, &suffix).await;
}

// ── get_summaries_by_ids ────────────────────────────────────────

#[tokio::test]
async fn test_get_summaries_by_ids_multiple() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_a = format!("!rs_gsi_a_{suffix}:localhost");
    let room_b = format!("!rs_gsi_b_{suffix}:localhost");
    let room_c = format!("!rs_gsi_c_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_a).await;
    ensure_test_room(&pool, &room_b).await;
    ensure_test_room(&pool, &room_c).await;

    let storage = RoomSummaryStorage::new(&pool);
    let req = |id: &str| CreateRoomSummaryRequest {
        room_id: id.to_string(),
        room_type: None,
        name: None,
        topic: None,
        avatar_url: None,
        canonical_alias: None,
        join_rule: None,
        history_visibility: None,
        guest_access: None,
        is_direct: None,
        is_space: None,
    };
    storage.create_summary(req(&room_a)).await.unwrap();
    storage.create_summary(req(&room_b)).await.unwrap();
    storage.create_summary(req(&room_c)).await.unwrap();

    let ids = vec![room_a.clone(), room_b.clone(), room_c.clone()];
    let results = storage.get_summaries_by_ids(&ids).await.unwrap();
    assert_eq!(results.len(), 3);

    // Partial — one matching
    let partial = storage.get_summaries_by_ids(&[room_a.clone()]).await.unwrap();
    assert_eq!(partial.len(), 1);
    assert_eq!(partial[0].room_id, room_a);

    // Empty input
    let empty: Vec<String> = vec![];
    let results = storage.get_summaries_by_ids(&empty).await.unwrap();
    assert!(results.is_empty());

    cleanup_summary_data(&pool, &suffix).await;
}

// ── get_summaries_for_user ──────────────────────────────────────

#[tokio::test]
async fn test_get_summaries_for_user_returns_joined_rooms() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_a = format!("!rs_gsu_a_{suffix}:localhost");
    let room_b = format!("!rs_gsu_b_{suffix}:localhost");
    let user_id = format!("@rs_gsu_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_a).await;
    ensure_test_room(&pool, &room_b).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    let req = |id: &str| CreateRoomSummaryRequest {
        room_id: id.to_string(),
        room_type: None,
        name: None,
        topic: None,
        avatar_url: None,
        canonical_alias: None,
        join_rule: None,
        history_visibility: None,
        guest_access: None,
        is_direct: None,
        is_space: None,
    };
    storage.create_summary(req(&room_a)).await.unwrap();
    storage.create_summary(req(&room_b)).await.unwrap();

    // Add user to room_a only
    storage
        .add_member(CreateSummaryMemberRequest {
            room_id: room_a.clone(),
            user_id: user_id.clone(),
            display_name: Some("TestUser".to_string()),
            avatar_url: None,
            membership: "join".to_string(),
            is_hero: None,
            last_active_ts: None,
        })
        .await
        .unwrap();

    let summaries = storage.get_summaries_for_user(&user_id).await.unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].room_id, room_a);

    cleanup_summary_data(&pool, &suffix).await;
}

// ── add_member ──────────────────────────────────────────────────

#[tokio::test]
async fn test_add_member_creates_record() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_am_{suffix}:localhost");
    let user_id = format!("@rs_am_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: None,
            topic: None,
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

    let member = storage
        .add_member(CreateSummaryMemberRequest {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            display_name: Some("Alice".to_string()),
            avatar_url: Some("mxc://alice".to_string()),
            membership: "join".to_string(),
            is_hero: Some(true),
            last_active_ts: Some(1_700_000_000_000i64),
        })
        .await
        .unwrap();

    assert_eq!(member.room_id, room_id);
    assert_eq!(member.user_id, user_id);
    assert_eq!(member.display_name.as_deref(), Some("Alice"));
    assert_eq!(member.avatar_url.as_deref(), Some("mxc://alice"));
    assert_eq!(member.membership, "join");
    assert!(member.is_hero);

    // Verify member counts were refreshed
    let summary = storage.get_summary(&room_id).await.unwrap().unwrap();
    assert_eq!(summary.member_count, Some(1));
    assert_eq!(summary.joined_member_count, Some(1));
    assert_eq!(summary.invited_member_count, Some(0));

    cleanup_summary_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_add_member_duplicate_upserts() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_amd_{suffix}:localhost");
    let user_id = format!("@rs_amd_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: None,
            topic: None,
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

    // First add (creates the record)
    let _m1 = storage
        .add_member(CreateSummaryMemberRequest {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            display_name: Some("First".to_string()),
            avatar_url: None,
            membership: "join".to_string(),
            is_hero: Some(false),
            last_active_ts: None,
        })
        .await
        .unwrap();

    // Duplicate with different membership
    let m2 = storage
        .add_member(CreateSummaryMemberRequest {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            display_name: Some("Second".to_string()),
            avatar_url: Some("mxc://second".to_string()),
            membership: "leave".to_string(),
            is_hero: None,
            last_active_ts: None,
        })
        .await
        .unwrap();

    // Should update membership and overwrite display_name (COALESCE prefers EXCLUDED when non-null)
    assert_eq!(m2.membership, "leave");
    assert_eq!(m2.display_name.as_deref(), Some("Second"));
    assert_eq!(m2.avatar_url.as_deref(), Some("mxc://second"));

    cleanup_summary_data(&pool, &suffix).await;
}

// ── add_members_batch ───────────────────────────────────────────

#[tokio::test]
async fn test_add_members_batch_inserts_multiple() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_amb_{suffix}:localhost");
    let u1 = format!("@rs_amb1_{suffix}:localhost");
    let u2 = format!("@rs_amb2_{suffix}:localhost");
    let u3 = format!("@rs_amb3_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &u1).await;
    ensure_test_user(&pool, &u2).await;
    ensure_test_user(&pool, &u3).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: None,
            topic: None,
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

    let affected = storage
        .add_members_batch(
            &room_id,
            vec![
                CreateSummaryMemberRequest {
                    room_id: room_id.clone(),
                    user_id: u1.clone(),
                    display_name: Some("User1".to_string()),
                    avatar_url: None,
                    membership: "join".to_string(),
                    is_hero: Some(true),
                    last_active_ts: None,
                },
                CreateSummaryMemberRequest {
                    room_id: room_id.clone(),
                    user_id: u2.clone(),
                    display_name: Some("User2".to_string()),
                    avatar_url: None,
                    membership: "join".to_string(),
                    is_hero: Some(false),
                    last_active_ts: None,
                },
                CreateSummaryMemberRequest {
                    room_id: room_id.clone(),
                    user_id: u3.clone(),
                    display_name: Some("User3".to_string()),
                    avatar_url: None,
                    membership: "invite".to_string(),
                    is_hero: None,
                    last_active_ts: None,
                },
            ],
        )
        .await
        .unwrap();

    assert_eq!(affected, 3);

    let members = storage.get_members(&room_id).await.unwrap();
    assert_eq!(members.len(), 3);

    // Member counts should be refreshed
    let summary = storage.get_summary(&room_id).await.unwrap().unwrap();
    assert_eq!(summary.member_count, Some(3));
    assert_eq!(summary.joined_member_count, Some(2));
    assert_eq!(summary.invited_member_count, Some(1));

    cleanup_summary_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_add_members_batch_empty_returns_zero() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_ambe_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;

    let storage = RoomSummaryStorage::new(&pool);
    let affected = storage.add_members_batch(&room_id, vec![]).await.unwrap();
    assert_eq!(affected, 0);

    cleanup_summary_data(&pool, &suffix).await;
}

// ── update_member ───────────────────────────────────────────────

#[tokio::test]
async fn test_update_member_changes_fields() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_um_{suffix}:localhost");
    let user_id = format!("@rs_um_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: None,
            topic: None,
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
    storage
        .add_member(CreateSummaryMemberRequest {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            display_name: Some("Original".to_string()),
            avatar_url: None,
            membership: "join".to_string(),
            is_hero: Some(false),
            last_active_ts: None,
        })
        .await
        .unwrap();

    let updated = storage
        .update_member(
            &room_id,
            &user_id,
            UpdateSummaryMemberRequest {
                display_name: Some("Updated".to_string()),
                avatar_url: Some("mxc://new_avatar".to_string()),
                membership: Some("leave".to_string()),
                is_hero: Some(true),
                last_active_ts: Some(1_700_000_000_000i64),
            },
        )
        .await
        .unwrap();

    assert_eq!(updated.display_name.as_deref(), Some("Updated"));
    assert_eq!(updated.avatar_url.as_deref(), Some("mxc://new_avatar"));
    assert_eq!(updated.membership, "leave");
    assert!(updated.is_hero);

    cleanup_summary_data(&pool, &suffix).await;
}

// ── remove_member ───────────────────────────────────────────────

#[tokio::test]
async fn test_remove_member_deletes_and_updates_counts() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_rm_{suffix}:localhost");
    let user_id = format!("@rs_rm_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: None,
            topic: None,
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
    storage
        .add_member(CreateSummaryMemberRequest {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            display_name: None,
            avatar_url: None,
            membership: "join".to_string(),
            is_hero: None,
            last_active_ts: None,
        })
        .await
        .unwrap();

    storage.remove_member(&room_id, &user_id).await.unwrap();

    let members = storage.get_members(&room_id).await.unwrap();
    assert!(members.is_empty());

    // Counts should be zero
    let summary = storage.get_summary(&room_id).await.unwrap().unwrap();
    assert_eq!(summary.member_count, Some(0));

    // Idempotent — removing again should not error
    storage.remove_member(&room_id, &user_id).await.unwrap();

    cleanup_summary_data(&pool, &suffix).await;
}

// ── get_members ─────────────────────────────────────────────────

#[tokio::test]
async fn test_get_members_returns_ordered_list() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_gm_{suffix}:localhost");
    let u1 = format!("@rs_gma_{suffix}:localhost");
    let u2 = format!("@rs_gmb_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &u1).await;
    ensure_test_user(&pool, &u2).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: None,
            topic: None,
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

    storage
        .add_members_batch(
            &room_id,
            vec![
                CreateSummaryMemberRequest {
                    room_id: room_id.clone(),
                    user_id: u2.clone(),
                    display_name: None,
                    avatar_url: None,
                    membership: "join".to_string(),
                    is_hero: Some(false),
                    last_active_ts: None,
                },
                CreateSummaryMemberRequest {
                    room_id: room_id.clone(),
                    user_id: u1.clone(),
                    display_name: None,
                    avatar_url: None,
                    membership: "join".to_string(),
                    is_hero: Some(true),
                    last_active_ts: None,
                },
            ],
        )
        .await
        .unwrap();

    let members = storage.get_members(&room_id).await.unwrap();
    assert_eq!(members.len(), 2);
    // Heroes first, then by user_id
    assert!(members[0].is_hero);
    assert_eq!(members[0].user_id, u1);

    cleanup_summary_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_members_empty_room() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_gme_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: None,
            topic: None,
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

    let members = storage.get_members(&room_id).await.unwrap();
    assert!(members.is_empty());

    cleanup_summary_data(&pool, &suffix).await;
}

// ── get_heroes ──────────────────────────────────────────────────

#[tokio::test]
async fn test_get_heroes_ordered_and_limited() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_gh_{suffix}:localhost");
    let u1 = format!("@rs_gh1_{suffix}:localhost");
    let u2 = format!("@rs_gh2_{suffix}:localhost");
    let u3 = format!("@rs_gh3_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &u1).await;
    ensure_test_user(&pool, &u2).await;
    ensure_test_user(&pool, &u3).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: None,
            topic: None,
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

    storage
        .add_members_batch(
            &room_id,
            vec![
                CreateSummaryMemberRequest {
                    room_id: room_id.clone(),
                    user_id: u1.clone(),
                    display_name: None,
                    avatar_url: None,
                    membership: "join".to_string(),
                    is_hero: Some(false),
                    last_active_ts: Some(1_700_000_000_001i64),
                },
                CreateSummaryMemberRequest {
                    room_id: room_id.clone(),
                    user_id: u2.clone(),
                    display_name: None,
                    avatar_url: None,
                    membership: "join".to_string(),
                    is_hero: Some(true),
                    last_active_ts: Some(1_700_000_000_000i64),
                },
                CreateSummaryMemberRequest {
                    room_id: room_id.clone(),
                    user_id: u3.clone(),
                    display_name: None,
                    avatar_url: None,
                    membership: "invite".to_string(),
                    is_hero: Some(false),
                    last_active_ts: None,
                },
            ],
        )
        .await
        .unwrap();

    // u3 is invited, so should be excluded from heroes (only 'join')
    let heroes = storage.get_heroes(&room_id, 10).await.unwrap();
    assert_eq!(heroes.len(), 2);
    // Heroes (is_hero=true) first
    assert!(heroes[0].is_hero);
    assert_eq!(heroes[0].user_id, u2);

    // Respect limit
    let limited = storage.get_heroes(&room_id, 1).await.unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].user_id, u2);

    cleanup_summary_data(&pool, &suffix).await;
}

// ── get_heroes_batch ────────────────────────────────────────────

#[tokio::test]
async fn test_get_heroes_batch_multiple_rooms() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_a = format!("!rs_ghb_a_{suffix}:localhost");
    let room_b = format!("!rs_ghb_b_{suffix}:localhost");
    let u1 = format!("@rs_ghb1_{suffix}:localhost");
    let u2 = format!("@rs_ghb2_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_a).await;
    ensure_test_room(&pool, &room_b).await;
    ensure_test_user(&pool, &u1).await;
    ensure_test_user(&pool, &u2).await;

    let storage = RoomSummaryStorage::new(&pool);
    let req = |id: &str| CreateRoomSummaryRequest {
        room_id: id.to_string(),
        room_type: None,
        name: None,
        topic: None,
        avatar_url: None,
        canonical_alias: None,
        join_rule: None,
        history_visibility: None,
        guest_access: None,
        is_direct: None,
        is_space: None,
    };
    storage.create_summary(req(&room_a)).await.unwrap();
    storage.create_summary(req(&room_b)).await.unwrap();

    // Add member to room_a only
    storage
        .add_member(CreateSummaryMemberRequest {
            room_id: room_a.clone(),
            user_id: u1.clone(),
            display_name: None,
            avatar_url: None,
            membership: "join".to_string(),
            is_hero: Some(true),
            last_active_ts: None,
        })
        .await
        .unwrap();

    let result = storage.get_heroes_batch(&[room_a.clone(), room_b.clone()], 10).await.unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[&room_a].len(), 1);
    assert_eq!(result[&room_a][0].user_id, u1);
    assert!(result[&room_b].is_empty());

    // Empty input
    let empty_result = storage.get_heroes_batch(&[], 10).await.unwrap();
    assert!(empty_result.is_empty());

    cleanup_summary_data(&pool, &suffix).await;
}

// ── get_hero_candidates ─────────────────────────────────────────

#[tokio::test]
async fn test_get_hero_candidates_returns_joined_sorted() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_ghc_{suffix}:localhost");
    let u1 = format!("@rs_ghc1_{suffix}:localhost");
    let u2 = format!("@rs_ghc2_{suffix}:localhost");
    let u3 = format!("@rs_ghc3_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &u1).await;
    ensure_test_user(&pool, &u2).await;
    ensure_test_user(&pool, &u3).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: None,
            topic: None,
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

    storage
        .add_members_batch(
            &room_id,
            vec![
                CreateSummaryMemberRequest {
                    room_id: room_id.clone(),
                    user_id: u1.clone(),
                    display_name: None,
                    avatar_url: None,
                    membership: "join".to_string(),
                    is_hero: Some(false),
                    last_active_ts: Some(1_700_000_000_003i64),
                },
                CreateSummaryMemberRequest {
                    room_id: room_id.clone(),
                    user_id: u2.clone(),
                    display_name: None,
                    avatar_url: None,
                    membership: "join".to_string(),
                    is_hero: Some(true),
                    last_active_ts: Some(1_700_000_000_001i64),
                },
                CreateSummaryMemberRequest {
                    room_id: room_id.clone(),
                    user_id: u3.clone(),
                    display_name: None,
                    avatar_url: None,
                    membership: "leave".to_string(),
                    is_hero: Some(false),
                    last_active_ts: Some(1_700_000_000_002i64),
                },
            ],
        )
        .await
        .unwrap();

    // Candidates include all joined members, sorted by last_active_ts DESC
    let candidates = storage.get_hero_candidates(&room_id, 10).await.unwrap();
    // u3 is 'leave', so excluded; u1 and u2 are 'join'
    assert_eq!(candidates.len(), 2);
    // Most recently active first
    assert_eq!(candidates[0].user_id, u1);
    assert_eq!(candidates[1].user_id, u2);

    // Respect limit
    let limited = storage.get_hero_candidates(&room_id, 1).await.unwrap();
    assert_eq!(limited.len(), 1);

    cleanup_summary_data(&pool, &suffix).await;
}

// ── set_hero_members ────────────────────────────────────────────

#[tokio::test]
async fn test_set_hero_members_updates_flags() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_shm_{suffix}:localhost");
    let u1 = format!("@rs_shm1_{suffix}:localhost");
    let u2 = format!("@rs_shm2_{suffix}:localhost");
    let u3 = format!("@rs_shm3_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &u1).await;
    ensure_test_user(&pool, &u2).await;
    ensure_test_user(&pool, &u3).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: None,
            topic: None,
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

    storage
        .add_members_batch(
            &room_id,
            vec![
                CreateSummaryMemberRequest {
                    room_id: room_id.clone(),
                    user_id: u1.clone(),
                    display_name: None,
                    avatar_url: None,
                    membership: "join".to_string(),
                    is_hero: Some(true),
                    last_active_ts: None,
                },
                CreateSummaryMemberRequest {
                    room_id: room_id.clone(),
                    user_id: u2.clone(),
                    display_name: None,
                    avatar_url: None,
                    membership: "join".to_string(),
                    is_hero: Some(false),
                    last_active_ts: None,
                },
                CreateSummaryMemberRequest {
                    room_id: room_id.clone(),
                    user_id: u3.clone(),
                    display_name: None,
                    avatar_url: None,
                    membership: "join".to_string(),
                    is_hero: Some(false),
                    last_active_ts: None,
                },
            ],
        )
        .await
        .unwrap();

    // Set only u2 and u3 as heroes (u1 gets removed)
    storage.set_hero_members(&room_id, &[u2.clone(), u3.clone()]).await.unwrap();

    let members = storage.get_members(&room_id).await.unwrap();
    let hero_count = members.iter().filter(|m| m.is_hero).count();
    assert_eq!(hero_count, 2);
    let u1_member = members.iter().find(|m| m.user_id == u1).unwrap();
    assert!(!u1_member.is_hero);
    let u2_member = members.iter().find(|m| m.user_id == u2).unwrap();
    assert!(u2_member.is_hero);
    let u3_member = members.iter().find(|m| m.user_id == u3).unwrap();
    assert!(u3_member.is_hero);

    cleanup_summary_data(&pool, &suffix).await;
}

// ── set_state / get_state / get_all_state ───────────────────────

#[tokio::test]
async fn test_state_crud_single() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_scs_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    let content = json!({"creator": "@admin:localhost", "room_version": "1"});

    // Create state
    let state = storage.set_state(&room_id, "m.room.create", "", None, content.clone()).await.unwrap();
    assert_eq!(state.room_id, room_id);
    assert_eq!(state.event_type, "m.room.create");
    assert_eq!(state.state_key, "");
    assert_eq!(state.content, content);

    // Fetch it back
    let fetched = storage.get_state(&room_id, "m.room.create", "").await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().content, content);

    // Fetch nonexistent
    let nonexistent = storage.get_state(&room_id, "m.room.name", "").await.unwrap();
    assert!(nonexistent.is_none());

    cleanup_summary_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_state_upsert_overwrites_existing() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_suo_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = RoomSummaryStorage::new(&pool);

    let s1 =
        storage.set_state(&room_id, "m.room.name", "", Some("$ev1:localhost"), json!({"name": "First"})).await.unwrap();
    assert_eq!(s1.event_id.as_deref(), Some("$ev1:localhost"));
    assert_eq!(s1.content, json!({"name": "First"}));

    // Upsert same type+key with different data
    let s2 = storage
        .set_state(&room_id, "m.room.name", "", Some("$ev2:localhost"), json!({"name": "Second"}))
        .await
        .unwrap();
    assert_eq!(s2.event_id.as_deref(), Some("$ev2:localhost"));
    assert_eq!(s2.content, json!({"name": "Second"}));

    cleanup_summary_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_set_states_batch_inserts_all() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_ssb_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = RoomSummaryStorage::new(&pool);

    let affected = storage
        .set_states_batch(
            &room_id,
            &[
                RoomSummaryStateEntry {
                    event_type: "m.room.create".to_string(),
                    state_key: "".to_string(),
                    event_id: None,
                    content: json!({"creator": "@a:localhost"}),
                },
                RoomSummaryStateEntry {
                    event_type: "m.room.name".to_string(),
                    state_key: "".to_string(),
                    event_id: Some("$ev_n:localhost".to_string()),
                    content: json!({"name": "Batch Room"}),
                },
                RoomSummaryStateEntry {
                    event_type: "m.room.join_rules".to_string(),
                    state_key: "".to_string(),
                    event_id: None,
                    content: json!({"join_rule": "public"}),
                },
            ],
        )
        .await
        .unwrap();
    assert_eq!(affected, 3);

    let all = storage.get_all_state(&room_id).await.unwrap();
    assert_eq!(all.len(), 3);

    // Empty batch
    let zero = storage.set_states_batch(&room_id, &[]).await.unwrap();
    assert_eq!(zero, 0);

    cleanup_summary_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_all_state_returns_all_entries() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_gas_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .set_states_batch(
            &room_id,
            &[
                RoomSummaryStateEntry {
                    event_type: "m.room.create".to_string(),
                    state_key: "".to_string(),
                    event_id: None,
                    content: json!({}),
                },
                RoomSummaryStateEntry {
                    event_type: "m.room.member".to_string(),
                    state_key: "@alice:localhost".to_string(),
                    event_id: None,
                    content: json!({"membership": "join"}),
                },
            ],
        )
        .await
        .unwrap();

    let all = storage.get_all_state(&room_id).await.unwrap();
    assert_eq!(all.len(), 2);

    // Verify types
    let types: Vec<&str> = all.iter().map(|s| s.event_type.as_str()).collect();
    assert!(types.contains(&"m.room.create"));
    assert!(types.contains(&"m.room.member"));

    cleanup_summary_data(&pool, &suffix).await;
}

// ── get_stats / update_stats ────────────────────────────────────

#[tokio::test]
async fn test_stats_crud() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_st_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = RoomSummaryStorage::new(&pool);

    // Initially no stats
    let initial = storage.get_stats(&room_id).await.unwrap();
    assert!(initial.is_none());

    // Insert stats
    let stats = storage.update_stats(&room_id, 100, 20, 80, 5, 1048576).await.unwrap();
    assert_eq!(stats.total_events, 100);
    assert_eq!(stats.total_state_events, 20);
    assert_eq!(stats.total_messages, 80);
    assert_eq!(stats.total_media, 5);
    assert_eq!(stats.storage_size, 1048576);

    // Fetch back
    let fetched = storage.get_stats(&room_id).await.unwrap().unwrap();
    assert_eq!(fetched.total_events, 100);

    // Update — upsert
    let updated = storage.update_stats(&room_id, 200, 40, 160, 10, 2097152).await.unwrap();
    assert_eq!(updated.total_events, 200);
    assert_eq!(updated.storage_size, 2097152);

    cleanup_summary_data(&pool, &suffix).await;
}

// ── queue_update / get_pending_updates / mark_processed / mark_failed ─

#[tokio::test]
async fn test_queue_lifecycle() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_ql_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = RoomSummaryStorage::new(&pool);

    // Queue some updates
    storage.queue_update(&room_id, "$ev1", "m.room.message", None, 10).await.unwrap();
    storage.queue_update(&room_id, "$ev2", "m.room.member", Some("@alice:localhost"), 5).await.unwrap();
    storage.queue_update(&room_id, "$ev3", "m.room.name", Some(""), 1).await.unwrap();

    // Get pending (ordered by priority DESC) — may contain entries from other tests
    let pending = storage.get_pending_updates(100).await.unwrap();
    let ours_before: Vec<_> = pending.iter().filter(|p| p.room_id == room_id).collect();
    assert_eq!(ours_before.len(), 3, "should have 3 pending updates for our room");
    // Highest priority first
    assert_eq!(ours_before[0].event_id, "$ev1");
    assert_eq!(ours_before[0].priority, 10);
    assert_eq!(ours_before[1].event_id, "$ev2");
    assert_eq!(ours_before[2].event_id, "$ev3");

    // Respect limit — count of our items in limited result
    let limited = storage.get_pending_updates(1).await.unwrap();
    let ours_limited: Vec<_> = limited.iter().filter(|p| p.room_id == room_id).collect();
    // May be 0 or 1 depending on whether higher-priority items from other tests exist
    assert!(ours_limited.len() <= 1);

    // Mark one of ours as processed
    let target = ours_before[0];
    storage.mark_update_processed(target.id).await.unwrap();
    let after = storage.get_pending_updates(100).await.unwrap();
    let ours_after: Vec<_> = after.iter().filter(|p| p.room_id == room_id).collect();
    assert_eq!(ours_after.len(), 2, "should have 2 pending after marking one processed");

    // Mark another of ours as failed
    storage.mark_update_failed(ours_after[0].id, "test error").await.unwrap();

    cleanup_summary_data(&pool, &suffix).await;
}

// ── unread notifications ────────────────────────────────────────

#[tokio::test]
async fn test_unread_notifications_increment_and_clear() {
    let pool = test_pool().await;
    let suffix = make_suffix();
    let room_id = format!("!rs_un_{suffix}:localhost");
    cleanup_summary_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = RoomSummaryStorage::new(&pool);
    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: None,
            topic: None,
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

    // Increment regular
    storage.increment_unread_notifications(&room_id, false).await.unwrap();
    let s = storage.get_summary(&room_id).await.unwrap().unwrap();
    assert_eq!(s.unread_notifications, 1);
    assert_eq!(s.unread_highlight, 0);

    // Increment highlight
    storage.increment_unread_notifications(&room_id, true).await.unwrap();
    let s = storage.get_summary(&room_id).await.unwrap().unwrap();
    assert_eq!(s.unread_notifications, 2);
    assert_eq!(s.unread_highlight, 1);

    // Clear
    storage.clear_unread_notifications(&room_id).await.unwrap();
    let s = storage.get_summary(&room_id).await.unwrap().unwrap();
    assert_eq!(s.unread_notifications, 0);
    assert_eq!(s.unread_highlight, 0);

    cleanup_summary_data(&pool, &suffix).await;
}
