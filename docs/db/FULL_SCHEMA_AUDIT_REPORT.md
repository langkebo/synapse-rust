# Database Migration Full Audit Report

> **Date**: 2026-04-22
> **Scope**: All active migration files vs all `FromRow` Rust structs
> **SQL Tables**: 232 | **Rust Structs**: 150 | **Matched Pairs**: 141

## Issue Summary

| Severity | Count | Description |
|----------|-------|-------------|
| CRITICAL | 68 | Runtime crash (missing column / type panic) |
| HIGH | 511 | Conditional runtime error (nullable mismatch, missing field) |
| MEDIUM | 10 | Naming inconsistency / overly permissive types |
| LOW | 301 | Unused SQL column / no Rust struct |
| INFO | 9 | Informational only |
| **Total** | **899** | |

## Matched Table-Struct Pairs

| SQL Table | Rust Struct | SQL Cols | Rust Fields | Source |
|----------|-------------|----------|--------------|--------|
| `access_tokens` | `AccessToken` | 11 | 10 | 00000000_unified_schema_v6.sql |
| `account_data_callbacks` | `AccountDataCallback` | 6 | 7 | 00000000_unified_schema_v6.sql |
| `account_validity` | `AccountValidity` | 8 | 8 | 00000000_unified_schema_v6.sql |
| `ai_chat_roles` | `AiChatRole` | 14 | 14 | 20260401000001_consolidated_schema_additions.sql |
| `ai_connections` | `AiConnection` | 7 | 7 | 20260410000001_consolidated_feature_additions.sql |
| `ai_conversations` | `AiConversation` | 12 | 12 | 20260401000001_consolidated_schema_additions.sql |
| `ai_generations` | `AiGeneration` | 12 | 11 | 20260401000001_consolidated_schema_additions.sql |
| `ai_messages` | `AiMessage` | 9 | 9 | 20260401000001_consolidated_schema_additions.sql |
| `application_service_events` | `ApplicationServiceEvent` | 8 | 10 | 00000000_unified_schema_v6.sql |
| `application_service_state` | `ApplicationServiceState` | 6 | 4 | 00000000_unified_schema_v6.sql+ALTER(20260422000001_schema_code_alignment.sql) |
| `application_service_transactions` | `ApplicationServiceTransaction` | 13 | 8 | 00000000_unified_schema_v6.sql+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql) |
| `application_service_user_namespaces` | `ApplicationServiceNamespace` | 5 | 6 | 00000000_unified_schema_v6.sql |
| `application_service_users` | `ApplicationServiceUser` | 5 | 5 | 20260401000001_consolidated_schema_additions.sql |
| `application_services` | `ApplicationService` | 15 | 15 | 00000000_unified_schema_v6.sql |
| `audit_events` | `AuditEvent` | 9 | 9 | 00000000_unified_schema_v6.sql |
| `background_update_history` | `BackgroundUpdateHistory` | 8 | 8 | 20260401000001_consolidated_schema_additions.sql |
| `background_update_locks` | `BackgroundUpdateLock` | 4 | 4 | 20260401000001_consolidated_schema_additions.sql |
| `background_update_stats` | `BackgroundUpdateStats` | 10 | 10 | 20260401000001_consolidated_schema_additions.sql |
| `background_updates` | `BackgroundUpdate` | 23 | 20 | 00000000_unified_schema_v6.sql |
| `backup_keys` | `BackupKeyInfo` | 6 | 8 | 00000000_unified_schema_v6.sql |
| `beacon_info` | `BeaconInfo` | 12 | 12 | 20260401000001_consolidated_schema_additions.sql |
| `beacon_locations` | `BeaconLocation` | 10 | 10 | 20260401000001_consolidated_schema_additions.sql |
| `call_candidates` | `CallCandidate` | 6 | 6 | 20260401000001_consolidated_schema_additions.sql |
| `call_sessions` | `CallSession` | 12 | 12 | 20260401000001_consolidated_schema_additions.sql |
| `captcha_config` | `CaptchaConfig` | 6 | 6 | 00000000_unified_schema_v6.sql |
| `captcha_send_log` | `CaptchaSendLog` | 11 | 11 | 00000000_unified_schema_v6.sql |
| `captcha_template` | `CaptchaTemplate` | 10 | 10 | 00000000_unified_schema_v6.sql |
| `cas_proxy_granting_tickets` | `CasProxyGrantingTicket` | 9 | 8 | 00000000_unified_schema_v6.sql+00000001_extensions_cas.sql |
| `cas_proxy_tickets` | `CasProxyTicket` | 11 | 9 | 00000000_unified_schema_v6.sql+00000001_extensions_cas.sql |
| `cas_services` | `CasService` | 12 | 12 | 00000000_unified_schema_v6.sql+00000001_extensions_cas.sql |
| `cas_slo_sessions` | `CasSloSession` | 8 | 7 | 00000000_unified_schema_v6.sql+00000001_extensions_cas.sql |
| `cas_tickets` | `CasTicket` | 11 | 9 | 00000000_unified_schema_v6.sql+00000001_extensions_cas.sql |
| `cas_user_attributes` | `CasUserAttribute` | 6 | 6 | 00000000_unified_schema_v6.sql+00000001_extensions_cas.sql |
| `devices` | `Device` | 11 | 11 | 00000000_unified_schema_v6.sql |
| `e2ee_audit_log` | `KeyAuditEntry` | 11 | 9 | 20260401000001_consolidated_schema_additions.sql+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql) |
| `email_verification_tokens` | `EmailVerificationToken` | 8 | 8 | 20260401000001_consolidated_schema_additions.sql |
| `event_relations` | `EventRelation` | 10 | 10 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
| `event_relations` | `AggregationResult` | 10 | 4 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
| `event_report_history` | `EventReportHistory` | 10 | 10 | 00000000_unified_schema_v6.sql |
| `event_report_stats` | `EventReportStats` | 10 | 9 | 00000000_unified_schema_v6.sql |
| `event_reports` | `EventReport` | 14 | 9 | 00000000_unified_schema_v6.sql |
| `event_signatures` | `EventSignature` | 8 | 7 | 00000000_unified_schema_v6.sql |
| `events` | `RoomEvent` | 23 | 13 | 00000000_unified_schema_v6.sql |
| `events` | `StateEvent` | 23 | 16 | 00000000_unified_schema_v6.sql |
| `feature_flags` | `FeatureFlagRecord` | 9 | 9 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
| `federation_access_stats` | `FederationAccessStats` | 12 | 12 | 20260401000001_consolidated_schema_additions.sql |
| `federation_blacklist` | `FederationBlacklist` | 12 | 10 | 00000000_unified_schema_v6.sql+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql) |
| `federation_blacklist_log` | `FederationBlacklistLog` | 11 | 11 | 20260401000001_consolidated_schema_additions.sql |
| `federation_blacklist_rule` | `FederationBlacklistRule` | 11 | 11 | 20260401000001_consolidated_schema_additions.sql |
| `federation_signing_keys` | `SigningKey` | 9 | 9 | 20260401000001_consolidated_schema_additions.sql |
| `filters` | `Filter` | 5 | 5 | 00000000_unified_schema_v6.sql |
| `friend_requests` | `FriendRequestRecord` | 7 | 7 | 00000000_unified_schema_v6.sql+00000001_extensions_friends.sql |
| `key_backups` | `KeyBackup` | 11 | 8 | 00000000_unified_schema_v6.sql |
| `matrixrtc_encryption_keys` | `RTCEncryptionKey` | 9 | 9 | 20260401000001_consolidated_schema_additions.sql |
| `matrixrtc_memberships` | `RTCMembership` | 15 | 15 | 20260401000001_consolidated_schema_additions.sql |
| `matrixrtc_sessions` | `RTCSession` | 10 | 10 | 20260401000001_consolidated_schema_additions.sql |
| `media_callbacks` | `MediaCallback` | 7 | 9 | 00000000_unified_schema_v6.sql |
| `media_quota_alerts` | `MediaQuotaAlert` | 9 | 9 | 20260406000001_consolidated_schema_fixes.sql |
| `media_quota_config` | `MediaQuotaConfig` | 9 | 12 | 00000000_unified_schema_v6.sql |
| `media_usage_log` | `MediaUsageLog` | 7 | 7 | 20260406000001_consolidated_schema_fixes.sql |
| `moderation_logs` | `ModerationLog` | 9 | 9 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
| `moderation_rules` | `ModerationRule` | 12 | 12 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
| `module_execution_logs` | `ModuleExecutionLog` | 9 | 10 | 00000000_unified_schema_v6.sql |
| `modules` | `Module` | 9 | 14 | 00000000_unified_schema_v6.sql |
| `notification_delivery_log` | `NotificationDeliveryLog` | 7 | 7 | 20260401000001_consolidated_schema_additions.sql |
| `notification_templates` | `NotificationTemplate` | 9 | 9 | 20260401000001_consolidated_schema_additions.sql |
| `openclaw_connections` | `OpenClawConnection` | 11 | 11 | 20260401000001_consolidated_schema_additions.sql |
| `openid_tokens` | `OpenIdToken` | 7 | 7 | 00000000_unified_schema_v6.sql |
| `password_auth_providers` | `PasswordAuthProvider` | 8 | 8 | 00000000_unified_schema_v6.sql |
| `presence_routes` | `PresenceRoute` | 6 | 5 | 00000000_unified_schema_v6.sql |
| `push_device` | `PushDevice` | 18 | 18 | 20260401000001_consolidated_schema_additions.sql |
| `push_notification_log` | `PushNotificationLog` | 18 | 13 | 00000000_unified_schema_v6.sql+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql) |
| `push_notification_queue` | `PushNotificationQueue` | 17 | 15 | 00000000_unified_schema_v6.sql+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql) |
| `push_rules` | `PushRule` | 14 | 14 | 00000000_unified_schema_v6.sql |
| `rate_limit_callbacks` | `RateLimitCallback` | 5 | 8 | 00000000_unified_schema_v6.sql |
| `refresh_token_families` | `RefreshTokenFamily` | 9 | 9 | 00000000_unified_schema_v6.sql |
| `refresh_token_rotations` | `RefreshTokenRotation` | 6 | 6 | 00000000_unified_schema_v6.sql |
| `refresh_token_usage` | `RefreshTokenUsage` | 10 | 10 | 00000000_unified_schema_v6.sql |
| `refresh_tokens` | `RefreshToken` | 15 | 15 | 00000000_unified_schema_v6.sql |
| `registration_captcha` | `RegistrationCaptcha` | 15 | 15 | 00000000_unified_schema_v6.sql |
| `registration_token_batches` | `RegistrationTokenBatch` | 11 | 11 | 20260401000001_consolidated_schema_additions.sql |
| `registration_token_usage` | `RegistrationTokenUsage` | 11 | 11 | 00000000_unified_schema_v6.sql+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql) |
| `registration_tokens` | `RegistrationToken` | 18 | 18 | 00000000_unified_schema_v6.sql |
| `rendezvous_messages` | `StoredRendezvousMessage` | 6 | 6 | 00000000_unified_schema_v6.sql |
| `rendezvous_session` | `RendezvousSession` | 13 | 11 | 00000000_unified_schema_v6.sql |
| `report_rate_limits` | `ReportRateLimit` | 9 | 9 | 00000000_unified_schema_v6.sql+20260410000001_consolidated_feature_additions.sql |
| `retention_cleanup_logs` | `RetentionCleanupLog` | 10 | 10 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
| `retention_stats` | `RetentionStats` | 7 | 7 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
| `room_invites` | `RoomInvite` | 17 | 13 | 00000000_unified_schema_v6.sql+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql) |
| `room_memberships` | `RoomMember` | 20 | 18 | 00000000_unified_schema_v6.sql |
| `room_memberships` | `UserRoomMembership` | 20 | 2 | 00000000_unified_schema_v6.sql |
| `room_retention_policies` | `RoomRetentionPolicy` | 8 | 8 | 00000000_unified_schema_v6.sql |
| `room_summaries` | `RoomSummary` | 24 | 24 | 00000000_unified_schema_v6.sql |
| `room_summary_members` | `RoomSummaryMember` | 10 | 10 | 00000000_unified_schema_v6.sql |
| `room_summary_state` | `RoomSummaryState` | 7 | 7 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
| `room_summary_stats` | `RoomSummaryStats` | 8 | 8 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
| `room_summary_update_queue` | `RoomSummaryUpdateQueueItem` | 11 | 11 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
| `room_tags` | `RoomTag` | 6 | 6 | 00000000_unified_schema_v6.sql |
| `saml_auth_events` | `SamlAuthEvent` | 13 | 13 | 00000000_unified_schema_v6.sql+00000001_extensions_saml.sql |
| `saml_identity_providers` | `SamlIdentityProvider` | 13 | 13 | 00000000_unified_schema_v6.sql+00000001_extensions_saml.sql |
| `saml_logout_requests` | `SamlLogoutRequest` | 10 | 10 | 00000000_unified_schema_v6.sql+00000001_extensions_saml.sql |
| `saml_sessions` | `SamlSession` | 11 | 11 | 00000000_unified_schema_v6.sql+00000001_extensions_saml.sql |
| `saml_user_mapping` | `SamlUserMapping` | 8 | 8 | 00000000_unified_schema_v6.sql+00000001_extensions_saml.sql |
| `scheduled_notifications` | `ScheduledNotification` | 6 | 6 | 20260401000001_consolidated_schema_additions.sql |
| `server_media_quota` | `ServerMediaQuota` | 8 | 8 | 20260406000001_consolidated_schema_fixes.sql |
| `server_notifications` | `ServerNotification` | 16 | 16 | 20260401000001_consolidated_schema_additions.sql |
| `server_retention_policy` | `ServerRetentionPolicy` | 6 | 6 | 00000000_unified_schema_v6.sql |
| `sliding_sync_lists` | `SlidingSyncList` | 11 | 11 | 00000000_unified_schema_v6.sql |
| `sliding_sync_rooms` | `SlidingSyncRoom` | 18 | 18 | 00000000_unified_schema_v6.sql |
| `sliding_sync_rooms` | `AdminRoomTokenSyncEntry` | 18 | 19 | 00000000_unified_schema_v6.sql |
| `sliding_sync_tokens` | `SlidingSyncToken` | 8 | 8 | 00000000_unified_schema_v6.sql |
| `space_children` | `SpaceChild` | 7 | 11 | 00000000_unified_schema_v6.sql |
| `space_events` | `SpaceEvent` | 8 | 8 | 00000000_unified_schema_v6.sql |
| `space_members` | `SpaceMember` | 8 | 7 | 00000000_unified_schema_v6.sql |
| `space_summaries` | `SpaceSummary` | 6 | 6 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
| `spaces` | `Space` | 17 | 13 | 00000000_unified_schema_v6.sql |
| `spam_check_results` | `SpamCheckResult` | 7 | 12 | 00000000_unified_schema_v6.sql |
| `third_party_rule_results` | `ThirdPartyRuleResult` | 8 | 10 | 00000000_unified_schema_v6.sql |
| `thread_read_receipts` | `ThreadReadReceipt` | 8 | 8 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
| `thread_relations` | `ThreadRelation` | 8 | 8 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
| `thread_replies` | `ThreadReply` | 12 | 12 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
| `thread_roots` | `ThreadRoot` | 13 | 13 | 00000000_unified_schema_v6.sql |
| `thread_roots` | `ThreadSummary` | 13 | 16 | 00000000_unified_schema_v6.sql |
| `thread_roots` | `ThreadStatistics` | 13 | 12 | 00000000_unified_schema_v6.sql |
| `thread_subscriptions` | `ThreadSubscription` | 9 | 9 | 00000000_unified_schema_v6.sql |
| `upload_progress` | `UploadProgress` | 12 | 12 | 20260401000001_consolidated_schema_additions.sql |
| `user_media_quota` | `UserMediaQuota` | 7 | 10 | 00000000_unified_schema_v6.sql |
| `user_notification_status` | `UserNotificationStatus` | 8 | 8 | 20260401000001_consolidated_schema_additions.sql |
| `user_privacy_settings` | `UserPrivacySettings` | 12 | 9 | 00000000_unified_schema_v6.sql+00000001_extensions_privacy.sql |
| `user_threepids` | `UserThreepid` | 9 | 9 | 00000000_unified_schema_v6.sql |
| `users` | `User` | 25 | 25 | 00000000_unified_schema_v6.sql |
| `users` | `UserProfile` | 25 | 5 | 00000000_unified_schema_v6.sql |
| `users` | `UserSearchResult` | 25 | 5 | 00000000_unified_schema_v6.sql |
| `users` | `UserSearchResultWithPresence` | 25 | 7 | 00000000_unified_schema_v6.sql |
| `widget_permissions` | `WidgetPermission` | 6 | 6 | 20260401000001_consolidated_schema_additions.sql |
| `widget_sessions` | `WidgetSession` | 9 | 9 | 20260401000001_consolidated_schema_additions.sql |
| `widgets` | `Widget` | 11 | 11 | 20260401000001_consolidated_schema_additions.sql |
| `worker_commands` | `WorkerCommandRow` | 14 | 14 | 00000000_unified_schema_v6.sql |
| `worker_events` | `WorkerEventRow` | 9 | 9 | 00000000_unified_schema_v6.sql |
| `worker_task_assignments` | `WorkerTaskAssignment` | 12 | 12 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
| `workers` | `WorkerRow` | 14 | 13 | 00000000_unified_schema_v6.sql |

