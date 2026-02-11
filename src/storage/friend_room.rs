//! Friend Room Storage Module
//!
//! This module implements the room-based friend system using Matrix rooms.
//! Each user has a dedicated "friend list room" that stores their friend relationships
//! as state events, enabling federation and standard Matrix client compatibility.

use crate::storage::{EventStorage, RoomMemberStorage, RoomStorage};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;

// ==============================================================================
// Custom Matrix Event Types for Friend System
// ==============================================================================

/// Custom event type for storing friend list as room state
pub const EVENT_TYPE_FRIENDS_LIST: &str = "m.friends.list";

/// Custom event type for friend request state
pub const EVENT_TYPE_FRIEND_REQUEST: &str = "m.friend.request";

/// Custom event type for marking rooms as friend-related
pub const EVENT_TYPE_FRIENDS_RELATED: &str = "m.friends.related_users";

/// Room type for friend list rooms
pub const ROOM_TYPE_FRIEND_LIST: &str = "m.friends.list";

/// Room type for direct message rooms between friends
pub const ROOM_TYPE_DIRECT_MESSAGE: &str = "m.direct";

/// Room type for private/secret friends
pub const ROOM_TYPE_PRIVATE_FRIEND: &str = "m.private";

// ==============================================================================
// Data Structures
// ==============================================================================

/// Friend information stored in friend list state event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendInfo {
    /// Matrix user ID (e.g., "@alice:example.com")
    pub user_id: String,
    /// Display name
    pub display_name: Option<String>,
    /// Avatar URL (mxc://)
    pub avatar_url: Option<String>,
    /// When they became friends (Unix timestamp)
    pub since: i64,
    /// Online status (optional)
    pub status: Option<String>,
    /// Last activity timestamp
    pub last_active: Option<i64>,
    /// Optional note about this friend
    pub note: Option<String>,
    /// Whether this is a "private" friend relationship
    pub is_private: Option<bool>,
}

/// Friend list state event content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendListContent {
    /// List of friends
    #[serde(default)]
    pub friends: Vec<FriendInfo>,
    /// Version of the friend list (for conflict resolution)
    pub version: i64,
}

/// Friend request state event content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendRequestContent {
    /// User ID of the requester
    pub requester: String,
    /// User ID of the recipient
    pub recipient: String,
    /// Optional message from requester
    pub message: Option<String>,
    /// Request status: pending, accepted, declined, cancelled
    pub status: String,
    /// Request creation timestamp
    pub created_ts: i64,
    /// Last update timestamp
    pub updated_ts: Option<i64>,
}

/// Related users marker for DM rooms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedUsersContent {
    /// Users in this direct message relationship
    pub related_users: Vec<String>,
    /// Whether this is a private/secret chat
    pub is_private: Option<bool>,
}

// ==============================================================================
// Friend Room Storage
// ==============================================================================

/// Storage layer for friend room operations.
///
/// This handles creating and managing rooms that store friend relationships
/// as Matrix state events, enabling federation support.
#[derive(Clone)]
pub struct FriendRoomStorage {
    pub pool: Arc<Pool<Postgres>>,
    pub room_storage: RoomStorage,
    pub member_storage: RoomMemberStorage,
    pub event_storage: EventStorage,
}

impl FriendRoomStorage {
    /// Create a new FriendRoomStorage instance
    pub fn new(pool: &Arc<Pool<Postgres>>, server_name: &str) -> Self {
        Self {
            pool: pool.clone(),
            room_storage: RoomStorage::new(pool),
            member_storage: RoomMemberStorage::new(pool, server_name),
            event_storage: EventStorage::new(pool),
        }
    }

    /// Create a new FriendRoomStorage instance with default server name
    pub fn new_default(pool: &Arc<Pool<Postgres>>) -> Self {
        Self::new(pool, "")
    }

    // ========================================================================
    // Friend List Room Operations
    // ========================================================================

    /// Get the friend list room ID for a user.
    /// Format: !friends:@user:server.com
    pub fn get_friend_list_room_id(&self, user_id: &str) -> String {
        // Create a deterministic room ID based on user ID
        // This allows finding the room without additional lookups
        format!("!friends:{}", user_id.trim_start_matches('@'))
    }

    /// Check if a user's friend list room exists
    pub async fn friend_list_room_exists(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        let room_id = self.get_friend_list_room_id(user_id);
        self.room_storage.get_room(&room_id).await.map(|r| r.is_some())
    }

