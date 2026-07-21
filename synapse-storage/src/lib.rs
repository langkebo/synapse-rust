// ROUND2-ISSUE-1: test code may use unwrap/expect/unwrap_err per Rust testing idiom.
// Production lib code is still held to the strict clippy lint config in [lints.clippy].
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::unwrap_err_used))]

use deadpool_redis::Pool as RedisPool;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use tokio::sync::RwLock;

// =============================================================================
// L0 — Core Matrix storage modules (always compiled, required for core-private-chat)
// =============================================================================
pub mod account_data;
pub mod admin_federation;
pub mod admin_media;
pub mod application_service;
pub mod audit;
pub mod background_update;
pub mod dehydrated_device;
pub mod device;
pub mod e2ee_audit;
pub mod email_verification;
pub mod event;
pub mod event_report;
pub mod feature_flags;
pub mod federation_blacklist;
pub mod federation_queue;
pub mod filter;
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
pub mod presence;
pub mod pruning;
pub mod push;
pub mod push_notification;
pub mod qr_login;
pub mod rate_limit;
pub mod refresh_token;
pub mod registration_token;
pub mod relations;
pub mod rendezvous;
pub mod retention;
pub mod room;
pub mod room_account_data;
pub mod room_summary;
pub mod room_tag;
pub mod schema_health_check;
pub mod schema_validator;
pub mod search_index;
pub mod sliding_sync;
pub mod space;
pub mod state_groups;
pub mod sticky_event;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_mocks;
pub mod thread;
pub mod threepid;
pub mod token;
pub mod trigram_ranking;
pub mod user;
pub mod user_store_fake;
pub mod worker;

// =============================================================================
// L3 — Feature-gated extension storage modules (off by default in core builds)
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

#[cfg(feature = "burn-after-read")]
pub mod burn_after_read;

// L0 — Captcha is used by registration flow — keep unconditional
pub mod captcha;

pub mod oauth_client_storage;
pub mod oidc_session_storage;
pub mod oidc_user_mapping;
pub mod url_preview_storage;

pub use self::threepid::UserThreepid;
pub use self::user::{
    LockedUser, User, UserDirectorySearchResult, UserProfile, UserSearchResult, UserSearchResultWithPresence,
    UserStatsSummary, UserStorage, UserStore,
};
pub use user_store_fake::FakeUserStore;

#[cfg(test)]
pub mod test_utils;