## CRITICAL Issues (68)

| # | Type | Table / Struct | Field / Column | Detail |
|---|------|----------------|-----------------|--------|
| 1 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `backup_keys` | `user_id` | Rust field 'user_id: String' maps to SQL column 'user_id' which does not exist in table 'backup_keys' |
| 2 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `backup_keys` | `backup_id` | Rust field 'backup_id: String' maps to SQL column 'kb' which does not exist in table 'backup_keys' |
| 3 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `backup_keys` | `forwarded_count` | Rust field 'forwarded_count: i64' maps to SQL column 'BIGINT' which does not exist in table 'backup_keys' |
| 4 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `backup_keys` | `is_verified` | Rust field 'is_verified: bool' maps to SQL column 'FALSE' which does not exist in table 'backup_keys' |
| 5 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `refresh_token_usage` | `success` | Rust field 'success: bool' maps to SQL column 'success' which does not exist in table 'refresh_token_usage' |
| 6 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `federation_blacklist` | `is_enabled` | Rust field 'is_enabled: bool' maps to SQL column 'TRUE' which does not exist in table 'federation_blacklist' |
| 7 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `federation_blacklist` | `metadata` | Rust field 'metadata: serde_json::Value' maps to SQL column 'jsonb' which does not exist in table 'federation_blacklist' |
| 8 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `event_relations` | `count` | Rust field 'count: i64' maps to SQL column 'count' which does not exist in table 'event_relations' |
| 9 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `saml_sessions` | `expires_at` | Rust field 'expires_at: i64' maps to SQL column 'expires_at' which does not exist in table 'saml_sessions' |
| 10 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `thread_roots` | `thread_id` | Rust field 'thread_id: String' maps to SQL column 'tr' which does not exist in table 'thread_roots' |
| 11 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `thread_roots` | `root_sender` | Rust field 'root_sender: String' maps to SQL column 'root_sender' which does not exist in table 'thread_roots' |
| 12 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `thread_roots` | `root_origin_server_ts` | Rust field 'root_origin_server_ts: i64' maps to SQL column 'e' which does not exist in table 'thread_roots' |
| 13 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `thread_roots` | `reply_count` | Rust field 'reply_count: i32' maps to SQL column 'INTEGER' which does not exist in table 'thread_roots' |
| 14 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `thread_roots` | `is_frozen` | Rust field 'is_frozen: bool' maps to SQL column 'is_frozen' which does not exist in table 'thread_roots' |
| 15 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `thread_roots` | `updated_ts` | Rust field 'updated_ts: i64' maps to SQL column 'tr' which does not exist in table 'thread_roots' |
| 16 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `thread_roots` | `total_redactions` | Rust field 'total_redactions: i32' maps to SQL column 'INTEGER' which does not exist in table 'thread_roots' |
| 17 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `room_summaries` | `join_rule` | Rust field 'join_rule: String' maps to SQL column 'join_rule' which does not exist in table 'room_summaries' |
| 18 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `room_summary_state` | `id` | Rust field 'id: i64' maps to SQL column 'BIGINT' which does not exist in table 'room_summary_state' |
| 19 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `events` | `processed_ts` | Rust field 'processed_ts: i64' maps to SQL column 'processed_ts' which does not exist in table 'events' |
| 20 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `rendezvous_session` | `expires_at` | Rust field 'expires_at: i64' maps to SQL column 'expires_at' which does not exist in table 'rendezvous_session' |
| 21 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `modules` | `version` | Rust field 'version: String' maps to SQL column 'version' which does not exist in table 'modules' |
| 22 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `modules` | `execution_count` | Rust field 'execution_count: i32' maps to SQL column 'execution_count' which does not exist in table 'modules' |
| 23 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `modules` | `error_count` | Rust field 'error_count: i32' maps to SQL column 'error_count' which does not exist in table 'modules' |
| 24 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `spam_check_results` | `sender` | Rust field 'sender: String' maps to SQL column 'sender' which does not exist in table 'spam_check_results' |
| 25 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `spam_check_results` | `event_type` | Rust field 'event_type: String' maps to SQL column 'event_type' which does not exist in table 'spam_check_results' |
| 26 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `spam_check_results` | `result` | Rust field 'result: String' maps to SQL column 'result' which does not exist in table 'spam_check_results' |
| 27 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `spam_check_results` | `score` | Rust field 'score: i32' maps to SQL column 'score' which does not exist in table 'spam_check_results' |
| 28 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `spam_check_results` | `checker_module` | Rust field 'checker_module: String' maps to SQL column 'checker_module' which does not exist in table 'spam_check_results' |
| 29 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `spam_check_results` | `checked_ts` | Rust field 'checked_ts: i64' maps to SQL column 'checked_ts' which does not exist in table 'spam_check_results' |
| 30 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `third_party_rule_results` | `sender` | Rust field 'sender: String' maps to SQL column 'sender' which does not exist in table 'third_party_rule_results' |
| 31 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `third_party_rule_results` | `event_type` | Rust field 'event_type: String' maps to SQL column 'event_type' which does not exist in table 'third_party_rule_results' |
| 32 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `third_party_rule_results` | `rule_name` | Rust field 'rule_name: String' maps to SQL column 'rule_name' which does not exist in table 'third_party_rule_results' |
| 33 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `third_party_rule_results` | `allowed` | Rust field 'allowed: bool' maps to SQL column 'allowed' which does not exist in table 'third_party_rule_results' |
| 34 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `third_party_rule_results` | `checked_ts` | Rust field 'checked_ts: i64' maps to SQL column 'checked_ts' which does not exist in table 'third_party_rule_results' |
| 35 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `module_execution_logs` | `module_name` | Rust field 'module_name: String' maps to SQL column 'module_name' which does not exist in table 'module_execution_logs' |
| 36 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `module_execution_logs` | `module_type` | Rust field 'module_type: String' maps to SQL column 'module_type' which does not exist in table 'module_execution_logs' |
| 37 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `module_execution_logs` | `success` | Rust field 'success: bool' maps to SQL column 'success' which does not exist in table 'module_execution_logs' |
| 38 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `module_execution_logs` | `executed_ts` | Rust field 'executed_ts: i64' maps to SQL column 'executed_ts' which does not exist in table 'module_execution_logs' |
| 39 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `account_validity` | `expiration_ts` | Rust field 'expiration_ts: i64' maps to SQL column 'expiration_ts' which does not exist in table 'account_validity' |
| 40 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `presence_routes` | `user_id` | Rust field 'user_id: String' maps to SQL column 'user_id' which does not exist in table 'presence_routes' |
| 41 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `presence_routes` | `presence_server` | Rust field 'presence_server: String' maps to SQL column 'presence_server' which does not exist in table 'presence_routes' |
| 42 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `presence_routes` | `updated_ts` | Rust field 'updated_ts: i64' maps to SQL column 'updated_ts' which does not exist in table 'presence_routes' |
| 43 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_callbacks` | `media_id` | Rust field 'media_id: String' maps to SQL column 'media_id' which does not exist in table 'media_callbacks' |
| 44 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_callbacks` | `user_id` | Rust field 'user_id: String' maps to SQL column 'user_id' which does not exist in table 'media_callbacks' |
| 45 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_callbacks` | `status` | Rust field 'status: String' maps to SQL column 'status' which does not exist in table 'media_callbacks' |
| 46 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `rate_limit_callbacks` | `callback_type` | Rust field 'callback_type: String' maps to SQL column 'callback_type' which does not exist in table 'rate_limit_callbacks' |
| 47 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `rate_limit_callbacks` | `user_id` | Rust field 'user_id: String' maps to SQL column 'user_id' which does not exist in table 'rate_limit_callbacks' |
| 48 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `rate_limit_callbacks` | `ip_address` | Rust field 'ip_address: String' maps to SQL column 'ip_address' which does not exist in table 'rate_limit_callbacks' |
| 49 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `rate_limit_callbacks` | `rate_limit_type` | Rust field 'rate_limit_type: String' maps to SQL column 'rate_limit_type' which does not exist in table 'rate_limit_callbacks' |
| 50 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `account_data_callbacks` | `callback_type` | Rust field 'callback_type: String' maps to SQL column 'callback_type' which does not exist in table 'account_data_callbacks' |
| 51 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `account_data_callbacks` | `user_id` | Rust field 'user_id: String' maps to SQL column 'user_id' which does not exist in table 'account_data_callbacks' |
| 52 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `account_data_callbacks` | `data_type` | Rust field 'data_type: String' maps to SQL column 'data_type' which does not exist in table 'account_data_callbacks' |
| 53 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `push_device` | `enabled` | Rust field 'enabled: bool' maps to SQL column 'enabled' which does not exist in table 'push_device' |
| 54 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `push_rules` | `enabled` | Rust field 'enabled: bool' maps to SQL column 'enabled' which does not exist in table 'push_rules' |
| 55 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `application_service_events` | `content` | Rust field 'content: serde_json::Value' maps to SQL column 'jsonb' which does not exist in table 'application_service_events' |
| 56 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_quota_config` | `name` | Rust field 'name: String' maps to SQL column 'name' which does not exist in table 'media_quota_config' |
| 57 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_quota_config` | `max_storage_bytes` | Rust field 'max_storage_bytes: i64' maps to SQL column 'max_storage_bytes' which does not exist in table 'media_quota_config' |
| 58 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_quota_config` | `max_file_size_bytes` | Rust field 'max_file_size_bytes: i64' maps to SQL column 'max_file_size_bytes' which does not exist in table 'media_quota_config' |
| 59 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_quota_config` | `max_files_count` | Rust field 'max_files_count: i32' maps to SQL column 'max_files_count' which does not exist in table 'media_quota_config' |
| 60 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_quota_config` | `allowed_mime_types` | Rust field 'allowed_mime_types: serde_json::Value' maps to SQL column 'allowed_mime_types' which does not exist in table 'media_quota_config' |
| 61 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_quota_config` | `blocked_mime_types` | Rust field 'blocked_mime_types: serde_json::Value' maps to SQL column 'blocked_mime_types' which does not exist in table 'media_quota_config' |
| 62 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_quota_config` | `is_default` | Rust field 'is_default: bool' maps to SQL column 'is_default' which does not exist in table 'media_quota_config' |
| 63 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `user_media_quota` | `current_storage_bytes` | Rust field 'current_storage_bytes: i64' maps to SQL column 'current_storage_bytes' which does not exist in table 'user_media_quota' |
| 64 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `user_media_quota` | `current_files_count` | Rust field 'current_files_count: i32' maps to SQL column 'current_files_count' which does not exist in table 'user_media_quota' |
| 65 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `registration_captcha` | `expires_at` | Rust field 'expires_at: i64' maps to SQL column 'expires_at' which does not exist in table 'registration_captcha' |
| 66 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `sliding_sync_rooms` | `room_timestamp` | Rust field 'room_timestamp: i64' maps to SQL column 'room_timestamp' which does not exist in table 'sliding_sync_rooms' |
| 67 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `sliding_sync_rooms` | `room_updated_ts` | Rust field 'room_updated_ts: i64' maps to SQL column 'room_updated_ts' which does not exist in table 'sliding_sync_rooms' |
| 68 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `sliding_sync_rooms` | `is_expired` | Rust field 'is_expired: bool' maps to SQL column 'tokens' which does not exist in table 'sliding_sync_rooms' |

## HIGH Issues (511)

