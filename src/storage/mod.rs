use sqlx::{Pool, Postgres};
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod device;
pub mod event;
pub mod maintenance;
pub mod membership;
pub mod monitoring;
pub mod private_chat;
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
pub use self::private_chat::*;
pub use self::room::*;
pub use self::schema_validator::*;
pub use self::token::*;
pub use self::user::*;
pub use self::voice::*;

pub struct Database {
    pub pool: Pool<Postgres>,
    pub monitor: Arc<RwLock<DatabaseMonitor>>,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = sqlx::PgPool::connect(database_url).await?;
        let monitor = Arc::new(RwLock::new(DatabaseMonitor::new(pool.clone(), 10000)));
        Ok(Self { pool, monitor })
    }

    pub fn from_pool(pool: Pool<Postgres>) -> Self {
        let monitor = Arc::new(RwLock::new(DatabaseMonitor::new(pool.clone(), 10000)));
        Self { pool, monitor }
    }

    pub fn pool(&self) -> &Pool<Postgres> {
        &self.pool
    }

    pub async fn health_check(&self) -> Result<DatabaseHealthStatus, sqlx::Error> {
        self.monitor.write().await.get_full_health_status().await
    }

    pub async fn get_performance_metrics(&self) -> Result<PerformanceMetrics, sqlx::Error> {
        self.monitor.write().await.get_performance_metrics().await
    }

    pub async fn verify_data_integrity(&self) -> Result<DataIntegrityReport, sqlx::Error> {
        self.monitor.write().await.verify_data_integrity().await
    }
}

