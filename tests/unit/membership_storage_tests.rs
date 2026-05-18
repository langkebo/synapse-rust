use sqlx::{Pool, Postgres};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::storage::membership::{RoomMember, RoomMemberStorage, UserRoomMembership};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database() -> Option<Arc<Pool<Postgres>>> {
    let pool = match synapse_rust::test_utils::prepare_empty_isolated_test_pool().await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!(
                "Skipping membership storage tests because test database is unavailable: {error}"
            );
            return None;
        }
    };

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
    .execute(&*pool)
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
    .execute(&*pool)
    .await
    .expect("Failed to create rooms table");

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
    .execute(&*pool)
    .await
    .expect("Failed to create room_memberships table");

    Some(pool)
}

fn create_storage(pool: &Arc<Pool<Postgres>>) -> RoomMemberStorage {
    RoomMemberStorage::new(pool, "localhost")
}

async fn insert_user(pool: &Pool<Postgres>, user_id: &str, username: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(username)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to insert test user");
}

async fn insert_room(pool: &Pool<Postgres>, room_id: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO rooms (room_id, created_ts) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(room_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to insert test room");
}

async fn insert_membership(
    pool: &Pool<Postgres>,
    room_id: &str,
    user_id: &str,
    membership: &str,
) {
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

#[test]
fn test_room_member_struct_all_fields() {
    let member = RoomMember {
        room_id: "!room123:localhost".to_string(),
        user_id: "@alice:localhost".to_string(),
        sender: Some("@alice:localhost".to_string()),
        membership: "join".to_string(),
        event_id: Some("$event123:localhost".to_string()),
        event_type: Some("m.room.member".to_string()),
        display_name: Some("Alice".to_string()),
        avatar_url: Some("mxc://localhost/avatar".to_string()),
        is_banned: Some(false),
        invite_token: None,
        updated_ts: Some(1234567890),
        joined_ts: Some(1234567890),
        left_ts: None,
        reason: None,
        banned_by: None,
        ban_reason: None,
        banned_ts: None,
        join_reason: None,
    };

    assert_eq!(member.room_id, "!room123:localhost");
    assert_eq!(member.user_id, "@alice:localhost");
    assert_eq!(member.membership, "join");
    assert_eq!(member.display_name.as_deref(), Some("Alice"));
    assert!(!member.is_banned.unwrap());
}

#[test]
fn test_room_member_struct_minimal() {
    let member = RoomMember {
        room_id: "!room:localhost".to_string(),
        user_id: "@user:localhost".to_string(),
        sender: None,
        membership: "join".to_string(),
        event_id: None,
        event_type: None,
        display_name: None,
        avatar_url: None,
        is_banned: None,
        invite_token: None,
        updated_ts: None,
        joined_ts: None,
        left_ts: None,
        reason: None,
        banned_by: None,
        ban_reason: None,
        banned_ts: None,
        join_reason: None,
    };

    assert_eq!(member.membership, "join");
    assert!(member.sender.is_none());
    assert!(member.event_id.is_none());
    assert!(member.display_name.is_none());
    assert!(member.is_banned.is_none());
}

#[test]
fn test_room_member_serialization() {
    let member = RoomMember {
        room_id: "!room:localhost".to_string(),
        user_id: "@user:localhost".to_string(),
        sender: Some("@user:localhost".to_string()),
        membership: "join".to_string(),
        event_id: Some("$event:localhost".to_string()),
        event_type: Some("m.room.member".to_string()),
        display_name: Some("User".to_string()),
        avatar_url: Some("mxc://localhost/avatar".to_string()),
        is_banned: Some(false),
        invite_token: None,
        updated_ts: Some(1234567890),
        joined_ts: Some(1234567890),
        left_ts: None,
        reason: None,
        banned_by: None,
        ban_reason: None,
        banned_ts: None,
        join_reason: None,
    };

    let json = serde_json::to_string(&member).unwrap();
    assert!(json.contains("join"));
    assert!(json.contains("@user:localhost"));
    assert!(json.contains("!room:localhost"));

    let deserialized: RoomMember = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.room_id, member.room_id);
    assert_eq!(deserialized.user_id, member.user_id);
    assert_eq!(deserialized.membership, member.membership);
}

#[test]
fn test_user_room_membership_struct() {
    let membership = UserRoomMembership {
        room_id: "!room:localhost".to_string(),
        membership: "join".to_string(),
    };

    assert_eq!(membership.room_id, "!room:localhost");
    assert_eq!(membership.membership, "join");
}

#[test]
fn test_room_member_banned_state() {
    let member = RoomMember {
        room_id: "!room:localhost".to_string(),
        user_id: "@bob:localhost".to_string(),
        sender: Some("@admin:localhost".to_string()),
        membership: "ban".to_string(),
        event_id: Some("$ban_event:localhost".to_string()),
        event_type: Some("m.room.member".to_string()),
        display_name: None,
        avatar_url: None,
        is_banned: Some(true),
        invite_token: None,
        updated_ts: Some(1234567890),
        joined_ts: None,
        left_ts: Some(1234567890),
        reason: Some("Spam".to_string()),
        banned_by: Some("@admin:localhost".to_string()),
        ban_reason: Some("Spam behavior".to_string()),
        banned_ts: Some(1234567890),
        join_reason: None,
    };

    assert_eq!(member.membership, "ban");
    assert!(member.is_banned.unwrap_or(false));
    assert_eq!(member.banned_by.as_deref(), Some("@admin:localhost"));
    assert_eq!(member.ban_reason.as_deref(), Some("Spam behavior"));
}

#[test]
fn test_room_member_invited_state() {
    let member = RoomMember {
        room_id: "!room:localhost".to_string(),
        user_id: "@charlie:localhost".to_string(),
        sender: Some("@alice:localhost".to_string()),
        membership: "invite".to_string(),
        event_id: Some("$invite_event:localhost".to_string()),
        event_type: Some("m.room.member".to_string()),
        display_name: Some("Charlie".to_string()),
        avatar_url: None,
        is_banned: Some(false),
        invite_token: Some("token123".to_string()),
        updated_ts: Some(1234567890),
        joined_ts: None,
        left_ts: None,
        reason: None,
        banned_by: None,
        ban_reason: None,
        banned_ts: None,
        join_reason: None,
    };

    assert_eq!(member.membership, "invite");
    assert_eq!(member.invite_token.as_deref(), Some("token123"));
    assert!(member.joined_ts.is_none());
}

#[test]
fn test_room_member_left_state() {
    let member = RoomMember {
        room_id: "!room:localhost".to_string(),
        user_id: "@dave:localhost".to_string(),
        sender: Some("@dave:localhost".to_string()),
        membership: "leave".to_string(),
        event_id: Some("$leave_event:localhost".to_string()),
        event_type: Some("m.room.member".to_string()),
        display_name: Some("Dave".to_string()),
        avatar_url: None,
        is_banned: Some(false),
        invite_token: None,
        updated_ts: Some(1234567900),
        joined_ts: Some(1234567800),
        left_ts: Some(1234567900),
        reason: Some("Leaving room".to_string()),
        banned_by: None,
        ban_reason: None,
        banned_ts: None,
        join_reason: None,
    };

    assert_eq!(member.membership, "leave");
    assert!(member.left_ts.is_some());
    assert!(member.joined_ts.is_some());
    assert!(member.left_ts.unwrap() > member.joined_ts.unwrap());
}

#[test]
fn test_room_member_forgotten_state() {
    let member = RoomMember {
        room_id: "!room:localhost".to_string(),
        user_id: "@eve:localhost".to_string(),
        sender: None,
        membership: "forget".to_string(),
        event_id: None,
        event_type: None,
        display_name: None,
        avatar_url: None,
        is_banned: None,
        invite_token: None,
        updated_ts: Some(1234567900),
        joined_ts: None,
        left_ts: Some(1234567900),
        reason: None,
        banned_by: None,
        ban_reason: None,
        banned_ts: None,
        join_reason: None,
    };

    assert_eq!(member.membership, "forget");
    assert!(member.left_ts.is_some());
}

#[tokio::test]
async fn test_add_member_join() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@addjoin_{suffix}:localhost");
    let room_id = format!("!addjoin_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("addjoin_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    let member = storage
        .add_member(&room_id, &user_id, "join", None, None, None)
        .await
        .unwrap();

    assert_eq!(member.room_id, room_id);
    assert_eq!(member.user_id, user_id);
    assert_eq!(member.membership, "join");
    assert!(member.event_id.is_some());
    assert!(member.joined_ts.is_some());
}

#[tokio::test]
async fn test_add_member_with_display_name() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@displayname_{suffix}:localhost");
    let room_id = format!("!displayname_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("displayname_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    let member = storage
        .add_member(&room_id, &user_id, "join", Some("Alice"), None, None)
        .await
        .unwrap();

    assert_eq!(member.display_name.as_deref(), Some("Alice"));
}

#[tokio::test]
async fn test_add_member_with_join_reason() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@joinreason_{suffix}:localhost");
    let room_id = format!("!joinreason_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("joinreason_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    let member = storage
        .add_member(&room_id, &user_id, "join", None, Some("Invited by friend"), None)
        .await
        .unwrap();

    assert_eq!(member.join_reason.as_deref(), Some("Invited by friend"));
}

#[tokio::test]
async fn test_add_member_upsert() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@upsert_{suffix}:localhost");
    let room_id = format!("!upsert_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("upsert_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    let member1 = storage
        .add_member(&room_id, &user_id, "join", Some("OldName"), None, None)
        .await
        .unwrap();
    assert_eq!(member1.display_name.as_deref(), Some("OldName"));

    let member2 = storage
        .add_member(&room_id, &user_id, "join", Some("NewName"), Some("Updated"), None)
        .await
        .unwrap();
    assert_eq!(member2.display_name.as_deref(), Some("NewName"));
    assert_eq!(member2.join_reason.as_deref(), Some("Updated"));
}

#[tokio::test]
async fn test_get_member_found() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@getmember_{suffix}:localhost");
    let room_id = format!("!getmember_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("getmember_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage
        .add_member(&room_id, &user_id, "join", Some("TestUser"), None, None)
        .await
        .unwrap();

    let member = storage.get_member(&room_id, &user_id).await.unwrap();
    assert!(member.is_some());
    let m = member.unwrap();
    assert_eq!(m.user_id, user_id);
    assert_eq!(m.membership, "join");
    assert_eq!(m.display_name.as_deref(), Some("TestUser"));
}

#[tokio::test]
async fn test_get_member_not_found() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();

    let result = storage
        .get_member(&format!("!nonexistent_{suffix}:localhost"), &format!("@nobody_{suffix}:localhost"))
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_room_members() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!roommembers_room_{suffix}:localhost");
    let user1 = format!("@rmember1_{suffix}:localhost");
    let user2 = format!("@rmember2_{suffix}:localhost");
    let user3 = format!("@rmember3_{suffix}:localhost");

    insert_user(&pool, &user1, &format!("rmember1_{suffix}")).await;
    insert_user(&pool, &user2, &format!("rmember2_{suffix}")).await;
    insert_user(&pool, &user3, &format!("rmember3_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage
        .add_member(&room_id, &user1, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user2, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user3, "leave", None, None, None)
        .await
        .unwrap();

    let joined = storage.get_room_members(&room_id, "join").await.unwrap();
    assert_eq!(joined.len(), 2);

    let left = storage.get_room_members(&room_id, "leave").await.unwrap();
    assert_eq!(left.len(), 1);
}

#[tokio::test]
async fn test_get_room_members_empty() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!emptymembers_room_{suffix}:localhost");

    insert_room(&pool, &room_id).await;

    let members = storage.get_room_members(&room_id, "join").await.unwrap();
    assert!(members.is_empty());
}

#[tokio::test]
async fn test_get_room_member_count() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!membercount_room_{suffix}:localhost");
    let user1 = format!("@mcount1_{suffix}:localhost");
    let user2 = format!("@mcount2_{suffix}:localhost");
    let user3 = format!("@mcount3_{suffix}:localhost");

    insert_user(&pool, &user1, &format!("mcount1_{suffix}")).await;
    insert_user(&pool, &user2, &format!("mcount2_{suffix}")).await;
    insert_user(&pool, &user3, &format!("mcount3_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage
        .add_member(&room_id, &user1, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user2, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user3, "leave", None, None, None)
        .await
        .unwrap();

    let count = storage.get_room_member_count(&room_id).await.unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn test_get_room_member_count_zero() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!nomembers_room_{suffix}:localhost");

    insert_room(&pool, &room_id).await;

    let count = storage.get_room_member_count(&room_id).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_remove_member_join() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@removejoin_{suffix}:localhost");
    let room_id = format!("!removejoin_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("removejoin_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage
        .add_member(&room_id, &user_id, "join", None, None, None)
        .await
        .unwrap();

    storage.remove_member(&room_id, &user_id).await.unwrap();

    let member = storage.get_member(&room_id, &user_id).await.unwrap();
    assert!(member.is_some());
    assert_eq!(member.unwrap().membership, "leave");
}

#[tokio::test]
async fn test_remove_member_banned() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@removeban_{suffix}:localhost");
    let room_id = format!("!removeban_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("removeban_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage
        .add_member(&room_id, &user_id, "join", None, None, None)
        .await
        .unwrap();
    storage
        .ban_member(&room_id, &user_id, "@admin:localhost")
        .await
        .unwrap();

    storage.remove_member(&room_id, &user_id).await.unwrap();

    let member = storage.get_member(&room_id, &user_id).await.unwrap();
    assert!(member.is_some());
    assert_eq!(member.unwrap().membership, "leave");
}

#[tokio::test]
async fn test_remove_member_already_left() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@removeleft_{suffix}:localhost");
    let room_id = format!("!removeleft_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("removeleft_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    insert_membership(&pool, &room_id, &user_id, "leave").await;

    storage.remove_member(&room_id, &user_id).await.unwrap();

    let member = storage.get_member(&room_id, &user_id).await.unwrap();
    assert!(member.is_some());
    assert_eq!(member.unwrap().membership, "leave");
}

#[tokio::test]
async fn test_forget_member_leave() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@forgetleave_{suffix}:localhost");
    let room_id = format!("!forgetleave_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("forgetleave_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    insert_membership(&pool, &room_id, &user_id, "leave").await;

    storage.forget_member(&room_id, &user_id).await.unwrap();

    let member = storage.get_member(&room_id, &user_id).await.unwrap();
    assert!(member.is_some());
    assert_eq!(member.unwrap().membership, "forget");
}

#[tokio::test]
async fn test_forget_member_invite() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@forgetinvite_{suffix}:localhost");
    let room_id = format!("!forgetinvite_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("forgetinvite_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    insert_membership(&pool, &room_id, &user_id, "invite").await;

    storage.forget_member(&room_id, &user_id).await.unwrap();

    let member = storage.get_member(&room_id, &user_id).await.unwrap();
    assert!(member.is_some());
    assert_eq!(member.unwrap().membership, "forget");
}

#[tokio::test]
async fn test_forget_member_joined_no_effect() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@forgetjoin_{suffix}:localhost");
    let room_id = format!("!forgetjoin_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("forgetjoin_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage
        .add_member(&room_id, &user_id, "join", None, None, None)
        .await
        .unwrap();

    storage.forget_member(&room_id, &user_id).await.unwrap();

    let member = storage.get_member(&room_id, &user_id).await.unwrap();
    assert!(member.is_some());
    assert_eq!(member.unwrap().membership, "join");
}

#[tokio::test]
async fn test_is_forgotten_true() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@isforgotten_{suffix}:localhost");
    let room_id = format!("!isforgotten_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("isforgotten_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    insert_membership(&pool, &room_id, &user_id, "leave").await;
    storage.forget_member(&room_id, &user_id).await.unwrap();

    let forgotten = storage.is_forgotten(&room_id, &user_id).await.unwrap();
    assert!(forgotten);
}

#[tokio::test]
async fn test_is_forgotten_false() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@notforgotten_{suffix}:localhost");
    let room_id = format!("!notforgotten_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("notforgotten_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage
        .add_member(&room_id, &user_id, "join", None, None, None)
        .await
        .unwrap();

    let forgotten = storage.is_forgotten(&room_id, &user_id).await.unwrap();
    assert!(!forgotten);
}

#[tokio::test]
async fn test_is_forgotten_no_record() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();

    let forgotten = storage
        .is_forgotten(&format!("!noforgotten_room_{suffix}:localhost"), &format!("@noforgotten_{suffix}:localhost"))
        .await
        .unwrap();
    assert!(!forgotten);
}

#[tokio::test]
async fn test_get_shared_room_users() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_a = format!("@shared_a_{suffix}:localhost");
    let user_b = format!("@shared_b_{suffix}:localhost");
    let user_c = format!("@shared_c_{suffix}:localhost");
    let room_id = format!("!shared_room_{suffix}:localhost");

    insert_user(&pool, &user_a, &format!("shared_a_{suffix}")).await;
    insert_user(&pool, &user_b, &format!("shared_b_{suffix}")).await;
    insert_user(&pool, &user_c, &format!("shared_c_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage
        .add_member(&room_id, &user_a, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user_b, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user_c, "join", None, None, None)
        .await
        .unwrap();

    let shared = storage.get_shared_room_users(&user_a).await.unwrap();
    assert_eq!(shared.len(), 2);
    assert!(shared.contains(&user_b));
    assert!(shared.contains(&user_c));
}

#[tokio::test]
async fn test_get_shared_room_users_no_shared() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@noshared_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("noshared_{suffix}")).await;

    let shared = storage.get_shared_room_users(&user_id).await.unwrap();
    assert!(shared.is_empty());
}

#[tokio::test]
async fn test_remove_all_members() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!removeall_room_{suffix}:localhost");
    let user1 = format!("@removeall1_{suffix}:localhost");
    let user2 = format!("@removeall2_{suffix}:localhost");

    insert_user(&pool, &user1, &format!("removeall1_{suffix}")).await;
    insert_user(&pool, &user2, &format!("removeall2_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage
        .add_member(&room_id, &user1, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user2, "join", None, None, None)
        .await
        .unwrap();

    storage.remove_all_members(&room_id).await.unwrap();

    let members = storage.get_room_members(&room_id, "join").await.unwrap();
    assert!(members.is_empty());
}

#[tokio::test]
async fn test_ban_member() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@ban_{suffix}:localhost");
    let room_id = format!("!ban_room_{suffix}:localhost");
    let banner = format!("@banner_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("ban_{suffix}")).await;
    insert_user(&pool, &banner, &format!("banner_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage
        .ban_member(&room_id, &user_id, &banner)
        .await
        .unwrap();

    let state = storage.get_membership_state(&room_id, &user_id).await.unwrap();
    assert_eq!(state.as_deref(), Some("ban"));
}

#[tokio::test]
async fn test_ban_member_upsert() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@banupsert_{suffix}:localhost");
    let room_id = format!("!banupsert_room_{suffix}:localhost");
    let banner1 = format!("@banner1_{suffix}:localhost");
    let banner2 = format!("@banner2_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("banupsert_{suffix}")).await;
    insert_user(&pool, &banner1, &format!("banner1_{suffix}")).await;
    insert_user(&pool, &banner2, &format!("banner2_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage
        .add_member(&room_id, &user_id, "join", None, None, None)
        .await
        .unwrap();

    storage
        .ban_member(&room_id, &user_id, &banner1)
        .await
        .unwrap();
    storage
        .ban_member(&room_id, &user_id, &banner2)
        .await
        .unwrap();

    let state = storage.get_membership_state(&room_id, &user_id).await.unwrap();
    assert_eq!(state.as_deref(), Some("ban"));
}

#[tokio::test]
async fn test_unban_member() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@unban_{suffix}:localhost");
    let room_id = format!("!unban_room_{suffix}:localhost");
    let banner = format!("@unbanner_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("unban_{suffix}")).await;
    insert_user(&pool, &banner, &format!("unbanner_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage
        .add_member(&room_id, &user_id, "join", None, None, None)
        .await
        .unwrap();
    storage
        .ban_member(&room_id, &user_id, &banner)
        .await
        .unwrap();

    storage.unban_member(&room_id, &user_id).await.unwrap();

    let state = storage.get_membership_state(&room_id, &user_id).await.unwrap();
    assert_eq!(state.as_deref(), Some("leave"));
}

#[tokio::test]
async fn test_unban_member_not_banned() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@unbannot_{suffix}:localhost");
    let room_id = format!("!unbannot_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("unbannot_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage
        .add_member(&room_id, &user_id, "join", None, None, None)
        .await
        .unwrap();

    storage.unban_member(&room_id, &user_id).await.unwrap();

    let member = storage.get_member(&room_id, &user_id).await.unwrap();
    assert!(member.is_some());
    assert_eq!(member.unwrap().membership, "join");
}

#[tokio::test]
async fn test_get_joined_rooms() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@joinedrooms_{suffix}:localhost");
    let room1 = format!("!joinedroom1_{suffix}:localhost");
    let room2 = format!("!joinedroom2_{suffix}:localhost");
    let room3 = format!("!joinedroom3_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("joinedrooms_{suffix}")).await;
    insert_room(&pool, &room1).await;
    insert_room(&pool, &room2).await;
    insert_room(&pool, &room3).await;

    storage
        .add_member(&room1, &user_id, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room2, &user_id, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room3, &user_id, "leave", None, None, None)
        .await
        .unwrap();

    let rooms = storage.get_joined_rooms(&user_id).await.unwrap();
    assert_eq!(rooms.len(), 2);
    assert!(rooms.contains(&room1));
    assert!(rooms.contains(&room2));
    assert!(!rooms.contains(&room3));
}

#[tokio::test]
async fn test_get_joined_rooms_empty() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@nojoinedrooms_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("nojoinedrooms_{suffix}")).await;

    let rooms = storage.get_joined_rooms(&user_id).await.unwrap();
    assert!(rooms.is_empty());
}

#[tokio::test]
async fn test_get_sync_rooms_join_only() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@syncjoin_{suffix}:localhost");
    let room1 = format!("!syncjoin1_{suffix}:localhost");
    let room2 = format!("!syncjoin2_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("syncjoin_{suffix}")).await;
    insert_room(&pool, &room1).await;
    insert_room(&pool, &room2).await;

    storage
        .add_member(&room1, &user_id, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room2, &user_id, "leave", None, None, None)
        .await
        .unwrap();

    let memberships = storage.get_sync_rooms(&user_id, false).await.unwrap();
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0].membership, "join");
    assert_eq!(memberships[0].room_id, room1);
}

#[tokio::test]
async fn test_get_sync_rooms_include_leave() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@syncleave_{suffix}:localhost");
    let room1 = format!("!syncleave1_{suffix}:localhost");
    let room2 = format!("!syncleave2_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("syncleave_{suffix}")).await;
    insert_room(&pool, &room1).await;
    insert_room(&pool, &room2).await;

    storage
        .add_member(&room1, &user_id, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room2, &user_id, "leave", None, None, None)
        .await
        .unwrap();

    let memberships = storage.get_sync_rooms(&user_id, true).await.unwrap();
    assert_eq!(memberships.len(), 2);

    let membership_types: Vec<&str> = memberships.iter().map(|m| m.membership.as_str()).collect();
    assert!(membership_types.contains(&"join"));
    assert!(membership_types.contains(&"leave"));
}

#[tokio::test]
async fn test_get_membership_state_found() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@mstate_{suffix}:localhost");
    let room_id = format!("!mstate_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("mstate_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage
        .add_member(&room_id, &user_id, "join", None, None, None)
        .await
        .unwrap();

    let state = storage.get_membership_state(&room_id, &user_id).await.unwrap();
    assert_eq!(state.as_deref(), Some("join"));
}

#[tokio::test]
async fn test_get_membership_state_not_found() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();

    let state = storage
        .get_membership_state(&format!("!nostate_room_{suffix}:localhost"), &format!("@nostate_{suffix}:localhost"))
        .await
        .unwrap();
    assert!(state.is_none());
}

#[tokio::test]
async fn test_get_joined_room_count() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@jroomcount_{suffix}:localhost");
    let room1 = format!("!jroomcount1_{suffix}:localhost");
    let room2 = format!("!jroomcount2_{suffix}:localhost");
    let room3 = format!("!jroomcount3_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("jroomcount_{suffix}")).await;
    insert_room(&pool, &room1).await;
    insert_room(&pool, &room2).await;
    insert_room(&pool, &room3).await;

    storage
        .add_member(&room1, &user_id, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room2, &user_id, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room3, &user_id, "leave", None, None, None)
        .await
        .unwrap();

    let count = storage.get_joined_room_count(&user_id).await.unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn test_get_joined_room_count_zero() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@nojroomcount_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("nojroomcount_{suffix}")).await;

    let count = storage.get_joined_room_count(&user_id).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_is_member_true() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@ismember_{suffix}:localhost");
    let room_id = format!("!ismember_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("ismember_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage
        .add_member(&room_id, &user_id, "join", None, None, None)
        .await
        .unwrap();

    let result = storage.is_member(&room_id, &user_id).await.unwrap();
    assert!(result);
}

#[tokio::test]
async fn test_is_member_false_left() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@notmember_{suffix}:localhost");
    let room_id = format!("!notmember_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("notmember_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage
        .add_member(&room_id, &user_id, "leave", None, None, None)
        .await
        .unwrap();

    let result = storage.is_member(&room_id, &user_id).await.unwrap();
    assert!(!result);
}

#[tokio::test]
async fn test_is_member_false_no_record() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();

    let result = storage
        .is_member(&format!("!norecord_room_{suffix}:localhost"), &format!("@norecord_{suffix}:localhost"))
        .await
        .unwrap();
    assert!(!result);
}

#[tokio::test]
async fn test_get_room_member_found() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@grm_{suffix}:localhost");
    let room_id = format!("!grm_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("grm_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage
        .add_member(&room_id, &user_id, "join", Some("TestUser"), None, None)
        .await
        .unwrap();

    let member = storage.get_room_member(&room_id, &user_id).await.unwrap();
    assert!(member.is_some());
    let m = member.unwrap();
    assert_eq!(m.user_id, user_id);
    assert_eq!(m.membership, "join");
    assert_eq!(m.display_name.as_deref(), Some("TestUser"));
}

#[tokio::test]
async fn test_get_room_member_not_found() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();

    let result = storage
        .get_room_member(&format!("!grmnf_room_{suffix}:localhost"), &format!("@grmnf_{suffix}:localhost"))
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_joined_members() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!jmembers_room_{suffix}:localhost");
    let user1 = format!("@jmember1_{suffix}:localhost");
    let user2 = format!("@jmember2_{suffix}:localhost");
    let user3 = format!("@jmember3_{suffix}:localhost");

    insert_user(&pool, &user1, &format!("jmember1_{suffix}")).await;
    insert_user(&pool, &user2, &format!("jmember2_{suffix}")).await;
    insert_user(&pool, &user3, &format!("jmember3_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage
        .add_member(&room_id, &user1, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user2, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user3, "leave", None, None, None)
        .await
        .unwrap();

    let members = storage.get_joined_members(&room_id).await.unwrap();
    assert_eq!(members.len(), 2);

    let member_ids: Vec<&str> = members.iter().map(|m| m.user_id.as_str()).collect();
    assert!(member_ids.contains(&user1.as_str()));
    assert!(member_ids.contains(&user2.as_str()));
}

#[tokio::test]
async fn test_get_joined_members_empty() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!emptyjmembers_room_{suffix}:localhost");

    insert_room(&pool, &room_id).await;

    let members = storage.get_joined_members(&room_id).await.unwrap();
    assert!(members.is_empty());
}

#[tokio::test]
async fn test_get_joined_member_found() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@gjm_{suffix}:localhost");
    let room_id = format!("!gjm_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("gjm_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage
        .add_member(&room_id, &user_id, "join", Some("JoinedUser"), None, None)
        .await
        .unwrap();

    let member = storage.get_joined_member(&room_id, &user_id).await.unwrap();
    assert!(member.is_some());
    let m = member.unwrap();
    assert_eq!(m.membership, "join");
    assert_eq!(m.display_name.as_deref(), Some("JoinedUser"));
}

#[tokio::test]
async fn test_get_joined_member_not_joined() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@gjmnj_{suffix}:localhost");
    let room_id = format!("!gjmnj_room_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("gjmnj_{suffix}")).await;
    insert_room(&pool, &room_id).await;
    storage
        .add_member(&room_id, &user_id, "leave", None, None, None)
        .await
        .unwrap();

    let member = storage.get_joined_member(&room_id, &user_id).await.unwrap();
    assert!(member.is_none());
}

#[tokio::test]
async fn test_share_common_room_true() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_a = format!("@share_a_{suffix}:localhost");
    let user_b = format!("@share_b_{suffix}:localhost");
    let room_id = format!("!share_room_{suffix}:localhost");

    insert_user(&pool, &user_a, &format!("share_a_{suffix}")).await;
    insert_user(&pool, &user_b, &format!("share_b_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage
        .add_member(&room_id, &user_a, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user_b, "join", None, None, None)
        .await
        .unwrap();

    let result = storage.share_common_room(&user_a, &user_b).await.unwrap();
    assert!(result);
}

#[tokio::test]
async fn test_share_common_room_false() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_a = format!("@noshare_a_{suffix}:localhost");
    let user_b = format!("@noshare_b_{suffix}:localhost");

    insert_user(&pool, &user_a, &format!("noshare_a_{suffix}")).await;
    insert_user(&pool, &user_b, &format!("noshare_b_{suffix}")).await;

    let result = storage.share_common_room(&user_a, &user_b).await.unwrap();
    assert!(!result);
}

#[tokio::test]
async fn test_share_common_room_one_left() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_a = format!("@shareleft_a_{suffix}:localhost");
    let user_b = format!("@shareleft_b_{suffix}:localhost");
    let room_id = format!("!shareleft_room_{suffix}:localhost");

    insert_user(&pool, &user_a, &format!("shareleft_a_{suffix}")).await;
    insert_user(&pool, &user_b, &format!("shareleft_b_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage
        .add_member(&room_id, &user_a, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user_b, "leave", None, None, None)
        .await
        .unwrap();

    let result = storage.share_common_room(&user_a, &user_b).await.unwrap();
    assert!(!result);
}

#[tokio::test]
async fn test_get_membership_history() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!mhistory_room_{suffix}:localhost");
    let user1 = format!("@mhist1_{suffix}:localhost");
    let user2 = format!("@mhist2_{suffix}:localhost");
    let user3 = format!("@mhist3_{suffix}:localhost");

    insert_user(&pool, &user1, &format!("mhist1_{suffix}")).await;
    insert_user(&pool, &user2, &format!("mhist2_{suffix}")).await;
    insert_user(&pool, &user3, &format!("mhist3_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage
        .add_member(&room_id, &user1, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user2, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user3, "leave", None, None, None)
        .await
        .unwrap();

    let history = storage.get_membership_history(&room_id, 10).await.unwrap();
    assert_eq!(history.len(), 3);
}

#[tokio::test]
async fn test_get_membership_history_with_limit() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!mhistlimit_room_{suffix}:localhost");
    let user1 = format!("@mhistl1_{suffix}:localhost");
    let user2 = format!("@mhistl2_{suffix}:localhost");
    let user3 = format!("@mhistl3_{suffix}:localhost");

    insert_user(&pool, &user1, &format!("mhistl1_{suffix}")).await;
    insert_user(&pool, &user2, &format!("mhistl2_{suffix}")).await;
    insert_user(&pool, &user3, &format!("mhistl3_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage
        .add_member(&room_id, &user1, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user2, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user3, "join", None, None, None)
        .await
        .unwrap();

    let history = storage.get_membership_history(&room_id, 2).await.unwrap();
    assert_eq!(history.len(), 2);
}

#[tokio::test]
async fn test_get_joined_rooms_with_details() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@jdetails_{suffix}:localhost");
    let room1 = format!("!jdetails1_{suffix}:localhost");
    let room2 = format!("!jdetails2_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("jdetails_{suffix}")).await;
    insert_room(&pool, &room1).await;
    insert_room(&pool, &room2).await;

    sqlx::query(
        "UPDATE rooms SET name = $1, topic = $2, avatar_url = $3 WHERE room_id = $4",
    )
    .bind("Room One")
    .bind("Topic One")
    .bind("mxc://localhost/avatar1")
    .bind(&room1)
    .execute(&*pool)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE rooms SET name = $1 WHERE room_id = $2",
    )
    .bind("Room Two")
    .bind(&room2)
    .execute(&*pool)
    .await
    .unwrap();

    storage
        .add_member(&room1, &user_id, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room2, &user_id, "join", None, None, None)
        .await
        .unwrap();

    let details = storage.get_joined_rooms_with_details(&user_id).await.unwrap();
    assert_eq!(details.len(), 2);

    let room1_detail = details.iter().find(|d| d.0 == room1).unwrap();
    assert_eq!(room1_detail.1, "Room One");
    assert_eq!(room1_detail.2.as_deref(), Some("Topic One"));
    assert_eq!(room1_detail.3.as_deref(), Some("mxc://localhost/avatar1"));
}

#[tokio::test]
async fn test_get_joined_rooms_with_details_empty() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@nojdetails_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("nojdetails_{suffix}")).await;

    let details = storage.get_joined_rooms_with_details(&user_id).await.unwrap();
    assert!(details.is_empty());
}

#[tokio::test]
async fn test_get_room_members_with_profiles() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!profiles_room_{suffix}:localhost");
    let user1 = format!("@profile1_{suffix}:localhost");
    let user2 = format!("@profile2_{suffix}:localhost");

    insert_user(&pool, &user1, &format!("profile1_{suffix}")).await;
    insert_user(&pool, &user2, &format!("profile2_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    sqlx::query(
        "UPDATE users SET displayname = $1, avatar_url = $2 WHERE user_id = $3",
    )
    .bind("Display One")
    .bind("mxc://localhost/avatar1")
    .bind(&user1)
    .execute(&*pool)
    .await
    .unwrap();

    storage
        .add_member(&room_id, &user1, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user2, "join", None, None, None)
        .await
        .unwrap();

    let profiles = storage
        .get_room_members_with_profiles(&room_id, "join")
        .await
        .unwrap();
    assert_eq!(profiles.len(), 2);

    let user1_profile = profiles.iter().find(|p| p.0.user_id == user1).unwrap();
    assert_eq!(user1_profile.1.as_deref(), Some("Display One"));
    assert_eq!(user1_profile.2.as_deref(), Some("mxc://localhost/avatar1"));

    let user2_profile = profiles.iter().find(|p| p.0.user_id == user2).unwrap();
    assert!(user2_profile.1.is_none());
    assert!(user2_profile.2.is_none());
}

#[tokio::test]
async fn test_get_room_members_with_profiles_empty() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!emptyprofiles_room_{suffix}:localhost");

    insert_room(&pool, &room_id).await;

    let profiles = storage
        .get_room_members_with_profiles(&room_id, "join")
        .await
        .unwrap();
    assert!(profiles.is_empty());
}

#[tokio::test]
async fn test_get_members_batch() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room1 = format!("!batch1_{suffix}:localhost");
    let room2 = format!("!batch2_{suffix}:localhost");
    let user1 = format!("@batch1_{suffix}:localhost");
    let user2 = format!("@batch2_{suffix}:localhost");
    let user3 = format!("@batch3_{suffix}:localhost");

    insert_user(&pool, &user1, &format!("batch1_{suffix}")).await;
    insert_user(&pool, &user2, &format!("batch2_{suffix}")).await;
    insert_user(&pool, &user3, &format!("batch3_{suffix}")).await;
    insert_room(&pool, &room1).await;
    insert_room(&pool, &room2).await;

    storage
        .add_member(&room1, &user1, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room1, &user2, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room2, &user3, "join", None, None, None)
        .await
        .unwrap();

    let result = storage
        .get_members_batch(&[room1.clone(), room2.clone()], "join")
        .await
        .unwrap();

    assert_eq!(result[&room1].len(), 2);
    assert_eq!(result[&room2].len(), 1);
}

