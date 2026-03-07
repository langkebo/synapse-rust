use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DehydratedDevice {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub device_data: serde_json::Value,
    pub algorithm: String,
    pub account: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub expires_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDehydratedDeviceParams {
    pub user_id: String,
    pub device_id: String,
    pub device_data: serde_json::Value,
    pub algorithm: String,
    pub account: Option<serde_json::Value>,
    pub expires_in_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DehydratedDeviceEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub content: DehydratedDeviceContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DehydratedDeviceContent {
    pub device_id: String,
    pub algorithm: String,
    pub device_data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DehydratedDeviceKey {
    pub algorithm: String,
    pub key_id: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DehydratedDeviceClaimRequest {
    pub device_id: String,
    pub key: DehydratedDeviceKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DehydratedDeviceClaimResponse {
    pub device_id: String,
    pub device_data: serde_json::Value,
    pub account: Option<serde_json::Value>,
}

#[derive(Clone)]
pub struct DehydratedDeviceStorage {
    pool: Arc<Pool<Postgres>>,
}

impl DehydratedDeviceStorage {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    pub async fn create_device(
        &self,
        params: CreateDehydratedDeviceParams,
    ) -> Result<DehydratedDevice, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let expires_ts = params.expires_in_ms.map(|ms| now + ms);

        sqlx::query_as::<_, DehydratedDevice>(
            r#"
            INSERT INTO dehydrated_devices 
                (user_id, device_id, device_data, algorithm, account, created_ts, updated_ts, expires_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, device_id) DO UPDATE SET
                device_data = EXCLUDED.device_data,
                algorithm = EXCLUDED.algorithm,
                account = EXCLUDED.account,
                updated_ts = EXCLUDED.updated_ts,
                expires_ts = EXCLUDED.expires_ts
            RETURNING *
            "#,
        )
        .bind(&params.user_id)
        .bind(&params.device_id)
        .bind(&params.device_data)
        .bind(&params.algorithm)
        .bind(&params.account)
        .bind(now)
        .bind(now)
        .bind(expires_ts)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<DehydratedDevice>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as::<_, DehydratedDevice>(
            r#"
            SELECT * FROM dehydrated_devices 
            WHERE user_id = $1 AND device_id = $2 
              AND (expires_ts IS NULL OR expires_ts > $3)
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_devices_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<DehydratedDevice>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as::<_, DehydratedDevice>(
            r#"
            SELECT * FROM dehydrated_devices 
            WHERE user_id = $1 
              AND (expires_ts IS NULL OR expires_ts > $2)
            ORDER BY created_ts DESC
            "#,
        )
        .bind(user_id)
        .bind(now)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn delete_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM dehydrated_devices 
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_devices_for_user(
        &self,
        user_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM dehydrated_devices WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn claim_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<DehydratedDevice>, sqlx::Error> {
        let device = self.get_device(user_id, device_id).await?;

        if device.is_some() {
            self.delete_device(user_id, device_id).await?;
        }

        Ok(device)
    }

    pub async fn cleanup_expired_devices(&self) -> Result<u64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r#"
            DELETE FROM dehydrated_devices 
            WHERE expires_ts IS NOT NULL AND expires_ts < $1
            "#,
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn update_device_data(
        &self,
        user_id: &str,
        device_id: &str,
        device_data: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE dehydrated_devices 
            SET device_data = $3, updated_ts = $4
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(device_data)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_account(
        &self,
        user_id: &str,
        device_id: &str,
        account: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE dehydrated_devices 
            SET account = $3, updated_ts = $4
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(account)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn extend_expiry(
        &self,
        user_id: &str,
        device_id: &str,
        extend_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE dehydrated_devices 
            SET expires_ts = expires_ts + $3
            WHERE user_id = $1 AND device_id = $2 AND expires_ts IS NOT NULL
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(extend_ms)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dehydrated_device_struct() {
        let device = DehydratedDevice {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEHYDRATED_DEVICE".to_string(),
            device_data: serde_json::json!({"key": "encrypted_data"}),
            algorithm: "m.megolm.v1".to_string(),
            account: Some(serde_json::json!({"account": "data"})),
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
            expires_ts: Some(1234654290000),
        };

        assert_eq!(device.user_id, "@alice:example.com");
        assert_eq!(device.device_id, "DEHYDRATED_DEVICE");
        assert_eq!(device.algorithm, "m.megolm.v1");
    }

    #[test]
    fn test_create_params() {
        let params = CreateDehydratedDeviceParams {
            user_id: "@bob:example.com".to_string(),
            device_id: "NEW_DEVICE".to_string(),
            device_data: serde_json::json!({"encrypted": "data"}),
            algorithm: "m.megolm.v1".to_string(),
            account: None,
            expires_in_ms: Some(86400000),
        };

        assert_eq!(params.user_id, "@bob:example.com");
        assert!(params.expires_in_ms.is_some());
    }

    #[test]
    fn test_dehydrated_device_event() {
        let event = DehydratedDeviceEvent {
            event_type: "m.dehydrated_device".to_string(),
            content: DehydratedDeviceContent {
                device_id: "DEVICE123".to_string(),
                algorithm: "m.megolm.v1".to_string(),
                device_data: serde_json::json!({"data": "value"}),
                account: None,
            },
        };

        assert_eq!(event.event_type, "m.dehydrated_device");
        assert_eq!(event.content.device_id, "DEVICE123");
    }

    #[test]
    fn test_dehydrated_device_without_expiry() {
        let device = DehydratedDevice {
            id: 2,
            user_id: "@charlie:example.com".to_string(),
            device_id: "NO_EXPIRY".to_string(),
            device_data: serde_json::json!({}),
            algorithm: "m.megolm.v1".to_string(),
            account: None,
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
            expires_ts: None,
        };

        assert!(device.expires_ts.is_none());
    }

    #[test]
    fn test_dehydrated_device_with_account() {
        let device = DehydratedDevice {
            id: 3,
            user_id: "@dave:example.com".to_string(),
            device_id: "WITH_ACCOUNT".to_string(),
            device_data: serde_json::json!({"keys": "data"}),
            algorithm: "m.megolm.v1".to_string(),
            account: Some(serde_json::json!({
                "pickle": "account_pickle_data",
                "passphrase": "encrypted_passphrase"
            })),
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
            expires_ts: None,
        };

        assert!(device.account.is_some());
        let account = device.account.unwrap();
        assert!(account.get("pickle").is_some());
    }

    #[test]
    fn test_dehydrated_device_key() {
        let key = DehydratedDeviceKey {
            algorithm: "m.megolm.v1".to_string(),
            key_id: "key123".to_string(),
            key: "base64_encoded_key".to_string(),
        };

        assert_eq!(key.algorithm, "m.megolm.v1");
        assert_eq!(key.key_id, "key123");
    }

    #[test]
    fn test_claim_request() {
        let request = DehydratedDeviceClaimRequest {
            device_id: "DEVICE456".to_string(),
            key: DehydratedDeviceKey {
                algorithm: "m.megolm.v1".to_string(),
                key_id: "key456".to_string(),
                key: "secret_key".to_string(),
            },
        };

        assert_eq!(request.device_id, "DEVICE456");
        assert_eq!(request.key.key_id, "key456");
    }

    #[test]
    fn test_claim_response() {
        let response = DehydratedDeviceClaimResponse {
            device_id: "DEVICE789".to_string(),
            device_data: serde_json::json!({"restored": "data"}),
            account: Some(serde_json::json!({"pickle": "restored"})),
        };

        assert_eq!(response.device_id, "DEVICE789");
        assert!(response.account.is_some());
    }
}
