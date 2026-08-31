// QR Code Login Storage - MSC4388
// Secure out-of-band channel for sign in with QR
// Following project field naming standards

use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

#[async_trait]
pub trait QrLoginStoreApi: Send + Sync {
    async fn create_qr_login(
        &self,
        transaction_id: &str,
        user_id: &str,
        device_id: Option<&str>,
    ) -> Result<(), sqlx::Error>;
    async fn get_qr_transaction(&self, transaction_id: &str) -> Result<Option<QrTransaction>, sqlx::Error>;
    async fn update_qr_status(&self, transaction_id: &str, status: &str) -> Result<(), sqlx::Error>;
    async fn delete_qr_transaction(&self, transaction_id: &str) -> Result<(), sqlx::Error>;
    async fn cleanup_expired(&self) -> Result<u64, sqlx::Error>;
}

#[derive(Clone)]
pub struct QrLoginStorage {
    pool: Arc<PgPool>,
}

impl QrLoginStorage {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Create a new QR login transaction
    pub async fn create_qr_login(
        &self,
        transaction_id: &str,
        user_id: &str,
        device_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        // QR code expires in 5 minutes (300000ms)
        let expires_at = now + 300000;

        sqlx::query(
            r"
            INSERT INTO qr_login_transactions (transaction_id, user_id, device_id, status, created_ts, expires_at)
            VALUES ($1, $2, $3, 'pending', $4, $5)
            ON CONFLICT (transaction_id) DO UPDATE
            SET user_id = EXCLUDED.user_id, device_id = EXCLUDED.device_id, status = 'pending', expires_at = EXCLUDED.expires_at
            ",
        )
        .bind(transaction_id)
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .bind(expires_at)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Get QR login transaction by ID
    pub async fn get_qr_transaction(&self, transaction_id: &str) -> Result<Option<QrTransaction>, sqlx::Error> {
        let result = sqlx::query_as::<_, (String, String, Option<String>, String, i64, Option<i64>, i64)>(
            r"
            SELECT transaction_id, user_id, device_id, status, created_ts, updated_ts, expires_at
            FROM qr_login_transactions
            WHERE transaction_id = $1
            ",
        )
        .bind(transaction_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|(transaction_id, user_id, device_id, status, created_ts, updated_ts, expires_at)| {
            QrTransaction { transaction_id, user_id, device_id, status, created_ts, updated_ts, expires_at }
        }))
    }

