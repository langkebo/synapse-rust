//! Per-room and global invite blocklist / allowlist policy.
//!
//! Wraps `InviteBlocklistStorage` with `ApiError` mapping so the invite and
//! admin-server routes do not hold a storage handle (B4-5c).

use std::sync::Arc;
use synapse_common::error::ApiError;
use synapse_storage::invite_blocklist::InviteBlocklistStorage;

/// Service over the invite blocklist / allowlist tables.
pub struct InviteBlocklistService {
    storage: Arc<InviteBlocklistStorage>,
}

impl InviteBlocklistService {
    /// See [`new`].
    pub fn new(storage: Arc<InviteBlocklistStorage>) -> Self {
        Self { storage }
    }

    /// Replace the room's invite blocklist.
    pub async fn set_invite_blocklist(&self, room_id: &str, user_ids: Vec<String>) -> Result<(), ApiError> {
        self.storage
            .set_invite_blocklist(room_id, user_ids)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to set blocklist", e))
    }

    /// Read the room's invite blocklist.
    pub async fn get_invite_blocklist(&self, room_id: &str) -> Result<Vec<String>, ApiError> {
        self.storage
            .get_invite_blocklist(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get blocklist", e))
    }

    /// Replace the room's invite allowlist.
    pub async fn set_invite_allowlist(&self, room_id: &str, user_ids: Vec<String>) -> Result<(), ApiError> {
        self.storage
            .set_invite_allowlist(room_id, user_ids)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to set allowlist", e))
    }

    /// Read the room's invite allowlist.
    pub async fn get_invite_allowlist(&self, room_id: &str) -> Result<Vec<String>, ApiError> {
        self.storage
            .get_invite_allowlist(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get allowlist", e))
    }

    /// Read the server-wide invite blocklist rows.
    pub async fn get_global_invite_blocklist(&self) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage
            .get_global_invite_blocklist()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get global blocklist", e))
    }

    /// Read the server-wide invite allowlist rows.
    pub async fn get_global_invite_allowlist(&self) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage
            .get_global_invite_allowlist()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get global allowlist", e))
    }
}
