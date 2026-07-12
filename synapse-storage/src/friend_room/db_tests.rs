use std::env;
use std::sync::Arc;

use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};

use super::*;

async fn test_pool() -> Arc<Pool<Postgres>> {
    let db_url = env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
    let pool =
        PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
    Arc::new(pool)
}

async fn ensure_test_user(pool: &Pool<Postgres>, user_id: &str) {
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

async fn ensure_test_room(pool: &Pool<Postgres>, room_id: &str) {
    sqlx::query(
        "INSERT INTO rooms (room_id, room_version, is_public, creator, created_ts) VALUES ($1, '1', false, '@test:localhost', EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (room_id) DO NOTHING",
    )
    .bind(room_id)
    .execute(pool)
    .await
    .ok();
}

async fn insert_event(
    pool: &Pool<Postgres>,
    room_id: &str,
    sender: &str,
    event_type: &str,
    state_key: &str,
    content: &serde_json::Value,
) {
    let event_id = format!("${}", uuid::Uuid::new_v4().simple());
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO events (event_id, room_id, sender, event_type, state_key, content, origin_server_ts, depth) VALUES ($1, $2, $3, $4, $5, $6, $7, 1)",
    )
    .bind(&event_id)
    .bind(room_id)
    .bind(sender)
    .bind(event_type)
    .bind(state_key)
    .bind(content)
    .bind(now)
    .execute(pool)
    .await
    .ok();
}

async fn cleanup_all(pool: &Pool<Postgres>, suffix: &str) {
    let pattern = format!("%{}%", suffix);
    let _ = sqlx::query("DELETE FROM friend_requests WHERE sender_id LIKE $1 OR receiver_id LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM friends WHERE user_id LIKE $1 OR friend_id LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM friend_categories WHERE user_id LIKE $1").bind(&pattern).execute(pool).await;
    let _ =
        sqlx::query("DELETE FROM events WHERE sender LIKE $1 OR room_id LIKE $1").bind(&pattern).execute(pool).await;
    let _ = sqlx::query("DELETE FROM room_memberships WHERE user_id LIKE $1 OR room_id LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM room_summaries WHERE room_id LIKE $1").bind(&pattern).execute(pool).await;
    let _ =
        sqlx::query("DELETE FROM rooms WHERE room_id LIKE $1 OR creator LIKE $1").bind(&pattern).execute(pool).await;
    let _ = sqlx::query("DELETE FROM users WHERE user_id LIKE $1").bind(&pattern).execute(pool).await;
}

