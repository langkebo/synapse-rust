//! Friend Sync Service Module
//!
//! This service maintains data consistency between the legacy friends table
//! and the new room-based friend system during the dual-operation phase.
//!
//! Phase 2 Strategy: Dual Operation
//! - Both systems run in parallel
//! - Sync service ensures data consistency
//! - New API endpoints prefer friend rooms, fallback to legacy

use crate::services::{FriendStorage, RegistrationService, UserStorage};
use crate::services::friend_room_service::FriendRoomService;
use crate::storage::friend_room::FriendInfo;
use serde_json::json;
use std::sync::Arc;

/// Configuration for the friend sync service
#[derive(Clone, Debug)]
pub struct FriendSyncConfig {
    /// Whether to enable dual-mode operation (sync between old and new systems)
    pub enable_dual_mode: bool,
    /// Whether to prefer friend rooms over legacy friends table
    pub prefer_friend_rooms: bool,
    /// Whether to automatically migrate users to friend rooms on access
    pub auto_migrate_on_access: bool,
}

impl Default for FriendSyncConfig {
    fn default() -> Self {
        Self {
            enable_dual_mode: true,
            prefer_friend_rooms: true,
            auto_migrate_on_access: true,
        }
    }
}

/// Synchronization status between systems
#[derive(Debug, Clone)]
pub struct SyncStatus {
    /// Whether the friend list room exists
    pub friend_room_exists: bool,
    /// Whether the legacy friends table has data
    pub legacy_data_exists: bool,
    /// Whether the systems are in sync
    pub is_synced: bool,
    /// Number of friends in friend room
    pub friend_room_count: usize,
    /// Number of friends in legacy table
    pub legacy_count: usize,
}

/// Result type for sync operations
pub type SyncResult<T> = Result<T, SyncError>;

/// Errors that can occur during synchronization
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Friend room error: {0}")]
    FriendRoom(String),

    #[error("Legacy friends error: {0}")]
    LegacyFriends(String),

    #[error("Sync conflict: {0}")]
    Conflict(String),
}

impl From<sqlx::Error> for SyncError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e.to_string())
    }
}

/// Friend Sync Service
///
/// Maintains consistency between legacy friends table and new friend room system.
pub struct FriendSyncService {
    /// Legacy friend storage
    pub legacy_storage: FriendStorage,
    /// New friend room service
    pub friend_room_service: FriendRoomService,
    /// User storage for profile lookups
    pub user_storage: UserStorage,
    /// Registration service for profile data
    pub registration_service: Arc<RegistrationService>,
    /// Sync configuration
    pub config: FriendSyncConfig,
}

impl FriendSyncService {
    /// Create a new FriendSyncService
    pub fn new(
        legacy_storage: FriendStorage,
        friend_room_service: FriendRoomService,
        user_storage: UserStorage,
        registration_service: Arc<RegistrationService>,
        config: FriendSyncConfig,
    ) -> Self {
        Self {
            legacy_storage,
            friend_room_service,
            user_storage,
            registration_service,
            config,
        }
    }

    // ========================================================================
    // Synchronization Operations
    // ========================================================================

    /// Get the sync status for a user
    pub async fn get_sync_status(&self, user_id: &str) -> SyncResult<SyncStatus> {
        // Check if friend room exists
        let friend_room_exists = self
            .friend_room_service
            .storage
            .friend_list_room_exists(user_id)
            .await
            .map_err(|e| SyncError::FriendRoom(e.to_string()))?;

        // Get friend counts from both systems
        let (friend_room_count, legacy_count) = if friend_room_exists {
            let list = self
                .friend_room_service
                .storage
                .get_friend_list(user_id)
                .await
                .map_err(|e| SyncError::FriendRoom(e.to_string()))?;
            let legacy = self
                .legacy_storage
                .get_friends(user_id)
                .await
                .map_err(|e| SyncError::LegacyFriends(e.to_string()))?;

            (list.friends.len(), legacy.len())
        } else {
            let legacy = self
                .legacy_storage
                .get_friends(user_id)
                .await
                .unwrap_or_default();
            (0, legacy.len())
        };

        let legacy_data_exists = legacy_count > 0;
        let is_synced = friend_room_count == legacy_count;

        Ok(SyncStatus {
            friend_room_exists,
            legacy_data_exists,
            is_synced,
            friend_room_count,
            legacy_count,
        })
    }