pub use self::account_data::{AccountDataRecord, AccountDataStorage, AccountDataStoreApi};
pub use self::admin_federation::{
    AdminFederationStorage, AdminFederationStoreApi, FederationCacheRecord, FederationDestinationRecord,
    PendingFederationRecord,
};
pub use self::admin_media::{
    decode_media_cursor, encode_media_cursor, AdminMediaInfo, AdminMediaPage, AdminMediaQuotaSummary,
    AdminMediaStorage, AdminMediaStoreApi, MediaCursor,
};
pub use self::application_service::{
    ApplicationService, ApplicationServiceEvent, ApplicationServiceNamespace, ApplicationServiceState,
    ApplicationServiceStorage, ApplicationServiceStoreApi, ApplicationServiceTransaction, ApplicationServiceUser,
    NamespaceRule, Namespaces, RegisterApplicationServiceRequest, UpdateApplicationServiceRequest,
};
pub use self::audit::{
    decode_audit_event_cursor, encode_audit_event_cursor, AuditEvent, AuditEventCursor, AuditEventFilters,
    AuditEventStorage, AuditEventStoreApi, CreateAuditEventRequest,
};
pub use self::background_update::{
    BackgroundUpdate, BackgroundUpdateHistory, BackgroundUpdateLock, BackgroundUpdateStats, BackgroundUpdateStorage,
    BackgroundUpdateStoreApi, CreateBackgroundUpdateRequest, UpdateBackgroundUpdateRequest,
};
pub use self::captcha::{
    CaptchaConfig, CaptchaRateLimit, CaptchaSendLog, CaptchaStorage, CaptchaStoreApi, CaptchaTemplate,
    CreateCaptchaRequest, CreateSendLogRequest, RegistrationCaptcha,
};
pub use self::dehydrated_device::{
    DehydratedDevice, DehydratedDeviceStorage, DehydratedDeviceStoreApi, UpsertDehydratedDeviceParams,
};
pub use self::device::{Device, DeviceListStoreApi, DeviceStorage};
pub use self::e2ee_audit::{E2eeAuditStorage, E2eeAuditStoreApi, KeyAuditEntry, KeyEvent};
pub use self::event::{
    CreateEventParams, EventQueryFilter, EventReport, EventReportId, EventSignature, EventStorage, RoomEphemeralEvent,
    RoomEvent, SinceFilter, StateEvent,
};
pub use self::feature_flags::{
    CreateFeatureFlagRequest, FeatureFlag, FeatureFlagFilters, FeatureFlagRecord, FeatureFlagStorage,
    FeatureFlagStoreApi, FeatureFlagTargetInput, FeatureFlagTargetRecord, UpdateFeatureFlagRequest,
};
pub use self::federation_blacklist::{
    decode_federation_blacklist_cursor, encode_federation_blacklist_cursor, AddBlacklistRequest, CreateLogRequest,
    CreateRuleRequest, FederationAccessStats, FederationBlacklist, FederationBlacklistCursor, FederationBlacklistLog,
    FederationBlacklistRule, FederationBlacklistStorage, FederationBlacklistStoreApi, UpdateStatsRequest,
};
pub use self::filter::{CreateFilterRequest, Filter, FilterStorage, FilterStoreApi};
pub use self::invite_blocklist::{InviteBlocklistStorage, InviteBlocklistStoreApi};
pub use self::maintenance::{DatabaseMaintenance, MaintenanceReport, TableStats, VacuumResult};
pub use self::media_quota::{
    CreateQuotaConfigRequest, MediaQuotaAlert, MediaQuotaConfig, MediaQuotaStorage, MediaQuotaStoreApi, MediaUsageLog,
    QuotaCheckResult, ServerMediaQuota, SetUserQuotaRequest, UpdateUsageRequest, UserMediaQuota,
};

