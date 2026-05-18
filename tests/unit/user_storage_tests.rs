#![cfg(test)]

use sqlx::{Pool, Postgres};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::runtime::Runtime;

use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::storage::user::UserStorage;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database() -> Option<Arc<Pool<Postgres>>> {
    let pool = match synapse_rust::test_utils::prepare_empty_isolated_test_pool().await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!(
                "Skipping user storage tests because test database is unavailable: {error}"
            );
            return None;
        }
    };

    sqlx::query(
        r#"
        CREATE TABLE users (
            user_id VARCHAR(255) PRIMARY KEY,
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
            password_expires_at BIGINT,
            failed_login_attempts INT DEFAULT 0,
            locked_until BIGINT,
            must_change_password BOOLEAN DEFAULT FALSE
        )
    "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create users table");

    sqlx::query(
        r#"
        CREATE TABLE user_directory (
            user_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            visibility TEXT NOT NULL DEFAULT 'private',
            added_by TEXT,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            CONSTRAINT pk_user_directory PRIMARY KEY (user_id, room_id)
        )
    "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create user_directory table");

    Some(pool)
}

fn create_user_storage(pool: &Arc<Pool<Postgres>>) -> UserStorage {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    UserStorage::new(pool, cache)
}

#[test]
fn test_create_user_and_get_by_id() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@testuser_{id}:localhost");
        let username = format!("testuser_{id}");

        let user = storage
            .create_user(&user_id, &username, Some("hash123"), false)
            .await
            .unwrap();

        assert_eq!(user.user_id, user_id);
        assert_eq!(user.username, username);
        assert_eq!(user.password_hash, Some("hash123".to_string()));
        assert!(!user.is_admin);
        assert!(!user.is_guest);
        assert!(!user.is_deactivated);
        assert!(user.created_ts > 0);

        let fetched = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert_eq!(fetched.user_id, user_id);
        assert_eq!(fetched.username, username);
    });
}

#[test]
fn test_get_user_by_id_not_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);

        let result = storage
            .get_user_by_id("@nonexistent:localhost")
            .await
            .unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_get_user_by_username() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@username_user_{id}:localhost");
        let username = format!("username_user_{id}");

        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();

        let fetched = storage.get_user_by_username(&username).await.unwrap().unwrap();
        assert_eq!(fetched.user_id, user_id);
        assert_eq!(fetched.username, username);
    });
}

#[test]
fn test_get_user_by_username_not_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);

        let result = storage
            .get_user_by_username("nonexistent_user")
            .await
            .unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_get_user_by_email() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let id = unique_id();
        let user_id = format!("@email_user_{id}:localhost");
        let username = format!("email_user_{id}");
        let email = format!("email_{id}@example.com");

        sqlx::query(
            r#"
            INSERT INTO users (user_id, username, email, created_ts, generation)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(&user_id)
        .bind(&username)
        .bind(&email)
        .bind(chrono::Utc::now().timestamp_millis())
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*pool)
        .await
        .unwrap();

        let storage = create_user_storage(&pool);
        let fetched = storage.get_user_by_email(&email).await.unwrap().unwrap();
        assert_eq!(fetched.user_id, user_id);
        assert_eq!(fetched.email, Some(email));
    });
}

#[test]
fn test_get_user_by_email_deactivated_excluded() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let id = unique_id();
        let user_id = format!("@deactivated_email_{id}:localhost");
        let username = format!("deactivated_email_{id}");
        let email = format!("deactivated_{id}@example.com");

        sqlx::query(
            r#"
            INSERT INTO users (user_id, username, email, is_deactivated, created_ts, generation)
            VALUES ($1, $2, $3, TRUE, $4, $5)
            "#,
        )
        .bind(&user_id)
        .bind(&username)
        .bind(&email)
        .bind(chrono::Utc::now().timestamp_millis())
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*pool)
        .await
        .unwrap();

        let storage = create_user_storage(&pool);
        let result = storage.get_user_by_email(&email).await.unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_get_user_by_identifier_user_id() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@ident_user_{id}:localhost");
        let username = format!("ident_user_{id}");

        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();

        let fetched = storage.get_user_by_identifier(&user_id).await.unwrap().unwrap();
        assert_eq!(fetched.user_id, user_id);
    });
}

#[test]
fn test_get_user_by_identifier_username() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@ident_name_{id}:localhost");
        let username = format!("ident_name_{id}");

        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();

        let fetched = storage.get_user_by_identifier(&username).await.unwrap().unwrap();
        assert_eq!(fetched.username, username);
    });
}

