# Database Migration Full Audit Report

> **Date**: 2026-05-29
> **Scope**: All active migration files vs all `FromRow` Rust structs
> **SQL Tables**: 255 | **Rust Structs**: 167 | **Matched Pairs**: 148

## Issue Summary

| Severity | Count | Description |
|----------|-------|-------------|
| CRITICAL | 16 | Runtime crash (missing column / type panic) |
| HIGH | 345 | Conditional runtime error (nullable mismatch, missing field) |
| MEDIUM | 7 | Naming inconsistency / overly permissive types |
| LOW | 190 | Unused SQL column / no Rust struct |
| INFO | 19 | Informational only |
| **Total** | **577** | |

## Matched Table-Struct Pairs

| SQL Table | Rust Struct | SQL Cols | Rust Fields | Source |
|----------|-------------|----------|--------------|--------|
| `access_tokens` | `AccessToken` | 11 | 10 | 00000000_unified_schema_v7.sql |
| `account_data_callbacks` | `AccountDataCallback` | 6 | 7 | 00000000_unified_schema_v7.sql |
| `account_validity` | `AccountValidity` | 8 | 7 | 00000000_unified_schema_v7.sql |
| `ai_chat_roles` | `AiChatRole` | 14 | 14 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `ai_connections` | `AiConnection` | 7 | 7 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `ai_conversations` | `AiConversation` | 12 | 12 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `ai_generations` | `AiGeneration` | 12 | 11 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `ai_messages` | `AiMessage` | 9 | 9 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `application_service_events` | `ApplicationServiceEvent` | 8 | 10 | 00000000_unified_schema_v7.sql |
| `application_service_state` | `ApplicationServiceState` | 6 | 4 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql) |
| `application_service_transactions` | `ApplicationServiceTransaction` | 13 | 8 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql) |
| `application_service_user_namespaces` | `ApplicationServiceNamespace` | 5 | 6 | 00000000_unified_schema_v7.sql |
| `application_service_users` | `ApplicationServiceUser` | 5 | 5 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `application_services` | `ApplicationService` | 15 | 15 | 00000000_unified_schema_v7.sql |
| `audit_events` | `AuditEvent` | 9 | 9 | 00000000_unified_schema_v7.sql |
| `background_update_history` | `BackgroundUpdateHistory` | 8 | 8 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `background_update_locks` | `BackgroundUpdateLock` | 4 | 4 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `background_update_stats` | `BackgroundUpdateStats` | 10 | 10 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `background_updates` | `BackgroundUpdate` | 23 | 20 | 00000000_unified_schema_v7.sql |
| `backup_keys` | `BackupKeyInfo` | 9 | 8 | 00000000_unified_schema_v7.sql |
| `beacon_info` | `BeaconInfo` | 12 | 12 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `beacon_locations` | `BeaconLocation` | 10 | 10 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `burn_after_read_log` | `BurnStatsRow` | 5 | 3 | 20260515120000_burn_after_read_persistence.sql |
| `burn_after_read_settings` | `BurnSettingsRow` | 6 | 6 | 20260515120000_burn_after_read_persistence.sql |
| `call_candidates` | `CallCandidate` | 6 | 6 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `call_sessions` | `CallSession` | 12 | 12 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `captcha_config` | `CaptchaConfig` | 6 | 6 | 00000000_unified_schema_v7.sql |
| `captcha_send_log` | `CaptchaSendLog` | 11 | 11 | 00000000_unified_schema_v7.sql |
| `captcha_template` | `CaptchaTemplate` | 10 | 10 | 00000000_unified_schema_v7.sql |
| `cas_proxy_granting_tickets` | `CasProxyGrantingTicket` | 8 | 8 | 00000000_unified_schema_v7.sql+00000001_extensions.sql |
| `cas_proxy_tickets` | `CasProxyTicket` | 10 | 9 | 00000000_unified_schema_v7.sql+00000001_extensions.sql |
| `cas_services` | `CasRegisteredService` | 12 | 12 | 00000000_unified_schema_v7.sql+00000001_extensions.sql |
| `cas_slo_sessions` | `CasSloSession` | 8 | 7 | 00000000_unified_schema_v7.sql+00000001_extensions.sql |
| `cas_tickets` | `CasTicket` | 10 | 9 | 00000000_unified_schema_v7.sql+00000001_extensions.sql |
| `cas_user_attributes` | `CasUserAttribute` | 6 | 6 | 00000000_unified_schema_v7.sql+00000001_extensions.sql |
| `dehydrated_devices` | `DehydratedDevice` | 9 | 9 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `devices` | `Device` | 11 | 11 | 00000000_unified_schema_v7.sql |
| `e2ee_audit_log` | `KeyAuditEntry` | 11 | 9 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `email_verification_tokens` | `EmailVerificationToken` | 8 | 8 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `event_relations` | `EventRelation` | 10 | 10 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `event_report_history` | `EventReportHistory` | 10 | 10 | 00000000_unified_schema_v7.sql |
| `event_report_stats` | `EventReportStats` | 10 | 9 | 00000000_unified_schema_v7.sql |
| `event_reports` | `EventReport` | 14 | 9 | 00000000_unified_schema_v7.sql |
| `event_signatures` | `EventSignature` | 8 | 7 | 00000000_unified_schema_v7.sql |
| `event_to_state_groups` | `EventToStateGroup` | 2 | 2 | 00000000_unified_schema_v7.sql |
| `events` | `RoomEvent` | 24 | 14 | 00000000_unified_schema_v7.sql |
| `events` | `FriendDmLink` | 24 | 3 | 00000000_unified_schema_v7.sql |
| `feature_flag_targets` | `FeatureFlagTargetRecord` | 5 | 5 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `feature_flags` | `FeatureFlagRecord` | 9 | 9 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `federation_access_stats` | `FederationAccessStats` | 12 | 12 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `federation_blacklist` | `FederationBlacklist` | 12 | 10 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql) |
| `federation_blacklist_log` | `FederationBlacklistLog` | 11 | 11 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `federation_blacklist_rule` | `FederationBlacklistRule` | 11 | 11 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `federation_queue` | `FederationQueueEntry` | 10 | 10 | 00000000_unified_schema_v7.sql |
| `federation_signing_keys` | `SigningKey` | 9 | 9 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `filters` | `Filter` | 5 | 5 | 00000000_unified_schema_v7.sql |
| `friend_requests` | `FriendRequestRecord` | 7 | 7 | 00000000_unified_schema_v7.sql+00000001_extensions.sql |
| `key_backups` | `KeyBackup` | 11 | 8 | 00000000_unified_schema_v7.sql |
| `matrixrtc_encryption_keys` | `RTCEncryptionKey` | 9 | 9 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `matrixrtc_memberships` | `RTCMembership` | 15 | 15 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `matrixrtc_sessions` | `RTCSession` | 10 | 10 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `media_callbacks` | `MediaCallback` | 16 | 9 | 00000000_unified_schema_v7.sql+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql) |
| `media_quota_alerts` | `MediaQuotaAlert` | 9 | 9 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `media_quota_alerts` | `ServerMediaQuota` | 9 | 8 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `media_quota_config` | `MediaQuotaConfig` | 17 | 12 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql) |
| `media_usage_log` | `MediaUsageLog` | 7 | 7 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `moderation_logs` | `ModerationLog` | 9 | 9 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `moderation_rules` | `ModerationRule` | 12 | 12 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `module_execution_logs` | `ModuleExecutionLog` | 16 | 10 | 00000000_unified_schema_v7.sql+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql) |
| `modules` | `Module` | 14 | 14 | 00000000_unified_schema_v7.sql+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql)+ALTER(20260529000001_module_schema_alignment.sql) |
| `notification_delivery_log` | `NotificationDeliveryLog` | 7 | 7 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `notification_templates` | `NotificationTemplate` | 9 | 9 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `notifications` | `RoomNotification` | 10 | 5 | 00000000_unified_schema_v7.sql |
| `openclaw_connections` | `OpenClawConnection` | 11 | 11 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `openid_tokens` | `OpenIdToken` | 7 | 7 | 00000000_unified_schema_v7.sql |
| `password_auth_providers` | `PasswordAuthProvider` | 8 | 8 | 00000000_unified_schema_v7.sql |
| `push_config` | `PushNotificationLog` | 9 | 13 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql) |
| `push_device` | `PushDevice` | 18 | 18 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `push_notification_queue` | `PushNotificationQueue` | 17 | 15 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql) |
| `push_rules` | `PushRule` | 14 | 14 | 00000000_unified_schema_v7.sql |
| `refresh_token_rotations` | `RefreshTokenRotation` | 6 | 6 | 00000000_unified_schema_v7.sql |
| `refresh_token_usage` | `RefreshTokenUsage` | 10 | 10 | 00000000_unified_schema_v7.sql |
| `refresh_tokens` | `RefreshToken` | 15 | 15 | 00000000_unified_schema_v7.sql |
| `registration_captcha` | `RegistrationCaptcha` | 15 | 15 | 00000000_unified_schema_v7.sql |
| `registration_token_batches` | `RegistrationTokenBatch` | 11 | 11 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `registration_token_usage` | `RegistrationTokenUsage` | 11 | 11 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql) |
| `registration_tokens` | `RegistrationToken` | 18 | 18 | 00000000_unified_schema_v7.sql |
| `rendezvous_session` | `RendezvousSession` | 13 | 11 | 00000000_unified_schema_v7.sql |
| `report_rate_limits` | `ReportRateLimit` | 9 | 9 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `retention_cleanup_logs` | `RetentionCleanupLog` | 10 | 10 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `retention_stats` | `RetentionStats` | 7 | 7 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `room_invites` | `RoomInvite` | 17 | 13 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql) |
| `room_memberships` | `RoomMember` | 20 | 18 | 00000000_unified_schema_v7.sql |
| `room_memberships` | `UserRoomMembership` | 20 | 2 | 00000000_unified_schema_v7.sql |
| `room_retention_policies` | `RoomRetentionPolicy` | 8 | 8 | 00000000_unified_schema_v7.sql |
| `room_summaries` | `RoomSummary` | 24 | 24 | 00000000_unified_schema_v7.sql |
| `room_summary_members` | `RoomSummaryMember` | 10 | 10 | 00000000_unified_schema_v7.sql |
| `room_summary_state` | `RoomSummaryState` | 7 | 7 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `room_summary_stats` | `RoomSummaryStats` | 8 | 8 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `room_tags` | `RoomTag` | 6 | 6 | 00000000_unified_schema_v7.sql |
| `saml_auth_events` | `SamlAuthEvent` | 13 | 13 | 00000000_unified_schema_v7.sql+00000001_extensions.sql |
| `saml_identity_providers` | `SamlIdentityProvider` | 13 | 13 | 00000000_unified_schema_v7.sql+00000001_extensions.sql |
| `saml_logout_requests` | `SamlLogoutRequest` | 10 | 10 | 00000000_unified_schema_v7.sql+00000001_extensions.sql |
| `saml_sessions` | `SamlSession` | 11 | 11 | 00000000_unified_schema_v7.sql+00000001_extensions.sql |
| `saml_user_mapping` | `SamlUserMapping` | 8 | 8 | 00000000_unified_schema_v7.sql+00000001_extensions.sql |
| `scheduled_notifications` | `ScheduledNotification` | 6 | 6 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `server_notifications` | `ServerNotification` | 16 | 16 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `server_retention_policy` | `ServerRetentionPolicy` | 6 | 6 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `sliding_sync_lists` | `SlidingSyncList` | 11 | 11 | 00000000_unified_schema_v7.sql |
| `sliding_sync_rooms` | `SlidingSyncRoom` | 18 | 18 | 00000000_unified_schema_v7.sql |
| `sliding_sync_rooms` | `AdminRoomTokenSyncEntry` | 18 | 19 | 00000000_unified_schema_v7.sql |
| `sliding_sync_tokens` | `SlidingSyncToken` | 8 | 8 | 00000000_unified_schema_v7.sql |
| `space_children` | `SpaceChild` | 7 | 11 | 00000000_unified_schema_v7.sql |
| `space_events` | `SpaceEvent` | 8 | 8 | 00000000_unified_schema_v7.sql |
| `space_members` | `SpaceMember` | 8 | 7 | 00000000_unified_schema_v7.sql |
| `space_summaries` | `SpaceSummary` | 6 | 6 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `spaces` | `Space` | 17 | 13 | 00000000_unified_schema_v7.sql |
| `spam_check_results` | `SpamCheckResult` | 16 | 12 | 00000000_unified_schema_v7.sql+20260529000002_module_result_persistence.sql+ALTER(20260529000002_module_result_persistence.sql)+ALTER(20260529000002_module_result_persistence.sql) |
| `state_group_edges` | `StateGroupEdge` | 2 | 2 | 00000000_unified_schema_v7.sql |
| `state_group_state` | `StateGroupState` | 4 | 4 | 00000000_unified_schema_v7.sql |
| `state_groups` | `StateGroup` | 5 | 5 | 00000000_unified_schema_v7.sql |
| `third_party_rule_results` | `ThirdPartyRuleResult` | 14 | 10 | 00000000_unified_schema_v7.sql+20260529000002_module_result_persistence.sql+ALTER(20260529000002_module_result_persistence.sql) |
| `thread_read_receipts` | `ThreadReadReceipt` | 8 | 8 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `thread_relations` | `ThreadRelation` | 8 | 8 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `thread_roots` | `ThreadRoot` | 13 | 13 | 00000000_unified_schema_v7.sql |
| `thread_roots` | `ThreadSummary` | 13 | 16 | 00000000_unified_schema_v7.sql |
| `thread_subscriptions` | `ThreadSubscription` | 9 | 9 | 00000000_unified_schema_v7.sql |
| `threepid_validation_session` | `ThreepidValidationSession` | 12 | 12 | 00000000_unified_schema_v7.sql |
| `upload_progress` | `UploadProgress` | 12 | 12 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `user_media_quota` | `UserMediaQuota` | 13 | 10 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql) |
| `user_notification_status` | `UserNotificationStatus` | 8 | 8 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `user_privacy_settings` | `UserPrivacySettings` | 12 | 9 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+00000001_extensions.sql |
| `user_threepids` | `UserThreepid` | 9 | 9 | 00000000_unified_schema_v7.sql |
| `users` | `User` | 25 | 25 | 00000000_unified_schema_v7.sql |
| `users` | `UserProfile` | 25 | 5 | 00000000_unified_schema_v7.sql |
| `users` | `UserSearchResult` | 25 | 5 | 00000000_unified_schema_v7.sql |
| `users` | `UserSearchResultWithPresence` | 25 | 7 | 00000000_unified_schema_v7.sql |
| `users` | `UserDirectorySearchResult` | 25 | 9 | 00000000_unified_schema_v7.sql |
| `voice_usage_stats` | `VoiceUsageRecord` | 18 | 8 | 00000000_unified_schema_v7.sql+00000001_extensions.sql+20260517000001_voice_usage_stats.sql |
| `voice_usage_stats` | `VoiceAggregatedStats` | 18 | 3 | 00000000_unified_schema_v7.sql+00000001_extensions.sql+20260517000001_voice_usage_stats.sql |
| `voice_usage_stats` | `VoiceUserAggregatedStats` | 18 | 4 | 00000000_unified_schema_v7.sql+00000001_extensions.sql+20260517000001_voice_usage_stats.sql |
| `widget_permissions` | `WidgetPermission` | 6 | 6 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `widget_sessions` | `WidgetSession` | 9 | 9 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `widgets` | `Widget` | 11 | 11 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `worker_commands` | `WorkerCommandRow` | 14 | 14 | 00000000_unified_schema_v7.sql |
| `worker_events` | `WorkerEventRow` | 9 | 9 | 00000000_unified_schema_v7.sql |
| `worker_task_assignments` | `WorkerTaskAssignment` | 12 | 12 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
| `workers` | `WorkerRow` | 14 | 13 | 00000000_unified_schema_v7.sql |

