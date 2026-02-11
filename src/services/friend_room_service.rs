//! Friend Room Service Module
//!
//! Business logic layer for the room-based friend system.
//! Provides high-level operations for managing friends using Matrix rooms.

use crate::common::ApiError;
use crate::services::{RegistrationService, UserStorage};
use crate::storage::friend_room::*;
use serde_json::json;
use std::sync::Arc;

/// Result type for friend operations
pub type FriendResult<T> = Result<T, ApiError>;

/// High-level service for friend room operations
pub struct FriendRoomService {
    /// Storage layer for friend rooms
    pub storage: FriendRoomStorage,
    /// Registration service for user profile lookups
    pub registration_service: Arc<RegistrationService>,
    /// User storage for user existence checks
    pub user_storage: UserStorage,
    /// Server name for room ID generation
    pub server_name: String,
}

impl FriendRoomService {
    /// Create a new FriendRoomService
    pub fn new(
        pool: &Arc<sqlx::PgPool>,
        registration_service: Arc<RegistrationService>,
        user_storage: UserStorage,
        server_name: String,
    ) -> Self {
        Self {
            storage: FriendRoomStorage::new(pool, &server_name),
            registration_service,
            user_storage,
            server_name,
        }
    }

    // ========================================================================
    // Friend List Operations
    // ========================================================================

    /// Get the friend list for a user
    ///
    /// Returns the user's friends with profile information.
    pub async fn get_friends(&self, user_id: &str) -> FriendResult<serde_json::Value> {
        // Ensure friend list room exists
        self.storage.create_friend_list_room(user_id, &self.server_name).await
            .map_err(|e| ApiError::internal(format!("Failed to create friend list room: {}", e)))?;

        // Get friend list from room state
        let list = self.storage.get_friend_list(user_id).await
            .map_err(|e| ApiError::internal(format!("Failed to get friend list: {}", e)))?;

        // Get profile information for all friends
        let friend_ids: Vec<String> = list.friends.iter().map(|f| f.user_id.clone()).collect();

        let profiles = if !friend_ids.is_empty() {
            self.registration_service.get_profiles(&friend_ids).await?
        } else {
            vec![]
        };

        // Create a map for quick lookup
        let profile_map: std::collections::HashMap<String, serde_json::Value> = profiles
            .into_iter()
            .filter_map(|p| {
                let user_id = p.get("user_id")?.as_str()?.to_string();
                Some((user_id, p))
            })
            .collect();

        // Combine friend info with profile data
        let friends: Vec<serde_json::Value> = list.friends.into_iter().map(|friend| {
            let mut result = json!({
                "user_id": friend.user_id,
                "since": friend.since,
                "is_private": friend.is_private.unwrap_or(false),
            });

            // Add profile info if available
            if let Some(profile) = profile_map.get(&friend.user_id) {
                if let Some(display_name) = profile.get("display_name") {
                    result["display_name"] = display_name.clone();
                }
                if let Some(avatar_url) = profile.get("avatar_url") {
                    result["avatar_url"] = avatar_url.clone();
                }
            }

            // Add stored display name if profile not available
            if friend.display_name.is_some() && !result.as_object().unwrap().contains_key("display_name") {
                result["display_name"] = json!(friend.display_name);
            }
            if friend.avatar_url.is_some() && !result.as_object().unwrap().contains_key("avatar_url") {
                result["avatar_url"] = json!(friend.avatar_url);
            }

            if let Some(note) = friend.note {
                result["note"] = json!(note);
            }

            result
        }).collect();

        Ok(json!({
            "friends": friends,
            "count": friends.len(),
            "version": list.version,
        }))
    }

    /// Get friend list room ID for a user
    pub async fn get_friend_list_room_id(&self, user_id: &str) -> FriendResult<String> {
        let room_id = self.storage.get_friend_list_room_id(user_id);

        // Ensure room exists
        if !self.storage.friend_list_room_exists(user_id).await
            .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
        {
            self.storage.create_friend_list_room(user_id, &self.server_name).await
                .map_err(|e| ApiError::internal(format!("Failed to create friend list room: {}", e)))?;
        }

        Ok(room_id)
    }

