use crate::services::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::FromRow;
use std::sync::Arc;

#[derive(Debug, Clone, FromRow)]
struct FriendRecord {
    friend_id: String,
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

#[derive(Clone)]
pub struct FriendStorage {
    pool: Arc<sqlx::PgPool>,
}

impl FriendStorage {
    pub fn new(pool: &Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query!(
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
            "#
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query!(
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
            "#
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query!(
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
            "#
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query!(
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
            "#
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_friends(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<FriendRecord> = sqlx::query_as(
            r#"SELECT friend_id FROM friends WHERE user_id = $1 ORDER BY created_ts DESC"#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows.iter().map(|r| r.friend_id.clone()).collect())
    }

    pub async fn add_friend(&self, user_id: &str, friend_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query!(
            r#"
            INSERT INTO friends (user_id, friend_id, created_ts)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id, friend_id) DO NOTHING
            "#,
            user_id,
            friend_id,
            now
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_friend(&self, user_id: &str, friend_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"DELETE FROM friends WHERE user_id = $1 AND friend_id = $2"#,
            user_id,
            friend_id
        )
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

    pub async fn create_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query!(
            r#"
            INSERT INTO friend_requests (from_user_id, to_user_id, message, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $4)
            RETURNING id
            "#,
            sender_id,
            receiver_id,
            message,
            now
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(result.id)
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
        let now = chrono::Utc::now().timestamp();
        let request = sqlx::query!(
            r#"SELECT from_user_id as sender_id FROM friend_requests WHERE id = $1 AND to_user_id = $2"#,
            request_id,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(r) = request {
            sqlx::query!(
                r#"UPDATE friend_requests SET status = 'accepted', updated_ts = $1 WHERE id = $2"#,
                now,
                request_id
            )
            .execute(&*self.pool)
            .await?;

            self.add_friend(user_id, &r.sender_id).await?;
            self.add_friend(&r.sender_id, user_id).await?;
        }
        Ok(())
    }

    pub async fn decline_request(&self, request_id: i64, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE friend_requests SET status = 'declined', updated_ts = $1 WHERE id = $2 AND to_user_id = $3"#,
            chrono::Utc::now().timestamp(), request_id, user_id
        ).execute(&*self.pool).await?;
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
        let result = sqlx::query!(
            r#"INSERT INTO friend_categories (user_id, name, color, created_ts) VALUES ($1, $2, $3, $4) RETURNING id"#,
            user_id, name, color, now
        ).fetch_one(&*self.pool).await?;
        Ok(result.id)
    }

    pub async fn update_category(
        &self,
        category_id: i64,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        if let Some(name) = name {
            sqlx::query!(
                "UPDATE friend_categories SET name = $1 WHERE id = $2",
                name,
                category_id
            )
            .execute(&*self.pool)
            .await?;
        }
        if let Some(color) = color {
            sqlx::query!(
                "UPDATE friend_categories SET color = $1 WHERE id = $2",
                color,
                category_id
            )
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
            sqlx::query!(
                "UPDATE friend_categories SET name = $1 WHERE user_id = $2 AND name = $3",
                name,
                user_id,
                category_name
            )
            .execute(&*self.pool)
            .await?;
        }
        if let Some(color) = new_color {
            sqlx::query!(
                "UPDATE friend_categories SET color = $1 WHERE user_id = $2 AND name = $3",
                color,
                user_id,
                category_name
            )
            .execute(&*self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn delete_category(&self, category_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM friend_categories WHERE id = $1", category_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_category_by_name(
        &self,
        user_id: &str,
        category_name: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM friend_categories WHERE user_id = $1 AND name = $2",
            user_id,
            category_name
        )
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
        sqlx::query!(
            r#"
            INSERT INTO blocked_users (user_id, blocked_user_id, reason, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, blocked_user_id) DO NOTHING
            "#,
            user_id,
            blocked_user_id,
            reason,
            now
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn unblock_user(
        &self,
        user_id: &str,
        blocked_user_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"DELETE FROM blocked_users WHERE user_id = $1 AND blocked_user_id = $2"#,
            user_id,
            blocked_user_id
        )
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
        Ok(rows.iter().map(|r| r.0.clone()).collect())
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
}

#[derive(Debug)]
pub struct RequestInfo {
    pub id: i64,
    pub sender_id: String,
    pub receiver_id: String,
    pub message: Option<String>,
    pub status: String,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CategoryInfo {
    pub id: i64,
    pub user_id: String,
    pub name: String,
    pub color: String,
    pub created_ts: i64,
}

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
        let friends = self
            .friend_storage
            .get_friends(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let mut friend_list = Vec::new();
        for friend_id in friends {
            let registration_service = RegistrationService::new(self.services);
            if let Ok(profile) = registration_service.get_profile(&friend_id).await {
                friend_list.push(profile);
            }
        }

        Ok(json!({
            "friends": friend_list,
            "count": friend_list.len()
        }))
    }

    pub async fn send_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
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

        let mut request_list = Vec::new();
        for req in requests {
            let registration_service = RegistrationService::new(self.services);
            let profile = registration_service
                .get_profile(&req.sender_id)
                .await
                .unwrap_or(json!({
                    "user_id": req.sender_id
                }));
            request_list.push(json!({
                "request_id": req.id,
                "sender": profile,
                "message": req.message,
                "created_ts": req.created_ts
            }));
        }

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
        self.friend_storage
            .remove_friend(user_id, friend_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
        self.friend_storage
            .remove_friend(friend_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
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
        self.friend_storage
            .block_user(user_id, blocked_user_id, reason)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
        self.friend_storage
            .remove_friend(user_id, blocked_user_id)
            .await
            .ok();
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
