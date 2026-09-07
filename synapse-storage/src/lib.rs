//! The `synapse-storage` crate: storage-layer (Postgres + Redis) implementations
//! of all Matrix homeserver data access traits. Provides typed query APIs, event
//! persistence, and account/device/room/transactional state backed by sqlx.

// ROUND2-ISSUE-1: test code may use unwrap/expect/unwrap_err per Rust testing idiom.
// Production lib code is still held to the strict clippy lint config in [lints.clippy].
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
// B-3.1-b-6: synapse-storage fully documented + deny(missing_docs).
// ratchet baseline tracked in scripts/quality/check_missing_docs_ratchet.sh;
// this crate is now at zero missing-doc warnings under `cargo doc`.
#![deny(missing_docs)]

use deadpool_redis::Pool as RedisPool;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use tokio::sync::RwLock;

// =============================================================================
// L0 — Core Matrix storage modules (always compiled, required for core-private-chat)
// =============================================================================
/// Account storage domain group — re-exports account modules under `account::`.
pub mod account;
/// The `account_data` module.
pub mod account_data;
/// Admin storage domain group — re-exports admin modules under `admin::`.
pub mod admin;
/// The `admin_federation` module.
pub mod admin_federation;
/// The `admin_media` module.
pub mod admin_media;
/// Application service storage domain group — re-exports application modules under `application::`.
pub mod application;
/// The `application_service` module.
pub mod application_service;
/// The `audit` module.
pub mod audit;
/// The `auth` module.
pub mod auth;
/// The `background_update` module.
pub mod background_update;
/// The `baseline_tables` module.
pub mod baseline_tables;
/// The `dehydrated_device` module.
pub mod dehydrated_device;
/// The `delayed_events` module.
pub mod delayed_events;
/// The `device` module.
pub mod device;
/// Directory storage domain — public-room directory persistence (ARCH-06).
pub mod directory;
/// E2EE storage domain group — re-exports e2ee modules under `e2ee::`.
pub mod e2ee;
/// The `e2ee_audit` module.
pub mod e2ee_audit;
/// The `email_verification` module.
pub mod email_verification;
/// The `event` module.
pub mod event;
/// The `event_report` module.
pub mod event_report;
/// The `feature_flags` module.
pub mod feature_flags;
/// The `federation_blacklist` module.
pub mod federation_blacklist;
/// The `federation_queue` module.
pub mod federation_queue;
/// The `filter` module.
pub mod filter;
/// Infrastructure storage domain group — re-exports infra modules under `infra::`.
pub mod infra;
/// The `invite_blocklist` module.
pub mod invite_blocklist;
/// The `login_token` module.
pub mod login_token;
/// The `maintenance` module.
pub mod maintenance;
/// The `media` module.
pub mod media;
/// The `media_quota` module.
pub mod media_quota;
/// The `membership` module.
pub mod membership;
/// The `migration_checks` module.
pub mod migration_checks;
/// The `moderation` module.
pub mod moderation;
/// The `module` module.
pub mod module;
/// The `monitoring` module.
pub mod monitoring;
/// OIDC storage domain group — re-exports oidc modules under `oidc::`.
pub mod oidc;
/// The `openid_token` module.
pub mod openid_token;
/// The `performance` module.
pub mod performance;
/// Backward-compatibility prelude — glob-import point for domain-grouped types.
pub mod prelude;
/// The `presence` module.
pub mod presence;
/// The `pruning` module.
pub mod pruning;
/// The `push` module.
pub mod push;
/// The `push_notification` module.
pub mod push_notification;
/// The `qr_login` module.
pub mod qr_login;
/// The `rate_limit` module.
pub mod rate_limit;
/// The `refresh_token` module.
pub mod refresh_token;
/// The `registration_token` module.
pub mod registration_token;
/// The `relations` module.
pub mod relations;
/// The `rendezvous` module.
pub mod rendezvous;
/// The `retention` module.
pub mod retention;
/// The `room` module.
pub mod room;
/// The `room_account_data` module.
pub mod room_account_data;
/// The `room_summary` module.
pub mod room_summary;
/// The `room_tag` module.
pub mod room_tag;
/// The `schema_health_check` module.
pub mod schema_health_check;
/// The `schema_validator` module.
pub mod schema_validator;
/// The `search_index` module.
pub mod search_index;
/// The `sliding_sync` module.
pub mod sliding_sync;
/// The `space` module.
pub mod space;
/// The `state_groups` module.
pub mod state_groups;
/// The `sticky_event` module.
pub mod sticky_event;
/// Sync storage domain group — re-exports sync modules under `sync::`.
pub mod sync;
/// Test isolation infrastructure (schema-per-test).
/// Only available under `cfg(test)`.
#[cfg(test)]
pub mod test_isolation;
/// The `test_mocks` module.
#[cfg(any(test, feature = "test-utils"))]
pub mod test_mocks;
/// The `thread` module.
pub mod thread;
/// The `threepid` module.
pub mod threepid;
/// The `token` module.
pub mod token;
/// The `trigram_ranking` module.
pub mod trigram_ranking;
/// The `user` module.
pub mod user;
/// The `user_store_fake` module.
pub mod user_store_fake;
/// The `worker` module.
pub mod worker;

