-- ============================================================================
-- Rollback Script: 00000001_extensions.undo.sql
-- Forward Script: 00000001_extensions.sql
-- Created: 2026-05-07 (consolidated from 5 extension undo stubs)
-- Risk: LOW (all IF EXISTS guarded, no data mutation)
-- Rollback RTO: < 1 minute
-- ============================================================================

--no-transaction

-- Extension: CAS SSO
DROP TABLE IF EXISTS cas_slo_sessions;
DROP TABLE IF EXISTS cas_user_attributes;
DROP TABLE IF EXISTS cas_services;
DROP TABLE IF EXISTS cas_proxy_granting_tickets;
DROP TABLE IF EXISTS cas_proxy_tickets;
DROP TABLE IF EXISTS cas_tickets;

-- Extension: SAML SSO
DROP TABLE IF EXISTS saml_logout_requests;
DROP TABLE IF EXISTS saml_auth_events;
DROP TABLE IF EXISTS saml_identity_providers;
DROP TABLE IF EXISTS saml_user_mapping;
DROP TABLE IF EXISTS saml_sessions;

-- Extension: Friends System
DROP TABLE IF EXISTS friend_categories;
DROP TABLE IF EXISTS friend_requests;
DROP TABLE IF EXISTS friends;

-- Extension: Voice Messages
DROP TABLE IF EXISTS voice_usage_stats;
DROP TABLE IF EXISTS voice_messages;

-- Extension: Privacy Settings
DROP TABLE IF EXISTS user_privacy_settings;

-- ============================================================================
-- Rollback Verification
-- ============================================================================
-- Verify all extension tables removed:
-- SELECT tablename FROM pg_tables
-- WHERE schemaname = 'public' AND tablename IN (
--   'cas_tickets','cas_proxy_tickets','cas_proxy_granting_tickets',
--   'cas_services','cas_user_attributes','cas_slo_sessions',
--   'saml_sessions','saml_user_mapping','saml_identity_providers',
--   'saml_auth_events','saml_logout_requests',
--   'friends','friend_requests','friend_categories',
--   'voice_messages','voice_usage_stats',
--   'user_privacy_settings'
-- ); -- 应返回 0 行
