-- V260330_001__MIG-XXX__add_missing_schema_tables.undo.sql
--
-- 描述: 回滚 V260330_001__MIG-XXX__add_missing_schema_tables.sql
-- 删除所有新增的表
--
-- 注意: 此回滚会删除数据和表结构，不可逆

SET TIME ZONE 'UTC';

DO $$
BEGIN
    RAISE NOTICE '开始回滚缺失 schema 表...';
END $$;

-- ============================================================================
-- 回滚所有创建的表 (按依赖关系逆序)
-- ============================================================================

-- 删除 leak_alerts
DROP TABLE IF EXISTS leak_alerts CASCADE;

-- 删除 federation_blacklist_rule
DROP TABLE IF EXISTS federation_blacklist_rule CASCADE;

-- 删除 federation_blacklist_log
DROP TABLE IF EXISTS federation_blacklist_log CASCADE;

-- 删除 federation_blacklist_config
DROP TABLE IF EXISTS federation_blacklist_config CASCADE;

-- 删除 federation_access_stats
DROP TABLE IF EXISTS federation_access_stats CASCADE;

-- 删除 email_verification_tokens
DROP TABLE IF EXISTS email_verification_tokens CASCADE;

-- 删除 e2ee_stored_secrets
DROP TABLE IF EXISTS e2ee_stored_secrets CASCADE;

-- 删除 e2ee_secret_storage_keys
DROP TABLE IF EXISTS e2ee_secret_storage_keys CASCADE;

-- 删除 e2ee_audit_log
DROP TABLE IF EXISTS e2ee_audit_log CASCADE;

-- 删除 delayed_events
DROP TABLE IF EXISTS delayed_events CASCADE;

-- 删除 dehydrated_devices
DROP TABLE IF EXISTS dehydrated_devices CASCADE;

-- ============================================================================
-- 完成
-- ============================================================================

DO $$
BEGIN
    RAISE NOTICE '缺失 schema 表回滚完成';
END $$;