    /// Create a friend list room for a user.
    /// This room stores the user's friend relationships as state events.
    pub async fn create_friend_list_room(
        &self,
        user_id: &str,
        _server_name: &str,
    ) -> Result<String, sqlx::Error> {
        let room_id = self.get_friend_list_room_id(user_id);
        let now = chrono::Utc::now().timestamp_millis();

        // Check if room already exists
        if self.friend_list_room_exists(user_id).await? {
            return Ok(room_id);
        }

        // Create the room
        sqlx::query(
            r#"
            INSERT INTO rooms (room_id, creator, join_rule, version, is_public, member_count,
                              history_visibility, creation_ts, last_activity_ts)
            VALUES ($1, $2, 'invite', '1', false, 1, 'joined', $3, $4)
            "#,
        )
        .bind(&room_id)
        .bind(user_id)
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        // Set room name
        self.set_room_name(&room_id, &format!("{}'s Friends",
            user_id.split(':').next().unwrap_or(user_id).trim_start_matches('@'))).await?;

        // Add the user as a member
        sqlx::query(
            r#"
            INSERT INTO room_memberships (room_id, user_id, membership, event_id,
                                         sender, origin_server_ts, state_key)
            VALUES ($1, $2, 'join', $3, $2, $4, $2)
            "#,
        )
        .bind(&room_id)
        .bind(user_id)
        .bind(crate::common::generate_event_id(""))
        .bind(now)
        .execute(&*self.pool)
        .await?;

        // Set initial empty friend list state
        self.update_friend_list_state(user_id, &FriendListContent {
            friends: Vec::new(),
            version: 1,
        }).await?;

        Ok(room_id)
    }