// ——————————————————————————————————————————————
// get_friend_list_room_id
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_get_friend_list_room_id() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_id = format!("@fr_test_{suffix}:localhost");
    let room_id = format!("!fr_room_{suffix}:localhost");
    ensure_test_user(&pool, &user_id).await;
    ensure_test_room(&pool, &room_id).await;

    // Insert m.room.create with type=m.friends
    let content = json!({"type": "m.friends", "creator": &user_id});
    insert_event(&pool, &room_id, &user_id, "m.room.create", "", &content).await;

    let storage = FriendRoomStorage::new(pool.clone());

    // Found
    let result = storage.get_friend_list_room_id(&user_id).await.expect("query should succeed");
    assert_eq!(result.as_deref(), Some(room_id.as_str()), "should find the friend list room");

    // Not found: user with no friend-list create event
    let other = format!("@fr_other_{suffix}:localhost");
    ensure_test_user(&pool, &other).await;
    let result = storage.get_friend_list_room_id(&other).await.expect("query should succeed");
    assert!(result.is_none(), "should not find room for user without friend-list");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// get_friend_list_content
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_get_friend_list_content() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_id = format!("@fr_test_{suffix}:localhost");
    let room_id = format!("!fr_room_{suffix}:localhost");
    ensure_test_room(&pool, &room_id).await;

    // Insert a friend-list event with content
    let friend_content = json!({
        "friends": [
            {"user_id": format!("@friend_{suffix}:localhost"), "display_name": "Friend One"}
        ],
        "version": 1
    });
    insert_event(&pool, &room_id, &user_id, "m.friends.list", "", &friend_content).await;

    let storage = FriendRoomStorage::new(pool.clone());

    // Found with content
    let result = storage.get_friend_list_content(&room_id).await.expect("query should succeed");
    assert!(result.is_some(), "should find friend list content");
    let content = result.unwrap();
    let friends = content.get("friends").and_then(|f| f.as_array());
    assert!(friends.is_some_and(|f| f.len() == 1), "should have one friend");

    // Not found: room with no friend-list event
    let other_room = format!("!fr_other_{suffix}:localhost");
    ensure_test_room(&pool, &other_room).await;
    let result = storage.get_friend_list_content(&other_room).await.expect("query should succeed");
    assert!(result.is_none(), "should return None for room without friend-list");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// find_friend_lists_by_dm_room_id
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_find_friend_lists_by_dm_room_id() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_id = format!("@fr_test_{suffix}:localhost");
    let friend_room_id = format!("!fr_room_{suffix}:localhost");
    let dm_room_id = format!("!dm_room_{suffix}:localhost");
    ensure_test_user(&pool, &user_id).await;
    ensure_test_room(&pool, &friend_room_id).await;

    let friend_content = json!({
        "friends": [
            {"user_id": format!("@friend_{suffix}:localhost"), "dm_room_id": dm_room_id}
        ]
    });
    insert_event(&pool, &friend_room_id, &user_id, "m.friends.list", "", &friend_content).await;

    let storage = FriendRoomStorage::new(pool.clone());

    // Finds
    let results = storage.find_friend_lists_by_dm_room_id(&dm_room_id).await.expect("query should succeed");
    assert_eq!(results.len(), 1, "should find one friend list");
    assert_eq!(results[0].owner_user_id, user_id);

    // Not found: DM room that is not in any friend list
    let other_dm = format!("!dm_other_{suffix}:localhost");
    let results = storage.find_friend_lists_by_dm_room_id(&other_dm).await.expect("query should succeed");
    assert!(results.is_empty(), "should return empty for unknown DM room");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// get_effective_direct_links_fallback
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_get_effective_direct_links_fallback() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_a = format!("@fr_a_{suffix}:localhost");
    let user_b = format!("@fr_b_{suffix}:localhost");
    let dm_room = format!("!dm_{suffix}:localhost");
    ensure_test_user(&pool, &user_a).await;
    ensure_test_user(&pool, &user_b).await;
    ensure_test_room(&pool, &dm_room).await;

    let now = chrono::Utc::now().timestamp_millis();
    // Insert room_summaries with is_direct=true
    sqlx::query(
        "INSERT INTO room_summaries (room_id, is_direct, updated_ts, created_ts) VALUES ($1, true, $2, $2) ON CONFLICT (room_id) DO NOTHING",
    )
    .bind(&dm_room)
    .bind(now)
    .execute(&*pool)
    .await
    .ok();

    // Insert room_memberships for both users
    sqlx::query(
        "INSERT INTO room_memberships (room_id, user_id, membership) VALUES ($1, $2, 'join') ON CONFLICT (room_id, user_id) DO NOTHING",
    )
    .bind(&dm_room)
    .bind(&user_a)
    .execute(&*pool)
    .await
    .ok();
    sqlx::query(
        "INSERT INTO room_memberships (room_id, user_id, membership) VALUES ($1, $2, 'join') ON CONFLICT (room_id, user_id) DO NOTHING",
    )
    .bind(&dm_room)
    .bind(&user_b)
    .execute(&*pool)
    .await
    .ok();

    let storage = FriendRoomStorage::new(pool.clone());

    // Returns results
    let results = storage.get_effective_direct_links_fallback(&user_a).await.expect("query should succeed");
    assert_eq!(results.len(), 1, "should find one direct room");
    assert_eq!(results[0].other_user_id, user_b);
    assert_eq!(results[0].room_id, dm_room);

    // Empty: user with no direct rooms
    let user_c = format!("@fr_c_{suffix}:localhost");
    ensure_test_user(&pool, &user_c).await;
    let results = storage.get_effective_direct_links_fallback(&user_c).await.expect("query should succeed");
    assert!(results.is_empty(), "should return empty for user without direct rooms");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// get_existing_direct_room_id
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_get_existing_direct_room_id() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_a = format!("@fr_a_{suffix}:localhost");
    let user_b = format!("@fr_b_{suffix}:localhost");
    let dm_room = format!("!dm_{suffix}:localhost");
    ensure_test_user(&pool, &user_a).await;
    ensure_test_user(&pool, &user_b).await;
    ensure_test_room(&pool, &dm_room).await;

    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO room_summaries (room_id, is_direct, updated_ts, created_ts) VALUES ($1, true, $2, $2) ON CONFLICT (room_id) DO NOTHING",
    )
    .bind(&dm_room)
    .bind(now)
    .execute(&*pool)
    .await
    .ok();
    sqlx::query(
        "INSERT INTO room_memberships (room_id, user_id, membership) VALUES ($1, $2, 'join') ON CONFLICT (room_id, user_id) DO NOTHING",
    )
    .bind(&dm_room)
    .bind(&user_a)
    .execute(&*pool)
    .await
    .ok();
    sqlx::query(
        "INSERT INTO room_memberships (room_id, user_id, membership) VALUES ($1, $2, 'join') ON CONFLICT (room_id, user_id) DO NOTHING",
    )
    .bind(&dm_room)
    .bind(&user_b)
    .execute(&*pool)
    .await
    .ok();

    let storage = FriendRoomStorage::new(pool.clone());

    // Found
    let result = storage.get_existing_direct_room_id(&user_a, &user_b).await.expect("query should succeed");
    assert_eq!(result.as_deref(), Some(dm_room.as_str()), "should find the DM room");

    // Not found: pair not sharing a DM
    let user_c = format!("@fr_c_{suffix}:localhost");
    ensure_test_user(&pool, &user_c).await;
    let result = storage.get_existing_direct_room_id(&user_a, &user_c).await.expect("query should succeed");
    assert!(result.is_none(), "should not find room for unrelated pair");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// get_dm_partner_for_room
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_get_dm_partner_for_room() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_a = format!("@fr_a_{suffix}:localhost");
    let user_b = format!("@fr_b_{suffix}:localhost");
    let room_id = format!("!room_{suffix}:localhost");
    ensure_test_user(&pool, &user_a).await;
    ensure_test_user(&pool, &user_b).await;
    ensure_test_room(&pool, &room_id).await;

    sqlx::query(
        "INSERT INTO room_memberships (room_id, user_id, membership, display_name, avatar_url) VALUES ($1, $2, 'join', 'User A', '') ON CONFLICT (room_id, user_id) DO NOTHING",
    )
    .bind(&room_id)
    .bind(&user_a)
    .execute(&*pool)
    .await
    .ok();
    sqlx::query(
        "INSERT INTO room_memberships (room_id, user_id, membership, display_name, avatar_url) VALUES ($1, $2, 'join', 'User B', '') ON CONFLICT (room_id, user_id) DO NOTHING",
    )
    .bind(&room_id)
    .bind(&user_b)
    .execute(&*pool)
    .await
    .ok();

    let storage = FriendRoomStorage::new(pool.clone());

    // Found
    let partner = storage.get_dm_partner_for_room(&room_id, &user_a).await.expect("query should succeed");
    assert!(partner.is_some(), "should find the partner");
    let p = partner.unwrap();
    assert_eq!(p.user_id, user_b, "partner should be user_b");
    assert_eq!(p.display_name, "User B");

    // Not found: room with only one member
    let solo_room = format!("!solo_{suffix}:localhost");
    let solo_user = format!("@fr_solo_{suffix}:localhost");
    ensure_test_user(&pool, &solo_user).await;
    ensure_test_room(&pool, &solo_room).await;
    sqlx::query(
        "INSERT INTO room_memberships (room_id, user_id, membership) VALUES ($1, $2, 'join') ON CONFLICT (room_id, user_id) DO NOTHING",
    )
    .bind(&solo_room)
    .bind(&solo_user)
    .execute(&*pool)
    .await
    .ok();
    let result = storage.get_dm_partner_for_room(&solo_room, &solo_user).await.expect("query should succeed");
    assert!(result.is_none(), "should return None for room with single member");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// get_friend_requests (from events)
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_get_friend_requests() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_id = format!("@fr_test_{suffix}:localhost");
    let room_id = format!("!fr_room_{suffix}:localhost");
    ensure_test_user(&pool, &user_id).await;
    ensure_test_room(&pool, &room_id).await;

    let requests_content = json!({
        "requests": [
            {"sender_id": format!("@sender_{suffix}:localhost"), "receiver_id": &user_id, "status": "pending"},
            {"sender_id": format!("@sender2_{suffix}:localhost"), "receiver_id": &user_id, "status": "pending"}
        ]
    });
    insert_event(&pool, &room_id, &user_id, "m.friend_requests.incoming", "", &requests_content).await;

    let storage = FriendRoomStorage::new(pool.clone());

    // Returns list
    let incoming = storage.get_friend_requests(&room_id, "incoming").await.expect("query should succeed");
    assert_eq!(incoming.len(), 2, "should have 2 incoming requests");

    // Empty: different request type
    let outgoing = storage.get_friend_requests(&room_id, "outgoing").await.expect("query should succeed");
    assert!(outgoing.is_empty(), "should return empty for outgoing type");

    // Filters: non-existent type
    let nonsense = storage.get_friend_requests(&room_id, "nonsense").await.expect("query should succeed");
    assert!(nonsense.is_empty(), "should return empty for unknown type");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// is_friend
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_is_friend() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_id = format!("@fr_test_{suffix}:localhost");
    let friend_id = format!("@friend_{suffix}:localhost");
    let room_id = format!("!fr_room_{suffix}:localhost");
    ensure_test_user(&pool, &user_id).await;
    ensure_test_room(&pool, &room_id).await;

    let friend_content = json!({
        "friends": [
            {"user_id": &friend_id, "display_name": "My Friend"}
        ]
    });
    insert_event(&pool, &room_id, &user_id, "m.friends.list", "", &friend_content).await;

    let storage = FriendRoomStorage::new(pool.clone());

    // True: friend is in list
    assert!(storage.is_friend(&room_id, &friend_id).await.expect("query should succeed"), "should be a friend");

    // False: user not in list
    let stranger = format!("@stranger_{suffix}:localhost");
    assert!(!storage.is_friend(&room_id, &stranger).await.expect("query should succeed"), "should not be a friend");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// get_friend_info
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_get_friend_info() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_id = format!("@fr_test_{suffix}:localhost");
    let friend_id = format!("@friend_{suffix}:localhost");
    let room_id = format!("!fr_room_{suffix}:localhost");
    ensure_test_user(&pool, &user_id).await;
    ensure_test_room(&pool, &room_id).await;

    let friend_content = json!({
        "friends": [
            {"user_id": &friend_id, "display_name": "My Friend", "avatar_url": "mxc://avatar"}
        ]
    });
    insert_event(&pool, &room_id, &user_id, "m.friends.list", "", &friend_content).await;

    let storage = FriendRoomStorage::new(pool.clone());

    // Found
    let info = storage.get_friend_info(&room_id, &friend_id).await.expect("query should succeed");
    assert!(info.is_some(), "should find friend info");
    let info = info.unwrap();
    assert_eq!(info.get("display_name").and_then(|v| v.as_str()), Some("My Friend"));
    assert_eq!(info.get("avatar_url").and_then(|v| v.as_str()), Some("mxc://avatar"));

    // Not found: friend not in list
    let stranger = format!("@stranger_{suffix}:localhost");
    let info = storage.get_friend_info(&room_id, &stranger).await.expect("query should succeed");
    assert!(info.is_none(), "should return None for non-friend");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// get_friend_groups
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_get_friend_groups() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_id = format!("@fr_test_{suffix}:localhost");
    let room_id = format!("!fr_room_{suffix}:localhost");
    ensure_test_user(&pool, &user_id).await;
    ensure_test_room(&pool, &room_id).await;

    let groups_content = json!({
        "groups": [
            {"name": "close_friends", "members": [], "created_ts": 1000, "updated_ts": 1000}
        ],
        "version": 1,
        "updated_ts": 1000
    });
    insert_event(&pool, &room_id, &user_id, "m.friends.groups", "", &groups_content).await;

    let storage = FriendRoomStorage::new(pool.clone());

    // With groups
    let result = storage.get_friend_groups(&room_id).await.expect("query should succeed");
    assert!(result.is_some(), "should find groups");
    let groups = result.unwrap();
    let arr = groups.get("groups").and_then(|g| g.as_array());
    assert!(arr.is_some_and(|a| a.len() == 1), "should have one group");

    // None: room with no groups event
    let other_room = format!("!fr_other_{suffix}:localhost");
    ensure_test_room(&pool, &other_room).await;
    let result = storage.get_friend_groups(&other_room).await.expect("query should succeed");
    assert!(result.is_none(), "should return None for room without groups");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// get_friend_groups_for_user
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_get_friend_groups_for_user() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_id = format!("@fr_test_{suffix}:localhost");
    let friend_id = format!("@friend_{suffix}:localhost");
    let room_id = format!("!fr_room_{suffix}:localhost");
    ensure_test_user(&pool, &user_id).await;
    ensure_test_room(&pool, &room_id).await;

    let groups_content = json!({
        "groups": [
            {"name": "close_friends", "members": [&friend_id], "created_ts": 1000, "updated_ts": 1000},
            {"name": "work", "members": [], "created_ts": 1001, "updated_ts": 1001}
        ],
        "version": 1,
        "updated_ts": 1000
    });
    insert_event(&pool, &room_id, &user_id, "m.friends.groups", "", &groups_content).await;

    let storage = FriendRoomStorage::new(pool.clone());

    // Returns groups
    let group_names = storage.get_friend_groups_for_user(&room_id, &friend_id).await.expect("query should succeed");
    assert_eq!(group_names.len(), 1, "should be in one group");
    assert_eq!(group_names[0], "close_friends");

    // Empty: friend not in any group
    let stranger = format!("@stranger_{suffix}:localhost");
    let group_names = storage.get_friend_groups_for_user(&room_id, &stranger).await.expect("query should succeed");
    assert!(group_names.is_empty(), "should return empty for non-member");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// create_friend_group
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_create_friend_group() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_id = format!("@fr_test_{suffix}:localhost");
    let room_id = format!("!fr_room_{suffix}:localhost");
    ensure_test_user(&pool, &user_id).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = FriendRoomStorage::new(pool.clone());
    let group_name = format!("test_group_{suffix}");

    // Creates
    storage.create_friend_group(&room_id, &user_id, &group_name).await.expect("create should succeed");

    let groups = storage.get_friend_groups(&room_id).await.expect("query should succeed");
    assert!(groups.is_some(), "groups should exist after create");
    let groups = groups.unwrap();
    let arr = groups.get("groups").and_then(|g| g.as_array()).unwrap();
    assert_eq!(arr.len(), 1, "should have one group");
    assert_eq!(arr[0].get("name").and_then(|n| n.as_str()), Some(group_name.as_str()));

    // Duplicate: creating same name again should error
    let err = storage.create_friend_group(&room_id, &user_id, &group_name).await;
    assert!(err.is_err(), "duplicate create should return error");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// delete_friend_group
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_delete_friend_group() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_id = format!("@fr_test_{suffix}:localhost");
    let room_id = format!("!fr_room_{suffix}:localhost");
    let group_name = format!("temp_group_{suffix}");
    ensure_test_user(&pool, &user_id).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = FriendRoomStorage::new(pool.clone());

    // Create a group first
    storage.create_friend_group(&room_id, &user_id, &group_name).await.expect("create should succeed");

    // Deletes
    let deleted = storage.delete_friend_group(&room_id, &user_id, &group_name).await.expect("delete should succeed");
    assert!(deleted, "should return true when group was deleted");

    let groups = storage.get_friend_groups(&room_id).await.expect("query should succeed");
    let groups_val = groups.unwrap_or(json!({"groups": []}));
    let arr = groups_val.get("groups").and_then(|g| g.as_array()).unwrap();
    assert!(arr.is_empty(), "groups should be empty after delete");

    // Idempotent: delete non-existent group returns false
    let deleted = storage.delete_friend_group(&room_id, &user_id, &group_name).await.expect("delete should succeed");
    assert!(!deleted, "should return false when group does not exist");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// rename_friend_group
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_rename_friend_group() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_id = format!("@fr_test_{suffix}:localhost");
    let room_id = format!("!fr_room_{suffix}:localhost");
    let old_name = format!("old_group_{suffix}");
    let new_name = format!("new_group_{suffix}");
    ensure_test_user(&pool, &user_id).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = FriendRoomStorage::new(pool.clone());
    storage.create_friend_group(&room_id, &user_id, &old_name).await.expect("create should succeed");

    // Renames
    let renamed =
        storage.rename_friend_group(&room_id, &user_id, &old_name, &new_name).await.expect("rename should succeed");
    assert!(renamed, "should return true on successful rename");

    let groups = storage.get_friend_groups(&room_id).await.expect("query should succeed");
    let groups = groups.unwrap();
    let arr = groups.get("groups").and_then(|g| g.as_array()).unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0].get("name").and_then(|n| n.as_str()), Some(new_name.as_str()), "group should have new name");

    // Not found: rename non-existent group
    let fake = format!("fake_group_{suffix}");
    let renamed =
        storage.rename_friend_group(&room_id, &user_id, &fake, "irrelevant").await.expect("rename should succeed");
    assert!(!renamed, "should return false when group not found");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// add_friend_to_group
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_add_friend_to_group() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_id = format!("@fr_test_{suffix}:localhost");
    let friend_id = format!("@friend_{suffix}:localhost");
    let room_id = format!("!fr_room_{suffix}:localhost");
    let group_name = format!("group_{suffix}");
    ensure_test_user(&pool, &user_id).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = FriendRoomStorage::new(pool.clone());
    storage.create_friend_group(&room_id, &user_id, &group_name).await.expect("create should succeed");

    // Add friend
    let added =
        storage.add_friend_to_group(&room_id, &user_id, &group_name, &friend_id).await.expect("add should succeed");
    assert!(added, "should return true when friend was added");

    let group_names = storage.get_friend_groups_for_user(&room_id, &friend_id).await.expect("query should succeed");
    assert!(group_names.contains(&group_name), "friend should be in the group");

    // Add same friend again should return false
    let added_again =
        storage.add_friend_to_group(&room_id, &user_id, &group_name, &friend_id).await.expect("add should succeed");
    assert!(!added_again, "should return false for duplicate addition");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// remove_friend_from_group
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_remove_friend_from_group() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let user_id = format!("@fr_test_{suffix}:localhost");
    let friend_id = format!("@friend_{suffix}:localhost");
    let room_id = format!("!fr_room_{suffix}:localhost");
    let group_name = format!("group_{suffix}");
    ensure_test_user(&pool, &user_id).await;
    ensure_test_room(&pool, &room_id).await;

    let storage = FriendRoomStorage::new(pool.clone());
    storage.create_friend_group(&room_id, &user_id, &group_name).await.expect("create should succeed");
    storage.add_friend_to_group(&room_id, &user_id, &group_name, &friend_id).await.expect("add should succeed");

    // Remove friend
    let removed = storage
        .remove_friend_from_group(&room_id, &user_id, &group_name, &friend_id)
        .await
        .expect("remove should succeed");
    assert!(removed, "should return true when friend was removed");

    let group_names = storage.get_friend_groups_for_user(&room_id, &friend_id).await.expect("query should succeed");
    assert!(!group_names.contains(&group_name), "friend should no longer be in the group");

    // Remove again should return false
    let removed_again = storage
        .remove_friend_from_group(&room_id, &user_id, &group_name, &friend_id)
        .await
        .expect("remove should succeed");
    assert!(!removed_again, "should return false when friend already removed");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// create_friend_request (friend_requests table)
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_create_friend_request() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let sender = format!("@fr_sender_{suffix}:localhost");
    let receiver = format!("@fr_receiver_{suffix}:localhost");
    ensure_test_user(&pool, &sender).await;
    ensure_test_user(&pool, &receiver).await;

    let storage = FriendRoomStorage::new(pool.clone());

    // Creates
    let id = storage.create_friend_request(&sender, &receiver, Some("Hello!")).await.expect("create should succeed");
    assert!(id > 0, "should return a valid ID");

    // Duplicate: upsert updates the status
    let id2 = storage
        .create_friend_request(&sender, &receiver, Some("Updated message"))
        .await
        .expect("upsert should succeed");
    assert_eq!(id2, id, "upsert should return same ID");

    // Verify the record was updated
    let record = storage.get_friend_request(&sender, &receiver).await.expect("query should succeed");
    assert!(record.is_some());
    let record = record.unwrap();
    assert_eq!(record.status, "pending");
    assert_eq!(record.message.as_deref(), Some("Updated message"));

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// get_friend_request
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_get_friend_request() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let sender = format!("@fr_sender_{suffix}:localhost");
    let receiver = format!("@fr_receiver_{suffix}:localhost");
    ensure_test_user(&pool, &sender).await;
    ensure_test_user(&pool, &receiver).await;

    let storage = FriendRoomStorage::new(pool.clone());
    storage.create_friend_request(&sender, &receiver, Some("Hi")).await.expect("create should succeed");

    // Found
    let record = storage.get_friend_request(&sender, &receiver).await.expect("query should succeed");
    assert!(record.is_some(), "should find the friend request");
    let r = record.unwrap();
    assert_eq!(r.sender_id, sender);
    assert_eq!(r.receiver_id, receiver);
    assert_eq!(r.status, "pending");

    // Not found: non-existent pair
    let other = format!("@fr_other_{suffix}:localhost");
    ensure_test_user(&pool, &other).await;
    let record = storage.get_friend_request(&sender, &other).await.expect("query should succeed");
    assert!(record.is_none(), "should return None for non-existent request");

    cleanup_all(&pool, &suffix).await;
}

