#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use base64::Engine;
use std::sync::atomic::{AtomicU64, Ordering};
use synapse_common::current_timestamp_millis;
use synapse_web::federation::device_sync::DeviceSyncManager;
use synapse_web::federation::key_rotation::KeyRotationManager;

use sqlx::{Pool, Postgres};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn cleanup_test_database(pool: &Pool<Postgres>) {
    sqlx::query("DELETE FROM federation_signing_keys WHERE server_name LIKE 'test%'")
        .execute(pool)
        .await
        .expect("clean up federation signing keys for the test server prefix");
}

fn generate_valid_test_key() -> String {
    use rand::RngCore;
    let mut rng = rand::rng();
    let mut secret_bytes = [0u8; 32];
    rng.fill_bytes(&mut secret_bytes);
    base64::engine::general_purpose::STANDARD_NO_PAD.encode(secret_bytes)
}

#[tokio::test]
async fn test_key_rotation_initialization() {
    let pool = crate::require_test_pool().await;

    cleanup_test_database(&pool).await;

    let id = unique_id();
    let server_name = format!("test{id}.example.com");
    let manager = KeyRotationManager::new(&pool, &server_name).with_allow_plaintext_signing_keys(true);

    let valid_key = generate_valid_test_key();
    let key_id = format!("ed25519:test_{id}");
    let result = manager.initialize(&valid_key, &key_id).await;
    if result.is_err() {
        eprintln!("Key rotation init error: {result:?}");
    }
    assert!(result.is_ok());

    let current = manager.get_current_key().await.unwrap().unwrap();
    assert_eq!(current.key_id, key_id);
    assert!(!current.public_key.is_empty());
}

#[tokio::test]
async fn test_should_rotate_keys() {
    let pool = crate::require_test_pool().await;

    cleanup_test_database(&pool).await;

    let id = unique_id();
    let server_name = format!("test{id}.example.com");
    let manager = KeyRotationManager::new(&pool, &server_name).with_allow_plaintext_signing_keys(true);

    let should_rotate_before = manager.should_rotate_keys().await;
    assert!(should_rotate_before, "Should rotate when no keys exist");

    let valid_key = generate_valid_test_key();
    let key_id = format!("ed25519:test_{id}");
    manager.initialize(&valid_key, &key_id).await.expect("Failed to initialize key");

    let current_key = manager.get_current_key().await.expect("Failed to get current key");
    assert!(current_key.is_some(), "Key should be in memory after initialization");

    let key = current_key.unwrap();
    let now = current_timestamp_millis();
    let days_until_expiry = (key.expires_at - now) / (24 * 60 * 60 * 1000);

    assert!(days_until_expiry >= 6, "Key should have at least 6 days until expiry, got {days_until_expiry} days");
}

/// 普通路径：`load_or_create_key` 会把密钥落进 `federation_signing_keys`。
///
/// ⚠️ 原名为 `test_load_or_create_key_recovers_missing_signing_key_table`，但这条用例跑在
/// **共享 `public`** 上、表本来就在，从未构造"表缺失"场景（D-79：名字超出测试体）。
/// 真正构造恢复场景的是 crate 内的隔离用例
/// `synapse_federation::key_rotation::db_tests::load_or_create_key_recreates_a_dropped_signing_keys_table`
/// —— 它在 per-test schema 上先 `DROP TABLE`，再断言自愈把表与两条索引都建回来。
#[tokio::test]
async fn test_load_or_create_key_persists_a_signing_key() {
    let pool = crate::require_test_pool().await;

    let id = unique_id();
    let server_name = format!("test{id}.example.com");
    let manager = KeyRotationManager::new(&pool, &server_name).with_allow_plaintext_signing_keys(true);

    manager.load_or_create_key().await.expect("Failed to load or create key");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM federation_signing_keys WHERE server_name = $1")
        .bind(&server_name)
        .fetch_one(&*pool)
        .await
        .expect("Failed to count federation signing keys");

    assert!(count >= 1, "Expected at least 1 key, found {count}");
}

#[tokio::test]
async fn test_device_sync_cache() {
    let pool = crate::require_test_pool().await;
    let manager = DeviceSyncManager::new(&pool, None, None);

    let devices = manager.get_local_devices("@test:example.com").await.unwrap();
    assert!(devices.is_empty());
}

#[tokio::test]
async fn test_device_revocation() {
    let pool = crate::require_test_pool().await;
    let manager = DeviceSyncManager::new(&pool, None, None);

    let result = manager.revoke_device("DEVICE123", "@test:example.com").await;
    assert!(result.is_ok());
}
