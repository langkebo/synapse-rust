-- ============================================================================
-- Consolidated Migration: Schema Fixes & Contract Alignment
-- Created: 2026-04-22 (consolidated from 8 migrations dated 2026-04-05 ~ 2026-04-06)
--
-- Merged source files:
--   1. 20260405000001_fix_push_rules_unique_constraint.sql
--   2. 20260405000002_fix_push_rules_unique_constraint_v2.sql
--   3. 20260406000001_restore_verification_requests_pending_index.sql
--   4. 20260406000002_restore_schema_contract_foreign_keys.sql
--   5. 20260406000003_restore_public_schema_contract_foreign_keys.sql
--   6. 20260406000004_cleanup_schema_contract_room_orphans.sql
--   7. 20260406000005_align_presence_schema_contract.sql
--   8. 20260406000006_align_media_quota_schema_contract.sql
--
-- All statements use IF NOT EXISTS / IF EXISTS guards for idempotent execution.
-- ============================================================================


-- ===== Merged from: 20260405000001_fix_push_rules_unique_constraint.sql =====

ALTER TABLE push_rules
    DROP CONSTRAINT IF EXISTS uq_push_rules_user_scope_rule;

ALTER TABLE push_rules
    DROP CONSTRAINT IF EXISTS uq_push_rules_user_scope_kind_rule;

ALTER TABLE push_rules
    ADD CONSTRAINT uq_push_rules_user_scope_kind_rule UNIQUE (user_id, scope, kind, rule_id);

-- ===== Merged from: 20260405000002_fix_push_rules_unique_constraint_v2.sql =====

ALTER TABLE push_rules
    DROP CONSTRAINT IF EXISTS uq_push_rules_user_scope_rule;

ALTER TABLE push_rules
    DROP CONSTRAINT IF EXISTS uq_push_rules_user_scope_kind_rule;

ALTER TABLE push_rules
    ADD CONSTRAINT uq_push_rules_user_scope_kind_rule UNIQUE (user_id, scope, kind, rule_id);


-- ===== Merged from: 20260406000001_restore_verification_requests_pending_index.sql =====

-- ============================================================================
-- Restore verification_requests pending lookup index
-- Created: 2026-04-06
-- Description: Re-create a critical verification_requests index that was
-- accidentally dropped during schema alignment consolidation.
-- ============================================================================

SET TIME ZONE 'UTC';

CREATE INDEX IF NOT EXISTS idx_verification_requests_to_user_state
ON verification_requests(to_user, state, updated_ts DESC);

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260406000001',
    'restore_verification_requests_pending_index',
    TRUE,
    'Restore idx_verification_requests_to_user_state dropped by consolidated schema alignment',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
)
ON CONFLICT (version) DO NOTHING;

-- ===== Merged from: 20260406000002_restore_schema_contract_foreign_keys.sql =====

-- ============================================================================
-- Restore schema-contract foreign keys
-- Created: 2026-04-06
-- Description: Re-create foreign keys required by schema validator and
-- database integrity tests for room summary and retention tables.
-- ============================================================================

SET TIME ZONE 'UTC';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_summary_state_room'
    ) THEN
        ALTER TABLE room_summary_state
        ADD CONSTRAINT fk_room_summary_state_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_summary_stats_room'
    ) THEN
        ALTER TABLE room_summary_stats
        ADD CONSTRAINT fk_room_summary_stats_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_summary_update_queue_room'
    ) THEN
        ALTER TABLE room_summary_update_queue
        ADD CONSTRAINT fk_room_summary_update_queue_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_children_parent'
    ) THEN
        ALTER TABLE room_children
        ADD CONSTRAINT fk_room_children_parent
        FOREIGN KEY (parent_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_children_child'
    ) THEN
        ALTER TABLE room_children
        ADD CONSTRAINT fk_room_children_child
        FOREIGN KEY (child_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_retention_cleanup_queue_room'
    ) THEN
        ALTER TABLE retention_cleanup_queue
        ADD CONSTRAINT fk_retention_cleanup_queue_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_retention_cleanup_logs_room'
    ) THEN
        ALTER TABLE retention_cleanup_logs
        ADD CONSTRAINT fk_retention_cleanup_logs_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_retention_stats_room'
    ) THEN
        ALTER TABLE retention_stats
        ADD CONSTRAINT fk_retention_stats_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_deleted_events_index_room'
    ) THEN
        ALTER TABLE deleted_events_index
        ADD CONSTRAINT fk_deleted_events_index_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260406000002',
    'restore_schema_contract_foreign_keys',
    TRUE,
    'Restore room summary and retention foreign keys required by schema contract checks',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
)
ON CONFLICT (version) DO NOTHING;

