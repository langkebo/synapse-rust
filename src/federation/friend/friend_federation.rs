//! Friend Federation Module
//!
//! Handles cross-server friend events, friend requests, and DM room federation.

use super::friend_queries::FriendListQueryResponse;
use crate::common::ApiError;
use crate::services::friend_room_service::FriendRoomService;
use crate::storage::friend_room::{EVENT_TYPE_FRIENDS_LIST, FriendInfo};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;

/// Friend Federation Handler
///
/// Manages federation of friend-related events and cross-server friend operations.
pub struct FriendFederation {
    pool: Arc<Pool<Postgres>>,
    server_name: String,
    friend_room_service: FriendRoomService,
}

impl FriendFederation {
    /// Create a new FriendFederation instance
    pub fn new(
        pool: &Arc<Pool<Postgres>>,
        server_name: String,
        friend_room_service: FriendRoomService,
    ) -> Self {
        Self {
            pool: pool.clone(),
            server_name,
            friend_room_service,
        }
    }

    // ========================================================================
    // Incoming Federation Events
    // ========================================================================

    /// Handle an incoming friend list state event from another server
    ///
    /// This is called when a remote server sends a friend list update.
    /// Validates and stores the event for a remote user's friend list.
    pub async fn handle_incoming_friend_list_event(
        &self,
        event: &FederationFriendEvent,
    ) -> Result<(), ApiError> {
        // Validate the event
        self.validate_friend_event(event)?;

        // Check if this is for a local user (should not happen)
        if self.is_local_user(&event.user_id) {
            return Err(ApiError::bad_request(
                "Cannot process friend event for local user from remote server".to_string(),
            ));
        }

        // Store the event in the events table
        // Note: For remote users, we only store a shadow copy for local reference
        let event_id = format!("$fed_{}_{}:{}", event.origin_server_ts, event.user_id, self.server_name);

        sqlx::query(
            r#"
            INSERT INTO events (event_id, room_id, user_id, sender, event_type, content,
                              state_key, origin_server_ts, processed_ts, origin)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (event_id) DO UPDATE SET
                content = EXCLUDED.content,
                origin_server_ts = EXCLUDED.origin_server_ts
            "#,
        )
        .bind(&event_id)
        .bind(&event.room_id)
        .bind(&event.user_id)
        .bind(&event.sender)
        .bind(EVENT_TYPE_FRIENDS_LIST)
        .bind(&event.content)
        .bind(&event.state_key)
        .bind(event.origin_server_ts)
        .bind(chrono::Utc::now().timestamp_millis())
        .bind(event.origin.clone().unwrap_or_else(|| event.room_id.split(':').last().unwrap_or("").to_string()))
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to store friend event: {}", e)))?;

        Ok(())
    }

