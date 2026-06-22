// =============================================================================
// L0 — Core Matrix storage modules (always compiled, required for core-private-chat)
// =============================================================================
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
pub mod invite_blocklist;
pub mod maintenance;
pub mod media;
pub mod media_quota;
pub mod membership;
pub mod moderation;
pub mod module;
pub mod monitoring;
pub mod oidc_user_mapping;
pub mod performance;
pub mod presence;
pub mod refresh_token;
pub mod registration_token;
pub mod relations;
pub mod retention;
pub mod room;
pub mod room_tag;
pub mod schema_health_check;
pub mod schema_validator;
pub mod search_index;
pub mod sliding_sync;
pub mod space;
pub mod state_groups;
pub mod sticky_event;
pub mod thread;
pub mod threepid;
pub mod token;
pub mod user;

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

pub use self::user::{
    LockedUser, User, UserDirectorySearchResult, UserProfile, UserSearchResult, UserSearchResultWithPresence,
    UserStatsSummary, UserStorage,
};
pub use synapse_storage::threepid::UserThreepid; // user storage types

// =============================================================================
// Explicit re-exports of storage types.
//
// Each `pub use self::<module>::{...}` below re-exports the public storage
// structs/traits (e.g. `UserStorage`, `DeviceStorage`, `EventStorage`) so that
// callers can write `crate::storage::UserStorage` instead of the fully-qualified
// `crate::storage::user::UserStorage`. The storage layer historically exposed
// a flat surface and many call sites rely on the short paths.
// =============================================================================
pub use self::admin_media::{
    decode_media_cursor, encode_media_cursor, AdminMediaInfo, AdminMediaPage, AdminMediaQuotaSummary,
    AdminMediaStorage, MediaCursor,
}; // admin media storage types
pub use self::application_service::{
    ApplicationService, ApplicationServiceEvent, ApplicationServiceNamespace, ApplicationServiceState,
    ApplicationServiceStorage, ApplicationServiceTransaction, ApplicationServiceUser, NamespaceRule, Namespaces,
    RegisterApplicationServiceRequest, UpdateApplicationServiceRequest,
}; // application service storage types
pub use self::audit::{
    decode_audit_event_cursor, encode_audit_event_cursor, AuditEvent, AuditEventCursor, AuditEventFilters,
    AuditEventStorage, CreateAuditEventRequest,
}; // audit log storage types
pub use self::captcha::{
    CaptchaConfig, CaptchaRateLimit, CaptchaSendLog, CaptchaStorage, CaptchaTemplate, CreateCaptchaRequest,
    CreateSendLogRequest, RegistrationCaptcha,
}; // captcha storage types
pub use self::dehydrated_device::{DehydratedDevice, DehydratedDeviceStorage, UpsertDehydratedDeviceParams}; // dehydrated device storage types
pub use self::device::{Device, DeviceStorage}; // Device struct and device storage types
pub use self::e2ee_audit::{E2eeAuditStorage, KeyAuditEntry, KeyEvent}; // E2EE audit storage types
pub use self::event::{
    CreateEventParams, EventQueryFilter, EventReport, EventReportId, EventSignature, EventStorage, RoomEphemeralEvent,
    RoomEvent, StateEvent,
}; // RoomEvent struct and event storage types
pub use self::feature_flags::{
    CreateFeatureFlagRequest, FeatureFlag, FeatureFlagFilters, FeatureFlagRecord, FeatureFlagStorage,
    FeatureFlagTargetInput, FeatureFlagTargetRecord, UpdateFeatureFlagRequest,
}; // feature flag storage types
pub use self::federation_blacklist::{
    decode_federation_blacklist_cursor, encode_federation_blacklist_cursor, AddBlacklistRequest, CreateLogRequest,
    CreateRuleRequest, FederationAccessStats, FederationBlacklist, FederationBlacklistCursor, FederationBlacklistLog,
    FederationBlacklistRule, FederationBlacklistStorage, UpdateStatsRequest,
}; // federation blacklist storage types
pub use self::invite_blocklist::InviteBlocklistStorage; // invite blocklist storage types
pub use self::maintenance::{DatabaseMaintenance, MaintenanceReport, TableStats, VacuumResult}; // database maintenance helpers
pub use self::media_quota::{
    CreateQuotaConfigRequest, MediaQuotaAlert, MediaQuotaConfig, MediaQuotaStorage, MediaUsageLog, QuotaCheckResult,
    ServerMediaQuota, SetUserQuotaRequest, UpdateUsageRequest, UserMediaQuota,
}; // media quota storage types
pub use self::membership::{RoomMember, RoomMemberStorage, UserRoomMembership}; // RoomMember struct and membership storage types
pub use self::moderation::{
    ContentScanResult, ContentType, CreateModerationRuleParams, MatchedRule, ModerationAction, ModerationLog,
    ModerationLogStorage, ModerationRule, ModerationRuleType, ModerationStorage, ScanContentRequest,
}; // moderation storage types
pub use self::monitoring::{
    ConnectionPoolStatus, DataIntegrityReport, DatabaseHealthStatus, DatabaseMonitor, DuplicateEntry,
    ForeignKeyViolation, NullConstraintViolation, OrphanedRecord, PerformanceMetrics,
};
pub use self::oidc_user_mapping::OidcUserMappingStorage; // OIDC user mapping storage types
pub use self::performance::{time_query, PerformanceMonitor, PoolStatistics, QueryMetrics};
pub use self::presence::{PresenceSnapshot, PresenceStorage}; // presence storage types
pub use self::room::{
    decode_room_search_cursor, encode_room_search_cursor, Receipt, Room, RoomEncryptionStatus, RoomSearchCursor,
    RoomSearchOrder, RoomStorage, RoomUnreadCounts,
}; // Room struct and room storage types
pub use self::room_tag::{RoomTag, RoomTagStorage}; // room tag storage types
pub use self::schema_validator::{SchemaValidationResult, SchemaValidator, TableSchemaInfo}; // schema validator types
pub use self::search_index::{
    SearchIndexCursor, SearchIndexEntry, SearchIndexStats, SearchIndexStorage, SearchQuery, SearchResult,
}; // search index storage types
pub use self::sliding_sync::{
    decode_room_token_sync_cursor, encode_room_token_sync_cursor, AdminRoomTokenSyncEntry, RoomTokenSyncCursor,
    SlidingSyncFilters, SlidingSyncList, SlidingSyncListData, SlidingSyncListQuery, SlidingSyncListRequest,
    SlidingSyncRequest, SlidingSyncResponse, SlidingSyncRoom, SlidingSyncStorage, SlidingSyncToken,
}; // sliding sync storage types
pub use self::space::{
    AddChildRequest, CreateSpaceRequest, Space, SpaceChild, SpaceChildInfo, SpaceEvent, SpaceHierarchy,
    SpaceHierarchyNode, SpaceHierarchyRequest, SpaceHierarchyResponse, SpaceHierarchyRoom, SpaceMember, SpaceStorage,
    SpaceSummary, UpdateSpaceRequest,
}; // space storage types
pub use self::sticky_event::{StickyEvent, StickyEventStorage}; // sticky event storage types
pub use self::thread::{
    CreateThreadReplyParams, CreateThreadRootParams, ThreadListParams, ThreadReadReceipt, ThreadRelation, ThreadReply,
    ThreadRoot, ThreadStatistics, ThreadStorage, ThreadSubscription, ThreadSummary, ThreadWithReplies,
}; // thread storage types
pub use self::threepid::{CreateThreepidRequest, ThreepidStorage, ThreepidValidationSession}; // third-party ID storage types
pub use self::token::{AccessToken, AccessTokenStorage}; // AccessToken struct and token storage types

