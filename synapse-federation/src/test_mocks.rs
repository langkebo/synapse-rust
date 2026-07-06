//! Pre-positioned Mock adapter for the federation layer.
//!
//! [`MockFederationClient`] is an in-memory test double that implements
//! [`crate::client_api::FederationClientApi`] without performing any
//! HTTP I/O. Engineers can pre-seed responses for specific remote servers
//! and assert outbound calls.
//!
//! # Strategy
//!
//! With the [`FederationClientApi`] trait extracted (FED-1..3), production
//! code accepts `Arc<dyn FederationClientApi>` and tests inject
//! `MockFederationClient`. This is the same seam pattern used for storage
//! traits (EventStoreApi, RoomStoreApi, etc.).

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::client::{
    BackfillResponse, DirectoryResponse, EventResponse, FederationClientError, FederationTransaction, InviteResponse,
    MakeJoinResponse, MakeLeaveResponse, ProfileResponse, ResolvedServer, SendJoinResponse, SendLeaveResponse,
    ServerKeys, StateIdsResponse, StateResponse, UserDevicesResponse, VersionResponse,
};
use crate::key_rotation::{KeyRotationManagerApi, SigningKey};
use synapse_common::ApiError;

/// In-memory federation client double.
///
/// Stores pre-seeded responses keyed by room_id (or server_name for key
/// lookups). Outbound transactions are recorded in `sent_transactions` for
/// assertion in tests.
#[derive(Debug, Default, Clone)]
pub struct MockFederationClient {
    server_name: String,
    server_keys: Arc<RwLock<HashMap<String, ServerKeys>>>,
    sent_transactions: Arc<RwLock<Vec<FederationTransaction>>>,
    make_join_responses: Arc<RwLock<HashMap<String, MakeJoinResponse>>>,
    send_join_responses: Arc<RwLock<HashMap<String, SendJoinResponse>>>,
    make_leave_responses: Arc<RwLock<HashMap<String, MakeLeaveResponse>>>,
    send_leave_responses: Arc<RwLock<HashMap<String, SendLeaveResponse>>>,
    invite_responses: Arc<RwLock<HashMap<String, InviteResponse>>>,
    backfill_responses: Arc<RwLock<HashMap<String, BackfillResponse>>>,
}

impl MockFederationClient {
    pub fn new(server_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            server_keys: Arc::new(RwLock::new(HashMap::new())),
            sent_transactions: Arc::new(RwLock::new(Vec::new())),
            make_join_responses: Arc::new(RwLock::new(HashMap::new())),
            send_join_responses: Arc::new(RwLock::new(HashMap::new())),
            make_leave_responses: Arc::new(RwLock::new(HashMap::new())),
            send_leave_responses: Arc::new(RwLock::new(HashMap::new())),
            invite_responses: Arc::new(RwLock::new(HashMap::new())),
            backfill_responses: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // ── Seeding API ───────────────────────────────────────────────────

    pub async fn seed_server_keys(&self, server_name: impl Into<String>, keys: ServerKeys) {
        self.server_keys.write().await.insert(server_name.into(), keys);
    }

    pub async fn seed_make_join(&self, room_id: impl Into<String>, response: MakeJoinResponse) {
        self.make_join_responses.write().await.insert(room_id.into(), response);
    }

    pub async fn seed_send_join(&self, room_id: impl Into<String>, response: SendJoinResponse) {
        self.send_join_responses.write().await.insert(room_id.into(), response);
    }

    pub async fn seed_make_leave(&self, room_id: impl Into<String>, response: MakeLeaveResponse) {
        self.make_leave_responses.write().await.insert(room_id.into(), response);
    }

    pub async fn seed_send_leave(&self, room_id: impl Into<String>, response: SendLeaveResponse) {
        self.send_leave_responses.write().await.insert(room_id.into(), response);
    }

    pub async fn seed_invite(&self, room_id: impl Into<String>, response: InviteResponse) {
        self.invite_responses.write().await.insert(room_id.into(), response);
    }

    pub async fn seed_backfill(&self, room_id: impl Into<String>, response: BackfillResponse) {
        self.backfill_responses.write().await.insert(room_id.into(), response);
    }

    /// Record an outbound PDU/EDU transaction (call from production code path
    /// under test, or directly from test setup to assert outbound behaviour).
    pub async fn record_transaction(&self, transaction: FederationTransaction) {
        self.sent_transactions.write().await.push(transaction);
    }

    /// Snapshot of all outbound transactions recorded so far.
    pub async fn sent_transactions(&self) -> Vec<FederationTransaction> {
        self.sent_transactions.read().await.clone()
    }
}

// ============================================================================
// FederationClientApi impl
// ============================================================================

#[async_trait::async_trait]
impl crate::client_api::FederationClientApi for MockFederationClient {
    fn server_name(&self) -> &str {
        &self.server_name
    }

