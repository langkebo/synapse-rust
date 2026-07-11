use serde_json::Value;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::crypto::random_string;
use synapse_common::ApiError;
use synapse_storage::account_data::AccountDataStoreApi;
use synapse_storage::filter::{CreateFilterRequest, FilterStoreApi};
use synapse_storage::openid_token::{CreateOpenIdTokenRequest, OpenIdToken, OpenIdTokenStoreApi};
use synapse_storage::room_account_data::RoomAccountDataStoreApi;
use synapse_storage::user::UserStore;
use tracing::instrument;

type AccountDataWithTimestamp = (Value, Option<i64>);

pub struct AccountDataService {
    cache: Arc<CacheManager>,
    account_data_storage: Arc<dyn AccountDataStoreApi>,
    user_storage: Arc<dyn UserStore>,
    room_account_data_storage: Arc<dyn RoomAccountDataStoreApi>,
    filter_storage: Arc<dyn FilterStoreApi>,
    openid_token_storage: Arc<dyn OpenIdTokenStoreApi>,
}

impl AccountDataService {
    pub fn new(
        cache: Arc<CacheManager>,
        account_data_storage: Arc<dyn AccountDataStoreApi>,
        user_storage: Arc<dyn UserStore>,
        room_account_data_storage: Arc<dyn RoomAccountDataStoreApi>,
        filter_storage: Arc<dyn FilterStoreApi>,
        openid_token_storage: Arc<dyn OpenIdTokenStoreApi>,
    ) -> Self {
        Self { cache, account_data_storage, user_storage, room_account_data_storage, filter_storage, openid_token_storage }
    }

    #[instrument(skip(self))]
    pub async fn list_account_data(&self, user_id: &str) -> Result<serde_json::Map<String, Value>, ApiError> {
        let result = self
            .account_data_storage
            .list_account_data(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to list account data", &e))?;
        Ok(result.into_iter().map(|row| (row.data_type, row.content)).collect())
    }

    #[instrument(skip(self, body))]
    pub async fn set_account_data(&self, user_id: &str, data_type: &str, body: &Value) -> Result<(), ApiError> {
        validate_account_data_payload(data_type, body)?;
        self.user_storage
            .upsert_account_data_content(user_id, data_type, body)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to save account data", &e))?;

        // Invalidate the account-data cache for this user so the next /sync
        // will re-read the fresh data (OPT-015-b, audit 04 §5).
        let _ = self.cache.delete(&format!("account_data:{user_id}")).await;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_account_data(&self, user_id: &str, data_type: &str) -> Result<Option<Value>, ApiError> {
        self.user_storage
            .get_account_data_content(user_id, data_type)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))
    }

    /// Get the set of user IDs that `user_id` has ignored via the
    /// `m.ignored_user_list` account_data event.
    ///
    /// The Matrix spec format is:
    /// ```json
    /// {"ignored_users": {"@bad:server": {}, "@worse:server": {}}}
    /// ```
    ///
    /// Returns an empty set if the user has no `m.ignored_user_list`
    /// account_data, or if the content is malformed (best-effort).
    #[instrument(skip(self))]
    pub async fn get_ignored_users(&self, user_id: &str) -> Result<std::collections::HashSet<String>, ApiError> {
        let Some(content) = self.get_account_data(user_id, "m.ignored_user_list").await? else {
            return Ok(std::collections::HashSet::new());
        };

        let Some(ignored_users) = content.get("ignored_users").and_then(|v| v.as_object()) else {
            return Ok(std::collections::HashSet::new());
        };

        Ok(ignored_users.keys().cloned().collect())
    }

    #[instrument(skip(self))]
    pub async fn delete_account_data(&self, user_id: &str, data_type: &str) -> Result<bool, ApiError> {
        let result = self
            .account_data_storage
            .delete_account_data(user_id, data_type)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete account data", &e))?;

        // Invalidate the account-data cache for this user so the next /sync
        // will re-read the fresh data (OPT-015-b, audit 04 §5).
        let _ = self.cache.delete(&format!("account_data:{user_id}")).await;

        Ok(result)
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
        let now = chrono::Utc::now().timestamp_millis();
        self.room_account_data_storage
            .upsert_room_account_data(user_id, room_id, data_type, body, now)
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
        self.room_account_data_storage.get_room_account_data_content(user_id, room_id, data_type).await
    }