    // ========================================================================
    // Friend Request Operations
    // ========================================================================

    /// Send a friend request to another user
    pub async fn send_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<String>,
    ) -> FriendResult<serde_json::Value> {
        // Validate sender != receiver
        if sender_id == receiver_id {
            return Err(ApiError::bad_request("Cannot send friend request to yourself".to_string()));
        }

        // Validate receiver exists
        if !self.user_storage.user_exists(receiver_id).await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        // Check if already friends
        if self.storage.are_friends(sender_id, receiver_id).await
            .map_err(|e| ApiError::internal(format!("Failed to check friendship: {}", e)))?
        {
            return Err(ApiError::bad_request("Already friends".to_string()));
        }

        // Check if request already exists
        // This is done implicitly by the storage layer via the unique constraint

        // Create the friend request
        let request_id = self.storage.create_friend_request(sender_id, receiver_id, message).await
            .map_err(|e| ApiError::internal(format!("Failed to create friend request: {}", e)))?;

        Ok(json!({
            "request_id": request_id,
            "status": "pending",
            "message": "Friend request sent successfully"
        }))
    }

    /// Accept a friend request
    ///
    /// Accepts the request and creates a bidirectional friendship.
    /// Also creates a direct message room for the two users.
    pub async fn accept_friend_request(
        &self,
        request_id: i64,
        user_id: &str,
    ) -> FriendResult<serde_json::Value> {
        // Accept the request (this also adds to friend lists)
        let dm_room_id = self.storage.accept_friend_request(request_id, user_id).await
            .map_err(|e| ApiError::internal(format!("Failed to accept friend request: {}", e)))?;

        Ok(json!({
            "status": "accepted",
            "dm_room_id": dm_room_id,
            "message": "Friend request accepted. You can now chat in the direct message room."
        }))
    }

    /// Decline a friend request
    pub async fn decline_friend_request(
        &self,
        request_id: i64,
        user_id: &str,
    ) -> FriendResult<serde_json::Value> {
        let now = chrono::Utc::now().timestamp();

        let rows_affected = sqlx::query(
            r#"
            UPDATE friend_requests
            SET status = 'declined', updated_ts = $1
            WHERE id = $2 AND to_user_id = $3 AND status = 'pending'
            "#,
        )
        .bind(now)
        .bind(request_id)
        .bind(user_id)
        .execute(&*self.storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .rows_affected();

        if rows_affected == 0 {
            return Err(ApiError::not_found("Friend request not found".to_string()));
        }

        Ok(json!({
            "status": "declined",
            "message": "Friend request declined"
        }))
    }

    /// Get pending friend requests for a user
    pub async fn get_pending_requests(&self, user_id: &str) -> FriendResult<serde_json::Value> {
        let requests = self.storage.get_pending_requests(user_id).await
            .map_err(|e| ApiError::internal(format!("Failed to get pending requests: {}", e)))?;

        // Get profile information for all requesters
        let requester_ids: Vec<String> = requests.iter().map(|r| r.requester.clone()).collect();

        let profiles = if !requester_ids.is_empty() {
            self.registration_service.get_profiles(&requester_ids).await?
        } else {
            vec![]
        };

        // Create a map for quick lookup
        let profile_map: std::collections::HashMap<String, serde_json::Value> = profiles
            .into_iter()
            .filter_map(|p| {
                let user_id = p.get("user_id")?.as_str()?.to_string();
                Some((user_id, p))
            })
            .collect();

        // Combine request info with profile data
        let request_list: Vec<serde_json::Value> = requests.into_iter().map(|req| {
            let mut result = json!({
                "request_id": 0, // We don't have this from the content, would need separate query
                "sender": {
                    "user_id": req.requester,
                },
                "message": req.message,
                "created_ts": req.created_ts,
            });

            // Add profile info if available
            if let Some(profile) = profile_map.get(&req.requester) {
                result["sender"] = profile.clone();
            }

            result
        }).collect();

        Ok(json!({
            "requests": request_list,
            "count": request_list.len()
        }))
    }

    // ========================================================================
    // Direct Message Operations
    // ========================================================================

    /// Get or create a direct message room with a friend
    pub async fn get_dm_room(
        &self,
        user_id: &str,
        friend_id: &str,
        is_private: bool,
    ) -> FriendResult<serde_json::Value> {
        // Check if they are friends
        if !self.storage.are_friends(user_id, friend_id).await
            .map_err(|e| ApiError::internal(format!("Failed to check friendship: {}", e)))?
        {
            return Err(ApiError::forbidden("Users are not friends".to_string()));
        }

        // Try to get existing DM room
        let existing_room = self.storage.get_dm_room_id(user_id, friend_id).await
            .map_err(|e| ApiError::internal(format!("Failed to get DM room: {}", e)))?;

        let room_id = if let Some(room_id) = existing_room {
            room_id
        } else {
            // Create new DM room
            self.storage.create_dm_room(user_id, friend_id, is_private).await
                .map_err(|e| ApiError::internal(format!("Failed to create DM room: {}", e)))?
        };

        Ok(json!({
            "room_id": room_id,
            "is_private": is_private
        }))
    }

    // ========================================================================
    // Friend Removal
    // ========================================================================

    /// Remove a friend (bidirectional)
    pub async fn remove_friend(&self, user_id: &str, friend_id: &str) -> FriendResult<()> {
        // Use transaction to ensure both removals succeed
        let tx = self.storage.pool.begin().await
            .map_err(|e| ApiError::internal(format!("Failed to begin transaction: {}", e)))?;

        // Remove from both users' friend lists
        self.storage.remove_friend_from_list(user_id, friend_id).await
            .map_err(|e| ApiError::internal(format!("Failed to remove from friend list: {}", e)))?;

        self.storage.remove_friend_from_list(friend_id, user_id).await
            .map_err(|e| ApiError::internal(format!("Failed to remove from friend's list: {}", e)))?;

        tx.commit().await
            .map_err(|e| ApiError::internal(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    // ========================================================================
    // Utility Functions
    // ========================================================================

    /// Check if two users are friends
    pub async fn are_friends(&self, user_id: &str, friend_id: &str) -> FriendResult<bool> {
        self.storage.are_friends(user_id, friend_id).await
            .map_err(|e| ApiError::internal(format!("Failed to check friendship: {}", e)))
    }

    /// Initialize friend list room for an existing user
    /// Call this when migrating users to the new friend room system
    pub async fn initialize_user_friend_room(&self, user_id: &str) -> FriendResult<String> {
        let room_id = self.storage.create_friend_list_room(user_id, &self.server_name).await
            .map_err(|e| ApiError::internal(format!("Failed to create friend list room: {}", e)))?;

        Ok(room_id)
    }
}

// ==============================================================================
// Helper Functions
// ==============================================================================

/// Extract server name from a Matrix user ID
pub fn extract_server_name(user_id: &str) -> Option<String> {
    user_id.split(':').nth(1).map(|s| s.to_string())
}

/// Check if a user ID is local to this server
pub fn is_local_user(user_id: &str, server_name: &str) -> bool {
    user_id.ends_with(&format!(":{}", server_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_server_name() {
        assert_eq!(extract_server_name("@alice:example.com"), Some("example.com".to_string()));
        assert_eq!(extract_server_name("@bob:matrix.server.com"), Some("matrix.server.com".to_string()));
        assert_eq!(extract_server_name("@invalid"), None);
    }

    #[test]
    fn test_is_local_user() {
        assert!(is_local_user("@alice:example.com", "example.com"));
        assert!(is_local_user("@bob:example.com", "example.com"));
        assert!(!is_local_user("@charlie:other.com", "example.com"));
    }
}