    async fn resolve_server(&self, _server_name: &str) -> Result<ResolvedServer, FederationClientError> {
        Err(FederationClientError::DiscoveryFailed("mock: resolve_server not configured".into()))
    }

    async fn get_server_keys(&self, destination: &str) -> Result<ServerKeys, FederationClientError> {
        self.server_keys
            .read()
            .await
            .get(destination)
            .cloned()
            .ok_or_else(|| FederationClientError::InvalidResponse(format!("server keys not seeded for {destination}")))
    }

    async fn query_server_keys(
        &self,
        _destination: &str,
        _server_name: &str,
        _key_id: Option<&str>,
    ) -> Result<ServerKeys, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: query_server_keys not configured".into()))
    }

    async fn get_version(&self, _destination: &str) -> Result<VersionResponse, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: get_version not configured".into()))
    }

    async fn send_transaction(
        &self,
        _destination: &str,
        transaction: &FederationTransaction,
    ) -> Result<serde_json::Value, FederationClientError> {
        self.sent_transactions.write().await.push(transaction.clone());
        Ok(serde_json::json!({}))
    }

    async fn make_join(
        &self,
        _destination: &str,
        room_id: &str,
        _user_id: &str,
    ) -> Result<MakeJoinResponse, FederationClientError> {
        self.make_join_responses
            .read()
            .await
            .get(room_id)
            .cloned()
            .ok_or_else(|| FederationClientError::InvalidResponse(format!("make_join not seeded for {room_id}")))
    }

    async fn send_join(
        &self,
        _destination: &str,
        room_id: &str,
        _event_id: &str,
        _event: &serde_json::Value,
    ) -> Result<SendJoinResponse, FederationClientError> {
        self.send_join_responses
            .read()
            .await
            .get(room_id)
            .cloned()
            .ok_or_else(|| FederationClientError::InvalidResponse(format!("send_join not seeded for {room_id}")))
    }

    async fn make_leave(
        &self,
        _destination: &str,
        room_id: &str,
        _user_id: &str,
    ) -> Result<MakeLeaveResponse, FederationClientError> {
        self.make_leave_responses
            .read()
            .await
            .get(room_id)
            .cloned()
            .ok_or_else(|| FederationClientError::InvalidResponse(format!("make_leave not seeded for {room_id}")))
    }

    async fn send_leave(
        &self,
        _destination: &str,
        room_id: &str,
        _event_id: &str,
        _event: &serde_json::Value,
    ) -> Result<SendLeaveResponse, FederationClientError> {
        self.send_leave_responses
            .read()
            .await
            .get(room_id)
            .cloned()
            .ok_or_else(|| FederationClientError::InvalidResponse(format!("send_leave not seeded for {room_id}")))
    }

    async fn invite(
        &self,
        _destination: &str,
        room_id: &str,
        _event_id: &str,
        _event: &serde_json::Value,
    ) -> Result<InviteResponse, FederationClientError> {
        self.invite_responses
            .read()
            .await
            .get(room_id)
            .cloned()
            .ok_or_else(|| FederationClientError::InvalidResponse(format!("invite not seeded for {room_id}")))
    }

    async fn get_event(&self, _destination: &str, _event_id: &str) -> Result<EventResponse, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: get_event not configured".into()))
    }

    async fn get_state(&self, _destination: &str, _room_id: &str) -> Result<StateResponse, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: get_state not configured".into()))
    }

    async fn get_state_ids(
        &self,
        _destination: &str,
        _room_id: &str,
    ) -> Result<StateIdsResponse, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: get_state_ids not configured".into()))
    }

    async fn backfill(
        &self,
        _destination: &str,
        room_id: &str,
        _event_ids: &[String],
        _limit: u32,
    ) -> Result<BackfillResponse, FederationClientError> {
        self.backfill_responses
            .read()
            .await
            .get(room_id)
            .cloned()
            .ok_or_else(|| FederationClientError::InvalidResponse(format!("backfill not seeded for {room_id}")))
    }

    async fn get_missing_events(
        &self,
        _destination: &str,
        _room_id: &str,
        _earliest_events: &[String],
        _latest_events: &[String],
        _limit: u32,
        _min_depth: Option<i64>,
    ) -> Result<serde_json::Value, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: get_missing_events not configured".into()))
    }

    async fn get_event_auth(
        &self,
        _destination: &str,
        _room_id: &str,
        _event_id: &str,
    ) -> Result<serde_json::Value, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: get_event_auth not configured".into()))
    }

    async fn get_user_devices(
        &self,
        _destination: &str,
        _user_id: &str,
    ) -> Result<UserDevicesResponse, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: get_user_devices not configured".into()))
    }

    async fn query_profile(
        &self,
        _destination: &str,
        _user_id: &str,
    ) -> Result<ProfileResponse, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: query_profile not configured".into()))
    }

    async fn query_directory(
        &self,
        _destination: &str,
        _room_alias: &str,
    ) -> Result<DirectoryResponse, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: query_directory not configured".into()))
    }

    async fn claim_keys(
        &self,
        _destination: &str,
        _claims: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: claim_keys not configured".into()))
    }

    async fn query_keys(
        &self,
        _destination: &str,
        _query: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: query_keys not configured".into()))
    }

    async fn timestamp_to_event(
        &self,
        _destination: &str,
        _room_id: &str,
        _timestamp: i64,
        _direction: &str,
    ) -> Result<serde_json::Value, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: timestamp_to_event not configured".into()))
    }

    async fn get_public_rooms(
        &self,
        _destination: &str,
        _limit: Option<u32>,
        _since: Option<&str>,
    ) -> Result<serde_json::Value, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: get_public_rooms not configured".into()))
    }

    async fn knock_room(
        &self,
        _destination: &str,
        _room_id: &str,
        _user_id: &str,
        _event: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: knock_room not configured".into()))
    }

    async fn exchange_third_party_invite(
        &self,
        _destination: &str,
        _room_id: &str,
        _event: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: exchange_third_party_invite not configured".into()))
    }

    async fn media_download(
        &self,
        _destination: &str,
        _server_name: &str,
        _media_id: &str,
    ) -> Result<reqwest::Response, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: media_download not configured".into()))
    }

    async fn media_thumbnail(
        &self,
        _destination: &str,
        _server_name: &str,
        _media_id: &str,
        _width: u32,
        _height: u32,
        _method: &str,
    ) -> Result<reqwest::Response, FederationClientError> {
        Err(FederationClientError::InvalidResponse("mock: media_thumbnail not configured".into()))
    }

    async fn get_cached_key(&self, _server_name: &str) -> Option<ServerKeys> {
        None
    }

    async fn health_check(&self, _destination: &str) -> bool {
        false
    }
}

