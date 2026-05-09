-- ============================================================================
-- Rollback Script: 20260515000006_consolidated_constraint_governance_v7.undo.sql
-- Forward Script: 20260515000006_consolidated_constraint_governance_v7.sql
-- Created: 2026-05-09
-- ============================================================================

SET TIME ZONE 'UTC';

ALTER TABLE IF EXISTS refresh_token_rotations
    DROP CONSTRAINT IF EXISTS fk_refresh_token_rotations_family;

ALTER TABLE IF EXISTS refresh_token_families
    DROP CONSTRAINT IF EXISTS fk_refresh_token_families_device;

ALTER TABLE IF EXISTS refresh_token_families
    DROP CONSTRAINT IF EXISTS fk_refresh_token_families_user;

ALTER TABLE IF EXISTS refresh_token_usage
    DROP CONSTRAINT IF EXISTS fk_refresh_token_usage_user;

ALTER TABLE IF EXISTS refresh_token_usage
    DROP CONSTRAINT IF EXISTS fk_refresh_token_usage_token;

ALTER TABLE IF EXISTS report_rate_limits
    DROP CONSTRAINT IF EXISTS fk_report_rate_limits_user;

ALTER TABLE IF EXISTS registration_token_usage
    DROP CONSTRAINT IF EXISTS fk_registration_token_usage_user;

ALTER TABLE IF EXISTS token_blacklist
    DROP CONSTRAINT IF EXISTS fk_token_blacklist_user;

ALTER TABLE IF EXISTS refresh_tokens
    DROP CONSTRAINT IF EXISTS fk_refresh_tokens_device;

ALTER TABLE IF EXISTS access_tokens
    DROP CONSTRAINT IF EXISTS fk_access_tokens_device;

DROP INDEX IF EXISTS idx_token_blacklist_user_id;
DROP INDEX IF EXISTS idx_refresh_token_families_device;
DROP INDEX IF EXISTS idx_refresh_tokens_device_id;
DROP INDEX IF EXISTS idx_access_tokens_device_id;

ALTER TABLE IF EXISTS presence_subscriptions
    DROP CONSTRAINT IF EXISTS pk_presence_subscriptions;

ALTER TABLE IF EXISTS presence_subscriptions
    ADD CONSTRAINT uq_presence_subscriptions UNIQUE (subscriber_id, target_id);

ALTER TABLE IF EXISTS typing
    DROP CONSTRAINT IF EXISTS pk_typing;

ALTER TABLE IF EXISTS typing
    ADD CONSTRAINT uq_typing_user_room UNIQUE (user_id, room_id);