    /// Set the room name for the friend list room
    async fn set_room_name(&self, room_id: &str, name: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let event_id = crate::common::generate_event_id("");

        // Insert m.room.name event
        sqlx::query(
            r#"
            INSERT INTO events (event_id, room_id, user_id, sender, event_type, content,
                              state_key, origin_server_ts, processed_ts)
            VALUES ($1, $2, $3, $3, 'm.room.name', $4, '', $5, $6)
            "#,
        )
        .bind(&event_id)
        .bind(room_id)
        .bind(room_id) // Use room_id as placeholder for system user
        .bind(json!({"name": name}))
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    // ========================================================================
    // Friend List State Operations
    // ========================================================================

    /// Get the friend list state for a user
    pub async fn get_friend_list(&self, user_id: &str) -> Result<FriendListContent, sqlx::Error> {
        let room_id = self.get_friend_list_room_id(user_id);

        let row = sqlx::query(
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
        .await?;

        match row {
            Some(row) => {
                let content_json: serde_json::Value = row.try_get("content")?;
                serde_json::from_value(content_json)
                    .map_err(|e| sqlx::Error::Decode(Box::new(e)))
            }
            None => Ok(FriendListContent {
                friends: Vec::new(),
                version: 1,
            }),
        }
    }

    /// Update the friend list state for a user
    pub async fn update_friend_list_state(
        &self,
        user_id: &str,
        content: &FriendListContent,
    ) -> Result<(), sqlx::Error> {
        let room_id = self.get_friend_list_room_id(user_id);
        let now = chrono::Utc::now().timestamp_millis();
        let event_id = crate::common::generate_event_id("");

        let content_json = serde_json::to_value(content)
            .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;

        sqlx::query(
            r#"
            INSERT INTO events (event_id, room_id, user_id, sender, event_type, content,
                              state_key, origin_server_ts, processed_ts)
            VALUES ($1, $2, $3, $3, $4, $5, '', $6, $7)
            "#,
        )
        .bind(&event_id)
        .bind(&room_id)
        .bind(user_id)
        .bind(EVENT_TYPE_FRIENDS_LIST)
        .bind(&content_json)
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Add a friend to the user's friend list
    pub async fn add_friend_to_list(
        &self,
        user_id: &str,
        friend: FriendInfo,
    ) -> Result<(), sqlx::Error> {
        let mut list = self.get_friend_list(user_id).await?;

        // Remove if already exists
        list.friends.retain(|f| f.user_id != friend.user_id);

        // Add the friend
        list.friends.push(friend);
        list.version += 1;

        self.update_friend_list_state(user_id, &list).await
    }

    /// Remove a friend from the user's friend list
    pub async fn remove_friend_from_list(
        &self,
        user_id: &str,
        friend_id: &str,
    ) -> Result<(), sqlx::Error> {
        let mut list = self.get_friend_list(user_id).await?;

        // Remove the friend
        list.friends.retain(|f| f.user_id != friend_id);
        list.version += 1;

        self.update_friend_list_state(user_id, &list).await
    }

    /// Check if two users are friends (bidirectional check)
    pub async fn are_friends(&self, user_id: &str, friend_id: &str) -> Result<bool, sqlx::Error> {
        let list1 = self.get_friend_list(user_id).await?;
        let list2 = self.get_friend_list(friend_id).await?;

        let user1_has_user2 = list1.friends.iter().any(|f| f.user_id == friend_id);
        let user2_has_user1 = list2.friends.iter().any(|f| f.user_id == user_id);

        Ok(user1_has_user2 && user2_has_user1)
    }

    // ========================================================================
    // Direct Message Room Operations
    // ========================================================================

    /// Create a direct message room between two users
    pub async fn create_dm_room(
        &self,
        user_id: &str,
        friend_id: &str,
        _is_private: bool,
    ) -> Result<String, sqlx::Error> {
        // Generate a deterministic room ID for the DM
        // Sort user IDs to ensure both users get the same room ID
        let mut users = [user_id, friend_id];
        users.sort();
        let room_id = format!("!dm:{}:{}",
            users[0].trim_start_matches('@').replace(':', "_"),
            users[1].trim_start_matches('@').replace(':', "_")
        );

        let now = chrono::Utc::now().timestamp_millis();

        // Create the room
        sqlx::query(
            r#"
            INSERT INTO rooms (room_id, creator, join_rule, version, is_public, member_count,
                              history_visibility, creation_ts, last_activity_ts)
            VALUES ($1, $2, 'invite', '1', false, 2, 'joined', $3, $4)
            ON CONFLICT (room_id) DO NOTHING
            "#,
        )
        .bind(&room_id)
        .bind(user_id)
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        // Mark this room as a direct message with related users
        let related_content = RelatedUsersContent {
            related_users: vec![user_id.to_string(), friend_id.to_string()],
            is_private: Some(_is_private),
        };

        let content_json = serde_json::to_value(&related_content)
            .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;

        sqlx::query(
            r#"
            INSERT INTO events (event_id, room_id, user_id, sender, event_type, content,
                              state_key, origin_server_ts, processed_ts)
            VALUES ($1, $2, $3, $3, $4, $5, '', $6, $7)
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(crate::common::generate_event_id(""))
        .bind(&room_id)
        .bind(user_id)
        .bind(EVENT_TYPE_FRIENDS_RELATED)
        .bind(&content_json)
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        // Set encryption event for E2EE
        sqlx::query(
            r#"
            INSERT INTO events (event_id, room_id, user_id, sender, event_type, content,
                              state_key, origin_server_ts, processed_ts)
            VALUES ($1, $2, $3, $3, 'm.room.encryption', $4, '', $5, $6)
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(crate::common::generate_event_id(""))
        .bind(&room_id)
        .bind(user_id)
        .bind(json!({"algorithm": "m.megolm.v1.aes-sha2"}))
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        // Add both users as members
        for uid in &[user_id, friend_id] {
            sqlx::query(
                r#"
                INSERT INTO room_memberships (room_id, user_id, membership, event_id,
                                             sender, origin_server_ts, state_key)
                VALUES ($1, $2, 'join', $3, $2, $4, $2)
                ON CONFLICT (room_id, user_id) DO NOTHING
                "#,
            )
            .bind(&room_id)
            .bind(uid)
            .bind(crate::common::generate_event_id(""))
            .bind(now)
            .execute(&*self.pool)
            .await?;
        }

        Ok(room_id)
    }

    /// Get the DM room ID between two users
    pub async fn get_dm_room_id(
        &self,
        user_id: &str,
        friend_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let mut users = [user_id, friend_id];
        users.sort();

        // Try to find an existing DM room
        let room_id = format!("!dm:{}:{}",
            users[0].trim_start_matches('@').replace(':', "_"),
            users[1].trim_start_matches('@').replace(':', "_")
        );

        let exists = self.room_storage.get_room(&room_id).await?.is_some();
        if exists {
            Ok(Some(room_id))
        } else {
            Ok(None)
        }
    }

    // ========================================================================
    // Friend Request Operations
    // ========================================================================

    /// Create a friend request state event
    pub async fn create_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<String>,
    ) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();

        // Store the request in a dedicated table for easy querying
        let row = sqlx::query(
            r#"
            INSERT INTO friend_requests (from_user_id, to_user_id, message, created_ts, status)
            VALUES ($1, $2, $3, $4, 'pending')
            RETURNING id
            "#,
        )
        .bind(sender_id)
        .bind(receiver_id)
        .bind(message)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        row.try_get::<i64, _>("id")
    }

    /// Accept a friend request and add to both users' friend lists
    pub async fn accept_friend_request(
        &self,
        request_id: i64,
        user_id: &str,
    ) -> Result<String, sqlx::Error> {
        // Get the request details
        let row = sqlx::query(
            r#"
            SELECT from_user_id, to_user_id, message FROM friend_requests
            WHERE id = $1 AND to_user_id = $2 AND status = 'pending'
            "#,
        )
        .bind(request_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        let (sender_id, receiver_id) = match row {
            Some(r) => {
                let sender: String = r.try_get("from_user_id")?;
                let receiver: String = r.try_get("to_user_id")?;
                (sender, receiver)
            }
            None => return Err(sqlx::Error::RowNotFound),
        };

        let now = chrono::Utc::now().timestamp();

        // Update request status
        sqlx::query(
            r#"
            UPDATE friend_requests SET status = 'accepted', updated_ts = $1
            WHERE id = $2
            "#,
        )
        .bind(now)
        .bind(request_id)
        .execute(&*self.pool)
        .await?;

        // Ensure both users have friend list rooms
        self.create_friend_list_room(&sender_id, "").await?;
        self.create_friend_list_room(&receiver_id, "").await?;

        // Add to both users' friend lists
        let friend1 = FriendInfo {
            user_id: sender_id.clone(),
            display_name: None,
            avatar_url: None,
            since: now,
            status: None,
            last_active: None,
            note: None,
            is_private: None,
        };

        let friend2 = FriendInfo {
            user_id: receiver_id.clone(),
            display_name: None,
            avatar_url: None,
            since: now,
            status: None,
            last_active: None,
            note: None,
            is_private: None,
        };

        self.add_friend_to_list(&receiver_id, friend1).await?;
        self.add_friend_to_list(&sender_id, friend2).await?;

        // Create a DM room for them
        let dm_room_id = self.create_dm_room(&sender_id, &receiver_id, false).await?;

        Ok(dm_room_id)
    }

    /// Get pending friend requests for a user
    pub async fn get_pending_requests(
        &self,
        user_id: &str,
    ) -> Result<Vec<FriendRequestContent>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT from_user_id, to_user_id, message, status, created_ts, updated_ts
            FROM friend_requests
            WHERE to_user_id = $1 AND status = 'pending'
            ORDER BY created_ts DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        let mut requests = Vec::new();
        for row in rows {
            requests.push(FriendRequestContent {
                requester: row.try_get("from_user_id")?,
                recipient: row.try_get("to_user_id")?,
                message: row.try_get("message").ok(),
                status: row.try_get("status")?,
                created_ts: row.try_get("created_ts")?,
                updated_ts: row.try_get("updated_ts").ok(),
            });
        }

        Ok(requests)
    }
}

// ==============================================================================
// Tests
// ==============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_friend_list_room_id_format() {
        // The helper trims the @ prefix to match the actual implementation
        let room_id = get_friend_list_room_id_static("@alice:example.com");
        assert_eq!(room_id, "!friends:alice:example.com");

        let room_id = get_friend_list_room_id_static("@bob:other.server.com");
        assert_eq!(room_id, "!friends:bob:other.server.com");

        // Test with user_id that doesn't start with @
        let room_id = get_friend_list_room_id_static("charlie:example.com");
        assert_eq!(room_id, "!friends:charlie:example.com");
    }

    #[test]
    fn test_dm_room_id_format() {
        // Same pair should produce same room ID regardless of order
        let room1 = get_dm_room_id_static("@alice:example.com", "@bob:example.com");
        let room2 = get_dm_room_id_static("@bob:example.com", "@alice:example.com");

        assert_eq!(room1, room2);
    }

    #[test]
    fn test_dm_room_id_different_servers() {
        // Test cross-server DM room IDs
        let room1 = get_dm_room_id_static("@alice:server1.com", "@bob:server2.com");
        let room2 = get_dm_room_id_static("@bob:server2.com", "@alice:server1.com");

        assert_eq!(room1, room2);
        // The room ID should contain both users (with colons replaced by underscores)
        assert!(room1.starts_with("!dm:"));
        assert!(room1.contains("alice"));
        assert!(room1.contains("bob"));
    }

    #[test]
    fn test_dm_room_id_with_special_chars() {
        // Test with user IDs that contain periods in server names
        let room1 = get_dm_room_id_static("@user:name.server.com", "@other:server.example.com");
        let room2 = get_dm_room_id_static("@other:server.example.com", "@user:name.server.com");

        assert_eq!(room1, room2);
    }

    #[test]
    fn test_friend_info_serialization() {
        let friend = FriendInfo {
            user_id: "@bob:example.com".to_string(),
            display_name: Some("Bob".to_string()),
            avatar_url: Some("mxc://example.com/abc123".to_string()),
            since: 1234567890,
            status: Some("online".to_string()),
            last_active: Some(1234567890),
            note: Some("My friend Bob".to_string()),
            is_private: Some(false),
        };

        let json = serde_json::to_string(&friend).unwrap();
        assert!(json.contains("@bob:example.com"));
        assert!(json.contains("Bob"));
        assert!(json.contains("online"));

        // Test deserialization
        let deserialized: FriendInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.user_id, "@bob:example.com");
        assert_eq!(deserialized.display_name, Some("Bob".to_string()));
    }

