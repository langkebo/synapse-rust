#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::storage::friend_room::FriendRoomStorage;
static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {

    sqlx::query(
        r#"
        CREATE TABLE users (
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
        CREATE TABLE rooms (
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
        CREATE TABLE events (
            event_id TEXT NOT NULL PRIMARY KEY,
            room_id TEXT NOT NULL,
            sender TEXT NOT NULL,
            event_type TEXT NOT NULL,
            content JSONB NOT NULL,
            origin_server_ts BIGINT NOT NULL,
            state_key TEXT,
            is_redacted BOOLEAN DEFAULT FALSE,
            redacted_at BIGINT,
            redacted_by TEXT,
            transaction_id TEXT,
            depth BIGINT,
            prev_events JSONB,
            auth_events JSONB,
            signatures JSONB,
            hashes JSONB,
            unsigned JSONB DEFAULT '{}',
            processed_at BIGINT,
            not_before BIGINT DEFAULT 0,
            status TEXT,
            reference_image TEXT,
            origin TEXT,
            user_id TEXT,
            stream_ordering BIGSERIAL
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create events table");

    sqlx::query(
        r#"
        CREATE TABLE room_memberships (
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

    sqlx::query(
        r#"
        CREATE TABLE friend_requests (
            id BIGSERIAL PRIMARY KEY,
            sender_id TEXT NOT NULL,
            receiver_id TEXT NOT NULL,
            message TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            UNIQUE (sender_id, receiver_id)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create friend_requests table");
}

fn create_storage(pool: &Arc<sqlx::PgPool>) -> FriendRoomStorage {
    FriendRoomStorage::new(pool.clone())
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

async fn insert_event(
    pool: &sqlx::PgPool,
    event_id: &str,
    room_id: &str,
    sender: &str,
    event_type: &str,
    state_key: Option<&str>,
    content: &serde_json::Value,
) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts, state_key, depth)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 1)
        "#,
    )
    .bind(event_id)
    .bind(room_id)
    .bind(sender)
    .bind(event_type)
    .bind(content)
    .bind(now)
    .bind(state_key)
    .execute(pool)
    .await
    .expect("Failed to insert test event");
}

#[tokio::test]
async fn test_get_friend_list_room_id_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@friend_user_{suffix}:localhost");
        let room_id = format!("!friend_room_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("friend_user_{suffix}")).await;
        insert_room(&pool, &room_id).await;
        insert_event(
            &pool,
            &format!("$create_{suffix}"),
            &room_id,
            &user_id,
            "m.room.create",
            None,
            &json!({"type": "m.friends"}),
        )
        .await;

        let result = storage.get_friend_list_room_id(&user_id).await.unwrap();
        assert_eq!(result, Some(room_id));
}

#[tokio::test]
async fn test_get_friend_list_room_id_not_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@nonexistent_{suffix}:localhost");

        let result = storage.get_friend_list_room_id(&user_id).await.unwrap();
        assert_eq!(result, None);
}

#[tokio::test]
async fn test_get_friend_list_content() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@friend_content_{suffix}:localhost");
        let room_id = format!("!friend_content_room_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("friend_content_{suffix}")).await;
        insert_room(&pool, &room_id).await;

        let content = json!({
            "friends": [
                {"user_id": "@alice:localhost", "dm_room_id": "!dm1:localhost"},
                {"user_id": "@bob:localhost", "dm_room_id": "!dm2:localhost"}
            ]
        });
        insert_event(&pool, &format!("$flist_{suffix}"), &room_id, &user_id, "m.friends.list", Some(""), &content)
            .await;

        let result = storage.get_friend_list_content(&room_id).await.unwrap();
        assert!(result.is_some());
        let result_val = result.unwrap();
        let friends = result_val.get("friends").unwrap().as_array().unwrap();
        assert_eq!(friends.len(), 2);
}

#[tokio::test]
async fn test_get_friend_list_content_empty() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let room_id = format!("!empty_room_{suffix}:localhost");

        let result = storage.get_friend_list_content(&room_id).await.unwrap();
        assert_eq!(result, None);
}

#[tokio::test]
async fn test_is_friend_true() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@is_friend_{suffix}:localhost");
        let room_id = format!("!is_friend_room_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("is_friend_{suffix}")).await;
        insert_room(&pool, &room_id).await;

        let content = json!({
            "friends": [
                {"user_id": "@alice:localhost"},
                {"user_id": "@bob:localhost"}
            ]
        });
        insert_event(&pool, &format!("$isfriend_{suffix}"), &room_id, &user_id, "m.friends.list", Some(""), &content)
            .await;

        let result = storage.is_friend(&room_id, "@alice:localhost").await.unwrap();
        assert!(result);
}

#[tokio::test]
async fn test_is_friend_false() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@not_friend_{suffix}:localhost");
        let room_id = format!("!not_friend_room_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("not_friend_{suffix}")).await;
        insert_room(&pool, &room_id).await;

        let content = json!({
            "friends": [
                {"user_id": "@alice:localhost"}
            ]
        });
        insert_event(&pool, &format!("$notfriend_{suffix}"), &room_id, &user_id, "m.friends.list", Some(""), &content)
            .await;

        let result = storage.is_friend(&room_id, "@charlie:localhost").await.unwrap();
        assert!(!result);
}

#[tokio::test]
async fn test_get_friend_info_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@friend_info_{suffix}:localhost");
        let room_id = format!("!friend_info_room_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("friend_info_{suffix}")).await;
        insert_room(&pool, &room_id).await;

        let content = json!({
            "friends": [
                {"user_id": "@alice:localhost", "displayname": "Alice"},
                {"user_id": "@bob:localhost", "displayname": "Bob"}
            ]
        });
        insert_event(&pool, &format!("$finfo_{suffix}"), &room_id, &user_id, "m.friends.list", Some(""), &content)
            .await;

        let result = storage.get_friend_info(&room_id, "@alice:localhost").await.unwrap();
        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.get("displayname").unwrap().as_str(), Some("Alice"));
}

#[tokio::test]
async fn test_get_friend_info_not_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@friend_info_nf_{suffix}:localhost");
        let room_id = format!("!friend_info_nf_room_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("friend_info_nf_{suffix}")).await;
        insert_room(&pool, &room_id).await;

        let content = json!({"friends": [{"user_id": "@alice:localhost"}]});
        insert_event(&pool, &format!("$finfonf_{suffix}"), &room_id, &user_id, "m.friends.list", Some(""), &content)
            .await;

        let result = storage.get_friend_info(&room_id, "@nobody:localhost").await.unwrap();
        assert_eq!(result, None);
}

#[tokio::test]
async fn test_create_friend_request() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let sender_id = format!("@sender_{suffix}:localhost");
        let receiver_id = format!("@receiver_{suffix}:localhost");

        insert_user(&pool, &sender_id, &format!("sender_{suffix}")).await;
        insert_user(&pool, &receiver_id, &format!("receiver_{suffix}")).await;

        let id = storage.create_friend_request(&sender_id, &receiver_id, Some("Hello!")).await.unwrap();
        assert!(id > 0);

        let req = storage.get_friend_request(&sender_id, &receiver_id).await.unwrap().unwrap();
        assert_eq!(req.sender_id, sender_id);
        assert_eq!(req.receiver_id, receiver_id);
        assert_eq!(req.message, Some("Hello!".to_string()));
        assert_eq!(req.status, "pending");
}

#[tokio::test]
async fn test_create_friend_request_upsert() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let sender_id = format!("@upsert_sender_{suffix}:localhost");
        let receiver_id = format!("@upsert_receiver_{suffix}:localhost");

        insert_user(&pool, &sender_id, &format!("upsert_sender_{suffix}")).await;
        insert_user(&pool, &receiver_id, &format!("upsert_receiver_{suffix}")).await;

        let id1 = storage.create_friend_request(&sender_id, &receiver_id, Some("First")).await.unwrap();

        let id2 = storage.create_friend_request(&sender_id, &receiver_id, Some("Second")).await.unwrap();

        assert_eq!(id1, id2);

        let req = storage.get_friend_request(&sender_id, &receiver_id).await.unwrap().unwrap();
        assert_eq!(req.message, Some("Second".to_string()));
        assert_eq!(req.status, "pending");
}

