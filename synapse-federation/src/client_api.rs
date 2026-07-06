//! FederationClientApi — trait seam for FederationClient (Phase 3 FED-1..3).
//!
//! Extracts the full public surface of [`crate::FederationClient`] behind a
//! trait so callers can accept `Arc<dyn FederationClientApi>` and tests can
//! inject [`crate::test_mocks::MockFederationClient`] without HTTP I/O.
//!
//! See: `.trae/documents/TDD落地执行清单.md` Phase 3 FED-1..5.

use async_trait::async_trait;

use crate::client::{
    BackfillResponse, DirectoryResponse, EventResponse, FederationClient, FederationClientError, FederationTransaction,
    InviteResponse, MakeJoinResponse, MakeLeaveResponse, ProfileResponse, ResolvedServer, SendJoinResponse,
    SendLeaveResponse, ServerKeys, StateIdsResponse, StateResponse, UserDevicesResponse, VersionResponse,
};

/// Trait abstracting federation client operations.
///
/// Implemented by [`FederationClient`] (HTTP-backed) and
/// [`crate::test_mocks::MockFederationClient`] (in-memory).
#[async_trait]
pub trait FederationClientApi: Send + Sync {
    /// The local server name this client acts as.
    fn server_name(&self) -> &str;

    /// Resolve a server name to host/port via well-known or explicit port.
    async fn resolve_server(&self, server_name: &str) -> Result<ResolvedServer, FederationClientError>;

    /// Fetch the published signing keys for a remote server.
    async fn get_server_keys(&self, destination: &str) -> Result<ServerKeys, FederationClientError>;

    /// Query specific server keys by server name and optional key ID.
    async fn query_server_keys(
        &self,
        destination: &str,
        server_name: &str,
        key_id: Option<&str>,
    ) -> Result<ServerKeys, FederationClientError>;

    /// Query the remote server's Matrix version.
    async fn get_version(&self, destination: &str) -> Result<VersionResponse, FederationClientError>;

    /// Send a federation transaction (PDUs + EDUs) to a remote server.
    async fn send_transaction(
        &self,
        destination: &str,
        transaction: &FederationTransaction,
    ) -> Result<serde_json::Value, FederationClientError>;

    /// Request a join template event from a remote server.
    async fn make_join(
        &self,
        destination: &str,
        room_id: &str,
        user_id: &str,
    ) -> Result<MakeJoinResponse, FederationClientError>;

    /// Send a signed join event to a remote server.
    async fn send_join(
        &self,
        destination: &str,
        room_id: &str,
        event_id: &str,
        event: &serde_json::Value,
    ) -> Result<SendJoinResponse, FederationClientError>;

    /// Request a leave template event from a remote server.
    async fn make_leave(
        &self,
        destination: &str,
        room_id: &str,
        user_id: &str,
    ) -> Result<MakeLeaveResponse, FederationClientError>;

    /// Send a signed leave event to a remote server.
    async fn send_leave(
        &self,
        destination: &str,
        room_id: &str,
        event_id: &str,
        event: &serde_json::Value,
    ) -> Result<SendLeaveResponse, FederationClientError>;

    /// Send an invite event to a remote server.
    async fn invite(
        &self,
        destination: &str,
        room_id: &str,
        event_id: &str,
        event: &serde_json::Value,
    ) -> Result<InviteResponse, FederationClientError>;

    /// Fetch a single event from a remote server.
    async fn get_event(&self, destination: &str, event_id: &str) -> Result<EventResponse, FederationClientError>;

    /// Fetch the full current state for a room from a remote server.
    async fn get_state(&self, destination: &str, room_id: &str) -> Result<StateResponse, FederationClientError>;

    /// Fetch only state event IDs for a room from a remote server.
    async fn get_state_ids(&self, destination: &str, room_id: &str) -> Result<StateIdsResponse, FederationClientError>;

    /// Backfill events from a remote server.
    async fn backfill(
        &self,
        destination: &str,
        room_id: &str,
        event_ids: &[String],
        limit: u32,
    ) -> Result<BackfillResponse, FederationClientError>;

    /// Request missing events from a remote server.
    async fn get_missing_events(
        &self,
        destination: &str,
        room_id: &str,
        earliest_events: &[String],
        latest_events: &[String],
        limit: u32,
        min_depth: Option<i64>,
    ) -> Result<serde_json::Value, FederationClientError>;

    /// Fetch the auth chain for an event from a remote server.
    async fn get_event_auth(
        &self,
        destination: &str,
        room_id: &str,
        event_id: &str,
    ) -> Result<serde_json::Value, FederationClientError>;