| # | Type | Table / Struct | Field / Column | Detail |
|---|------|----------------|-----------------|--------|
| 1 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `event_signatures` | `algorithm` | SQL column 'algorithm' (TEXT NOT NULL) has no matching Rust field in EventSignature |
| 2 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_signatures` | `id` | SQL column 'id' is nullable but Rust field 'id: Uuid' is not Option |
| 3 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `key_backups` | `created_ts` | SQL column 'created_ts' (BIGINT NOT NULL) has no matching Rust field in KeyBackup |
| 4 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `key_backups` | `backup_id` | SQL column 'backup_id_text' is nullable but Rust field 'backup_id: String' is not Option |
| 5 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `key_backups` | `version` | SQL column 'version' is nullable but Rust field 'version: i64' is not Option |
| 6 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `key_backups` | `auth_key` | SQL column 'auth_key' is nullable but Rust field 'auth_key: String' is not Option |
| 7 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `key_backups` | `mgmt_key` | SQL column 'mgmt_key' is nullable but Rust field 'mgmt_key: String' is not Option |
| 8 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `key_backups` | `backup_data` | SQL column 'auth_data' is nullable but Rust field 'backup_data: serde_json::Value' is not Option |
| 9 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `backup_keys` | `backup_id` | SQL column 'backup_id' (BIGINT NOT NULL) has no matching Rust field in BackupKeyInfo |
| 10 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `backup_keys` | `created_ts` | SQL column 'created_ts' (BIGINT NOT NULL) has no matching Rust field in BackupKeyInfo |
| 11 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `room_tags` | `order` | Rust field 'order: Option<f64>' maps to SQL column 'order' which does not exist in table 'room_tags' |
| 12 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_tags` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 13 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `feature_flags` | `flag_key` | SQL column 'flag_key' is nullable but Rust field 'flag_key: String' is not Option |
| 14 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `feature_flags` | `rollout_percent` | SQL column 'rollout_percent' is nullable but Rust field 'rollout_percent: i32' is not Option |
| 15 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `feature_flags` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 16 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `matrixrtc_sessions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 17 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `matrixrtc_sessions` | `is_active` | SQL column 'is_active' is nullable but Rust field 'is_active: bool' is not Option |
| 18 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `matrixrtc_memberships` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 19 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `matrixrtc_memberships` | `is_active` | SQL column 'is_active' is nullable but Rust field 'is_active: bool' is not Option |
| 20 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `matrixrtc_encryption_keys` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 21 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `refresh_tokens` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 22 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `refresh_tokens` | `use_count` | SQL column 'use_count' is nullable but Rust field 'use_count: i32' is not Option |
| 23 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `refresh_tokens` | `is_revoked` | SQL column 'is_revoked' is nullable but Rust field 'is_revoked: bool' is not Option |
| 24 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `refresh_token_usage` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 25 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `refresh_token_families` | `compromised_ts` | Rust field 'compromised_ts: Option<i64>' maps to SQL column 'compromised_ts' which does not exist in table 'refresh_token_families' |
| 26 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `refresh_token_families` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 27 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `refresh_token_families` | `refresh_count` | SQL column 'refresh_count' is nullable but Rust field 'refresh_count: i32' is not Option |
| 28 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `refresh_token_families` | `is_compromised` | SQL column 'is_compromised' is nullable but Rust field 'is_compromised: bool' is not Option |
| 29 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `refresh_token_rotations` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 30 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_connections` | `id` | SQL column 'id' is nullable but Rust field 'id: String' is not Option |
| 31 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_connections` | `is_active` | SQL column 'is_active' is nullable but Rust field 'is_active: bool' is not Option |
| 32 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `federation_blacklist` | `expires_at` | Rust field 'expires_at: Option<i64>' maps to SQL column 'BIGINT' which does not exist in table 'federation_blacklist' |
| 33 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_blacklist` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 34 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_blacklist` | `block_type` | SQL column 'block_type' is nullable but Rust field 'block_type: String' is not Option |
| 35 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_blacklist` | `blocked_by` | SQL column 'added_by' is nullable but Rust field 'blocked_by: String' is not Option |
| 36 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_blacklist` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 37 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_blacklist_log` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 38 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_blacklist_log` | `metadata` | SQL column 'metadata' is nullable but Rust field 'metadata: serde_json::Value' is not Option |
| 39 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_access_stats` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 40 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_access_stats` | `total_requests` | SQL column 'total_requests' is nullable but Rust field 'total_requests: i64' is not Option |
| 41 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_access_stats` | `successful_requests` | SQL column 'successful_requests' is nullable but Rust field 'successful_requests: i64' is not Option |
| 42 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_access_stats` | `failed_requests` | SQL column 'failed_requests' is nullable but Rust field 'failed_requests: i64' is not Option |
| 43 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_access_stats` | `average_response_time_ms` | SQL column 'average_response_time_ms' is nullable but Rust field 'average_response_time_ms: f64' is not Option |
| 44 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_access_stats` | `error_rate` | SQL column 'error_rate' is nullable but Rust field 'error_rate: f64' is not Option |
| 45 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_blacklist_rule` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 46 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_blacklist_rule` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 47 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_blacklist_rule` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 48 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_relations` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 49 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_relations` | `content` | SQL column 'content' is nullable but Rust field 'content: serde_json::Value' is not Option |
| 50 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_relations` | `is_redacted` | SQL column 'is_redacted' is nullable but Rust field 'is_redacted: bool' is not Option |
| 51 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `event_relations` | `created_ts` | SQL column 'created_ts' (BIGINT NOT NULL) has no matching Rust field in AggregationResult |
| 52 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `event_relations` | `event_id` | SQL column 'event_id' (TEXT NOT NULL) has no matching Rust field in AggregationResult |
| 53 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `event_relations` | `origin_server_ts` | SQL column 'origin_server_ts' (BIGINT NOT NULL) has no matching Rust field in AggregationResult |
| 54 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `event_relations` | `relates_to_event_id` | SQL column 'relates_to_event_id' (TEXT NOT NULL) has no matching Rust field in AggregationResult |
| 55 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `event_relations` | `room_id` | SQL column 'room_id' (TEXT NOT NULL) has no matching Rust field in AggregationResult |
| 56 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `event_relations` | `sender` | SQL column 'sender' (TEXT NOT NULL) has no matching Rust field in AggregationResult |
| 57 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `event_relations` | `key` | Rust field 'key: Option<String>' maps to SQL column 'key' which does not exist in table 'event_relations' |
| 58 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `event_relations` | `sender` | Rust field 'sender: Option<String>' maps to SQL column 'text' which does not exist in table 'event_relations' |
| 59 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_tokens` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 60 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_tokens` | `token_type` | SQL column 'token_type' is nullable but Rust field 'token_type: String' is not Option |
| 61 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_tokens` | `max_uses` | SQL column 'max_uses' is nullable but Rust field 'max_uses: i32' is not Option |
| 62 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_tokens` | `uses_count` | SQL column 'uses_count' is nullable but Rust field 'uses_count: i32' is not Option |
| 63 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_tokens` | `is_used` | SQL column 'is_used' is nullable but Rust field 'is_used: bool' is not Option |
| 64 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_tokens` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 65 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_tokens` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 66 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_token_usage` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 67 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_token_usage` | `token_id` | SQL column 'token_id' is nullable but Rust field 'token_id: i64' is not Option |
| 68 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_token_usage` | `token` | SQL column 'token' is nullable but Rust field 'token: String' is not Option |
| 69 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_token_usage` | `success` | SQL column 'success' is nullable but Rust field 'success: bool' is not Option |
| 70 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `room_invites` | `invitee` | SQL column 'invitee' (TEXT NOT NULL) has no matching Rust field in RoomInvite |
| 71 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `room_invites` | `inviter` | SQL column 'inviter' (TEXT NOT NULL) has no matching Rust field in RoomInvite |
| 72 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_invites` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 73 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_invites` | `invite_code` | SQL column 'invite_code' is nullable but Rust field 'invite_code: String' is not Option |
| 74 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_invites` | `inviter_user_id` | SQL column 'inviter_user_id' is nullable but Rust field 'inviter_user_id: String' is not Option |
| 75 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_invites` | `is_used` | SQL column 'is_used' is nullable but Rust field 'is_used: bool' is not Option |
| 76 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_invites` | `is_revoked` | SQL column 'is_revoked' is nullable but Rust field 'is_revoked: bool' is not Option |
| 77 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_token_batches` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 78 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_token_batches` | `tokens_used` | SQL column 'tokens_used' is nullable but Rust field 'tokens_used: i32' is not Option |
| 79 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_token_batches` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 80 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `saml_sessions` | `expires_ts` | SQL column 'expires_ts' (BIGINT NOT NULL) has no matching Rust field in SamlSession |
| 81 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_sessions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 82 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_sessions` | `attributes` | SQL column 'attributes' is nullable but Rust field 'attributes: serde_json::Value' is not Option |
| 83 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_sessions` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 84 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_user_mapping` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 85 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_user_mapping` | `authentication_count` | SQL column 'authentication_count' is nullable but Rust field 'authentication_count: i32' is not Option |
| 86 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_user_mapping` | `attributes` | SQL column 'attributes' is nullable but Rust field 'attributes: serde_json::Value' is not Option |
| 87 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `saml_identity_providers` | `last_metadata_refresh_ts` | Rust field 'last_metadata_refresh_ts: Option<i64>' maps to SQL column 'last_metadata_refresh_ts' which does not exist in table 'saml_identity_providers' |
| 88 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `saml_identity_providers` | `metadata_valid_until` | Rust field 'metadata_valid_until: Option<i64>' maps to SQL column 'metadata_valid_until' which does not exist in table 'saml_identity_providers' |
| 89 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_identity_providers` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 90 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_identity_providers` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 91 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_identity_providers` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 92 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_identity_providers` | `attribute_mapping` | SQL column 'attribute_mapping' is nullable but Rust field 'attribute_mapping: serde_json::Value' is not Option |
| 93 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_auth_events` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 94 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_auth_events` | `attributes` | SQL column 'attributes' is nullable but Rust field 'attributes: serde_json::Value' is not Option |
| 95 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `saml_logout_requests` | `processed_ts` | Rust field 'processed_ts: Option<i64>' maps to SQL column 'processed_ts' which does not exist in table 'saml_logout_requests' |
| 96 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_logout_requests` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 97 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_logout_requests` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 98 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `moderation_rules` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 99 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `moderation_rules` | `is_active` | SQL column 'is_active' is nullable but Rust field 'is_active: bool' is not Option |
| 100 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `moderation_rules` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 101 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `moderation_logs` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 102 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `audit_events` | `event_id` | SQL column 'event_id' is nullable but Rust field 'event_id: String' is not Option |
| 103 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `audit_events` | `details` | SQL column 'details' is nullable but Rust field 'details: Value' is not Option |
| 104 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `widgets` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 105 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `widgets` | `data` | SQL column 'data' is nullable but Rust field 'data: serde_json::Value' is not Option |
| 106 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `widgets` | `is_active` | SQL column 'is_active' is nullable but Rust field 'is_active: bool' is not Option |
| 107 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `widget_permissions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 108 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `widget_permissions` | `permissions` | SQL column 'permissions' is nullable but Rust field 'permissions: serde_json::Value' is not Option |
| 109 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `widget_sessions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 110 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `widget_sessions` | `is_active` | SQL column 'is_active' is nullable but Rust field 'is_active: bool' is not Option |
| 111 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `access_tokens` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 112 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `access_tokens` | `is_revoked` | SQL column 'is_revoked' is nullable but Rust field 'is_revoked: bool' is not Option |
| 113 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_roots` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 114 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_roots` | `reply_count` | SQL column 'reply_count' is nullable but Rust field 'reply_count: i64' is not Option |
| 115 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_roots` | `is_fetched` | SQL column 'is_fetched' is nullable but Rust field 'is_fetched: bool' is not Option |
| 116 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_replies` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 117 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_replies` | `content` | SQL column 'content' is nullable but Rust field 'content: serde_json::Value' is not Option |
| 118 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_replies` | `is_edited` | SQL column 'is_edited' is nullable but Rust field 'is_edited: bool' is not Option |
| 119 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_replies` | `is_redacted` | SQL column 'is_redacted' is nullable but Rust field 'is_redacted: bool' is not Option |
| 120 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_subscriptions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 121 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_subscriptions` | `notification_level` | SQL column 'notification_level' is nullable but Rust field 'notification_level: String' is not Option |
| 122 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_subscriptions` | `is_muted` | SQL column 'is_muted' is nullable but Rust field 'is_muted: bool' is not Option |
| 123 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_subscriptions` | `is_pinned` | SQL column 'is_pinned' is nullable but Rust field 'is_pinned: bool' is not Option |
| 124 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_read_receipts` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 125 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_read_receipts` | `last_read_ts` | SQL column 'last_read_ts' is nullable but Rust field 'last_read_ts: i64' is not Option |
| 126 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_read_receipts` | `unread_count` | SQL column 'unread_count' is nullable but Rust field 'unread_count: i32' is not Option |
| 127 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_relations` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 128 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_relations` | `is_falling_back` | SQL column 'is_falling_back' is nullable but Rust field 'is_falling_back: bool' is not Option |
| 129 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `thread_roots` | `sender` | SQL column 'sender' (TEXT NOT NULL) has no matching Rust field in ThreadSummary |
| 130 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `thread_roots` | `latest_event_id` | Rust field 'latest_event_id: Option<String>' maps to SQL column 'latest_event_id' which does not exist in table 'thread_roots' |
| 131 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `thread_roots` | `latest_sender` | Rust field 'latest_sender: Option<String>' maps to SQL column 'latest_sender' which does not exist in table 'thread_roots' |
| 132 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `thread_roots` | `latest_content` | Rust field 'latest_content: Option<serde_json::Value>' maps to SQL column 'latest_content' which does not exist in table 'thread_roots' |
| 133 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `thread_roots` | `latest_origin_server_ts` | Rust field 'latest_origin_server_ts: Option<i64>' maps to SQL column 'latest_origin_server_ts' which does not exist in table 'thread_roots' |
| 134 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_roots` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 135 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_roots` | `participants` | SQL column 'participants' is nullable but Rust field 'participants: serde_json::Value' is not Option |
| 136 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_roots` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 137 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `thread_roots` | `root_event_id` | SQL column 'root_event_id' (TEXT NOT NULL) has no matching Rust field in ThreadStatistics |
| 138 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `thread_roots` | `sender` | SQL column 'sender' (TEXT NOT NULL) has no matching Rust field in ThreadStatistics |
| 139 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `thread_roots` | `first_reply_ts` | Rust field 'first_reply_ts: Option<i64>' maps to SQL column 'first_reply_ts' which does not exist in table 'thread_roots' |
| 140 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `thread_roots` | `avg_reply_time_ms` | Rust field 'avg_reply_time_ms: Option<i64>' maps to SQL column 'BIGINT' which does not exist in table 'thread_roots' |
| 141 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_roots` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 142 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `event_reports` | `resolved_ts` | Rust field 'resolved_ts: Option<i64>' maps to SQL column 'resolved_ts' which does not exist in table 'event_reports' |
| 143 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_reports` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 144 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_reports` | `score` | SQL column 'score' is nullable but Rust field 'score: i32' is not Option |
| 145 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_report_history` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 146 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `report_rate_limits` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 147 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `report_rate_limits` | `report_count` | SQL column 'report_count' is nullable but Rust field 'report_count: i32' is not Option |
| 148 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `report_rate_limits` | `is_blocked` | SQL column 'is_blocked' is nullable but Rust field 'is_blocked: bool' is not Option |
| 149 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `report_rate_limits` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 150 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_report_stats` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 151 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_report_stats` | `total_reports` | SQL column 'total_reports' is nullable but Rust field 'total_reports: i32' is not Option |
| 152 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_report_stats` | `open_reports` | SQL column 'open_reports' is nullable but Rust field 'open_reports: i32' is not Option |
| 153 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_report_stats` | `resolved_reports` | SQL column 'resolved_reports' is nullable but Rust field 'resolved_reports: i32' is not Option |
| 154 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_report_stats` | `dismissed_reports` | SQL column 'dismissed_reports' is nullable but Rust field 'dismissed_reports: i32' is not Option |
| 155 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_report_stats` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 156 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summaries` | `history_visibility` | SQL column 'history_visibility' is nullable but Rust field 'history_visibility: String' is not Option |
| 157 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summaries` | `guest_access` | SQL column 'guest_access' is nullable but Rust field 'guest_access: String' is not Option |
| 158 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summaries` | `is_direct` | SQL column 'is_direct' is nullable but Rust field 'is_direct: bool' is not Option |
| 159 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summaries` | `is_space` | SQL column 'is_space' is nullable but Rust field 'is_space: bool' is not Option |
| 160 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summaries` | `is_encrypted` | SQL column 'is_encrypted' is nullable but Rust field 'is_encrypted: bool' is not Option |
| 161 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summaries` | `member_count` | SQL column 'member_count' is nullable but Rust field 'member_count: i64' is not Option |
| 162 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summaries` | `joined_member_count` | SQL column 'joined_member_count' is nullable but Rust field 'joined_member_count: i64' is not Option |
| 163 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summaries` | `invited_member_count` | SQL column 'invited_member_count' is nullable but Rust field 'invited_member_count: i64' is not Option |
| 164 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summaries` | `hero_users` | SQL column 'hero_users' is nullable but Rust field 'hero_users: serde_json::Value' is not Option |
| 165 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summaries` | `unread_notifications` | SQL column 'unread_notifications' is nullable but Rust field 'unread_notifications: i64' is not Option |
| 166 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summaries` | `unread_highlight` | SQL column 'unread_highlight' is nullable but Rust field 'unread_highlight: i64' is not Option |
| 167 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_members` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 168 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_members` | `is_hero` | SQL column 'is_hero' is nullable but Rust field 'is_hero: bool' is not Option |
| 169 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_state` | `content` | SQL column 'content' is nullable but Rust field 'content: serde_json::Value' is not Option |
| 170 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_stats` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 171 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_stats` | `total_events` | SQL column 'total_events' is nullable but Rust field 'total_events: i64' is not Option |
| 172 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_stats` | `total_state_events` | SQL column 'total_state_events' is nullable but Rust field 'total_state_events: i64' is not Option |
| 173 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_stats` | `total_messages` | SQL column 'total_messages' is nullable but Rust field 'total_messages: i64' is not Option |
| 174 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_stats` | `total_media` | SQL column 'total_media' is nullable but Rust field 'total_media: i64' is not Option |
| 175 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_stats` | `storage_size` | SQL column 'storage_size' is nullable but Rust field 'storage_size: i64' is not Option |
| 176 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_update_queue` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 177 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_update_queue` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 178 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_update_queue` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 179 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_update_queue` | `retry_count` | SQL column 'retry_count' is nullable but Rust field 'retry_count: i32' is not Option |
| 180 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `events` | `depth` | SQL column 'depth' is nullable but Rust field 'depth: i64' is not Option |
| 181 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `events` | `not_before` | SQL column 'not_before' is nullable but Rust field 'not_before: i64' is not Option |
| 182 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `events` | `origin` | SQL column 'origin' is nullable but Rust field 'origin: String' is not Option |
| 183 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `events` | `processed_ts` | Rust field 'processed_ts: Option<i64>' maps to SQL column 'processed_ts' which does not exist in table 'events' |
| 184 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `users` | `is_admin` | SQL column 'is_admin' is nullable but Rust field 'is_admin: bool' is not Option |
| 185 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `users` | `is_guest` | SQL column 'is_guest' is nullable but Rust field 'is_guest: bool' is not Option |
| 186 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `users` | `is_shadow_banned` | SQL column 'is_shadow_banned' is nullable but Rust field 'is_shadow_banned: bool' is not Option |
| 187 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `users` | `is_deactivated` | SQL column 'is_deactivated' is nullable but Rust field 'is_deactivated: bool' is not Option |
| 188 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `users` | `generation` | SQL column 'generation' is nullable but Rust field 'generation: i64' is not Option |
| 189 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `users` | `is_password_change_required` | SQL column 'is_password_change_required' is nullable but Rust field 'is_password_change_required: bool' is not Option |
| 190 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `users` | `failed_login_attempts` | SQL column 'failed_login_attempts' is nullable but Rust field 'failed_login_attempts: i32' is not Option |
| 191 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `users` | `must_change_password` | SQL column 'must_change_password' is nullable but Rust field 'must_change_password: bool' is not Option |
| 192 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `users` | `displayname` | Rust field 'displayname: Option<String>' maps to SQL column 'u' which does not exist in table 'users' |
| 193 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `users` | `presence` | Rust field 'presence: Option<String>' maps to SQL column 'presence' which does not exist in table 'users' |
| 194 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `users` | `last_active_ts` | Rust field 'last_active_ts: Option<i64>' maps to SQL column 'last_active_ts' which does not exist in table 'users' |
| 195 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `rendezvous_session` | `expires_ts` | SQL column 'expires_ts' (BIGINT NOT NULL) has no matching Rust field in RendezvousSession |
| 196 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `rendezvous_session` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 197 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `rendezvous_session` | `intent` | SQL column 'intent' is nullable but Rust field 'intent: String' is not Option |
| 198 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `rendezvous_session` | `transport` | SQL column 'transport' is nullable but Rust field 'transport: String' is not Option |
| 199 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `rendezvous_session` | `key` | SQL column 'key' is nullable but Rust field 'key: String' is not Option |
| 200 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `rendezvous_session` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 201 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `rendezvous_messages` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 202 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `room_memberships` | `user_id` | SQL column 'user_id' (TEXT NOT NULL) has no matching Rust field in UserRoomMembership |
| 203 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `user_threepids` | `validated_at` | Rust field 'validated_at: Option<i64>' maps to SQL column 'validated_at' which does not exist in table 'user_threepids' |
| 204 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `user_threepids` | `verification_expires_at` | Rust field 'verification_expires_at: Option<i64>' maps to SQL column 'verification_expires_at' which does not exist in table 'user_threepids' |
| 205 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_threepids` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 206 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_threepids` | `is_verified` | SQL column 'is_verified' is nullable but Rust field 'is_verified: bool' is not Option |
| 207 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_privacy_settings` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 208 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_privacy_settings` | `profile_visibility` | SQL column 'profile_visibility' is nullable but Rust field 'profile_visibility: String' is not Option |
| 209 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_privacy_settings` | `avatar_visibility` | SQL column 'avatar_visibility' is nullable but Rust field 'avatar_visibility: String' is not Option |
| 210 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_privacy_settings` | `displayname_visibility` | SQL column 'displayname_visibility' is nullable but Rust field 'displayname_visibility: String' is not Option |
| 211 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_privacy_settings` | `presence_visibility` | SQL column 'presence_visibility' is nullable but Rust field 'presence_visibility: String' is not Option |
| 212 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_privacy_settings` | `room_membership_visibility` | SQL column 'room_membership_visibility' is nullable but Rust field 'room_membership_visibility: String' is not Option |
| 213 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `openclaw_connections` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 214 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `openclaw_connections` | `is_default` | SQL column 'is_default' is nullable but Rust field 'is_default: bool' is not Option |
| 215 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `openclaw_connections` | `is_active` | SQL column 'is_active' is nullable but Rust field 'is_active: bool' is not Option |
| 216 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_conversations` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 217 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_conversations` | `is_pinned` | SQL column 'is_pinned' is nullable but Rust field 'is_pinned: bool' is not Option |
| 218 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_messages` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 219 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `ai_generations` | `type` | SQL column 'type' (TEXT NOT NULL CHECK (type IN ('image', 'video', 'audio'))) has no matching Rust field in AiGeneration |
| 220 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_generations` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 221 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_generations` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 222 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_chat_roles` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 223 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_chat_roles` | `is_public` | SQL column 'is_public' is nullable but Rust field 'is_public: bool' is not Option |
| 224 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `spaces` | `room_id` | SQL column 'room_id' is nullable but Rust field 'room_id: String' is not Option |
| 225 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `spaces` | `join_rule` | SQL column 'join_rule' is nullable but Rust field 'join_rule: String' is not Option |
| 226 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `spaces` | `is_public` | SQL column 'is_public' is nullable but Rust field 'is_public: bool' is not Option |
| 227 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `space_children` | `order` | Rust field 'order: Option<String>' maps to SQL column 'order' which does not exist in table 'space_children' |
| 228 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `space_children` | `suggested` | Rust field 'suggested: Option<bool>' maps to SQL column 'BOOLEAN' which does not exist in table 'space_children' |
| 229 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `space_children` | `added_by` | Rust field 'added_by: Option<String>' maps to SQL column 'TEXT' which does not exist in table 'space_children' |
| 230 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `space_children` | `removed_ts` | Rust field 'removed_ts: Option<i64>' maps to SQL column 'BIGINT' which does not exist in table 'space_children' |
| 231 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `space_children` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 232 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `space_children` | `is_suggested` | SQL column 'is_suggested' is nullable but Rust field 'is_suggested: bool' is not Option |
| 233 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `space_children` | `via_servers` | SQL column 'via_servers' is nullable but Rust field 'via_servers: Vec<String>' is not Option |
| 234 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `space_members` | `membership` | SQL column 'membership' is nullable but Rust field 'membership: String' is not Option |
| 235 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `space_summaries` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 236 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `space_summaries` | `summary` | SQL column 'summary' is nullable but Rust field 'summary: serde_json::Value' is not Option |
| 237 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `space_summaries` | `children_count` | SQL column 'children_count' is nullable but Rust field 'children_count: i64' is not Option |
| 238 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `space_summaries` | `member_count` | SQL column 'member_count' is nullable but Rust field 'member_count: i64' is not Option |
| 239 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `modules` | `last_executed_ts` | Rust field 'last_executed_ts: Option<i64>' maps to SQL column 'last_executed_ts' which does not exist in table 'modules' |
| 240 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `modules` | `last_error` | Rust field 'last_error: Option<String>' maps to SQL column 'last_error' which does not exist in table 'modules' |
| 241 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `modules` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 242 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `modules` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 243 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `modules` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 244 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `modules` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 245 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `spam_check_results` | `created_ts` | SQL column 'created_ts' (BIGINT NOT NULL) has no matching Rust field in SpamCheckResult |
| 246 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `spam_check_results` | `user_id` | SQL column 'user_id' (TEXT NOT NULL) has no matching Rust field in SpamCheckResult |
| 247 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `spam_check_results` | `content` | Rust field 'content: Option<serde_json::Value>' maps to SQL column 'content' which does not exist in table 'spam_check_results' |
| 248 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `spam_check_results` | `reason` | Rust field 'reason: Option<String>' maps to SQL column 'reason' which does not exist in table 'spam_check_results' |
| 249 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `spam_check_results` | `action_taken` | Rust field 'action_taken: Option<String>' maps to SQL column 'action_taken' which does not exist in table 'spam_check_results' |
| 250 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `spam_check_results` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 251 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `third_party_rule_results` | `created_ts` | SQL column 'created_ts' (BIGINT NOT NULL) has no matching Rust field in ThirdPartyRuleResult |
| 252 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `third_party_rule_results` | `rule_type` | SQL column 'rule_type' (TEXT NOT NULL) has no matching Rust field in ThirdPartyRuleResult |
| 253 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `third_party_rule_results` | `reason` | Rust field 'reason: Option<String>' maps to SQL column 'reason' which does not exist in table 'third_party_rule_results' |
| 254 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `third_party_rule_results` | `modified_content` | Rust field 'modified_content: Option<serde_json::Value>' maps to SQL column 'modified_content' which does not exist in table 'third_party_rule_results' |
| 255 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `third_party_rule_results` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 256 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `third_party_rule_results` | `event_id` | SQL column 'event_id' is nullable but Rust field 'event_id: String' is not Option |
| 257 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `third_party_rule_results` | `room_id` | SQL column 'room_id' is nullable but Rust field 'room_id: String' is not Option |
| 258 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `module_execution_logs` | `created_ts` | SQL column 'created_ts' (BIGINT NOT NULL) has no matching Rust field in ModuleExecutionLog |
| 259 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `module_execution_logs` | `execution_type` | SQL column 'execution_type' (TEXT NOT NULL) has no matching Rust field in ModuleExecutionLog |
| 260 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `module_execution_logs` | `event_id` | Rust field 'event_id: Option<String>' maps to SQL column 'event_id' which does not exist in table 'module_execution_logs' |
| 261 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `module_execution_logs` | `room_id` | Rust field 'room_id: Option<String>' maps to SQL column 'room_id' which does not exist in table 'module_execution_logs' |
| 262 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `module_execution_logs` | `metadata` | Rust field 'metadata: Option<serde_json::Value>' maps to SQL column 'metadata' which does not exist in table 'module_execution_logs' |
| 263 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `module_execution_logs` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 264 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `module_execution_logs` | `execution_time_ms` | SQL column 'execution_time_ms' is nullable but Rust field 'execution_time_ms: i64' is not Option |
| 265 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `account_validity` | `email_sent_ts` | Rust field 'email_sent_ts: Option<i64>' maps to SQL column 'email_sent_ts' which does not exist in table 'account_validity' |
| 266 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `account_validity` | `renewal_token_ts` | Rust field 'renewal_token_ts: Option<i64>' maps to SQL column 'renewal_token_ts' which does not exist in table 'account_validity' |
| 267 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `account_validity` | `is_valid` | SQL column 'is_valid' is nullable but Rust field 'is_valid: bool' is not Option |
| 268 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `account_validity` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 269 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `password_auth_providers` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 270 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `password_auth_providers` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 271 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `password_auth_providers` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 272 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `password_auth_providers` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 273 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `presence_routes` | `created_ts` | SQL column 'created_ts' (BIGINT NOT NULL) has no matching Rust field in PresenceRoute |
| 274 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `presence_routes` | `route_name` | SQL column 'route_name' (TEXT NOT NULL) has no matching Rust field in PresenceRoute |
| 275 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `presence_routes` | `route_type` | SQL column 'route_type' (TEXT NOT NULL) has no matching Rust field in PresenceRoute |
| 276 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `presence_routes` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 277 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `presence_routes` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 278 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `media_callbacks` | `callback_name` | SQL column 'callback_name' (TEXT NOT NULL) has no matching Rust field in MediaCallback |
| 279 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `media_callbacks` | `url` | SQL column 'url' (TEXT NOT NULL) has no matching Rust field in MediaCallback |
| 280 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_callbacks` | `result` | Rust field 'result: Option<serde_json::Value>' maps to SQL column 'result' which does not exist in table 'media_callbacks' |
| 281 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_callbacks` | `completed_ts` | Rust field 'completed_ts: Option<i64>' maps to SQL column 'completed_ts' which does not exist in table 'media_callbacks' |
| 282 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_callbacks` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 283 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_callbacks` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 284 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `rate_limit_callbacks` | `callback_name` | SQL column 'callback_name' (TEXT NOT NULL) has no matching Rust field in RateLimitCallback |
| 285 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `rate_limit_callbacks` | `result` | Rust field 'result: Option<serde_json::Value>' maps to SQL column 'result' which does not exist in table 'rate_limit_callbacks' |
| 286 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `rate_limit_callbacks` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 287 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `rate_limit_callbacks` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 288 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `account_data_callbacks` | `callback_name` | SQL column 'callback_name' (TEXT NOT NULL) has no matching Rust field in AccountDataCallback |
| 289 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `account_data_callbacks` | `result` | Rust field 'result: Option<serde_json::Value>' maps to SQL column 'result' which does not exist in table 'account_data_callbacks' |
| 290 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `account_data_callbacks` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 291 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `account_data_callbacks` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 292 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `friend_requests` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 293 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `friend_requests` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 294 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_notifications` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 295 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_notifications` | `notification_type` | SQL column 'notification_type' is nullable but Rust field 'notification_type: String' is not Option |
| 296 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_notifications` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 297 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_notifications` | `target_audience` | SQL column 'target_audience' is nullable but Rust field 'target_audience: String' is not Option |
| 298 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_notifications` | `target_user_ids` | SQL column 'target_user_ids' is nullable but Rust field 'target_user_ids: serde_json::Value' is not Option |
| 299 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_notifications` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 300 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_notifications` | `is_dismissable` | SQL column 'is_dismissable' is nullable but Rust field 'is_dismissable: bool' is not Option |
| 301 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_notifications` | `created_ts` | SQL column 'created_ts' is nullable but Rust field 'created_ts: i64' is not Option |
| 302 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_notifications` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 303 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_notification_status` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 304 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_notification_status` | `is_read` | SQL column 'is_read' is nullable but Rust field 'is_read: bool' is not Option |
| 305 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_notification_status` | `is_dismissed` | SQL column 'is_dismissed' is nullable but Rust field 'is_dismissed: bool' is not Option |
| 306 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_notification_status` | `created_ts` | SQL column 'created_ts' is nullable but Rust field 'created_ts: i64' is not Option |
| 307 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `notification_templates` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 308 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `notification_templates` | `notification_type` | SQL column 'notification_type' is nullable but Rust field 'notification_type: String' is not Option |
| 309 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `notification_templates` | `variables` | SQL column 'variables' is nullable but Rust field 'variables: serde_json::Value' is not Option |
| 310 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `notification_templates` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 311 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `notification_templates` | `created_ts` | SQL column 'created_ts' is nullable but Rust field 'created_ts: i64' is not Option |
| 312 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `notification_templates` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 313 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `notification_delivery_log` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 314 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `notification_delivery_log` | `delivered_ts` | SQL column 'delivered_ts' is nullable but Rust field 'delivered_ts: i64' is not Option |
| 315 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `scheduled_notifications` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 316 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `scheduled_notifications` | `is_sent` | SQL column 'is_sent' is nullable but Rust field 'is_sent: bool' is not Option |
| 317 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `scheduled_notifications` | `created_ts` | SQL column 'created_ts' is nullable but Rust field 'created_ts: i64' is not Option |
| 318 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `beacon_info` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 319 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `beacon_info` | `is_live` | SQL column 'is_live' is nullable but Rust field 'is_live: bool' is not Option |
| 320 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `beacon_locations` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 321 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_device` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 322 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_device` | `error_count` | SQL column 'error_count' is nullable but Rust field 'error_count: i32' is not Option |
| 323 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_device` | `metadata` | SQL column 'metadata' is nullable but Rust field 'metadata: serde_json::Value' is not Option |
| 324 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_rules` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 325 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_rules` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 326 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_rules` | `conditions` | SQL column 'conditions' is nullable but Rust field 'conditions: serde_json::Value' is not Option |
| 327 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_rules` | `actions` | SQL column 'actions' is nullable but Rust field 'actions: serde_json::Value' is not Option |
| 328 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_rules` | `is_default` | SQL column 'is_default' is nullable but Rust field 'is_default: bool' is not Option |
| 329 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_queue` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 330 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_queue` | `content` | SQL column 'content' is nullable but Rust field 'content: serde_json::Value' is not Option |
| 331 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_queue` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 332 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_queue` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 333 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_queue` | `attempts` | SQL column 'attempts' is nullable but Rust field 'attempts: i32' is not Option |
| 334 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_queue` | `max_attempts` | SQL column 'max_attempts' is nullable but Rust field 'max_attempts: i32' is not Option |
| 335 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_queue` | `next_attempt_at` | SQL column 'next_attempt_at' is nullable but Rust field 'next_attempt_at: chrono::DateTime<Utc>' is not Option |
| 336 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `push_notification_log` | `created_ts` | SQL column 'created_ts' (BIGINT NOT NULL) has no matching Rust field in PushNotificationLog |
| 337 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `push_notification_log` | `pushkey` | SQL column 'pushkey' (TEXT NOT NULL) has no matching Rust field in PushNotificationLog |
| 338 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `push_notification_log` | `status` | SQL column 'status' (TEXT NOT NULL) has no matching Rust field in PushNotificationLog |
| 339 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_log` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 340 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_log` | `push_type` | SQL column 'push_type' is nullable but Rust field 'push_type: String' is not Option |
| 341 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_log` | `sent_at` | SQL column 'sent_at' is nullable but Rust field 'sent_at: DateTime<Utc>' is not Option |
| 342 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_log` | `success` | SQL column 'success' is nullable but Rust field 'success: bool' is not Option |
| 343 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_log` | `metadata` | SQL column 'metadata' is nullable but Rust field 'metadata: serde_json::Value' is not Option |
| 344 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `openid_tokens` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 345 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `openid_tokens` | `is_valid` | SQL column 'is_valid' is nullable but Rust field 'is_valid: bool' is not Option |
| 346 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_retention_policies` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 347 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_retention_policies` | `min_lifetime` | SQL column 'min_lifetime' is nullable but Rust field 'min_lifetime: i64' is not Option |
| 348 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_retention_policies` | `expire_on_clients` | SQL column 'expire_on_clients' is nullable but Rust field 'expire_on_clients: bool' is not Option |
| 349 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_retention_policies` | `is_server_default` | SQL column 'is_server_default' is nullable but Rust field 'is_server_default: bool' is not Option |
| 350 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_retention_policy` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 351 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_retention_policy` | `min_lifetime` | SQL column 'min_lifetime' is nullable but Rust field 'min_lifetime: i64' is not Option |
| 352 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_retention_policy` | `expire_on_clients` | SQL column 'expire_on_clients' is nullable but Rust field 'expire_on_clients: bool' is not Option |
| 353 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_cleanup_logs` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 354 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_cleanup_logs` | `events_deleted` | SQL column 'events_deleted' is nullable but Rust field 'events_deleted: i64' is not Option |
| 355 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_cleanup_logs` | `state_events_deleted` | SQL column 'state_events_deleted' is nullable but Rust field 'state_events_deleted: i64' is not Option |
| 356 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_cleanup_logs` | `media_deleted` | SQL column 'media_deleted' is nullable but Rust field 'media_deleted: i64' is not Option |
| 357 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_cleanup_logs` | `bytes_freed` | SQL column 'bytes_freed' is nullable but Rust field 'bytes_freed: i64' is not Option |
| 358 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_stats` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 359 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_stats` | `total_events` | SQL column 'total_events' is nullable but Rust field 'total_events: i64' is not Option |
| 360 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_stats` | `events_in_retention` | SQL column 'events_in_retention' is nullable but Rust field 'events_in_retention: i64' is not Option |
| 361 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_stats` | `events_expired` | SQL column 'events_expired' is nullable but Rust field 'events_expired: i64' is not Option |
| 362 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `filters` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 363 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `filters` | `content` | SQL column 'content' is nullable but Rust field 'content: serde_json::Value' is not Option |
| 364 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_services` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 365 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_services` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 366 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_services` | `rate_limited` | SQL column 'rate_limited' is nullable but Rust field 'rate_limited: bool' is not Option |
| 367 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_services` | `protocols` | SQL column 'protocols' is nullable but Rust field 'protocols: Vec<String>' is not Option |
| 368 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_services` | `namespaces` | SQL column 'namespaces' is nullable but Rust field 'namespaces: serde_json::Value' is not Option |
| 369 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_services` | `config` | SQL column 'config' is nullable but Rust field 'config: serde_json::Value' is not Option |
| 370 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `application_service_state` | `value` | SQL column 'value' (JSONB NOT NULL) has no matching Rust field in ApplicationServiceState |
| 371 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_state` | `state_value` | SQL column 'state_value' is nullable but Rust field 'state_value: String' is not Option |
| 372 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `application_service_events` | `transaction_id` | Rust field 'transaction_id: Option<String>' maps to SQL column 'text' which does not exist in table 'application_service_events' |
| 373 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_events` | `room_id` | SQL column 'room_id' is nullable but Rust field 'room_id: String' is not Option |
| 374 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_events` | `event_type` | SQL column 'event_type' is nullable but Rust field 'event_type: String' is not Option |
| 375 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `application_service_transactions` | `created_ts` | SQL column 'created_ts' (BIGINT NOT NULL) has no matching Rust field in ApplicationServiceTransaction |
| 376 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `application_service_transactions` | `txn_id` | SQL column 'txn_id' (TEXT NOT NULL) has no matching Rust field in ApplicationServiceTransaction |
| 377 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_transactions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 378 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_transactions` | `transaction_id` | SQL column 'transaction_id' is nullable but Rust field 'transaction_id: String' is not Option |
| 379 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_transactions` | `events` | SQL column 'events' is nullable but Rust field 'events: serde_json::Value' is not Option |
| 380 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_transactions` | `sent_ts` | SQL column 'sent_ts' is nullable but Rust field 'sent_ts: i64' is not Option |
| 381 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_transactions` | `retry_count` | SQL column 'retry_count' is nullable but Rust field 'retry_count: i32' is not Option |
| 382 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_user_namespaces` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 383 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_user_namespaces` | `exclusive` | SQL column 'is_exclusive' is nullable but Rust field 'exclusive: bool' is not Option |
| 384 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `cas_tickets` | `expires_ts` | SQL column 'expires_ts' (BIGINT NOT NULL) has no matching Rust field in CasTicket |
| 385 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_tickets` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 386 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_tickets` | `is_valid` | SQL column 'is_valid' is nullable but Rust field 'is_valid: bool' is not Option |
| 387 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `cas_proxy_tickets` | `expires_ts` | SQL column 'expires_ts' (BIGINT NOT NULL) has no matching Rust field in CasProxyTicket |
| 388 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_proxy_tickets` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 389 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_proxy_tickets` | `is_valid` | SQL column 'is_valid' is nullable but Rust field 'is_valid: bool' is not Option |
| 390 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `cas_proxy_granting_tickets` | `expires_ts` | SQL column 'expires_ts' (BIGINT NOT NULL) has no matching Rust field in CasProxyGrantingTicket |
| 391 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_proxy_granting_tickets` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 392 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_proxy_granting_tickets` | `is_valid` | SQL column 'is_valid' is nullable but Rust field 'is_valid: bool' is not Option |
| 393 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_services` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 394 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_services` | `allowed_attributes` | SQL column 'allowed_attributes' is nullable but Rust field 'allowed_attributes: serde_json::Value' is not Option |
| 395 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_services` | `allowed_proxy_callbacks` | SQL column 'allowed_proxy_callbacks' is nullable but Rust field 'allowed_proxy_callbacks: serde_json::Value' is not Option |
| 396 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_services` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 397 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_services` | `require_secure` | SQL column 'require_secure' is nullable but Rust field 'require_secure: bool' is not Option |
| 398 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_services` | `single_logout` | SQL column 'single_logout' is nullable but Rust field 'single_logout: bool' is not Option |
| 399 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_slo_sessions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 400 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_user_attributes` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 401 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `media_quota_config` | `config_name` | SQL column 'config_name' (TEXT NOT NULL) has no matching Rust field in MediaQuotaConfig |
| 402 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_quota_config` | `description` | Rust field 'description: Option<String>' maps to SQL column 'description' which does not exist in table 'media_quota_config' |
| 403 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_config` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 404 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_config` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 405 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `user_media_quota` | `quota_config_id` | Rust field 'quota_config_id: Option<i64>' maps to SQL column 'quota_config_id' which does not exist in table 'user_media_quota' |
| 406 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `user_media_quota` | `custom_max_storage_bytes` | Rust field 'custom_max_storage_bytes: Option<i64>' maps to SQL column 'custom_max_storage_bytes' which does not exist in table 'user_media_quota' |
| 407 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `user_media_quota` | `custom_max_file_size_bytes` | Rust field 'custom_max_file_size_bytes: Option<i64>' maps to SQL column 'custom_max_file_size_bytes' which does not exist in table 'user_media_quota' |
| 408 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `user_media_quota` | `custom_max_files_count` | Rust field 'custom_max_files_count: Option<i32>' maps to SQL column 'custom_max_files_count' which does not exist in table 'user_media_quota' |
| 409 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_media_quota` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 410 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_usage_log` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 411 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_alerts` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 412 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_alerts` | `is_read` | SQL column 'is_read' is nullable but Rust field 'is_read: bool' is not Option |
| 413 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_alerts` | `created_ts` | SQL column 'created_ts' is nullable but Rust field 'created_ts: i64' is not Option |
| 414 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_media_quota` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 415 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_media_quota` | `current_storage_bytes` | SQL column 'current_storage_bytes' is nullable but Rust field 'current_storage_bytes: i64' is not Option |
| 416 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_media_quota` | `current_files_count` | SQL column 'current_files_count' is nullable but Rust field 'current_files_count: i32' is not Option |
| 417 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `server_media_quota` | `alert_threshold_percent` | SQL column 'alert_threshold_percent' is nullable but Rust field 'alert_threshold_percent: i32' is not Option |
| 418 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `background_updates` | `update_name` | SQL column 'update_name' (TEXT NOT NULL) has no matching Rust field in BackgroundUpdate |
| 419 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `background_updates` | `last_updated_ts` | Rust field 'last_updated_ts: Option<i64>' maps to SQL column 'last_updated_ts' which does not exist in table 'background_updates' |
| 420 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `job_name` | SQL column 'job_name' is nullable but Rust field 'job_name: String' is not Option |
| 421 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `job_type` | SQL column 'job_type' is nullable but Rust field 'job_type: String' is not Option |
| 422 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 423 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `progress` | SQL column 'progress' is nullable but Rust field 'progress: serde_json::Value' is not Option |
| 424 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `total_items` | SQL column 'total_items' is nullable but Rust field 'total_items: i32' is not Option |
| 425 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `processed_items` | SQL column 'processed_items' is nullable but Rust field 'processed_items: i32' is not Option |
| 426 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `created_ts` | SQL column 'created_ts' is nullable but Rust field 'created_ts: i64' is not Option |
| 427 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `retry_count` | SQL column 'retry_count' is nullable but Rust field 'retry_count: i32' is not Option |
| 428 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `max_retries` | SQL column 'max_retries' is nullable but Rust field 'max_retries: i32' is not Option |
| 429 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `batch_size` | SQL column 'batch_size' is nullable but Rust field 'batch_size: i32' is not Option |
| 430 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `sleep_ms` | SQL column 'sleep_ms' is nullable but Rust field 'sleep_ms: i32' is not Option |
| 431 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_update_history` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 432 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_update_history` | `items_processed` | SQL column 'items_processed' is nullable but Rust field 'items_processed: i32' is not Option |
| 433 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_update_locks` | `lock_name` | SQL column 'lock_name' is nullable but Rust field 'lock_name: String' is not Option |
| 434 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_update_stats` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 435 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_update_stats` | `total_updates` | SQL column 'total_updates' is nullable but Rust field 'total_updates: i32' is not Option |
| 436 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_update_stats` | `completed_updates` | SQL column 'completed_updates' is nullable but Rust field 'completed_updates: i32' is not Option |
| 437 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_update_stats` | `failed_updates` | SQL column 'failed_updates' is nullable but Rust field 'failed_updates: i32' is not Option |
| 438 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_update_stats` | `average_duration_ms` | SQL column 'average_duration_ms' is nullable but Rust field 'average_duration_ms: i64' is not Option |
| 439 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_update_stats` | `created_ts` | SQL column 'created_ts' is nullable but Rust field 'created_ts: i64' is not Option |
| 440 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_update_stats` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 441 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `registration_captcha` | `expires_ts` | SQL column 'expires_ts' (BIGINT NOT NULL) has no matching Rust field in RegistrationCaptcha |
| 442 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `registration_captcha` | `used_ts` | Rust field 'used_ts: Option<i64>' maps to SQL column 'used_ts' which does not exist in table 'registration_captcha' |
| 443 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `registration_captcha` | `verified_ts` | Rust field 'verified_ts: Option<i64>' maps to SQL column 'verified_ts' which does not exist in table 'registration_captcha' |
| 444 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_captcha` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 445 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_captcha` | `attempt_count` | SQL column 'attempt_count' is nullable but Rust field 'attempt_count: i32' is not Option |
| 446 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_captcha` | `max_attempts` | SQL column 'max_attempts' is nullable but Rust field 'max_attempts: i32' is not Option |
| 447 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_captcha` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 448 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_captcha` | `metadata` | SQL column 'metadata' is nullable but Rust field 'metadata: serde_json::Value' is not Option |
| 449 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_send_log` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 450 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_template` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 451 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_template` | `variables` | SQL column 'variables' is nullable but Rust field 'variables: serde_json::Value' is not Option |
| 452 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_template` | `is_default` | SQL column 'is_default' is nullable but Rust field 'is_default: bool' is not Option |
| 453 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_template` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 454 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_template` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 455 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_config` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 456 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_config` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 457 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_tokens` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 458 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_lists` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 459 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_lists` | `sort` | SQL column 'sort' is nullable but Rust field 'sort: serde_json::Value' is not Option |
| 460 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 461 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `bump_stamp` | SQL column 'bump_stamp' is nullable but Rust field 'bump_stamp: i64' is not Option |
| 462 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `highlight_count` | SQL column 'highlight_count' is nullable but Rust field 'highlight_count: i32' is not Option |
| 463 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `notification_count` | SQL column 'notification_count' is nullable but Rust field 'notification_count: i32' is not Option |
| 464 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `is_dm` | SQL column 'is_dm' is nullable but Rust field 'is_dm: bool' is not Option |
| 465 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `is_encrypted` | SQL column 'is_encrypted' is nullable but Rust field 'is_encrypted: bool' is not Option |
| 466 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `is_tombstoned` | SQL column 'is_tombstoned' is nullable but Rust field 'is_tombstoned: bool' is not Option |
| 467 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `invited` | SQL column 'invited' is nullable but Rust field 'invited: bool' is not Option |
| 468 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `timestamp` | SQL column 'timestamp' is nullable but Rust field 'timestamp: i64' is not Option |
| 469 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `sliding_sync_rooms` | `created_ts` | SQL column 'created_ts' (BIGINT NOT NULL) has no matching Rust field in AdminRoomTokenSyncEntry |
| 470 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `sliding_sync_rooms` | `room_id` | SQL column 'room_id' (TEXT NOT NULL) has no matching Rust field in AdminRoomTokenSyncEntry |
| 471 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `sliding_sync_rooms` | `updated_ts` | SQL column 'updated_ts' (BIGINT NOT NULL) has no matching Rust field in AdminRoomTokenSyncEntry |
| 472 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `sliding_sync_rooms` | `pos` | Rust field 'pos: Option<i64>' maps to SQL column 'pos' which does not exist in table 'sliding_sync_rooms' |
| 473 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `sliding_sync_rooms` | `token_created_ts` | Rust field 'token_created_ts: Option<i64>' maps to SQL column 'token_created_ts' which does not exist in table 'sliding_sync_rooms' |
| 474 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `sliding_sync_rooms` | `token_expires_at` | Rust field 'token_expires_at: Option<i64>' maps to SQL column 'token_expires_at' which does not exist in table 'sliding_sync_rooms' |
| 475 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `bump_stamp` | SQL column 'bump_stamp' is nullable but Rust field 'bump_stamp: i64' is not Option |
| 476 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `highlight_count` | SQL column 'highlight_count' is nullable but Rust field 'highlight_count: i32' is not Option |
| 477 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `notification_count` | SQL column 'notification_count' is nullable but Rust field 'notification_count: i32' is not Option |
| 478 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `is_dm` | SQL column 'is_dm' is nullable but Rust field 'is_dm: bool' is not Option |
| 479 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `is_encrypted` | SQL column 'is_encrypted' is nullable but Rust field 'is_encrypted: bool' is not Option |
| 480 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `is_tombstoned` | SQL column 'is_tombstoned' is nullable but Rust field 'is_tombstoned: bool' is not Option |
| 481 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `invited` | SQL column 'invited' is nullable but Rust field 'invited: bool' is not Option |
| 482 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `email_verification_tokens` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 483 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `email_verification_tokens` | `used` | SQL column 'used' is nullable but Rust field 'used: bool' is not Option |
| 484 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `call_sessions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 485 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `call_candidates` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 486 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_signing_keys` | `key_json` | SQL column 'key_json' is nullable but Rust field 'key_json: serde_json::Value' is not Option |
| 487 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_task_assignments` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 488 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_task_assignments` | `task_data` | SQL column 'task_data' is nullable but Rust field 'task_data: serde_json::Value' is not Option |
| 489 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_task_assignments` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 490 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_task_assignments` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 491 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `workers` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 492 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `workers` | `host` | SQL column 'host' is nullable but Rust field 'host: String' is not Option |
| 493 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `workers` | `port` | SQL column 'port' is nullable but Rust field 'port: i32' is not Option |
| 494 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `workers` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 495 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `workers` | `config` | SQL column 'config' is nullable but Rust field 'config: serde_json::Value' is not Option |
| 496 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `workers` | `metadata` | SQL column 'metadata' is nullable but Rust field 'metadata: serde_json::Value' is not Option |
| 497 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_commands` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 498 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_commands` | `command_data` | SQL column 'command_data' is nullable but Rust field 'command_data: serde_json::Value' is not Option |
| 499 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_commands` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 500 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_commands` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 501 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_commands` | `retry_count` | SQL column 'retry_count' is nullable but Rust field 'retry_count: i32' is not Option |
| 502 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_commands` | `max_retries` | SQL column 'max_retries' is nullable but Rust field 'max_retries: i32' is not Option |
| 503 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_events` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 504 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_events` | `event_data` | SQL column 'event_data' is nullable but Rust field 'event_data: serde_json::Value' is not Option |
| 505 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `e2ee_audit_log` | `action` | SQL column 'action' (TEXT NOT NULL) has no matching Rust field in KeyAuditEntry |
| 506 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `e2ee_audit_log` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 507 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `e2ee_audit_log` | `operation` | SQL column 'operation' is nullable but Rust field 'operation: String' is not Option |
| 508 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `upload_progress` | `upload_id` | SQL column 'upload_id' is nullable but Rust field 'upload_id: String' is not Option |
| 509 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `upload_progress` | `uploaded_size` | SQL column 'uploaded_size' is nullable but Rust field 'uploaded_size: i64' is not Option |
| 510 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `upload_progress` | `uploaded_chunks` | SQL column 'uploaded_chunks' is nullable but Rust field 'uploaded_chunks: i32' is not Option |
| 511 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `upload_progress` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |

## MEDIUM Issues (10)

| # | Type | Table / Struct | Field / Column | Detail |
|---|------|----------------|-----------------|--------|
| 1 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `registration_tokens` | `created_by` | SQL column 'created_by' is NOT NULL but Rust field 'created_by: Option<String>' is Option (overly permissive) |
| 2 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `room_summaries` | `updated_ts` | SQL column 'updated_ts' is NOT NULL but Rust field 'updated_ts: Option<i64>' is Option (overly permissive) |
| 3 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `room_summaries` | `created_ts` | SQL column 'created_ts' is NOT NULL but Rust field 'created_ts: Option<i64>' is Option (overly permissive) |
| 4 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `events` | `event_type` | SQL column 'event_type' is NOT NULL but Rust field 'event_type: Option<String>' is Option (overly permissive) |
| 5 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `beacon_info` | `updated_ts` | SQL column 'updated_ts' is NOT NULL but Rust field 'updated_ts: Option<i64>' is Option (overly permissive) |
| 6 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `push_notification_queue` | `event_id` | SQL column 'event_id' is NOT NULL but Rust field 'event_id: Option<String>' is Option (overly permissive) |
| 7 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `push_notification_queue` | `room_id` | SQL column 'room_id' is NOT NULL but Rust field 'room_id: Option<String>' is Option (overly permissive) |
| 8 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `push_notification_queue` | `notification_type` | SQL column 'notification_type' is NOT NULL but Rust field 'notification_type: Option<String>' is Option (overly permissive) |
| 9 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `e2ee_audit_log` | `device_id` | SQL column 'device_id' is NOT NULL but Rust field 'device_id: Option<String>' is Option (overly permissive) |
| 10 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `upload_progress` | `expires_at` | SQL column 'expires_at' is NOT NULL but Rust field 'expires_at: Option<i64>' is Option (overly permissive) |

