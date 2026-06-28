// =============================================================================
// L0 — Core Matrix storage re-exports (always compiled, required for core-private-chat)
// =============================================================================
// All re-exports now resolve directly to synapse_storage. The local facade files
// and directory modules have been removed.

pub use synapse_storage::user::{
    LockedUser, User, UserDirectorySearchResult, UserProfile, UserSearchResult, UserSearchResultWithPresence,
    UserStatsSummary, UserStorage, UserStore,
};
pub use synapse_storage::threepid::UserThreepid; // user storage types

// =============================================================================
// Explicit re-exports of storage types (L0 — from synapse_storage).
//
// Each `pub use synapse_storage::<module>::{...}` below re-exports the public
// storage structs/traits so that callers can write `crate::storage::TypeName`
// instead of the fully-qualified path.
// =============================================================================
pub use synapse_storage::admin_media::{
    decode_media_cursor, encode_media_cursor, AdminMediaInfo, AdminMediaPage, AdminMediaQuotaSummary,
    AdminMediaStorage, MediaCursor,
};
pub use synapse_storage::application_service::{
    ApplicationService, ApplicationServiceEvent, ApplicationServiceNamespace, ApplicationServiceState,
    ApplicationServiceStorage, ApplicationServiceTransaction, ApplicationServiceUser, NamespaceRule, Namespaces,
    RegisterApplicationServiceRequest, UpdateApplicationServiceRequest,
};
pub use synapse_storage::audit::{
    decode_audit_event_cursor, encode_audit_event_cursor, AuditEvent, AuditEventCursor, AuditEventFilters,
    AuditEventStorage, CreateAuditEventRequest,
};
pub use synapse_storage::captcha::{
    CaptchaConfig, CaptchaRateLimit, CaptchaSendLog, CaptchaStorage, CaptchaTemplate, CreateCaptchaRequest,
    CreateSendLogRequest, RegistrationCaptcha,
};
pub use synapse_storage::dehydrated_device::{DehydratedDevice, DehydratedDeviceStorage, UpsertDehydratedDeviceParams};
pub use synapse_storage::device::{Device, DeviceStorage};
pub use synapse_storage::e2ee_audit::{E2eeAuditStorage, KeyAuditEntry, KeyEvent};
pub use synapse_storage::event::{
    CreateEventParams, EventQueryFilter, EventReport, EventReportId, EventSignature, EventStorage, RoomEphemeralEvent,
    RoomEvent, StateEvent,
};
pub use synapse_storage::feature_flags::{
    CreateFeatureFlagRequest, FeatureFlag, FeatureFlagFilters, FeatureFlagRecord, FeatureFlagStorage,
    FeatureFlagTargetInput, FeatureFlagTargetRecord, UpdateFeatureFlagRequest,
};
pub use synapse_storage::federation_blacklist::{
    decode_federation_blacklist_cursor, encode_federation_blacklist_cursor, AddBlacklistRequest, CreateLogRequest,
    CreateRuleRequest, FederationAccessStats, FederationBlacklist, FederationBlacklistCursor, FederationBlacklistLog,
    FederationBlacklistRule, FederationBlacklistStorage, UpdateStatsRequest,
};
pub use synapse_storage::invite_blocklist::InviteBlocklistStorage;
pub use synapse_storage::maintenance::{DatabaseMaintenance, MaintenanceReport, TableStats, VacuumResult};
pub use synapse_storage::media_quota::{
    CreateQuotaConfigRequest, MediaQuotaAlert, MediaQuotaConfig, MediaQuotaStorage, MediaUsageLog, QuotaCheckResult,
    ServerMediaQuota, SetUserQuotaRequest, UpdateUsageRequest, UserMediaQuota,
};
pub use synapse_storage::membership::{RoomMember, RoomMemberStorage, UserRoomMembership};
pub use synapse_storage::moderation::{
    ContentScanResult, ContentType, CreateModerationRuleParams, MatchedRule, ModerationAction, ModerationLog,
    ModerationLogStorage, ModerationRule, ModerationRuleType, ModerationStorage, ScanContentRequest,
};
pub use synapse_storage::monitoring::{
    ConnectionPoolStatus, DataIntegrityReport, DatabaseHealthStatus, DatabaseMonitor, DuplicateEntry,
    ForeignKeyViolation, NullConstraintViolation, OrphanedRecord, PerformanceMetrics,
};
pub use synapse_storage::oidc_user_mapping::OidcUserMappingStorage;
pub use synapse_storage::performance::{time_query, PerformanceMonitor, PoolStatistics, QueryMetrics};
pub use synapse_storage::presence::{PresenceSnapshot, PresenceStorage};
pub use synapse_storage::room::{
    decode_room_search_cursor, encode_room_search_cursor, Receipt, Room, RoomEncryptionStatus, RoomSearchCursor,
    RoomSearchOrder, RoomStorage, RoomUnreadCounts,
};
pub use synapse_storage::room_tag::{RoomTag, RoomTagStorage};
pub use synapse_storage::schema_validator::{SchemaValidationResult, SchemaValidator, TableSchemaInfo};
pub use synapse_storage::search_index::{
    SearchIndexCursor, SearchIndexEntry, SearchIndexStats, SearchIndexStorage, SearchQuery, SearchResult,
};
pub use synapse_storage::sliding_sync::{
    decode_room_token_sync_cursor, encode_room_token_sync_cursor, AdminRoomTokenSyncEntry, RoomTokenSyncCursor,
    SlidingSyncFilters, SlidingSyncList, SlidingSyncListData, SlidingSyncListQuery, SlidingSyncListRequest,
    SlidingSyncRequest, SlidingSyncResponse, SlidingSyncRoom, SlidingSyncStorage, SlidingSyncToken,
};
pub use synapse_storage::space::{
    AddChildRequest, CreateSpaceRequest, Space, SpaceChild, SpaceChildInfo, SpaceEvent, SpaceHierarchy,
    SpaceHierarchyNode, SpaceHierarchyRequest, SpaceHierarchyResponse, SpaceHierarchyRoom, SpaceMember, SpaceStorage,
    SpaceSummary, UpdateSpaceRequest,
};
pub use synapse_storage::sticky_event::{StickyEvent, StickyEventStorage};
pub use synapse_storage::thread::{
    CreateThreadReplyParams, CreateThreadRootParams, ThreadListParams, ThreadReadReceipt, ThreadRelation, ThreadReply,
    ThreadRoot, ThreadStatistics, ThreadStorage, ThreadSubscription, ThreadSummary, ThreadWithReplies,
};
pub use synapse_storage::threepid::{CreateThreepidRequest, ThreepidStorage, ThreepidValidationSession};
pub use synapse_storage::token::{AccessToken, AccessTokenStorage};

