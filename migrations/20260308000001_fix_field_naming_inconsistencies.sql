-- ============================================================================
-- Migration: Fix field naming inconsistencies
-- Version: 20260308000001
-- Description: 统一字段命名，符合 DATABASE_FIELD_STANDARDS.md 规范
-- ============================================================================

-- 1. notifications 表：删除冗余字段
ALTER TABLE notifications DROP COLUMN IF EXISTS read;

-- 2. notifications 表：重命名 ts
ALTER TABLE notifications RENAME COLUMN ts TO event_ts;

-- 3. voice_usage_stats 表：删除重复字段
ALTER TABLE voice_usage_stats DROP COLUMN IF EXISTS last_active_ts;

-- 4. voice_usage_stats 表：修改时间类型（如果存在 TIMESTAMP 类型）
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'voice_usage_stats' 
          AND column_name = 'period_start' 
          AND data_type = 'timestamp with time zone'
    ) THEN
        ALTER TABLE voice_usage_stats 
          ALTER COLUMN period_start TYPE BIGINT USING EXTRACT(EPOCH FROM period_start) * 1000;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'voice_usage_stats' 
          AND column_name = 'period_end' 
          AND data_type = 'timestamp with time zone'
    ) THEN
        ALTER TABLE voice_usage_stats 
          ALTER COLUMN period_end TYPE BIGINT USING EXTRACT(EPOCH FROM period_end) * 1000;
    END IF;
END $$;

-- 5. media_usage_log 表：重命名 timestamp
ALTER TABLE media_usage_log RENAME COLUMN timestamp TO event_ts;

-- 6. space_children 表：重命名 suggested
ALTER TABLE space_children RENAME COLUMN suggested TO is_suggested;

-- 7. scheduled_notifications 表：重命名 scheduled_for
ALTER TABLE scheduled_notifications RENAME COLUMN scheduled_for TO scheduled_ts;

-- 8. server_notifications 表：重命名 starts_at
ALTER TABLE server_notifications RENAME COLUMN starts_at TO starts_ts;

-- 9. rooms 表：确保 created_ts 字段名正确（而非 creation_ts）
-- 此字段已在 v5 schema 中正确定义，无需修改

-- 10. events 表：确保 is_redacted 字段名正确
-- 此字段已在 v5 schema 中正确定义，无需修改

-- 记录迁移完成
INSERT INTO schema_migrations (version, name, applied_ts, description)
VALUES ('20260308000001', 'fix_field_naming_inconsistencies', 
        EXTRACT(EPOCH FROM NOW()) * 1000, 
        '统一字段命名，符合 DATABASE_FIELD_STANDARDS.md 规范')
ON CONFLICT (version) DO NOTHING;
