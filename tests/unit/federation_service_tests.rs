use base64::Engine;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::federation::key_rotation::KeyRotationManager;
use tokio::runtime::Runtime;

use synapse_rust::federation::device_sync::DeviceSyncManager;

use sqlx::{Pool, Postgres};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database() -> Option<Pool<Postgres>> {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "postgresql://synapse:secret@localhost:5432/synapse_test".to_string());

    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!(
                "Skipping federation service tests because test database is unavailable: {}",
                error
            );
            return None;
        }
    };

    // 使用 IF NOT EXISTS 避免并发冲突
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS federation_signing_keys (
            server_name VARCHAR(255) NOT NULL,
            key_id VARCHAR(255) NOT NULL,
            secret_key TEXT NOT NULL,
            public_key TEXT NOT NULL,
            created_ts BIGINT NOT NULL,
            expires_at BIGINT NOT NULL,
            key_json JSONB NOT NULL DEFAULT '{}'::jsonb,
            ts_added_ms BIGINT NOT NULL,
            ts_valid_until_ms BIGINT NOT NULL,
            PRIMARY KEY (server_name, key_id)
        )
    "#,
    )
    .execute(&pool)
    .await
    .ok();

    Some(pool)
}

async fn cleanup_test_database(pool: &Pool<Postgres>) {
    // 清理测试数据
    sqlx::query("DELETE FROM federation_signing_keys WHERE server_name LIKE 'test%'")
        .execute(pool)
        .await
        .ok();
}

fn generate_valid_test_key() -> String {
    use rand::RngCore;
    let mut rng = rand::thread_rng();
    let mut secret_bytes = [0u8; 32];
    rng.fill_bytes(&mut secret_bytes);
    base64::engine::general_purpose::STANDARD_NO_PAD.encode(secret_bytes)
}

#[test]
fn test_key_rotation_initialization() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };

        cleanup_test_database(&pool).await;

        let id = unique_id();
        let server_name = format!("test{}.example.com", id);
        let manager = KeyRotationManager::new(&pool, &server_name);

        let valid_key = generate_valid_test_key();
        let key_id = format!("ed25519:test_{}", id);
        let result = manager.initialize(&valid_key, &key_id).await;
        if result.is_err() {
            eprintln!("Key rotation init error: {:?}", result);
        }
        assert!(result.is_ok());

        let current = manager.get_current_key().await.unwrap().unwrap();
        assert_eq!(current.key_id, key_id);
        assert!(!current.public_key.is_empty());
    });
}

#[test]
fn test_should_rotate_keys() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };

        cleanup_test_database(&pool).await;

        let id = unique_id();
        let server_name = format!("test{}.example.com", id);
        let manager = KeyRotationManager::new(&pool, &server_name);

        let should_rotate_before = manager.should_rotate_keys().await;
        assert!(should_rotate_before, "Should rotate when no keys exist");

        let valid_key = generate_valid_test_key();
        let key_id = format!("ed25519:test_{}", id);
        manager
            .initialize(&valid_key, &key_id)
            .await
            .expect("Failed to initialize key");

        let current_key = manager
            .get_current_key()
            .await
            .expect("Failed to get current key");
        assert!(
            current_key.is_some(),
            "Key should be in memory after initialization"
        );

        let key = current_key.unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        let days_until_expiry = (key.expires_at - now) / (24 * 60 * 60 * 1000);

        assert!(
            days_until_expiry >= 6,
            "Key should have at least 6 days until expiry, got {} days",
            days_until_expiry
        );
    });
}

#[test]
fn test_load_or_create_key_recovers_missing_signing_key_table() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };

        let id = unique_id();
        let server_name = format!("test{}.example.com", id);
        let manager = KeyRotationManager::new(&pool, &server_name);

        // 先尝试初始化（如果不存在表会自动创建）
        manager
            .load_or_create_key()
            .await
            .expect("Failed to load or create key");

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM federation_signing_keys WHERE server_name = $1",
        )
        .bind(&server_name)
        .fetch_one(&*pool)
        .await
        .expect("Failed to count federation signing keys");

        assert!(count >= 1, "Expected at least 1 key, found {}", count);
    });
}

#[test]
fn test_device_sync_cache() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let manager = DeviceSyncManager::new(&pool, None, None);

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS devices (
                device_id VARCHAR(255) NOT NULL,
                user_id VARCHAR(255) NOT NULL,
                display_name VARCHAR(255),
                last_seen_ts BIGINT,
                last_seen_ip VARCHAR(255),
                created_ts BIGINT,
                hidden BOOLEAN DEFAULT FALSE,
                PRIMARY KEY (device_id, user_id)
            )
        "#,
        )
        .execute(&*pool)
        .await
        .ok();

        let devices = manager
            .get_local_devices("@test:example.com")
            .await
            .unwrap();
        assert!(devices.is_empty());
    });
}

#[test]
fn test_device_revocation() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let manager = DeviceSyncManager::new(&pool, None, None);

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS devices (
                device_id VARCHAR(255) NOT NULL,
                user_id VARCHAR(255) NOT NULL,
                display_name VARCHAR(255),
                last_seen_ts BIGINT,
                last_seen_ip VARCHAR(255),
                created_ts BIGINT,
                hidden BOOLEAN DEFAULT FALSE,
                PRIMARY KEY (device_id, user_id)
            )
        "#,
        )
        .execute(&*pool)
        .await
        .ok();

        let result = manager
            .revoke_device("DEVICE123", "@test:example.com")
            .await;
        assert!(result.is_ok());
    });
}
