-- =============================================================================
-- Synapse-Rust 数据库字段规范化迁移脚本
-- 版本: 1.1.0
-- 创建日期: 2026-02-20
-- 描述: 根据 DATABASE_FIELD_STANDARDS.md 规范，规范化字段命名
-- 
-- 执行方式:
--   docker exec -i synapse-postgres psql -U synapse -d synapse_test < 20260220000000_normalize_field_names.sql
-- =============================================================================

BEGIN;

-- 记录迁移版本
INSERT INTO schema_migrations (version, description)
VALUES ('1.1.0', 'Normalize field names according to DATABASE_FIELD_STANDARDS.md')
ON CONFLICT (version) DO NOTHING;

-- =============================================================================
-- 第一部分: 布尔字段规范化 (添加 is_ 前缀)
-- =============================================================================

-- 1. users 表
ALTER TABLE users RENAME COLUMN deactivated TO is_deactivated;
ALTER TABLE users RENAME COLUMN shadow_banned TO is_shadow_banned;

-- 2. access_tokens 表
ALTER TABLE access_tokens RENAME COLUMN invalidated_ts TO revoked_ts;

-- 3. events 表
ALTER TABLE events RENAME COLUMN processed TO is_processed;
ALTER TABLE events RENAME COLUMN outlier TO is_outlier;

-- 4. voice_messages 表
ALTER TABLE voice_messages RENAME COLUMN processed TO is_processed;

-- 5. media_repository 表
ALTER TABLE media_repository RENAME COLUMN quarantine_media TO is_quarantined;
ALTER TABLE media_repository RENAME COLUMN safe_from_quarantine TO is_safe_from_quarantine;

-- 6. space_children 表
ALTER TABLE space_children RENAME COLUMN suggested TO is_suggested;

-- 7. ip_reputation 表
ALTER TABLE ip_reputation RENAME COLUMN blocked TO is_blocked;

-- 8. federation_blacklist 表
ALTER TABLE federation_blacklist RENAME COLUMN is_active TO is_enabled;

-- 9. federation_blacklist_rule 表
ALTER TABLE federation_blacklist_rule RENAME COLUMN enabled TO is_enabled;

-- 10. modules 表
ALTER TABLE modules RENAME COLUMN enabled TO is_enabled;

-- 11. saml_identity_providers 表
ALTER TABLE saml_identity_providers RENAME COLUMN enabled TO is_enabled;

-- 12. cas_services 表
ALTER TABLE cas_services RENAME COLUMN enabled TO is_enabled;

-- 13. captcha_template 表
ALTER TABLE captcha_template RENAME COLUMN enabled TO is_enabled;

-- 14. push_device 表
ALTER TABLE push_device RENAME COLUMN enabled TO is_enabled;

-- 15. push_rule 表
ALTER TABLE push_rule RENAME COLUMN enabled TO is_enabled;

-- 16. pushers 表
ALTER TABLE pushers RENAME COLUMN enabled TO is_enabled;

-- 17. push_rules 表
ALTER TABLE push_rules RENAME COLUMN enabled TO is_enabled;

-- 18. server_notifications 表
ALTER TABLE server_notifications RENAME COLUMN is_active TO is_enabled;
ALTER TABLE server_notifications RENAME COLUMN is_dismissible TO is_dismissable;

-- 19. application_services 表
ALTER TABLE application_services RENAME COLUMN is_active TO is_enabled;

-- 20. registration_tokens 表
ALTER TABLE registration_tokens RENAME COLUMN is_active TO is_enabled;

-- 21. registration_token_batches 表
ALTER TABLE registration_token_batches RENAME COLUMN is_active TO is_enabled;

-- 22. thread_roots 表
ALTER TABLE thread_roots RENAME COLUMN is_active TO is_enabled;

-- 23. ip_blocks 表
ALTER TABLE ip_blocks RENAME COLUMN is_active TO is_enabled;

-- =============================================================================
-- 第二部分: 时间字段规范化
-- =============================================================================

-- 1. devices 表 - 统一使用 _ts 后缀
ALTER TABLE devices RENAME COLUMN created_at TO created_ts;

-- 2. federation_signing_keys 表
ALTER TABLE federation_signing_keys RENAME COLUMN created_at TO created_ts;

-- 3. voice_messages 表 - 移除冗余字段
ALTER TABLE voice_messages DROP COLUMN IF EXISTS processed_at;

-- 4. ip_reputation 表 - 统一使用 _ts 后缀
ALTER TABLE ip_reputation RENAME COLUMN last_failed_at TO last_failed_ts;
ALTER TABLE ip_reputation RENAME COLUMN last_success_at TO last_success_ts;
ALTER TABLE ip_reputation RENAME COLUMN blocked_at TO blocked_ts;

-- =============================================================================
-- 第三部分: 移除冗余字段
-- =============================================================================

-- 1. access_tokens 表 - 移除重复的 ip 字段
ALTER TABLE access_tokens DROP COLUMN IF EXISTS ip;

-- 2. events 表 - 统一使用 event_type
ALTER TABLE events DROP COLUMN IF EXISTS type;

-- 3. voice_messages 表 - 移除冗余字段
ALTER TABLE voice_messages DROP COLUMN IF EXISTS waveform_data;
ALTER TABLE voice_messages DROP COLUMN IF EXISTS transcribe_text;

-- =============================================================================
-- 第四部分: 更新索引
-- =============================================================================

-- 重建受影响的索引
DROP INDEX IF EXISTS idx_users_deactivated;
CREATE INDEX IF NOT EXISTS idx_users_deactivated ON users(is_deactivated) WHERE is_deactivated = TRUE;

DROP INDEX IF EXISTS idx_voice_messages_processed;
CREATE INDEX IF NOT EXISTS idx_voice_messages_processed ON voice_messages(is_processed);

DROP INDEX IF EXISTS idx_federation_blacklist_active;
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_enabled ON federation_blacklist(is_enabled);

DROP INDEX IF EXISTS idx_federation_blacklist_rule_enabled;
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_rule_enabled ON federation_blacklist_rule(is_enabled);

DROP INDEX IF EXISTS idx_ip_blocks_active;
CREATE INDEX IF NOT EXISTS idx_ip_blocks_enabled ON ip_blocks(is_enabled);

-- =============================================================================
-- 第五部分: 验证迁移
-- =============================================================================

DO $$
BEGIN
    RAISE NOTICE '==========================================';
    RAISE NOTICE 'Field name normalization completed!';
    RAISE NOTICE 'Total tables modified: 23';
    RAISE NOTICE '==========================================';
END $$;

COMMIT;

-- =============================================================================
-- 迁移完成
-- =============================================================================