#[tokio::test]
async fn test_get_pending_friend_request() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let sender_id = format!("@pending_sender_{suffix}:localhost");
        let receiver_id = format!("@pending_receiver_{suffix}:localhost");

        insert_user(&pool, &sender_id, &format!("pending_sender_{suffix}")).await;
        insert_user(&pool, &receiver_id, &format!("pending_receiver_{suffix}")).await;

        storage.create_friend_request(&sender_id, &receiver_id, None).await.unwrap();

        let result = storage.get_pending_friend_request(&sender_id, &receiver_id).await.unwrap();
        assert!(result.is_some());

        storage.update_friend_request_status(&sender_id, &receiver_id, "accepted").await.unwrap();

        let result = storage.get_pending_friend_request(&sender_id, &receiver_id).await.unwrap();
        assert!(result.is_none());
}

#[tokio::test]
async fn test_get_incoming_friend_requests() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let receiver_id = format!("@incoming_recv_{suffix}:localhost");
        let sender1 = format!("@incoming_s1_{suffix}:localhost");
        let sender2 = format!("@incoming_s2_{suffix}:localhost");

        insert_user(&pool, &receiver_id, &format!("incoming_recv_{suffix}")).await;
        insert_user(&pool, &sender1, &format!("incoming_s1_{suffix}")).await;
        insert_user(&pool, &sender2, &format!("incoming_s2_{suffix}")).await;

        storage.create_friend_request(&sender1, &receiver_id, None).await.unwrap();
        storage.create_friend_request(&sender2, &receiver_id, Some("Hi")).await.unwrap();

        let incoming = storage.get_incoming_friend_requests(&receiver_id).await.unwrap();
        assert_eq!(incoming.len(), 2);
}