    /// Query device keys for a user from a remote server.
    async fn get_user_devices(
        &self,
        destination: &str,
        user_id: &str,
    ) -> Result<UserDevicesResponse, FederationClientError>;

    /// Query a user's profile from a remote server.
    async fn query_profile(&self, destination: &str, user_id: &str) -> Result<ProfileResponse, FederationClientError>;

    /// Resolve a room alias to a room ID via a remote server.
    async fn query_directory(
        &self,
        destination: &str,
        room_alias: &str,
    ) -> Result<DirectoryResponse, FederationClientError>;

    /// Claim one-time keys from a remote server.
    async fn claim_keys(
        &self,
        destination: &str,
        claims: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError>;

    /// Query device keys from a remote server.
    async fn query_keys(
        &self,
        destination: &str,
        query: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError>;

    /// Convert a timestamp to an event ID ordering on a remote server.
    async fn timestamp_to_event(
        &self,
        destination: &str,
        room_id: &str,
        timestamp: i64,
        direction: &str,
    ) -> Result<serde_json::Value, FederationClientError>;

    /// Query public rooms from a remote server.
    async fn get_public_rooms(
        &self,
        destination: &str,
        limit: Option<u32>,
        since: Option<&str>,
    ) -> Result<serde_json::Value, FederationClientError>;

    /// Send a knock event to a remote server.
    async fn knock_room(
        &self,
        destination: &str,
        room_id: &str,
        user_id: &str,
        event: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError>;

    /// Exchange a third-party invite via a remote server.
    async fn exchange_third_party_invite(
        &self,
        destination: &str,
        room_id: &str,
        event: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError>;

    /// Download a media file from a remote server.
    async fn media_download(
        &self,
        destination: &str,
        server_name: &str,
        media_id: &str,
    ) -> Result<reqwest::Response, FederationClientError>;

    /// Download a media thumbnail from a remote server.
    async fn media_thumbnail(
        &self,
        destination: &str,
        server_name: &str,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
    ) -> Result<reqwest::Response, FederationClientError>;

    /// Look up a cached server key (non-persistent, TTL-based).
    async fn get_cached_key(&self, server_name: &str) -> Option<ServerKeys>;

    /// Quick server reachability check via GET /version.
    async fn health_check(&self, destination: &str) -> bool;
}

// ============================================================================
// Real implementation — delegates to the existing FederationClient methods.
// ============================================================================

#[async_trait]
impl FederationClientApi for FederationClient {
    fn server_name(&self) -> &str {
        FederationClient::server_name(self)
    }

    async fn resolve_server(&self, server_name: &str) -> Result<ResolvedServer, FederationClientError> {
        FederationClient::resolve_server(self, server_name).await
    }

    async fn get_server_keys(&self, destination: &str) -> Result<ServerKeys, FederationClientError> {
        FederationClient::get_server_keys(self, destination).await
    }

    async fn query_server_keys(
        &self,
        destination: &str,
        server_name: &str,
        key_id: Option<&str>,
    ) -> Result<ServerKeys, FederationClientError> {
        FederationClient::query_server_keys(self, destination, server_name, key_id).await
    }

    async fn get_version(&self, destination: &str) -> Result<VersionResponse, FederationClientError> {
        FederationClient::get_version(self, destination).await
    }

    async fn send_transaction(
        &self,
        destination: &str,
        transaction: &FederationTransaction,
    ) -> Result<serde_json::Value, FederationClientError> {
        FederationClient::send_transaction(self, destination, transaction).await
    }

    async fn make_join(
        &self,
        destination: &str,
        room_id: &str,
        user_id: &str,
    ) -> Result<MakeJoinResponse, FederationClientError> {
        FederationClient::make_join(self, destination, room_id, user_id).await
    }

    async fn send_join(
        &self,
        destination: &str,
        room_id: &str,
        event_id: &str,
        event: &serde_json::Value,
    ) -> Result<SendJoinResponse, FederationClientError> {
        FederationClient::send_join(self, destination, room_id, event_id, event).await
    }

    async fn make_leave(
        &self,
        destination: &str,
        room_id: &str,
        user_id: &str,
    ) -> Result<MakeLeaveResponse, FederationClientError> {
        FederationClient::make_leave(self, destination, room_id, user_id).await
    }

    async fn send_leave(
        &self,
        destination: &str,
        room_id: &str,
        event_id: &str,
        event: &serde_json::Value,
    ) -> Result<SendLeaveResponse, FederationClientError> {
        FederationClient::send_leave(self, destination, room_id, event_id, event).await
    }

    async fn invite(
        &self,
        destination: &str,
        room_id: &str,
        event_id: &str,
        event: &serde_json::Value,
    ) -> Result<InviteResponse, FederationClientError> {
        FederationClient::invite(self, destination, room_id, event_id, event).await
    }

    async fn get_event(&self, destination: &str, event_id: &str) -> Result<EventResponse, FederationClientError> {
        FederationClient::get_event(self, destination, event_id).await
    }

    async fn get_state(&self, destination: &str, room_id: &str) -> Result<StateResponse, FederationClientError> {
        FederationClient::get_state(self, destination, room_id).await
    }

    async fn get_state_ids(&self, destination: &str, room_id: &str) -> Result<StateIdsResponse, FederationClientError> {
        FederationClient::get_state_ids(self, destination, room_id).await
    }

    async fn backfill(
        &self,
        destination: &str,
        room_id: &str,
        event_ids: &[String],
        limit: u32,
    ) -> Result<BackfillResponse, FederationClientError> {
        FederationClient::backfill(self, destination, room_id, event_ids, limit).await
    }

    async fn get_missing_events(
        &self,
        destination: &str,
        room_id: &str,
        earliest_events: &[String],
        latest_events: &[String],
        limit: u32,
        min_depth: Option<i64>,
    ) -> Result<serde_json::Value, FederationClientError> {
        FederationClient::get_missing_events(
            self,
            destination,
            room_id,
            earliest_events,
            latest_events,
            limit,
            min_depth,
        )
        .await
    }

    async fn get_event_auth(
        &self,
        destination: &str,
        room_id: &str,
        event_id: &str,
    ) -> Result<serde_json::Value, FederationClientError> {
        FederationClient::get_event_auth(self, destination, room_id, event_id).await
    }

    async fn get_user_devices(
        &self,
        destination: &str,
        user_id: &str,
    ) -> Result<UserDevicesResponse, FederationClientError> {
        FederationClient::get_user_devices(self, destination, user_id).await
    }

    async fn query_profile(&self, destination: &str, user_id: &str) -> Result<ProfileResponse, FederationClientError> {
        FederationClient::query_profile(self, destination, user_id).await
    }

    async fn query_directory(
        &self,
        destination: &str,
        room_alias: &str,
    ) -> Result<DirectoryResponse, FederationClientError> {
        FederationClient::query_directory(self, destination, room_alias).await
    }

    async fn claim_keys(
        &self,
        destination: &str,
        claims: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        FederationClient::claim_keys(self, destination, claims).await
    }

    async fn query_keys(
        &self,
        destination: &str,
        query: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        FederationClient::query_keys(self, destination, query).await
    }

    async fn timestamp_to_event(
        &self,
        destination: &str,
        room_id: &str,
        timestamp: i64,
        direction: &str,
    ) -> Result<serde_json::Value, FederationClientError> {
        FederationClient::timestamp_to_event(self, destination, room_id, timestamp, direction).await
    }

    async fn get_public_rooms(
        &self,
        destination: &str,
        limit: Option<u32>,
        since: Option<&str>,
    ) -> Result<serde_json::Value, FederationClientError> {
        FederationClient::get_public_rooms(self, destination, limit, since).await
    }

    async fn knock_room(
        &self,
        destination: &str,
        room_id: &str,
        user_id: &str,
        event: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        FederationClient::knock_room(self, destination, room_id, user_id, event).await
    }

    async fn exchange_third_party_invite(
        &self,
        destination: &str,
        room_id: &str,
        event: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        FederationClient::exchange_third_party_invite(self, destination, room_id, event).await
    }

    async fn media_download(
        &self,
        destination: &str,
        server_name: &str,
        media_id: &str,
    ) -> Result<reqwest::Response, FederationClientError> {
        FederationClient::media_download(self, destination, server_name, media_id).await
    }

    async fn media_thumbnail(
        &self,
        destination: &str,
        server_name: &str,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
    ) -> Result<reqwest::Response, FederationClientError> {
        FederationClient::media_thumbnail(self, destination, server_name, media_id, width, height, method).await
    }

    async fn get_cached_key(&self, server_name: &str) -> Option<ServerKeys> {
        FederationClient::get_cached_key(self, server_name).await
    }

    async fn health_check(&self, destination: &str) -> bool {
        FederationClient::health_check(self, destination).await
    }
}

// ============================================================================
// Compile-time test: real impl satisfies the trait.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn real_client_implements_trait() {
        fn _assert<T: FederationClientApi + ?Sized>() {}
        _assert::<FederationClient>();
    }

    #[test]
    fn arc_dyn_trait_object_compiles() {
        fn _accepts_dyn(_client: Arc<dyn FederationClientApi>) {}
        let _f: fn(Arc<dyn FederationClientApi>) = _accepts_dyn;
    }
}
