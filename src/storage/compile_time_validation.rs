//! 编译期数据库验证模块
//!
//! 提供数据库 Schema 与代码模型一致性验证
//!
//! # 验证方式
//!
//! 1. **运行时验证** - 使用 `query_as` + 类型标注，查询错误会在运行时捕获
//! 2. **启动时验证** - 使用 `schema_health_check` 模块验证表/列/索引
//!
//! # 使用方法
//!
//! ```rust
//! use synapse_rust::storage::compile_time_validation::*;
//!
//! // 查询用户
//! async fn get_user(pool: &sqlx::PgPool, user_id: &str) -> Result<Option<User>, sqlx::Error> {
//!     query_user(pool, user_id).await
//! }
//! ```

use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;

/// 用户模型
///
/// 注意：字段名必须与数据库列名完全匹配
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct User {
    pub user_id: String,
    pub username: String,
    pub created_ts: i64,
    pub is_deactivated: bool,
}

/// 设备模型
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Device {
    pub device_id: String,
    pub user_id: String,
    pub last_seen_ts: Option<i64>,
    pub display_name: Option<String>,
}

/// 房间模型
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Room {
    pub room_id: String,
    pub creator: Option<String>,
    pub created_ts: i64,
    pub is_public: bool,
}

/// 事件模型
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub origin_server_ts: i64,
    #[sqlx(rename = "type")]
    pub event_type: String,
}

/// 成员资格模型
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Membership {
    pub room_id: String,
    pub user_id: String,
    pub membership: String,
    pub joined_ts: Option<i64>,
    pub invited_ts: Option<i64>,
    pub left_ts: Option<i64>,
}

/// 用户 threepid 模型
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserThreepid {
    pub id: i64,
    pub user_id: String,
    pub medium: String,
    pub address: String,
    pub validated_ts: Option<i64>,
    pub added_ts: i64,
    pub is_verified: bool,
}

/// 查询用户
pub async fn query_user(pool: &PgPool, user_id: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as(
        "SELECT user_id, username, created_ts, is_deactivated FROM users WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// 查询用户设备列表
pub async fn query_user_devices(pool: &PgPool, user_id: &str) -> Result<Vec<Device>, sqlx::Error> {
    sqlx::query_as(
        "SELECT device_id, user_id, last_seen_ts, display_name FROM devices WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// 查询房间详情
pub async fn query_room(pool: &PgPool, room_id: &str) -> Result<Option<Room>, sqlx::Error> {
    sqlx::query_as("SELECT room_id, creator, created_ts, is_public FROM rooms WHERE room_id = $1")
        .bind(room_id)
        .fetch_optional(pool)
        .await
}

/// 查询房间事件
pub async fn query_room_events(
    pool: &PgPool,
    room_id: &str,
    limit: i64,
) -> Result<Vec<Event>, sqlx::Error> {
    sqlx::query_as(
        "SELECT event_id, room_id, user_id, origin_server_ts, type FROM events 
         WHERE room_id = $1 ORDER BY origin_server_ts DESC LIMIT $2",
    )
    .bind(room_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// 查询房间成员
pub async fn query_room_members(
    pool: &PgPool,
    room_id: &str,
    membership: &str,
) -> Result<Vec<Membership>, sqlx::Error> {
    sqlx::query_as(
        "SELECT room_id, user_id, membership, joined_ts, invited_ts, left_ts 
         FROM room_memberships WHERE room_id = $1 AND membership = $2",
    )
    .bind(room_id)
    .bind(membership)
    .fetch_all(pool)
    .await
}

/// 查询用户 threepid
pub async fn query_user_threepids(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<UserThreepid>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, user_id, medium, address, validated_ts, added_ts, is_verified 
         FROM user_threepids WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// 统计用户房间数
pub async fn count_user_rooms(pool: &PgPool, user_id: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM room_memberships WHERE user_id = $1 AND membership = 'join'",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
}

/// 验证用户存在
pub async fn user_exists(pool: &PgPool, user_id: &str) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await?;

    Ok(count > 0)
}

/// 验证房间存在
pub async fn room_exists(pool: &PgPool, room_id: &str) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms WHERE room_id = $1")
        .bind(room_id)
        .fetch_one(pool)
        .await?;

    Ok(count > 0)
}

/// 验证设备存在
pub async fn device_exists(pool: &PgPool, device_id: &str) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE device_id = $1")
        .bind(device_id)
        .fetch_one(pool)
        .await?;

    Ok(count > 0)
}

/// ============ 测试 ============
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_structure() {
        let user = User {
            user_id: "@test:example.com".to_string(),
            username: "test".to_string(),
            created_ts: 1234567890,
            is_deactivated: false,
        };

        assert_eq!(user.user_id, "@test:example.com");
        assert!(!user.is_deactivated);
    }

    #[test]
    fn test_device_structure() {
        let device = Device {
            device_id: "DEVICE123".to_string(),
            user_id: "@test:example.com".to_string(),
            last_seen_ts: Some(1234567890),
            display_name: Some("My Device".to_string()),
        };

        assert!(device.last_seen_ts.is_some());
    }

    #[test]
    fn test_room_structure() {
        let room = Room {
            room_id: "!room:example.com".to_string(),
            creator: Some("@admin:example.com".to_string()),
            created_ts: 1234567890,
            is_public: true,
        };

        assert!(room.is_public);
    }

    #[test]
    fn test_membership_structure() {
        let membership = Membership {
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            membership: "join".to_string(),
            joined_ts: Some(1234567890),
            invited_ts: None,
            left_ts: None,
        };

        assert_eq!(membership.membership, "join");
        assert!(membership.joined_ts.is_some());
    }

    #[test]
    fn test_user_threepid_structure() {
        let threepid = UserThreepid {
            id: 1,
            user_id: "@test:example.com".to_string(),
            medium: "email".to_string(),
            address: "test@example.com".to_string(),
            validated_ts: Some(1234567890),
            added_ts: 1234567800,
            is_verified: true,
        };

        assert_eq!(threepid.medium, "email");
        assert!(threepid.is_verified);
    }
}