#[tokio::test]
async fn test_get_outgoing_friend_requests() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let sender_id = format!("@outgoing_sender_{suffix}:localhost");
        let recv1 = format!("@outgoing_r1_{suffix}:localhost");
        let recv2 = format!("@outgoing_r2_{suffix}:localhost");

        insert_user(&pool, &sender_id, &format!("outgoing_sender_{suffix}")).await;
        insert_user(&pool, &recv1, &format!("outgoing_r1_{suffix}")).await;
        insert_user(&pool, &recv2, &format!("outgoing_r2_{suffix}")).await;

        storage.create_friend_request(&sender_id, &recv1, None).await.unwrap();
        storage.create_friend_request(&sender_id, &recv2, None).await.unwrap();

        let outgoing = storage.get_outgoing_friend_requests(&sender_id).await.unwrap();
        assert_eq!(outgoing.len(), 2);
}

#[tokio::test]
async fn test_update_friend_request_status() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let sender_id = format!("@status_sender_{suffix}:localhost");
        let receiver_id = format!("@status_receiver_{suffix}:localhost");

        insert_user(&pool, &sender_id, &format!("status_sender_{suffix}")).await;
        insert_user(&pool, &receiver_id, &format!("status_receiver_{suffix}")).await;

        storage.create_friend_request(&sender_id, &receiver_id, None).await.unwrap();

        let updated = storage.update_friend_request_status(&sender_id, &receiver_id, "accepted").await.unwrap();
        assert!(updated);

        let req = storage.get_friend_request(&sender_id, &receiver_id).await.unwrap().unwrap();
        assert_eq!(req.status, "accepted");
}

#[tokio::test]
async fn test_update_friend_request_status_already_accepted() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let sender_id = format!("@already_sender_{suffix}:localhost");
        let receiver_id = format!("@already_receiver_{suffix}:localhost");

        insert_user(&pool, &sender_id, &format!("already_sender_{suffix}")).await;
        insert_user(&pool, &receiver_id, &format!("already_receiver_{suffix}")).await;

        storage.create_friend_request(&sender_id, &receiver_id, None).await.unwrap();

        storage.update_friend_request_status(&sender_id, &receiver_id, "accepted").await.unwrap();

        let updated = storage.update_friend_request_status(&sender_id, &receiver_id, "rejected").await.unwrap();
        assert!(!updated);
}