// Module-level re-exports — needed by consumers that access types via
// `crate::storage::<module>::TypeName` rather than the flat re-export path.
pub use synapse_storage::application_service;
pub use synapse_storage::event;
pub use synapse_storage::audit;
pub use synapse_storage::event_report;
pub use synapse_storage::federation_blacklist;
pub use synapse_storage::maintenance;
pub use synapse_storage::module;
pub use synapse_storage::registration_token;
pub use synapse_storage::retention;
pub use synapse_storage::room_tag;
pub use synapse_storage::schema_health_check;
pub use synapse_storage::sliding_sync;
pub use synapse_storage::space;
pub use synapse_storage::sticky_event;
pub use synapse_storage::thread;
pub use synapse_storage::threepid;
pub use synapse_storage::user;

// The following re-export selected public types from the `synapse_storage`
// crate's sub-modules.
pub use synapse_storage::account_data::{AccountDataRecord, AccountDataStorage};
pub use synapse_storage::filter::{CreateFilterRequest, Filter, FilterStorage};
pub use synapse_storage::openid_token::{CreateOpenIdTokenRequest, OpenIdToken, OpenIdTokenStorage};
pub use synapse_storage::push::PushStorage;
pub use synapse_storage::push_notification::{
    CreateNotificationLogRequest, CreatePushRuleRequest, PushDevice, PushNotificationLog, PushNotificationQueue,
    PushNotificationStorage, PushRule, QueueNotificationRequest, RegisterDeviceRequest, RoomNotification,
};
pub use synapse_storage::qr_login::{QrLoginStorage, QrTransaction};
pub use synapse_storage::rate_limit::{RateLimitRecord, RateLimitStorage};
pub use synapse_storage::rendezvous::{
    CreateRendezvousSessionParams, RendezvousCode, RendezvousIntent, RendezvousLoginFinish, RendezvousLoginStart,
    RendezvousLoginUser, RendezvousMessage, RendezvousMessageStorage, RendezvousSession, RendezvousStorage,
    RendezvousTransport, StoredRendezvousMessage,
};
pub use synapse_storage::room_account_data::{RoomAccountDataRecord, RoomAccountDataStorage};
pub use synapse_storage::room_summary::{
    CreateRoomSummaryRequest, CreateSummaryMemberRequest, RoomSummary, RoomSummaryHero, RoomSummaryMember,
    RoomSummaryResponse, RoomSummaryState, RoomSummaryStateEntry, RoomSummaryStats, RoomSummaryStorage,
    RoomSummaryUpdateQueueItem, UpdateRoomSummaryRequest, UpdateSummaryMemberRequest,
};