#[test]
fn test_get_all_users() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);

        for i in 0..3 {
            let id = unique_id();
            let user_id = format!("@all_user_{id}:localhost");
            let username = format!("all_user_{id}");
            storage
                .create_user(&user_id, &username, None, false)
                .await
                .unwrap();
            let _ = i;
        }

        let users = storage.get_all_users(10).await.unwrap();
        assert!(users.len() >= 3);
    });
}

#[test]
fn test_get_all_users_limit() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);

        for _ in 0..5 {
            let id = unique_id();
            let user_id = format!("@limit_user_{id}:localhost");
            let username = format!("limit_user_{id}");
            storage
                .create_user(&user_id, &username, None, false)
                .await
                .unwrap();
        }

        let users = storage.get_all_users(2).await.unwrap();
        assert_eq!(users.len(), 2);
    });
}

#[test]
fn test_get_user_count() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);

        let count_before = storage.get_user_count().await.unwrap();

        let id = unique_id();
        let user_id = format!("@count_user_{id}:localhost");
        let username = format!("count_user_{id}");
        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();

        let count_after = storage.get_user_count().await.unwrap();
        assert_eq!(count_after, count_before + 1);
    });
}

#[test]
fn test_user_exists() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@exists_user_{id}:localhost");
        let username = format!("exists_user_{id}");

        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();

        assert!(storage.user_exists(&user_id).await.unwrap());
        assert!(!storage.user_exists("@no_such_user:localhost").await.unwrap());
    });
}

#[test]
fn test_user_exists_deactivated_returns_false() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@deactivated_exists_{id}:localhost");
        let username = format!("deactivated_exists_{id}");

        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();

        storage.deactivate_user(&user_id).await.unwrap();
        assert!(!storage.user_exists(&user_id).await.unwrap());
    });
}

#[test]
fn test_update_password() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@pwd_user_{id}:localhost");
        let username = format!("pwd_user_{id}");

        storage
            .create_user(&user_id, &username, Some("old_hash"), false)
            .await
            .unwrap();

        storage.update_password(&user_id, "new_hash").await.unwrap();

        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert_eq!(user.password_hash, Some("new_hash".to_string()));
        assert!(user.password_changed_ts.is_some());
        assert!(!user.is_password_change_required);
        assert!(!user.must_change_password);
    });
}

#[test]
fn test_update_displayname() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@display_user_{id}:localhost");
        let username = format!("display_user_{id}");

        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();

        storage
            .update_displayname(&user_id, Some("Display Name"))
            .await
            .unwrap();

        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert_eq!(user.displayname, Some("Display Name".to_string()));

        storage
            .update_displayname(&user_id, None)
            .await
            .unwrap();

        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert_eq!(user.displayname, None);
    });
}

#[test]
fn test_update_avatar_url() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@avatar_user_{id}:localhost");
        let username = format!("avatar_user_{id}");

        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();

        storage
            .update_avatar_url(&user_id, Some("mxc://localhost/avatar123"))
            .await
            .unwrap();

        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert_eq!(user.avatar_url, Some("mxc://localhost/avatar123".to_string()));

        storage
            .update_avatar_url(&user_id, None)
            .await
            .unwrap();

        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert_eq!(user.avatar_url, None);
    });
}

#[test]
fn test_deactivate_user() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@deact_user_{id}:localhost");
        let username = format!("deact_user_{id}");

        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();

        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert!(!user.is_deactivated);

        storage.deactivate_user(&user_id).await.unwrap();

        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert!(user.is_deactivated);
    });
}

#[test]
fn test_set_admin_status() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@admin_user_{id}:localhost");
        let username = format!("admin_user_{id}");

        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();

        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert!(!user.is_admin);

        storage.set_admin_status(&user_id, true).await.unwrap();

        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert!(user.is_admin);

        storage.set_admin_status(&user_id, false).await.unwrap();

        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert!(!user.is_admin);
    });
}

#[test]
fn test_delete_user() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@delete_user_{id}:localhost");
        let username = format!("delete_user_{id}");

        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();

        assert!(storage.get_user_by_id(&user_id).await.unwrap().is_some());

        storage.delete_user(&user_id).await.unwrap();

        assert!(storage.get_user_by_id(&user_id).await.unwrap().is_none());
    });
}