    #[test]
    fn test_friend_list_content_serialization() {
        let content = FriendListContent {
            friends: vec![
                FriendInfo {
                    user_id: "@alice:example.com".to_string(),
                    display_name: Some("Alice".to_string()),
                    avatar_url: None,
                    since: 1234567890,
                    status: None,
                    last_active: None,
                    note: None,
                    is_private: None,
                },
            ],
            version: 5,
        };

        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("version"));
        assert!(json.contains("5"));

        let deserialized: FriendListContent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.version, 5);
        assert_eq!(deserialized.friends.len(), 1);
        assert_eq!(deserialized.friends[0].user_id, "@alice:example.com");
    }

    #[test]
    fn test_friend_request_content_serialization() {
        let request = FriendRequestContent {
            requester: "@alice:example.com".to_string(),
            recipient: "@bob:example.com".to_string(),
            message: Some("Let's be friends!".to_string()),
            status: "pending".to_string(),
            created_ts: 1234567890,
            updated_ts: Some(1234567895),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("pending"));

        let deserialized: FriendRequestContent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.status, "pending");
        assert_eq!(deserialized.requester, "@alice:example.com");
        assert_eq!(deserialized.recipient, "@bob:example.com");
    }

    #[test]
    fn test_related_users_content_serialization() {
        let content = RelatedUsersContent {
            related_users: vec!["@alice:example.com".to_string(), "@bob:example.com".to_string()],
            is_private: Some(true),
        };

        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("related_users"));

        let deserialized: RelatedUsersContent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.related_users.len(), 2);
        assert_eq!(deserialized.is_private, Some(true));
    }

    #[test]
    fn test_event_type_constants() {
        assert_eq!(EVENT_TYPE_FRIENDS_LIST, "m.friends.list");
        assert_eq!(EVENT_TYPE_FRIEND_REQUEST, "m.friend.request");
        assert_eq!(EVENT_TYPE_FRIENDS_RELATED, "m.friends.related_users");
    }

    #[test]
    fn test_room_type_constants() {
        assert_eq!(ROOM_TYPE_FRIEND_LIST, "m.friends.list");
        assert_eq!(ROOM_TYPE_DIRECT_MESSAGE, "m.direct");
        assert_eq!(ROOM_TYPE_PRIVATE_FRIEND, "m.private");
    }

    #[test]
    fn test_friend_info_defaults() {
        let friend = FriendInfo {
            user_id: "@test:example.com".to_string(),
            display_name: None,
            avatar_url: None,
            since: 0,
            status: None,
            last_active: None,
            note: None,
            is_private: None,
        };

        assert!(friend.display_name.is_none());
        assert!(friend.status.is_none());
        assert!(friend.is_private.is_none());
    }

    // Helper functions for testing (non-async versions)
    fn get_friend_list_room_id_static(user_id: &str) -> String {
        format!("!friends:{}", user_id.trim_start_matches('@'))
    }

    fn get_dm_room_id_static(user_id: &str, friend_id: &str) -> String {
        let mut users = [user_id, friend_id];
        users.sort();
        format!("!dm:{}:{}",
            users[0].trim_start_matches('@').replace(':', "_"),
            users[1].trim_start_matches('@').replace(':', "_")
        )
    }
}