#[tokio::test]
async fn test_delete_friend_request() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let sender_id = format!("@del_sender_{suffix}:localhost");
        let receiver_id = format!("@del_receiver_{suffix}:localhost");

        insert_user(&pool, &sender_id, &format!("del_sender_{suffix}")).await;
        insert_user(&pool, &receiver_id, &format!("del_receiver_{suffix}")).await;

        storage.create_friend_request(&sender_id, &receiver_id, None).await.unwrap();

        let deleted = storage.delete_friend_request(&sender_id, &receiver_id).await.unwrap();
        assert!(deleted);

        let result = storage.get_friend_request(&sender_id, &receiver_id).await.unwrap();
        assert!(result.is_none());
}

#[tokio::test]
async fn test_delete_friend_request_nonexistent() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();

        let deleted = storage
            .delete_friend_request(&format!("@ghost_{suffix}:localhost"), &format!("@ghost2_{suffix}:localhost"))
            .await
            .unwrap();
        assert!(!deleted);
}

#[tokio::test]
async fn test_has_pending_request() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let sender_id = format!("@has_sender_{suffix}:localhost");
        let receiver_id = format!("@has_receiver_{suffix}:localhost");

        insert_user(&pool, &sender_id, &format!("has_sender_{suffix}")).await;
        insert_user(&pool, &receiver_id, &format!("has_receiver_{suffix}")).await;

        assert!(!storage.has_pending_request(&sender_id, &receiver_id).await.unwrap());

        storage.create_friend_request(&sender_id, &receiver_id, None).await.unwrap();

        assert!(storage.has_pending_request(&sender_id, &receiver_id).await.unwrap());
}

#[tokio::test]
async fn test_has_any_pending_request_bidirectional() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_a = format!("@bidir_a_{suffix}:localhost");
        let user_b = format!("@bidir_b_{suffix}:localhost");

        insert_user(&pool, &user_a, &format!("bidir_a_{suffix}")).await;
        insert_user(&pool, &user_b, &format!("bidir_b_{suffix}")).await;

        assert!(!storage.has_any_pending_request(&user_a, &user_b).await.unwrap());

        storage.create_friend_request(&user_a, &user_b, None).await.unwrap();

        assert!(storage.has_any_pending_request(&user_a, &user_b).await.unwrap());
        assert!(storage.has_any_pending_request(&user_b, &user_a).await.unwrap());
}

#[tokio::test]
async fn test_ensure_user_exists_present() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@ensure_present_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("ensure_present_{suffix}")).await;

        let result = storage.ensure_user_exists(&user_id).await;
        assert!(result.is_ok());
}

#[tokio::test]
async fn test_ensure_user_exists_absent() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@ensure_absent_{suffix}:localhost");

        let result = storage.ensure_user_exists(&user_id).await;
        assert!(result.is_err());
}

#[tokio::test]
async fn test_create_friend_request_with_user_ensure_success() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let sender_id = format!("@ensure_ok_sender_{suffix}:localhost");
        let receiver_id = format!("@ensure_ok_receiver_{suffix}:localhost");

        insert_user(&pool, &sender_id, &format!("ensure_ok_sender_{suffix}")).await;
        insert_user(&pool, &receiver_id, &format!("ensure_ok_receiver_{suffix}")).await;

        let id = storage.create_friend_request_with_user_ensure(&sender_id, &receiver_id, Some("Hi")).await.unwrap();
        assert!(id > 0);
}

#[tokio::test]
async fn test_create_friend_request_with_user_ensure_sender_missing() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let sender_id = format!("@missing_sender_{suffix}:localhost");
        let receiver_id = format!("@missing_receiver_{suffix}:localhost");

        insert_user(&pool, &receiver_id, &format!("missing_receiver_{suffix}")).await;

        let result = storage.create_friend_request_with_user_ensure(&sender_id, &receiver_id, None).await;
        assert!(result.is_err());
}