    /// Ensure user has a friend list room, creating if necessary
    pub async fn ensure_friend_room(&self, user_id: &str) -> SyncResult<String> {
        if !self
            .friend_room_service
            .storage
            .friend_list_room_exists(user_id)
            .await
            .map_err(|e| SyncError::FriendRoom(e.to_string()))?
        {
            // Create friend list room
            self.friend_room_service
                .initialize_user_friend_room(user_id)
                .await
                .map_err(|e| SyncError::FriendRoom(e.to_string()))?;
        }

        Ok(self.friend_room_service.storage.get_friend_list_room_id(user_id))
    }

    /// Sync friend additions to both systems
    pub async fn sync_add_friend(
        &self,
        user_id: &str,
        friend_id: &str,
    ) -> SyncResult<()> {
        let now = chrono::Utc::now().timestamp();

        // Get profile information
        let profiles = self
            .registration_service
            .get_profiles(&[friend_id.to_string()])
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        let profile = profiles.into_iter().next();
        let display_name = profile
            .as_ref()
            .and_then(|p| p.get("display_name"))
            .and_then(|v| v.as_str());
        let avatar_url = profile
            .as_ref()
            .and_then(|p| p.get("avatar_url"))
            .and_then(|v| v.as_str());

        // Add to legacy friends table
        self.legacy_storage
            .add_friend(user_id, friend_id)
            .await
            .map_err(|e| SyncError::LegacyFriends(e.to_string()))?;

        // Add to friend room system
        if self.config.auto_migrate_on_access {
            self.ensure_friend_room(user_id).await?;
        }

        let friend_info = FriendInfo {
            user_id: friend_id.to_string(),
            display_name: display_name.map(|s| s.to_string()),
            avatar_url: avatar_url.map(|s| s.to_string()),
            since: now,
            status: None,
            last_active: None,
            note: None,
            is_private: None,
        };

        self.friend_room_service
            .storage
            .add_friend_to_list(user_id, friend_info)
            .await
            .map_err(|e| SyncError::FriendRoom(e.to_string()))?;

        Ok(())
    }

    /// Sync friend removals to both systems
    pub async fn sync_remove_friend(
        &self,
        user_id: &str,
        friend_id: &str,
    ) -> SyncResult<()> {
        // Remove from legacy friends table
        self.legacy_storage
            .remove_friend(user_id, friend_id)
            .await
            .map_err(|e| SyncError::LegacyFriends(e.to_string()))?;

        // Remove from friend room system
        self.friend_room_service
            .storage
            .remove_friend_from_list(user_id, friend_id)
            .await
            .map_err(|e| SyncError::FriendRoom(e.to_string()))?;

        Ok(())
    }

    /// Sync friend request acceptance to both systems
    pub async fn sync_accept_request(
        &self,
        request_id: i64,
        user_id: &str,
    ) -> SyncResult<String> {
        // Accept in legacy system
        self.legacy_storage
            .accept_request(request_id, user_id)
            .await
            .map_err(|e| SyncError::LegacyFriends(e.to_string()))?;

        // Accept in friend room system
        let dm_room_id = self
            .friend_room_service
            .storage
            .accept_friend_request(request_id, user_id)
            .await
            .map_err(|e| SyncError::FriendRoom(e.to_string()))?;

        Ok(dm_room_id)
    }

