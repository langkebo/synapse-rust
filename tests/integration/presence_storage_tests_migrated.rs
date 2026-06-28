#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::storage::presence::PresenceStorage;
use synapse_rust::PresenceState;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database() -> Option<(Arc<sqlx::PgPool>, PresenceStorage)> {
    let pool = match synapse_rust::test_utils::prepare_empty_isolated_test_pool().await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("Skipping presence storage tests because test database is unavailable: {error}");
            return None;
        }
    };

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            user_id TEXT NOT NULL PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT,
            is_admin BOOLEAN DEFAULT FALSE,
            is_guest BOOLEAN DEFAULT FALSE,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            displayname TEXT,
            avatar_url TEXT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create users table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS presence (
            user_id TEXT NOT NULL,
            status_msg TEXT,
            presence TEXT NOT NULL DEFAULT 'offline',
            last_active_ts BIGINT NOT NULL DEFAULT 0,
            status_from TEXT,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL,
            CONSTRAINT pk_presence PRIMARY KEY (user_id),
            CONSTRAINT fk_presence_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create presence table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS presence_subscriptions (
            subscriber_id TEXT NOT NULL,
            target_id TEXT NOT NULL,
            created_ts BIGINT NOT NULL,
            CONSTRAINT pk_presence_subscriptions PRIMARY KEY (subscriber_id, target_id),
            CONSTRAINT fk_presence_subscriptions_subscriber FOREIGN KEY (subscriber_id) REFERENCES users(user_id) ON DELETE CASCADE,
            CONSTRAINT fk_presence_subscriptions_target FOREIGN KEY (target_id) REFERENCES users(user_id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create presence_subscriptions table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS typing (
            user_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            is_typing BOOLEAN DEFAULT FALSE,
            last_active_ts BIGINT NOT NULL,
            CONSTRAINT pk_typing PRIMARY KEY (user_id, room_id)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create typing table");

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let storage = PresenceStorage::new(pool.clone(), cache);

    Some((pool, storage))
}

async fn insert_test_user(pool: &sqlx::PgPool, user_id: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"
        INSERT INTO users (user_id, username, created_ts)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(user_id.trim_start_matches('@').split(':').next().unwrap_or(user_id))
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to insert test user");
}

#[tokio::test]
async fn test_set_and_get_presence() {
    let (pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let suffix = unique_id();
    let user_id = format!("@presence_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;

    storage.set_presence(&user_id, PresenceState::Online.as_str(), Some("working")).await.unwrap();

    let result = storage.get_presence(&user_id).await.unwrap();
    assert!(result.is_some());
    let (presence, status_msg) = result.unwrap();
    assert_eq!(presence, "online");
    assert_eq!(status_msg, Some("working".to_string()));
}

#[tokio::test]
async fn test_get_presence_nonexistent() {
    let (_pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let result = storage.get_presence("@nonexistent:localhost").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_set_presence_without_status_msg() {
    let (pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let suffix = unique_id();
    let user_id = format!("@presence_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;

    storage.set_presence(&user_id, PresenceState::Unavailable.as_str(), None).await.unwrap();

    let result = storage.get_presence(&user_id).await.unwrap();
    assert!(result.is_some());
    let (presence, status_msg) = result.unwrap();
    assert_eq!(presence, "unavailable");
    assert_eq!(status_msg, None);
}

#[tokio::test]
async fn test_presence_upsert_updates_existing() {
    let (pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let suffix = unique_id();
    let user_id = format!("@presence_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;

    storage.set_presence(&user_id, PresenceState::Online.as_str(), Some("working")).await.unwrap();

    storage.set_presence(&user_id, PresenceState::Offline.as_str(), Some("gone home")).await.unwrap();

    let result = storage.get_presence(&user_id).await.unwrap();
    assert!(result.is_some());
    let (presence, status_msg) = result.unwrap();
    assert_eq!(presence, "offline");
    assert_eq!(status_msg, Some("gone home".to_string()));
}

#[tokio::test]
async fn test_get_presence_with_meta() {
    let (pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let suffix = unique_id();
    let user_id = format!("@presence_user_{suffix}:localhost");
    insert_test_user(&pool, &user_id).await;

    storage.set_presence(&user_id, PresenceState::Online.as_str(), Some("active")).await.unwrap();

    let result = storage.get_presence_with_meta(&user_id).await.unwrap();
    assert!(result.is_some());
    let (presence, status_msg, last_active_ts) = result.unwrap();
    assert_eq!(presence, "online");
    assert_eq!(status_msg, Some("active".to_string()));
    assert!(last_active_ts.is_some());
    assert!(last_active_ts.unwrap() > 0);
}

#[tokio::test]
async fn test_get_presence_with_meta_nonexistent() {
    let (_pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let result = storage.get_presence_with_meta("@nonexistent:localhost").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_presences_batch() {
    let (pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let suffix = unique_id();
    let user1 = format!("@presence_user_{suffix}_1:localhost");
    let user2 = format!("@presence_user_{suffix}_2:localhost");
    let user3 = format!("@presence_user_{suffix}_3:localhost");

    insert_test_user(&pool, &user1).await;
    insert_test_user(&pool, &user2).await;
    insert_test_user(&pool, &user3).await;

    storage.set_presence(&user1, PresenceState::Online.as_str(), Some("working")).await.unwrap();
    storage.set_presence(&user2, PresenceState::Unavailable.as_str(), None).await.unwrap();

    let user_ids = vec![user1.clone(), user2.clone(), user3.clone()];
    let result = storage.get_presences(&user_ids).await.unwrap();

    assert_eq!(result.len(), 2);
    let (p1, s1) = result.get(&user1).unwrap();
    assert_eq!(p1, "online");
    assert_eq!(*s1, Some("working".to_string()));
    let (p2, s2) = result.get(&user2).unwrap();
    assert_eq!(p2, "unavailable");
    assert_eq!(*s2, None);
    assert!(!result.contains_key(&user3));
}

#[tokio::test]
async fn test_get_presences_empty_input() {
    let (_pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let result = storage.get_presences(&[]).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_add_and_get_subscriptions() {
    let (pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let suffix = unique_id();
    let subscriber = format!("@sub_{suffix}:localhost");
    let target1 = format!("@target_{suffix}_1:localhost");
    let target2 = format!("@target_{suffix}_2:localhost");

    insert_test_user(&pool, &subscriber).await;
    insert_test_user(&pool, &target1).await;
    insert_test_user(&pool, &target2).await;

    storage.add_subscription(&subscriber, &target1).await.unwrap();
    storage.add_subscription(&subscriber, &target2).await.unwrap();

    let subs = storage.get_subscriptions(&subscriber).await.unwrap();
    assert_eq!(subs.len(), 2);
    assert!(subs.contains(&target1));
    assert!(subs.contains(&target2));
}

#[tokio::test]
async fn test_add_subscription_idempotent() {
    let (pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let suffix = unique_id();
    let subscriber = format!("@sub_{suffix}:localhost");
    let target = format!("@target_{suffix}:localhost");

    insert_test_user(&pool, &subscriber).await;
    insert_test_user(&pool, &target).await;

    storage.add_subscription(&subscriber, &target).await.unwrap();
    storage.add_subscription(&subscriber, &target).await.unwrap();

    let subs = storage.get_subscriptions(&subscriber).await.unwrap();
    assert_eq!(subs.len(), 1);
}

#[tokio::test]
async fn test_remove_subscription() {
    let (pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let suffix = unique_id();
    let subscriber = format!("@sub_{suffix}:localhost");
    let target1 = format!("@target_{suffix}_1:localhost");
    let target2 = format!("@target_{suffix}_2:localhost");

    insert_test_user(&pool, &subscriber).await;
    insert_test_user(&pool, &target1).await;
    insert_test_user(&pool, &target2).await;

    storage.add_subscription(&subscriber, &target1).await.unwrap();
    storage.add_subscription(&subscriber, &target2).await.unwrap();

    storage.remove_subscription(&subscriber, &target1).await.unwrap();

    let subs = storage.get_subscriptions(&subscriber).await.unwrap();
    assert_eq!(subs.len(), 1);
    assert!(subs.contains(&target2));
    assert!(!subs.contains(&target1));
}

#[tokio::test]
async fn test_get_subscribers() {
    let (pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let suffix = unique_id();
    let sub1 = format!("@sub_{suffix}_1:localhost");
    let sub2 = format!("@sub_{suffix}_2:localhost");
    let target = format!("@target_{suffix}:localhost");

    insert_test_user(&pool, &sub1).await;
    insert_test_user(&pool, &sub2).await;
    insert_test_user(&pool, &target).await;

    storage.add_subscription(&sub1, &target).await.unwrap();
    storage.add_subscription(&sub2, &target).await.unwrap();

    let subscribers = storage.get_subscribers(&target).await.unwrap();
    assert_eq!(subscribers.len(), 2);
    assert!(subscribers.contains(&sub1));
    assert!(subscribers.contains(&sub2));
}

#[tokio::test]
async fn test_get_subscriptions_empty() {
    let (_pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let subs = storage.get_subscriptions("@nobody:localhost").await.unwrap();
    assert!(subs.is_empty());
}

#[tokio::test]
async fn test_get_subscribers_empty() {
    let (_pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let subscribers = storage.get_subscribers("@nobody:localhost").await.unwrap();
    assert!(subscribers.is_empty());
}

#[tokio::test]
async fn test_set_typing_start_and_stop() {
    let (pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let suffix = unique_id();
    let user_id = format!("@typing_user_{suffix}:localhost");
    let room_id = format!("!typing_room_{suffix}:localhost");

    insert_test_user(&pool, &user_id).await;

    storage.set_typing(&room_id, &user_id, true).await.unwrap();

    let row: Option<(bool,)> = sqlx::query_as("SELECT is_typing FROM typing WHERE user_id = $1 AND room_id = $2")
        .bind(&user_id)
        .bind(&room_id)
        .fetch_optional(pool.as_ref())
        .await
        .unwrap();
    assert!(row.is_some());
    assert!(row.unwrap().0);

    storage.set_typing(&room_id, &user_id, false).await.unwrap();

    let row: Option<(bool,)> = sqlx::query_as("SELECT is_typing FROM typing WHERE user_id = $1 AND room_id = $2")
        .bind(&user_id)
        .bind(&room_id)
        .fetch_optional(pool.as_ref())
        .await
        .unwrap();
    assert!(row.is_none());
}

#[tokio::test]
async fn test_set_typing_upsert() {
    let (pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let suffix = unique_id();
    let user_id = format!("@typing_user_{suffix}:localhost");
    let room_id = format!("!typing_room_{suffix}:localhost");

    insert_test_user(&pool, &user_id).await;

    storage.set_typing(&room_id, &user_id, true).await.unwrap();

    storage.set_typing(&room_id, &user_id, true).await.unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM typing WHERE user_id = $1 AND room_id = $2")
        .bind(&user_id)
        .bind(&room_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(count.0, 1);
}

#[tokio::test]
async fn test_get_presence_batch() {
    let (pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let suffix = unique_id();
    let user1 = format!("@batch_user_{suffix}_1:localhost");
    let user2 = format!("@batch_user_{suffix}_2:localhost");

    insert_test_user(&pool, &user1).await;
    insert_test_user(&pool, &user2).await;

    storage.set_presence(&user1, PresenceState::Online.as_str(), Some("active")).await.unwrap();
    storage.set_presence(&user2, PresenceState::Offline.as_str(), None).await.unwrap();

    let user_ids = vec![user1.clone(), user2.clone()];
    let result = storage.get_presence_batch(&user_ids).await.unwrap();
    assert_eq!(result.len(), 2);

    let u1 = result.iter().find(|(uid, _, _)| uid == &user1).unwrap();
    assert_eq!(u1.1, "online");
    assert_eq!(u1.2, Some("active".to_string()));

    let u2 = result.iter().find(|(uid, _, _)| uid == &user2).unwrap();
    assert_eq!(u2.1, "offline");
    assert_eq!(u2.2, None);
}

#[tokio::test]
async fn test_get_presence_batch_empty_input() {
    let (_pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let result = storage.get_presence_batch(&[]).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_presence_snapshots() {
    let (pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let suffix = unique_id();
    let user1 = format!("@snap_user_{suffix}_1:localhost");
    let user2 = format!("@snap_user_{suffix}_2:localhost");

    insert_test_user(&pool, &user1).await;
    insert_test_user(&pool, &user2).await;

    storage.set_presence(&user1, PresenceState::Online.as_str(), Some("active")).await.unwrap();
    storage.set_presence(&user2, PresenceState::Unavailable.as_str(), None).await.unwrap();

    let user_ids = vec![user1.clone(), user2.clone()];
    let result = storage.get_presence_snapshots(&user_ids).await.unwrap();
    assert_eq!(result.len(), 2);

    let snap1 = result.get(&user1).unwrap();
    assert_eq!(snap1.user_id, user1);
    assert_eq!(snap1.presence, "online");
    assert_eq!(snap1.status_msg, Some("active".to_string()));
    assert!(snap1.last_active_ts.is_some());

    let snap2 = result.get(&user2).unwrap();
    assert_eq!(snap2.user_id, user2);
    assert_eq!(snap2.presence, "unavailable");
    assert_eq!(snap2.status_msg, None);
}

#[tokio::test]
async fn test_get_presence_snapshots_empty_input() {
    let (_pool, storage) = match setup_test_database().await {
        Some(tuple) => tuple,
        None => return,
    };

    let result = storage.get_presence_snapshots(&[]).await.unwrap();
    assert!(result.is_empty());
}
