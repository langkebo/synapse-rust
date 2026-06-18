use serde_json::Value;
use std::sync::Arc;
use synapse_common::crypto::random_string;
use synapse_common::ApiError;
use synapse_storage::account_data::AccountDataStorage;
use synapse_storage::filter::{CreateFilterRequest, FilterStorage};
use synapse_storage::openid_token::{CreateOpenIdTokenRequest, OpenIdToken, OpenIdTokenStorage};
use synapse_storage::room::RoomStorage;
use synapse_storage::room_account_data::RoomAccountDataStorage;
use synapse_storage::user::UserStorage;
use tracing::instrument;

type AccountDataWithTimestamp = (Value, Option<i64>);

pub struct AccountDataService {
    account_data_storage: AccountDataStorage,
    user_storage: UserStorage,
    room_storage: RoomStorage,
    filter_storage: FilterStorage,
    openid_token_storage: OpenIdTokenStorage,
}

impl AccountDataService {
    pub fn new(
        pool: &Arc<sqlx::PgPool>,
        user_storage: UserStorage,
        room_storage: RoomStorage,
        filter_storage: FilterStorage,
        openid_token_storage: OpenIdTokenStorage,
    ) -> Self {
        Self {
            account_data_storage: AccountDataStorage::new(pool),
            user_storage,
            room_storage,
            filter_storage,
            openid_token_storage,
        }
    }

    #[instrument(skip(self))]
    pub async fn list_account_data(&self, user_id: &str) -> Result<serde_json::Map<String, Value>, ApiError> {
        let result = self.account_data_storage.list_account_data(user_id).await?;
        Ok(result.into_iter().map(|row| (row.data_type, row.content)).collect())
    }

    #[instrument(skip(self, body))]
    pub async fn set_account_data(&self, user_id: &str, data_type: &str, body: &Value) -> Result<(), ApiError> {
        validate_account_data_payload(data_type, body)?;
        self.user_storage
            .upsert_account_data_content(user_id, data_type, body)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to save account data", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_account_data(&self, user_id: &str, data_type: &str) -> Result<Option<Value>, ApiError> {
        self.user_storage
            .get_account_data_content(user_id, data_type)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))
    }

    #[instrument(skip(self))]
    pub async fn delete_account_data(&self, user_id: &str, data_type: &str) -> Result<bool, ApiError> {
        self.account_data_storage.delete_account_data(user_id, data_type).await
    }

    #[instrument(skip(self, body))]
    pub async fn set_room_account_data(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
        body: &Value,
    ) -> Result<(), ApiError> {
        validate_account_data_payload(data_type, body)?;
        self.room_storage
            .set_room_account_data(room_id, user_id, data_type, body)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to save room account data", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_room_account_data(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<Option<Value>, ApiError> {
        RoomAccountDataStorage::get_room_account_data_content(&self.room_storage.pool, user_id, room_id, data_type)
            .await
    }

    #[instrument(skip(self))]
    pub async fn get_room_account_data_with_ts(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<Option<AccountDataWithTimestamp>, ApiError> {
        RoomAccountDataStorage::get_room_account_data_with_ts(&self.room_storage.pool, user_id, room_id, data_type)
            .await
    }

    #[instrument(skip(self))]
    pub async fn delete_room_account_data(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<bool, ApiError> {
        RoomAccountDataStorage::delete_room_account_data(&self.room_storage.pool, user_id, room_id, data_type).await
    }

    #[instrument(skip(self, content))]
    pub async fn create_filter(&self, user_id: &str, content: Value) -> Result<String, ApiError> {
        let filter_id = random_string(16);
        self.filter_storage
            .create_filter(CreateFilterRequest { user_id: user_id.to_string(), filter_id: filter_id.clone(), content })
            .await?;
        Ok(filter_id)
    }

    #[instrument(skip(self))]
    pub async fn get_filter(&self, user_id: &str, filter_id: &str) -> Result<Option<Value>, ApiError> {
        Ok(self.filter_storage.get_filter(user_id, filter_id).await?.map(|filter| filter.content))
    }

    #[instrument(skip(self))]
    pub async fn delete_filter(&self, user_id: &str, filter_id: &str) -> Result<bool, ApiError> {
        self.filter_storage.delete_filter(user_id, filter_id).await
    }

    #[instrument(skip(self))]
    pub async fn create_openid_token(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        expires_in_seconds: i64,
    ) -> Result<(String, i64), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        let token = random_string(32);
        let expires_at = now + expires_in_seconds * 1000;
        self.openid_token_storage
            .create_token(CreateOpenIdTokenRequest {
                token: token.clone(),
                user_id: user_id.to_string(),
                device_id: device_id.map(str::to_owned),
                expires_at,
            })
            .await?;
        Ok((token, expires_in_seconds))
    }

    #[instrument(skip(self, token))]
    pub async fn validate_openid_token(&self, token: &str) -> Result<Option<OpenIdToken>, ApiError> {
        self.openid_token_storage.validate_token(token).await
    }
}

fn validate_account_data_payload(data_type: &str, body: &Value) -> Result<(), ApiError> {
    if data_type.len() > 128 {
        return Err(ApiError::bad_request("data_type too long (max 128 characters)".to_string()));
    }

    let body_str = serde_json::to_string(body).map_err(|e| ApiError::bad_request(format!("Invalid JSON: {e}")))?;
    if body_str.len() > 65536 {
        return Err(ApiError::bad_request("Account data too large (max 64KB)".to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_validate_account_data_ok() {
        let body = json!({"key": "value"});
        assert!(validate_account_data_payload("test.type", &body).is_ok());
    }

    #[test]
    fn test_validate_account_data_empty_body() {
        let body = json!({});
        assert!(validate_account_data_payload("empty.type", &body).is_ok());
    }

    #[test]
    fn test_validate_account_data_type_too_long() {
        let body = json!({"key": "value"});
        let long_type = "a".repeat(129);
        let result = validate_account_data_payload(&long_type, &body);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("too long"));
    }

    #[test]
    fn test_validate_account_data_type_at_limit() {
        let body = json!({"key": "value"});
        let max_type = "a".repeat(128);
        assert!(validate_account_data_payload(&max_type, &body).is_ok());
    }

    #[test]
    fn test_validate_account_data_type_empty() {
        let body = json!({"key": "value"});
        assert!(validate_account_data_payload("", &body).is_ok());
    }

    #[test]
    fn test_validate_account_data_body_too_large() {
        let large_value = "x".repeat(65537);
        let body = json!({"key": large_value});
        let result = validate_account_data_payload("test.type", &body);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("too large"));
    }

    #[test]
    fn test_validate_account_data_body_at_limit() {
        let value = "x".repeat(65500);
        let body = json!({"key": value});
        assert!(validate_account_data_payload("test.type", &body).is_ok());
    }
}