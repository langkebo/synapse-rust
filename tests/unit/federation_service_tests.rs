use std::sync::Arc;
use tokio::runtime::Runtime;
use synapse_rust::federation::key_rotation::KeyRotationManager;
use base64::Engine;

use synapse_rust::federation::device_sync::DeviceSyncManager;

use sqlx::{Pool, Postgres};

async fn setup_test_database() -> Option<Pool<Postgres>> {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgresql://synapse:synapse@localhost:5432/synapse_test".to_string()
    });

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

    // Ensure federation tables exist
    // Add any necessary schema setup here if not handled by migrations
    
    Some(pool)
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
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let valid_key = generate_valid_test_key();
        manager
            .initialize(&valid_key, "ed25519:test")
            .await
            .unwrap();

        let current = manager.get_current_key().await.unwrap().unwrap();
        assert_eq!(current.key_id, "ed25519:test");
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
        
        // Clean up keys for clean state
        sqlx::query("DELETE FROM server_keys")
            .execute(&*pool)
            .await
            .ok();

        let manager = KeyRotationManager::new(&pool, "test.example.com");

        assert!(manager.should_rotate_keys().await);

        let valid_key = generate_valid_test_key();
        manager
            .initialize(&valid_key, "ed25519:test")
            .await
            .unwrap();
        assert!(!manager.should_rotate_keys().await);
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

        // Ensure table exists (might be part of lazy migration)
        sqlx::query(r#"
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
        "#).execute(&*pool).await.ok();

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

        // Ensure table exists
        sqlx::query(r#"
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
        "#).execute(&*pool).await.ok();

        let result = manager
            .revoke_device("DEVICE123", "@test:example.com")
            .await;
        // It should return Ok even if device doesn't exist (idempotent) or if it deleted it
        assert!(result.is_ok());
    });
}
