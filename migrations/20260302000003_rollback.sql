-- Rollback: Remove media quota and server notification tables
-- Version: 20260302000003_rollback
-- Description: 回滚媒体配额和服务器通知相关的数据库表
-- Author: System
-- Date: 2026-03-02
-- WARNING: 此操作将删除所有相关数据，不可恢复！

-- ============================================================================
-- 1. 删除服务器通知相关表（按依赖顺序）
-- ============================================================================

-- 删除定时通知表
DROP TABLE IF EXISTS scheduled_notifications CASCADE;

-- 删除通知投递日志表
DROP TABLE IF EXISTS notification_delivery_log CASCADE;

-- 删除用户通知状态表
DROP TABLE IF EXISTS user_notification_status CASCADE;

-- 删除通知模板表
DROP TABLE IF EXISTS notification_templates CASCADE;

-- 删除服务器通知表
DROP TABLE IF EXISTS server_notifications CASCADE;

-- ============================================================================
-- 2. 删除媒体配额相关表（按依赖顺序）
-- ============================================================================

-- 删除配额告警表
DROP TABLE IF EXISTS media_quota_alerts CASCADE;

-- 删除媒体使用日志表
DROP TABLE IF EXISTS media_usage_log CASCADE;

-- 删除用户媒体配额表
DROP TABLE IF EXISTS user_media_quota CASCADE;

-- 删除服务器媒体配额表
DROP TABLE IF EXISTS server_media_quota CASCADE;

-- 删除配额配置表
DROP TABLE IF EXISTS media_quota_config CASCADE;

-- ============================================================================
-- 3. 删除迁移记录
-- ============================================================================

DELETE FROM schema_migrations WHERE version = '20260302000003';

-- ============================================================================
-- 4. 验证回滚结果
-- ============================================================================

-- 检查表是否已删除
DO $$
DECLARE
    table_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables
    WHERE table_schema = 'public'
    AND table_name IN (
        'media_quota_config', 'user_media_quota', 'media_usage_log',
        'media_quota_alerts', 'server_media_quota',
        'server_notifications', 'user_notification_status',
        'notification_templates', 'notification_delivery_log',
        'scheduled_notifications'
    );
    
    IF table_count > 0 THEN
        RAISE NOTICE '警告: 仍有 % 个表未删除', table_count;
    ELSE
        RAISE NOTICE '回滚成功: 所有表已删除';
    END IF;
END $$;
