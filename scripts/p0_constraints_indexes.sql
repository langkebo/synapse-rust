-- ============================================================================
-- p0_constraints_indexes.sql — B3-2 折入内容（P0-5/P0-6 完整性约束 + 热点索引）
-- 以及随后追加的 P1/P3 增量迁移折入块（20260904010000_schema_p1_federation_and_integrity
-- 的 FK/CHECK/索引 + P3-4 rooms.is_federated）。
--
-- 该文件由 scripts/generate_v12_baseline.py 读取并 append 到 (v11 + extensions) 之后，
-- 生成 migrations/00000000_unified_schema_v12.sql。缺失时生成器会**报错退出**
-- （而不是偷偷写一句"暂空"占位符——那会让 baseline 与脚本产物悄悄漂移）。
-- ============================================================================

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
--
-- 表名一律用 current_schema() 显式限定，绝不依赖 search_path —— public 中若
-- 残留同名表，未限定的 REFERENCES 会静默绑定到 public 副本。
-- 事故背景见 20260831070000_room_summary_members_fk_not_deferred.sql 头部。
DO $$
DECLARE
    edges_tbl  text := format('%I.%I', current_schema(), 'event_edges');
    events_tbl text := format('%I.%I', current_schema(), 'events');
BEGIN
    IF to_regclass(edges_tbl) IS NULL OR to_regclass(events_tbl) IS NULL THEN
        RAISE NOTICE 'event_edges/events not present in schema %, skipping P1-3', current_schema();
        RETURN;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_event_edges_prev') THEN
        EXECUTE format(
            'ALTER TABLE %s ADD CONSTRAINT fk_event_edges_prev '
            'FOREIGN KEY (prev_event_id) REFERENCES %I.events(event_id) ON DELETE SET NULL',
            edges_tbl,
            current_schema()
        );
    END IF;
END $$;

-- 补充索引（向后遍历事件图：给定 prev_event_id 找所有后续 event）
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_event_edges_prev_room
    ON event_edges(prev_event_id, event_id);

-- ============================================================
-- P1-4: events.redacted_by FK（bonus：同一表的 self-referential FK）
-- ============================================================
DO $$
DECLARE
    events_tbl text := format('%I.%I', current_schema(), 'events');
BEGIN
    IF to_regclass(events_tbl) IS NULL THEN
        RAISE NOTICE 'events not present in schema %, skipping P1-4', current_schema();
        RETURN;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_events_redacted_by') THEN
        EXECUTE format(
            'ALTER TABLE %s ADD CONSTRAINT fk_events_redacted_by '
            'FOREIGN KEY (redacted_by) REFERENCES %I.events(event_id) ON DELETE SET NULL',
            events_tbl,
            current_schema()
        );
    END IF;
END $$;
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