    /// Update QR login transaction status
    pub async fn update_qr_status(&self, transaction_id: &str, status: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query(
            r"
            UPDATE qr_login_transactions
            SET status = $2, updated_ts = $3
            WHERE transaction_id = $1
            ",
        )
        .bind(transaction_id)
        .bind(status)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Delete QR login transaction (cleanup)
    pub async fn delete_qr_transaction(&self, transaction_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            DELETE FROM qr_login_transactions
            WHERE transaction_id = $1
            ",
        )
        .bind(transaction_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Clean up expired transactions
    pub async fn cleanup_expired(&self) -> Result<u64, sqlx::Error> {
        let now = current_timestamp_millis();
        let result = sqlx::query(
            r"
            DELETE FROM qr_login_transactions
            WHERE expires_at < $1
            ",
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

/// QR Login Transaction
/// Following project field naming standards:
/// - created_ts: NOT NULL, milliseconds timestamp
/// - updated_ts: NULLABLE, milliseconds timestamp
/// - expires_at: NOT NULL, milliseconds timestamp
#[derive(Debug, Clone)]
pub struct QrTransaction {
    pub transaction_id: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub status: String,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub expires_at: i64,
}

#[async_trait]
impl QrLoginStoreApi for QrLoginStorage {
    async fn create_qr_login(
        &self,
        transaction_id: &str,
        user_id: &str,
        device_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.create_qr_login(transaction_id, user_id, device_id).await
    }
    async fn get_qr_transaction(&self, transaction_id: &str) -> Result<Option<QrTransaction>, sqlx::Error> {
        self.get_qr_transaction(transaction_id).await
    }
    async fn update_qr_status(&self, transaction_id: &str, status: &str) -> Result<(), sqlx::Error> {
        self.update_qr_status(transaction_id, status).await
    }
    async fn delete_qr_transaction(&self, transaction_id: &str) -> Result<(), sqlx::Error> {
        self.delete_qr_transaction(transaction_id).await
    }
    async fn cleanup_expired(&self) -> Result<u64, sqlx::Error> {
        self.cleanup_expired().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qr_transaction_structure() {
        let txn = QrTransaction {
            transaction_id: "qr_test123".to_string(),
            user_id: "@user:localhost".to_string(),
            device_id: Some("DEVICE123".to_string()),
            status: "pending".to_string(),
            created_ts: 1700000000000i64,
            updated_ts: None,
            expires_at: 1700000300000i64,
        };

        assert_eq!(txn.status, "pending");
        assert!(txn.device_id.is_some());
        assert!(txn.updated_ts.is_none());
    }

    #[test]
    fn test_expiry_calculation() {
        let created_ts = 1700000000000i64;
        let expiry_ms = 5 * 60 * 1000; // 5 minutes
        let expires_at = created_ts + expiry_ms;

        assert_eq!(expires_at, 1700000300000i64);
    }
}

#[cfg(test)]
mod db_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::env;
use std::time::Duration;
use std::sync::Arc;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse_test".to_string());
        let pool =
            PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(30)).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn ensure_test_user(pool: &PgPool, user_id: &str) {
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

    fn make_suffix() -> String {
        uuid::Uuid::new_v4().to_string().replace('-', "")
    }

    #[tokio::test]
    async fn create_qr_login_then_get() {
        let pool = test_pool().await;
        let storage = QrLoginStorage::new(pool.clone());
        let suffix = make_suffix();
        let txn_id = format!("qr_txn_{suffix}");
        let user_id = format!("@qrlogin_{suffix}:test");
        ensure_test_user(&pool, &user_id).await;

        storage.create_qr_login(&txn_id, &user_id, Some("DEVICE1")).await.unwrap();
        let txn = storage.get_qr_transaction(&txn_id).await.unwrap().unwrap();
        assert_eq!(txn.transaction_id, txn_id);
        assert_eq!(txn.user_id, user_id);
        assert_eq!(txn.status, "pending");
        assert_eq!(txn.device_id.as_deref(), Some("DEVICE1"));

        let _ = sqlx::query("DELETE FROM qr_login_transactions WHERE transaction_id = $1")
            .bind(&txn_id)
            .execute(pool.as_ref())
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE user_id = $1").bind(&user_id).execute(pool.as_ref()).await;
    }

    #[tokio::test]
    async fn get_qr_transaction_none_for_missing() {
        let pool = test_pool().await;
        let storage = QrLoginStorage::new(pool);
        let suffix = make_suffix();
        assert!(storage.get_qr_transaction(&format!("missing_{suffix}")).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn update_qr_status_changes_status() {
        let pool = test_pool().await;
        let storage = QrLoginStorage::new(pool.clone());
        let suffix = make_suffix();
        let txn_id = format!("qr_txn_{suffix}");
        let user_id = format!("@qrupdate_{suffix}:test");
        ensure_test_user(&pool, &user_id).await;

        storage.create_qr_login(&txn_id, &user_id, None).await.unwrap();
        storage.update_qr_status(&txn_id, "confirmed").await.unwrap();
        let txn = storage.get_qr_transaction(&txn_id).await.unwrap().unwrap();
        assert_eq!(txn.status, "confirmed");
        assert!(txn.updated_ts.is_some(), "update 应写入 updated_ts");

        let _ = sqlx::query("DELETE FROM qr_login_transactions WHERE transaction_id = $1")
            .bind(&txn_id)
            .execute(pool.as_ref())
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE user_id = $1").bind(&user_id).execute(pool.as_ref()).await;
    }

    #[tokio::test]
    async fn delete_qr_transaction_removes() {
        let pool = test_pool().await;
        let storage = QrLoginStorage::new(pool.clone());
        let suffix = make_suffix();
        let txn_id = format!("qr_txn_{suffix}");
        let user_id = format!("@qrdelete_{suffix}:test");
        ensure_test_user(&pool, &user_id).await;

        storage.create_qr_login(&txn_id, &user_id, None).await.unwrap();
        storage.delete_qr_transaction(&txn_id).await.unwrap();
        assert!(storage.get_qr_transaction(&txn_id).await.unwrap().is_none());

        let _ = sqlx::query("DELETE FROM users WHERE user_id = $1").bind(&user_id).execute(pool.as_ref()).await;
    }

    #[tokio::test]
    async fn cleanup_expired_removes_expired_keeps_valid() {
        let pool = test_pool().await;
        let storage = QrLoginStorage::new(pool.clone());
        let suffix = make_suffix();
        let user_id = format!("@qrcleanup_{suffix}:test");
        let valid_txn = format!("qr_valid_{suffix}");
        ensure_test_user(&pool, &user_id).await;

        // 直接插入已过期记录（create_qr_login 总是生成 5 分钟后过期）
        let expired_txn = format!("qr_expired_{suffix}");
        let now = current_timestamp_millis();
        sqlx::query(
            "INSERT INTO qr_login_transactions (transaction_id, user_id, device_id, status, created_ts, expires_at) VALUES ($1, $2, NULL, 'pending', $3, $4)",
        )
        .bind(&expired_txn)
        .bind(&user_id)
        .bind(now - 10000)
        .bind(now - 1000)
        .execute(pool.as_ref())
        .await
        .unwrap();

        storage.create_qr_login(&valid_txn, &user_id, None).await.unwrap();

        let removed = storage.cleanup_expired().await.unwrap();
        assert!(removed >= 1, "应至少删除我们插入的过期记录，实际 removed={removed}");
        assert!(storage.get_qr_transaction(&expired_txn).await.unwrap().is_none());
        assert!(storage.get_qr_transaction(&valid_txn).await.unwrap().is_some());

        let _ = sqlx::query("DELETE FROM qr_login_transactions WHERE transaction_id IN ($1, $2)")
            .bind(&valid_txn)
            .bind(&expired_txn)
            .execute(pool.as_ref())
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE user_id = $1").bind(&user_id).execute(pool.as_ref()).await;
    }
}