#[tokio::test]
async fn test_get_members_batch_empty() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);

    let result = storage
        .get_members_batch(&[], "join")
        .await
        .unwrap();

    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_joined_members_batch() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room1 = format!("!jbatch1_{suffix}:localhost");
    let room2 = format!("!jbatch2_{suffix}:localhost");
    let user1 = format!("@jbatch1_{suffix}:localhost");
    let user2 = format!("@jbatch2_{suffix}:localhost");

    insert_user(&pool, &user1, &format!("jbatch1_{suffix}")).await;
    insert_user(&pool, &user2, &format!("jbatch2_{suffix}")).await;
    insert_room(&pool, &room1).await;
    insert_room(&pool, &room2).await;

    storage
        .add_member(&room1, &user1, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room2, &user2, "join", None, None, None)
        .await
        .unwrap();

    let result = storage
        .get_joined_members_batch(&[room1.clone(), room2.clone()])
        .await
        .unwrap();

    assert_eq!(result[&room1].len(), 1);
    assert_eq!(result[&room2].len(), 1);
    assert_eq!(result[&room1][0].user_id, user1);
    assert_eq!(result[&room2][0].user_id, user2);
}

#[tokio::test]
async fn test_check_membership_batch() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!checkbatch_room_{suffix}:localhost");
    let user1 = format!("@check1_{suffix}:localhost");
    let user2 = format!("@check2_{suffix}:localhost");
    let user3 = format!("@check3_{suffix}:localhost");

    insert_user(&pool, &user1, &format!("check1_{suffix}")).await;
    insert_user(&pool, &user2, &format!("check2_{suffix}")).await;
    insert_user(&pool, &user3, &format!("check3_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage
        .add_member(&room_id, &user1, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user2, "join", None, None, None)
        .await
        .unwrap();
    storage
        .add_member(&room_id, &user3, "leave", None, None, None)
        .await
        .unwrap();

    let result = storage
        .check_membership_batch(&room_id, &[user1.clone(), user2.clone(), user3.clone()], "join")
        .await
        .unwrap();

    assert_eq!(result.len(), 2);
    assert!(result.contains(&user1));
    assert!(result.contains(&user2));
    assert!(!result.contains(&user3));
}

#[tokio::test]
async fn test_check_membership_batch_empty() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!checkempty_room_{suffix}:localhost");

    insert_room(&pool, &room_id).await;

    let result = storage
        .check_membership_batch(&room_id, &[], "join")
        .await
        .unwrap();

    assert!(result.is_empty());
}