// Feature-gated explicit re-exports (L3 — from synapse_storage).
#[cfg(feature = "openclaw-routes")]
pub use synapse_storage::ai_connection::{AiConnection, AiConnectionStorage};
#[cfg(feature = "openclaw-routes")]
pub use synapse_storage::openclaw::{
    AiChatRole, AiConversation, AiGeneration, AiMessage, ConversationCursor, CreateChatRoleParams,
    CreateConnectionParams, CreateConversationParams, GenerationCursor, MessageCursor, OpenClawConnection,
    OpenClawStorage, UpdateChatRoleParams, UpdateConnectionParams,
};

#[cfg(feature = "friends")]
pub use synapse_storage::friend_room::{
    AddFriendToGroupParams, CreateFriendGroupParams, DirectRoomFallbackLink, DmPartnerRecord, FriendDmLink,
    FriendRequestRecord, FriendRoomStorage, RemoveFriendFromGroupParams, RenameFriendGroupParams,
};

#[cfg(feature = "saml-sso")]
pub use synapse_storage::saml::{
    CreateSamlAuthEventRequest, CreateSamlIdentityProviderRequest, CreateSamlLogoutRequestRequest,
    CreateSamlSessionRequest, CreateSamlUserMappingRequest, SamlAuthEvent, SamlIdentityProvider, SamlLogoutRequest,
    SamlSession, SamlStorage, SamlUserMapping,
};

#[cfg(feature = "cas-sso")]
pub use synapse_storage::cas::{
    CasProxyGrantingTicket, CasProxyTicket, CasRegisteredService, CasSloSession, CasStorage, CasTicket,
    CasUserAttribute, CreatePgtRequest, CreateProxyTicketRequest, CreateTicketRequest, RegisterServiceRequest,
    ValidateTicketRequest,
};

#[cfg(feature = "beacons")]
pub use synapse_storage::beacon::{
    BeaconInfo, BeaconInfoWithLocations, BeaconLocation, BeaconStorage, CreateBeaconInfoParams,
    CreateBeaconLocationParams,
};

#[cfg(feature = "voip-tracking")]
pub use synapse_storage::call_session::{CallCandidate, CallSession, CallSessionStorage, CreateCallSessionParams};
#[cfg(feature = "voip-tracking")]
pub use synapse_storage::matrixrtc::{
    CreateMembershipParams, CreateSessionParams, MatrixRTCStorage, RTCEncryptionKey, RTCMembership, RTCSession,
    SessionWithMemberships,
};

#[cfg(feature = "widgets")]
pub use synapse_storage::widget::{CreateWidgetParams, Widget, WidgetPermission, WidgetSession, WidgetStorage};

#[cfg(feature = "server-notifications")]
pub use synapse_storage::server_notification::{
    decode_server_notification_cursor, encode_server_notification_cursor, CreateNotificationRequest,
    CreateTemplateRequest, NotificationDeliveryLog, NotificationTemplate, NotificationWithStatus,
    ScheduledNotification, ServerNotification, ServerNotificationCursor, ServerNotificationStorage,
    UserNotificationStatus,
};

#[cfg(feature = "privacy-ext")]
pub use synapse_storage::privacy::{CreatePrivacySettingsParams, PrivacySettingsUpdate, PrivacyStorage, UserPrivacySettings};

// Feature-gated module-level re-exports — needed by consumers that access types via
// `crate::storage::<module>::TypeName`.
#[cfg(feature = "openclaw-routes")]
pub use synapse_storage::openclaw;
#[cfg(feature = "saml-sso")]
pub use synapse_storage::saml;
#[cfg(feature = "widgets")]
pub use synapse_storage::widget;

// =============================================================================
// Database facade — re-export from canonical synapse-storage crate
// =============================================================================
pub use synapse_storage::{initialize_database, Database};