    #[instrument(skip(self))]
    pub async fn get_room_account_data_with_ts(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<Option<AccountDataWithTimestamp>, ApiError> {
        self.room_account_data_storage.get_room_account_data_with_ts(user_id, room_id, data_type).await
    }

    #[instrument(skip(self))]
    pub async fn delete_room_account_data(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<bool, ApiError> {
        self.room_account_data_storage.delete_room_account_data(user_id, room_id, data_type).await
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

    // Validate the shape of `m.ignored_user_list` per the Matrix spec:
    // content MUST be an object with an `ignored_users` object whose keys
    // are Matrix user IDs.
    if data_type == "m.ignored_user_list" {
        let Some(obj) = body.as_object() else {
            return Err(ApiError::bad_request("m.ignored_user_list content must be a JSON object".to_string()));
        };
        let Some(ignored_users) = obj.get("ignored_users") else {
            return Err(ApiError::bad_request("m.ignored_user_list content must contain 'ignored_users'".to_string()));
        };
        let Some(users_map) = ignored_users.as_object() else {
            return Err(ApiError::bad_request("'ignored_users' must be a JSON object".to_string()));
        };
        // Keys must look like Matrix user IDs (`@localpart:server`).
        for key in users_map.keys() {
            if !key.starts_with('@') || !key.contains(':') {
                return Err(ApiError::bad_request(format!(
                    "Invalid user ID in ignored_users: '{key}' (must be @localpart:server)"
                )));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use synapse_storage::test_mocks::{
        shared_fake_user_store, InMemoryAccountDataStore, InMemoryFilterStore, InMemoryOpenIdTokenStore,
        InMemoryRoomAccountDataStore,
    };
    use synapse_storage::{
        account_data::AccountDataStoreApi, filter::FilterStoreApi, openid_token::OpenIdTokenStoreApi,
        room_account_data::RoomAccountDataStoreApi, user::UserStore,
    };

    fn make_service() -> AccountDataService {
        let cache = Arc::new(CacheManager::new(&synapse_cache::CacheConfig::default()));
        let account_data: Arc<dyn AccountDataStoreApi> = Arc::new(InMemoryAccountDataStore::new());
        let user_storage: Arc<dyn UserStore> = shared_fake_user_store();
        let room_account_data: Arc<dyn RoomAccountDataStoreApi> = Arc::new(InMemoryRoomAccountDataStore::new());
        let filter: Arc<dyn FilterStoreApi> = Arc::new(InMemoryFilterStore::new());
        let openid_token: Arc<dyn OpenIdTokenStoreApi> = Arc::new(InMemoryOpenIdTokenStore::new());
        AccountDataService::new(cache, account_data, user_storage, room_account_data, filter, openid_token)
    }

    // ── Purely-functional validation tests (existing) ──

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

    // ---------- m.ignored_user_list validation tests ----------

    #[test]
    fn test_validate_ignored_user_list_valid() {
        let body = json!({
            "ignored_users": {
                "@alice:example.com": {},
                "@bob:example.org": {}
            }
        });
        assert!(validate_account_data_payload("m.ignored_user_list", &body).is_ok());
    }

    #[test]
    fn test_validate_ignored_user_list_empty() {
        let body = json!({"ignored_users": {}});
        assert!(validate_account_data_payload("m.ignored_user_list", &body).is_ok());
    }

    #[test]
    fn test_validate_ignored_user_list_not_object() {
        let body = json!(["not", "an", "object"]);
        let result = validate_account_data_payload("m.ignored_user_list", &body);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be a JSON object"));
    }

    #[test]
    fn test_validate_ignored_user_list_missing_field() {
        let body = json!({"foo": "bar"});
        let result = validate_account_data_payload("m.ignored_user_list", &body);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must contain 'ignored_users'"));
    }

    #[test]
    fn test_validate_ignored_user_list_users_not_object() {
        let body = json!({"ignored_users": ["@alice:example.com"]});
        let result = validate_account_data_payload("m.ignored_user_list", &body);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("'ignored_users' must be a JSON object"));
    }

    #[test]
    fn test_validate_ignored_user_list_invalid_user_id() {
        let body = json!({"ignored_users": {"alice:example.com": {}}});
        let result = validate_account_data_payload("m.ignored_user_list", &body);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid user ID"));
    }

    #[test]
    fn test_validate_ignored_user_list_invalid_user_id_no_server() {
        let body = json!({"ignored_users": {"@alice": {}}});
        let result = validate_account_data_payload("m.ignored_user_list", &body);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid user ID"));
    }

    // ========== AccountDataService mock-backed behavioural tests ==========

    // ── Filter round-trip ──

    #[tokio::test]
    async fn create_and_get_filter_roundtrip() {
        let service = make_service();

        let content = json!({"room": {"timeline": {"limit": 50}}});
        let filter_id = service.create_filter("@alice:localhost", content.clone()).await.unwrap();
        assert!(!filter_id.is_empty(), "filter_id should not be empty");

        let retrieved = service.get_filter("@alice:localhost", &filter_id).await.unwrap();
        let saved = retrieved.expect("expected Some after create_filter");
        assert_eq!(saved, content, "round-tripped filter content must match");
    }

    #[tokio::test]
    async fn get_filter_returns_none_for_unknown_id() {
        let service = make_service();

        let result = service.get_filter("@alice:localhost", "nonexistent-filter").await.unwrap();
        assert!(result.is_none(), "unknown filter_id should return None");
    }

    #[tokio::test]
    async fn get_filter_scoped_to_user() {
        let service = make_service();

        let content = json!({"presence": {"types": ["m.presence"]}});
        let filter_id = service.create_filter("@alice:localhost", content).await.unwrap();

        // Different user should not see the filter
        let result = service.get_filter("@bob:localhost", &filter_id).await.unwrap();
        assert!(result.is_none(), "filter should be scoped to the creating user");
    }

    // ── OpenID token round-trip ──

    #[tokio::test]
    async fn create_and_validate_openid_token_roundtrip() {
        let service = make_service();

        let (token, expires_in) = service.create_openid_token("@alice:localhost", None, 3600).await.unwrap();
        assert!(!token.is_empty(), "token should not be empty");
        assert!(expires_in > 0, "expires_in should be positive");

        let validated = service.validate_openid_token(&token).await.unwrap();
        let token_info = validated.expect("freshly created token should validate");
        assert_eq!(token_info.user_id, "@alice:localhost");
        assert!(token_info.is_valid, "freshly created token should be valid");
    }

    #[tokio::test]
    async fn validate_returns_none_for_unknown_token() {
        let service = make_service();

        let result = service.validate_openid_token("nonexistent-token").await.unwrap();
        assert!(result.is_none(), "unknown token should return None");
    }

    #[tokio::test]
    async fn immediately_expired_openid_token_not_valid() {
        let service = make_service();

        // expires_in_seconds=0 means expires_at = now, which is already past/equal
        // when validate runs its own now() call.
        let (token, _) = service.create_openid_token("@alice:localhost", None, 0).await.unwrap();

        let validated = service.validate_openid_token(&token).await.unwrap();
        assert!(validated.is_none(), "token with 0-second expiry should not pass validation");
    }

    // ── Room account data round-trip ──

    #[tokio::test]
    async fn set_and_get_room_account_data_roundtrip() {
        let service = make_service();
        let room_id = "!room1:localhost";

        let payload = json!({"tags": {"m.favourite": {"order": 0.5}}});
        service.set_room_account_data("@alice:localhost", room_id, "m.tag", &payload).await.unwrap();

        let retrieved = service.get_room_account_data("@alice:localhost", room_id, "m.tag").await.unwrap();
        let saved = retrieved.expect("expected Some after set_room_account_data");
        assert_eq!(saved, payload, "round-tripped room account data content must match");
    }

    #[tokio::test]
    async fn get_room_account_data_returns_none_for_wrong_type() {
        let service = make_service();
        let room_id = "!room2:localhost";

        service.set_room_account_data("@alice:localhost", room_id, "m.tag", &json!({"key": "val"})).await.unwrap();

        let result = service.get_room_account_data("@alice:localhost", room_id, "m.other_type").await.unwrap();
        assert!(result.is_none(), "wrong data_type should return None");
    }

    #[tokio::test]
    async fn delete_room_account_data_removes_it() {
        let service = make_service();
        let room_id = "!room3:localhost";
        let user_id = "@alice:localhost";

        service.set_room_account_data(user_id, room_id, "m.tag", &json!({"k": "v"})).await.unwrap();

        let deleted = service.delete_room_account_data(user_id, room_id, "m.tag").await.unwrap();
        assert!(deleted, "delete should return true for existing data");

        let after = service.get_room_account_data(user_id, room_id, "m.tag").await.unwrap();
        assert!(after.is_none(), "expected None after delete");
    }

    #[tokio::test]
    async fn room_account_data_uses_millis() {
        let service = make_service();

        let before = chrono::Utc::now().timestamp_millis();
        service
            .set_room_account_data("@a:localhost", "!r:localhost", "m.tag", &json!({}))
            .await
            .unwrap();

        let stored = service
            .get_room_account_data_with_ts("@a:localhost", "!r:localhost", "m.tag")
            .await
            .unwrap()
            .expect("expected Some after set_room_account_data");
        let ts = stored.1.expect("expected a stored timestamp");

        assert!(
            ts >= before,
            "expected a millis timestamp (>= {before}), got {ts} (seconds would be ~1000x smaller)"
        );
    }

    // ── Cache invalidation tests (OPT-015-b, audit 04 §5) ──

    /// Builds an [`AccountDataService`] with a shared in-memory cache and
    /// in-memory fakes so we can verify cache invalidation without a database.
    fn make_service_with_cache(cache: Arc<CacheManager>) -> AccountDataService {
        let account_data: Arc<dyn AccountDataStoreApi> = Arc::new(InMemoryAccountDataStore::new());
        let user_storage: Arc<dyn UserStore> = shared_fake_user_store();
        let room_account_data: Arc<dyn RoomAccountDataStoreApi> = Arc::new(InMemoryRoomAccountDataStore::new());
        let filter: Arc<dyn FilterStoreApi> = Arc::new(InMemoryFilterStore::new());
        let openid_token: Arc<dyn OpenIdTokenStoreApi> = Arc::new(InMemoryOpenIdTokenStore::new());
        AccountDataService::new(cache, account_data, user_storage, room_account_data, filter, openid_token)
    }

    #[tokio::test]
    async fn set_account_data_invalidates_cache() {
        let cache = Arc::new(CacheManager::new(&synapse_cache::CacheConfig::default()));
        let service = make_service_with_cache(cache.clone());

        let cache_key = "account_data:@u:hs";
        let seeded = Vec::<serde_json::Value>::new();
        cache.set(cache_key, &seeded, 600).await.expect("pre-seed cache");

        // Before: cache should have the value
        let before = cache.get::<Vec<serde_json::Value>>(cache_key).await.expect("cache get");
        assert!(before.is_some(), "cache should be pre-seeded");

        // Write account data — MUST invalidate the cache
        service
            .set_account_data("@u:hs", "m.direct", &json!({"@bob:hs": ["!r:hs"]}))
            .await
            .expect("set_account_data");

        let after = cache.get::<Vec<serde_json::Value>>(cache_key).await.expect("cache get after set");
        assert!(
            after.is_none(),
            "cache must be invalidated after set_account_data, but still has a value"
        );
    }

    #[tokio::test]
    async fn delete_account_data_invalidates_cache() {
        let cache = Arc::new(CacheManager::new(&synapse_cache::CacheConfig::default()));
        let service = make_service_with_cache(cache.clone());

        // Pre-seed some account data so the delete succeeds
        service
            .set_account_data("@u:hs", "m.direct", &json!({"@bob:hs": ["!r:hs"]}))
            .await
            .expect("seed account data");

        let cache_key = "account_data:@u:hs";
        let seeded = Vec::<serde_json::Value>::new();
        cache.set(cache_key, &seeded, 600).await.expect("pre-seed cache");

        // Before: cache should have the value
        let before = cache.get::<Vec<serde_json::Value>>(cache_key).await.expect("cache get");
        assert!(before.is_some(), "cache should be pre-seeded");

        // Delete account data — MUST invalidate the cache
        service
            .delete_account_data("@u:hs", "m.direct")
            .await
            .expect("delete_account_data");

        let after = cache.get::<Vec<serde_json::Value>>(cache_key).await.expect("cache get after delete");
        assert!(
            after.is_none(),
            "cache must be invalidated after delete_account_data, but still has a value"
        );
    }
}
