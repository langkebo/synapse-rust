//! One-time invite links (B-9).
//!
//! A user mints a token that can be claimed exactly once to create (or join)
//! a room. Only the SHA-256 hash of the token is persisted, so a database dump
//! cannot be replayed. Claiming is a single conditional `UPDATE` in the storage
//! layer, which makes the single-use property hold under concurrency.
//!
//! The room is *not* created at mint time: the invite stores the future
//! [`CreateRoomConfig`] and the room materialises when the first claimer
//! arrives, so an unclaimed invite costs nothing beyond one row.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;

use synapse_common::{current_timestamp_millis, ApiError, ApiResult};
use synapse_storage::one_time_invite::{ClaimOutcome, OneTimeInviteStorage};

use crate::room::service::CreateRoomConfig;
use crate::room::RoomService;

/// Default lifetime of a one-time invite: 7 days.
pub const ONE_TIME_INVITE_DEFAULT_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// Shortest accepted lifetime; anything below is raised to this value.
pub const ONE_TIME_INVITE_MIN_TTL_MS: i64 = 60 * 1000;
/// Longest accepted lifetime; anything above is clamped to this value so an
/// invite cannot outlive the room-configuration assumptions baked into it.
pub const ONE_TIME_INVITE_MAX_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Request body for [`OneTimeInviteService::create`].
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CreateOneTimeInviteRequest {
    /// Requested lifetime in milliseconds, clamped to
    /// [`ONE_TIME_INVITE_MIN_TTL_MS`]..=[`ONE_TIME_INVITE_MAX_TTL_MS`].
    pub ttl_ms: Option<i64>,
    /// Room configuration applied when the token is claimed. Defaults to an
    /// empty config, which the service turns into a private chat.
    pub room_config: Option<CreateRoomConfig>,
}

/// Request body for [`OneTimeInviteService::claim`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimOneTimeInviteRequest {
    /// The plaintext token handed out by [`OneTimeInviteService::create`].
    pub token: String,
}

/// Mints and redeems one-time invite tokens.
pub struct OneTimeInviteService {
    storage: Arc<OneTimeInviteStorage>,
    room_service: Arc<RoomService>,
    server_name: String,
}

impl OneTimeInviteService {
    /// Wire the service to its storage, room factory and public server name
    /// (the latter is used to build the shareable deep link).
    pub fn new(
        storage: Arc<OneTimeInviteStorage>,
        room_service: Arc<RoomService>,
        server_name: impl Into<String>,
    ) -> Self {
        Self { storage, room_service, server_name: server_name.into() }
    }

    /// Hash a plaintext token for storage. `hex` is not a dependency of this
    /// crate, so the digest is base64url-encoded instead of hex-encoded; both
    /// are fixed-length and collision-resistant.
    fn hash_token(token: &str) -> String {
        URL_SAFE_NO_PAD.encode(Sha256::digest(token.as_bytes()))
    }

    /// Mint an invite token for `creator`.
    ///
    /// Returns the plaintext token exactly once — it is not recoverable later,
    /// because only its hash is stored.
    pub async fn create(&self, creator: &str, request: CreateOneTimeInviteRequest) -> ApiResult<Value> {
        let ttl_ms = request
            .ttl_ms
            .unwrap_or(ONE_TIME_INVITE_DEFAULT_TTL_MS)
            .clamp(ONE_TIME_INVITE_MIN_TTL_MS, ONE_TIME_INVITE_MAX_TTL_MS);
        let expires_at = current_timestamp_millis().saturating_add(ttl_ms);
        let config = request.room_config.unwrap_or_default();
        let room_config = serde_json::to_value(&config)
            .map_err(|e| ApiError::internal_with_context("serialize one-time invite room config", &e))?;

        let mut token_bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut token_bytes);
        let token = URL_SAFE_NO_PAD.encode(token_bytes);
        let token_hash = Self::hash_token(&token);

        self.storage
            .create(&token_hash, creator, &room_config, expires_at)
            .await
            .map_err(|e| ApiError::internal_with_context("persist one-time invite", &e))?;

        Ok(json!({
            "token": token,
            "expires_at": expires_at,
            "deep_link": format!(
                "https://{}/_matrix/vendor/v1/invites/claim?token={}",
                self.server_name, token
            ),
        }))
    }

    /// Redeem `token` on behalf of `claimer`, creating the deferred room.
    ///
    /// If room creation fails the claim is released, so a transient failure
    /// does not burn the invite.
    pub async fn claim(&self, claimer: &str, token: &str) -> ApiResult<Value> {
        let token_hash = Self::hash_token(token);

        let claimed = match self
            .storage
            .claim(&token_hash, claimer)
            .await
            .map_err(|e| ApiError::internal_with_context("claim one-time invite", &e))?
        {
            ClaimOutcome::Claimed(claimed) => claimed,
            ClaimOutcome::NotFound => return Err(ApiError::not_found("Invite token not found")),
            ClaimOutcome::Expired => return Err(ApiError::bad_request("Invite token has expired")),
            ClaimOutcome::AlreadyClaimed => return Err(ApiError::forbidden("Invite token has already been used")),
        };

        // A config written by an older server version may not deserialize after
        // a schema change; falling back to defaults still yields a usable room
        // rather than failing the claim with an opaque error.
        let mut config: CreateRoomConfig = serde_json::from_value(claimed.room_config).unwrap_or_default();
        if config.preset.is_none() {
            config.preset = Some("private_chat".to_string());
        }
        let invite_list = config.invite_list.get_or_insert_with(Vec::new);
        if !invite_list.iter().any(|user_id| user_id == claimer) {
            invite_list.push(claimer.to_string());
        }

        match self.room_service.lifecycle.create_room(&claimed.creator, config).await {
            Ok(result) => Ok(json!({
                "room_id": result.get("room_id").cloned().unwrap_or(Value::Null),
                "room_alias": result.get("room_alias").cloned().unwrap_or(Value::Null),
            })),
            Err(error) => {
                if let Err(release_error) = self.storage.release_claim(&token_hash, claimer).await {
                    tracing::error!(
                        error = %release_error,
                        "failed to release one-time invite claim after room creation failure"
                    );
                }
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_is_stable_and_base64url() {
        let first = OneTimeInviteService::hash_token("token-value");
        let second = OneTimeInviteService::hash_token("token-value");
        assert_eq!(first, second);
        assert_eq!(first.len(), 43, "32-byte SHA-256 digest encodes to 43 base64url chars");
        assert!(!first.contains(['+', '/', '=']), "hash must be URL-safe without padding");
        assert_ne!(first, OneTimeInviteService::hash_token("token-valuE"));
    }

    #[test]
    fn create_request_defaults_to_empty_config() {
        let request: CreateOneTimeInviteRequest = serde_json::from_str("{}").expect("empty body");
        assert!(request.ttl_ms.is_none());
        assert!(request.room_config.is_none());
    }

    #[test]
    fn claim_request_rejects_unknown_fields() {
        let error = serde_json::from_str::<ClaimOneTimeInviteRequest>(r#"{"token":"a","extra":1}"#)
            .expect_err("unknown fields must be rejected");
        assert!(error.to_string().contains("unknown field"), "unexpected error: {error}");
    }
}
