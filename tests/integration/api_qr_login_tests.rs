#![cfg(test)]

mod qr_login_integration_suite {
    use sqlx::postgres::PgPoolOptions;
    use std::sync::Arc;
    use std::time::Duration;
    use synapse_storage::QrLoginStorage;

    #[tokio::test]
    async fn test_qr_login_storage_operations() {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://synapse:secret@localhost:5432/synapse_test".to_string());

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        let storage = QrLoginStorage::new(Arc::new(pool));
        let transaction_id = format!("test_qr_{}", uuid::Uuid::new_v4());
        let user_id = "@testuser:localhost";
        let device_id = Some("TEST_DEVICE");

        storage.create_qr_login(&transaction_id, user_id, device_id).await.expect("Failed to create QR login");

        let result = storage.get_qr_transaction(&transaction_id).await.expect("Failed to get QR transaction");

        assert!(result.is_some());
        let txn = result.unwrap();
        assert_eq!(txn.transaction_id, transaction_id);
        assert_eq!(txn.user_id, user_id);
        assert_eq!(txn.device_id, device_id);
        assert_eq!(txn.status, "pending");

        storage.update_qr_status(&transaction_id, "confirmed").await.expect("Failed to update status");

        let result = storage.get_qr_transaction(&transaction_id).await.expect("Failed to get QR transaction");

        assert!(result.is_some());
        assert_eq!(result.unwrap().status, "confirmed");

        storage.delete_qr_transaction(&transaction_id).await.expect("Failed to delete transaction");
    }
}
