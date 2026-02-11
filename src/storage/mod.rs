use sqlx::{Pool, Postgres};
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod device;
pub mod email_verification;
pub mod event;
pub mod maintenance;
pub mod membership;
pub mod monitoring;
pub mod performance;
pub mod room;
pub mod schema_validator;
pub mod token;
pub mod user;
pub mod voice;

pub use self::device::*;
pub use self::event::*;
pub use self::maintenance::*;
pub use self::membership::*;
pub use self::monitoring::{
    ConnectionPoolStatus, DataIntegrityReport, DatabaseHealthStatus, DatabaseMonitor,
    DuplicateEntry, ForeignKeyViolation, NullConstraintViolation, OrphanedRecord,
    PerformanceMetrics, VacuumStats,
};
pub use self::performance::{PerformanceMonitor, PoolStatistics, QueryMetrics, time_query};
pub use self::room::*;
pub use self::schema_validator::*;
pub use self::token::*;
pub use self::user::*;
pub use self::voice::*;

/// 数据库结构体。
///
/// Matrix Homeserver 的数据库访问层，封装 PostgreSQL 连接池和监控功能。
/// 提供数据库连接管理、健康检查、性能监控等功能。
pub struct Database {
    /// PostgreSQL 连接池
    pub pool: Pool<Postgres>,
    /// 数据库监控器
    pub monitor: Arc<RwLock<DatabaseMonitor>>,
}

impl Database {
    /// 创建新的数据库实例。
    ///
    /// 建立与 PostgreSQL 数据库的连接并初始化监控器。
    ///
    /// # 参数
    ///
    /// * `database_url` - PostgreSQL 连接 URL，如 "postgresql://user:pass@localhost/dbname"
    ///
    /// # 返回值
    ///
    /// 成功时返回 `Ok(Database)`，连接失败时返回 `Err(sqlx::Error)`
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = sqlx::PgPool::connect(database_url).await?;
        let monitor = Arc::new(RwLock::new(DatabaseMonitor::new(pool.clone(), 10000)));
        Ok(Self { pool, monitor })
    }

    /// 从现有连接池创建数据库实例。
    ///
    /// 用于复用已创建的连接池场景。
    ///
    /// # 参数
    ///
    /// * `pool` - 已存在的 PostgreSQL 连接池
    ///
    /// # 返回值
    ///
    /// 返回使用给定连接池的 `Database` 实例
    pub fn from_pool(pool: Pool<Postgres>) -> Self {
        let monitor = Arc::new(RwLock::new(DatabaseMonitor::new(pool.clone(), 10000)));
        Self { pool, monitor }
    }

    /// 获取数据库连接池引用。
    ///
    /// # 返回值
    ///
    /// 返回 PostgreSQL 连接池的不可变引用
    pub fn pool(&self) -> &Pool<Postgres> {
        &self.pool
    }

    /// 执行数据库健康检查。
    ///
    /// 检查数据库连接、表存在性、索引状态等。
    ///
    /// # 返回值
    ///
    /// 成功时返回 `Ok(DatabaseHealthStatus)`，包含详细健康信息
    /// 失败时返回 `Err(sqlx::Error)`
    pub async fn health_check(&self) -> Result<DatabaseHealthStatus, sqlx::Error> {
        self.monitor.write().await.get_full_health_status().await
    }

    /// 获取性能指标。
    ///
    /// 返回数据库连接池使用情况、查询延迟等性能数据。
    ///
    /// # 返回值
    ///
    /// 成功时返回 `Ok(PerformanceMetrics)`，包含性能数据
    /// 失败时返回 `Err(sqlx::Error)`
    pub async fn get_performance_metrics(&self) -> Result<PerformanceMetrics, sqlx::Error> {
        self.monitor.write().await.get_performance_metrics().await
    }

    /// 验证数据完整性。
    ///
    /// 检查外键约束、空值约束、孤立记录等数据完整性问题。
    ///
    /// # 返回值
    ///
    /// 成功时返回 `Ok(DataIntegrityReport)`，包含完整性检查报告
    /// 失败时返回 `Err(sqlx::Error)`
    pub async fn verify_data_integrity(&self) -> Result<DataIntegrityReport, sqlx::Error> {
        self.monitor.write().await.verify_data_integrity().await
    }
}

