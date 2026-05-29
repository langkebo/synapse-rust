-- ============================================================================
-- Rollback Script: 20260515000008_consolidated_field_rename_expires_at_v7.undo.sql
-- Forward Script: 20260515000008_consolidated_field_rename_expires_at_v7.sql
-- Created: 2026-05-09
-- Risk: HIGH — Reverts column renames. Ensure Rust code is reverted first.
-- Rollback RTO: < 5 minutes
-- ============================================================================

SET TIME ZONE 'UTC';

ALTER TABLE IF EXISTS registration_captcha RENAME COLUMN expires_at TO expires_ts;
ALTER TABLE IF EXISTS qr_login_sessions RENAME COLUMN expires_at TO expires_ts;
ALTER TABLE IF EXISTS user_threepids RENAME COLUMN verification_expires_at TO verification_expires_ts;
ALTER TABLE IF EXISTS rendezvous_session RENAME COLUMN expires_at TO expires_ts;
ALTER TABLE IF EXISTS invite_tokens RENAME COLUMN expires_at TO expires_ts;
ALTER TABLE IF EXISTS registration_tokens RENAME COLUMN expires_at TO expires_ts;
ALTER TABLE IF EXISTS refresh_tokens RENAME COLUMN expires_at TO expires_ts;
ALTER TABLE IF EXISTS access_tokens RENAME COLUMN expires_at TO expires_ts;
ALTER TABLE IF EXISTS saml_sessions RENAME COLUMN expires_at TO expires_ts;
