#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::storage::device::DeviceStorage;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    sqlx::query("DROP TABLE IF EXISTS devices CASCADE").execute(pool.as_ref()).await.ok();

    sqlx::query(
        r#"
        CREATE TABLE devices (
            device_id VARCHAR(255) PRIMARY KEY,
            user_id VARCHAR(255) NOT NULL,
            display_name TEXT,
            device_key JSONB,
            last_seen_ts BIGINT,
            last_seen_ip TEXT,
            first_seen_ts BIGINT NOT NULL,
            created_ts BIGINT NOT NULL,
            appservice_id TEXT,
            ignored_user_list TEXT
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create devices table");
}

#[tokio::test]
async fn test_create_device_success() {
        let pool = crate::require_test_pool().await;
        setup_test_database(&pool).await;
        let storage = DeviceStorage::new(&pool);
        let id = unique_id();
        let device_id = format!("DEVICE_{}", id);
        let user_id = format!("@alice_{}:localhost", id);

        let device = storage.create_device(&device_id, &user_id, Some("My Device")).await.unwrap();
        assert_eq!(device.device_id, device_id);
        assert_eq!(device.user_id, user_id);
        assert_eq!(device.display_name, Some("My Device".to_string()));
}

#[tokio::test]
async fn test_get_device() {
        let pool = crate::require_test_pool().await;
        setup_test_database(&pool).await;
        let storage = DeviceStorage::new(&pool);
        let id = unique_id();
        let device_id = format!("DEVICE_{}", id);
        let user_id = format!("@alice_{}:localhost", id);

        storage.create_device(&device_id, &user_id, None).await.unwrap();

        let device = storage.get_device(&device_id).await.unwrap();
        assert!(device.is_some());
        assert_eq!(device.unwrap().user_id, user_id);

        let device = storage.get_device("NONEXISTENT").await.unwrap();
        assert!(device.is_none());
}

#[tokio::test]
async fn test_get_user_devices() {
        let pool = crate::require_test_pool().await;
        setup_test_database(&pool).await;
        let storage = DeviceStorage::new(&pool);
        let id = unique_id();
        let user_id = format!("@alice_{}:localhost", id);
        let bob_id = format!("@bob_{}:localhost", id);

        storage.create_device(&format!("D1_{}", id), &user_id, None).await.unwrap();
        storage.create_device(&format!("D2_{}", id), &user_id, None).await.unwrap();
        storage.create_device(&format!("D3_{}", id), &bob_id, None).await.unwrap();

        let devices = storage.get_user_devices(&user_id).await.unwrap();
        assert_eq!(devices.len(), 2);
}

#[tokio::test]
async fn test_update_device_display_name() {
        let pool = crate::require_test_pool().await;
        setup_test_database(&pool).await;
        let storage = DeviceStorage::new(&pool);
        let id = unique_id();
        let device_id = format!("D_{}", id);
        let user_id = format!("@alice_{}:localhost", id);

        storage.create_device(&device_id, &user_id, Some("Old Name")).await.unwrap();
        storage.update_device_display_name(&device_id, "New Name").await.unwrap();

        let device = storage.get_device(&device_id).await.unwrap().unwrap();
        assert_eq!(device.display_name, Some("New Name".to_string()));
}

#[tokio::test]
async fn test_delete_device() {
        let pool = crate::require_test_pool().await;
        setup_test_database(&pool).await;
        let storage = DeviceStorage::new(&pool);
        let id = unique_id();
        let device_id = format!("D_{}", id);
        let user_id = format!("@alice_{}:localhost", id);

        storage.create_device(&device_id, &user_id, None).await.unwrap();
        assert!(storage.device_exists(&device_id).await.unwrap());

        storage.delete_device(&device_id).await.unwrap();
        assert!(!storage.device_exists(&device_id).await.unwrap());
}