## CRITICAL Issues (16)

| # | Type | Table / Struct | Field / Column | Detail |
|---|------|----------------|-----------------|--------|
| 1 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `users` | `match_score` | Rust field 'match_score: i32' maps to SQL column 'INTEGER' which does not exist in table 'users' |
| 2 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `users` | `match_type` | Rust field 'match_type: String' maps to SQL column 'END' which does not exist in table 'users' |
| 3 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `push_config` | `push_type` | Rust field 'push_type: String' maps to SQL column 'push_type' which does not exist in table 'push_config' |
| 4 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `push_config` | `sent_at` | Rust field 'sent_at: DateTime<Utc>' maps to SQL column 'sent_at' which does not exist in table 'push_config' |
| 5 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `push_config` | `is_success` | Rust field 'is_success: bool' maps to SQL column 'success' which does not exist in table 'push_config' |
| 6 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `push_config` | `metadata` | Rust field 'metadata: serde_json::Value' maps to SQL column 'metadata' which does not exist in table 'push_config' |
| 7 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `burn_after_read_log` | `total_burned` | Rust field 'total_burned: i64' maps to SQL column 'total_burned' which does not exist in table 'burn_after_read_log' |
| 8 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `burn_after_read_log` | `total_pending` | Rust field 'total_pending: i64' maps to SQL column 'total_pending' which does not exist in table 'burn_after_read_log' |
| 9 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `burn_after_read_log` | `rooms_enabled` | Rust field 'rooms_enabled: i64' maps to SQL column 'rooms_enabled' which does not exist in table 'burn_after_read_log' |
| 10 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_quota_alerts` | `current_storage_bytes` | Rust field 'current_storage_bytes: i64' maps to SQL column 'current_storage_bytes' which does not exist in table 'media_quota_alerts' |
| 11 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_quota_alerts` | `current_files_count` | Rust field 'current_files_count: i32' maps to SQL column 'current_files_count' which does not exist in table 'media_quota_alerts' |
| 12 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_quota_alerts` | `alert_threshold_percent` | Rust field 'alert_threshold_percent: i32' maps to SQL column 'alert_threshold_percent' which does not exist in table 'media_quota_alerts' |
| 13 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_quota_alerts` | `updated_ts` | Rust field 'updated_ts: i64' maps to SQL column 'updated_ts' which does not exist in table 'media_quota_alerts' |
| 14 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `voice_usage_stats` | `total_uploads` | Rust field 'total_uploads: i64' maps to SQL column 'total_uploads' which does not exist in table 'voice_usage_stats' |
| 15 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `voice_usage_stats` | `total_uploads` | Rust field 'total_uploads: i64' maps to SQL column 'total_uploads' which does not exist in table 'voice_usage_stats' |
| 16 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `voice_usage_stats` | `uploads_today` | Rust field 'uploads_today: i64' maps to SQL column 'uploads_today' which does not exist in table 'voice_usage_stats' |

## HIGH Issues (345)