#[test]
fn test_search_users_by_username() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        if sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_trgm")
            .execute(&*pool)
            .await
            .is_err()
        {
            eprintln!("Skipping test_search_users_by_username: pg_trgm not available");
            return;
        }
        let has_trgm = sqlx::query("SELECT similarity('test', 'test')")
            .execute(&*pool)
            .await
            .is_ok();
        if !has_trgm {
            eprintln!("Skipping test_search_users_by_username: similarity() not available in search_path");
            return;
        }
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@search_user_{id}:localhost");
        let username = format!("search_user_{id}");

        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();

        let results = storage.search_users(&username, 10).await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].user_id, user_id);
    });
}

#[test]
fn test_search_users_empty_query() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);

        let results = storage.search_users("", 10).await.unwrap();
        assert!(results.is_empty());

        let results = storage.search_users("   ", 10).await.unwrap();
        assert!(results.is_empty());
    });
}

#[test]
fn test_search_users_excludes_deactivated() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        if sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_trgm")
            .execute(&*pool)
            .await
            .is_err()
        {
            eprintln!("Skipping test_search_users_excludes_deactivated: pg_trgm not available");
            return;
        }
        let has_trgm = sqlx::query("SELECT similarity('test', 'test')")
            .execute(&*pool)
            .await
            .is_ok();
        if !has_trgm {
            eprintln!("Skipping test_search_users_excludes_deactivated: similarity() not available in search_path");
            return;
        }
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@deact_search_{id}:localhost");
        let username = format!("deact_search_{id}");

        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();

        storage.deactivate_user(&user_id).await.unwrap();

        let results = storage.search_users(&username, 10).await.unwrap();
        let found = results.iter().any(|r| r.user_id == user_id);
        assert!(!found);
    });
}

#[test]
fn test_filter_existing_users() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id1 = format!("@filter_user_a_{id}:localhost");
        let username1 = format!("filter_user_a_{id}");
        let user_id2 = format!("@filter_user_b_{id}:localhost");
        let username2 = format!("filter_user_b_{id}");

        storage
            .create_user(&user_id1, &username1, None, false)
            .await
            .unwrap();
        storage
            .create_user(&user_id2, &username2, None, false)
            .await
            .unwrap();

        let input = vec![
            user_id1.clone(),
            "@nonexistent:localhost".to_string(),
            user_id2.clone(),
        ];
        let existing = storage.filter_existing_users(&input).await.unwrap();
        assert_eq!(existing.len(), 2);
        assert!(existing.contains(&user_id1));
        assert!(existing.contains(&user_id2));
    });
}

#[test]
fn test_filter_existing_users_empty_input() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);

        let existing = storage.filter_existing_users(&[]).await.unwrap();
        assert!(existing.is_empty());
    });
}

#[test]
fn test_filter_existing_users_excludes_deactivated() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@filter_deact_{id}:localhost");
        let username = format!("filter_deact_{id}");

        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();
        storage.deactivate_user(&user_id).await.unwrap();

        let existing = storage
            .filter_existing_users(std::slice::from_ref(&user_id))
            .await
            .unwrap();
        assert!(!existing.contains(&user_id));
    });
}

#[test]
fn test_get_user_profile() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@profile_user_{id}:localhost");
        let username = format!("profile_user_{id}");

        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();

        storage
            .update_displayname(&user_id, Some("Profile Name"))
            .await
            .unwrap();

        let profile = storage.get_user_profile(&user_id).await.unwrap().unwrap();
        assert_eq!(profile.user_id, user_id);
        assert_eq!(profile.username, username);
        assert_eq!(profile.displayname, Some("Profile Name".to_string()));
    });
}

#[test]
fn test_get_user_profile_not_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);

        let result = storage
            .get_user_profile("@noprofile:localhost")
            .await
            .unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_get_user_profile_deactivated_returns_none() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@deact_profile_{id}:localhost");
        let username = format!("deact_profile_{id}");

        storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();
        storage.deactivate_user(&user_id).await.unwrap();

        let result = storage.get_user_profile(&user_id).await.unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_create_user_as_admin() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@admin_create_{id}:localhost");
        let username = format!("admin_create_{id}");

        let user = storage
            .create_user(&user_id, &username, Some("adminhash"), true)
            .await
            .unwrap();

        assert!(user.is_admin);
        assert_eq!(user.password_hash, Some("adminhash".to_string()));
    });
}

#[test]
fn test_create_user_no_password() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_user_storage(&pool);
        let id = unique_id();
        let user_id = format!("@nopwd_user_{id}:localhost");
        let username = format!("nopwd_user_{id}");

        let user = storage
            .create_user(&user_id, &username, None, false)
            .await
            .unwrap();

        assert_eq!(user.password_hash, None);
    });
}