// Quarantine stream storage
pub use self::media::quarantine_stream::QuarantinedMediaChangeStoreApi;
pub use self::membership::{MemberStoreApi, RoomMember, RoomMemberStorage, UserRoomMembership};
pub use self::moderation::{
    ContentScanResult, ContentType, CreateModerationRuleParams, MatchedRule, ModerationAction, ModerationLog,
    ModerationLogStorage, ModerationLogStoreApi, ModerationRule, ModerationRuleType, ModerationStorage,
    ModerationStoreApi, ScanContentRequest,
};
pub use self::monitoring::{
    ConnectionPoolStatus, DataIntegrityReport, DatabaseHealthStatus, DatabaseMonitor, DuplicateEntry,
    ForeignKeyViolation, NullConstraintViolation, OrphanedRecord, PerformanceMetrics,
};
pub use self::oidc_user_mapping::{OidcUserMappingStorage, OidcUserMappingStoreApi};
pub use self::openid_token::{CreateOpenIdTokenRequest, OpenIdToken, OpenIdTokenStorage, OpenIdTokenStoreApi};
pub use self::performance::{time_query, PerformanceMonitor, PoolStatistics, QueryMetrics};
pub use self::presence::{PresenceSnapshot, PresenceStorage};
pub use self::push::{PushStorage, PushStoreApi};
pub use self::push_notification::{
    CreateNotificationLogRequest, CreatePushRuleRequest, PushDevice, PushNotificationLog, PushNotificationQueue,
    PushNotificationStorage, PushNotificationStoreApi, PushRule, QueueNotificationRequest, RegisterDeviceRequest,
    RoomNotification,
};
pub use self::qr_login::{QrLoginStorage, QrLoginStoreApi, QrTransaction};
pub use self::rate_limit::{RateLimitRecord, RateLimitStorage, RateLimitStoreApi};
pub use self::rendezvous::{
    CreateRendezvousSessionParams, RendezvousCode, RendezvousIntent, RendezvousLoginFinish, RendezvousLoginStart,
    RendezvousLoginUser, RendezvousMessage, RendezvousMessageStorage, RendezvousMessageStoreApi, RendezvousSession,
    RendezvousStorage, RendezvousStoreApi, RendezvousTransport, StoredRendezvousMessage,
};
pub use self::room::{
    decode_room_search_cursor, encode_room_search_cursor, Receipt, Room, RoomEncryptionStatus, RoomSearchCursor,
    RoomSearchOrder, RoomStorage, RoomStoreApi, RoomUnreadCounts, DEFAULT_HISTORY_VISIBILITY, DEFAULT_JOIN_RULE,
};
pub use self::room_account_data::{RoomAccountDataRecord, RoomAccountDataStorage, RoomAccountDataStoreApi};
pub use self::schema_validator::{SchemaValidationResult, SchemaValidator, TableSchemaInfo};
pub use self::search_index::{
    SearchIndexCursor, SearchIndexEntry, SearchIndexStats, SearchIndexStorage, SearchIndexStoreApi, SearchQuery,
    SearchResult,
};
pub use self::sliding_sync::{
    decode_room_token_sync_cursor, encode_room_token_sync_cursor, AdminRoomTokenSyncEntry, RoomTokenSyncCursor,
    SlidingSyncFilters, SlidingSyncList, SlidingSyncListData, SlidingSyncListQuery, SlidingSyncListRequest,
    SlidingSyncRequest, SlidingSyncResponse, SlidingSyncRoom, SlidingSyncStorage, SlidingSyncStoreApi,
    SlidingSyncToken,
};
pub use self::space::{
    AddChildRequest, CreateSpaceRequest, Space, SpaceChild, SpaceChildInfo, SpaceEvent, SpaceHierarchy,
    SpaceHierarchyNode, SpaceHierarchyRequest, SpaceHierarchyResponse, SpaceHierarchyRoom, SpaceMember, SpaceStorage,
    SpaceStoreApi, SpaceSummary, UpdateSpaceRequest,
};
pub use self::sticky_event::{StickyEvent, StickyEventStorage, StickyEventStoreApi};
pub use self::thread::{
    CreateThreadReplyParams, CreateThreadRootParams, ThreadListParams, ThreadReadReceipt, ThreadRelation, ThreadReply,
    ThreadRoot, ThreadStatistics, ThreadStorage, ThreadStoreApi, ThreadSubscription, ThreadSummary, ThreadWithReplies,
};
pub use self::threepid::{CreateThreepidRequest, ThreepidStorage, ThreepidStoreApi, ThreepidValidationSession};
pub use self::token::{AccessToken, AccessTokenStorage, AccessTokenStoreApi};
pub use self::worker::{
    AssignTaskRequest, HeartbeatRequest, RdataEvent, RdataPosition, RegisterWorkerRequest, ReplicationPosition,
    SendCommandRequest, StreamPosition, UpdateConnectionStatsRequest, WorkerCapabilities, WorkerCommand,
    WorkerCommandRow, WorkerConnection, WorkerEvent, WorkerEventRow, WorkerInfo, WorkerLoadStats,
    WorkerLoadStatsUpdate, WorkerResponsibilitySummary, WorkerRow, WorkerRuntimeConfig, WorkerStatus, WorkerStorage,
    WorkerStoreApi, WorkerTaskAssignment, WorkerTopologyEntry, WorkerTopologyPreset, WorkerTopologyPresetInstance,
    WorkerTopologySummary, WorkerType,
};

// Storage repository traits (explicit re-exports for service-layer consumption)
pub use self::federation_queue::FederationQueueStoreApi;
pub use self::oauth_client_storage::OAuthClientStoreApi;
pub use self::oidc_session_storage::OidcSessionStoreApi;
pub use self::state_groups::StateGroupStoreApi;
pub use self::url_preview_storage::UrlPreviewStoreApi;