| # | Type | Table / Struct | Field / Column | Detail |
|---|------|----------------|-----------------|--------|
| 1 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `matrixrtc_sessions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 2 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `matrixrtc_sessions` | `is_active` | SQL column 'is_active' is nullable but Rust field 'is_active: bool' is not Option |
| 3 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `matrixrtc_memberships` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 4 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `matrixrtc_memberships` | `is_active` | SQL column 'is_active' is nullable but Rust field 'is_active: bool' is not Option |
| 5 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `matrixrtc_encryption_keys` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 6 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `refresh_tokens` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 7 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `refresh_tokens` | `use_count` | SQL column 'use_count' is nullable but Rust field 'use_count: i32' is not Option |
| 8 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `refresh_tokens` | `is_revoked` | SQL column 'is_revoked' is nullable but Rust field 'is_revoked: bool' is not Option |
| 9 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `refresh_token_usage` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 10 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `refresh_token_usage` | `success` | SQL column 'is_success' is nullable but Rust field 'success: bool' is not Option |
| 11 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `refresh_token_rotations` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 12 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_blacklist_log` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 13 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_blacklist_log` | `metadata` | SQL column 'metadata' is nullable but Rust field 'metadata: serde_json::Value' is not Option |
| 14 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_access_stats` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 15 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_access_stats` | `total_requests` | SQL column 'total_requests' is nullable but Rust field 'total_requests: i64' is not Option |
| 16 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_access_stats` | `successful_requests` | SQL column 'successful_requests' is nullable but Rust field 'successful_requests: i64' is not Option |
| 17 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_access_stats` | `failed_requests` | SQL column 'failed_requests' is nullable but Rust field 'failed_requests: i64' is not Option |
| 18 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_access_stats` | `average_response_time_ms` | SQL column 'average_response_time_ms' is nullable but Rust field 'average_response_time_ms: f64' is not Option |
| 19 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_access_stats` | `error_rate` | SQL column 'error_rate' is nullable but Rust field 'error_rate: f64' is not Option |
| 20 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_blacklist_rule` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 21 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_blacklist_rule` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 22 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `federation_blacklist_rule` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 23 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_tokens` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 24 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_tokens` | `token_type` | SQL column 'token_type' is nullable but Rust field 'token_type: String' is not Option |
| 25 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_tokens` | `max_uses` | SQL column 'max_uses' is nullable but Rust field 'max_uses: i32' is not Option |
| 26 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_tokens` | `uses_count` | SQL column 'uses_count' is nullable but Rust field 'uses_count: i32' is not Option |
| 27 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_tokens` | `is_used` | SQL column 'is_used' is nullable but Rust field 'is_used: bool' is not Option |
| 28 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_tokens` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 29 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_tokens` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 30 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_token_usage` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 31 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_token_usage` | `token_id` | SQL column 'token_id' is nullable but Rust field 'token_id: i64' is not Option |
| 32 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_token_usage` | `token` | SQL column 'token' is nullable but Rust field 'token: String' is not Option |
| 33 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_token_usage` | `is_success` | SQL column 'success' is nullable but Rust field 'is_success: bool' is not Option |
| 34 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `room_invites` | `invitee` | SQL column 'invitee' (TEXT NOT NULL) has no matching Rust field in RoomInvite |
| 35 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `room_invites` | `inviter` | SQL column 'inviter' (TEXT NOT NULL) has no matching Rust field in RoomInvite |
| 36 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_invites` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 37 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_invites` | `invite_code` | SQL column 'invite_code' is nullable but Rust field 'invite_code: String' is not Option |
| 38 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_invites` | `inviter_user_id` | SQL column 'inviter_user_id' is nullable but Rust field 'inviter_user_id: String' is not Option |
| 39 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_invites` | `is_used` | SQL column 'is_used' is nullable but Rust field 'is_used: bool' is not Option |
| 40 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_invites` | `is_revoked` | SQL column 'is_revoked' is nullable but Rust field 'is_revoked: bool' is not Option |
| 41 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_token_batches` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 42 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_token_batches` | `tokens_used` | SQL column 'tokens_used' is nullable but Rust field 'tokens_used: i32' is not Option |
| 43 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_token_batches` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 44 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_sessions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 45 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_sessions` | `attributes` | SQL column 'attributes' is nullable but Rust field 'attributes: serde_json::Value' is not Option |
| 46 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_sessions` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 47 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_user_mapping` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 48 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_user_mapping` | `authentication_count` | SQL column 'authentication_count' is nullable but Rust field 'authentication_count: i32' is not Option |
| 49 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_user_mapping` | `attributes` | SQL column 'attributes' is nullable but Rust field 'attributes: serde_json::Value' is not Option |
| 50 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_identity_providers` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 51 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_identity_providers` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 52 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_identity_providers` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 53 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_identity_providers` | `attribute_mapping` | SQL column 'attribute_mapping' is nullable but Rust field 'attribute_mapping: serde_json::Value' is not Option |
| 54 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_auth_events` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 55 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_auth_events` | `attributes` | SQL column 'attributes' is nullable but Rust field 'attributes: serde_json::Value' is not Option |
| 56 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_logout_requests` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 57 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `saml_logout_requests` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 58 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `moderation_rules` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 59 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `moderation_rules` | `is_active` | SQL column 'is_active' is nullable but Rust field 'is_active: bool' is not Option |
| 60 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `moderation_rules` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 61 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `moderation_logs` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 62 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `widgets` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 63 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `widgets` | `data` | SQL column 'data' is nullable but Rust field 'data: serde_json::Value' is not Option |
| 64 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `widgets` | `is_active` | SQL column 'is_active' is nullable but Rust field 'is_active: bool' is not Option |
| 65 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `widget_permissions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 66 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `widget_permissions` | `permissions` | SQL column 'permissions' is nullable but Rust field 'permissions: serde_json::Value' is not Option |
| 67 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `widget_sessions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 68 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `widget_sessions` | `is_active` | SQL column 'is_active' is nullable but Rust field 'is_active: bool' is not Option |
| 69 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_subscriptions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 70 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_subscriptions` | `notification_level` | SQL column 'notification_level' is nullable but Rust field 'notification_level: String' is not Option |
| 71 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_subscriptions` | `is_muted` | SQL column 'is_muted' is nullable but Rust field 'is_muted: bool' is not Option |
| 72 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_subscriptions` | `is_pinned` | SQL column 'is_pinned' is nullable but Rust field 'is_pinned: bool' is not Option |
| 73 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_relations` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 74 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `thread_relations` | `is_falling_back` | SQL column 'is_falling_back' is nullable but Rust field 'is_falling_back: bool' is not Option |
| 75 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_reports` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 76 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_reports` | `score` | SQL column 'score' is nullable but Rust field 'score: i32' is not Option |
| 77 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_report_history` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 78 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `report_rate_limits` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 79 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `report_rate_limits` | `report_count` | SQL column 'report_count' is nullable but Rust field 'report_count: i32' is not Option |
| 80 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `report_rate_limits` | `is_blocked` | SQL column 'is_blocked' is nullable but Rust field 'is_blocked: bool' is not Option |
| 81 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `report_rate_limits` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 82 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_report_stats` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 83 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_report_stats` | `total_reports` | SQL column 'total_reports' is nullable but Rust field 'total_reports: i32' is not Option |
| 84 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_report_stats` | `open_reports` | SQL column 'open_reports' is nullable but Rust field 'open_reports: i32' is not Option |
| 85 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_report_stats` | `resolved_reports` | SQL column 'resolved_reports' is nullable but Rust field 'resolved_reports: i32' is not Option |
| 86 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_report_stats` | `dismissed_reports` | SQL column 'dismissed_reports' is nullable but Rust field 'dismissed_reports: i32' is not Option |
| 87 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `event_report_stats` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 88 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_state` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 89 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_state` | `content` | SQL column 'content' is nullable but Rust field 'content: serde_json::Value' is not Option |
| 90 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_stats` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 91 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_stats` | `total_events` | SQL column 'total_events' is nullable but Rust field 'total_events: i64' is not Option |
| 92 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_stats` | `total_state_events` | SQL column 'total_state_events' is nullable but Rust field 'total_state_events: i64' is not Option |
| 93 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_stats` | `total_messages` | SQL column 'total_messages' is nullable but Rust field 'total_messages: i64' is not Option |
| 94 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_stats` | `total_media` | SQL column 'total_media' is nullable but Rust field 'total_media: i64' is not Option |
| 95 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `room_summary_stats` | `storage_size` | SQL column 'storage_size' is nullable but Rust field 'storage_size: i64' is not Option |
| 96 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `users` | `displayname` | Rust field 'displayname: Option<String>' maps to SQL column 'u' which does not exist in table 'users' |
| 97 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `users` | `displayname` | Rust field 'displayname: Option<String>' maps to SQL column 'u' which does not exist in table 'users' |
| 98 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `users` | `presence` | Rust field 'presence: Option<String>' maps to SQL column 'presence' which does not exist in table 'users' |
| 99 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `users` | `last_active_ts` | Rust field 'last_active_ts: Option<i64>' maps to SQL column 'last_active_ts' which does not exist in table 'users' |
| 100 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `users` | `displayname` | Rust field 'displayname: Option<String>' maps to SQL column 'u' which does not exist in table 'users' |
| 101 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `users` | `presence` | Rust field 'presence: Option<String>' maps to SQL column 'presence' which does not exist in table 'users' |
| 102 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `users` | `last_active_ts` | Rust field 'last_active_ts: Option<i64>' maps to SQL column 'last_active_ts' which does not exist in table 'users' |
| 103 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `rendezvous_session` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 104 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `rendezvous_session` | `intent` | SQL column 'intent' is nullable but Rust field 'intent: String' is not Option |
| 105 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `rendezvous_session` | `transport` | SQL column 'transport' is nullable but Rust field 'transport: String' is not Option |
| 106 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `rendezvous_session` | `key` | SQL column 'key' is nullable but Rust field 'key: String' is not Option |
| 107 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `rendezvous_session` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 108 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `threepid_validation_session` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 109 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `threepid_validation_session` | `send_attempt` | SQL column 'send_attempt' is nullable but Rust field 'send_attempt: i32' is not Option |
| 110 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `threepid_validation_session` | `is_validated` | SQL column 'is_validated' is nullable but Rust field 'is_validated: bool' is not Option |
| 111 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_privacy_settings` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 112 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_privacy_settings` | `profile_visibility` | SQL column 'profile_visibility' is nullable but Rust field 'profile_visibility: String' is not Option |
| 113 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_privacy_settings` | `avatar_visibility` | SQL column 'avatar_visibility' is nullable but Rust field 'avatar_visibility: String' is not Option |
| 114 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_privacy_settings` | `displayname_visibility` | SQL column 'displayname_visibility' is nullable but Rust field 'displayname_visibility: String' is not Option |
| 115 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_privacy_settings` | `presence_visibility` | SQL column 'presence_visibility' is nullable but Rust field 'presence_visibility: String' is not Option |
| 116 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_privacy_settings` | `room_membership_visibility` | SQL column 'room_membership_visibility' is nullable but Rust field 'room_membership_visibility: String' is not Option |
| 117 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `openclaw_connections` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 118 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `openclaw_connections` | `is_default` | SQL column 'is_default' is nullable but Rust field 'is_default: bool' is not Option |
| 119 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `openclaw_connections` | `is_active` | SQL column 'is_active' is nullable but Rust field 'is_active: bool' is not Option |
| 120 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_conversations` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 121 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_conversations` | `is_pinned` | SQL column 'is_pinned' is nullable but Rust field 'is_pinned: bool' is not Option |
| 122 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_messages` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 123 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `ai_generations` | `type` | SQL column 'type' (TEXT NOT NULL CHECK (type IN ('image', 'video', 'audio'))) has no matching Rust field in AiGeneration |
| 124 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_generations` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 125 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_generations` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 126 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_chat_roles` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 127 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `ai_chat_roles` | `is_public` | SQL column 'is_public' is nullable but Rust field 'is_public: bool' is not Option |
| 128 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `space_members` | `membership` | SQL column 'membership' is nullable but Rust field 'membership: String' is not Option |
| 129 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `space_summaries` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 130 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `space_summaries` | `summary` | SQL column 'summary' is nullable but Rust field 'summary: serde_json::Value' is not Option |
| 131 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `space_summaries` | `children_count` | SQL column 'children_count' is nullable but Rust field 'children_count: i64' is not Option |
| 132 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `space_summaries` | `member_count` | SQL column 'member_count' is nullable but Rust field 'member_count: i64' is not Option |
| 133 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `modules` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 134 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `modules` | `version` | SQL column 'version' is nullable but Rust field 'version: String' is not Option |
| 135 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `modules` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 136 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `modules` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 137 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `modules` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 138 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `modules` | `execution_count` | SQL column 'execution_count' is nullable but Rust field 'execution_count: i32' is not Option |
| 139 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `modules` | `error_count` | SQL column 'error_count' is nullable but Rust field 'error_count: i32' is not Option |
| 140 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `third_party_rule_results` | `created_ts` | SQL column 'created_ts' (BIGINT NOT NULL) has no matching Rust field in ThirdPartyRuleResult |
| 141 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `third_party_rule_results` | `rule_type` | SQL column 'rule_type' (TEXT NOT NULL) has no matching Rust field in ThirdPartyRuleResult |
| 142 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `third_party_rule_results` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 143 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `third_party_rule_results` | `event_id` | SQL column 'event_id' is nullable but Rust field 'event_id: String' is not Option |
| 144 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `third_party_rule_results` | `room_id` | SQL column 'room_id' is nullable but Rust field 'room_id: String' is not Option |
| 145 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `third_party_rule_results` | `is_allowed` | SQL column 'is_allowed' is nullable but Rust field 'is_allowed: bool' is not Option |
| 146 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `third_party_rule_results` | `checked_ts` | SQL column 'checked_ts' is nullable but Rust field 'checked_ts: i64' is not Option |
| 147 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `module_execution_logs` | `created_ts` | SQL column 'created_ts' (BIGINT NOT NULL) has no matching Rust field in ModuleExecutionLog |
| 148 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `module_execution_logs` | `execution_type` | SQL column 'execution_type' (TEXT NOT NULL) has no matching Rust field in ModuleExecutionLog |
| 149 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `module_execution_logs` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 150 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `module_execution_logs` | `module_name` | SQL column 'module_name' is nullable but Rust field 'module_name: String' is not Option |
| 151 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `module_execution_logs` | `module_type` | SQL column 'module_type' is nullable but Rust field 'module_type: String' is not Option |
| 152 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `module_execution_logs` | `execution_time_ms` | SQL column 'execution_time_ms' is nullable but Rust field 'execution_time_ms: i64' is not Option |
| 153 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `module_execution_logs` | `is_success` | SQL column 'success' is nullable but Rust field 'is_success: bool' is not Option |
| 154 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `module_execution_logs` | `executed_ts` | SQL column 'executed_ts' is nullable but Rust field 'executed_ts: i64' is not Option |
| 155 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `password_auth_providers` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 156 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `password_auth_providers` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 157 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `password_auth_providers` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 158 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `password_auth_providers` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 159 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `media_callbacks` | `callback_name` | SQL column 'callback_name' (TEXT NOT NULL) has no matching Rust field in MediaCallback |
| 160 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `media_callbacks` | `url` | SQL column 'url' (TEXT NOT NULL) has no matching Rust field in MediaCallback |
| 161 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_callbacks` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 162 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_callbacks` | `media_id` | SQL column 'media_id' is nullable but Rust field 'media_id: String' is not Option |
| 163 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_callbacks` | `user_id` | SQL column 'user_id' is nullable but Rust field 'user_id: String' is not Option |
| 164 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_callbacks` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 165 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_callbacks` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 166 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `notification_delivery_log` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 167 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `notification_delivery_log` | `delivered_ts` | SQL column 'delivered_ts' is nullable but Rust field 'delivered_ts: i64' is not Option |
| 168 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `beacon_info` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 169 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `beacon_info` | `is_live` | SQL column 'is_live' is nullable but Rust field 'is_live: bool' is not Option |
| 170 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `beacon_locations` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 171 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_device` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 172 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_device` | `enabled` | SQL column 'is_enabled' is nullable but Rust field 'enabled: bool' is not Option |
| 173 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_device` | `error_count` | SQL column 'error_count' is nullable but Rust field 'error_count: i32' is not Option |
| 174 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_device` | `metadata` | SQL column 'metadata' is nullable but Rust field 'metadata: serde_json::Value' is not Option |
| 175 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_rules` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 176 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_rules` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 177 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_rules` | `conditions` | SQL column 'conditions' is nullable but Rust field 'conditions: serde_json::Value' is not Option |
| 178 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_rules` | `actions` | SQL column 'actions' is nullable but Rust field 'actions: serde_json::Value' is not Option |
| 179 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_rules` | `enabled` | SQL column 'is_enabled' is nullable but Rust field 'enabled: bool' is not Option |
| 180 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_rules` | `is_default` | SQL column 'is_default' is nullable but Rust field 'is_default: bool' is not Option |
| 181 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_queue` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 182 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_queue` | `content` | SQL column 'content' is nullable but Rust field 'content: serde_json::Value' is not Option |
| 183 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_queue` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 184 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_queue` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 185 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_queue` | `attempts` | SQL column 'attempts' is nullable but Rust field 'attempts: i32' is not Option |
| 186 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_queue` | `max_attempts` | SQL column 'max_attempts' is nullable but Rust field 'max_attempts: i32' is not Option |
| 187 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `push_notification_queue` | `next_attempt_at` | SQL column 'next_attempt_at' is nullable but Rust field 'next_attempt_at: chrono::DateTime<Utc>' is not Option |
| 188 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `push_config` | `event_id` | Rust field 'event_id: Option<String>' maps to SQL column 'event_id' which does not exist in table 'push_config' |
| 189 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `push_config` | `room_id` | Rust field 'room_id: Option<String>' maps to SQL column 'room_id' which does not exist in table 'push_config' |
| 190 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `push_config` | `notification_type` | Rust field 'notification_type: Option<String>' maps to SQL column 'notification_type' which does not exist in table 'push_config' |
| 191 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `push_config` | `error_message` | Rust field 'error_message: Option<String>' maps to SQL column 'error_message' which does not exist in table 'push_config' |
| 192 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `push_config` | `provider_response` | Rust field 'provider_response: Option<String>' maps to SQL column 'provider_response' which does not exist in table 'push_config' |
| 193 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `push_config` | `response_time_ms` | Rust field 'response_time_ms: Option<i32>' maps to SQL column 'response_time_ms' which does not exist in table 'push_config' |
| 194 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `burn_after_read_log` | `burned_ts` | SQL column 'burned_ts' (BIGINT NOT NULL) has no matching Rust field in BurnStatsRow |
| 195 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `burn_after_read_log` | `event_id` | SQL column 'event_id' (TEXT NOT NULL) has no matching Rust field in BurnStatsRow |
| 196 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `burn_after_read_log` | `room_id` | SQL column 'room_id' (TEXT NOT NULL) has no matching Rust field in BurnStatsRow |
| 197 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `burn_after_read_log` | `user_id` | SQL column 'user_id' (TEXT NOT NULL) has no matching Rust field in BurnStatsRow |
| 198 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_cleanup_logs` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 199 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_cleanup_logs` | `events_deleted` | SQL column 'events_deleted' is nullable but Rust field 'events_deleted: i64' is not Option |
| 200 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_cleanup_logs` | `state_events_deleted` | SQL column 'state_events_deleted' is nullable but Rust field 'state_events_deleted: i64' is not Option |
| 201 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_cleanup_logs` | `media_deleted` | SQL column 'media_deleted' is nullable but Rust field 'media_deleted: i64' is not Option |
| 202 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_cleanup_logs` | `bytes_freed` | SQL column 'bytes_freed' is nullable but Rust field 'bytes_freed: i64' is not Option |
| 203 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_stats` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 204 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_stats` | `total_events` | SQL column 'total_events' is nullable but Rust field 'total_events: i64' is not Option |
| 205 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_stats` | `events_in_retention` | SQL column 'events_in_retention' is nullable but Rust field 'events_in_retention: i64' is not Option |
| 206 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `retention_stats` | `events_expired` | SQL column 'events_expired' is nullable but Rust field 'events_expired: i64' is not Option |
| 207 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_services` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 208 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_services` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 209 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_services` | `rate_limited` | SQL column 'rate_limited' is nullable but Rust field 'rate_limited: bool' is not Option |
| 210 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_services` | `protocols` | SQL column 'protocols' is nullable but Rust field 'protocols: Vec<String>' is not Option |
| 211 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_services` | `namespaces` | SQL column 'namespaces' is nullable but Rust field 'namespaces: serde_json::Value' is not Option |
| 212 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_services` | `config` | SQL column 'config' is nullable but Rust field 'config: serde_json::Value' is not Option |
| 213 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `application_service_state` | `value` | SQL column 'value' (JSONB NOT NULL) has no matching Rust field in ApplicationServiceState |
| 214 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_state` | `state_value` | SQL column 'state_value' is nullable but Rust field 'state_value: String' is not Option |
| 215 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `application_service_transactions` | `created_ts` | SQL column 'created_ts' (BIGINT NOT NULL) has no matching Rust field in ApplicationServiceTransaction |
| 216 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `application_service_transactions` | `txn_id` | SQL column 'txn_id' (TEXT NOT NULL) has no matching Rust field in ApplicationServiceTransaction |
| 217 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_transactions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 218 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_transactions` | `transaction_id` | SQL column 'transaction_id' is nullable but Rust field 'transaction_id: String' is not Option |
| 219 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_transactions` | `events` | SQL column 'events' is nullable but Rust field 'events: serde_json::Value' is not Option |
| 220 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_transactions` | `sent_ts` | SQL column 'sent_ts' is nullable but Rust field 'sent_ts: i64' is not Option |
| 221 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `application_service_transactions` | `retry_count` | SQL column 'retry_count' is nullable but Rust field 'retry_count: i32' is not Option |
| 222 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_tickets` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 223 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_tickets` | `is_valid` | SQL column 'is_valid' is nullable but Rust field 'is_valid: bool' is not Option |
| 224 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_proxy_tickets` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 225 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_proxy_tickets` | `is_valid` | SQL column 'is_valid' is nullable but Rust field 'is_valid: bool' is not Option |
| 226 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_proxy_granting_tickets` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 227 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_proxy_granting_tickets` | `is_valid` | SQL column 'is_valid' is nullable but Rust field 'is_valid: bool' is not Option |
| 228 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_services` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 229 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_services` | `allowed_attributes` | SQL column 'allowed_attributes' is nullable but Rust field 'allowed_attributes: serde_json::Value' is not Option |
| 230 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_services` | `allowed_proxy_callbacks` | SQL column 'allowed_proxy_callbacks' is nullable but Rust field 'allowed_proxy_callbacks: serde_json::Value' is not Option |
| 231 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_services` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 232 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_services` | `require_secure` | SQL column 'require_secure' is nullable but Rust field 'require_secure: bool' is not Option |
| 233 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_services` | `single_logout` | SQL column 'single_logout' is nullable but Rust field 'single_logout: bool' is not Option |
| 234 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_slo_sessions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 235 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `cas_user_attributes` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 236 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `media_quota_config` | `config_name` | SQL column 'config_name' (TEXT NOT NULL) has no matching Rust field in MediaQuotaConfig |
| 237 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_config` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 238 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_config` | `name` | SQL column 'name' is nullable but Rust field 'name: String' is not Option |
| 239 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_config` | `max_storage_bytes` | SQL column 'max_storage_bytes' is nullable but Rust field 'max_storage_bytes: i64' is not Option |
| 240 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_config` | `max_file_size_bytes` | SQL column 'max_file_size_bytes' is nullable but Rust field 'max_file_size_bytes: i64' is not Option |
| 241 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_config` | `max_files_count` | SQL column 'max_files_count' is nullable but Rust field 'max_files_count: i32' is not Option |
| 242 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_config` | `allowed_mime_types` | SQL column 'allowed_mime_types' is nullable but Rust field 'allowed_mime_types: serde_json::Value' is not Option |
| 243 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_config` | `blocked_mime_types` | SQL column 'blocked_mime_types' is nullable but Rust field 'blocked_mime_types: serde_json::Value' is not Option |
| 244 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_config` | `is_default` | SQL column 'is_default' is nullable but Rust field 'is_default: bool' is not Option |
| 245 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_config` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 246 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_media_quota` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 247 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_media_quota` | `current_storage_bytes` | SQL column 'current_storage_bytes' is nullable but Rust field 'current_storage_bytes: i64' is not Option |
| 248 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `user_media_quota` | `current_files_count` | SQL column 'current_files_count' is nullable but Rust field 'current_files_count: i32' is not Option |
| 249 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_usage_log` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 250 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_alerts` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 251 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_alerts` | `is_read` | SQL column 'is_read' is nullable but Rust field 'is_read: bool' is not Option |
| 252 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_alerts` | `created_ts` | SQL column 'created_ts' is nullable but Rust field 'created_ts: i64' is not Option |
| 253 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `media_quota_alerts` | `alert_type` | SQL column 'alert_type' (TEXT NOT NULL) has no matching Rust field in ServerMediaQuota |
| 254 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `media_quota_alerts` | `current_usage_bytes` | SQL column 'current_usage_bytes' (BIGINT NOT NULL) has no matching Rust field in ServerMediaQuota |
| 255 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `media_quota_alerts` | `quota_limit_bytes` | SQL column 'quota_limit_bytes' (BIGINT NOT NULL) has no matching Rust field in ServerMediaQuota |
| 256 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `media_quota_alerts` | `threshold_percent` | SQL column 'threshold_percent' (INTEGER NOT NULL) has no matching Rust field in ServerMediaQuota |
| 257 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `media_quota_alerts` | `user_id` | SQL column 'user_id' (TEXT NOT NULL) has no matching Rust field in ServerMediaQuota |
| 258 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_quota_alerts` | `max_storage_bytes` | Rust field 'max_storage_bytes: Option<i64>' maps to SQL column 'max_storage_bytes' which does not exist in table 'media_quota_alerts' |
| 259 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_quota_alerts` | `max_file_size_bytes` | Rust field 'max_file_size_bytes: Option<i64>' maps to SQL column 'max_file_size_bytes' which does not exist in table 'media_quota_alerts' |
| 260 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `media_quota_alerts` | `max_files_count` | Rust field 'max_files_count: Option<i32>' maps to SQL column 'max_files_count' which does not exist in table 'media_quota_alerts' |
| 261 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `media_quota_alerts` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 262 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `background_updates` | `update_name` | SQL column 'update_name' (TEXT NOT NULL) has no matching Rust field in BackgroundUpdate |
| 263 | `RUST_FIELD_WITHOUT_SQL_COLUMN` | `background_updates` | `last_updated_ts` | Rust field 'last_updated_ts: Option<i64>' maps to SQL column 'last_updated_ts' which does not exist in table 'background_updates' |
| 264 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `job_name` | SQL column 'job_name' is nullable but Rust field 'job_name: String' is not Option |
| 265 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `job_type` | SQL column 'job_type' is nullable but Rust field 'job_type: String' is not Option |
| 266 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 267 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `progress` | SQL column 'progress' is nullable but Rust field 'progress: serde_json::Value' is not Option |
| 268 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `total_items` | SQL column 'total_items' is nullable but Rust field 'total_items: i32' is not Option |
| 269 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `processed_items` | SQL column 'processed_items' is nullable but Rust field 'processed_items: i32' is not Option |
| 270 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `created_ts` | SQL column 'created_ts' is nullable but Rust field 'created_ts: i64' is not Option |
| 271 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `retry_count` | SQL column 'retry_count' is nullable but Rust field 'retry_count: i32' is not Option |
| 272 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `max_retries` | SQL column 'max_retries' is nullable but Rust field 'max_retries: i32' is not Option |
| 273 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `batch_size` | SQL column 'batch_size' is nullable but Rust field 'batch_size: i32' is not Option |
| 274 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_updates` | `sleep_ms` | SQL column 'sleep_ms' is nullable but Rust field 'sleep_ms: i32' is not Option |
| 275 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_update_history` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 276 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_update_history` | `items_processed` | SQL column 'items_processed' is nullable but Rust field 'items_processed: i32' is not Option |
| 277 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `background_update_locks` | `lock_name` | SQL column 'lock_name' is nullable but Rust field 'lock_name: String' is not Option |
| 278 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `voice_usage_stats` | `content_type` | SQL column 'content_type' (TEXT NOT NULL) has no matching Rust field in VoiceAggregatedStats |
| 279 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `voice_usage_stats` | `created_ts` | SQL column 'created_ts' (BIGINT NOT NULL) has no matching Rust field in VoiceAggregatedStats |
| 280 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `voice_usage_stats` | `date` | SQL column 'date' (DATE NOT NULL) has no matching Rust field in VoiceAggregatedStats |
| 281 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `voice_usage_stats` | `media_id` | SQL column 'media_id' (TEXT NOT NULL) has no matching Rust field in VoiceAggregatedStats |
| 282 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `voice_usage_stats` | `user_id` | SQL column 'user_id' (TEXT NOT NULL) has no matching Rust field in VoiceAggregatedStats |
| 283 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `voice_usage_stats` | `total_duration_ms` | SQL column 'total_duration_ms' is nullable but Rust field 'total_duration_ms: i64' is not Option |
| 284 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `voice_usage_stats` | `total_size_bytes` | SQL column 'total_size_bytes' is nullable but Rust field 'total_size_bytes: i64' is not Option |
| 285 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `voice_usage_stats` | `content_type` | SQL column 'content_type' (TEXT NOT NULL) has no matching Rust field in VoiceUserAggregatedStats |
| 286 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `voice_usage_stats` | `created_ts` | SQL column 'created_ts' (BIGINT NOT NULL) has no matching Rust field in VoiceUserAggregatedStats |
| 287 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `voice_usage_stats` | `date` | SQL column 'date' (DATE NOT NULL) has no matching Rust field in VoiceUserAggregatedStats |
| 288 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `voice_usage_stats` | `media_id` | SQL column 'media_id' (TEXT NOT NULL) has no matching Rust field in VoiceUserAggregatedStats |
| 289 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `voice_usage_stats` | `user_id` | SQL column 'user_id' (TEXT NOT NULL) has no matching Rust field in VoiceUserAggregatedStats |
| 290 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `voice_usage_stats` | `total_duration_ms` | SQL column 'total_duration_ms' is nullable but Rust field 'total_duration_ms: i64' is not Option |
| 291 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `voice_usage_stats` | `total_size_bytes` | SQL column 'total_size_bytes' is nullable but Rust field 'total_size_bytes: i64' is not Option |
| 292 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_captcha` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 293 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_captcha` | `attempt_count` | SQL column 'attempt_count' is nullable but Rust field 'attempt_count: i32' is not Option |
| 294 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_captcha` | `max_attempts` | SQL column 'max_attempts' is nullable but Rust field 'max_attempts: i32' is not Option |
| 295 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_captcha` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 296 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `registration_captcha` | `metadata` | SQL column 'metadata' is nullable but Rust field 'metadata: serde_json::Value' is not Option |
| 297 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_send_log` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 298 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_template` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 299 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_template` | `variables` | SQL column 'variables' is nullable but Rust field 'variables: serde_json::Value' is not Option |
| 300 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_template` | `is_default` | SQL column 'is_default' is nullable but Rust field 'is_default: bool' is not Option |
| 301 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_template` | `is_enabled` | SQL column 'is_enabled' is nullable but Rust field 'is_enabled: bool' is not Option |
| 302 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_template` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 303 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_config` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 304 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `captcha_config` | `updated_ts` | SQL column 'updated_ts' is nullable but Rust field 'updated_ts: i64' is not Option |
| 305 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_tokens` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 306 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_lists` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 307 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_lists` | `sort` | SQL column 'sort' is nullable but Rust field 'sort: serde_json::Value' is not Option |
| 308 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 309 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `bump_stamp` | SQL column 'bump_stamp' is nullable but Rust field 'bump_stamp: i64' is not Option |
| 310 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `highlight_count` | SQL column 'highlight_count' is nullable but Rust field 'highlight_count: i32' is not Option |
| 311 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `notification_count` | SQL column 'notification_count' is nullable but Rust field 'notification_count: i32' is not Option |
| 312 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `is_dm` | SQL column 'is_dm' is nullable but Rust field 'is_dm: bool' is not Option |
| 313 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `is_encrypted` | SQL column 'is_encrypted' is nullable but Rust field 'is_encrypted: bool' is not Option |
| 314 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `is_tombstoned` | SQL column 'is_tombstoned' is nullable but Rust field 'is_tombstoned: bool' is not Option |
| 315 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `is_invited` | SQL column 'invited' is nullable but Rust field 'is_invited: bool' is not Option |
| 316 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `sliding_sync_rooms` | `timestamp` | SQL column 'timestamp' is nullable but Rust field 'timestamp: i64' is not Option |
| 317 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `email_verification_tokens` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 318 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `email_verification_tokens` | `used` | SQL column 'used' is nullable but Rust field 'used: bool' is not Option |
| 319 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `call_sessions` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 320 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `call_candidates` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 321 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_task_assignments` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 322 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_task_assignments` | `task_data` | SQL column 'task_data' is nullable but Rust field 'task_data: serde_json::Value' is not Option |
| 323 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_task_assignments` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 324 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_task_assignments` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 325 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `workers` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 326 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `workers` | `host` | SQL column 'host' is nullable but Rust field 'host: String' is not Option |
| 327 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `workers` | `port` | SQL column 'port' is nullable but Rust field 'port: i32' is not Option |
| 328 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `workers` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 329 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `workers` | `config` | SQL column 'config' is nullable but Rust field 'config: serde_json::Value' is not Option |
| 330 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `workers` | `metadata` | SQL column 'metadata' is nullable but Rust field 'metadata: serde_json::Value' is not Option |
| 331 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_commands` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 332 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_commands` | `command_data` | SQL column 'command_data' is nullable but Rust field 'command_data: serde_json::Value' is not Option |
| 333 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_commands` | `priority` | SQL column 'priority' is nullable but Rust field 'priority: i32' is not Option |
| 334 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_commands` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |
| 335 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_commands` | `retry_count` | SQL column 'retry_count' is nullable but Rust field 'retry_count: i32' is not Option |
| 336 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_commands` | `max_retries` | SQL column 'max_retries' is nullable but Rust field 'max_retries: i32' is not Option |
| 337 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_events` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 338 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `worker_events` | `event_data` | SQL column 'event_data' is nullable but Rust field 'event_data: serde_json::Value' is not Option |
| 339 | `SQL_COLUMN_WITHOUT_RUST_FIELD` | `e2ee_audit_log` | `action` | SQL column 'action' (TEXT NOT NULL) has no matching Rust field in KeyAuditEntry |
| 340 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `e2ee_audit_log` | `id` | SQL column 'id' is nullable but Rust field 'id: i64' is not Option |
| 341 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `e2ee_audit_log` | `operation` | SQL column 'operation' is nullable but Rust field 'operation: String' is not Option |
| 342 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `upload_progress` | `upload_id` | SQL column 'upload_id' is nullable but Rust field 'upload_id: String' is not Option |
| 343 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `upload_progress` | `uploaded_size` | SQL column 'uploaded_size' is nullable but Rust field 'uploaded_size: i64' is not Option |
| 344 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `upload_progress` | `uploaded_chunks` | SQL column 'uploaded_chunks' is nullable but Rust field 'uploaded_chunks: i32' is not Option |
| 345 | `NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT` | `upload_progress` | `status` | SQL column 'status' is nullable but Rust field 'status: String' is not Option |

## MEDIUM Issues (7)

| # | Type | Table / Struct | Field / Column | Detail |
|---|------|----------------|-----------------|--------|
| 1 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `registration_tokens` | `created_by` | SQL column 'created_by' is NOT NULL but Rust field 'created_by: Option<String>' is Option (overly permissive) |
| 2 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `beacon_info` | `updated_ts` | SQL column 'updated_ts' is NOT NULL but Rust field 'updated_ts: Option<i64>' is Option (overly permissive) |
| 3 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `push_notification_queue` | `event_id` | SQL column 'event_id' is NOT NULL but Rust field 'event_id: Option<String>' is Option (overly permissive) |
| 4 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `push_notification_queue` | `room_id` | SQL column 'room_id' is NOT NULL but Rust field 'room_id: Option<String>' is Option (overly permissive) |
| 5 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `push_notification_queue` | `notification_type` | SQL column 'notification_type' is NOT NULL but Rust field 'notification_type: Option<String>' is Option (overly permissive) |
| 6 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `e2ee_audit_log` | `device_id` | SQL column 'device_id' is NOT NULL but Rust field 'device_id: Option<String>' is Option (overly permissive) |
| 7 | `NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL` | `upload_progress` | `expires_at` | SQL column 'expires_at' is NOT NULL but Rust field 'expires_at: Option<i64>' is Option (overly permissive) |

## LOW Issues (190)

| # | Type | Table / Struct | Field / Column | Detail |
|---|------|----------------|-----------------|--------|
| 1 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_invites` | `accepted_at` | SQL column 'accepted_at' not mapped to any Rust field (nullable/has_default) |
| 2 | `SQL_COLUMN_UNUSED_BY_RUST` | `room_invites` | `is_accepted` | SQL column 'is_accepted' not mapped to any Rust field (nullable/has_default) |
| 3 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_reports` | `description` | SQL column 'description' not mapped to any Rust field (nullable/has_default) |
| 4 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_reports` | `event_json` | SQL column 'event_json' not mapped to any Rust field (nullable/has_default) |
| 5 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_reports` | `reported_user_id` | SQL column 'reported_user_id' not mapped to any Rust field (nullable/has_default) |
| 6 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_reports` | `resolution_reason` | SQL column 'resolution_reason' not mapped to any Rust field (nullable/has_default) |
| 7 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_reports` | `status` | SQL column 'status' not mapped to any Rust field (nullable/has_default) |
| 8 | `SQL_COLUMN_UNUSED_BY_RUST` | `event_report_stats` | `escalated_reports` | SQL column 'escalated_reports' not mapped to any Rust field (nullable/has_default) |
| 9 | `SQL_COLUMN_UNUSED_BY_RUST` | `rendezvous_session` | `content` | SQL column 'content' not mapped to any Rust field (nullable/has_default) |
| 10 | `SQL_COLUMN_UNUSED_BY_RUST` | `rendezvous_session` | `updated_ts` | SQL column 'updated_ts' not mapped to any Rust field (nullable/has_default) |
| 11 | `SQL_COLUMN_UNUSED_BY_RUST` | `user_privacy_settings` | `allow_presence_lookup` | SQL column 'allow_presence_lookup' not mapped to any Rust field (nullable/has_default) |
| 12 | `SQL_COLUMN_UNUSED_BY_RUST` | `user_privacy_settings` | `allow_profile_lookup` | SQL column 'allow_profile_lookup' not mapped to any Rust field (nullable/has_default) |
| 13 | `SQL_COLUMN_UNUSED_BY_RUST` | `user_privacy_settings` | `allow_room_invites` | SQL column 'allow_room_invites' not mapped to any Rust field (nullable/has_default) |
| 14 | `SQL_COLUMN_UNUSED_BY_RUST` | `space_members` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 15 | `SQL_COLUMN_UNUSED_BY_RUST` | `third_party_rule_results` | `rule_details` | SQL column 'rule_details' not mapped to any Rust field (nullable/has_default) |
| 16 | `SQL_COLUMN_UNUSED_BY_RUST` | `third_party_rule_results` | `user_id` | SQL column 'user_id' not mapped to any Rust field (nullable/has_default) |
| 17 | `SQL_COLUMN_UNUSED_BY_RUST` | `module_execution_logs` | `input_data` | SQL column 'input_data' not mapped to any Rust field (nullable/has_default) |
| 18 | `SQL_COLUMN_UNUSED_BY_RUST` | `module_execution_logs` | `is_success` | SQL column 'is_success' not mapped to any Rust field (nullable/has_default) |
| 19 | `SQL_COLUMN_UNUSED_BY_RUST` | `module_execution_logs` | `module_id` | SQL column 'module_id' not mapped to any Rust field (nullable/has_default) |
| 20 | `SQL_COLUMN_UNUSED_BY_RUST` | `module_execution_logs` | `output_data` | SQL column 'output_data' not mapped to any Rust field (nullable/has_default) |
| 21 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_callbacks` | `headers` | SQL column 'headers' not mapped to any Rust field (nullable/has_default) |
| 22 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_callbacks` | `method` | SQL column 'method' not mapped to any Rust field (nullable/has_default) |
| 23 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_callbacks` | `retry_count` | SQL column 'retry_count' not mapped to any Rust field (nullable/has_default) |
| 24 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_callbacks` | `timeout_ms` | SQL column 'timeout_ms' not mapped to any Rust field (nullable/has_default) |
| 25 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_callbacks` | `updated_ts` | SQL column 'updated_ts' not mapped to any Rust field (nullable/has_default) |
| 26 | `SQL_COLUMN_UNUSED_BY_RUST` | `push_notification_queue` | `is_processed` | SQL column 'is_processed' not mapped to any Rust field (nullable/has_default) |
| 27 | `SQL_COLUMN_UNUSED_BY_RUST` | `push_notification_queue` | `processed_at` | SQL column 'processed_at' not mapped to any Rust field (nullable/has_default) |
| 28 | `SQL_COLUMN_UNUSED_BY_RUST` | `burn_after_read_log` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 29 | `SQL_COLUMN_UNUSED_BY_RUST` | `application_service_state` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 30 | `SQL_COLUMN_UNUSED_BY_RUST` | `application_service_transactions` | `data` | SQL column 'data' not mapped to any Rust field (nullable/has_default) |
| 31 | `SQL_COLUMN_UNUSED_BY_RUST` | `application_service_transactions` | `processed` | SQL column 'processed' not mapped to any Rust field (nullable/has_default) |
| 32 | `SQL_COLUMN_UNUSED_BY_RUST` | `application_service_transactions` | `processed_ts` | SQL column 'processed_ts' not mapped to any Rust field (nullable/has_default) |
| 33 | `SQL_COLUMN_UNUSED_BY_RUST` | `cas_tickets` | `consumed_at` | SQL column 'consumed_at' not mapped to any Rust field (nullable/has_default) |
| 34 | `SQL_COLUMN_UNUSED_BY_RUST` | `cas_proxy_tickets` | `consumed_at` | SQL column 'consumed_at' not mapped to any Rust field (nullable/has_default) |
| 35 | `SQL_COLUMN_UNUSED_BY_RUST` | `cas_slo_sessions` | `logout_sent_at` | SQL column 'logout_sent_at' not mapped to any Rust field (nullable/has_default) |
| 36 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_quota_config` | `allowed_content_types` | SQL column 'allowed_content_types' not mapped to any Rust field (nullable/has_default) |
| 37 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_quota_config` | `max_file_size` | SQL column 'max_file_size' not mapped to any Rust field (nullable/has_default) |
| 38 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_quota_config` | `max_upload_rate` | SQL column 'max_upload_rate' not mapped to any Rust field (nullable/has_default) |
| 39 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_quota_config` | `retention_days` | SQL column 'retention_days' not mapped to any Rust field (nullable/has_default) |
| 40 | `SQL_COLUMN_UNUSED_BY_RUST` | `user_media_quota` | `file_count` | SQL column 'file_count' not mapped to any Rust field (nullable/has_default) |
| 41 | `SQL_COLUMN_UNUSED_BY_RUST` | `user_media_quota` | `max_bytes` | SQL column 'max_bytes' not mapped to any Rust field (nullable/has_default) |
| 42 | `SQL_COLUMN_UNUSED_BY_RUST` | `user_media_quota` | `used_bytes` | SQL column 'used_bytes' not mapped to any Rust field (nullable/has_default) |
| 43 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_quota_alerts` | `created_ts` | SQL column 'created_ts' not mapped to any Rust field (nullable/has_default) |
| 44 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_quota_alerts` | `is_read` | SQL column 'is_read' not mapped to any Rust field (nullable/has_default) |
| 45 | `SQL_COLUMN_UNUSED_BY_RUST` | `media_quota_alerts` | `message` | SQL column 'message' not mapped to any Rust field (nullable/has_default) |
| 46 | `SQL_COLUMN_UNUSED_BY_RUST` | `background_updates` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 47 | `SQL_COLUMN_UNUSED_BY_RUST` | `background_updates` | `is_running` | SQL column 'is_running' not mapped to any Rust field (nullable/has_default) |
| 48 | `SQL_COLUMN_UNUSED_BY_RUST` | `background_updates` | `updated_ts` | SQL column 'updated_ts' not mapped to any Rust field (nullable/has_default) |
| 49 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `duration_ms` | SQL column 'duration_ms' not mapped to any Rust field (nullable/has_default) |
| 50 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 51 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `last_active_ts` | SQL column 'last_active_ts' not mapped to any Rust field (nullable/has_default) |
| 52 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `message_count` | SQL column 'message_count' not mapped to any Rust field (nullable/has_default) |
| 53 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `messages_sent` | SQL column 'messages_sent' not mapped to any Rust field (nullable/has_default) |
| 54 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `period_end` | SQL column 'period_end' not mapped to any Rust field (nullable/has_default) |
| 55 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `period_start` | SQL column 'period_start' not mapped to any Rust field (nullable/has_default) |
| 56 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `room_id` | SQL column 'room_id' not mapped to any Rust field (nullable/has_default) |
| 57 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `size_bytes` | SQL column 'size_bytes' not mapped to any Rust field (nullable/has_default) |
| 58 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `total_file_size` | SQL column 'total_file_size' not mapped to any Rust field (nullable/has_default) |
| 59 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `updated_ts` | SQL column 'updated_ts' not mapped to any Rust field (nullable/has_default) |
| 60 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `duration_ms` | SQL column 'duration_ms' not mapped to any Rust field (nullable/has_default) |
| 61 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `id` | SQL column 'id' not mapped to any Rust field (nullable/has_default) |
| 62 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `last_active_ts` | SQL column 'last_active_ts' not mapped to any Rust field (nullable/has_default) |
| 63 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `message_count` | SQL column 'message_count' not mapped to any Rust field (nullable/has_default) |
| 64 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `messages_sent` | SQL column 'messages_sent' not mapped to any Rust field (nullable/has_default) |
| 65 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `period_end` | SQL column 'period_end' not mapped to any Rust field (nullable/has_default) |
| 66 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `period_start` | SQL column 'period_start' not mapped to any Rust field (nullable/has_default) |
| 67 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `room_id` | SQL column 'room_id' not mapped to any Rust field (nullable/has_default) |
| 68 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `size_bytes` | SQL column 'size_bytes' not mapped to any Rust field (nullable/has_default) |
| 69 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `total_file_size` | SQL column 'total_file_size' not mapped to any Rust field (nullable/has_default) |
| 70 | `SQL_COLUMN_UNUSED_BY_RUST` | `voice_usage_stats` | `updated_ts` | SQL column 'updated_ts' not mapped to any Rust field (nullable/has_default) |
| 71 | `SQL_COLUMN_UNUSED_BY_RUST` | `workers` | `is_enabled` | SQL column 'is_enabled' not mapped to any Rust field (nullable/has_default) |
| 72 | `SQL_COLUMN_UNUSED_BY_RUST` | `e2ee_audit_log` | `event_id` | SQL column 'event_id' not mapped to any Rust field (nullable/has_default) |
| 73 | `TABLE_NO_RUST_STRUCT` | `account_data` | `` | Table 'account_data' (6 columns) has no matching Rust FromRow struct |
| 74 | `TABLE_NO_RUST_STRUCT` | `application_service_room_alias_namespaces` | `` | Table 'application_service_room_alias_namespaces' (5 columns) has no matching Rust FromRow struct |
| 75 | `TABLE_NO_RUST_STRUCT` | `application_service_room_namespaces` | `` | Table 'application_service_room_namespaces' (5 columns) has no matching Rust FromRow struct |
| 76 | `TABLE_NO_RUST_STRUCT` | `application_service_statistics` | `` | Table 'application_service_statistics' (10 columns) has no matching Rust FromRow struct |
| 77 | `TABLE_NO_RUST_STRUCT` | `blocked_rooms` | `` | Table 'blocked_rooms' (5 columns) has no matching Rust FromRow struct |
| 78 | `TABLE_NO_RUST_STRUCT` | `blocked_users` | `` | Table 'blocked_users' (5 columns) has no matching Rust FromRow struct |
| 79 | `TABLE_NO_RUST_STRUCT` | `burn_after_read_pending` | `` | Table 'burn_after_read_pending' (7 columns) has no matching Rust FromRow struct |
| 80 | `TABLE_NO_RUST_STRUCT` | `burn_after_read_user_defaults` | `` | Table 'burn_after_read_user_defaults' (4 columns) has no matching Rust FromRow struct |
| 81 | `TABLE_NO_RUST_STRUCT` | `cross_signing_keys` | `` | Table 'cross_signing_keys' (6 columns) has no matching Rust FromRow struct |
| 82 | `TABLE_NO_RUST_STRUCT` | `cross_signing_trust` | `` | Table 'cross_signing_trust' (8 columns) has no matching Rust FromRow struct |
| 83 | `TABLE_NO_RUST_STRUCT` | `db_metadata` | `` | Table 'db_metadata' (5 columns) has no matching Rust FromRow struct |
| 84 | `TABLE_NO_RUST_STRUCT` | `delayed_events` | `` | Table 'delayed_events' (14 columns) has no matching Rust FromRow struct |
| 85 | `TABLE_NO_RUST_STRUCT` | `deleted_events_index` | `` | Table 'deleted_events_index' (5 columns) has no matching Rust FromRow struct |
| 86 | `TABLE_NO_RUST_STRUCT` | `destination_retry_timings` | `` | Table 'destination_retry_timings' (7 columns) has no matching Rust FromRow struct |
| 87 | `TABLE_NO_RUST_STRUCT` | `device_keys` | `` | Table 'device_keys' (16 columns) has no matching Rust FromRow struct |
| 88 | `TABLE_NO_RUST_STRUCT` | `device_lists_changes` | `` | Table 'device_lists_changes' (6 columns) has no matching Rust FromRow struct |
| 89 | `TABLE_NO_RUST_STRUCT` | `device_lists_outbound_pokes` | `` | Table 'device_lists_outbound_pokes' (5 columns) has no matching Rust FromRow struct |
| 90 | `TABLE_NO_RUST_STRUCT` | `device_lists_stream` | `` | Table 'device_lists_stream' (4 columns) has no matching Rust FromRow struct |
| 91 | `TABLE_NO_RUST_STRUCT` | `device_signatures` | `` | Table 'device_signatures' (8 columns) has no matching Rust FromRow struct |
| 92 | `TABLE_NO_RUST_STRUCT` | `device_trust_status` | `` | Table 'device_trust_status' (8 columns) has no matching Rust FromRow struct |
| 93 | `TABLE_NO_RUST_STRUCT` | `device_verification_request` | `` | Table 'device_verification_request' (12 columns) has no matching Rust FromRow struct |
| 94 | `TABLE_NO_RUST_STRUCT` | `e2ee_key_requests` | `` | Table 'e2ee_key_requests' (13 columns) has no matching Rust FromRow struct |
| 95 | `TABLE_NO_RUST_STRUCT` | `e2ee_secret_storage_keys` | `` | Table 'e2ee_secret_storage_keys' (12 columns) has no matching Rust FromRow struct |
| 96 | `TABLE_NO_RUST_STRUCT` | `e2ee_security_events` | `` | Table 'e2ee_security_events' (8 columns) has no matching Rust FromRow struct |
| 97 | `TABLE_NO_RUST_STRUCT` | `e2ee_stored_secrets` | `` | Table 'e2ee_stored_secrets' (9 columns) has no matching Rust FromRow struct |
| 98 | `TABLE_NO_RUST_STRUCT` | `event_edges` | `` | Table 'event_edges' (3 columns) has no matching Rust FromRow struct |
| 99 | `TABLE_NO_RUST_STRUCT` | `event_forward_extremities` | `` | Table 'event_forward_extremities' (2 columns) has no matching Rust FromRow struct |
| 100 | `TABLE_NO_RUST_STRUCT` | `event_receipts` | `` | Table 'event_receipts' (9 columns) has no matching Rust FromRow struct |
| 101 | `TABLE_NO_RUST_STRUCT` | `federation_blacklist_config` | `` | Table 'federation_blacklist_config' (10 columns) has no matching Rust FromRow struct |
| 102 | `TABLE_NO_RUST_STRUCT` | `federation_cache` | `` | Table 'federation_cache' (5 columns) has no matching Rust FromRow struct |
| 103 | `TABLE_NO_RUST_STRUCT` | `federation_inbound_events` | `` | Table 'federation_inbound_events' (4 columns) has no matching Rust FromRow struct |
| 104 | `TABLE_NO_RUST_STRUCT` | `federation_servers` | `` | Table 'federation_servers' (10 columns) has no matching Rust FromRow struct |
| 105 | `TABLE_NO_RUST_STRUCT` | `friend_categories` | `` | Table 'friend_categories' (5 columns) has no matching Rust FromRow struct |
| 106 | `TABLE_NO_RUST_STRUCT` | `friends` | `` | Table 'friends' (4 columns) has no matching Rust FromRow struct |
| 107 | `TABLE_NO_RUST_STRUCT` | `ip_blocks` | `` | Table 'ip_blocks' (5 columns) has no matching Rust FromRow struct |
| 108 | `TABLE_NO_RUST_STRUCT` | `key_rotation_config` | `` | Table 'key_rotation_config' (2 columns) has no matching Rust FromRow struct |
| 109 | `TABLE_NO_RUST_STRUCT` | `key_rotation_history` | `` | Table 'key_rotation_history' (7 columns) has no matching Rust FromRow struct |
| 110 | `TABLE_NO_RUST_STRUCT` | `key_rotation_log` | `` | Table 'key_rotation_log' (9 columns) has no matching Rust FromRow struct |
| 111 | `TABLE_NO_RUST_STRUCT` | `key_rotation_pending` | `` | Table 'key_rotation_pending' (4 columns) has no matching Rust FromRow struct |
| 112 | `TABLE_NO_RUST_STRUCT` | `key_rotation_state` | `` | Table 'key_rotation_state' (4 columns) has no matching Rust FromRow struct |
| 113 | `TABLE_NO_RUST_STRUCT` | `key_signatures` | `` | Table 'key_signatures' (7 columns) has no matching Rust FromRow struct |
| 114 | `TABLE_NO_RUST_STRUCT` | `lazy_loaded_members` | `` | Table 'lazy_loaded_members' (6 columns) has no matching Rust FromRow struct |
| 115 | `TABLE_NO_RUST_STRUCT` | `leak_alerts` | `` | Table 'leak_alerts' (10 columns) has no matching Rust FromRow struct |
| 116 | `TABLE_NO_RUST_STRUCT` | `media_metadata` | `` | Table 'media_metadata' (9 columns) has no matching Rust FromRow struct |
| 117 | `TABLE_NO_RUST_STRUCT` | `media_quota` | `` | Table 'media_quota' (6 columns) has no matching Rust FromRow struct |
| 118 | `TABLE_NO_RUST_STRUCT` | `megolm_key_shares` | `` | Table 'megolm_key_shares' (4 columns) has no matching Rust FromRow struct |
| 119 | `TABLE_NO_RUST_STRUCT` | `megolm_sessions` | `` | Table 'megolm_sessions' (10 columns) has no matching Rust FromRow struct |
| 120 | `TABLE_NO_RUST_STRUCT` | `migration_audit` | `` | Table 'migration_audit' (11 columns) has no matching Rust FromRow struct |
| 121 | `TABLE_NO_RUST_STRUCT` | `moderation_actions` | `` | Table 'moderation_actions' (10 columns) has no matching Rust FromRow struct |
| 122 | `TABLE_NO_RUST_STRUCT` | `oidc_user_mapping` | `` | Table 'oidc_user_mapping' (7 columns) has no matching Rust FromRow struct |
| 123 | `TABLE_NO_RUST_STRUCT` | `olm_accounts` | `` | Table 'olm_accounts' (9 columns) has no matching Rust FromRow struct |
| 124 | `TABLE_NO_RUST_STRUCT` | `olm_sessions` | `` | Table 'olm_sessions' (11 columns) has no matching Rust FromRow struct |
| 125 | `TABLE_NO_RUST_STRUCT` | `one_time_keys` | `` | Table 'one_time_keys' (10 columns) has no matching Rust FromRow struct |
| 126 | `TABLE_NO_RUST_STRUCT` | `password_history` | `` | Table 'password_history' (4 columns) has no matching Rust FromRow struct |
| 127 | `TABLE_NO_RUST_STRUCT` | `password_policy` | `` | Table 'password_policy' (5 columns) has no matching Rust FromRow struct |
| 128 | `TABLE_NO_RUST_STRUCT` | `presence` | `` | Table 'presence' (7 columns) has no matching Rust FromRow struct |
| 129 | `TABLE_NO_RUST_STRUCT` | `presence_routes` | `` | Table 'presence_routes' (6 columns) has no matching Rust FromRow struct |
| 130 | `TABLE_NO_RUST_STRUCT` | `presence_stream` | `` | Table 'presence_stream' (7 columns) has no matching Rust FromRow struct |
| 131 | `TABLE_NO_RUST_STRUCT` | `presence_subscriptions` | `` | Table 'presence_subscriptions' (3 columns) has no matching Rust FromRow struct |
| 132 | `TABLE_NO_RUST_STRUCT` | `push_devices` | `` | Table 'push_devices' (14 columns) has no matching Rust FromRow struct |
| 133 | `TABLE_NO_RUST_STRUCT` | `push_notification_log` | `` | Table 'push_notification_log' (18 columns) has no matching Rust FromRow struct |
| 134 | `TABLE_NO_RUST_STRUCT` | `pushers` | `` | Table 'pushers' (15 columns) has no matching Rust FromRow struct |
| 135 | `TABLE_NO_RUST_STRUCT` | `qr_login_transactions` | `` | Table 'qr_login_transactions' (7 columns) has no matching Rust FromRow struct |
| 136 | `TABLE_NO_RUST_STRUCT` | `rate_limit_callbacks` | `` | Table 'rate_limit_callbacks' (5 columns) has no matching Rust FromRow struct |
| 137 | `TABLE_NO_RUST_STRUCT` | `rate_limits` | `` | Table 'rate_limits' (4 columns) has no matching Rust FromRow struct |
| 138 | `TABLE_NO_RUST_STRUCT` | `reaction_aggregations` | `` | Table 'reaction_aggregations' (7 columns) has no matching Rust FromRow struct |
| 139 | `TABLE_NO_RUST_STRUCT` | `read_markers` | `` | Table 'read_markers' (7 columns) has no matching Rust FromRow struct |
| 140 | `TABLE_NO_RUST_STRUCT` | `receipts_linearized` | `` | Table 'receipts_linearized' (6 columns) has no matching Rust FromRow struct |
| 141 | `TABLE_NO_RUST_STRUCT` | `refresh_token_families` | `` | Table 'refresh_token_families' (9 columns) has no matching Rust FromRow struct |
| 142 | `TABLE_NO_RUST_STRUCT` | `rendezvous_messages` | `` | Table 'rendezvous_messages' (6 columns) has no matching Rust FromRow struct |
| 143 | `TABLE_NO_RUST_STRUCT` | `replication_positions` | `` | Table 'replication_positions' (5 columns) has no matching Rust FromRow struct |
| 144 | `TABLE_NO_RUST_STRUCT` | `retention_cleanup_queue` | `` | Table 'retention_cleanup_queue' (11 columns) has no matching Rust FromRow struct |
| 145 | `TABLE_NO_RUST_STRUCT` | `room_account_data` | `` | Table 'room_account_data' (7 columns) has no matching Rust FromRow struct |
| 146 | `TABLE_NO_RUST_STRUCT` | `room_aliases` | `` | Table 'room_aliases' (4 columns) has no matching Rust FromRow struct |
| 147 | `TABLE_NO_RUST_STRUCT` | `room_children` | `` | Table 'room_children' (8 columns) has no matching Rust FromRow struct |
| 148 | `TABLE_NO_RUST_STRUCT` | `room_directory` | `` | Table 'room_directory' (6 columns) has no matching Rust FromRow struct |
| 149 | `TABLE_NO_RUST_STRUCT` | `room_ephemeral` | `` | Table 'room_ephemeral' (8 columns) has no matching Rust FromRow struct |
| 150 | `TABLE_NO_RUST_STRUCT` | `room_events` | `` | Table 'room_events' (10 columns) has no matching Rust FromRow struct |
| 151 | `TABLE_NO_RUST_STRUCT` | `room_invite_allowlist` | `` | Table 'room_invite_allowlist' (4 columns) has no matching Rust FromRow struct |
| 152 | `TABLE_NO_RUST_STRUCT` | `room_invite_blocklist` | `` | Table 'room_invite_blocklist' (4 columns) has no matching Rust FromRow struct |
| 153 | `TABLE_NO_RUST_STRUCT` | `room_parents` | `` | Table 'room_parents' (7 columns) has no matching Rust FromRow struct |
| 154 | `TABLE_NO_RUST_STRUCT` | `room_state_events` | `` | Table 'room_state_events' (7 columns) has no matching Rust FromRow struct |
| 155 | `TABLE_NO_RUST_STRUCT` | `room_stats_current` | `` | Table 'room_stats_current' (10 columns) has no matching Rust FromRow struct |
| 156 | `TABLE_NO_RUST_STRUCT` | `room_sticky_events` | `` | Table 'room_sticky_events' (8 columns) has no matching Rust FromRow struct |
| 157 | `TABLE_NO_RUST_STRUCT` | `room_summary_update_queue` | `` | Table 'room_summary_update_queue' (11 columns) has no matching Rust FromRow struct |
| 158 | `TABLE_NO_RUST_STRUCT` | `rooms` | `` | Table 'rooms' (15 columns) has no matching Rust FromRow struct |
| 159 | `TABLE_NO_RUST_STRUCT` | `saml_config_overrides` | `` | Table 'saml_config_overrides' (3 columns) has no matching Rust FromRow struct |
| 160 | `TABLE_NO_RUST_STRUCT` | `schema_migrations` | `` | Table 'schema_migrations' (8 columns) has no matching Rust FromRow struct |
| 161 | `TABLE_NO_RUST_STRUCT` | `search_index` | `` | Table 'search_index' (9 columns) has no matching Rust FromRow struct |
| 162 | `TABLE_NO_RUST_STRUCT` | `secure_backup_session_keys` | `` | Table 'secure_backup_session_keys' (6 columns) has no matching Rust FromRow struct |
| 163 | `TABLE_NO_RUST_STRUCT` | `secure_key_backups` | `` | Table 'secure_key_backups' (8 columns) has no matching Rust FromRow struct |
| 164 | `TABLE_NO_RUST_STRUCT` | `security_events` | `` | Table 'security_events' (7 columns) has no matching Rust FromRow struct |
| 165 | `TABLE_NO_RUST_STRUCT` | `server_media_quota` | `` | Table 'server_media_quota' (8 columns) has no matching Rust FromRow struct |
| 166 | `TABLE_NO_RUST_STRUCT` | `server_notices` | `` | Table 'server_notices' (5 columns) has no matching Rust FromRow struct |
| 167 | `TABLE_NO_RUST_STRUCT` | `space_hierarchy` | `` | Table 'space_hierarchy' (9 columns) has no matching Rust FromRow struct |
| 168 | `TABLE_NO_RUST_STRUCT` | `space_statistics` | `` | Table 'space_statistics' (7 columns) has no matching Rust FromRow struct |
| 169 | `TABLE_NO_RUST_STRUCT` | `sync_stream_id` | `` | Table 'sync_stream_id' (4 columns) has no matching Rust FromRow struct |
| 170 | `TABLE_NO_RUST_STRUCT` | `thread_replies` | `` | Table 'thread_replies' (12 columns) has no matching Rust FromRow struct |
| 171 | `TABLE_NO_RUST_STRUCT` | `thumbnails` | `` | Table 'thumbnails' (8 columns) has no matching Rust FromRow struct |
| 172 | `TABLE_NO_RUST_STRUCT` | `to_device_messages` | `` | Table 'to_device_messages' (10 columns) has no matching Rust FromRow struct |
| 173 | `TABLE_NO_RUST_STRUCT` | `to_device_transactions` | `` | Table 'to_device_transactions' (6 columns) has no matching Rust FromRow struct |
| 174 | `TABLE_NO_RUST_STRUCT` | `token_blacklist` | `` | Table 'token_blacklist' (8 columns) has no matching Rust FromRow struct |
| 175 | `TABLE_NO_RUST_STRUCT` | `typing` | `` | Table 'typing' (4 columns) has no matching Rust FromRow struct |
| 176 | `TABLE_NO_RUST_STRUCT` | `typing_stream` | `` | Table 'typing_stream' (6 columns) has no matching Rust FromRow struct |
| 177 | `TABLE_NO_RUST_STRUCT` | `upload_chunks` | `` | Table 'upload_chunks' (5 columns) has no matching Rust FromRow struct |
| 178 | `TABLE_NO_RUST_STRUCT` | `user_account_data` | `` | Table 'user_account_data' (5 columns) has no matching Rust FromRow struct |
| 179 | `TABLE_NO_RUST_STRUCT` | `user_directory` | `` | Table 'user_directory' (5 columns) has no matching Rust FromRow struct |
| 180 | `TABLE_NO_RUST_STRUCT` | `user_filters` | `` | Table 'user_filters' (5 columns) has no matching Rust FromRow struct |
| 181 | `TABLE_NO_RUST_STRUCT` | `user_notification_settings` | `` | Table 'user_notification_settings' (3 columns) has no matching Rust FromRow struct |
| 182 | `TABLE_NO_RUST_STRUCT` | `user_reputations` | `` | Table 'user_reputations' (11 columns) has no matching Rust FromRow struct |
| 183 | `TABLE_NO_RUST_STRUCT` | `user_settings` | `` | Table 'user_settings' (6 columns) has no matching Rust FromRow struct |
| 184 | `TABLE_NO_RUST_STRUCT` | `verification_qr` | `` | Table 'verification_qr' (6 columns) has no matching Rust FromRow struct |
| 185 | `TABLE_NO_RUST_STRUCT` | `verification_requests` | `` | Table 'verification_requests' (9 columns) has no matching Rust FromRow struct |
| 186 | `TABLE_NO_RUST_STRUCT` | `verification_sas` | `` | Table 'verification_sas' (10 columns) has no matching Rust FromRow struct |
| 187 | `TABLE_NO_RUST_STRUCT` | `voice_messages` | `` | Table 'voice_messages' (17 columns) has no matching Rust FromRow struct |
| 188 | `TABLE_NO_RUST_STRUCT` | `worker_connections` | `` | Table 'worker_connections' (11 columns) has no matching Rust FromRow struct |
| 189 | `TABLE_NO_RUST_STRUCT` | `worker_load_stats` | `` | Table 'worker_load_stats' (9 columns) has no matching Rust FromRow struct |
| 190 | `TABLE_NO_RUST_STRUCT` | `worker_statistics` | `` | Table 'worker_statistics' (11 columns) has no matching Rust FromRow struct |

