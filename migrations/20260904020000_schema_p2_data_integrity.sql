-- Migration: 20260904020000_schema_p2_data_integrity.sql
-- Author: DB Schema Audit 2026-09-04
-- Purpose: P2 medium-priority data integrity fixes
--
-- P2-1: device_keys UQ 补 algorithm
--   Spec §10.2: key_id 格式 <algorithm>:<key_id>，UQ 漏 algorithm 会冲突
--
-- P2-2: e2ee_audit_log 补 device/room/event 索引
--   审计查询: 设备/房间/事件维度的 audit log 检索
--
-- P2-3: cross_signing_keys FK VALIDATE
--   增量迁移时用 NOT VALID 跳过历史数据校验，生产低峰期补做
--
-- P2-4: events.depth / state_key 缺约束
--   depth 应 >= 0；state_key is NULL 仅非 state event

-- ============================================================
-- P2-1: device_keys UQ 补 algorithm
-- ============================================================
-- 注意：此 ALTER 需要重写整表（添加新的 UQ 约束）
-- 在生产部署前需要先验证无重复数据
DO $$
DECLARE
    dup_count BIGINT;
BEGIN
    -- 检查是否有重复数据会被新 UQ 阻止
    SELECT COUNT(*) INTO dup_count
    FROM (
        SELECT user_id, device_id, algorithm, key_id, COUNT(*)
        FROM device_keys
        GROUP BY user_id, device_id, algorithm, key_id
        HAVING COUNT(*) > 1
    ) sub;
    IF dup_count = 0 THEN
        -- 旧 UQ 仍然保留（向后兼容），新 UQ 叠加
        IF NOT EXISTS (
            SELECT 1 FROM pg_constraint WHERE conname = 'uq_device_keys_user_device_algorithm_keyid'
        ) THEN
            ALTER TABLE device_keys
                ADD CONSTRAINT uq_device_keys_user_device_algorithm_keyid
                UNIQUE (user_id, device_id, algorithm, key_id);
        END IF;
    ELSE
        RAISE WARNING 'device_keys has % duplicate rows for new UQ constraint, skipping', dup_count;
    END IF;
END $$;

-- ============================================================
-- P2-2: e2ee_audit_log 索引
-- ============================================================
-- 设备维度查询: WHERE device_id = ?
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_e2ee_audit_log_device
    ON e2ee_audit_log(device_id) WHERE device_id IS NOT NULL;

-- 房间维度查询: WHERE room_id = ? AND event_id = ?
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_e2ee_audit_log_room_event
    ON e2ee_audit_log(room_id, event_id)
    WHERE room_id IS NOT NULL AND event_id IS NOT NULL;

-- ============================================================
-- P2-3: cross_signing_keys FK VALIDATE
-- ============================================================
-- 生产部署后独立运行（耗时但仅持有 ShareUpdateExclusiveLock）
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_cross_signing_keys_user'
        AND NOT convalidated
    ) THEN
        ALTER TABLE cross_signing_keys VALIDATE CONSTRAINT fk_cross_signing_keys_user;
    END IF;
END $$;

-- ============================================================
-- P2-4: events 表约束补强
-- ============================================================
-- depth >= 0 检查（事件深度为负数无意义）
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'ck_events_depth_nonneg'
    ) THEN
        ALTER TABLE events
            ADD CONSTRAINT ck_events_depth_nonneg
            CHECK (depth IS NULL OR depth >= 0);
    END IF;
END $$;

-- events.not_before 时间约束
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'ck_events_not_before_nonneg'
    ) THEN
        ALTER TABLE events
            ADD CONSTRAINT ck_events_not_before_nonneg
            CHECK (not_before IS NULL OR not_before >= 0);
    END IF;
END $$;