#[tokio::test]
async fn test_create_and_delete_friend_group() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@group_user_{suffix}:localhost");
        let room_id = format!("!group_room_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("group_user_{suffix}")).await;
        insert_room(&pool, &room_id).await;

        storage.create_friend_group(&room_id, &user_id, "Work").await.unwrap();

        let groups = storage.get_friend_groups(&room_id).await.unwrap().unwrap();
        let groups_arr = groups.get("groups").unwrap().as_array().unwrap();
        assert_eq!(groups_arr.len(), 1);
        assert_eq!(groups_arr[0].get("name").unwrap().as_str(), Some("Work"));

        let deleted = storage.delete_friend_group(&room_id, &user_id, "Work").await.unwrap();
        assert!(deleted);

        let groups = storage.get_friend_groups(&room_id).await.unwrap().unwrap();
        let groups_arr = groups.get("groups").unwrap().as_array().unwrap();
        assert!(groups_arr.is_empty());
}

#[tokio::test]
async fn test_create_friend_group_duplicate() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@dup_group_user_{suffix}:localhost");
        let room_id = format!("!dup_group_room_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("dup_group_user_{suffix}")).await;
        insert_room(&pool, &room_id).await;

        storage.create_friend_group(&room_id, &user_id, "Family").await.unwrap();

        let result = storage.create_friend_group(&room_id, &user_id, "Family").await;
        assert!(result.is_err());
}

#[tokio::test]
async fn test_delete_friend_group_nonexistent() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@del_nogroup_user_{suffix}:localhost");
        let room_id = format!("!del_nogroup_room_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("del_nogroup_user_{suffix}")).await;
        insert_room(&pool, &room_id).await;

        let deleted = storage.delete_friend_group(&room_id, &user_id, "Nonexistent").await.unwrap();
        assert!(!deleted);
}

#[tokio::test]
async fn test_rename_friend_group() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@rename_user_{suffix}:localhost");
        let room_id = format!("!rename_room_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("rename_user_{suffix}")).await;
        insert_room(&pool, &room_id).await;

        storage.create_friend_group(&room_id, &user_id, "OldName").await.unwrap();

        let renamed = storage.rename_friend_group(&room_id, &user_id, "OldName", "NewName").await.unwrap();
        assert!(renamed);

        let groups = storage.get_friend_groups(&room_id).await.unwrap().unwrap();
        let groups_arr = groups.get("groups").unwrap().as_array().unwrap();
        assert_eq!(groups_arr.len(), 1);
        assert_eq!(groups_arr[0].get("name").unwrap().as_str(), Some("NewName"));
}

#[tokio::test]
async fn test_rename_friend_group_nonexistent() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@rename_nf_user_{suffix}:localhost");
        let room_id = format!("!rename_nf_room_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("rename_nf_user_{suffix}")).await;
        insert_room(&pool, &room_id).await;

        let renamed = storage.rename_friend_group(&room_id, &user_id, "Ghost", "NewGhost").await.unwrap();
        assert!(!renamed);
}

#[tokio::test]
async fn test_add_and_remove_friend_from_group() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@addrem_user_{suffix}:localhost");
        let room_id = format!("!addrem_room_{suffix}:localhost");
        let friend_id = format!("@addrem_friend_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("addrem_user_{suffix}")).await;
        insert_room(&pool, &room_id).await;

        storage.create_friend_group(&room_id, &user_id, "Close").await.unwrap();

        let added = storage.add_friend_to_group(&room_id, &user_id, "Close", &friend_id).await.unwrap();
        assert!(added);

        let groups_for = storage.get_friend_groups_for_user(&room_id, &friend_id).await.unwrap();
        assert_eq!(groups_for, vec!["Close"]);

        let removed = storage.remove_friend_from_group(&room_id, &user_id, "Close", &friend_id).await.unwrap();
        assert!(removed);

        let groups_for = storage.get_friend_groups_for_user(&room_id, &friend_id).await.unwrap();
        assert!(groups_for.is_empty());
}

#[tokio::test]
async fn test_add_friend_to_group_duplicate() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@dup_add_user_{suffix}:localhost");
        let room_id = format!("!dup_add_room_{suffix}:localhost");
        let friend_id = format!("@dup_add_friend_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("dup_add_user_{suffix}")).await;
        insert_room(&pool, &room_id).await;

        storage.create_friend_group(&room_id, &user_id, "Test").await.unwrap();

        storage.add_friend_to_group(&room_id, &user_id, "Test", &friend_id).await.unwrap();

        let added = storage.add_friend_to_group(&room_id, &user_id, "Test", &friend_id).await.unwrap();
        assert!(!added);
}