// The following re-export selected public types from the `synapse_storage`
// crate's sub-modules.
pub use synapse_storage::account_data::{AccountDataRecord, AccountDataStorage}; // account data storage types
pub use synapse_storage::filter::{CreateFilterRequest, Filter, FilterStorage}; // sync filter storage types
pub use synapse_storage::openid_token::{CreateOpenIdTokenRequest, OpenIdToken, OpenIdTokenStorage}; // OpenID token storage types
pub use synapse_storage::push::PushStorage; // push rule storage types
pub use synapse_storage::push_notification::{
    CreateNotificationLogRequest, CreatePushRuleRequest, PushDevice, PushNotificationLog, PushNotificationQueue,
    PushNotificationStorage, PushRule, QueueNotificationRequest, RegisterDeviceRequest, RoomNotification,
}; // push notification storage types
pub use synapse_storage::qr_login::{QrLoginStorage, QrTransaction}; // QR login storage types
pub use synapse_storage::rate_limit::{RateLimitRecord, RateLimitStorage}; // rate limit storage types
pub use synapse_storage::rendezvous::{
    CreateRendezvousSessionParams, RendezvousCode, RendezvousIntent, RendezvousLoginFinish, RendezvousLoginStart,
    RendezvousLoginUser, RendezvousMessage, RendezvousMessageStorage, RendezvousSession, RendezvousStorage,
    RendezvousTransport, StoredRendezvousMessage,
}; // rendezvous storage types
pub use synapse_storage::room_account_data::{RoomAccountDataRecord, RoomAccountDataStorage}; // room account data storage types
pub use synapse_storage::room_summary::{
    CreateRoomSummaryRequest, CreateSummaryMemberRequest, RoomSummary, RoomSummaryHero, RoomSummaryMember,
    RoomSummaryResponse, RoomSummaryState, RoomSummaryStateEntry, RoomSummaryStats, RoomSummaryStorage,
    RoomSummaryUpdateQueueItem, UpdateRoomSummaryRequest, UpdateSummaryMemberRequest,
}; // room summary storage types

