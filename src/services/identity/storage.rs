use super::models::ThirdPartyId;
use crate::common::error::ApiError;
use crate::storage::{CreateThreepidRequest, ThreepidStorage};
use sqlx::{PgPool, Row};
use std::sync::Arc;

#[derive(Clone)]
pub struct IdentityStorage {
    pool: PgPool,
    threepid_storage: ThreepidStorage,
}

impl IdentityStorage {
    pub fn new(pool: &PgPool) -> Self {
        let pool = pool.clone();
        let threepid_storage = ThreepidStorage::new(&Arc::new(pool.clone()));

        Self {
            pool,
            threepid_storage,
        }
    }

    pub async fn get_user_three_pids(&self, user_id: &str) -> Result<Vec<ThirdPartyId>, ApiError> {
        let rows = self.threepid_storage.get_threepids_by_user(user_id).await?;
        Ok(rows.into_iter().map(Self::map_threepid).collect())
    }

    pub async fn add_three_pid(&self, three_pid: &ThirdPartyId) -> Result<(), ApiError> {
        self.threepid_storage
            .add_threepid(CreateThreepidRequest {
                user_id: three_pid.user_id.clone(),
                medium: three_pid.medium.clone(),
                address: three_pid.address.clone(),
                verification_token: None,
                verification_expires_ts: None,
            })
            .await?;

        if three_pid.validated_ts > 0 {
            let _ = self
                .threepid_storage
                .verify_threepid(&three_pid.user_id, &three_pid.medium, &three_pid.address)
                .await?;
        }

        Ok(())
    }

    pub async fn remove_three_pid(
        &self,
        address: &str,
        medium: &str,
        user_id: &str,
    ) -> Result<(), ApiError> {
        self.threepid_storage
            .remove_threepid(user_id, medium, address)
            .await?;
        Ok(())
    }

    pub async fn get_three_pid_user(
        &self,
        address: &str,
        medium: &str,
    ) -> Result<Option<String>, ApiError> {
        Ok(self
            .threepid_storage
            .get_threepid_by_address(medium, address)
            .await?
            .map(|threepid| threepid.user_id))
    }

    pub async fn validate_three_pid(
        &self,
        address: &str,
        medium: &str,
        user_id: &str,
    ) -> Result<(), ApiError> {
        let _ = self
            .threepid_storage
            .verify_threepid(user_id, medium, address)
            .await?;
        Ok(())
    }

    pub async fn get_pending_three_pid_validations(
        &self,
    ) -> Result<Vec<serde_json::Value>, ApiError> {
        let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
            r#"
            SELECT address, medium, user_id, validated_ts, added_ts
            FROM user_threepids
            WHERE validated_ts < added_ts
            ORDER BY added_ts DESC
            LIMIT 100
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            ApiError::internal(format!("Failed to get pending 3PID validations: {}", e))
        })?;

        Ok(rows
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "address": r.get::<Option<String>, _>("address"),
                    "medium": r.get::<String, _>("medium"),
                    "user_id": r.get::<String, _>("user_id"),
                    "validated_ts": r.get::<Option<i64>, _>("validated_ts"),
                    "added_ts": r.get::<i64, _>("added_ts")
                })
            })
            .collect())
    }

    fn map_threepid(threepid: crate::storage::models::UserThreepid) -> ThirdPartyId {
        ThirdPartyId {
            address: threepid.address,
            medium: threepid.medium,
            user_id: threepid.user_id,
            validated_ts: threepid.validated_at.unwrap_or(threepid.added_ts),
            added_ts: threepid.added_ts,
        }
    }
}