// ============================================================================
// InMemoryKeyRotationManager
// ============================================================================

/// In-memory test double for [`KeyRotationManagerApi`].
///
/// Stores a configurable current key and rotation state. Tests can seed
/// a signing key and control whether rotation is enabled.
#[derive(Clone)]
pub struct InMemoryKeyRotationManager {
    rotation_enabled: Arc<RwLock<bool>>,
    current_key: Arc<RwLock<Option<SigningKey>>>,
    config: Arc<RwLock<HashMap<String, String>>>,
    rotation_status: Arc<RwLock<serde_json::Value>>,
}

impl InMemoryKeyRotationManager {
    pub fn new() -> Self {
        Self {
            rotation_enabled: Arc::new(RwLock::new(true)),
            current_key: Arc::new(RwLock::new(None)),
            config: Arc::new(RwLock::new(HashMap::new())),
            rotation_status: Arc::new(RwLock::new(serde_json::json!({
                "rotation_enabled": true,
                "has_current_key": false,
                "should_rotate": true,
                "server_name": "test.example.com",
                "rotation_interval_days": 7,
                "rotation_threshold_days": 1,
                "grace_period_minutes": 5
            }))),
        }
    }

    pub async fn seed_current_key(&self, key: SigningKey) {
        *self.current_key.write().await = Some(key);
    }

