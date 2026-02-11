#![cfg(test)]

use sqlx::{Pool, Postgres};
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    use synapse_rust::storage::device::DeviceStorage;

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
                    "Skipping device storage tests because test database is unavailable: {}",
                    error
                );
                return None;
            }
        };

        sqlx::query("DROP TABLE IF EXISTS devices CASCADE")
            .execute(&pool)
            .await
            .ok();

        sqlx::query(
            r#"
            CREATE TABLE devices (
                device_id VARCHAR(255) PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                display_name TEXT,
                device_key JSONB,
                last_seen_ts BIGINT,
                last_seen_ip TEXT,
                created_at BIGINT NOT NULL,
                first_seen_ts BIGINT NOT NULL,
                created_ts BIGINT,
                appservice_id TEXT,
                ignored_user_list TEXT
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create devices table");

        Some(pool)
    }

    #[test]
    fn test_create_device_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => Arc::new(pool),
                None => return,
            };
            let storage = DeviceStorage::new(&pool);

            let device = storage
                .create_device("DEVICE1", "@alice:localhost", Some("My Device"))
                .await
                .unwrap();
            assert_eq!(device.device_id, "DEVICE1");
            assert_eq!(device.user_id, "@alice:localhost");
            assert_eq!(device.display_name, Some("My Device".to_string()));
        });
    }

    #[test]
    fn test_get_device() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => Arc::new(pool),
                None => return,
            };
            let storage = DeviceStorage::new(&pool);

            storage
                .create_device("DEVICE1", "@alice:localhost", None)
                .await
                .unwrap();

            let device = storage.get_device("DEVICE1").await.unwrap();
            assert!(device.is_some());
            assert_eq!(device.unwrap().user_id, "@alice:localhost");

            let device = storage.get_device("NONEXISTENT").await.unwrap();
            assert!(device.is_none());
        });
    }

    #[test]
    fn test_get_user_devices() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => Arc::new(pool),
                None => return,
            };
            let storage = DeviceStorage::new(&pool);

            storage
                .create_device("D1", "@alice:localhost", None)
                .await
                .unwrap();
            storage
                .create_device("D2", "@alice:localhost", None)
                .await
                .unwrap();
            storage
                .create_device("D3", "@bob:localhost", None)
                .await
                .unwrap();

            let devices = storage.get_user_devices("@alice:localhost").await.unwrap();
            assert_eq!(devices.len(), 2);
        });
    }

    #[test]
    fn test_update_device_display_name() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => Arc::new(pool),
                None => return,
            };
            let storage = DeviceStorage::new(&pool);

            storage
                .create_device("D1", "@alice:localhost", Some("Old Name"))
                .await
                .unwrap();
            storage
                .update_device_display_name("D1", "New Name")
                .await
                .unwrap();

            let device = storage.get_device("D1").await.unwrap().unwrap();
            assert_eq!(device.display_name, Some("New Name".to_string()));
        });
    }

    #[test]
    fn test_delete_device() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(pool) => Arc::new(pool),
                None => return,
            };
            let storage = DeviceStorage::new(&pool);

            storage
                .create_device("D1", "@alice:localhost", None)
                .await
                .unwrap();
            assert!(storage.device_exists("D1").await.unwrap());

            storage.delete_device("D1").await.unwrap();
            assert!(!storage.device_exists("D1").await.unwrap());
        });
    }