-- ===== Merged from: 20260406000003_restore_public_schema_contract_foreign_keys.sql =====

-- ============================================================================
-- Restore public schema-contract foreign keys
-- Created: 2026-04-06
-- Description: Re-create room summary and retention foreign keys in the public
-- schema. Constraint existence checks are schema-scoped to avoid false
-- positives from isolated test schemas.
-- ============================================================================

SET TIME ZONE 'UTC';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'room_summary_state'
          AND constraint_name = 'fk_room_summary_state_room'
    ) THEN
        ALTER TABLE room_summary_state
        ADD CONSTRAINT fk_room_summary_state_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'room_summary_stats'
          AND constraint_name = 'fk_room_summary_stats_room'
    ) THEN
        ALTER TABLE room_summary_stats
        ADD CONSTRAINT fk_room_summary_stats_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'room_summary_update_queue'
          AND constraint_name = 'fk_room_summary_update_queue_room'
    ) THEN
        ALTER TABLE room_summary_update_queue
        ADD CONSTRAINT fk_room_summary_update_queue_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'room_children'
          AND constraint_name = 'fk_room_children_parent'
    ) THEN
        ALTER TABLE room_children
        ADD CONSTRAINT fk_room_children_parent
        FOREIGN KEY (parent_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'room_children'
          AND constraint_name = 'fk_room_children_child'
    ) THEN
        ALTER TABLE room_children
        ADD CONSTRAINT fk_room_children_child
        FOREIGN KEY (child_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'retention_cleanup_queue'
          AND constraint_name = 'fk_retention_cleanup_queue_room'
    ) THEN
        ALTER TABLE retention_cleanup_queue
        ADD CONSTRAINT fk_retention_cleanup_queue_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'retention_cleanup_logs'
          AND constraint_name = 'fk_retention_cleanup_logs_room'
    ) THEN
        ALTER TABLE retention_cleanup_logs
        ADD CONSTRAINT fk_retention_cleanup_logs_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'retention_stats'
          AND constraint_name = 'fk_retention_stats_room'
    ) THEN
        ALTER TABLE retention_stats
        ADD CONSTRAINT fk_retention_stats_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'deleted_events_index'
          AND constraint_name = 'fk_deleted_events_index_room'
    ) THEN
        ALTER TABLE deleted_events_index
        ADD CONSTRAINT fk_deleted_events_index_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260406000003',
    'restore_public_schema_contract_foreign_keys',
    TRUE,
    'Restore public schema room summary and retention foreign keys with schema-scoped existence checks',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
)
ON CONFLICT (version) DO NOTHING;

-- ===== Merged from: 20260406000004_cleanup_schema_contract_room_orphans.sql =====

-- ============================================================================
-- Cleanup schema-contract room orphans
-- Created: 2026-04-06
-- Description: Remove orphan rows from derived room summary and retention
-- tables so room foreign keys can be restored safely.
-- ============================================================================

SET TIME ZONE 'UTC';

DELETE FROM room_summary_state rss
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rss.room_id
);

DELETE FROM room_summary_stats rs
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rs.room_id
);

DELETE FROM room_summary_update_queue rsuq
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rsuq.room_id
);

DELETE FROM room_children rc
WHERE NOT EXISTS (
    SELECT 1 FROM rooms parent WHERE parent.room_id = rc.parent_room_id
)
   OR NOT EXISTS (
    SELECT 1 FROM rooms child WHERE child.room_id = rc.child_room_id
);

DELETE FROM retention_cleanup_queue rcq
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rcq.room_id
);

DELETE FROM retention_cleanup_logs rcl
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rcl.room_id
);

DELETE FROM retention_stats rs
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rs.room_id
);

DELETE FROM deleted_events_index dei
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = dei.room_id
);

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260406000004',
    'cleanup_schema_contract_room_orphans',
    TRUE,
    'Delete orphan rows from derived room summary and retention tables before restoring foreign keys',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
)
ON CONFLICT (version) DO NOTHING;