pub async fn initialize_database(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            user_id TEXT NOT NULL PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT,
            is_admin BOOLEAN DEFAULT FALSE,
            is_guest BOOLEAN DEFAULT FALSE,
            consent_version TEXT,
            appservice_id TEXT,
            creation_ts BIGINT NOT NULL,
            user_type TEXT,
            deactivated BOOLEAN DEFAULT FALSE,
            shadow_banned BOOLEAN DEFAULT FALSE,
            generation BIGINT NOT NULL,
            avatar_url TEXT,
            displayname TEXT,
            invalid_update_ts BIGINT,
            migration_state TEXT
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS devices (
            device_id TEXT NOT NULL PRIMARY KEY,
            user_id TEXT NOT NULL,
            display_name TEXT,
            last_seen_ts TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            last_seen_ip TEXT,
            created_ts BIGINT NOT NULL,
            user_agent TEXT,
            keys JSONB,
            device_display_name TEXT,
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS access_tokens (
            id BIGSERIAL PRIMARY KEY,
            token TEXT NOT NULL UNIQUE,
            user_id TEXT NOT NULL,
            device_id TEXT,
            created_ts BIGINT NOT NULL,
            expired_ts BIGINT,
            invalidated BOOLEAN DEFAULT FALSE,
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
            FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS refresh_tokens (
            id BIGSERIAL PRIMARY KEY,
            token TEXT NOT NULL UNIQUE,
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            created_ts BIGINT NOT NULL,
            expired_ts BIGINT,
            invalidated BOOLEAN DEFAULT FALSE,
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
            FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS rooms (
            room_id TEXT NOT NULL PRIMARY KEY,
            is_public BOOLEAN NOT NULL DEFAULT FALSE,
            creator TEXT NOT NULL,
            creation_ts BIGINT NOT NULL,
            federate BOOLEAN NOT NULL DEFAULT TRUE,
            version TEXT NOT NULL DEFAULT '1',
            name TEXT,
            topic TEXT,
            avatar TEXT,
            canonical_alias TEXT,
            guest_access BOOLEAN DEFAULT FALSE,
            history_visibility TEXT DEFAULT 'shared',
            encryption TEXT,
            is_flaged BOOLEAN DEFAULT FALSE,
            is_spotlight BOOLEAN DEFAULT FALSE,
            deleted_ts BIGINT,
            join_rule TEXT,
            visibility TEXT
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS room_memberships (
            room_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            sender TEXT NOT NULL,
            membership TEXT NOT NULL,
            event_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            display_name TEXT,
            avatar_url TEXT,
            is_banned BOOLEAN DEFAULT FALSE,
            invite_token TEXT,
            inviter TEXT,
            updated_ts BIGINT,
            joined_ts BIGINT,
            left_ts BIGINT,
            reason TEXT,
            banned_by TEXT,
            ban_reason TEXT,
            ban_ts BIGINT,
            join_reason TEXT,
            PRIMARY KEY (room_id, user_id),
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS room_events (
            event_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            content TEXT NOT NULL,
            state_key TEXT,
            depth BIGINT NOT NULL DEFAULT 0,
            origin_server_ts BIGINT NOT NULL,
            processed_ts BIGINT NOT NULL,
            not_before BIGINT DEFAULT 0,
            status TEXT DEFAULT NULL,
            reference_image TEXT,
            origin TEXT NOT NULL,
            sender TEXT NOT NULL,
            unsigned TEXT,
            redacted BOOLEAN DEFAULT FALSE,
            PRIMARY KEY (event_id),
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS invalidated BOOLEAN DEFAULT FALSE",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS invalidated BOOLEAN DEFAULT FALSE",
    )
    .execute(pool)
    .await?;
    sqlx::query("ALTER TABLE room_memberships ADD COLUMN IF NOT EXISTS banned_by TEXT")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE room_memberships ADD COLUMN IF NOT EXISTS ban_reason TEXT")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE room_memberships ADD COLUMN IF NOT EXISTS ban_ts BIGINT")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE room_memberships ADD COLUMN IF NOT EXISTS join_reason TEXT")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE room_events ADD COLUMN IF NOT EXISTS sender TEXT")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE room_events ADD COLUMN IF NOT EXISTS unsigned TEXT")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE room_events ADD COLUMN IF NOT EXISTS redacted BOOLEAN DEFAULT FALSE")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS presence (
            user_id TEXT NOT NULL PRIMARY KEY,
            status_msg TEXT,
            presence TEXT NOT NULL DEFAULT 'offline',
            last_active_ts BIGINT NOT NULL DEFAULT 0,
            status_from TEXT,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS user_directory (
            user_id TEXT NOT NULL PRIMARY KEY,
            room_id TEXT NOT NULL,
            visibility TEXT NOT NULL DEFAULT 'private',
            added_by TEXT,
            created_ts BIGINT NOT NULL,
            PRIMARY KEY (user_id, room_id),
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS push_rules (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            rule_id TEXT NOT NULL,
            priority_class INTEGER NOT NULL DEFAULT 0,
            priority INTEGER NOT NULL DEFAULT 0,
            conditions TEXT,
            actions TEXT,
            is_default_rule BOOLEAN DEFAULT FALSE,
            is_enabled BOOLEAN DEFAULT TRUE,
            is_user_created BOOLEAN DEFAULT FALSE,
            created_ts BIGINT NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS push_rules_user_sent_rules (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            rule_id TEXT NOT NULL,
            enable BOOLEAN DEFAULT TRUE,
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS receipts (
            sender TEXT NOT NULL,
            sent_to TEXT NOT NULL,
            room_id TEXT NOT NULL,
            event_id TEXT NOT NULL,
            sent_ts BIGINT NOT NULL,
            receipt_type TEXT NOT NULL,
            PRIMARY KEY (sent_to, sender, room_id),
            FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE,
            FOREIGN KEY (sent_to) REFERENCES users(user_id) ON DELETE CASCADE,
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS pusher_throttle (
            pusher TEXT NOT NULL PRIMARY KEY,
            last_sent_ts BIGINT NOT NULL,
            throttle_ms INTEGER NOT NULL DEFAULT 0
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS pushers (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            access_token TEXT NOT NULL,
            profile_tag TEXT,
            kind TEXT NOT NULL,
            app_id TEXT NOT NULL,
            app_display_name TEXT,
            device_name TEXT,
            pushkey TEXT NOT NULL,
            ts BIGINT NOT NULL,
            language TEXT,
            data TEXT,
            expiry_ts BIGINT,
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS ratelimit_shard (
            user_id TEXT NOT NULL PRIMARY KEY,
            shard_id INTEGER NOT NULL
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS user_filters (
            user_id TEXT NOT NULL,
            filter_id BIGINT NOT NULL,
            filter_definition TEXT NOT NULL,
            PRIMARY KEY (user_id, filter_id),
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS user_ips (
            user_id TEXT NOT NULL,
            access_token TEXT NOT NULL,
            ip TEXT NOT NULL,
            user_agent TEXT,
            device_id TEXT NOT NULL,
            last_seen BIGINT NOT NULL,
            first_seen BIGINT NOT NULL DEFAULT 0
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS current_state_events (
            room_id TEXT NOT NULL,
            type TEXT NOT NULL,
            state_key TEXT NOT NULL,
            event_id TEXT NOT NULL,
            membership TEXT,
            depth BIGINT NOT NULL,
            stream_ordering BIGINT,
            PRIMARY KEY (room_id, type, state_key),
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS friends (
            user_id TEXT NOT NULL,
            friend_id TEXT NOT NULL,
            created_ts BIGINT NOT NULL,
            note TEXT,
            is_favorite BOOLEAN DEFAULT FALSE,
            PRIMARY KEY (user_id, friend_id),
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
            FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS friend_requests (
            id BIGSERIAL PRIMARY KEY,
            from_user_id TEXT NOT NULL,
            to_user_id TEXT NOT NULL,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            status TEXT DEFAULT 'pending',
            message TEXT,
            hide BOOLEAN DEFAULT FALSE,
            FOREIGN KEY (from_user_id) REFERENCES users(user_id) ON DELETE CASCADE,
            FOREIGN KEY (to_user_id) REFERENCES users(user_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS blocked_users (
            user_id TEXT NOT NULL,
            blocked_user_id TEXT NOT NULL,
            reason TEXT,
            created_ts BIGINT NOT NULL,
            PRIMARY KEY (user_id, blocked_user_id),
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
            FOREIGN KEY (blocked_user_id) REFERENCES users(user_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS friend_categories (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            name TEXT NOT NULL,
            color TEXT,
            icon TEXT,
            sort_order BIGINT DEFAULT 0,
            created_ts BIGINT NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
            UNIQUE (user_id, name)
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS security_events (
            id BIGSERIAL PRIMARY KEY,
            event_type TEXT NOT NULL,
            severity TEXT NOT NULL DEFAULT 'info',
            user_id TEXT,
            ip_address TEXT,
            user_agent TEXT,
            details TEXT,
            created_at BIGINT NOT NULL,
            resolved BOOLEAN DEFAULT FALSE,
            resolved_by TEXT,
            resolved_ts BIGINT
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS ip_blocks (
            id BIGSERIAL PRIMARY KEY,
            ip_address TEXT NOT NULL,
            reason TEXT,
            blocked_by TEXT NOT NULL,
            blocked_at BIGINT NOT NULL,
            expires_at BIGINT,
            is_active BOOLEAN DEFAULT TRUE,
            FOREIGN KEY (blocked_by) REFERENCES users(user_id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS ip_reputation (
            ip_address TEXT NOT NULL PRIMARY KEY,
            score INTEGER NOT NULL DEFAULT 50,
            threat_level TEXT DEFAULT 'medium',
            last_seen_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL,
            report_count INTEGER DEFAULT 0,
            whitelist BOOLEAN DEFAULT FALSE,
            details JSONB
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS database_performance_stats (
            id BIGSERIAL PRIMARY KEY,
            metric_type TEXT NOT NULL,
            metric_name TEXT NOT NULL,
            metric_value DOUBLE PRECISION NOT NULL,
            collected_at BIGINT NOT NULL,
            metadata JSONB
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS database_health_history (
            id BIGSERIAL PRIMARY KEY,
            check_type TEXT NOT NULL,
            status TEXT NOT NULL,
            details JSONB,
            checked_at BIGINT NOT NULL
        )
    "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires running PostgreSQL server"]
    async fn test_database_struct_creation() {
        let pool = sqlx::PgPool::connect("postgres://test:test@localhost/test")
            .await
            .expect("Database connection failed - this test requires a running PostgreSQL server");
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
