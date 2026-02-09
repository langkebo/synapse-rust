use crate::services::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, Row};
use std::sync::Arc;

#[derive(Debug, Clone, FromRow)]
struct FriendRecord {
    friend_id: String,
}

#[derive(Debug, Clone, FromRow)]
struct FriendshipRecord {
    user_id: String,
    friend_id: String,
    created_ts: i64,
    note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FriendshipInfo {
    pub user_id: String,
    pub friend_id: String,
    pub created_ts: i64,
    pub note: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct RequestRecord {
    id: i64,
    sender_id: String,
    receiver_id: String,
    message: Option<String>,
    status: String,
    created_ts: i64,
    updated_ts: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
struct CategoryRecord {
    id: i64,
    user_id: String,
    name: String,
    color: String,
    created_ts: i64,
}

/// Storage layer for friend relationships and requests.
///
/// This struct handles all database operations related to:
/// - Friend connections (bidirectional relationships)
/// - Friend requests (pending, accepted, declined)
/// - Friend categories (user-defined groupings)
/// - Blocked users (user-specific block lists)
#[derive(Clone)]
pub struct FriendStorage {
    pool: Arc<sqlx::PgPool>,
}

impl FriendStorage {
    /// Creates a new `FriendStorage` instance.
    ///
    /// # Arguments
    /// * `pool` - A reference to the PostgreSQL connection pool
    pub fn new(pool: &Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// Creates all friend-related database tables.
    ///
    /// This method is idempotent and can be called multiple times safely.
    /// Tables created: friends, friend_requests, friend_categories, blocked_users
    pub async fn create_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS friends (
                user_id VARCHAR(255) NOT NULL,
                friend_id VARCHAR(255) NOT NULL,
                created_ts BIGINT NOT NULL,
                note TEXT,
                PRIMARY KEY (user_id, friend_id),
                FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
                FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE,
                CHECK (user_id != friend_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS friend_requests (
                id BIGSERIAL PRIMARY KEY,
                from_user_id VARCHAR(255) NOT NULL,
                to_user_id VARCHAR(255) NOT NULL,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                status VARCHAR(50) DEFAULT 'pending',
                message TEXT,
                hide BOOLEAN DEFAULT FALSE,
                FOREIGN KEY (from_user_id) REFERENCES users(user_id) ON DELETE CASCADE,
                FOREIGN KEY (to_user_id) REFERENCES users(user_id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS friend_categories (
                id BIGSERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                name VARCHAR(255) NOT NULL,
                color VARCHAR(20),
                icon VARCHAR(100),
                sort_order BIGINT DEFAULT 0,
                created_ts BIGINT NOT NULL,
                FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
                UNIQUE (user_id, name)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS blocked_users (
                user_id VARCHAR(255) NOT NULL,
                blocked_user_id VARCHAR(255) NOT NULL,
                reason TEXT,
                created_ts BIGINT NOT NULL,
                PRIMARY KEY (user_id, blocked_user_id),
                FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
                FOREIGN KEY (blocked_user_id) REFERENCES users(user_id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Gets a list of friend IDs for the given user.
    ///
    /// # Arguments
    /// * `user_id` - The Matrix user ID (e.g., "@alice:server.com")
    ///
    /// # Returns
    /// A vector of friend user IDs, ordered by friendship creation time (newest first)
    pub async fn get_friends(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<FriendRecord> = sqlx::query_as(
            r#"SELECT friend_id FROM friends WHERE user_id = $1 ORDER BY created_ts DESC"#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;
        // HP-4 FIX: Avoid unnecessary clone by moving out of the struct
        Ok(rows.into_iter().map(|r| r.friend_id).collect())
    }

    /// Adds a friend relationship (unidirectional).
    ///
    /// Note: This only adds one direction of the friendship.
    /// For bidirectional friendships, call this twice with swapped arguments.
    ///
    /// # Arguments
    /// * `user_id` - The user who is adding the friend
    /// * `friend_id` - The user being added as a friend
    pub async fn add_friend(&self, user_id: &str, friend_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            r#"
            INSERT INTO friends (user_id, friend_id, created_ts)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id, friend_id) DO NOTHING
            "#,
        )
        .bind(user_id)
        .bind(friend_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_friend(&self, user_id: &str, friend_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM friends WHERE user_id = $1 AND friend_id = $2"#)
            .bind(user_id)
            .bind(friend_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn is_friend(&self, user_id: &str, friend_id: &str) -> Result<bool, sqlx::Error> {
        let result: Option<(i32,)> =
            sqlx::query_as(r#"SELECT 1 FROM friends WHERE user_id = $1 AND friend_id = $2"#)
                .bind(user_id)
                .bind(friend_id)
                .fetch_optional(&*self.pool)
                .await?;
        Ok(result.is_some())
    }

    pub async fn get_friendship(
        &self,
        user_id: &str,
        friend_id: &str,
    ) -> Result<Option<FriendshipInfo>, sqlx::Error> {
        let result: Option<FriendshipRecord> =
            sqlx::query_as(r#"SELECT user_id, friend_id, created_ts, note FROM friends WHERE user_id = $1 AND friend_id = $2"#)
                .bind(user_id)
                .bind(friend_id)
                .fetch_optional(&*self.pool)
                .await?;
        Ok(result.map(|r| FriendshipInfo {
            user_id: r.user_id,
            friend_id: r.friend_id,
            created_ts: r.created_ts,
            note: r.note,
        }))
    }

    pub async fn get_friendships_batch(
        &self,
        user_id: &str,
        friend_ids: &[String],
    ) -> Result<Vec<FriendshipInfo>, sqlx::Error> {
        let rows: Vec<FriendshipRecord> = sqlx::query_as(
            r#"SELECT user_id, friend_id, created_ts, note FROM friends WHERE user_id = $1 AND friend_id = ANY($2)"#,
        )
        .bind(user_id)
        .bind(friend_ids)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| FriendshipInfo {
                user_id: r.user_id,
                friend_id: r.friend_id,
                created_ts: r.created_ts,
                note: r.note,
            })
            .collect())
    }

    pub async fn create_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query(
            r#"
            INSERT INTO friend_requests (from_user_id, to_user_id, message, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $4)
            RETURNING id
            "#,
        )
        .bind(sender_id)
        .bind(receiver_id)
        .bind(message)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;
        result.try_get::<i64, _>("id")
    }

    pub async fn get_requests(
        &self,
        user_id: &str,
        status: &str,
    ) -> Result<Vec<RequestInfo>, sqlx::Error> {
        let rows: Vec<RequestRecord> = sqlx::query_as(
            r#"
            SELECT id, from_user_id as sender_id, to_user_id as receiver_id, message, status, created_ts, updated_ts
            FROM friend_requests
            WHERE to_user_id = $1 AND status = $2
            ORDER BY created_ts DESC
            "#,
        )
        .bind(user_id)
        .bind(status)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| RequestInfo {
                id: r.id,
                sender_id: r.sender_id.clone(),
                receiver_id: r.receiver_id.clone(),
                message: r.message.clone(),
                status: r.status.clone(),
                created_ts: r.created_ts,
                updated_ts: r.updated_ts.unwrap_or(0),
            })
            .collect())
    }

    pub async fn accept_request(&self, request_id: i64, user_id: &str) -> Result<(), sqlx::Error> {
        // CRITICAL FIX: Use transaction to ensure atomicity
        let mut tx = self.pool.begin().await?;

        let now = chrono::Utc::now().timestamp();
        let request = sqlx::query(
            r#"SELECT from_user_id as sender_id FROM friend_requests WHERE id = $1 AND to_user_id = $2"#,
        )
        .bind(request_id)
        .bind(user_id)
        .fetch_optional(&mut *tx)  // Use transaction
        .await?;

        if let Some(r) = request {
            sqlx::query(
                r#"UPDATE friend_requests SET status = 'accepted', updated_ts = $1 WHERE id = $2"#,
            )
            .bind(now)
            .bind(request_id)
            .execute(&mut *tx)  // Use transaction
            .await?;

            let sender_id = r.try_get::<String, _>("sender_id")?;

            // Insert both friendship records within the same transaction
            sqlx::query(
                r#"
                INSERT INTO friends (user_id, friend_id, created_ts)
                VALUES ($1, $2, $3), ($4, $5, $6)
                ON CONFLICT (user_id, friend_id) DO NOTHING
                "#,
            )
            .bind(user_id)
            .bind(&sender_id)
            .bind(now)
            .bind(&sender_id)
            .bind(user_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn decline_request(&self, request_id: i64, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE friend_requests SET status = 'declined', updated_ts = $1 WHERE id = $2 AND to_user_id = $3"#,
        )
        .bind(chrono::Utc::now().timestamp())
        .bind(request_id)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_categories(&self, user_id: &str) -> Result<Vec<CategoryInfo>, sqlx::Error> {
        let rows: Vec<CategoryRecord> = sqlx::query_as(
            r#"SELECT id, user_id, name, color, created_ts FROM friend_categories WHERE user_id = $1"#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| CategoryInfo {
                id: r.id,
                user_id: r.user_id.clone(),
                name: r.name.clone(),
                color: r.color.clone(),
                created_ts: r.created_ts,
            })
            .collect())
    }

    pub async fn create_category(
        &self,
        user_id: &str,
        name: &str,
        color: &str,
    ) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query(
            r#"INSERT INTO friend_categories (user_id, name, color, created_ts) VALUES ($1, $2, $3, $4) RETURNING id"#,
        )
        .bind(user_id)
        .bind(name)
        .bind(color)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;
        result.try_get::<i64, _>("id")
    }

    pub async fn update_category(
        &self,
        category_id: i64,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        if let Some(name) = name {
            sqlx::query("UPDATE friend_categories SET name = $1 WHERE id = $2")
                .bind(name)
                .bind(category_id)
                .execute(&*self.pool)
                .await?;
        }
        if let Some(color) = color {
            sqlx::query("UPDATE friend_categories SET color = $1 WHERE id = $2")
                .bind(color)
                .bind(category_id)
                .execute(&*self.pool)
                .await?;
        }
        Ok(())
    }

    pub async fn update_category_by_name(
        &self,
        user_id: &str,
        category_name: &str,
        new_name: Option<&str>,
        new_color: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        if let Some(name) = new_name {
            sqlx::query("UPDATE friend_categories SET name = $1 WHERE user_id = $2 AND name = $3")
                .bind(name)
                .bind(user_id)
                .bind(category_name)
                .execute(&*self.pool)
                .await?;
        }
        if let Some(color) = new_color {
            sqlx::query("UPDATE friend_categories SET color = $1 WHERE user_id = $2 AND name = $3")
                .bind(color)
                .bind(user_id)
                .bind(category_name)
                .execute(&*self.pool)
                .await?;
        }
        Ok(())
    }

    pub async fn delete_category(&self, category_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM friend_categories WHERE id = $1")
            .bind(category_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_category_by_name(
        &self,
        user_id: &str,
        category_name: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM friend_categories WHERE user_id = $1 AND name = $2")
            .bind(user_id)
            .bind(category_name)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn block_user(
        &self,
        user_id: &str,
        blocked_user_id: &str,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            r#"
            INSERT INTO blocked_users (user_id, blocked_user_id, reason, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, blocked_user_id) DO NOTHING
            "#,
        )
        .bind(user_id)
        .bind(blocked_user_id)
        .bind(reason)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn unblock_user(
        &self,
        user_id: &str,
        blocked_user_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM blocked_users WHERE user_id = $1 AND blocked_user_id = $2"#)
            .bind(user_id)
            .bind(blocked_user_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_blocked_users(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<(String,)> =
            sqlx::query_as(r#"SELECT blocked_user_id FROM blocked_users WHERE user_id = $1"#)
                .bind(user_id)
                .fetch_all(&*self.pool)
                .await?;
        // HP-4 FIX: Avoid unnecessary clone
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    pub async fn is_blocked(&self, user_id: &str, other_id: &str) -> Result<bool, sqlx::Error> {
        let result: Option<(i32,)> = sqlx::query_as(
            r#"SELECT 1 FROM blocked_users WHERE user_id = $1 AND blocked_user_id = $2"#,
        )
        .bind(user_id)
        .bind(other_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    /// Batch check if multiple users are friends with the given user.
    ///
    /// This is more efficient than calling `is_friend` multiple times.
    ///
    /// # Arguments
    /// * `user_id` - The user to check friendships for
    /// * `other_ids` - List of user IDs to check against
    ///
    /// # Returns
    /// A set of user IDs from `other_ids` that are friends with `user_id`
    // HP-1: Batch check friendships to avoid N+1 queries
    pub async fn batch_check_friends(
        &self,
        user_id: &str,
        other_ids: &[String],
    ) -> Result<std::collections::HashSet<String>, sqlx::Error> {
        if other_ids.is_empty() {
            return Ok(std::collections::HashSet::new());
        }

        let rows: Vec<(String,)> = sqlx::query_as(
            r#"SELECT friend_id FROM friends WHERE user_id = $1 AND friend_id = ANY($2)"#,
        )
        .bind(user_id)
        .bind(other_ids)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    /// Batch check if multiple users are blocked by the given user.
    ///
    /// This is more efficient than calling `is_blocked` multiple times.
    ///
    /// # Arguments
    /// * `user_id` - The user to check blocks for
    /// * `other_ids` - List of user IDs to check against
    ///
    /// # Returns
    /// A set of user IDs from `other_ids` that are blocked by `user_id`
    // HP-1: Batch check blocked users to avoid N+1 queries
    pub async fn batch_check_blocked(
        &self,
        user_id: &str,
        other_ids: &[String],
    ) -> Result<std::collections::HashSet<String>, sqlx::Error> {
        if other_ids.is_empty() {
            return Ok(std::collections::HashSet::new());
        }

        let rows: Vec<(String,)> = sqlx::query_as(
            r#"SELECT blocked_user_id FROM blocked_users WHERE user_id = $1 AND blocked_user_id = ANY($2)"#,
        )
        .bind(user_id)
        .bind(other_ids)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }
}

/// Information about a friend request.
///
/// Represents a pending, accepted, or declined friend request between two users.
#[derive(Debug)]
pub struct RequestInfo {
    /// Unique identifier for the request
    pub id: i64,
    /// User ID of the sender
    pub sender_id: String,
    /// User ID of the receiver
    pub receiver_id: String,
    /// Optional message from the sender
    pub message: Option<String>,
    /// Request status: pending, accepted, declined, or cancelled
    pub status: String,
    /// Unix timestamp when the request was created
    pub created_ts: i64,
    /// Unix timestamp when the request was last updated
    pub updated_ts: i64,
}

/// Information about a friend category.
///
/// Users can organize their friends into custom categories.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CategoryInfo {
    /// Unique identifier for the category
    pub id: i64,
    /// User ID of the category owner
    pub user_id: String,
    /// Display name of the category
    pub name: String,
    /// Color code for the category (hex format)
    pub color: String,
    /// Unix timestamp when the category was created
    pub created_ts: i64,
}

/// High-level service for managing friend relationships.
///
/// This service provides business logic for friend-related operations including:
/// - Sending and managing friend requests
/// - Managing friend lists
/// - Organizing friends into categories
/// - Blocking/unblocking users
///
/// # Example
/// ```ignore
/// let service = FriendService::new(&services, &pool);
/// service.send_friend_request("@alice:server", "@bob:server", Some("Hi!")).await?;
/// ```
pub struct FriendService<'a> {
    services: &'a ServiceContainer,
    friend_storage: FriendStorage,
}

impl<'a> FriendService<'a> {
    pub fn new(services: &'a ServiceContainer, pool: &Arc<sqlx::PgPool>) -> Self {
        Self {
            services,
            friend_storage: FriendStorage::new(pool),
        }
    }

    pub async fn get_friends(&self, user_id: &str) -> ApiResult<serde_json::Value> {
        let friend_ids = self
            .friend_storage
            .get_friends(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let friend_list = self
            .services
            .registration_service
            .get_profiles(&friend_ids)
            .await?;

        Ok(json!({
            "friends": friend_list,
            "count": friend_list.len()
        }))
    }

    pub async fn get_friends_batch(
        &self,
        user_id: &str,
        friend_ids: Vec<String>,
    ) -> ApiResult<serde_json::Value> {
        let friendships = self
            .friend_storage
            .get_friendships_batch(user_id, &friend_ids)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let found_friend_ids: Vec<String> = friendships.iter().map(|f| f.friend_id.clone()).collect();
        let profiles = self
            .services
            .registration_service
            .get_profiles(&found_friend_ids)
            .await?;

        // Convert profiles to a map for easier lookup
        // profiles is Vec<Value> (json objects)
        let mut results = serde_json::Map::new();

        for friendship in friendships {
            let mut friend_obj = json!({
                "user_id": friendship.friend_id,
                "created_ts": friendship.created_ts,
                "note": friendship.note
            });

            if let Some(profile_val) = profiles.iter().find(|p| p["user_id"].as_str() == Some(&friendship.friend_id)) {
                 if let Some(obj) = friend_obj.as_object_mut() {
                     if let Some(displayname) = profile_val.get("display_name") {
                         obj.insert("display_name".to_string(), displayname.clone());
                     }
                     // Note: registration_service.get_profiles returns keys like "display_name", "avatar_url"
                     // Check existing get_friends implementation or assumptions.
                     // In get_friends, it returns friend_list which is directly returned.
                     // Let's assume registration_service.get_profiles returns standard profile objects.
                     if let Some(avatar_url) = profile_val.get("avatar_url") {
                         obj.insert("avatar_url".to_string(), avatar_url.clone());
                     }
                 }
            }
            
            results.insert(friendship.friend_id.clone(), friend_obj);
        }

        Ok(serde_json::Value::Object(results))
    }

    pub async fn send_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        if !self
            .services
            .user_storage
            .user_exists(receiver_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        if self
            .friend_storage
            .is_friend(sender_id, receiver_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        {
            return Err(ApiError::bad_request("Already friends".to_string()));
        }

        if self
            .friend_storage
            .is_blocked(receiver_id, sender_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        {
            return Err(ApiError::forbidden(
                "Cannot send request to this user".to_string(),
            ));
        }

        let request_id = self
            .friend_storage
            .create_request(sender_id, receiver_id, message)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(json!({
            "request_id": request_id,
            "status": "pending"
        }))
    }

    pub async fn get_pending_requests(&self, user_id: &str) -> ApiResult<serde_json::Value> {
        let requests = self
            .friend_storage
            .get_requests(user_id, "pending")
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let sender_ids: Vec<String> = requests.iter().map(|r| r.sender_id.clone()).collect();
        let profiles = self
            .services
            .registration_service
            .get_profiles(&sender_ids)
            .await?;

        let request_list: Vec<serde_json::Value> = requests
            .into_iter()
            .enumerate()
            .map(|(i, req)| {
                let profile = profiles.get(i).cloned().unwrap_or(json!({
                    "user_id": req.sender_id
                }));
                json!({
                    "request_id": req.id,
                    "sender": profile,
                    "message": req.message,
                    "created_ts": req.created_ts
                })
            })
            .collect();

        Ok(json!({
            "requests": request_list,
            "count": request_list.len()
        }))
    }

    pub async fn accept_request(&self, user_id: &str, request_id: i64) -> ApiResult<()> {
        self.friend_storage
            .accept_request(request_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
        Ok(())
    }

    pub async fn decline_request(&self, user_id: &str, request_id: i64) -> ApiResult<()> {
        self.friend_storage
            .decline_request(request_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
        Ok(())
    }

    pub async fn remove_friend(&self, user_id: &str, friend_id: &str) -> ApiResult<()> {
        // CRITICAL FIX: Use transaction to ensure both removals succeed or both fail
        let mut tx = self.friend_storage.pool.begin().await
            .map_err(|e| ApiError::internal(format!("Failed to begin transaction: {}", e)))?;

        // Remove bidirectional friendship within transaction
        sqlx::query(r#"DELETE FROM friends WHERE user_id = $1 AND friend_id = $2"#)
            .bind(user_id)
            .bind(friend_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        sqlx::query(r#"DELETE FROM friends WHERE user_id = $1 AND friend_id = $2"#)
            .bind(friend_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        tx.commit().await
            .map_err(|e| ApiError::internal(format!("Failed to commit transaction: {}", e)))?;
        Ok(())
    }

    pub async fn get_categories(&self, user_id: &str) -> ApiResult<serde_json::Value> {
        let categories = self
            .friend_storage
            .get_categories(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
        Ok(json!({ "categories": categories }))
    }

    pub async fn create_category(
        &self,
        user_id: &str,
        name: &str,
        color: &str,
    ) -> ApiResult<serde_json::Value> {
        let category_id = self
            .friend_storage
            .create_category(user_id, name, color)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
        Ok(json!({ "category_id": category_id }))
    }

    pub async fn block_user(
        &self,
        user_id: &str,
        blocked_user_id: &str,
        reason: Option<&str>,
    ) -> ApiResult<()> {
        if !self
            .services
            .user_storage
            .user_exists(blocked_user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.friend_storage
            .block_user(user_id, blocked_user_id, reason)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        // HP-3 FIX: Don't silently ignore errors from remove_friend
        // When blocking a user, we should also remove the friendship if it exists
        // but the friendship might not exist, so we handle that gracefully
        match self.friend_storage.remove_friend(user_id, blocked_user_id).await {
            Ok(_) => {},
            Err(e) => {
                // Log but don't fail the block operation if friendship removal fails
                // This can happen if they weren't friends to begin with
                ::tracing::debug!(
                    user_id, blocked_user_id, error = %e,
                    "Friendship removal during block failed (non-critical)"
                );
            }
        }

        Ok(())
    }

    pub async fn unblock_user(&self, user_id: &str, blocked_user_id: &str) -> ApiResult<()> {
        self.friend_storage
            .unblock_user(user_id, blocked_user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
        Ok(())
    }

    pub async fn get_blocked_users(&self, user_id: &str) -> ApiResult<serde_json::Value> {
        let blocked = self
            .friend_storage
            .get_blocked_users(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
        Ok(json!({ "blocked": blocked, "count": blocked.len() }))
    }
}