## INFO Issues (19)

| # | Type | Table / Struct | Field / Column | Detail |
|---|------|----------------|-----------------|--------|
| 1 | `NO_TABLE_MATCH` | `RefreshTokenFamily` | `` | Struct 'RefreshTokenFamily' has no obvious matching SQL table |
| 2 | `NO_TABLE_MATCH` | `TokenBlacklistEntry` | `` | Struct 'TokenBlacklistEntry' has no obvious matching SQL table |
| 3 | `NO_TABLE_MATCH` | `RefreshTokenStats` | `` | Struct 'RefreshTokenStats' has no obvious matching SQL table |
| 4 | `NO_TABLE_MATCH` | `PresenceSnapshot` | `` | Struct 'PresenceSnapshot' has no obvious matching SQL table |
| 5 | `NO_TABLE_MATCH` | `AggregationResult` | `` | Struct 'AggregationResult' has no obvious matching SQL table |
| 6 | `NO_TABLE_MATCH` | `ThreadReply` | `` | Struct 'ThreadReply' has no obvious matching SQL table |
| 7 | `NO_TABLE_MATCH` | `ThreadStatistics` | `` | Struct 'ThreadStatistics' has no obvious matching SQL table |
| 8 | `NO_TABLE_MATCH` | `RoomSummaryUpdateQueueItem` | `` | Struct 'RoomSummaryUpdateQueueItem' has no obvious matching SQL table |
| 9 | `NO_TABLE_MATCH` | `StateEvent` | `` | Struct 'StateEvent' has no obvious matching SQL table |
| 10 | `NO_TABLE_MATCH` | `EventReportId` | `` | Struct 'EventReportId' has no obvious matching SQL table |
| 11 | `NO_TABLE_MATCH` | `StoredRendezvousMessage` | `` | Struct 'StoredRendezvousMessage' has no obvious matching SQL table |
| 12 | `NO_TABLE_MATCH` | `SpaceHierarchyRoom` | `` | Struct 'SpaceHierarchyRoom' has no obvious matching SQL table |
| 13 | `NO_TABLE_MATCH` | `BurnPendingRow` | `` | Struct 'BurnPendingRow' has no obvious matching SQL table |
| 14 | `NO_TABLE_MATCH` | `BurnLogRow` | `` | Struct 'BurnLogRow' has no obvious matching SQL table |
| 15 | `NO_TABLE_MATCH` | `BurnUserDefaultsRow` | `` | Struct 'BurnUserDefaultsRow' has no obvious matching SQL table |
| 16 | `NO_TABLE_MATCH` | `RetentionCleanupQueueItem` | `` | Struct 'RetentionCleanupQueueItem' has no obvious matching SQL table |
| 17 | `NO_TABLE_MATCH` | `DeletedEventIndex` | `` | Struct 'DeletedEventIndex' has no obvious matching SQL table |
| 18 | `NO_TABLE_MATCH` | `CaptchaRateLimit` | `` | Struct 'CaptchaRateLimit' has no obvious matching SQL table |
| 19 | `NO_TABLE_MATCH` | `DeviceVerificationStatus` | `` | Struct 'DeviceVerificationStatus' has no obvious matching SQL table |