// ——————————————————————————————————————————————
// get_pending_friend_request
// ——————————————————————————————————————————————

#[tokio::test]
async fn test_get_pending_friend_request() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    cleanup_all(&pool, &suffix).await;

    let sender = format!("@fr_sender_{suffix}:localhost");
    let receiver = format!("@fr_receiver_{suffix}:localhost");
    ensure_test_user(&pool, &sender).await;
    ensure_test_user(&pool, &receiver).await;

    let storage = FriendRoomStorage::new(pool.clone());
    storage.create_friend_request(&sender, &receiver, Some("Hi")).await.expect("create should succeed");

    // Found: pending request
    let record = storage.get_pending_friend_request(&sender, &receiver).await.expect("query should succeed");
    assert!(record.is_some(), "should find the pending request");
    assert_eq!(record.unwrap().status, "pending");

    // Not found: non-existent pair
    let stranger = format!("@fr_stranger_{suffix}:localhost");
    ensure_test_user(&pool, &stranger).await;
    let record = storage.get_pending_friend_request(&sender, &stranger).await.expect("query should succeed");
    assert!(record.is_none(), "should return None for pair without pending request");

    // Not found: pair with non-pending status
    storage.update_friend_request_status(&sender, &receiver, "accepted").await.expect("update should succeed");
    let record = storage.get_pending_friend_request(&sender, &receiver).await.expect("query should succeed");
    assert!(record.is_none(), "should not return accepted request as pending");

    cleanup_all(&pool, &suffix).await;
}