-- ===== Merged from: 20260406000005_align_presence_schema_contract.sql =====

-- ============================================================================
-- Align presence schema contract
-- Created: 2026-04-06
-- Description: Repair legacy presence nullability/default drift so presence
-- schema contract tests match the unified schema baseline.
-- ============================================================================

SET TIME ZONE 'UTC';

UPDATE presence
SET presence = 'offline'
WHERE presence IS NULL;

UPDATE presence
SET last_active_ts = 0
WHERE last_active_ts IS NULL;

ALTER TABLE IF EXISTS presence
    ALTER COLUMN presence SET DEFAULT 'offline';

ALTER TABLE IF EXISTS presence
    ALTER COLUMN last_active_ts SET DEFAULT 0;

ALTER TABLE IF EXISTS presence
    ALTER COLUMN presence SET NOT NULL;

ALTER TABLE IF EXISTS presence
    ALTER COLUMN last_active_ts SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_presence_user_status
ON presence(user_id, presence);

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260406000005',
    'align_presence_schema_contract',
    TRUE,
    'Repair legacy presence nullability/default drift and ensure presence schema contract index',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
)
ON CONFLICT (version) DO NOTHING;

-- ===== Merged from: 20260406000006_align_media_quota_schema_contract.sql =====

-- ============================================================================
-- Align media quota schema contract
-- Created: 2026-04-06
-- Description: Restore media quota tables/columns required by MediaQuotaStorage
-- and the schema contract migration gate.
-- ============================================================================

SET TIME ZONE 'UTC';

CREATE TABLE IF NOT EXISTS media_usage_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    media_id TEXT NOT NULL,
    file_size_bytes BIGINT NOT NULL,
    mime_type TEXT,
    operation TEXT NOT NULL,
    timestamp BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_media_usage_log_user
ON media_usage_log(user_id);

CREATE INDEX IF NOT EXISTS idx_media_usage_log_timestamp
ON media_usage_log(timestamp);

CREATE TABLE IF NOT EXISTS media_quota_alerts (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    alert_type TEXT NOT NULL,
    threshold_percent INTEGER NOT NULL,
    current_usage_bytes BIGINT NOT NULL,
    quota_limit_bytes BIGINT NOT NULL,
    message TEXT,
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
);

CREATE INDEX IF NOT EXISTS idx_media_quota_alerts_user
ON media_quota_alerts(user_id)
WHERE is_read = FALSE;

CREATE TABLE IF NOT EXISTS server_media_quota (
    id BIGSERIAL PRIMARY KEY,
    max_storage_bytes BIGINT,
    max_file_size_bytes BIGINT,
    max_files_count INTEGER,
    current_storage_bytes BIGINT NOT NULL DEFAULT 0,
    current_files_count INTEGER NOT NULL DEFAULT 0,
    alert_threshold_percent INTEGER NOT NULL DEFAULT 80,
    updated_ts BIGINT NOT NULL
);

ALTER TABLE media_quota_config
    ADD COLUMN IF NOT EXISTS name TEXT,
    ADD COLUMN IF NOT EXISTS description TEXT,
    ADD COLUMN IF NOT EXISTS max_storage_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS max_file_size_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS max_files_count INTEGER,
    ADD COLUMN IF NOT EXISTS allowed_mime_types JSONB,
    ADD COLUMN IF NOT EXISTS blocked_mime_types JSONB,
    ADD COLUMN IF NOT EXISTS is_default BOOLEAN;

UPDATE media_quota_config
SET name = COALESCE(name, NULLIF(config_name, ''), 'default')
WHERE name IS NULL;

UPDATE media_quota_config
SET max_storage_bytes = COALESCE(max_storage_bytes, 10737418240)
WHERE max_storage_bytes IS NULL;

UPDATE media_quota_config
SET max_file_size_bytes = COALESCE(max_file_size_bytes, max_file_size, 10485760)
WHERE max_file_size_bytes IS NULL;

UPDATE media_quota_config
SET max_files_count = COALESCE(max_files_count, 10000)
WHERE max_files_count IS NULL;

UPDATE media_quota_config
SET allowed_mime_types = COALESCE(allowed_mime_types, to_jsonb(allowed_content_types), '[]'::jsonb)
WHERE allowed_mime_types IS NULL;

