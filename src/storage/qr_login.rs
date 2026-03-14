// QR Code Login Storage - MSC4388
// Secure out-of-band channel for sign in with QR
// Following project field naming standards

use sqlx::PgPool;
use std::sync::Arc;

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
        let now = chrono::Utc::now().timestamp_millis();
        // QR code expires in 5 minutes (300000ms)
        let expires_at = now + 300000;

        sqlx::query(
            r#"
            INSERT INTO qr_login_transactions (transaction_id, user_id, device_id, status, created_ts, expires_at)
            VALUES ($1, $2, $3, 'pending', $4, $5)
            ON CONFLICT (transaction_id) DO UPDATE 
            SET user_id = EXCLUDED.user_id, device_id = EXCLUDED.device_id, status = 'pending', expires_at = EXCLUDED.expires_at
            "#,
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
    pub async fn get_qr_transaction(
        &self,
        transaction_id: &str,
    ) -> Result<Option<QrTransaction>, sqlx::Error> {
        let result = sqlx::query_as::<_, (String, String, Option<String>, String, i64, Option<i64>, i64)>(
            r#"
            SELECT transaction_id, user_id, device_id, status, created_ts, updated_ts, expires_at 
            FROM qr_login_transactions 
            WHERE transaction_id = $1
            "#,
        )
        .bind(transaction_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(
            |(transaction_id, user_id, device_id, status, created_ts, updated_ts, expires_at)| QrTransaction {
                transaction_id,
                user_id,
                device_id,
                status,
                created_ts,
                updated_ts,
                expires_at,
            },
        ))
    }

    /// Update QR login transaction status
    pub async fn update_qr_status(
        &self,
        transaction_id: &str,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        
        sqlx::query(
            r#"
            UPDATE qr_login_transactions 
            SET status = $2, updated_ts = $3
            WHERE transaction_id = $1
            "#,
        )
        .bind(transaction_id)
        .bind(status)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Delete QR login transaction (cleanup)
    pub async fn delete_qr_transaction(
        &self,
        transaction_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM qr_login_transactions 
            WHERE transaction_id = $1
            "#,
        )
        .bind(transaction_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Clean up expired transactions
    pub async fn cleanup_expired(&self) -> Result<u64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query(
            r#"
            DELETE FROM qr_login_transactions 
            WHERE expires_at < $1
            "#,
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
/// - expires_at: NOT NULL (or use expires_ts for consistency), milliseconds timestamp
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