    /// Handle an incoming friend request from another server
    ///
    /// Processes a friend request from a remote user to a local user.
    pub async fn handle_incoming_friend_request(
        &self,
        request: IncomingFriendRequest,
    ) -> Result<FriendRequestResponse, ApiError> {
        // Validate the request
        self.validate_incoming_request(&request)?;

        // Check if recipient exists
        let recipient_exists = sqlx::query("SELECT user_id FROM users WHERE user_id = $1")
            .bind(&request.recipient_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if recipient_exists.is_none() {
            return Ok(FriendRequestResponse {
                status: "recipient_not_found".to_string(),
                message: "User not found on this server".to_string(),
                request_id: None,
            });
        }

        // Check if users are already friends
        if let Ok(true) = self.friend_room_service.are_friends(&request.sender_id, &request.recipient_id).await {
            return Ok(FriendRequestResponse {
                status: "already_friends".to_string(),
                message: "Users are already friends".to_string(),
                request_id: None,
            });
        }

        // Create friend request in local database
        let now = chrono::Utc::now().timestamp();

        let row = sqlx::query(
            r#"
            INSERT INTO friend_requests (from_user_id, to_user_id, message, created_ts, status)
            VALUES ($1, $2, $3, $4, 'pending')
            RETURNING id
            "#,
        )
        .bind(&request.sender_id)
        .bind(&request.recipient_id)
        .bind(&request.message)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create friend request: {}", e)))?;

        match row {
            Some(row) => {
                let request_id: i64 = row.try_get("id")
                    .map_err(|e| ApiError::internal(format!("Failed to get request ID: {}", e)))?;
                Ok(FriendRequestResponse {
                    status: "pending".to_string(),
                    message: "Friend request created".to_string(),
                    request_id: Some(request_id),
                })
            }
            None => Ok(FriendRequestResponse {
                status: "failed".to_string(),
                message: "Failed to create friend request".to_string(),
                request_id: None,
            }),
        }
    }

    /// Handle acceptance of a friend request from a remote server
    ///
    /// Called when a remote server accepts a friend request from a local user.
    pub async fn handle_remote_request_acceptance(
        &self,
        request_id: i64,
        local_user_id: &str,
        remote_user_id: &str,
    ) -> Result<DirectMessageRoomInfo, ApiError> {
        // Update request status
        let now = chrono::Utc::now().timestamp();

        let rows_affected = sqlx::query(
            r#"
            UPDATE friend_requests
            SET status = 'accepted', updated_ts = $1
            WHERE id = $2 AND to_user_id = $3 AND from_user_id = $4 AND status = 'pending'
            "#,
        )
        .bind(now)
        .bind(request_id)
        .bind(local_user_id)
        .bind(remote_user_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .rows_affected();

        if rows_affected == 0 {
            return Err(ApiError::not_found("Friend request not found".to_string()));
        }

        // Ensure both users have friend list rooms
        self.friend_room_service
            .initialize_user_friend_room(local_user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create friend room: {}", e)))?;

        // Create a shadow friend list room for the remote user
        let remote_room_id = self.friend_room_service.storage.get_friend_list_room_id(remote_user_id);
        self.ensure_shadow_friend_room(remote_user_id, &remote_room_id).await?;

        // Add to both users' friend lists
        let _friend_local = FriendInfo {
            user_id: local_user_id.to_string(),
            display_name: None,
            avatar_url: None,
            since: now,
            status: None,
            last_active: None,
            note: None,
            is_private: None,
        };

        let friend_remote = FriendInfo {
            user_id: remote_user_id.to_string(),
            display_name: None,
            avatar_url: None,
            since: now,
            status: None,
            last_active: None,
            note: None,
            is_private: None,
        };

        self.friend_room_service
            .storage
            .add_friend_to_list(local_user_id, friend_remote)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to add friend: {}", e)))?;

        // Note: We don't add to remote user's friend list directly - that's done via federation

        // Create or get DM room
        let dm_room_id = self.friend_room_service
            .storage
            .create_dm_room(local_user_id, remote_user_id, false)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create DM room: {}", e)))?;

        Ok(DirectMessageRoomInfo {
            room_id: dm_room_id,
            local_user: local_user_id.to_string(),
            remote_user: remote_user_id.to_string(),
            created_ts: now,
        })
    }

    // ========================================================================
    // Outgoing Federation Events
    // ========================================================================

    /// Prepare a friend list event for federation
    ///
    /// Creates an event that can be sent to other servers to update
    /// a local user's friend list.
    pub async fn prepare_friend_list_event_for_federation(
        &self,
        user_id: &str,
    ) -> Result<Option<FederationFriendEvent>, ApiError> {
        // Get friend list room
        let room_id = self.friend_room_service.storage.get_friend_list_room_id(user_id);

        // Get current friend list state
        let friend_list = self
            .friend_room_service
            .storage
            .get_friend_list(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get friend list: {}", e)))?;

        // Create the event
        let event_id = format!("$fed_{}_{}:{}", chrono::Utc::now().timestamp_millis(), user_id, self.server_name);

        Ok(Some(FederationFriendEvent {
            event_id,
            room_id,
            user_id: user_id.to_string(),
            sender: user_id.to_string(),
            event_type: EVENT_TYPE_FRIENDS_LIST.to_string(),
            content: json!({
                "friends": friend_list.friends,
                "version": friend_list.version,
            }),
            state_key: String::new(),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
            origin: Some(self.server_name.clone()),
        }))
    }

    /// Query a remote server for a user's friend list
    ///
    /// Makes a federation request to get a user's friend list from their home server.
    pub async fn query_remote_friend_list(
        &self,
        user_id: &str,
    ) -> Result<RemoteFriendListResponse, ApiError> {
        // Extract server name from user ID
        let server_name = user_id.split(':').nth(1).ok_or_else(|| {
            ApiError::bad_request("Invalid user ID format".to_string())
        })?;

        // If it's a local user, get directly
        if server_name == self.server_name {
            let friend_list = self.query_friend_list(user_id).await?;
            return Ok(RemoteFriendListResponse {
                user_id: user_id.to_string(),
                server: server_name.to_string(),
                exists: friend_list.exists,
                friends: friend_list.friends,
            });
        }

        // TODO: Implement actual federation HTTP request to remote server
        // For now, return a placeholder response
        Ok(RemoteFriendListResponse {
            user_id: user_id.to_string(),
            server: server_name.to_string(),
            exists: true,
            friends: Vec::new(), // Would be populated from federation response
        })
    }

    // ========================================================================
    // Helper Functions
    // ========================================================================

    /// Validate a friend event from federation
    fn validate_friend_event(&self, event: &FederationFriendEvent) -> Result<(), ApiError> {
        // Check event type
        if event.event_type != EVENT_TYPE_FRIENDS_LIST {
            return Err(ApiError::bad_request("Invalid event type".to_string()));
        }

        // Validate user ID format
        if !event.user_id.starts_with('@') || !event.user_id.contains(':') {
            return Err(ApiError::bad_request("Invalid user ID format".to_string()));
        }

        // Validate room ID format
        if !event.room_id.starts_with('!') {
            return Err(ApiError::bad_request("Invalid room ID format".to_string()));
        }

        // Validate content structure
        if !event.content.is_object() {
            return Err(ApiError::bad_request("Invalid event content".to_string()));
        }

        Ok(())
    }

    /// Validate an incoming friend request
    fn validate_incoming_request(&self, request: &IncomingFriendRequest) -> Result<(), ApiError> {
        // Validate user IDs
        if !request.sender_id.starts_with('@') || !request.sender_id.contains(':') {
            return Err(ApiError::bad_request("Invalid sender user ID format".to_string()));
        }

        if !request.recipient_id.starts_with('@') || !request.recipient_id.contains(':') {
            return Err(ApiError::bad_request("Invalid recipient user ID format".to_string()));
        }

        // Validate message length
        if let Some(msg) = &request.message {
            if msg.len() > 500 {
                return Err(ApiError::bad_request("Message too long".to_string()));
            }
        }

        Ok(())
    }

    /// Check if a user ID is local to this server
    fn is_local_user(&self, user_id: &str) -> bool {
        user_id.ends_with(&format!(":{}", self.server_name))
    }

    /// Ensure a shadow friend list room exists for a remote user
    async fn ensure_shadow_friend_room(
        &self,
        remote_user_id: &str,
        room_id: &str,
    ) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        // Check if room exists
        let exists = sqlx::query("SELECT room_id FROM rooms WHERE room_id = $1")
            .bind(room_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if exists.is_some() {
            return Ok(());
        }

        // Create shadow room for remote user
        sqlx::query(
            r#"
            INSERT INTO rooms (room_id, creator, join_rule, version, is_public, member_count,
                              history_visibility, creation_ts, last_activity_ts)
            VALUES ($1, $2, 'invite', '1', false, 0, 'joined', $3, $4)
            "#,
        )
        .bind(room_id)
        .bind(remote_user_id)
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create shadow room: {}", e)))?;

        Ok(())
    }

    /// Query friend list (for federation queries)
    async fn query_friend_list(&self, user_id: &str) -> Result<FriendListQueryResponse, ApiError> {
        let room_id = self.friend_room_service.storage.get_friend_list_room_id(user_id);

        // Check if room exists
        let room_exists = sqlx::query("SELECT room_id FROM rooms WHERE room_id = $1")
            .bind(&room_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if room_exists.is_none() {
            return Ok(FriendListQueryResponse {
                user_id: user_id.to_string(),
                exists: false,
                friend_list_room: None,
                friends: Vec::new(),
            });
        }

        // Get friend list state event
        let friend_list = sqlx::query(
            r#"
            SELECT content FROM events
            WHERE room_id = $1
              AND event_type = $2
              AND state_key = ''
            ORDER BY origin_server_ts DESC
            LIMIT 1
            "#,
        )
        .bind(&room_id)
        .bind(EVENT_TYPE_FRIENDS_LIST)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let friends = match friend_list {
            Some(row) => {
                let content: Value = row.try_get("content")
                    .map_err(|e| ApiError::internal(format!("Failed to parse content: {}", e)))?;
                content.get("friends")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        let mut friends = Vec::new();
                        for f in arr {
                            if let Some(uid) = f.get("user_id").and_then(|v| v.as_str()) {
                                friends.push(uid.to_string());
                            }
                        }
                        friends
                    })
                    .unwrap_or_default()
            }
            None => Vec::new(),
        };

        Ok(FriendListQueryResponse {
            user_id: user_id.to_string(),
            exists: true,
            friend_list_room: Some(room_id),
            friends,
        })
    }
}

// ==============================================================================
// Data Structures
// ==============================================================================

/// Federation friend event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationFriendEvent {
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: Value,
    pub state_key: String,
    pub origin_server_ts: i64,
    pub origin: Option<String>,
}

/// Incoming friend request from a remote server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingFriendRequest {
    pub sender_id: String,
    pub recipient_id: String,
    pub message: Option<String>,
    pub request_ts: Option<i64>,
}

/// Response to a friend request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendRequestResponse {
    pub status: String,
    pub message: String,
    pub request_id: Option<i64>,
}