UPDATE media_quota_config
SET blocked_mime_types = COALESCE(blocked_mime_types, '[]'::jsonb)
WHERE blocked_mime_types IS NULL;

UPDATE media_quota_config
SET is_default = COALESCE(is_default, FALSE)
WHERE is_default IS NULL;

ALTER TABLE media_quota_config
    ALTER COLUMN config_name SET DEFAULT '',
    ALTER COLUMN created_ts SET DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
    ALTER COLUMN name SET DEFAULT 'default',
    ALTER COLUMN max_storage_bytes SET DEFAULT 10737418240,
    ALTER COLUMN max_file_size_bytes SET DEFAULT 10485760,
    ALTER COLUMN max_files_count SET DEFAULT 10000,
    ALTER COLUMN allowed_mime_types SET DEFAULT '[]'::jsonb,
    ALTER COLUMN blocked_mime_types SET DEFAULT '[]'::jsonb,
    ALTER COLUMN is_default SET DEFAULT FALSE;

ALTER TABLE media_quota_config
    ALTER COLUMN name SET NOT NULL,
    ALTER COLUMN max_storage_bytes SET NOT NULL,
    ALTER COLUMN max_file_size_bytes SET NOT NULL,
    ALTER COLUMN max_files_count SET NOT NULL,
    ALTER COLUMN allowed_mime_types SET NOT NULL,
    ALTER COLUMN blocked_mime_types SET NOT NULL,
    ALTER COLUMN is_default SET NOT NULL;

ALTER TABLE user_media_quota
    ADD COLUMN IF NOT EXISTS quota_config_id BIGINT,
    ADD COLUMN IF NOT EXISTS custom_max_storage_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS custom_max_file_size_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS custom_max_files_count INTEGER,
    ADD COLUMN IF NOT EXISTS current_storage_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS current_files_count INTEGER;

UPDATE user_media_quota
SET current_storage_bytes = COALESCE(current_storage_bytes, used_bytes, 0)
WHERE current_storage_bytes IS NULL;

UPDATE user_media_quota
SET current_files_count = COALESCE(current_files_count, file_count, 0)
WHERE current_files_count IS NULL;

ALTER TABLE user_media_quota
    ALTER COLUMN current_storage_bytes SET DEFAULT 0,
    ALTER COLUMN current_files_count SET DEFAULT 0;

ALTER TABLE user_media_quota
    ALTER COLUMN current_storage_bytes SET NOT NULL,
    ALTER COLUMN current_files_count SET NOT NULL;

UPDATE media_quota_alerts
SET is_read = FALSE
WHERE is_read IS NULL;

ALTER TABLE media_quota_alerts
    ALTER COLUMN created_ts SET DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
    ALTER COLUMN is_read SET DEFAULT FALSE;

ALTER TABLE media_quota_alerts
    ALTER COLUMN is_read SET NOT NULL;

INSERT INTO server_media_quota (
    id,
    max_storage_bytes,
    max_file_size_bytes,
    max_files_count,
    current_storage_bytes,
    current_files_count,
    alert_threshold_percent,
    updated_ts
)
SELECT
    1,
    10995116277760,
    1073741824,
    1000000,
    0,
    0,
    80,
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
WHERE NOT EXISTS (
    SELECT 1 FROM server_media_quota WHERE id = 1
);

UPDATE server_media_quota
SET current_storage_bytes = COALESCE(current_storage_bytes, 0),
    current_files_count = COALESCE(current_files_count, 0),
    alert_threshold_percent = COALESCE(alert_threshold_percent, 80)
WHERE id = 1;

ALTER TABLE server_media_quota
    ALTER COLUMN current_storage_bytes SET DEFAULT 0,
    ALTER COLUMN current_files_count SET DEFAULT 0,
    ALTER COLUMN alert_threshold_percent SET DEFAULT 80;

ALTER TABLE server_media_quota
    ALTER COLUMN current_storage_bytes SET NOT NULL,
    ALTER COLUMN current_files_count SET NOT NULL,
    ALTER COLUMN alert_threshold_percent SET NOT NULL;

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260406000006',
    'align_media_quota_schema_contract',
    TRUE,
    'Restore media quota schema columns and tables required by MediaQuotaStorage and contract tests',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
)
ON CONFLICT (version) DO NOTHING;
