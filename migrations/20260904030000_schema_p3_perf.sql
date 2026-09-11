-- Migration: 20260904030000_schema_p3_perf.sql
-- Author: DB Schema Audit 2026-09-04
-- Purpose: P3 performance optimizations
--
-- P3-1: push_notification_queue 热点查询索引
--   推送 worker 取出待发通知：按 user_id + priority + created_ts
--   调度重试：按 next_attempt_at
--
-- P3-2: federation_queue 发送顺序索引
--   发送队列按 destination 先进先出
--   重试策略：status + retry_count + created_ts
--
-- P3-3: backup_keys 约束
--   缺 (backup_id, room_id, session_id) UNIQUE
--   缺 room_id FK

-- ============================================================
-- P3-1: push_notification_queue 索引
-- ============================================================
-- 推送 worker 取待发通知
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_push_queue_user_pending
    ON push_notification_queue(user_id, priority DESC, created_ts)
    WHERE is_processed = FALSE AND status = 'pending';

-- 调度重试
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_push_queue_retry
    ON push_notification_queue(next_attempt_at)
    WHERE is_processed = FALSE AND status = 'retry';

-- ============================================================
-- P3-2: federation_queue 索引
-- ============================================================
-- 按 destination 先进先出
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_federation_queue_dest_created
    ON federation_queue(destination, created_ts)
    WHERE status = 'pending';

-- 重试策略
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_federation_queue_retry
    ON federation_queue(destination, status, retry_count, created_ts)
    WHERE status = 'pending';

-- ============================================================
-- P3-3: backup_keys 约束
-- ============================================================
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'uq_backup_keys_room_session'
    ) THEN
        ALTER TABLE backup_keys
            ADD CONSTRAINT uq_backup_keys_room_session
            UNIQUE (backup_id, room_id, session_id);
    END IF;
END $$;

-- 表名用 current_schema() 显式限定，不依赖 search_path：public 中若残留同名
-- 表（rooms 由 `CREATE TABLE IF NOT EXISTS` 创建，可能被静默跳过），未限定的
-- REFERENCES 会绑定到 public 副本。事故背景见
-- 20260831070000_room_summary_members_fk_not_deferred.sql 头部。
DO $$
DECLARE
    backup_keys_tbl text := format('%I.%I', current_schema(), 'backup_keys');
BEGIN
    IF to_regclass(backup_keys_tbl) IS NULL OR to_regclass(format('%I.%I', current_schema(), 'rooms')) IS NULL THEN
        RAISE NOTICE 'backup_keys/rooms not present in schema %, skipping P3 FK', current_schema();
        RETURN;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_backup_keys_room') THEN
        EXECUTE format(
            'ALTER TABLE %s ADD CONSTRAINT fk_backup_keys_room '
            'FOREIGN KEY (room_id) REFERENCES %I.rooms(room_id) ON DELETE CASCADE',
            backup_keys_tbl,
            current_schema()
        );
    END IF;
END $$;

-- ============================================================
-- P3-4: rooms.is_federated 索引
-- ============================================================
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_rooms_federated
    ON rooms(is_federated) WHERE is_federated = TRUE;