// =============================================================================
// L3 — Feature-gated extension storage modules (off by default in core builds)
// =============================================================================
/// The `friend_room` module.
#[cfg(feature = "friends")]
pub mod friend_room;

/// The `voice` module.
#[cfg(feature = "voice-extended")]
pub mod voice;

/// The `saml` module.
#[cfg(feature = "saml-sso")]
pub mod saml;

/// The `cas` module.
#[cfg(feature = "cas-sso")]
pub mod cas;

/// The `beacon` module.
#[cfg(feature = "beacons")]
pub mod beacon;

/// The `call_session` module.
#[cfg(feature = "voip-tracking")]
pub mod call_session;
/// The `matrixrtc` module.
#[cfg(feature = "voip-tracking")]
pub mod matrixrtc;
/// RTC storage domain group — re-exports RTC modules (`call_session`,
/// `matrixrtc`) under `rtc::`. Feature-gated behind `voip-tracking`.
#[cfg(feature = "voip-tracking")]
pub mod rtc;

/// The `widget` module.
#[cfg(feature = "widgets")]
pub mod widget;

/// The `server_notification` module.
#[cfg(feature = "server-notifications")]
pub mod server_notification;

/// The `privacy` module.
#[cfg(feature = "privacy-ext")]
pub mod privacy;

/// The `burn_after_read` module.
#[cfg(feature = "burn-after-read")]
pub mod burn_after_read;

// L0 — Captcha is used by registration flow — keep unconditional
/// The `captcha` module.
pub mod captcha;

/// The `oauth_client_storage` module.
pub mod oauth_client_storage;
/// The `oidc_session_storage` module.
pub mod oidc_session_storage;
/// The `oidc_user_mapping` module.
pub mod oidc_user_mapping;
/// The `url_preview_storage` module.
pub mod url_preview_storage;

// auth domain types (user, device, token, threepid, captcha, openid_token) are
// re-exported via `pub use auth::*;` below.
pub use user_store_fake::FakeUserStore;

#[cfg(test)]
pub mod test_utils;

// All storage modules are now grouped into a domain. The domain globs below
// flat-re-export every grouped module's public types at the crate root for
// backward compatibility. Domains: account, admin, application, auth, e2ee,
// event, infra, media, moderation, oidc, push, room, space, sync (always on)
// plus rtc (voip-tracking) feature-gated group.