/// 初始化数据库 schema。
///
/// 使用 sqlx 迁移工具应用数据库变更。
///
/// # 参数
///
/// * `pool` - PostgreSQL 连接池
///
/// # 返回值
///
/// 成功时返回 `Ok(())`，失败时返回 `Err(sqlx::Error)`
pub async fn initialize_database(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_struct_creation() {
        let db_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://synapse:synapse@localhost:5432/synapse_test".to_string()
        });
        let pool = match sqlx::PgPool::connect(&db_url).await {
            Ok(p) => p,
            Err(_) => return,
        };
        let _db = Database {
            pool: pool.clone(),
            monitor: Arc::new(RwLock::new(DatabaseMonitor::new(pool, 50))),
        };
    }

    #[test]
    fn test_user_struct_fields() {
        let user = User {
            user_id: "@test:example.com".to_string(),
            username: "testuser".to_string(),
            password_hash: Some("hash123".to_string()),
            displayname: Some("Test User".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            is_admin: Some(false),
            deactivated: Some(false),
            is_guest: Some(false),
            consent_version: None,
            appservice_id: None,
            user_type: None,
            shadow_banned: Some(false),
            generation: 1,
            invalid_update_ts: None,
            migration_state: None,
            creation_ts: 1234567890,
            updated_ts: None,
        };
        assert_eq!(user.user_id, "@test:example.com");
        assert_eq!(user.username, "testuser");
    }

    #[test]
    fn test_device_struct_fields() {
        let device = Device {
            device_id: "DEVICE123".to_string(),
            user_id: "@test:example.com".to_string(),
            display_name: Some("My Device".to_string()),
            last_seen_ts: Some(1234567890000),
            last_seen_ip: Some("192.168.1.1".to_string()),
            created_at: 1234567890000,
            created_ts: Some(1234567890000),
            device_key: None,
            ignored_user_list: None,
            appservice_id: None,
            first_seen_ts: 1234567890000,
        };
        assert_eq!(device.device_id, "DEVICE123");
        assert_eq!(device.user_id, "@test:example.com");
    }

    #[test]
    fn test_access_token_struct_fields() {
        let token = AccessToken {
            id: 1,
            token: "test_token_123".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            created_ts: 1234567890,
            expires_ts: 1234571490,
            invalidated_ts: None,
        };
        assert_eq!(token.id, 1);
        assert_eq!(token.token, "test_token_123");
    }

    #[test]
    fn test_refresh_token_struct_fields() {
        let token = RefreshToken {
            id: 1,
            token: "refresh_token_123".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            created_ts: 1234567890,
            expires_ts: 1235171490,
            invalidated_ts: None,
        };
        assert_eq!(token.id, 1);
        assert_eq!(token.token, "refresh_token_123");
    }

    #[test]
    fn test_room_struct_fields() {
        let room = Room {
            room_id: "!test:example.com".to_string(),
            name: Some("Test Room".to_string()),
            topic: Some("A test room".to_string()),
            canonical_alias: Some("#test:example.com".to_string()),
            join_rule: "invite".to_string(),
            creator: "@test:example.com".to_string(),
            version: "1".to_string(),
            encryption: None,
            is_public: false,
            member_count: 0,
            history_visibility: "shared".to_string(),
            creation_ts: 1234567890,
            avatar_url: None,
        };
        assert_eq!(room.room_id, "!test:example.com");
        assert_eq!(room.join_rule, "invite");
        assert!(!room.is_public);
    }

    #[test]
    fn test_room_event_struct_fields() {
        let event = RoomEvent {
            event_id: "$test_event".to_string(),
            room_id: "!test:example.com".to_string(),
            user_id: "@test:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::from_str(r#"{"body":"Hello","msgtype":"m.text"}"#).unwrap(),
            state_key: None,
            depth: 1,
            origin_server_ts: 1234567890000,
            processed_ts: 1234567890,
            not_before: 0,
            status: None,
            reference_image: None,
            origin: "example.com".to_string(),
        };
        assert_eq!(event.event_id, "$test_event");
        assert_eq!(event.room_id, "!test:example.com");
        assert_eq!(event.event_type, "m.room.message");
    }

    #[test]
    fn test_room_member_struct_fields() {
        let member = RoomMember {
            room_id: "!test:example.com".to_string(),
            user_id: "@test:example.com".to_string(),
            display_name: Some("Test User".to_string()),
            membership: "join".to_string(),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            join_reason: Some("Joined via invite".to_string()),
            banned_by: None,
            sender: None,
            event_id: Some("$test_event:example.com".to_string()),
            event_type: None,
            is_banned: Some(false),
            invite_token: None,
            updated_ts: None,
            joined_ts: Some(1234567890000),
            left_ts: None,
            reason: None,
            ban_reason: None,
            ban_ts: None,
        };
        assert_eq!(member.room_id, "!test:example.com");
        assert_eq!(member.user_id, "@test:example.com");
        assert_eq!(member.membership, "join");
    }
}
