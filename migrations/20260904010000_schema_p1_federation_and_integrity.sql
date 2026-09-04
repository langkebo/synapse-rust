-- Migration: 20260904010000_schema_p1_federation_and_integrity.sql
-- Author: DB Schema Audit 2026-09-04
-- Purpose: P1 blocking fixes
--
-- P1-1: device_signatures 0-index federation hotpath
--   /keys/query 热点查询 WHERE user_id = ? AND device_id = ?
--   当前全表扫，在大型 homeserver 上可累积数十万行
--
-- P1-2: room_memberships membership CHECK constraint
--   Matrix Spec §7.2 要求值 in ('invite','join','knock','leave','ban')
--   当前无 CHECK，可写入脏数据
--
-- P1-3: event_edges prev_event_id FK
--   不引用 events(event_id)，无法保证 DAG 完整性
--   孤儿 prev_event_id 可破坏事件图遍历

-- ============================================================
-- P1-1: device_signatures 索引（联邦热点）
-- ============================================================
-- 联邦查询: SELECT * FROM device_signatures WHERE user_id = $1 AND device_id = $2;
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_device_signatures_user_device
    ON device_signatures(user_id, device_id);

-- 签名验证: SELECT * FROM device_signatures WHERE target_user_id = $1 AND target_device_id = $2;
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_device_signatures_target
    ON device_signatures(target_user_id, target_device_id);

-- ============================================================
-- P1-2: room_memberships CHECK 约束
-- ============================================================
-- 注意：PostgreSQL ADD CONSTRAINT 不支持 IF NOT EXISTS
-- 需要先检查是否已存在同名约束
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'ck_room_memberships_valid'
    ) THEN
        ALTER TABLE room_memberships
            ADD CONSTRAINT ck_room_memberships_valid
            CHECK (membership IN ('invite', 'join', 'knock', 'leave', 'ban'));
    END IF;
END $$;

-- ============================================================
-- P1-3: event_edges prev_event_id FK
-- ============================================================
-- 使用 SET NULL 而非 CASCADE，因为已存在的孤儿 prev_event_id 需要被宽容处理
-- production 可选: ON DELETE SET NULL 改为 ON DELETE NO ACTION + Rust 层清理
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_event_edges_prev'
    ) THEN
        ALTER TABLE event_edges
            ADD CONSTRAINT fk_event_edges_prev
            FOREIGN KEY (prev_event_id) REFERENCES events(event_id)
            ON DELETE SET NULL;
    END IF;
END $$;

-- 补充索引（向后遍历事件图：给定 prev_event_id 找所有后续 event）
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_event_edges_prev_room
    ON event_edges(prev_event_id, event_id);

-- ============================================================
-- P1-4: events.redacted_by FK（bonus：同一表的 self-referential FK）
-- ============================================================
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_events_redacted_by'
    ) THEN
        ALTER TABLE events
            ADD CONSTRAINT fk_events_redacted_by
            FOREIGN KEY (redacted_by) REFERENCES events(event_id)
            ON DELETE SET NULL;
    END IF;
END $$;