#[tokio::test]
async fn test_add_friend_to_group_nonexistent_group() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@nogroup_add_user_{suffix}:localhost");
        let room_id = format!("!nogroup_add_room_{suffix}:localhost");
        let friend_id = format!("@nogroup_add_friend_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("nogroup_add_user_{suffix}")).await;
        insert_room(&pool, &room_id).await;

        let added = storage.add_friend_to_group(&room_id, &user_id, "Ghost", &friend_id).await.unwrap();
        assert!(!added);
}

#[tokio::test]
async fn test_remove_friend_from_group_not_member() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@rem_notmember_user_{suffix}:localhost");
        let room_id = format!("!rem_notmember_room_{suffix}:localhost");
        let friend_id = format!("@rem_notmember_friend_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("rem_notmember_user_{suffix}")).await;
        insert_room(&pool, &room_id).await;

        storage.create_friend_group(&room_id, &user_id, "Test").await.unwrap();

        let removed = storage.remove_friend_from_group(&room_id, &user_id, "Test", &friend_id).await.unwrap();
        assert!(!removed);
}

#[tokio::test]
async fn test_get_friend_groups_for_user_multiple_groups() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@multigroup_user_{suffix}:localhost");
        let room_id = format!("!multigroup_room_{suffix}:localhost");
        let friend_id = format!("@multigroup_friend_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("multigroup_user_{suffix}")).await;
        insert_room(&pool, &room_id).await;

        storage.create_friend_group(&room_id, &user_id, "GroupA").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        storage.create_friend_group(&room_id, &user_id, "GroupB").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;

        storage.add_friend_to_group(&room_id, &user_id, "GroupA", &friend_id).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        storage.add_friend_to_group(&room_id, &user_id, "GroupB", &friend_id).await.unwrap();

        let groups_for = storage.get_friend_groups_for_user(&room_id, &friend_id).await.unwrap();
        assert_eq!(groups_for.len(), 2);
        assert!(groups_for.contains(&"GroupA".to_string()));
        assert!(groups_for.contains(&"GroupB".to_string()));
}

#[tokio::test]
async fn test_get_friend_requests_empty() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let room_id = format!("!freq_empty_room_{suffix}:localhost");

        let result = storage.get_friend_requests(&room_id, "incoming").await.unwrap();
        assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_user_friend_ids() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@friendids_user_{suffix}:localhost");
        let room_id = format!("!friendids_room_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("friendids_user_{suffix}")).await;
        insert_room(&pool, &room_id).await;
        insert_event(
            &pool,
            &format!("$create_fids_{suffix}"),
            &room_id,
            &user_id,
            "m.room.create",
            None,
            &json!({"type": "m.friends"}),
        )
        .await;
        insert_event(
            &pool,
            &format!("$flist_fids_{suffix}"),
            &room_id,
            &user_id,
            "m.friends.list",
            Some(""),
            &json!({
                "friends": [
                    {"user_id": "@alice:localhost"},
                    {"user_id": "@bob:localhost"}
                ]
            }),
        )
        .await;

        let friend_ids = storage.get_user_friend_ids(&user_id).await.unwrap();
        assert_eq!(friend_ids.len(), 2);
        assert!(friend_ids.contains(&"@alice:localhost".to_string()));
        assert!(friend_ids.contains(&"@bob:localhost".to_string()));
}

#[tokio::test]
async fn test_get_user_friend_ids_no_room() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@nofriends_user_{suffix}:localhost");

        let friend_ids = storage.get_user_friend_ids(&user_id).await.unwrap();
        assert!(friend_ids.is_empty());
}

