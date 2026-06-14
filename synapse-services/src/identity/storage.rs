use super::models::ThirdPartyId;
use synapse_common::error::ApiError;
use synapse_storage::{CreateThreepidRequest, ThreepidStorage};

#[derive(Clone)]
pub struct IdentityStorage {
    threepid_storage: ThreepidStorage,
}

impl IdentityStorage {
    pub fn new(pool: &sqlx::PgPool) -> Self {
        let threepid_storage = ThreepidStorage::new(pool);

        Self { threepid_storage }
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
                verification_expires_at: None,
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

    pub async fn remove_three_pid(&self, address: &str, medium: &str, user_id: &str) -> Result<(), ApiError> {
        self.threepid_storage.remove_threepid(user_id, medium, address).await?;
        Ok(())
    }

    pub async fn get_three_pid_user(&self, address: &str, medium: &str) -> Result<Option<String>, ApiError> {
        Ok(self.threepid_storage.get_threepid_by_address(medium, address).await?.map(|threepid| threepid.user_id))
    }

    pub async fn validate_three_pid(&self, address: &str, medium: &str, user_id: &str) -> Result<(), ApiError> {
        let _ = self.threepid_storage.verify_threepid(user_id, medium, address).await?;
        Ok(())
    }

    pub async fn get_pending_three_pid_validations(&self) -> Result<Vec<serde_json::Value>, ApiError> {
        let rows = self.threepid_storage.get_pending_threepids(100).await?;

        Ok(rows
            .into_iter()
            .map(|threepid| {
                serde_json::json!({
                    "address": threepid.address,
                    "medium": threepid.medium,
                    "user_id": threepid.user_id,
                    "validated_ts": threepid.validated_at,
                    "added_ts": threepid.added_ts
                })
            })
            .collect())
    }

    fn map_threepid(threepid: synapse_storage::UserThreepid) -> ThirdPartyId {
        ThirdPartyId {
            address: threepid.address,
            medium: threepid.medium,
            user_id: threepid.user_id,
            validated_ts: threepid.validated_at.unwrap_or(threepid.added_ts),
            added_ts: threepid.added_ts,
        }
    }
}
