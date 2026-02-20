-- =============================================================================
-- Synapse-Rust 数据库字段规范化回滚脚本
-- 版本: 1.1.0
-- 创建日期: 2026-02-20
-- 描述: 回滚字段命名规范化修改
-- 
-- 执行方式:
--   docker exec -i synapse-postgres psql -U synapse -d synapse_test < 20260220000000_rollback_normalize.sql
-- =============================================================================

BEGIN;

-- =============================================================================
-- 第一部分: 回滚布尔字段规范化
-- =============================================================================

-- 1. users 表
ALTER TABLE users RENAME COLUMN is_deactivated TO deactivated;
ALTER TABLE users RENAME COLUMN is_shadow_banned TO shadow_banned;

-- 2. access_tokens 表
ALTER TABLE access_tokens RENAME COLUMN revoked_ts TO invalidated_ts;

-- 3. events 表
ALTER TABLE events RENAME COLUMN is_processed TO processed;
ALTER TABLE events RENAME COLUMN is_outlier TO outlier;

-- 4. voice_messages 表
ALTER TABLE voice_messages RENAME COLUMN is_processed TO processed;

-- 5. media_repository 表
ALTER TABLE media_repository RENAME COLUMN is_quarantined TO quarantine_media;
ALTER TABLE media_repository RENAME COLUMN is_safe_from_quarantine TO safe_from_quarantine;

-- 6. space_children 表
ALTER TABLE space_children RENAME COLUMN is_suggested TO suggested;

-- 7. ip_reputation 表
ALTER TABLE ip_reputation RENAME COLUMN is_blocked TO blocked;

-- 8. federation_blacklist 表
ALTER TABLE federation_blacklist RENAME COLUMN is_enabled TO is_active;

-- 9. federation_blacklist_rule 表
ALTER TABLE federation_blacklist_rule RENAME COLUMN is_enabled TO enabled;

-- 10. modules 表
ALTER TABLE modules RENAME COLUMN is_enabled TO enabled;

-- 11. saml_identity_providers 表
ALTER TABLE saml_identity_providers RENAME COLUMN is_enabled TO enabled;

-- 12. cas_services 表
ALTER TABLE cas_services RENAME COLUMN is_enabled TO enabled;

-- 13. captcha_template 表
ALTER TABLE captcha_template RENAME COLUMN is_enabled TO enabled;

-- 14. push_device 表
ALTER TABLE push_device RENAME COLUMN is_enabled TO enabled;

-- 15. push_rule 表
ALTER TABLE push_rule RENAME COLUMN is_enabled TO enabled;

-- 16. pushers 表
ALTER TABLE pushers RENAME COLUMN is_enabled TO enabled;

-- 17. push_rules 表
ALTER TABLE push_rules RENAME COLUMN is_enabled TO enabled;

-- 18. server_notifications 表
ALTER TABLE server_notifications RENAME COLUMN is_enabled TO is_active;
ALTER TABLE server_notifications RENAME COLUMN is_dismissable TO is_dismissible;

-- 19. application_services 表
ALTER TABLE application_services RENAME COLUMN is_enabled TO is_active;

-- 20. registration_tokens 表
ALTER TABLE registration_tokens RENAME COLUMN is_enabled TO is_active;

-- 21. registration_token_batches 表
ALTER TABLE registration_token_batches RENAME COLUMN is_enabled TO is_active;

-- 22. thread_roots 表
ALTER TABLE thread_roots RENAME COLUMN is_enabled TO is_active;

-- 23. ip_blocks 表
ALTER TABLE ip_blocks RENAME COLUMN is_enabled TO is_active;

-- =============================================================================
-- 第二部分: 回滚时间字段规范化
-- =============================================================================

-- 1. devices 表
ALTER TABLE devices RENAME COLUMN created_ts TO created_at;

-- 2. federation_signing_keys 表
ALTER TABLE federation_signing_keys RENAME COLUMN created_ts TO created_at;

-- 3. ip_reputation 表
ALTER TABLE ip_reputation RENAME COLUMN last_failed_ts TO last_failed_at;
ALTER TABLE ip_reputation RENAME COLUMN last_success_ts TO last_success_at;
ALTER TABLE ip_reputation RENAME COLUMN blocked_ts TO blocked_at;

-- =============================================================================
-- 第三部分: 恢复被删除的字段
-- =============================================================================

-- 1. access_tokens 表
ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS ip VARCHAR(255);

-- 2. events 表
ALTER TABLE events ADD COLUMN IF NOT EXISTS type VARCHAR(255);

-- 3. voice_messages 表
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS processed_at BIGINT;
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS waveform_data TEXT;
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS transcribe_text TEXT;

-- =============================================================================
-- 第四部分: 恢复索引
-- =============================================================================

DROP INDEX IF EXISTS idx_users_deactivated;
CREATE INDEX IF NOT EXISTS idx_users_deactivated ON users(deactivated) WHERE deactivated = TRUE;

DROP INDEX IF EXISTS idx_voice_messages_processed;
CREATE INDEX IF NOT EXISTS idx_voice_messages_processed ON voice_messages(processed);

DROP INDEX IF EXISTS idx_federation_blacklist_enabled;
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_active ON federation_blacklist(is_active);

DROP INDEX IF EXISTS idx_federation_blacklist_rule_enabled;
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_rule_enabled ON federation_blacklist_rule(enabled);

DROP INDEX IF EXISTS idx_ip_blocks_enabled;
CREATE INDEX IF NOT EXISTS idx_ip_blocks_active ON ip_blocks(is_active);

-- 删除迁移记录
DELETE FROM schema_migrations WHERE version = '1.1.0';

COMMIT;

-- =============================================================================
-- 回滚完成
-- =============================================================================