#[tokio::test]
async fn test_get_mutual_friends() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_a = format!("@mutual_a_{suffix}:localhost");
        let user_b = format!("@mutual_b_{suffix}:localhost");
        let room_a = format!("!mutual_room_a_{suffix}:localhost");
        let room_b = format!("!mutual_room_b_{suffix}:localhost");

        insert_user(&pool, &user_a, &format!("mutual_a_{suffix}")).await;
        insert_user(&pool, &user_b, &format!("mutual_b_{suffix}")).await;
        insert_room(&pool, &room_a).await;
        insert_room(&pool, &room_b).await;

        insert_event(
            &pool,
            &format!("$mutual_create_a_{suffix}"),
            &room_a,
            &user_a,
            "m.room.create",
            None,
            &json!({"type": "m.friends"}),
        )
        .await;
        insert_event(
            &pool,
            &format!("$mutual_flist_a_{suffix}"),
            &room_a,
            &user_a,
            "m.friends.list",
            Some(""),
            &json!({
                "friends": [
                    {"user_id": "@shared1:localhost"},
                    {"user_id": "@shared2:localhost"},
                    {"user_id": "@unique_a:localhost"}
                ]
            }),
        )
        .await;

        insert_event(
            &pool,
            &format!("$mutual_create_b_{suffix}"),
            &room_b,
            &user_b,
            "m.room.create",
            None,
            &json!({"type": "m.friends"}),
        )
        .await;
        insert_event(
            &pool,
            &format!("$mutual_flist_b_{suffix}"),
            &room_b,
            &user_b,
            "m.friends.list",
            Some(""),
            &json!({
                "friends": [
                    {"user_id": "@shared1:localhost"},
                    {"user_id": "@shared2:localhost"},
                    {"user_id": "@unique_b:localhost"}
                ]
            }),
        )
        .await;

        let mutual = storage.get_mutual_friends(&user_a, &user_b).await.unwrap();
        assert_eq!(mutual.len(), 2);
        assert!(mutual.contains(&"@shared1:localhost".to_string()));
        assert!(mutual.contains(&"@shared2:localhost".to_string()));
}

#[tokio::test]
async fn test_find_friend_lists_by_dm_room_id() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_id = format!("@dm_user_{suffix}:localhost");
        let room_id = format!("!dm_room_{suffix}:localhost");
        let dm_room_id = format!("!dm_target_{suffix}:localhost");

        insert_user(&pool, &user_id, &format!("dm_user_{suffix}")).await;
        insert_room(&pool, &room_id).await;
        let content = json!({
            "friends": [
                {"user_id": "@friend1:localhost", "dm_room_id": dm_room_id},
                {"user_id": "@friend2:localhost", "dm_room_id": "!other_dm:localhost"}
            ]
        });
        insert_event(&pool, &format!("$dm_flist_{suffix}"), &room_id, &user_id, "m.friends.list", Some(""), &content)
            .await;

        let links = storage.find_friend_lists_by_dm_room_id(&dm_room_id).await.unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].owner_user_id, user_id);
        assert_eq!(links[0].friend_room_id, room_id);
}

#[tokio::test]
async fn test_get_shared_rooms() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_a = format!("@shared_a_{suffix}:localhost");
        let user_b = format!("@shared_b_{suffix}:localhost");
        let shared_room = format!("!shared_room_{suffix}:localhost");

        insert_user(&pool, &user_a, &format!("shared_a_{suffix}")).await;
        insert_user(&pool, &user_b, &format!("shared_b_{suffix}")).await;
        insert_room(&pool, &shared_room).await;

        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO room_memberships (room_id, user_id, membership, joined_ts) VALUES ($1, $2, 'join', $3)",
        )
        .bind(&shared_room)
        .bind(&user_a)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO room_memberships (room_id, user_id, membership, joined_ts) VALUES ($1, $2, 'join', $3)",
        )
        .bind(&shared_room)
        .bind(&user_b)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();

        let shared = storage.get_shared_rooms(&user_a, &user_b).await.unwrap();
        assert_eq!(shared.len(), 1);
        assert_eq!(shared[0], shared_room);
}

#[tokio::test]
async fn test_get_shared_rooms_none() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let user_a = format!("@noshared_a_{suffix}:localhost");
        let user_b = format!("@noshared_b_{suffix}:localhost");

        let shared = storage.get_shared_rooms(&user_a, &user_b).await.unwrap();
        assert!(shared.is_empty());
}
