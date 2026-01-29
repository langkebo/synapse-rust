use sqlx::{Pool, Postgres};

pub struct Database {
    pub pool: Pool<Postgres>,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = sqlx::PgPool::connect(database_url).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &Pool<Postgres> {
        &self.pool
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
            deleted_ts BIGINT
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
            event_id TEXT NOT NULL PRIMARY KEY,
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
    sqlx::query("ALTER TABLE events ADD COLUMN IF NOT EXISTS sender TEXT")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE events ADD COLUMN IF NOT EXISTS unsigned TEXT")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE events ADD COLUMN IF NOT EXISTS redacted BOOLEAN DEFAULT FALSE")
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
            FOREIGN KEY (user_id) REFERENCES users(name) ON DELETE CASCADE
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
            FOREIGN KEY (user_id) REFERENCES users(name) ON DELETE CASCADE,
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
            FOREIGN KEY (user_id) REFERENCES users(name) ON DELETE CASCADE
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
            FOREIGN KEY (user_id) REFERENCES users(name) ON DELETE CASCADE
        )
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS痍
            sender TEXT NOT NULL,
            sent_to TEXT NOT NULL,
            room_id TEXT NOT NULL,
            event_id TEXT NOT NULL,
            sent_ts BIGINT NOT NULL,
            receipt_type TEXT NOT NULL,
            PRIMARY KEY (sent_to, sender, room_id),
            FOREIGN KEY (sender) REFERENCES users(name) ON DELETE CASCADE,
            FOREIGN KEY (sent_to) REFERENCES users(name) ON DELETE CASCADE,
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
            FOREIGN KEY (user_id) REFERENCES users(name) ON DELETE CASCADE
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
            FOREIGN KEY (user_id) REFERENCES users(name) ON DELETE CASCADE
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

    Ok(())
}

pub mod device;
pub mod event;
pub mod membership;
pub mod room;
pub mod token;
pub mod user;

pub use device::*;
pub use event::*;
pub use membership::*;
pub use room::*;
pub use token::*;
pub use user::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_struct_creation() {
        let _ = Database {
            pool: Pool::<Postgres>::builder().max_size(5).build(),
        };
    }

    #[test]
    fn test_database_pool_method() {
        let pool = Pool::<Postgres>::builder().max_size(5).build();
        let db = Database { pool };
        let _ = db.pool();
    }

    #[test]
    fn test_user_struct_fields() {
        let user = User {
            user_id: "@test:example.com".to_string(),
            username: "testuser".to_string(),
            password_hash: Some("hash123".to_string()),
            displayname: Some("Test User".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            admin: Some(false),
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
        };
        assert_eq!(user.user_id(), "@test:example.com");
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
            created_ts: Some(1234567890000),
            ignored_user_list: None,
            appservice_id: None,
            first_seen_ts: Some(1234567890000),
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
            expires_ts: Some(1234571490),
            invalidated_ts: None,
        };
        assert_eq!(token.id, 1);
        assert_eq!(token.token, "test_token_123");
        assert!(token.expires_ts.is_some());
    }

    #[test]
    fn test_refresh_token_struct_fields() {
        let token = RefreshToken {
            id: 1,
            token: "refresh_token_123".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            created_ts: 1234567890,
            expires_ts: Some(1235171490),
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
            member_count: 5,
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
            content: r#"{"body":"Hello","msgtype":"m.text"}"#.to_string(),
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
            event_id: None,
            event_type: None,
            is_banned: Some(false),
            invite_token: None,
            inviter: None,
            updated_ts: None,
            joined_ts: Some(1234567890000),
            left_ts: None,
            reason: None,
        };
        assert_eq!(member.room_id, "!test:example.com");
        assert_eq!(member.user_id, "@test:example.com");
        assert_eq!(member.membership, "join");
    }
}