#[tokio::test]
async fn test_check_membership_batch_no_matches() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let room_id = format!("!checknomatch_room_{suffix}:localhost");
    let user1 = format!("@checknomatch_{suffix}:localhost");

    insert_user(&pool, &user1, &format!("checknomatch_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage
        .add_member(&room_id, &user1, "leave", None, None, None)
        .await
        .unwrap();

    let result = storage
        .check_membership_batch(&room_id, std::slice::from_ref(&user1), "join")
        .await
        .unwrap();

    assert!(result.is_empty());
}

#[tokio::test]
async fn test_full_membership_lifecycle() {
    let pool = match setup_test_database().await {
        Some(pool) => pool,
        None => return,
    };
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@lifecycle_{suffix}:localhost");
    let room_id = format!("!lifecycle_room_{suffix}:localhost");
    let banner = format!("@lifecycle_admin_{suffix}:localhost");

    insert_user(&pool, &user_id, &format!("lifecycle_{suffix}")).await;
    insert_user(&pool, &banner, &format!("lifecycle_admin_{suffix}")).await;
    insert_room(&pool, &room_id).await;

    storage
        .add_member(&room_id, &user_id, "join", Some("LifecycleUser"), Some("Wanted to join"), None)
        .await
        .unwrap();
    assert!(storage.is_member(&room_id, &user_id).await.unwrap());
    assert_eq!(storage.get_room_member_count(&room_id).await.unwrap(), 1);

    storage
        .ban_member(&room_id, &user_id, &banner)
        .await
        .unwrap();
    assert!(!storage.is_member(&room_id, &user_id).await.unwrap());
    assert_eq!(
        storage.get_membership_state(&room_id, &user_id).await.unwrap().as_deref(),
        Some("ban")
    );

    storage.unban_member(&room_id, &user_id).await.unwrap();
    assert_eq!(
        storage.get_membership_state(&room_id, &user_id).await.unwrap().as_deref(),
        Some("leave")
    );

    storage
        .add_member(&room_id, &user_id, "join", Some("LifecycleUser"), None, None)
        .await
        .unwrap();
    assert!(storage.is_member(&room_id, &user_id).await.unwrap());

    storage.remove_member(&room_id, &user_id).await.unwrap();
    assert!(!storage.is_member(&room_id, &user_id).await.unwrap());
    assert_eq!(
        storage.get_membership_state(&room_id, &user_id).await.unwrap().as_deref(),
        Some("leave")
    );

    storage.forget_member(&room_id, &user_id).await.unwrap();
    assert!(storage.is_forgotten(&room_id, &user_id).await.unwrap());
    assert_eq!(
        storage.get_membership_state(&room_id, &user_id).await.unwrap().as_deref(),
        Some("forget")
    );
}
