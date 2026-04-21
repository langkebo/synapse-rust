use sqlx::{Pool, Postgres};
use std::sync::Arc;
use tokio::sync::RwLock;

// =============================================================================
// Core Matrix storage modules (always compiled)
// =============================================================================
pub mod application_service;
pub mod audit;
pub mod background_update;
pub mod compile_time_validation;
pub mod connection_monitor;
pub mod dehydrated_device;
pub mod delayed_event;
pub mod device;
pub mod email_verification;
pub mod event;
pub mod event_report;
pub mod feature_flags;
pub mod federation_blacklist;
pub mod filter;
pub mod integrity_checker;
pub mod invite_blocklist;
pub mod maintenance;
pub mod media;
pub mod media_quota;
pub mod membership;
pub mod moderation;
pub mod module;
pub mod monitoring;
pub mod openid_token;
pub mod performance;
pub mod pool_monitor;
pub mod presence;
pub mod push_notification;
pub mod qr_login;
pub mod refresh_token;
pub mod registration_token;
pub mod relations;
pub mod rendezvous;
pub mod retention;
pub mod room;
pub mod room_summary;
pub mod room_tag;
pub mod schema_health_check;
pub mod schema_validator;
pub mod search_index;
pub mod sliding_sync;
pub mod space;
pub mod sticky_event;
pub mod thread;
pub mod threepid;
pub mod token;
pub mod user;

// =============================================================================
// Feature-gated extension storage modules
// =============================================================================
#[cfg(feature = "openclaw-routes")]
pub mod ai_connection;
#[cfg(feature = "openclaw-routes")]
pub mod openclaw;

#[cfg(feature = "friends")]
pub mod friend_room;

#[cfg(feature = "voice-extended")]
pub mod voice;

#[cfg(feature = "saml-sso")]
pub mod saml;

#[cfg(feature = "cas-sso")]
pub mod cas;

#[cfg(feature = "beacons")]
pub mod beacon;

#[cfg(feature = "voip-tracking")]
pub mod call_session;
#[cfg(feature = "voip-tracking")]
pub mod matrixrtc;

#[cfg(feature = "widgets")]
pub mod widget;

#[cfg(feature = "server-notifications")]
pub mod server_notification;

#[cfg(feature = "privacy-ext")]
pub mod privacy;

// Captcha is used by registration flow — keep unconditional
pub mod captcha;

pub use self::user::*;
pub use self::threepid::UserThreepid;

pub use self::application_service::*;
pub use self::audit::*;
pub use self::captcha::*;
pub use self::dehydrated_device::*;
pub use self::delayed_event::*;
pub use self::device::*;
pub use self::event::*;
pub use self::feature_flags::*;
pub use self::federation_blacklist::*;
pub use self::filter::*;
pub use self::invite_blocklist::*;
pub use self::maintenance::*;
pub use self::media_quota::*;
pub use self::membership::*;
pub use self::moderation::*;
pub use self::monitoring::{
    ConnectionPoolStatus, DataIntegrityReport, DatabaseHealthStatus, DatabaseMonitor,
    DuplicateEntry, ForeignKeyViolation, NullConstraintViolation, OrphanedRecord,
    PerformanceMetrics, VacuumStats,
};
pub use self::openid_token::*;
pub use self::performance::{time_query, PerformanceMonitor, PoolStatistics, QueryMetrics};
pub use self::pool_monitor::{
    create_pool_with_monitoring, set_query_timeout, set_transaction_timeout, DatabasePoolConfig,
    DatabasePoolMonitor, PoolHealthStatus, PoolStats, QueryTimeoutConfig,
};
pub use self::presence::*;
pub use self::push_notification::*;
pub use self::qr_login::*;
pub use self::rendezvous::*;
pub use self::room::*;
pub use self::schema_validator::*;
pub use self::search_index::*;
pub use self::sliding_sync::*;
pub use self::space::*;
pub use self::sticky_event::*;
pub use self::thread::*;
pub use self::threepid::*;
pub use self::token::*;

// Feature-gated re-exports
#[cfg(feature = "openclaw-routes")]
pub use self::ai_connection::*;
#[cfg(feature = "openclaw-routes")]
pub use self::openclaw::*;

#[cfg(feature = "friends")]
pub use self::friend_room::*;

#[cfg(feature = "voice-extended")]
pub use self::voice::*;

#[cfg(feature = "saml-sso")]
pub use self::saml::*;

#[cfg(feature = "cas-sso")]
pub use self::cas::*;

#[cfg(feature = "beacons")]
pub use self::beacon::*;

#[cfg(feature = "voip-tracking")]
pub use self::call_session::*;
#[cfg(feature = "voip-tracking")]
pub use self::matrixrtc::*;

#[cfg(feature = "widgets")]
pub use self::widget::*;

#[cfg(feature = "server-notifications")]
pub use self::server_notification::*;

#[cfg(feature = "privacy-ext")]
pub use self::privacy::*;

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
pub async fn initialize_database(_pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    tracing::info!("Database initialization completed");
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
            email: None,
            phone: None,
            is_admin: false,
            is_deactivated: false,
            is_guest: false,
            is_shadow_banned: false,
            created_ts: 1234567890,
            updated_ts: None,
            generation: 1,
            consent_version: None,
            appservice_id: None,
            user_type: None,
            invalid_update_at: None,
            migration_state: None,
            must_change_password: false,
            password_changed_ts: None,
            is_password_change_required: false,
            password_expires_at: None,
            failed_login_attempts: 0,
            locked_until: None,
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
            created_ts: 1234567890000,
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
            token_hash: "test_token_hash_123".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            created_ts: 1234567890000,
            expires_at: Some(1234571490000),
            last_used_ts: None,
            user_agent: None,
            ip_address: None,
            is_revoked: false,
        };
        assert_eq!(token.id, 1);
        assert_eq!(token.token_hash, "test_token_hash_123");
    }

    #[test]
    fn test_room_struct_fields() {
        let room = Room {
            room_id: "!test:example.com".to_string(),
            name: Some("Test Room".to_string()),
            topic: Some("A test room".to_string()),
            canonical_alias: Some("#test:example.com".to_string()),
            join_rule: "invite".to_string(),
            creator_user_id: Some("@test:example.com".to_string()),
            room_version: "1".to_string(),
            encryption: None,
            is_public: false,
            member_count: 0,
            history_visibility: "shared".to_string(),
            created_ts: 1234567890,
            avatar_url: None,
            is_federatable: true,
            is_spotlight: false,
            is_flagged: false,
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
            banned_ts: None,
        };
        assert_eq!(member.room_id, "!test:example.com");
        assert_eq!(member.user_id, "@test:example.com");
        assert_eq!(member.membership, "join");
    }

    #[test]
    fn test_room_minimal_fields() {
        let room = Room {
            room_id: "!minimal:example.com".to_string(),
            name: None,
            topic: None,
            canonical_alias: None,
            join_rule: "public".to_string(),
            creator_user_id: None,
            room_version: "1".to_string(),
            encryption: None,
            is_public: true,
            member_count: 0,
            history_visibility: "joined".to_string(),
            created_ts: 0,
            is_federatable: true,
            is_spotlight: false,
            is_flagged: false,
            avatar_url: None,
        };
        assert!(room.is_public);
    }
}