## Orphan Tables (No Matching Rust Struct)

| Table | Columns | Source |
|-------|---------|--------|
`account_data` | 6 | 00000000_unified_schema_v7.sql |
`application_service_room_alias_namespaces` | 5 | 00000000_unified_schema_v7.sql |
`application_service_room_namespaces` | 5 | 00000000_unified_schema_v7.sql |
`application_service_statistics` | 10 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`blocked_rooms` | 5 | 00000000_unified_schema_v7.sql |
`blocked_users` | 5 | 00000000_unified_schema_v7.sql |
`burn_after_read_pending` | 7 | 20260515120000_burn_after_read_persistence.sql |
`burn_after_read_user_defaults` | 4 | 20260515120000_burn_after_read_persistence.sql |
`cross_signing_keys` | 6 | 00000000_unified_schema_v7.sql |
`cross_signing_trust` | 8 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`db_metadata` | 5 | 00000000_unified_schema_v7.sql |
`delayed_events` | 14 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`deleted_events_index` | 5 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`destination_retry_timings` | 7 | 00000000_unified_schema_v7.sql |
`device_keys` | 16 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql) |
`device_lists_changes` | 6 | 00000000_unified_schema_v7.sql |
`device_lists_outbound_pokes` | 5 | 00000000_unified_schema_v7.sql |
`device_lists_stream` | 4 | 00000000_unified_schema_v7.sql |
`device_signatures` | 8 | 00000000_unified_schema_v7.sql |
`device_trust_status` | 8 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`device_verification_request` | 12 | 00000000_unified_schema_v7.sql |
`e2ee_key_requests` | 13 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql) |
`e2ee_secret_storage_keys` | 12 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`e2ee_security_events` | 8 | 00000000_unified_schema_v7.sql |
`e2ee_stored_secrets` | 9 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`event_edges` | 3 | 00000000_unified_schema_v7.sql |
`event_forward_extremities` | 2 | 00000000_unified_schema_v7.sql |
`event_receipts` | 9 | 00000000_unified_schema_v7.sql |
`federation_blacklist_config` | 10 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`federation_cache` | 5 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`federation_inbound_events` | 4 | 00000000_unified_schema_v7.sql |
`federation_servers` | 10 | 00000000_unified_schema_v7.sql |
`friend_categories` | 5 | 00000000_unified_schema_v7.sql+00000001_extensions.sql |
`friends` | 4 | 00000000_unified_schema_v7.sql+00000001_extensions.sql |
`ip_blocks` | 5 | 00000000_unified_schema_v7.sql |
`key_rotation_config` | 2 | 20260528000001_key_rotation_config_table.sql |
`key_rotation_history` | 7 | 00000000_unified_schema_v7.sql |
`key_rotation_log` | 9 | 00000000_unified_schema_v7.sql |
`key_rotation_pending` | 4 | 20260516000001_key_rotation_pending_tables.sql |
`key_rotation_state` | 4 | 20260516000001_key_rotation_pending_tables.sql |
`key_signatures` | 7 | 00000000_unified_schema_v7.sql |
`lazy_loaded_members` | 6 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`leak_alerts` | 10 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`media_metadata` | 9 | 00000000_unified_schema_v7.sql |
`media_quota` | 6 | 00000000_unified_schema_v7.sql |
`megolm_key_shares` | 4 | 20260516000001_key_rotation_pending_tables.sql |
`megolm_sessions` | 10 | 00000000_unified_schema_v7.sql |
`migration_audit` | 11 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`moderation_actions` | 10 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`oidc_user_mapping` | 7 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`olm_accounts` | 9 | 00000000_unified_schema_v7.sql |
`olm_sessions` | 11 | 00000000_unified_schema_v7.sql |
`one_time_keys` | 10 | 00000000_unified_schema_v7.sql |
`password_history` | 4 | 00000000_unified_schema_v7.sql |
`password_policy` | 5 | 00000000_unified_schema_v7.sql |
`presence` | 7 | 00000000_unified_schema_v7.sql |
`presence_routes` | 6 | 00000000_unified_schema_v7.sql |
`presence_stream` | 7 | 00000000_unified_schema_v7.sql |
`presence_subscriptions` | 3 | 00000000_unified_schema_v7.sql |
`push_devices` | 14 | 00000000_unified_schema_v7.sql |
`push_notification_log` | 18 | 00000000_unified_schema_v7.sql+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql)+ALTER(00000000_unified_schema_v7.sql) |
`pushers` | 15 | 00000000_unified_schema_v7.sql |
`qr_login_transactions` | 7 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`rate_limit_callbacks` | 5 | 00000000_unified_schema_v7.sql |
`rate_limits` | 4 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`reaction_aggregations` | 7 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`read_markers` | 7 | 00000000_unified_schema_v7.sql |
`receipts_linearized` | 6 | 00000000_unified_schema_v7.sql |
`refresh_token_families` | 9 | 00000000_unified_schema_v7.sql |
`rendezvous_messages` | 6 | 00000000_unified_schema_v7.sql |
`replication_positions` | 5 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`retention_cleanup_queue` | 11 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`room_account_data` | 7 | 00000000_unified_schema_v7.sql |
`room_aliases` | 4 | 00000000_unified_schema_v7.sql |
`room_children` | 8 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`room_directory` | 6 | 00000000_unified_schema_v7.sql |
`room_ephemeral` | 8 | 00000000_unified_schema_v7.sql |
`room_events` | 10 | 00000000_unified_schema_v7.sql |
`room_invite_allowlist` | 4 | 00000000_unified_schema_v7.sql |
`room_invite_blocklist` | 4 | 00000000_unified_schema_v7.sql |
`room_parents` | 7 | 00000000_unified_schema_v7.sql |
`room_state_events` | 7 | 00000000_unified_schema_v7.sql |
`room_stats_current` | 10 | 00000000_unified_schema_v7.sql |
`room_sticky_events` | 8 | 00000000_unified_schema_v7.sql |
`room_summary_update_queue` | 11 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`rooms` | 15 | 00000000_unified_schema_v7.sql |
`saml_config_overrides` | 3 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`schema_migrations` | 8 | 00000000_unified_schema_v7.sql |
`search_index` | 9 | 00000000_unified_schema_v7.sql |
`secure_backup_session_keys` | 6 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`secure_key_backups` | 8 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`security_events` | 7 | 00000000_unified_schema_v7.sql |
`server_media_quota` | 8 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`server_notices` | 5 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`space_hierarchy` | 9 | 00000000_unified_schema_v7.sql |
`space_statistics` | 7 | 00000000_unified_schema_v7.sql |
`sync_stream_id` | 4 | 00000000_unified_schema_v7.sql |
`thread_replies` | 12 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`thumbnails` | 8 | 00000000_unified_schema_v7.sql |
`to_device_messages` | 10 | 00000000_unified_schema_v7.sql |
`to_device_transactions` | 6 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`token_blacklist` | 8 | 00000000_unified_schema_v7.sql |
`typing` | 4 | 00000000_unified_schema_v7.sql |
`typing_stream` | 6 | 00000000_unified_schema_v7.sql |
`upload_chunks` | 5 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`user_account_data` | 5 | 00000000_unified_schema_v7.sql |
`user_directory` | 5 | 00000000_unified_schema_v7.sql |
`user_filters` | 5 | 00000000_unified_schema_v7.sql |
`user_notification_settings` | 3 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`user_reputations` | 11 | 00000000_unified_schema_v7.sql |
`user_settings` | 6 | 00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`verification_qr` | 6 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`verification_requests` | 9 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`verification_sas` | 10 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`voice_messages` | 17 | 00000000_unified_schema_v7.sql+00000001_extensions.sql |
`worker_connections` | 11 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`worker_load_stats` | 9 | 00000000_unified_schema_v7.sql+00000000_unified_schema_v7.sql+20260515000001_consolidated_schema_contract_and_features_v7.sql |
`worker_statistics` | 11 | 00000000_unified_schema_v7.sql |