    pub async fn set_rotation_enabled_state(&self, enabled: bool) {
        *self.rotation_enabled.write().await = enabled;
    }
}

impl Default for InMemoryKeyRotationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl KeyRotationManagerApi for InMemoryKeyRotationManager {
    async fn get_rotation_status(&self) -> serde_json::Value {
        self.rotation_status.read().await.clone()
    }

    async fn rotate_keys(&self, _requested_key_id: Option<String>) -> Result<(), ApiError> {
        let key_id = format!("ed25519:{}", chrono::Utc::now().timestamp_millis());
        let new_key = SigningKey {
            server_name: "test.example.com".into(),
            key_id,
            secret_key: "test-secret".into(),
            public_key: "test-public".into(),
            created_ts: chrono::Utc::now().timestamp_millis(),
            expires_at: chrono::Utc::now().timestamp_millis() + 7 * 24 * 3600 * 1000,
            key_json: serde_json::json!({"public_key": "test-public"}),
            ts_added_ms: chrono::Utc::now().timestamp_millis(),
            ts_valid_until_ms: chrono::Utc::now().timestamp_millis() + 7 * 24 * 3600 * 1000,
        };
        *self.current_key.write().await = Some(new_key);
        Ok(())
    }

    async fn get_current_key(&self) -> Result<Option<SigningKey>, ApiError> {
        Ok(self.current_key.read().await.clone())
    }

    async fn revoke_key(
        &self,
        _key_id: &str,
        _reason: Option<&str>,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        *self.current_key.write().await = None;
        Ok(1)
    }

    async fn set_rotation_enabled(&self, enabled: bool) {
        *self.rotation_enabled.write().await = enabled;
    }

    async fn set_rotation_config_value(&self, key: &str, value: &str) -> Result<(), ApiError> {
        self.config.write().await.insert(key.to_string(), value.to_string());
        Ok(())
    }
}

// ============================================================================
// Extension TODOs (tracked in 执行清单 Phase 3)
// ============================================================================

// TODO(FED-4): Update call sites that take `FederationClient` to take `Arc<dyn FederationClientApi>`.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_api::FederationClientApi;

    #[tokio::test]
    async fn seeded_server_keys_round_trip() {
        let client = MockFederationClient::new("example.com");
        let keys = ServerKeys {
            server_name: "remote.example.com".to_string(),
            verify_keys: serde_json::json!({}),
            old_verify_keys: serde_json::json!({}),
            signatures: serde_json::json!({}),
            valid_until_ts: 1_700_000_000_000,
        };
        client.seed_server_keys("remote.example.com", keys.clone()).await;

        let fetched = client.get_server_keys("remote.example.com").await;
        assert_eq!(fetched.unwrap().server_name, "remote.example.com");
        assert!(client.get_server_keys("unknown.example.com").await.is_err());
    }

    #[tokio::test]
    async fn sent_transactions_recorded_in_order() {
        let client = MockFederationClient::new("example.com");
        let tx = FederationTransaction {
            transaction_id: "t1".to_string(),
            origin: "example.com".to_string(),
            origin_server_ts: 1,
            destination: "remote.example.com".to_string(),
            pdus: Vec::new(),
            edus: Vec::new(),
        };
        client.send_transaction("remote.example.com", &tx).await.unwrap();

        let sent = client.sent_transactions().await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].transaction_id, "t1");
    }

    #[test]
    fn mock_client_implements_trait() {
        fn _assert<T: FederationClientApi + ?Sized>() {}
        _assert::<MockFederationClient>();
    }

    #[tokio::test]
    async fn mock_trait_object_returns_seeded_data() {
        let mock = MockFederationClient::new("local.test");
        let keys = ServerKeys {
            server_name: "remote.test".to_string(),
            verify_keys: serde_json::json!({}),
            old_verify_keys: serde_json::json!({}),
            signatures: serde_json::json!({}),
            valid_until_ts: 1_700_000_000_000,
        };
        mock.seed_server_keys("remote.test", keys.clone()).await;

        let api: std::sync::Arc<dyn FederationClientApi> = std::sync::Arc::new(mock);
        let result = api.get_server_keys("remote.test").await;
        assert_eq!(result.unwrap().server_name, "remote.test");

        let missing = api.get_server_keys("unknown.test").await;
        assert!(missing.is_err());
    }
}