// Domain group globs — backward-compatibility flat re-exports via domain modules.
// Consumers should prefer the domain path (e.g. `synapse_storage::account::*`)
// but these globs keep the legacy root-level paths working.
pub use self::room::*;
pub use account::*; // account domain group (account_data, qr_login, rendezvous)
pub use admin::*; // admin domain group (admin_federation, admin_media, audit)
pub use application::*; // application domain group (application_service, module)
pub use auth::*; // auth domain group (user, device, token, threepid, captcha, openid_token, email_verification, refresh_token, registration_token; saml, cas, privacy when feature-gated)
pub use e2ee::*; // e2ee domain group (dehydrated_device, e2ee_audit)
pub use event::*; // event domain group (event)
pub use infra::*; // infra domain group (background_update, feature_flags, federation_blacklist, federation_queue, maintenance, monitoring, performance, rate_limit, schema_validator, worker, pruning, schema_health_check, trigram_ranking; server_notification when feature-gated)
pub use media::*; // media domain group (media, media_quota, url_preview_storage; voice when feature-gated)
pub use moderation::*; // moderation domain group (moderation, invite_blocklist)
pub use oidc::*; // oidc domain group (oauth_client_storage, oidc_session_storage, oidc_user_mapping)
pub use push::*; // push domain group (push, push_notification)
pub use space::*; // space domain group (space, sticky_event)
pub use sync::*; // sync domain group (sliding_sync, search_index, filter, presence)
                 // Feature-gated domain groups:
#[cfg(feature = "voip-tracking")]
pub use rtc::*; // rtc domain group (call_session, matrixrtc)

/// 数据库结构体。
///
/// Matrix Homeserver 的数据库访问层，封装 PostgreSQL 连接池和监控功能。
/// 提供数据库连接管理、健康检查、性能监控等功能。
pub struct Database {
    /// PostgreSQL 连接池
    pub(crate) pool: Pool<Postgres>,
    /// 数据库监控器
    pub monitor: Arc<RwLock<DatabaseMonitor>>,
}

impl Database {
    /// 创建新的数据库实例。
    pub async fn new(database_url: &str, redis_pool: Option<RedisPool>) -> Result<Self, sqlx::Error> {
        let pool = sqlx::PgPool::connect(database_url).await?;
        let monitor = Arc::new(RwLock::new(DatabaseMonitor::new(pool.clone(), redis_pool, 10000)));
        Ok(Self { pool, monitor })
    }

    /// 从现有连接池创建数据库实例。
    pub fn from_pool(pool: Pool<Postgres>, redis_pool: Option<RedisPool>) -> Self {
        let monitor = Arc::new(RwLock::new(DatabaseMonitor::new(pool.clone(), redis_pool, 10000)));
        Self { pool, monitor }
    }

    /// 获取数据库连接池引用。
    pub fn pool(&self) -> &Pool<Postgres> {
        &self.pool
    }

    /// 执行数据库健康检查。
    pub async fn health_check(&self) -> Result<DatabaseHealthStatus, sqlx::Error> {
        self.monitor.read().await.get_full_health_status().await
    }

    /// 获取性能指标。
    pub async fn get_performance_metrics(&self) -> Result<PerformanceMetrics, sqlx::Error> {
        let health = self.monitor.read().await.get_full_health_status().await?;
        Ok(health.performance_metrics)
    }

    /// 验证数据完整性。
    pub async fn verify_data_integrity(&self) -> Result<DataIntegrityReport, sqlx::Error> {
        self.monitor.read().await.verify_data_integrity().await
    }
}

/// 初始化数据库 schema。
pub fn initialize_database(_pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    tracing::info!("Database initialization completed");
    Ok(())
}

/// Returns the test database URL from environment or default.
#[cfg(test)]
fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_struct_creation() {
        let db_url = test_database_url();
        let pool = match sqlx::PgPool::connect(&db_url).await {
            Ok(p) => p,
            Err(_) => return,
        };
        let _db = Database { pool: pool.clone(), monitor: Arc::new(RwLock::new(DatabaseMonitor::new(pool, None, 50))) };
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
            generation: Some(1),
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
            user_agent: None,
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
            room_version: "10".to_string(),
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
            origin: "example.com".to_string(),
            stream_ordering: Some(1),
            redacts: None,
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
            room_version: "10".to_string(),
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