    /// Migrate all friend data from legacy to friend room system
    pub async fn migrate_user_funds(&self, user_id: &str) -> SyncResult<MigrationReport> {
        let _status = self.get_sync_status(user_id).await?;

        let mut report = MigrationReport {
            user_id: user_id.to_string(),
            migrated_friends: 0,
            skipped_friends: 0,
            errors: Vec::new(),
        };

        // Get all friends from legacy table
        let legacy_friends = self
            .legacy_storage
            .get_friends(user_id)
            .await
            .unwrap_or_default();

        if legacy_friends.is_empty() {
            return Ok(report);
        }

        // Ensure friend room exists
        self.ensure_friend_room(user_id).await?;

        // Get profile information for all friends
        let profiles = self
            .registration_service
            .get_profiles(&legacy_friends)
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        let mut profile_map: std::collections::HashMap<
            String,
            (Option<String>, Option<String>),
        > = std::collections::HashMap::new();

        for p in profiles {
            if let Some(uid) = p.get("user_id").and_then(|v| v.as_str()) {
                let display_name = p
                    .get("display_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let avatar_url = p
                    .get("avatar_url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                profile_map.insert(uid.to_string(), (display_name, avatar_url));
            }
        }

        // Get current friend list from room
        let existing_list = self
            .friend_room_service
            .storage
            .get_friend_list(user_id)
            .await
            .map_err(|e| SyncError::FriendRoom(e.to_string()))?;

        let existing_ids: std::collections::HashSet<String> = existing_list
            .friends
            .iter()
            .map(|f| f.user_id.clone())
            .collect();

        // Migrate each friend
        for friend_id in legacy_friends {
            if existing_ids.contains(&friend_id) {
                report.skipped_friends += 1;
                continue;
            }

            match self
                .migrate_single_friend(user_id, &friend_id, &profile_map)
                .await
            {
                Ok(_) => report.migrated_friends += 1,
                Err(e) => report.errors.push(format!("{}: {}", friend_id, e)),
            }
        }

        Ok(report)
    }

    /// Migrate a single friend relationship
    async fn migrate_single_friend(
        &self,
        user_id: &str,
        friend_id: &str,
        profile_map: &std::collections::HashMap<String, (Option<String>, Option<String>)>,
    ) -> Result<(), SyncError> {
        // Get friendship info from legacy table
        let friendship = self
            .legacy_storage
            .get_friendship(user_id, friend_id)
            .await
            .map_err(|e| SyncError::LegacyFriends(e.to_string()))?;

        let friendship = match friendship {
            Some(f) => f,
            None => return Ok(()), // No friendship to migrate
        };

        let (display_name, avatar_url) = profile_map
            .get(friend_id)
            .cloned()
            .unwrap_or((None, None));

        let friend_info = FriendInfo {
            user_id: friend_id.to_string(),
            display_name,
            avatar_url,
            since: friendship.created_ts,
            status: None,
            last_active: None,
            note: friendship.note,
            is_private: None,
        };

        self.friend_room_service
            .storage
            .add_friend_to_list(user_id, friend_info)
            .await
            .map_err(|e| SyncError::FriendRoom(e.to_string()))?;

        Ok(())
    }

    /// Get unified friend list (prefers friend rooms if configured)
    pub async fn get_friends_unified(&self, user_id: &str) -> SyncResult<serde_json::Value> {
        // If auto-migrate is enabled, ensure friend room exists
        if self.config.auto_migrate_on_access {
            self.ensure_friend_room(user_id).await?;
        }

        // Get friends from preferred source
        let friends = if self.config.prefer_friend_rooms {
            self.friend_room_service
                .get_friends(user_id)
                .await
                .map_err(|e| SyncError::Database(e.to_string()))?
        } else {
            // Fall back to legacy system
            let friend_ids = self
                .legacy_storage
                .get_friends(user_id)
                .await
                .map_err(|e| SyncError::LegacyFriends(e.to_string()))?;

            let profiles = self
                .registration_service
                .get_profiles(&friend_ids)
                .await
                .map_err(|e| SyncError::Database(e.to_string()))?;

            json!({
                "friends": profiles,
                "count": profiles.len(),
                "source": "legacy"
            })
        };

        Ok(friends)
    }
}

/// Report from a migration operation
#[derive(Debug, Clone, serde::Serialize)]
pub struct MigrationReport {
    /// User ID that was migrated
    pub user_id: String,
    /// Number of friends migrated
    pub migrated_friends: usize,
    /// Number of friends skipped (already existed)
    pub skipped_friends: usize,
    /// Any errors that occurred
    pub errors: Vec<String>,
}

// ==============================================================================
// Tests
// ==============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = FriendSyncConfig::default();
        assert!(config.enable_dual_mode);
        assert!(config.prefer_friend_rooms);
        assert!(config.auto_migrate_on_access);
    }

    #[test]
    fn test_migration_report_serialization() {
        let report = MigrationReport {
            user_id: "@alice:example.com".to_string(),
            migrated_friends: 5,
            skipped_friends: 2,
            errors: vec!["@bob:example.com: database error".to_string()],
        };

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("@alice:example.com"));
        assert!(json.contains("5"));
    }
}