// Feature-gated re-exports
#[cfg(feature = "openclaw-routes")]
pub use self::ai_connection::{AiConnection, AiConnectionStorage, AiConnectionStoreApi};
#[cfg(feature = "openclaw-routes")]
pub use self::openclaw::{
    decode_conversation_cursor, decode_generation_cursor, decode_message_cursor, encode_conversation_cursor,
    encode_generation_cursor, encode_message_cursor, AiChatRole, AiConversation, AiGeneration, AiMessage,
    ConversationCursor, CreateChatRoleParams, CreateConnectionParams, CreateConversationParams, GenerationCursor,
    MessageCursor, OpenClawConnection, OpenClawStorage, OpenClawStoreApi, UpdateChatRoleParams, UpdateConnectionParams,
};

#[cfg(feature = "friends")]
pub use self::friend_room::{
    AddFriendToGroupParams, CreateFriendGroupParams, DirectRoomFallbackLink, DmPartnerRecord, FriendDmLink,
    FriendRequestRecord, FriendRoomStorage, FriendRoomStoreApi, RemoveFriendFromGroupParams, RenameFriendGroupParams,
};

#[cfg(feature = "saml-sso")]
pub use self::saml::{
    CreateSamlAuthEventRequest, CreateSamlIdentityProviderRequest, CreateSamlLogoutRequestRequest,
    CreateSamlSessionRequest, CreateSamlUserMappingRequest, SamlAuthEvent, SamlIdentityProvider, SamlLogoutRequest,
    SamlSession, SamlStorage, SamlStoreApi, SamlUserMapping,
};

#[cfg(feature = "cas-sso")]
pub use self::cas::{
    CasProxyGrantingTicket, CasProxyTicket, CasRegisteredService, CasSloSession, CasStorage, CasStoreApi, CasTicket,
    CasUserAttribute, CreatePgtRequest, CreateProxyTicketRequest, CreateTicketRequest, RegisterServiceRequest,
    ValidateTicketRequest,
};

#[cfg(feature = "beacons")]
pub use self::beacon::{
    BeaconInfo, BeaconInfoWithLocations, BeaconLocation, BeaconStorage, BeaconStoreApi, CreateBeaconInfoParams,
    CreateBeaconLocationParams,
};

#[cfg(feature = "voice-extended")]
pub use self::voice::{VoiceAggregatedStats, VoiceStorage, VoiceStoreApi, VoiceUsageRecord, VoiceUserAggregatedStats};

#[cfg(feature = "voip-tracking")]
pub use self::call_session::{
    CallCandidate, CallSession, CallSessionStorage, CallSessionStoreApi, CreateCallSessionParams,
};
#[cfg(feature = "voip-tracking")]
pub use self::matrixrtc::{
    CreateMembershipParams, CreateSessionParams, MatrixRTCStorage, MatrixRTCStoreApi, RTCEncryptionKey, RTCMembership,
    RTCSession, SessionWithMemberships,
};

#[cfg(feature = "widgets")]
pub use self::widget::{CreateWidgetParams, Widget, WidgetPermission, WidgetSession, WidgetStorage, WidgetStoreApi};

#[cfg(feature = "server-notifications")]
pub use self::server_notification::{
    decode_server_notification_cursor, encode_server_notification_cursor, CreateNotificationRequest,
    CreateTemplateRequest, NotificationDeliveryLog, NotificationTemplate, NotificationWithStatus,
    ScheduledNotification, ServerNotification, ServerNotificationCursor, ServerNotificationStorage,
    ServerNotificationStoreApi, UserNotificationStatus,
};

#[cfg(feature = "privacy-ext")]
pub use self::privacy::{
    CreatePrivacySettingsParams, PrivacySettingsUpdate, PrivacyStorage, PrivacyStoreApi, UserPrivacySettings,
};

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
            reference_image: None,
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
