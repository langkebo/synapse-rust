-- Migration: Optimize schema integrity after architecture review findings
-- Version: 20260304000003
-- Date: 2026-03-04

-- ============================================================================
-- 1. 清理重复索引（application_services.as_id）
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'application_services'
    ) THEN
        DROP INDEX IF EXISTS idx_application_services_as_id;
        DROP INDEX IF EXISTS idx_application_services_id;
    END IF;
END $$;

-- ============================================================================
-- 2. room_summary_* 补齐与 rooms 的关联约束并清理孤儿数据
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'rooms') THEN
        IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'room_summaries') THEN
            DELETE FROM room_summaries s
            WHERE NOT EXISTS (
                SELECT 1 FROM rooms r WHERE r.room_id = s.room_id
            );

            IF NOT EXISTS (
                SELECT 1 FROM pg_constraint
                WHERE conname = 'fk_room_summaries_room'
                  AND conrelid = 'room_summaries'::regclass
            ) THEN
                ALTER TABLE room_summaries
                ADD CONSTRAINT fk_room_summaries_room
                FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
            END IF;
        END IF;

        IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'room_summary_members') THEN
            DELETE FROM room_summary_members m
            WHERE NOT EXISTS (
                SELECT 1 FROM rooms r WHERE r.room_id = m.room_id
            );

            IF NOT EXISTS (
                SELECT 1 FROM pg_constraint
                WHERE conname = 'fk_room_summary_members_room'
                  AND conrelid = 'room_summary_members'::regclass
            ) THEN
                ALTER TABLE room_summary_members
                ADD CONSTRAINT fk_room_summary_members_room
                FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
            END IF;
        END IF;

        IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'room_summary_state') THEN
            DELETE FROM room_summary_state s
            WHERE NOT EXISTS (
                SELECT 1 FROM rooms r WHERE r.room_id = s.room_id
            );

            IF NOT EXISTS (
                SELECT 1 FROM pg_constraint
                WHERE conname = 'fk_room_summary_state_room'
                  AND conrelid = 'room_summary_state'::regclass
            ) THEN
                ALTER TABLE room_summary_state
                ADD CONSTRAINT fk_room_summary_state_room
                FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
            END IF;
        END IF;

        IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'room_summary_stats') THEN
            DELETE FROM room_summary_stats s
            WHERE NOT EXISTS (
                SELECT 1 FROM rooms r WHERE r.room_id = s.room_id
            );

            IF NOT EXISTS (
                SELECT 1 FROM pg_constraint
                WHERE conname = 'fk_room_summary_stats_room'
                  AND conrelid = 'room_summary_stats'::regclass
            ) THEN
                ALTER TABLE room_summary_stats
                ADD CONSTRAINT fk_room_summary_stats_room
                FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
            END IF;
        END IF;
    END IF;
END $$;

-- ============================================================================
-- 3. 关键业务表补齐 created_ts/updated_ts 审计字段对
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'room_summary_state') THEN
        ALTER TABLE room_summary_state
        ADD COLUMN IF NOT EXISTS created_ts BIGINT;

        UPDATE room_summary_state
        SET created_ts = COALESCE(created_ts, updated_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
        WHERE created_ts IS NULL;

        ALTER TABLE room_summary_state
        ALTER COLUMN created_ts SET DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000);

        ALTER TABLE room_summary_state
        ALTER COLUMN updated_ts SET DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000);
    END IF;

    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'room_summary_stats') THEN
        ALTER TABLE room_summary_stats
        ADD COLUMN IF NOT EXISTS created_ts BIGINT;

        ALTER TABLE room_summary_stats
        ADD COLUMN IF NOT EXISTS updated_ts BIGINT;

        UPDATE room_summary_stats
        SET created_ts = COALESCE(created_ts, last_updated_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
            updated_ts = COALESCE(updated_ts, last_updated_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
        WHERE created_ts IS NULL OR updated_ts IS NULL;

        ALTER TABLE room_summary_stats
        ALTER COLUMN created_ts SET DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000);

        ALTER TABLE room_summary_stats
        ALTER COLUMN updated_ts SET DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000);
    END IF;

    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'room_summary_update_queue') THEN
        ALTER TABLE room_summary_update_queue
        ADD COLUMN IF NOT EXISTS updated_ts BIGINT;

        UPDATE room_summary_update_queue
        SET updated_ts = COALESCE(updated_ts, created_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
        WHERE updated_ts IS NULL;

        ALTER TABLE room_summary_update_queue
        ALTER COLUMN updated_ts SET DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000);
    END IF;
END $$;