/// Information about a direct message room
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectMessageRoomInfo {
    pub room_id: String,
    pub local_user: String,
    pub remote_user: String,
    pub created_ts: i64,
}

/// Response from remote friend list query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFriendListResponse {
    pub user_id: String,
    pub server: String,
    pub exists: bool,
    pub friends: Vec<String>,
}

// ==============================================================================
// Tests
// ==============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_federation_friend_event_serialization() {
        let event = FederationFriendEvent {
            event_id: "$event:example.com".to_string(),
            room_id: "!friends:@alice:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            event_type: "m.friends.list".to_string(),
            content: json!({"friends": [], "version": 1}),
            state_key: String::new(),
            origin_server_ts: 1234567890,
            origin: Some("example.com".to_string()),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("m.friends.list"));
    }

    #[test]
    fn test_incoming_friend_request_validation() {
        let valid_request = IncomingFriendRequest {
            sender_id: "@alice:other.com".to_string(),
            recipient_id: "@bob:example.com".to_string(),
            message: Some("Hi, let's be friends!".to_string()),
            request_ts: Some(1234567890),
        };

        let pool = Arc::new(sqlx::PgPool::connect("postgresql://localhost/test").await.unwrap());
        let service = FriendFederation::new(
            &pool,
            "example.com".to_string(),
            // friend_room_service would be needed here
        );

        // Just check that the validation doesn't panic
        let _ = service.validate_incoming_request(&valid_request);
    }
}