// Feature-gated explicit re-exports.
#[cfg(feature = "openclaw-routes")]
pub use self::ai_connection::{AiConnection, AiConnectionStorage}; // AI connection storage types
#[cfg(feature = "openclaw-routes")]
pub use self::openclaw::{
    AiChatRole, AiConversation, AiGeneration, AiMessage, ConversationCursor, CreateChatRoleParams,
    CreateConnectionParams, CreateConversationParams, GenerationCursor, MessageCursor, OpenClawConnection,
    OpenClawStorage, UpdateChatRoleParams, UpdateConnectionParams,
}; // openclaw storage types

#[cfg(feature = "friends")]
pub use self::friend_room::{
    AddFriendToGroupParams, CreateFriendGroupParams, DirectRoomFallbackLink, DmPartnerRecord, FriendDmLink,
    FriendRequestRecord, FriendRoomStorage, RemoveFriendFromGroupParams, RenameFriendGroupParams,
}; // friend room storage types

#[cfg(feature = "saml-sso")]
pub use self::saml::{
    CreateSamlAuthEventRequest, CreateSamlIdentityProviderRequest, CreateSamlLogoutRequestRequest,
    CreateSamlSessionRequest, CreateSamlUserMappingRequest, SamlAuthEvent, SamlIdentityProvider, SamlLogoutRequest,
    SamlSession, SamlStorage, SamlUserMapping,
}; // SAML storage types

#[cfg(feature = "cas-sso")]
pub use self::cas::{
    CasProxyGrantingTicket, CasProxyTicket, CasRegisteredService, CasSloSession, CasStorage, CasTicket,
    CasUserAttribute, CreatePgtRequest, CreateProxyTicketRequest, CreateTicketRequest, RegisterServiceRequest,
    ValidateTicketRequest,
}; // CAS storage types

#[cfg(feature = "beacons")]
pub use self::beacon::{
    BeaconInfo, BeaconInfoWithLocations, BeaconLocation, BeaconStorage, CreateBeaconInfoParams,
    CreateBeaconLocationParams,
}; // beacon storage types

#[cfg(feature = "voip-tracking")]
pub use self::call_session::{CallCandidate, CallSession, CallSessionStorage, CreateCallSessionParams}; // call session storage types
#[cfg(feature = "voip-tracking")]
pub use self::matrixrtc::{
    CreateMembershipParams, CreateSessionParams, MatrixRTCStorage, RTCEncryptionKey, RTCMembership, RTCSession,
    SessionWithMemberships,
}; // Matrix RTC storage types

#[cfg(feature = "widgets")]
pub use self::widget::{CreateWidgetParams, Widget, WidgetPermission, WidgetSession, WidgetStorage}; // widget storage types

#[cfg(feature = "server-notifications")]
pub use self::server_notification::{
    decode_server_notification_cursor, encode_server_notification_cursor, CreateNotificationRequest,
    CreateTemplateRequest, NotificationDeliveryLog, NotificationTemplate, NotificationWithStatus,
    ScheduledNotification, ServerNotification, ServerNotificationCursor, ServerNotificationStorage,
    UserNotificationStatus,
}; // server notification storage types

#[cfg(feature = "privacy-ext")]
pub use self::privacy::{CreatePrivacySettingsParams, PrivacySettingsUpdate, PrivacyStorage, UserPrivacySettings}; // privacy extension storage types

// =============================================================================
// Database facade — re-export from canonical synapse-storage crate
// =============================================================================
pub use synapse_storage::{Database, initialize_database};