## LOW Issues (301)

| # | Type | Table / Struct | Field / Column | Detail |
|---|------|----------------|-----------------|--------|
| 1 | `SQL_COLUMN_UNUSED_BY_RUST` | `key_backups` | `backup_id` | SQL column 'backup_id' not mapped to any Rust field (nullable/has_default) |
| 2 | `SQL_COLUMN_UNUSED_BY_RUST` | `key_backups` | `updated_ts` | SQL column 'updated_ts' not mapped to any Rust field (nullable/has_default) |
| 3 | `SQL_COLUMN_UNUSED_BY_RUST` | `backup_keys` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 4 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_tags` | `order_value` | SQL column 'order_value' not mapped to any Rust field (nullable/has_default) |
| 5 | `SQL_COLUMN_UNUSED_BY_RUST` | `refresh_token_usage` | `is_success` | SQL column 'is_success' not mapped to any Rust field (nullable/has_default) |
| 6 | `SQL_COLUMN_UNUSED_BY_RUST` | `refresh_token_families` | `compromised_at` | SQL column 'compromised_at' not mapped to any Rust field (nullable/has_default) |
| 7 | `SQL_COLUMN_UNUSED_BY_RUST` | `federation_blacklist` | `blocked_by` | SQL column 'blocked_by' not mapped to any Rust field (nullable/has_default) |
| 8 | `SQL_COLUMN_UNUSED_BY_RUST` | `federation_blacklist` | `created_ts` | SQL column 'created_ts' not mapped to any Rust field (nullable/has_default) |
| 9 | `SQL_COLUMN_UNUSED_BY_RUST` | `federation_blacklist` | `expires_at` | SQL column 'expires_at' not mapped to any Rust field (nullable/has_default) |
| 10 | `SQL_COLUMN_UNUSED_BY_RUST` | `federation_blacklist` | `is_enabled` | SQL column 'is_enabled' not mapped to any Rust field (nullable/has_default) |
| 11 | `SQL_COLUMN_UNUSED_BY_RUST` | `federation_blacklist` | `metadata` | SQL column 'metadata' not mapped to any Rust field (nullable/has_default) |
| 12 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_relations` | `content` | SQL column 'content' not mapped to any Rust field (nullable/has_default) |
| 13 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_relations` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 14 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_relations` | `is_redacted` | SQL column 'is_redacted' not mapped to any Rust field (nullable/has_default) |
| 15 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_invites` | `accepted_at` | SQL column 'accepted_at' not mapped to any Rust field (nullable/has_default) |
| 16 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_invites` | `is_accepted` | SQL column 'is_accepted' not mapped to any Rust field (nullable/has_default) |
| 17 | `SQL_COLUMN_UNUSED_BY_RUST` | `saml_identity_providers` | `last_metadata_refresh_at` | SQL column 'last_metadata_refresh_at' not mapped to any Rust field (nullable/has_default) |
| 18 | `SQL_COLUMN_UNUSED_BY_RUST` | `saml_identity_providers` | `metadata_valid_until_at` | SQL column 'metadata_valid_until_at' not mapped to any Rust field (nullable/has_default) |
| 19 | `SQL_COLUMN_UNUSED_BY_RUST` | `saml_logout_requests` | `processed_at` | SQL column 'processed_at' not mapped to any Rust field (nullable/has_default) |
| 20 | `SQL_COLUMN_UNUSED_BY_RUST` | `access_tokens` | `token` | SQL column 'token' not mapped to any Rust field (nullable/has_default) |
| 21 | `SQL_COLUMN_UNUSED_BY_RUST` | `thread_roots` | `is_fetched` | SQL column 'is_fetched' not mapped to any Rust field (nullable/has_default) |
| 22 | `SQL_COLUMN_UNUSED_BY_RUST` | `thread_roots` | `last_reply_event_id` | SQL column 'last_reply_event_id' not mapped to any Rust field (nullable/has_default) |
| 23 | `SQL_COLUMN_UNUSED_BY_RUST` | `thread_roots` | `last_reply_sender` | SQL column 'last_reply_sender' not mapped to any Rust field (nullable/has_default) |
| 24 | `SQL_COLUMN_UNUSED_BY_RUST` | `thread_roots` | `last_reply_ts` | SQL column 'last_reply_ts' not mapped to any Rust field (nullable/has_default) |
| 25 | `SQL_COLUMN_UNUSED_BY_RUST` | `thread_roots` | `reply_count` | SQL column 'reply_count' not mapped to any Rust field (nullable/has_default) |
| 26 | `SQL_COLUMN_UNUSED_BY_RUST` | `thread_roots` | `thread_id` | SQL column 'thread_id' not mapped to any Rust field (nullable/has_default) |
| 27 | `SQL_COLUMN_UNUSED_BY_RUST` | `thread_roots` | `is_fetched` | SQL column 'is_fetched' not mapped to any Rust field (nullable/has_default) |
| 28 | `SQL_COLUMN_UNUSED_BY_RUST` | `thread_roots` | `last_reply_event_id` | SQL column 'last_reply_event_id' not mapped to any Rust field (nullable/has_default) |
| 29 | `SQL_COLUMN_UNUSED_BY_RUST` | `thread_roots` | `last_reply_sender` | SQL column 'last_reply_sender' not mapped to any Rust field (nullable/has_default) |
| 30 | `SQL_COLUMN_UNUSED_BY_RUST` | `thread_roots` | `participants` | SQL column 'participants' not mapped to any Rust field (nullable/has_default) |
| 31 | `SQL_COLUMN_UNUSED_BY_RUST` | `thread_roots` | `reply_count` | SQL column 'reply_count' not mapped to any Rust field (nullable/has_default) |
| 32 | `SQL_COLUMN_UNUSED_BY_RUST` | `thread_roots` | `thread_id` | SQL column 'thread_id' not mapped to any Rust field (nullable/has_default) |
| 33 | `SQL_COLUMN_UNUSED_BY_RUST` | `thread_roots` | `updated_ts` | SQL column 'updated_ts' not mapped to any Rust field (nullable/has_default) |
| 34 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_reports` | `description` | SQL column 'description' not mapped to any Rust field (nullable/has_default) |
| 35 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_reports` | `event_json` | SQL column 'event_json' not mapped to any Rust field (nullable/has_default) |
| 36 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_reports` | `reported_user_id` | SQL column 'reported_user_id' not mapped to any Rust field (nullable/has_default) |
| 37 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_reports` | `resolution_reason` | SQL column 'resolution_reason' not mapped to any Rust field (nullable/has_default) |
| 38 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_reports` | `resolved_at` | SQL column 'resolved_at' not mapped to any Rust field (nullable/has_default) |
| 39 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_reports` | `status` | SQL column 'status' not mapped to any Rust field (nullable/has_default) |
| 40 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_report_stats` | `escalated_reports` | SQL column 'escalated_reports' not mapped to any Rust field (nullable/has_default) |
| 41 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_summaries` | `join_rules` | SQL column 'join_rules' not mapped to any Rust field (nullable/has_default) |
| 42 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_summary_state` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 43 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `auth_events` | SQL column 'auth_events' not mapped to any Rust field (nullable/has_default) |
| 44 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `hashes` | SQL column 'hashes' not mapped to any Rust field (nullable/has_default) |
| 45 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `is_redacted` | SQL column 'is_redacted' not mapped to any Rust field (nullable/has_default) |
| 46 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `prev_events` | SQL column 'prev_events' not mapped to any Rust field (nullable/has_default) |
| 47 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `processed_at` | SQL column 'processed_at' not mapped to any Rust field (nullable/has_default) |
| 48 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `redacted_at` | SQL column 'redacted_at' not mapped to any Rust field (nullable/has_default) |
| 49 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `redacted_by` | SQL column 'redacted_by' not mapped to any Rust field (nullable/has_default) |
| 50 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `signatures` | SQL column 'signatures' not mapped to any Rust field (nullable/has_default) |
| 51 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `transaction_id` | SQL column 'transaction_id' not mapped to any Rust field (nullable/has_default) |
| 52 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `unsigned` | SQL column 'unsigned' not mapped to any Rust field (nullable/has_default) |
| 53 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `user_id` | SQL column 'user_id' not mapped to any Rust field (nullable/has_default) |
| 54 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `auth_events` | SQL column 'auth_events' not mapped to any Rust field (nullable/has_default) |
| 55 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `hashes` | SQL column 'hashes' not mapped to any Rust field (nullable/has_default) |
| 56 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `prev_events` | SQL column 'prev_events' not mapped to any Rust field (nullable/has_default) |
| 57 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `processed_at` | SQL column 'processed_at' not mapped to any Rust field (nullable/has_default) |
| 58 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `redacted_at` | SQL column 'redacted_at' not mapped to any Rust field (nullable/has_default) |
| 59 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `redacted_by` | SQL column 'redacted_by' not mapped to any Rust field (nullable/has_default) |
| 60 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `signatures` | SQL column 'signatures' not mapped to any Rust field (nullable/has_default) |
| 61 | `SQL_COLUMN_UNUSED_BY_RUST` | `events` | `transaction_id` | SQL column 'transaction_id' not mapped to any Rust field (nullable/has_default) |
| 62 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `appservice_id` | SQL column 'appservice_id' not mapped to any Rust field (nullable/has_default) |
| 63 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `consent_version` | SQL column 'consent_version' not mapped to any Rust field (nullable/has_default) |
| 64 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `email` | SQL column 'email' not mapped to any Rust field (nullable/has_default) |
| 65 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `failed_login_attempts` | SQL column 'failed_login_attempts' not mapped to any Rust field (nullable/has_default) |
| 66 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `generation` | SQL column 'generation' not mapped to any Rust field (nullable/has_default) |
| 67 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `invalid_update_at` | SQL column 'invalid_update_at' not mapped to any Rust field (nullable/has_default) |
| 68 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `is_admin` | SQL column 'is_admin' not mapped to any Rust field (nullable/has_default) |
| 69 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `is_deactivated` | SQL column 'is_deactivated' not mapped to any Rust field (nullable/has_default) |
| 70 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `is_guest` | SQL column 'is_guest' not mapped to any Rust field (nullable/has_default) |
| 71 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `is_password_change_required` | SQL column 'is_password_change_required' not mapped to any Rust field (nullable/has_default) |
| 72 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `is_shadow_banned` | SQL column 'is_shadow_banned' not mapped to any Rust field (nullable/has_default) |
| 73 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `locked_until` | SQL column 'locked_until' not mapped to any Rust field (nullable/has_default) |
| 74 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `migration_state` | SQL column 'migration_state' not mapped to any Rust field (nullable/has_default) |
| 75 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `must_change_password` | SQL column 'must_change_password' not mapped to any Rust field (nullable/has_default) |
| 76 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `password_changed_ts` | SQL column 'password_changed_ts' not mapped to any Rust field (nullable/has_default) |
| 77 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `password_expires_at` | SQL column 'password_expires_at' not mapped to any Rust field (nullable/has_default) |
| 78 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `password_hash` | SQL column 'password_hash' not mapped to any Rust field (nullable/has_default) |
| 79 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `phone` | SQL column 'phone' not mapped to any Rust field (nullable/has_default) |
| 80 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `updated_ts` | SQL column 'updated_ts' not mapped to any Rust field (nullable/has_default) |
| 81 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `user_type` | SQL column 'user_type' not mapped to any Rust field (nullable/has_default) |
| 82 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `appservice_id` | SQL column 'appservice_id' not mapped to any Rust field (nullable/has_default) |
| 83 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `consent_version` | SQL column 'consent_version' not mapped to any Rust field (nullable/has_default) |
| 84 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `email` | SQL column 'email' not mapped to any Rust field (nullable/has_default) |
| 85 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `failed_login_attempts` | SQL column 'failed_login_attempts' not mapped to any Rust field (nullable/has_default) |
| 86 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `generation` | SQL column 'generation' not mapped to any Rust field (nullable/has_default) |
| 87 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `invalid_update_at` | SQL column 'invalid_update_at' not mapped to any Rust field (nullable/has_default) |
| 88 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `is_admin` | SQL column 'is_admin' not mapped to any Rust field (nullable/has_default) |
| 89 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `is_deactivated` | SQL column 'is_deactivated' not mapped to any Rust field (nullable/has_default) |
| 90 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `is_guest` | SQL column 'is_guest' not mapped to any Rust field (nullable/has_default) |
| 91 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `is_password_change_required` | SQL column 'is_password_change_required' not mapped to any Rust field (nullable/has_default) |
| 92 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `is_shadow_banned` | SQL column 'is_shadow_banned' not mapped to any Rust field (nullable/has_default) |
| 93 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `locked_until` | SQL column 'locked_until' not mapped to any Rust field (nullable/has_default) |
| 94 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `migration_state` | SQL column 'migration_state' not mapped to any Rust field (nullable/has_default) |
| 95 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `must_change_password` | SQL column 'must_change_password' not mapped to any Rust field (nullable/has_default) |
| 96 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `password_changed_ts` | SQL column 'password_changed_ts' not mapped to any Rust field (nullable/has_default) |
| 97 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `password_expires_at` | SQL column 'password_expires_at' not mapped to any Rust field (nullable/has_default) |
| 98 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `password_hash` | SQL column 'password_hash' not mapped to any Rust field (nullable/has_default) |
| 99 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `phone` | SQL column 'phone' not mapped to any Rust field (nullable/has_default) |
| 100 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `updated_ts` | SQL column 'updated_ts' not mapped to any Rust field (nullable/has_default) |
| 101 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `user_type` | SQL column 'user_type' not mapped to any Rust field (nullable/has_default) |
| 102 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `appservice_id` | SQL column 'appservice_id' not mapped to any Rust field (nullable/has_default) |
| 103 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `consent_version` | SQL column 'consent_version' not mapped to any Rust field (nullable/has_default) |
| 104 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `displayname` | SQL column 'displayname' not mapped to any Rust field (nullable/has_default) |
| 105 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `email` | SQL column 'email' not mapped to any Rust field (nullable/has_default) |
| 106 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `failed_login_attempts` | SQL column 'failed_login_attempts' not mapped to any Rust field (nullable/has_default) |
| 107 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `generation` | SQL column 'generation' not mapped to any Rust field (nullable/has_default) |
| 108 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `invalid_update_at` | SQL column 'invalid_update_at' not mapped to any Rust field (nullable/has_default) |
| 109 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `is_admin` | SQL column 'is_admin' not mapped to any Rust field (nullable/has_default) |
| 110 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `is_deactivated` | SQL column 'is_deactivated' not mapped to any Rust field (nullable/has_default) |
| 111 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `is_guest` | SQL column 'is_guest' not mapped to any Rust field (nullable/has_default) |
| 112 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `is_password_change_required` | SQL column 'is_password_change_required' not mapped to any Rust field (nullable/has_default) |
| 113 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `is_shadow_banned` | SQL column 'is_shadow_banned' not mapped to any Rust field (nullable/has_default) |
| 114 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `locked_until` | SQL column 'locked_until' not mapped to any Rust field (nullable/has_default) |
| 115 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `migration_state` | SQL column 'migration_state' not mapped to any Rust field (nullable/has_default) |
| 116 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `must_change_password` | SQL column 'must_change_password' not mapped to any Rust field (nullable/has_default) |
| 117 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `password_changed_ts` | SQL column 'password_changed_ts' not mapped to any Rust field (nullable/has_default) |
| 118 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `password_expires_at` | SQL column 'password_expires_at' not mapped to any Rust field (nullable/has_default) |
| 119 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `password_hash` | SQL column 'password_hash' not mapped to any Rust field (nullable/has_default) |
| 120 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `phone` | SQL column 'phone' not mapped to any Rust field (nullable/has_default) |
| 121 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `updated_ts` | SQL column 'updated_ts' not mapped to any Rust field (nullable/has_default) |
| 122 | `SQL_COLUMN_UNUSED_BY_RUST` | `users` | `user_type` | SQL column 'user_type' not mapped to any Rust field (nullable/has_default) |
| 123 | `SQL_COLUMN_UNUSED_BY_RUST` | `rendezvous_session` | `content` | SQL column 'content' not mapped to any Rust field (nullable/has_default) |
| 124 | `SQL_COLUMN_UNUSED_BY_RUST` | `rendezvous_session` | `updated_ts` | SQL column 'updated_ts' not mapped to any Rust field (nullable/has_default) |
| 125 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 126 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `invited_ts` | SQL column 'invited_ts' not mapped to any Rust field (nullable/has_default) |
| 127 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `avatar_url` | SQL column 'avatar_url' not mapped to any Rust field (nullable/has_default) |
| 128 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `ban_reason` | SQL column 'ban_reason' not mapped to any Rust field (nullable/has_default) |
| 129 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `banned_by` | SQL column 'banned_by' not mapped to any Rust field (nullable/has_default) |
| 130 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `banned_ts` | SQL column 'banned_ts' not mapped to any Rust field (nullable/has_default) |
| 131 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `display_name` | SQL column 'display_name' not mapped to any Rust field (nullable/has_default) |
| 132 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `event_id` | SQL column 'event_id' not mapped to any Rust field (nullable/has_default) |
| 133 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `event_type` | SQL column 'event_type' not mapped to any Rust field (nullable/has_default) |
| 134 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 135 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `invite_token` | SQL column 'invite_token' not mapped to any Rust field (nullable/has_default) |
| 136 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `invited_ts` | SQL column 'invited_ts' not mapped to any Rust field (nullable/has_default) |
| 137 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `is_banned` | SQL column 'is_banned' not mapped to any Rust field (nullable/has_default) |
| 138 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `join_reason` | SQL column 'join_reason' not mapped to any Rust field (nullable/has_default) |
| 139 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `joined_ts` | SQL column 'joined_ts' not mapped to any Rust field (nullable/has_default) |
| 140 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `left_ts` | SQL column 'left_ts' not mapped to any Rust field (nullable/has_default) |
| 141 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `reason` | SQL column 'reason' not mapped to any Rust field (nullable/has_default) |
| 142 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `sender` | SQL column 'sender' not mapped to any Rust field (nullable/has_default) |
| 143 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_memberships` | `updated_ts` | SQL column 'updated_ts' not mapped to any Rust field (nullable/has_default) |
| 144 | `SQL_COLUMN_UNUSED_BY_RUST` | `user_threepids` | `validated_ts` | SQL column 'validated_ts' not mapped to any Rust field (nullable/has_default) |
| 145 | `SQL_COLUMN_UNUSED_BY_RUST` | `user_threepids` | `verification_expires_ts` | SQL column 'verification_expires_ts' not mapped to any Rust field (nullable/has_default) |
| 146 | `SQL_COLUMN_UNUSED_BY_RUST` | `user_privacy_settings` | `allow_presence_lookup` | SQL column 'allow_presence_lookup' not mapped to any Rust field (nullable/has_default) |
| 147 | `SQL_COLUMN_UNUSED_BY_RUST` | `user_privacy_settings` | `allow_profile_lookup` | SQL column 'allow_profile_lookup' not mapped to any Rust field (nullable/has_default) |
| 148 | `SQL_COLUMN_UNUSED_BY_RUST` | `user_privacy_settings` | `allow_room_invites` | SQL column 'allow_room_invites' not mapped to any Rust field (nullable/has_default) |
| 149 | `SQL_COLUMN_UNUSED_BY_RUST` | `spaces` | `canonical_alias` | SQL column 'canonical_alias' not mapped to any Rust field (nullable/has_default) |
| 150 | `SQL_COLUMN_UNUSED_BY_RUST` | `spaces` | `history_visibility` | SQL column 'history_visibility' not mapped to any Rust field (nullable/has_default) |
| 151 | `SQL_COLUMN_UNUSED_BY_RUST` | `spaces` | `is_private` | SQL column 'is_private' not mapped to any Rust field (nullable/has_default) |
| 152 | `SQL_COLUMN_UNUSED_BY_RUST` | `spaces` | `member_count` | SQL column 'member_count' not mapped to any Rust field (nullable/has_default) |
| 153 | `SQL_COLUMN_UNUSED_BY_RUST` | `space_members` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 154 | `SQL_COLUMN_UNUSED_BY_RUST` | `spam_check_results` | `is_spam` | SQL column 'is_spam' not mapped to any Rust field (nullable/has_default) |
| 155 | `SQL_COLUMN_UNUSED_BY_RUST` | `spam_check_results` | `spam_score` | SQL column 'spam_score' not mapped to any Rust field (nullable/has_default) |
| 156 | `SQL_COLUMN_UNUSED_BY_RUST` | `third_party_rule_results` | `is_allowed` | SQL column 'is_allowed' not mapped to any Rust field (nullable/has_default) |
| 157 | `SQL_COLUMN_UNUSED_BY_RUST` | `third_party_rule_results` | `rule_details` | SQL column 'rule_details' not mapped to any Rust field (nullable/has_default) |
| 158 | `SQL_COLUMN_UNUSED_BY_RUST` | `third_party_rule_results` | `user_id` | SQL column 'user_id' not mapped to any Rust field (nullable/has_default) |
| 159 | `SQL_COLUMN_UNUSED_BY_RUST` | `module_execution_logs` | `input_data` | SQL column 'input_data' not mapped to any Rust field (nullable/has_default) |
| 160 | `SQL_COLUMN_UNUSED_BY_RUST` | `module_execution_logs` | `is_success` | SQL column 'is_success' not mapped to any Rust field (nullable/has_default) |
| 161 | `SQL_COLUMN_UNUSED_BY_RUST` | `module_execution_logs` | `module_id` | SQL column 'module_id' not mapped to any Rust field (nullable/has_default) |
| 162 | `SQL_COLUMN_UNUSED_BY_RUST` | `module_execution_logs` | `output_data` | SQL column 'output_data' not mapped to any Rust field (nullable/has_default) |
| 163 | `SQL_COLUMN_UNUSED_BY_RUST` | `account_validity` | `expiration_at` | SQL column 'expiration_at' not mapped to any Rust field (nullable/has_default) |
| 164 | `SQL_COLUMN_UNUSED_BY_RUST` | `account_validity` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 165 | `SQL_COLUMN_UNUSED_BY_RUST` | `account_validity` | `last_check_at` | SQL column 'last_check_at' not mapped to any Rust field (nullable/has_default) |
| 166 | `SQL_COLUMN_UNUSED_BY_RUST` | `presence_routes` | `config` | SQL column 'config' not mapped to any Rust field (nullable/has_default) |
| 167 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_callbacks` | `headers` | SQL column 'headers' not mapped to any Rust field (nullable/has_default) |
| 168 | `SQL_COLUMN_UNUSED_BY_RUST` | `rate_limit_callbacks` | `config` | SQL column 'config' not mapped to any Rust field (nullable/has_default) |
| 169 | `SQL_COLUMN_UNUSED_BY_RUST` | `account_data_callbacks` | `config` | SQL column 'config' not mapped to any Rust field (nullable/has_default) |
| 170 | `SQL_COLUMN_UNUSED_BY_RUST` | `account_data_callbacks` | `data_types` | SQL column 'data_types' not mapped to any Rust field (nullable/has_default) |
| 171 | `SQL_COLUMN_UNUSED_BY_RUST` | `push_device` | `is_enabled` | SQL column 'is_enabled' not mapped to any Rust field (nullable/has_default) |
| 172 | `SQL_COLUMN_UNUSED_BY_RUST` | `push_rules` | `is_enabled` | SQL column 'is_enabled' not mapped to any Rust field (nullable/has_default) |
| 173 | `SQL_COLUMN_UNUSED_BY_RUST` | `push_notification_queue` | `is_processed` | SQL column 'is_processed' not mapped to any Rust field (nullable/has_default) |
| 174 | `SQL_COLUMN_UNUSED_BY_RUST` | `push_notification_queue` | `processed_at` | SQL column 'processed_at' not mapped to any Rust field (nullable/has_default) |
| 175 | `SQL_COLUMN_UNUSED_BY_RUST` | `push_notification_log` | `last_attempt_at` | SQL column 'last_attempt_at' not mapped to any Rust field (nullable/has_default) |
| 176 | `SQL_COLUMN_UNUSED_BY_RUST` | `push_notification_log` | `retry_count` | SQL column 'retry_count' not mapped to any Rust field (nullable/has_default) |
| 177 | `SQL_COLUMN_UNUSED_BY_RUST` | `application_service_state` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 178 | `SQL_COLUMN_UNUSED_BY_RUST` | `application_service_events` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 179 | `SQL_COLUMN_UNUSED_BY_RUST` | `application_service_events` | `processed` | SQL column 'processed' not mapped to any Rust field (nullable/has_default) |
| 180 | `SQL_COLUMN_UNUSED_BY_RUST` | `application_service_transactions` | `data` | SQL column 'data' not mapped to any Rust field (nullable/has_default) |
| 181 | `SQL_COLUMN_UNUSED_BY_RUST` | `application_service_transactions` | `processed` | SQL column 'processed' not mapped to any Rust field (nullable/has_default) |
| 182 | `SQL_COLUMN_UNUSED_BY_RUST` | `application_service_transactions` | `processed_ts` | SQL column 'processed_ts' not mapped to any Rust field (nullable/has_default) |
| 183 | `SQL_COLUMN_UNUSED_BY_RUST` | `cas_tickets` | `consumed_at` | SQL column 'consumed_at' not mapped to any Rust field (nullable/has_default) |
| 184 | `SQL_COLUMN_UNUSED_BY_RUST` | `cas_proxy_tickets` | `consumed_at` | SQL column 'consumed_at' not mapped to any Rust field (nullable/has_default) |
| 185 | `SQL_COLUMN_UNUSED_BY_RUST` | `cas_slo_sessions` | `logout_sent_at` | SQL column 'logout_sent_at' not mapped to any Rust field (nullable/has_default) |
| 186 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_quota_config` | `allowed_content_types` | SQL column 'allowed_content_types' not mapped to any Rust field (nullable/has_default) |
| 187 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_quota_config` | `max_file_size` | SQL column 'max_file_size' not mapped to any Rust field (nullable/has_default) |
| 188 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_quota_config` | `max_upload_rate` | SQL column 'max_upload_rate' not mapped to any Rust field (nullable/has_default) |
| 189 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_quota_config` | `retention_days` | SQL column 'retention_days' not mapped to any Rust field (nullable/has_default) |
| 190 | `SQL_COLUMN_UNUSED_BY_RUST` | `user_media_quota` | `file_count` | SQL column 'file_count' not mapped to any Rust field (nullable/has_default) |
| 191 | `SQL_COLUMN_UNUSED_BY_RUST` | `user_media_quota` | `max_bytes` | SQL column 'max_bytes' not mapped to any Rust field (nullable/has_default) |
| 192 | `SQL_COLUMN_UNUSED_BY_RUST` | `user_media_quota` | `used_bytes` | SQL column 'used_bytes' not mapped to any Rust field (nullable/has_default) |
| 193 | `SQL_COLUMN_UNUSED_BY_RUST` | `background_updates` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 194 | `SQL_COLUMN_UNUSED_BY_RUST` | `background_updates` | `is_running` | SQL column 'is_running' not mapped to any Rust field (nullable/has_default) |
| 195 | `SQL_COLUMN_UNUSED_BY_RUST` | `background_updates` | `updated_ts` | SQL column 'updated_ts' not mapped to any Rust field (nullable/has_default) |
| 196 | `SQL_COLUMN_UNUSED_BY_RUST` | `registration_captcha` | `used_at` | SQL column 'used_at' not mapped to any Rust field (nullable/has_default) |
| 197 | `SQL_COLUMN_UNUSED_BY_RUST` | `registration_captcha` | `verified_at` | SQL column 'verified_at' not mapped to any Rust field (nullable/has_default) |
| 198 | `SQL_COLUMN_UNUSED_BY_RUST` | `sliding_sync_rooms` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 199 | `SQL_COLUMN_UNUSED_BY_RUST` | `sliding_sync_rooms` | `timestamp` | SQL column 'timestamp' not mapped to any Rust field (nullable/has_default) |
| 200 | `SQL_COLUMN_UNUSED_BY_RUST` | `workers` | `is_enabled` | SQL column 'is_enabled' not mapped to any Rust field (nullable/has_default) |
| 201 | `SQL_COLUMN_UNUSED_BY_RUST` | `e2ee_audit_log` | `event_id` | SQL column 'event_id' not mapped to any Rust field (nullable/has_default) |
| 202 | `TABLE_NO_RUST_STRUCT` | `account_data` | `` | Table 'account_data' (6 columns) has no matching Rust FromRow struct |
| 203 | `TABLE_NO_RUST_STRUCT` | `application_service_room_alias_namespaces` | `` | Table 'application_service_room_alias_namespaces' (5 columns) has no matching Rust FromRow struct |
| 204 | `TABLE_NO_RUST_STRUCT` | `application_service_room_namespaces` | `` | Table 'application_service_room_namespaces' (5 columns) has no matching Rust FromRow struct |
| 205 | `TABLE_NO_RUST_STRUCT` | `application_service_statistics` | `` | Table 'application_service_statistics' (10 columns) has no matching Rust FromRow struct |
| 206 | `TABLE_NO_RUST_STRUCT` | `blocked_rooms` | `` | Table 'blocked_rooms' (5 columns) has no matching Rust FromRow struct |
| 207 | `TABLE_NO_RUST_STRUCT` | `blocked_users` | `` | Table 'blocked_users' (5 columns) has no matching Rust FromRow struct |
| 208 | `TABLE_NO_RUST_STRUCT` | `cross_signing_keys` | `` | Table 'cross_signing_keys' (6 columns) has no matching Rust FromRow struct |
| 209 | `TABLE_NO_RUST_STRUCT` | `cross_signing_trust` | `` | Table 'cross_signing_trust' (8 columns) has no matching Rust FromRow struct |
| 210 | `TABLE_NO_RUST_STRUCT` | `db_metadata` | `` | Table 'db_metadata' (5 columns) has no matching Rust FromRow struct |
| 211 | `TABLE_NO_RUST_STRUCT` | `dehydrated_devices` | `` | Table 'dehydrated_devices' (9 columns) has no matching Rust FromRow struct |
| 212 | `TABLE_NO_RUST_STRUCT` | `delayed_events` | `` | Table 'delayed_events' (14 columns) has no matching Rust FromRow struct |
| 213 | `TABLE_NO_RUST_STRUCT` | `deleted_events_index` | `` | Table 'deleted_events_index' (5 columns) has no matching Rust FromRow struct |
| 214 | `TABLE_NO_RUST_STRUCT` | `device_keys` | `` | Table 'device_keys' (16 columns) has no matching Rust FromRow struct |
| 215 | `TABLE_NO_RUST_STRUCT` | `device_lists_changes` | `` | Table 'device_lists_changes' (6 columns) has no matching Rust FromRow struct |
| 216 | `TABLE_NO_RUST_STRUCT` | `device_lists_stream` | `` | Table 'device_lists_stream' (4 columns) has no matching Rust FromRow struct |
| 217 | `TABLE_NO_RUST_STRUCT` | `device_signatures` | `` | Table 'device_signatures' (8 columns) has no matching Rust FromRow struct |
| 218 | `TABLE_NO_RUST_STRUCT` | `device_trust_status` | `` | Table 'device_trust_status' (8 columns) has no matching Rust FromRow struct |
| 219 | `TABLE_NO_RUST_STRUCT` | `device_verification_request` | `` | Table 'device_verification_request' (12 columns) has no matching Rust FromRow struct |
| 220 | `TABLE_NO_RUST_STRUCT` | `e2ee_key_requests` | `` | Table 'e2ee_key_requests' (13 columns) has no matching Rust FromRow struct |
| 221 | `TABLE_NO_RUST_STRUCT` | `e2ee_secret_storage_keys` | `` | Table 'e2ee_secret_storage_keys' (12 columns) has no matching Rust FromRow struct |
| 222 | `TABLE_NO_RUST_STRUCT` | `e2ee_security_events` | `` | Table 'e2ee_security_events' (8 columns) has no matching Rust FromRow struct |
| 223 | `TABLE_NO_RUST_STRUCT` | `e2ee_stored_secrets` | `` | Table 'e2ee_stored_secrets' (9 columns) has no matching Rust FromRow struct |
| 224 | `TABLE_NO_RUST_STRUCT` | `event_receipts` | `` | Table 'event_receipts' (9 columns) has no matching Rust FromRow struct |
| 225 | `TABLE_NO_RUST_STRUCT` | `feature_flag_targets` | `` | Table 'feature_flag_targets' (5 columns) has no matching Rust FromRow struct |
| 226 | `TABLE_NO_RUST_STRUCT` | `federation_blacklist_config` | `` | Table 'federation_blacklist_config' (10 columns) has no matching Rust FromRow struct |
| 227 | `TABLE_NO_RUST_STRUCT` | `federation_cache` | `` | Table 'federation_cache' (5 columns) has no matching Rust FromRow struct |
| 228 | `TABLE_NO_RUST_STRUCT` | `federation_queue` | `` | Table 'federation_queue' (10 columns) has no matching Rust FromRow struct |
| 229 | `TABLE_NO_RUST_STRUCT` | `federation_servers` | `` | Table 'federation_servers' (8 columns) has no matching Rust FromRow struct |
| 230 | `TABLE_NO_RUST_STRUCT` | `friend_categories` | `` | Table 'friend_categories' (5 columns) has no matching Rust FromRow struct |
| 231 | `TABLE_NO_RUST_STRUCT` | `friends` | `` | Table 'friends' (4 columns) has no matching Rust FromRow struct |
| 232 | `TABLE_NO_RUST_STRUCT` | `ip_blocks` | `` | Table 'ip_blocks' (5 columns) has no matching Rust FromRow struct |
| 233 | `TABLE_NO_RUST_STRUCT` | `ip_reputation` | `` | Table 'ip_reputation' (0 columns) has no matching Rust FromRow struct |
| 234 | `TABLE_NO_RUST_STRUCT` | `key_rotation_history` | `` | Table 'key_rotation_history' (7 columns) has no matching Rust FromRow struct |
| 235 | `TABLE_NO_RUST_STRUCT` | `key_rotation_log` | `` | Table 'key_rotation_log' (9 columns) has no matching Rust FromRow struct |
| 236 | `TABLE_NO_RUST_STRUCT` | `key_signatures` | `` | Table 'key_signatures' (7 columns) has no matching Rust FromRow struct |
| 237 | `TABLE_NO_RUST_STRUCT` | `lazy_loaded_members` | `` | Table 'lazy_loaded_members' (6 columns) has no matching Rust FromRow struct |
| 238 | `TABLE_NO_RUST_STRUCT` | `leak_alerts` | `` | Table 'leak_alerts' (10 columns) has no matching Rust FromRow struct |
| 239 | `TABLE_NO_RUST_STRUCT` | `media_metadata` | `` | Table 'media_metadata' (9 columns) has no matching Rust FromRow struct |
| 240 | `TABLE_NO_RUST_STRUCT` | `media_quota` | `` | Table 'media_quota' (6 columns) has no matching Rust FromRow struct |
| 241 | `TABLE_NO_RUST_STRUCT` | `megolm_sessions` | `` | Table 'megolm_sessions' (10 columns) has no matching Rust FromRow struct |
| 242 | `TABLE_NO_RUST_STRUCT` | `migration_audit` | `` | Table 'migration_audit' (11 columns) has no matching Rust FromRow struct |
| 243 | `TABLE_NO_RUST_STRUCT` | `moderation_actions` | `` | Table 'moderation_actions' (10 columns) has no matching Rust FromRow struct |
| 244 | `TABLE_NO_RUST_STRUCT` | `notifications` | `` | Table 'notifications' (10 columns) has no matching Rust FromRow struct |
| 245 | `TABLE_NO_RUST_STRUCT` | `olm_accounts` | `` | Table 'olm_accounts' (9 columns) has no matching Rust FromRow struct |
| 246 | `TABLE_NO_RUST_STRUCT` | `olm_sessions` | `` | Table 'olm_sessions' (11 columns) has no matching Rust FromRow struct |
| 247 | `TABLE_NO_RUST_STRUCT` | `one_time_keys` | `` | Table 'one_time_keys' (10 columns) has no matching Rust FromRow struct |
| 248 | `TABLE_NO_RUST_STRUCT` | `password_history` | `` | Table 'password_history' (4 columns) has no matching Rust FromRow struct |
| 249 | `TABLE_NO_RUST_STRUCT` | `password_policy` | `` | Table 'password_policy' (5 columns) has no matching Rust FromRow struct |
| 250 | `TABLE_NO_RUST_STRUCT` | `presence` | `` | Table 'presence' (7 columns) has no matching Rust FromRow struct |
| 251 | `TABLE_NO_RUST_STRUCT` | `presence_subscriptions` | `` | Table 'presence_subscriptions' (3 columns) has no matching Rust FromRow struct |
| 252 | `TABLE_NO_RUST_STRUCT` | `push_config` | `` | Table 'push_config' (9 columns) has no matching Rust FromRow struct |
| 253 | `TABLE_NO_RUST_STRUCT` | `push_devices` | `` | Table 'push_devices' (14 columns) has no matching Rust FromRow struct |
| 254 | `TABLE_NO_RUST_STRUCT` | `pushers` | `` | Table 'pushers' (15 columns) has no matching Rust FromRow struct |
| 255 | `TABLE_NO_RUST_STRUCT` | `qr_login_transactions` | `` | Table 'qr_login_transactions' (7 columns) has no matching Rust FromRow struct |
| 256 | `TABLE_NO_RUST_STRUCT` | `rate_limits` | `` | Table 'rate_limits' (4 columns) has no matching Rust FromRow struct |
| 257 | `TABLE_NO_RUST_STRUCT` | `reaction_aggregations` | `` | Table 'reaction_aggregations' (7 columns) has no matching Rust FromRow struct |
| 258 | `TABLE_NO_RUST_STRUCT` | `read_markers` | `` | Table 'read_markers' (7 columns) has no matching Rust FromRow struct |
| 259 | `TABLE_NO_RUST_STRUCT` | `replication_positions` | `` | Table 'replication_positions' (5 columns) has no matching Rust FromRow struct |
| 260 | `TABLE_NO_RUST_STRUCT` | `retention_cleanup_queue` | `` | Table 'retention_cleanup_queue' (11 columns) has no matching Rust FromRow struct |
| 261 | `TABLE_NO_RUST_STRUCT` | `room_account_data` | `` | Table 'room_account_data' (7 columns) has no matching Rust FromRow struct |
| 262 | `TABLE_NO_RUST_STRUCT` | `room_aliases` | `` | Table 'room_aliases' (4 columns) has no matching Rust FromRow struct |
| 263 | `TABLE_NO_RUST_STRUCT` | `room_children` | `` | Table 'room_children' (8 columns) has no matching Rust FromRow struct |
| 264 | `TABLE_NO_RUST_STRUCT` | `room_directory` | `` | Table 'room_directory' (6 columns) has no matching Rust FromRow struct |
| 265 | `TABLE_NO_RUST_STRUCT` | `room_ephemeral` | `` | Table 'room_ephemeral' (9 columns) has no matching Rust FromRow struct |
| 266 | `TABLE_NO_RUST_STRUCT` | `room_events` | `` | Table 'room_events' (10 columns) has no matching Rust FromRow struct |
| 267 | `TABLE_NO_RUST_STRUCT` | `room_invite_allowlist` | `` | Table 'room_invite_allowlist' (4 columns) has no matching Rust FromRow struct |
| 268 | `TABLE_NO_RUST_STRUCT` | `room_invite_blocklist` | `` | Table 'room_invite_blocklist' (4 columns) has no matching Rust FromRow struct |
| 269 | `TABLE_NO_RUST_STRUCT` | `room_parents` | `` | Table 'room_parents' (7 columns) has no matching Rust FromRow struct |
| 270 | `TABLE_NO_RUST_STRUCT` | `room_state_events` | `` | Table 'room_state_events' (7 columns) has no matching Rust FromRow struct |
| 271 | `TABLE_NO_RUST_STRUCT` | `room_sticky_events` | `` | Table 'room_sticky_events' (8 columns) has no matching Rust FromRow struct |
| 272 | `TABLE_NO_RUST_STRUCT` | `rooms` | `` | Table 'rooms' (15 columns) has no matching Rust FromRow struct |
| 273 | `TABLE_NO_RUST_STRUCT` | `schema_migrations` | `` | Table 'schema_migrations' (8 columns) has no matching Rust FromRow struct |
| 274 | `TABLE_NO_RUST_STRUCT` | `search_index` | `` | Table 'search_index' (9 columns) has no matching Rust FromRow struct |
| 275 | `TABLE_NO_RUST_STRUCT` | `secure_backup_session_keys` | `` | Table 'secure_backup_session_keys' (6 columns) has no matching Rust FromRow struct |
| 276 | `TABLE_NO_RUST_STRUCT` | `secure_key_backups` | `` | Table 'secure_key_backups' (8 columns) has no matching Rust FromRow struct |
| 277 | `TABLE_NO_RUST_STRUCT` | `security_events` | `` | Table 'security_events' (7 columns) has no matching Rust FromRow struct |
| 278 | `TABLE_NO_RUST_STRUCT` | `server_notices` | `` | Table 'server_notices' (5 columns) has no matching Rust FromRow struct |
| 279 | `TABLE_NO_RUST_STRUCT` | `space_hierarchy` | `` | Table 'space_hierarchy' (9 columns) has no matching Rust FromRow struct |
| 280 | `TABLE_NO_RUST_STRUCT` | `space_statistics` | `` | Table 'space_statistics' (7 columns) has no matching Rust FromRow struct |
| 281 | `TABLE_NO_RUST_STRUCT` | `sync_stream_id` | `` | Table 'sync_stream_id' (4 columns) has no matching Rust FromRow struct |
| 282 | `TABLE_NO_RUST_STRUCT` | `thumbnails` | `` | Table 'thumbnails' (8 columns) has no matching Rust FromRow struct |
| 283 | `TABLE_NO_RUST_STRUCT` | `to_device_messages` | `` | Table 'to_device_messages' (10 columns) has no matching Rust FromRow struct |
| 284 | `TABLE_NO_RUST_STRUCT` | `to_device_transactions` | `` | Table 'to_device_transactions' (5 columns) has no matching Rust FromRow struct |
| 285 | `TABLE_NO_RUST_STRUCT` | `token_blacklist` | `` | Table 'token_blacklist' (8 columns) has no matching Rust FromRow struct |
| 286 | `TABLE_NO_RUST_STRUCT` | `typing` | `` | Table 'typing' (4 columns) has no matching Rust FromRow struct |
| 287 | `TABLE_NO_RUST_STRUCT` | `upload_chunks` | `` | Table 'upload_chunks' (5 columns) has no matching Rust FromRow struct |
| 288 | `TABLE_NO_RUST_STRUCT` | `user_account_data` | `` | Table 'user_account_data' (5 columns) has no matching Rust FromRow struct |
| 289 | `TABLE_NO_RUST_STRUCT` | `user_directory` | `` | Table 'user_directory' (5 columns) has no matching Rust FromRow struct |
| 290 | `TABLE_NO_RUST_STRUCT` | `user_filters` | `` | Table 'user_filters' (5 columns) has no matching Rust FromRow struct |
| 291 | `TABLE_NO_RUST_STRUCT` | `user_notification_settings` | `` | Table 'user_notification_settings' (3 columns) has no matching Rust FromRow struct |
| 292 | `TABLE_NO_RUST_STRUCT` | `user_reputations` | `` | Table 'user_reputations' (11 columns) has no matching Rust FromRow struct |
| 293 | `TABLE_NO_RUST_STRUCT` | `user_settings` | `` | Table 'user_settings' (6 columns) has no matching Rust FromRow struct |
| 294 | `TABLE_NO_RUST_STRUCT` | `verification_qr` | `` | Table 'verification_qr' (6 columns) has no matching Rust FromRow struct |
| 295 | `TABLE_NO_RUST_STRUCT` | `verification_requests` | `` | Table 'verification_requests' (9 columns) has no matching Rust FromRow struct |
| 296 | `TABLE_NO_RUST_STRUCT` | `verification_sas` | `` | Table 'verification_sas' (10 columns) has no matching Rust FromRow struct |
| 297 | `TABLE_NO_RUST_STRUCT` | `voice_messages` | `` | Table 'voice_messages' (17 columns) has no matching Rust FromRow struct |
| 298 | `TABLE_NO_RUST_STRUCT` | `voice_usage_stats` | `` | Table 'voice_usage_stats' (14 columns) has no matching Rust FromRow struct |
| 299 | `TABLE_NO_RUST_STRUCT` | `worker_connections` | `` | Table 'worker_connections' (11 columns) has no matching Rust FromRow struct |
| 300 | `TABLE_NO_RUST_STRUCT` | `worker_load_stats` | `` | Table 'worker_load_stats' (9 columns) has no matching Rust FromRow struct |
| 301 | `TABLE_NO_RUST_STRUCT` | `worker_statistics` | `` | Table 'worker_statistics' (11 columns) has no matching Rust FromRow struct |

## INFO Issues (9)

| # | Type | Table / Struct | Field / Column | Detail |
|---|------|----------------|-----------------|--------|
| 1 | `NO_TABLE_MATCH` | `FeatureFlagTargetRecord` | `` | Struct 'FeatureFlagTargetRecord' has no obvious matching SQL table |
| 2 | `NO_TABLE_MATCH` | `TokenBlacklistEntry` | `` | Struct 'TokenBlacklistEntry' has no obvious matching SQL table |
| 3 | `NO_TABLE_MATCH` | `RefreshTokenStats` | `` | Struct 'RefreshTokenStats' has no obvious matching SQL table |
| 4 | `NO_TABLE_MATCH` | `EventReportId` | `` | Struct 'EventReportId' has no obvious matching SQL table |
| 5 | `NO_TABLE_MATCH` | `SpaceHierarchyRoom` | `` | Struct 'SpaceHierarchyRoom' has no obvious matching SQL table |
| 6 | `NO_TABLE_MATCH` | `RetentionCleanupQueueItem` | `` | Struct 'RetentionCleanupQueueItem' has no obvious matching SQL table |
| 7 | `NO_TABLE_MATCH` | `DeletedEventIndex` | `` | Struct 'DeletedEventIndex' has no obvious matching SQL table |
| 8 | `NO_TABLE_MATCH` | `CaptchaRateLimit` | `` | Struct 'CaptchaRateLimit' has no obvious matching SQL table |
| 9 | `NO_TABLE_MATCH` | `DeviceVerificationStatus` | `` | Struct 'DeviceVerificationStatus' has no obvious matching SQL table |

## Orphan Tables (No Matching Rust Struct)

| Table | Columns | Source |
|-------|---------|--------|
`account_data` | 6 | 00000000_unified_schema_v6.sql |
`application_service_room_alias_namespaces` | 5 | 00000000_unified_schema_v6.sql |
`application_service_room_namespaces` | 5 | 00000000_unified_schema_v6.sql |
`application_service_statistics` | 10 | 20260401000001_consolidated_schema_additions.sql |
`blocked_rooms` | 5 | 00000000_unified_schema_v6.sql |
`blocked_users` | 5 | 00000000_unified_schema_v6.sql |
`cross_signing_keys` | 6 | 00000000_unified_schema_v6.sql |
`cross_signing_trust` | 8 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
`db_metadata` | 5 | 00000000_unified_schema_v6.sql |
`dehydrated_devices` | 9 | 20260401000001_consolidated_schema_additions.sql |
`delayed_events` | 14 | 20260401000001_consolidated_schema_additions.sql |
`deleted_events_index` | 5 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
`device_keys` | 16 | 00000000_unified_schema_v6.sql+ALTER(20260422000001_schema_code_alignment.sql) |
`device_lists_changes` | 6 | 00000000_unified_schema_v6.sql |
`device_lists_stream` | 4 | 00000000_unified_schema_v6.sql |
`device_signatures` | 8 | 00000000_unified_schema_v6.sql |
`device_trust_status` | 8 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
`device_verification_request` | 12 | 00000000_unified_schema_v6.sql |
`e2ee_key_requests` | 13 | 00000000_unified_schema_v6.sql+ALTER(20260422000001_schema_code_alignment.sql) |
`e2ee_secret_storage_keys` | 12 | 20260401000001_consolidated_schema_additions.sql+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql) |
`e2ee_security_events` | 8 | 00000000_unified_schema_v6.sql |
`e2ee_stored_secrets` | 9 | 20260401000001_consolidated_schema_additions.sql+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql) |
`event_receipts` | 9 | 00000000_unified_schema_v6.sql |
`feature_flag_targets` | 5 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
`federation_blacklist_config` | 10 | 20260401000001_consolidated_schema_additions.sql |
`federation_cache` | 5 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
`federation_queue` | 10 | 00000000_unified_schema_v6.sql |
`federation_servers` | 8 | 00000000_unified_schema_v6.sql |
`friend_categories` | 5 | 00000000_unified_schema_v6.sql+00000001_extensions_friends.sql |
`friends` | 4 | 00000000_unified_schema_v6.sql+00000001_extensions_friends.sql |
`ip_blocks` | 5 | 00000000_unified_schema_v6.sql |
`ip_reputation` | 0 | 00000000_unified_schema_v6.sql |
`key_rotation_history` | 7 | 00000000_unified_schema_v6.sql |
`key_rotation_log` | 9 | 00000000_unified_schema_v6.sql |
`key_signatures` | 7 | 00000000_unified_schema_v6.sql |
`lazy_loaded_members` | 6 | 00000000_unified_schema_v6.sql+20260410000001_consolidated_feature_additions.sql |
`leak_alerts` | 10 | 20260401000001_consolidated_schema_additions.sql |
`media_metadata` | 9 | 00000000_unified_schema_v6.sql |
`media_quota` | 6 | 00000000_unified_schema_v6.sql |
`megolm_sessions` | 10 | 00000000_unified_schema_v6.sql |
`migration_audit` | 11 | 20260401000001_consolidated_schema_additions.sql |
`moderation_actions` | 10 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
`notifications` | 10 | 00000000_unified_schema_v6.sql |
`olm_accounts` | 9 | 00000000_unified_schema_v6.sql |
`olm_sessions` | 11 | 00000000_unified_schema_v6.sql |
`one_time_keys` | 10 | 00000000_unified_schema_v6.sql |
`password_history` | 4 | 00000000_unified_schema_v6.sql |
`password_policy` | 5 | 00000000_unified_schema_v6.sql |
`presence` | 7 | 00000000_unified_schema_v6.sql |
`presence_subscriptions` | 3 | 00000000_unified_schema_v6.sql |
`push_config` | 9 | 00000000_unified_schema_v6.sql+ALTER(20260422000001_schema_code_alignment.sql)+ALTER(20260422000001_schema_code_alignment.sql) |
`push_devices` | 14 | 00000000_unified_schema_v6.sql |
`pushers` | 15 | 00000000_unified_schema_v6.sql |
`qr_login_transactions` | 7 | 20260401000001_consolidated_schema_additions.sql |
`rate_limits` | 4 | 20260401000001_consolidated_schema_additions.sql |
`reaction_aggregations` | 7 | 20260401000001_consolidated_schema_additions.sql |
`read_markers` | 7 | 00000000_unified_schema_v6.sql |
`replication_positions` | 5 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
`retention_cleanup_queue` | 11 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
`room_account_data` | 7 | 00000000_unified_schema_v6.sql |
`room_aliases` | 4 | 00000000_unified_schema_v6.sql |
`room_children` | 8 | 20260401000001_consolidated_schema_additions.sql |
`room_directory` | 6 | 00000000_unified_schema_v6.sql |
`room_ephemeral` | 9 | 00000000_unified_schema_v6.sql |
`room_events` | 10 | 00000000_unified_schema_v6.sql |
`room_invite_allowlist` | 4 | 00000000_unified_schema_v6.sql |
`room_invite_blocklist` | 4 | 00000000_unified_schema_v6.sql |
`room_parents` | 7 | 00000000_unified_schema_v6.sql |
`room_state_events` | 7 | 00000000_unified_schema_v6.sql |
`room_sticky_events` | 8 | 00000000_unified_schema_v6.sql |
`rooms` | 15 | 00000000_unified_schema_v6.sql |
`schema_migrations` | 8 | 00000000_unified_schema_v6.sql |
`search_index` | 9 | 00000000_unified_schema_v6.sql |
`secure_backup_session_keys` | 6 | 20260401000001_consolidated_schema_additions.sql |
`secure_key_backups` | 8 | 20260401000001_consolidated_schema_additions.sql |
`security_events` | 7 | 00000000_unified_schema_v6.sql |
`server_notices` | 5 | 20260401000001_consolidated_schema_additions.sql |
`space_hierarchy` | 9 | 00000000_unified_schema_v6.sql |
`space_statistics` | 7 | 00000000_unified_schema_v6.sql |
`sync_stream_id` | 4 | 00000000_unified_schema_v6.sql |
`thumbnails` | 8 | 00000000_unified_schema_v6.sql |
`to_device_messages` | 10 | 00000000_unified_schema_v6.sql |
`to_device_transactions` | 5 | 20260422000001_schema_code_alignment.sql |
`token_blacklist` | 8 | 00000000_unified_schema_v6.sql |
`typing` | 4 | 00000000_unified_schema_v6.sql |
`upload_chunks` | 5 | 20260401000001_consolidated_schema_additions.sql |
`user_account_data` | 5 | 00000000_unified_schema_v6.sql |
`user_directory` | 5 | 00000000_unified_schema_v6.sql |
`user_filters` | 5 | 00000000_unified_schema_v6.sql |
`user_notification_settings` | 3 | 20260401000001_consolidated_schema_additions.sql |
`user_reputations` | 11 | 00000000_unified_schema_v6.sql |
`user_settings` | 6 | 20260401000001_consolidated_schema_additions.sql |
`verification_qr` | 6 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
`verification_requests` | 9 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
`verification_sas` | 10 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
`voice_messages` | 17 | 00000000_unified_schema_v6.sql+00000001_extensions_voice.sql |
`voice_usage_stats` | 14 | 00000000_unified_schema_v6.sql+00000001_extensions_voice.sql |
`worker_connections` | 11 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
`worker_load_stats` | 9 | 00000000_unified_schema_v6.sql+20260401000001_consolidated_schema_additions.sql |
`worker_statistics` | 11 | 00000000_unified_schema_v6.sql |
