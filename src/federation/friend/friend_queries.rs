//! Friend Federation Queries Module
//!
//! Handles Matrix federation queries for friend list discovery and
//! friend relationship verification across servers.

use crate::common::ApiError;
use crate::storage::friend_room::EVENT_TYPE_FRIENDS_LIST;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;

/// Query type for friend list discovery
pub const QUERY_TYPE_FRIEND_LIST: &str = "m.friends.list";

/// Query type for friend relationship verification
pub const QUERY_TYPE_FRIEND_RELATIONSHIP: &str = "m.friends.relationship";

/// Federation query handler for friend-related queries
pub struct FriendFederationQueries {
    pool: Arc<Pool<Postgres>>,
    server_name: String,
}

impl FriendFederationQueries {
    /// Create a new FriendFederationQueries instance
    pub fn new(pool: &Arc<Pool<Postgres>>, server_name: String) -> Self {
        Self {
            pool: pool.clone(),
            server_name,
        }
    }

    /// Handle a federation query for a user's friend list
    ///
    /// This is called when another server queries for a user's friend list.
    /// Returns the friend list state event content.
    pub async fn query_friend_list(
        &self,
        user_id: &str,
    ) -> Result<FriendListQueryResponse, ApiError> {
        // Verify user exists
        let user_exists = sqlx::query("SELECT user_id FROM users WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if user_exists.is_none() {
            return Ok(FriendListQueryResponse {
                user_id: user_id.to_string(),
                exists: false,
                friend_list_room: None,
                friends: Vec::new(),
            });
        }

        // Get friend list room ID
        let room_id = format!("!friends:{}", user_id.trim_start_matches('@'));

        // Check if room exists
        let room_exists = sqlx::query("SELECT room_id FROM rooms WHERE room_id = $1")
            .bind(&room_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if room_exists.is_none() {
            return Ok(FriendListQueryResponse {
                user_id: user_id.to_string(),
                exists: true,
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
                if let Some(arr) = content.get("friends").and_then(|v| v.as_array()) {
                    let mut friends = Vec::new();
                    for f in arr {
                        if let Some(uid) = f.get("user_id").and_then(|v| v.as_str()) {
                            friends.push(uid.to_string());
                        }
                    }
                    friends
                } else {
                    Vec::new()
                }
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

    /// Verify a friend relationship between two users
    ///
    /// Checks if user_id has friend_id in their friend list.
    /// Works for both local and remote users.
    pub async fn verify_friend_relationship(
        &self,
        user_id: &str,
        friend_id: &str,
    ) -> Result<FriendRelationshipResponse, ApiError> {
        // Get friend list room ID for the user
        let room_id = format!("!friends:{}", user_id.trim_start_matches('@'));

        // Check if room exists
        let room_exists = sqlx::query("SELECT room_id FROM rooms WHERE room_id = $1")
            .bind(&room_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if room_exists.is_none() {
            return Ok(FriendRelationshipResponse {
                user_id: user_id.to_string(),
                friend_id: friend_id.to_string(),
                are_friends: false,
                since: None,
            });
        }

        // Check if friend_id is in the friend list
        let result = sqlx::query(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM events
                WHERE room_id = $1
                  AND event_type = $2
                  AND state_key = ''
                  AND content->'friends' @> $3::jsonb
            ) as is_friend
            "#,
        )
        .bind(&room_id)
        .bind(EVENT_TYPE_FRIENDS_LIST)
        .bind(json!([{"user_id": friend_id}]))
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let is_friend: bool = result.try_get("is_friend")
            .map_err(|e| ApiError::internal(format!("Failed to parse result: {}", e)))?;

        // If friends, get the since timestamp
        let since = if is_friend {
            let friend_data = sqlx::query(
                r#"
                SELECT content FROM events
                WHERE room_id = $1
                  AND event_type = $2
                  AND state_key = ''
                  AND content->'friends' @> $3::jsonb
                ORDER BY origin_server_ts DESC
                LIMIT 1
                "#,
            )
            .bind(&room_id)
            .bind(EVENT_TYPE_FRIENDS_LIST)
            .bind(json!([{"user_id": friend_id}]))
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

            if let Some(row) = friend_data {
                let content: Value = row.try_get("content")
                    .map_err(|e| ApiError::internal(format!("Failed to parse content: {}", e)))?;
                content.get("friends")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.iter().find(|f| {
                        f.get("user_id")
                            .and_then(|uid| uid.as_str())
                            .map(|s| s == friend_id)
                            .unwrap_or(false)
                    }))
                    .and_then(|f| f.get("since"))
                    .and_then(|s| s.as_i64())
            } else {
                None
            }
        } else {
            None
        };

        Ok(FriendRelationshipResponse {
            user_id: user_id.to_string(),
            friend_id: friend_id.to_string(),
            are_friends: is_friend,
            since,
        })
    }

    /// Get multiple friend lists in batch
    ///
    /// Efficiently queries friend lists for multiple users.
    pub async fn batch_query_friend_lists(
        &self,
        user_ids: &[String],
    ) -> Result<Vec<FriendListQueryResponse>, ApiError> {
        let mut results = Vec::new();

        for user_id in user_ids {
            match self.query_friend_list(user_id).await {
                Ok(response) => results.push(response),
                Err(e) => {
                    eprintln!("Failed to query friend list for {}: {}", user_id, e);
                    results.push(FriendListQueryResponse {
                        user_id: user_id.clone(),
                        exists: false,
                        friend_list_room: None,
                        friends: Vec::new(),
                    });
                }
            }
        }

        Ok(results)
    }

    /// Discover friend relationships across servers
    ///
    /// Given a user, discover which of their friends are on other servers
    /// and return the server names for federation queries.
    pub async fn discover_federation_servers(
        &self,
        user_id: &str,
    ) -> Result<FederationServerDiscovery, ApiError> {
        let friend_list = self.query_friend_list(user_id).await?;

        // Extract server names from friend IDs
        let mut servers = std::collections::HashSet::new();
        servers.insert(self.server_name.clone()); // Always include local server

        let total_friends = friend_list.friends.len();
        for friend_id in &friend_list.friends {
            if let Some(server) = friend_id.split(':').nth(1) {
                servers.insert(server.to_string());
            }
        }

        Ok(FederationServerDiscovery {
            user_id: user_id.to_string(),
            total_friends,
            servers: servers.into_iter().collect(),
            local_server: self.server_name.clone(),
        })
    }
}

// ==============================================================================
// Query Types
// ==============================================================================

/// Response for a friend list federation query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendListQueryResponse {
    /// User ID that was queried
    pub user_id: String,
    /// Whether the user exists on this server
    pub exists: bool,
    /// Friend list room ID (if exists)
    pub friend_list_room: Option<String>,
    /// List of friend user IDs
    pub friends: Vec<String>,
}

/// Response for a friend relationship verification query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendRelationshipResponse {
    /// User ID
    pub user_id: String,
    /// Friend ID that was checked
    pub friend_id: String,
    /// Whether they are friends
    pub are_friends: bool,
    /// When they became friends (if applicable)
    pub since: Option<i64>,
}

/// Response for federation server discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationServerDiscovery {
    /// User ID
    pub user_id: String,
    /// Total number of friends
    pub total_friends: usize,
    /// List of servers where friends are hosted
    pub servers: Vec<String>,
    /// This server's name
    pub local_server: String,
}

// ==============================================================================
// Tests
// ==============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_friend_list_response_serialization() {
        let response = FriendListQueryResponse {
            user_id: "@alice:example.com".to_string(),
            exists: true,
            friend_list_room: Some("!friends:@alice:example.com".to_string()),
            friends: vec!["@bob:example.com".to_string(), "@charlie:other.com".to_string()],
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("@alice:example.com"));
        assert!(json.contains("@charlie:other.com"));
    }

    #[test]
    fn test_friend_relationship_response_serialization() {
        let response = FriendRelationshipResponse {
            user_id: "@alice:example.com".to_string(),
            friend_id: "@bob:example.com".to_string(),
            are_friends: true,
            since: Some(1234567890),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("true"));
        assert!(json.contains("1234567890"));
    }
}
